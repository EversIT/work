//! Flows that send e-mail: password reset and invitations by address, plus public instance info
//! so the UI knows which of them to offer.

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Path, State},
    http::HeaderMap,
    Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::OkOut;
use crate::{
    auth::{hash_password, hash_token, random_token, AuthUser, MAX_PASSWORD_LEN, MIN_PASSWORD_LEN},
    authz::{require_role, Role},
    error::{ApiError, ApiResult},
    mailer,
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(system_info))
        .routes(routes!(request_password_reset))
        .routes(routes!(confirm_password_reset))
        .routes(routes!(invite_by_email))
}

#[derive(Serialize, ToSchema)]
pub struct SystemInfo {
    pub registration_enabled: bool,
    /// Outgoing e-mail is configured (password reset, invitations, notifications by e-mail).
    pub mail_enabled: bool,
    pub slack_enabled: bool,
}

/// Public, non-sensitive facts the sign-in pages need.
#[utoipa::path(get, operation_id = "getSystemInfo", path = "/system/info", tag = "system", responses((status = 200, body = SystemInfo)))]
async fn system_info(State(s): State<AppState>) -> Json<SystemInfo> {
    Json(SystemInfo {
        registration_enabled: s.config.registration_enabled,
        mail_enabled: mailer::enabled(&s),
        slack_enabled: s.config.slack.is_some(),
    })
}

fn client_ip(s: &AppState, headers: &HeaderMap, peer: SocketAddr) -> String {
    if s.config.trust_forwarded_for {
        if let Some(last) = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit(',').next())
        {
            return last.trim().to_owned();
        }
    }
    peer.ip().to_string()
}

#[derive(Deserialize, ToSchema)]
pub struct ResetRequest {
    pub email: String,
}

const RESET_TTL_MINUTES: i64 = 60;

/// Always answers `200`, whether or not the address belongs to an account, so it cannot be used to
/// find out who is registered. At most 3 mails per address and hour.
#[utoipa::path(post, operation_id = "requestPasswordReset", path = "/auth/password-reset/request", tag = "auth",
    request_body = ResetRequest, responses((status = 200, body = OkOut), (status = 429)))]
async fn request_password_reset(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(b): Json<ResetRequest>,
) -> ApiResult<Json<OkOut>> {
    if !s
        .auth_limiter
        .check(&format!("reset-ip:{}", client_ip(&s, &headers, peer)))
    {
        return Err(ApiError::TooManyRequests);
    }
    let email = b.email.trim().to_lowercase();
    if email.len() > 254 || !email.contains('@') {
        return Ok(Json(OkOut { ok: true })); // same answer for junk input
    }
    if !s.reset_limiter.check(&format!("reset:{email}")) {
        return Ok(Json(OkOut { ok: true })); // silently capped; no signal to an attacker
    }
    let user: Option<(Uuid, String)> =
        sqlx::query_as("SELECT id, first_name FROM users WHERE email = $1 AND deleted_at IS NULL")
            .bind(&email)
            .fetch_optional(&s.db)
            .await?;
    if let (Some((user_id, first)), true) = (user, mailer::enabled(&s)) {
        let token = random_token();
        let mut tx = s.db.begin().await?;
        sqlx::query("DELETE FROM password_resets WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?; // one live link at a time
        sqlx::query(
            "INSERT INTO password_resets (token_hash, user_id, expires_at) VALUES ($1,$2,$3)",
        )
        .bind(hash_token(&token))
        .bind(user_id)
        .bind(Utc::now() + Duration::minutes(RESET_TTL_MINUTES))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        let link = format!("{}/reset-password?token={}", s.config.frontend_url, token);
        let html = format!(
            "<p>Hello {},</p><p>Someone asked to reset the password of your work account. If that was you, choose a new password within {} minutes:</p><p><a href=\"{}\">Reset password</a></p><p>If it was not you, you can ignore this e-mail; your password stays unchanged.</p>",
            mailer::escape_html(&first),
            RESET_TTL_MINUTES,
            mailer::escape_html(&link)
        );
        mailer::send(&s, email, "Reset your work password".into(), html);
    }
    Ok(Json(OkOut { ok: true }))
}

#[derive(Deserialize, ToSchema)]
pub struct ResetConfirm {
    pub token: String,
    pub new_password: String,
}

/// Single-use, expiring token. Every session of the account is revoked afterwards.
#[utoipa::path(post, operation_id = "confirmPasswordReset", path = "/auth/password-reset/confirm", tag = "auth",
    request_body = ResetConfirm, responses((status = 200, body = OkOut), (status = 400), (status = 429)))]
