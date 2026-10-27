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

use super::{projects::list_invite_role, OkOut};
use crate::{
    error::{ApiError, ApiResult},
    security::auth::{hash_password, hash_token, random_token, AuthUser},
    security::authz::{require_role, Role},
    security::policy::check_password,
    services::mailer,
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(system_info))
        .routes(routes!(request_password_reset))
        .routes(routes!(confirm_password_reset))
        .routes(routes!(invite_by_email))
        .routes(routes!(invite_to_list_by_email))
}

#[derive(Serialize, ToSchema)]
pub struct SystemInfo {
    pub registration_enabled: bool,
    /// Outgoing e-mail is configured (password reset, invitations, notifications by e-mail).
    pub mail_enabled: bool,
    pub slack_enabled: bool,
    /// Two-factor authentication can be enabled (needs `WORK_ENCRYPTION_KEY`).
    pub mfa_available: bool,
    /// Sign-up needs a confirmed e-mail address.
    pub email_verification_required: bool,
    /// Single sign-on providers configured on this instance (`google`, `github`, `microsoft`,
    /// `slack`, `saml`).
    pub sso_providers: Vec<String>,
    /// Label for the `oidc` provider, e.g. the name of the company's identity provider.
    pub sso_oidc_name: Option<String>,
    /// Passkeys can be used (they need the page to be served over https, or localhost).
    pub passkeys_available: bool,
}

/// Public, non-sensitive facts the sign-in pages need.
#[utoipa::path(get, operation_id = "getSystemInfo", path = "/system/info", tag = "system", responses((status = 200, body = SystemInfo)))]
async fn system_info(State(s): State<AppState>) -> Json<SystemInfo> {
    Json(SystemInfo {
        registration_enabled: s.config.registration_enabled,
        mail_enabled: mailer::enabled(&s),
        slack_enabled: s.config.slack.is_some(),
        mfa_available: s.config.encryption_key.is_some(),
        email_verification_required: s.config.require_email_verification,
        sso_providers: s.config.sso.names(),
        sso_oidc_name: s.config.sso.oidc_name(),
        passkeys_available: s.config.frontend_url.starts_with("https://")
            || s.config.frontend_url.starts_with("http://localhost"),
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
        .await
    {
        return Err(ApiError::TooManyRequests);
    }
    let email = b.email.trim().to_lowercase();
    if email.len() > 254 || !email.contains('@') {
        return Ok(Json(OkOut { ok: true })); // same answer for junk input
    }
    if !s.reset_limiter.check(&format!("reset:{email}")).await {
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
        .await
    {
        return Err(ApiError::TooManyRequests);
    }
    // Look the account up without consuming the link, so a rejected password does not burn it.
    let account: Option<(String, String, String)> = sqlx::query_as(
        "SELECT u.email::text, u.first_name, u.last_name FROM password_resets r JOIN users u ON u.id = r.user_id
         WHERE r.token_hash = $1 AND r.used_at IS NULL AND r.expires_at > now() AND u.deleted_at IS NULL",
    )
    .bind(hash_token(&b.token))
    .fetch_optional(&s.db)
    .await?;
    let (email, first, last) = account
        .ok_or_else(|| ApiError::BadRequest("this reset link is invalid or has expired".into()))?;
    check_password(&b.new_password, &[&email, &first, &last])?;
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
    // Proof of the mailbox also ends a lock-out; MFA stays required.
    sqlx::query("UPDATE users SET password_hash = $2, failed_logins = 0, locked_until = NULL WHERE id = $1 AND deleted_at IS NULL")
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
    send_invite(&s, user.id, project_id, None, role, &b.email).await
}

/// Like the project invitation, but for one list: the invited address becomes a limited-access
/// member that only sees this list (existing members just get access to it).
#[utoipa::path(post, operation_id = "inviteToListByEmail", path = "/lists/{id}/invites/email", tag = "lists",
    params(("id" = Uuid, Path)), request_body = EmailInvite,
    responses((status = 200, body = EmailInviteOut), (status = 400), (status = 403), (status = 404), (status = 409), (status = 429)), security(("bearer" = [])))]
async fn invite_to_list_by_email(
    State(s): State<AppState>,
    user: AuthUser,
    Path(list_id): Path<Uuid>,
    Json(b): Json<EmailInvite>,
) -> ApiResult<Json<EmailInviteOut>> {
    let (scope, role) = list_invite_role(&s, user.id, list_id, &b.role).await?;
    send_invite(&s, user.id, scope.project_id, Some(list_id), role, &b.email).await
}

async fn send_invite(
    s: &AppState,
    user_id: Uuid,
    project_id: Uuid,
    list_id: Option<Uuid>,
    role: Role,
    email: &str,
) -> ApiResult<Json<EmailInviteOut>> {
    let email = email.trim().to_lowercase();
    if email.len() > 254 || !email.contains('@') || email.contains(char::is_whitespace) {
        return Err(ApiError::BadRequest("invalid e-mail address".into()));
    }
    if !mailer::enabled(s) {
        return Err(ApiError::Conflict); // nothing to send with: use an invite link instead
    }
    if !s.email_limiter.check(&format!("invite:{user_id}")).await {
        return Err(ApiError::TooManyRequests);
    }
    let (project_name,): (String,) = sqlx::query_as("SELECT name FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&s.db)
        .await?;
    // What the invitation is for: the project, or one list of it.
    let target = match list_id {
        Some(l) => {
            let (list_name,): (String,) = sqlx::query_as("SELECT name FROM lists WHERE id = $1")
                .bind(l)
                .fetch_one(&s.db)
                .await?;
            list_name
        }
        None => project_name,
    };
    let (inviter,): (String,) =
        sqlx::query_as("SELECT first_name || ' ' || last_name FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&s.db)
            .await?;
    let token = random_token();
    let expires_at = Utc::now() + Duration::days(7);
    sqlx::query(
        "INSERT INTO invites (id, project_id, list_id, token_hash, role, expires_at, max_uses, created_by, email) VALUES ($1,$2,$3,$4,$5,$6,1,$7,$8)",
    )
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(list_id)
    .bind(hash_token(&token))
    .bind(role.as_str())
    .bind(expires_at)
    .bind(user_id)
    .bind(&email)
    .execute(&s.db)
    .await?;
    let link = format!("{}/invite?token={}", s.config.frontend_url, token);
    let html = format!(
        "<p>{} invited you to join <b>{}</b> on work as {}.</p><p><a href=\"{}\">Accept the invitation</a></p><p>The link works once, for this e-mail address, and expires on {}.</p>",
        mailer::escape_html(&inviter),
        mailer::escape_html(&target),
        role.as_str(),
        mailer::escape_html(&link),
        expires_at.format("%Y-%m-%d")
    );
    mailer::send(s, email, format!("{inviter} invited you to {target}"), html);
    Ok(Json(EmailInviteOut { expires_at }))
}