async fn confirm_password_reset(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(b): Json<ResetConfirm>,
) -> ApiResult<Json<OkOut>> {
    if !s
        .auth_limiter
        .check(&format!("reset-confirm:{}", client_ip(&s, &headers, peer)))
    {
        return Err(ApiError::TooManyRequests);
    }
    let n = b.new_password.chars().count();
    if !(MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&n) {
        return Err(ApiError::BadRequest(format!(
            "password must be {MIN_PASSWORD_LEN}-{MAX_PASSWORD_LEN} characters"
        )));
    }
    let new = b.new_password.clone();
    let hash = tokio::task::spawn_blocking(move || hash_password(&new))
        .await
        .map_err(|e| ApiError::Internal(Box::new(e)))??;
    let mut tx = s.db.begin().await?;
    // Redeem atomically: only one request can ever consume a token.
    let redeemed: Option<(Uuid,)> = sqlx::query_as(
        "UPDATE password_resets SET used_at = now() WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now() RETURNING user_id",
    )
    .bind(hash_token(&b.token))
    .fetch_optional(&mut *tx)
    .await?;
    let (user_id,) = redeemed
        .ok_or_else(|| ApiError::BadRequest("this reset link is invalid or has expired".into()))?;
    sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1 AND deleted_at IS NULL")
        .bind(user_id)
        .bind(hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(OkOut { ok: true }))
}

#[derive(Deserialize, ToSchema)]
pub struct EmailInvite {
    pub email: String,
    /// `viewer`, `member` or `admin` (below your own role)
    pub role: String,
}

#[derive(Serialize, ToSchema)]
pub struct EmailInviteOut {
    pub expires_at: chrono::DateTime<Utc>,
}

/// Mails an invitation that only the invited address can redeem (the account's e-mail must match).
#[utoipa::path(post, operation_id = "inviteByEmail", path = "/projects/{id}/invites/email", tag = "projects",
    params(("id" = Uuid, Path)), request_body = EmailInvite,
    responses((status = 200, body = EmailInviteOut), (status = 400), (status = 403), (status = 404), (status = 409), (status = 429)), security(("bearer" = [])))]
async fn invite_by_email(
    State(s): State<AppState>,
    user: AuthUser,
    Path(project_id): Path<Uuid>,
    Json(b): Json<EmailInvite>,
) -> ApiResult<Json<EmailInviteOut>> {
    let actor = require_role(&s.db, user.id, project_id, Role::Admin).await?;
    let role = Role::parse(&b.role).ok_or_else(|| ApiError::BadRequest("unknown role".into()))?;
    if role >= actor {
        return Err(ApiError::Forbidden);
    }
    let email = b.email.trim().to_lowercase();
    if email.len() > 254 || !email.contains('@') || email.contains(char::is_whitespace) {
        return Err(ApiError::BadRequest("invalid e-mail address".into()));
    }
    if !mailer::enabled(&s) {
        return Err(ApiError::Conflict); // nothing to send with: use an invite link instead
    }
    if !s.email_limiter.check(&format!("invite:{}", user.id)) {
        return Err(ApiError::TooManyRequests);
    }
    let (project_name,): (String,) = sqlx::query_as("SELECT name FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&s.db)
        .await?;
    let (inviter,): (String,) =
        sqlx::query_as("SELECT first_name || ' ' || last_name FROM users WHERE id = $1")
            .bind(user.id)
            .fetch_one(&s.db)
            .await?;
    let token = random_token();
    let expires_at = Utc::now() + Duration::days(7);
    sqlx::query(
        "INSERT INTO invites (id, project_id, token_hash, role, expires_at, max_uses, created_by, email) VALUES ($1,$2,$3,$4,$5,1,$6,$7)",
    )
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(hash_token(&token))
    .bind(role.as_str())
    .bind(expires_at)
    .bind(user.id)
    .bind(&email)
    .execute(&s.db)
    .await?;
    let link = format!("{}/invite?token={}", s.config.frontend_url, token);
    let html = format!(
        "<p>{} invited you to join <b>{}</b> on work as {}.</p><p><a href=\"{}\">Accept the invitation</a></p><p>The link works once, for this e-mail address, and expires on {}.</p>",
        mailer::escape_html(&inviter),
        mailer::escape_html(&project_name),
        role.as_str(),
        mailer::escape_html(&link),
        expires_at.format("%Y-%m-%d")
    );
    mailer::send(
        &s,
        email,
        format!("{inviter} invited you to {project_name}"),
        html,
    );
    Ok(Json(EmailInviteOut { expires_at }))
}
