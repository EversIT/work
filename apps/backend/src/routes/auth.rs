use std::net::SocketAddr;

use super::OkOut;
use axum::{
    extract::{ConnectInfo, State},
    http::{
        header::{COOKIE, SET_COOKIE},
        HeaderMap, HeaderValue,
    },
    Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;
use validator::Validate;

use crate::{
    error::{ApiError, ApiResult},
    security::auth::*,
    security::authz::Role,
    security::policy::check_password,
    services::mailer,
    state::AppState,
};

use super::mfa::second_factor_ok;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(register))
        .routes(routes!(login))
        .routes(routes!(verify_email))
        .routes(routes!(mfa_verify))
        .routes(routes!(refresh))
        .routes(routes!(logout))
        .routes(routes!(me))
}

pub(crate) fn client_ip(state: &AppState, headers: &HeaderMap, peer: SocketAddr) -> String {
    if state.config.trust_forwarded_for {
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

pub(crate) async fn throttle(state: &AppState, key: &str) -> ApiResult<()> {
    if state.auth_limiter.check(key).await {
        Ok(())
    } else {
        Err(ApiError::TooManyRequests)
    }
}

#[derive(Serialize, ToSchema)]
pub struct TokenPair {
    access_token: String,
    /// Seconds until the access token expires. The refresh token is never in the body:
    /// it is set as an HttpOnly cookie so scripts (and XSS) cannot read it.
    expires_in: i64,
}

const REFRESH_COOKIE: &str = "work_refresh";

fn refresh_cookie(state: &AppState, value: &str, max_age: i64) -> HeaderValue {
    let secure = if state.config.frontend_url.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    // Path-scoped so the cookie is only ever sent to the auth endpoints.
    HeaderValue::from_str(&format!(
        "{REFRESH_COOKIE}={value}; Path=/api/v1/auth; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure}"
    ))
    .expect("cookie value is ascii")
}

pub(crate) fn cookie_headers(state: &AppState, refresh: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        SET_COOKIE,
        refresh_cookie(state, refresh, REFRESH_TTL_DAYS * 86400),
    );
    h
}

fn read_refresh_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| {
            kv.trim()
                .strip_prefix(&format!("{REFRESH_COOKIE}="))
                .map(str::to_owned)
        })
        .next()
}

/// Result of a sign-in or sign-up step. Exactly one of three shapes: a session
/// (`access_token`), a second step (`mfa_required` + `mfa_token`) or a pending confirmation
/// (`verification_required`).
#[derive(Serialize, ToSchema, Default)]
pub struct AuthOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    access_token: Option<String>,
    /// Seconds until the access token expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_in: Option<i64>,
    /// The password was right, but a code from the authenticator (or a recovery code) is needed.
    mfa_required: bool,
    /// Present with `mfa_required`: send it with the code to `/auth/mfa/verify`. Valid for 5 minutes.
    #[serde(skip_serializing_if = "Option::is_none")]
    mfa_token: Option<String>,
    /// A confirmation e-mail was sent. The answer is the same whether or not the address is
    /// already registered.
    verification_required: bool,
}

type Authed = (HeaderMap, Json<AuthOut>);

impl From<TokenPair> for AuthOut {
    fn from(p: TokenPair) -> Self {
        AuthOut {
            access_token: Some(p.access_token),
            expires_in: Some(p.expires_in),
            ..Default::default()
        }
    }
}

/// What the session list shows about the device a session is used from.
#[derive(Default)]
pub(crate) struct SessionMeta {
    user_agent: Option<String>,
    ip: Option<String>,
}

impl SessionMeta {
    pub(crate) fn from_request(state: &AppState, headers: &HeaderMap, peer: SocketAddr) -> Self {
        let user_agent = headers
            .get(axum::http::header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.chars().take(300).collect::<String>())
            .filter(|v| !v.is_empty());
        Self {
            user_agent,
            ip: Some(client_ip(state, headers, peer).chars().take(64).collect()),
        }
    }
}

pub(crate) async fn start_session(
    state: &AppState,
    user_id: Uuid,
    family_id: Uuid,
    meta: &SessionMeta,
    exec: impl sqlx::PgExecutor<'_>,
) -> ApiResult<(TokenPair, String)> {
    let session_id = Uuid::new_v4();
    let refresh = random_token();
    sqlx::query(
        "INSERT INTO sessions (id, family_id, user_id, token_hash, expires_at, user_agent, ip) VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(session_id)
    .bind(family_id)
    .bind(user_id)
    .bind(hash_token(&refresh))
    .bind(Utc::now() + Duration::days(REFRESH_TTL_DAYS))
    .bind(&meta.user_agent)
    .bind(&meta.ip)
    .execute(exec)
    .await?;
    let pair = TokenPair {
        access_token: issue_access_token(&state.config.jwt_secret, user_id, session_id)?,
        expires_in: ACCESS_TTL_MINUTES * 60,
    };
    Ok((pair, refresh))
}

#[derive(Deserialize, Validate, ToSchema)]
struct RegisterBody {
    #[validate(email, length(max = 255))]
    email: String,
    password: String,
    #[validate(length(min = 1, max = 100))]
    first_name: String,
    #[validate(length(min = 1, max = 100))]
    last_name: String,
}

const VERIFY_TTL_HOURS: i64 = 24;

/// Account, personal project and owner membership, in the caller's transaction.
pub(crate) async fn create_account(
    tx: &mut sqlx::PgConnection,
    email: &str,
    password_hash: &str,
    first_name: &str,
    last_name: &str,
) -> ApiResult<Uuid> {
    let user_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    // The first account of an instance administers it; `work-backend make-admin <e-mail>` adds more.
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, first_name, last_name, is_instance_admin)
                 VALUES ($1,$2,$3,$4,$5, NOT EXISTS (SELECT 1 FROM users WHERE is_instance_admin))",
    )
    .bind(user_id)
    .bind(email)
    .bind(password_hash)
    .bind(first_name)
    .bind(last_name)
    .execute(&mut *tx)
    .await?; // unique violation -> 409
    sqlx::query("INSERT INTO projects (id, name, created_by) VALUES ($1,$2,$3)")
        .bind(project_id)
        .bind(format!("{first_name}'s Project"))
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES ($1,$2,$3)")
        .bind(project_id)
        .bind(user_id)
        .bind(Role::Owner.as_str())
        .execute(&mut *tx)
        .await?;
    Ok(user_id)
}

/// With e-mail verification on (the default whenever SMTP is configured), this never creates an
/// account and never says whether the address is taken: it stores a pending registration and mails
/// a link, or, for a known address, mails a notice. Without SMTP the account is created at once
/// and a taken address answers `409`.
#[utoipa::path(post, path="/auth/register", tag="auth", request_body = RegisterBody, responses((status=200, body=AuthOut), (status=400), (status=409), (status=429)))]
async fn register(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<RegisterBody>,
) -> ApiResult<Authed> {
    if !state.config.registration_enabled {
        return Err(ApiError::Forbidden);
    }
    throttle(
        &state,
        &format!("register:{}", client_ip(&state, &headers, peer)),
    )
    .await?;
    body.validate()?;
    let email = body.email.trim().to_lowercase();
    let (first, last) = (
        body.first_name.trim().to_owned(),
        body.last_name.trim().to_owned(),
    );
    check_password(&body.password, &[&email, &first, &last])?;

    // Hashing is CPU-heavy; keep it off the async workers.
    let password = body.password.clone();
    let hash = tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|e| ApiError::Internal(Box::new(e)))??;

    if state.config.require_email_verification {
        register_pending(&state, &email, &hash, &first, &last).await?;
        return Ok((
            HeaderMap::new(),
            Json(AuthOut {
                verification_required: true,
                ..Default::default()
            }),
        ));
    }

    let mut tx = state.db.begin().await?;
    let user_id = create_account(&mut tx, &email, &hash, &first, &last).await?;
    let (pair, refresh) = start_session(
        &state,
        user_id,
        Uuid::new_v4(),
        &SessionMeta::from_request(&state, &headers, peer),
        &mut *tx,
    )
    .await?;
    tx.commit().await?;
    Ok((cookie_headers(&state, &refresh), Json(pair.into())))
}

async fn register_pending(
    state: &AppState,
    email: &str,
    hash: &str,
    first: &str,
    last: &str,
) -> ApiResult<()> {
    // Same cap as password reset: nobody can use sign-up to flood an inbox.
    if !state
        .reset_limiter
        .check(&format!("register:{email}"))
        .await
    {
        return Ok(());
    }
    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE email = $1 AND deleted_at IS NULL")
            .bind(email)
            .fetch_optional(&state.db)
            .await?;
    if exists.is_some() {
        let html = format!(
            "<p>Hello {},</p><p>Someone tried to sign up for work with this e-mail address, but an account with it already exists. If it was you, sign in or <a href=\"{}\">reset your password</a>.</p><p>If it was not you, you can ignore this e-mail.</p>",
            mailer::escape_html(first),
            mailer::escape_html(&format!("{}/forgot-password", state.config.frontend_url)),
        );
        mailer::send(state, email.to_owned(), "Your work account".into(), html);
        return Ok(());
    }
    let token = random_token();
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM pending_registrations WHERE expires_at < now()")
        .execute(&mut *tx)
        .await?;
    // The latest registration for an address wins; its predecessor's link stops working.
    sqlx::query(
        "INSERT INTO pending_registrations (email, password_hash, first_name, last_name, token_hash, expires_at)
         VALUES ($1,$2,$3,$4,$5,$6)
         ON CONFLICT (email) DO UPDATE SET password_hash = EXCLUDED.password_hash, first_name = EXCLUDED.first_name,
           last_name = EXCLUDED.last_name, token_hash = EXCLUDED.token_hash, expires_at = EXCLUDED.expires_at,
           created_at = now()",
    )
    .bind(email)
    .bind(hash)
    .bind(first)
    .bind(last)
    .bind(hash_token(&token))
    .bind(Utc::now() + Duration::hours(VERIFY_TTL_HOURS))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let link = format!("{}/verify-email?token={}", state.config.frontend_url, token);
    let html = format!(
        "<p>Hello {},</p><p>Confirm your e-mail address to finish creating your work account (the link works for {} hours):</p><p><a href=\"{}\">Confirm e-mail address</a></p><p>If you did not sign up, you can ignore this e-mail; no account is created.</p>",
        mailer::escape_html(first),
        VERIFY_TTL_HOURS,
        mailer::escape_html(&link)
    );
    mailer::send(
        state,
        email.to_owned(),
        "Confirm your work account".into(),
        html,
    );
    Ok(())
}

#[derive(Deserialize, ToSchema)]
struct VerifyEmailBody {
    token: String,
}

/// Turns a pending registration into an account and signs the new user in. The link works once.
#[utoipa::path(post, path="/auth/verify-email", tag="auth", request_body = VerifyEmailBody, responses((status=200, body=AuthOut), (status=400), (status=429)))]
async fn verify_email(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<VerifyEmailBody>,
) -> ApiResult<Authed> {
    throttle(
        &state,
        &format!("verify:{}", client_ip(&state, &headers, peer)),
    )
    .await?;
    let invalid =
        || ApiError::BadRequest("this confirmation link is invalid or has expired".into());
    if !state.config.registration_enabled {
        return Err(ApiError::Forbidden);
    }
    let mut tx = state.db.begin().await?;
    let pending: Option<(String, String, String, String)> = sqlx::query_as(
        "DELETE FROM pending_registrations WHERE token_hash = $1 AND expires_at > now()
         RETURNING email::text, password_hash, first_name, last_name",
    )
    .bind(hash_token(body.token.trim()))
    .fetch_optional(&mut *tx)
    .await?;
    let (email, hash, first, last) = pending.ok_or_else(invalid)?;
    let user_id = match create_account(&mut tx, &email, &hash, &first, &last).await {
        Ok(id) => id,
        Err(ApiError::Conflict) => return Err(invalid()), // the address got an account in the meantime
        Err(e) => return Err(e),
    };
    let (pair, refresh) = start_session(
        &state,
        user_id,
        Uuid::new_v4(),
        &SessionMeta::from_request(&state, &headers, peer),
        &mut *tx,
    )
    .await?;
    tx.commit().await?;
    Ok((cookie_headers(&state, &refresh), Json(pair.into())))
}

#[derive(Deserialize, ToSchema)]
struct LoginBody {
    email: String,
    password: String,
}

const MAX_FAILURES: i32 = 5;
const MFA_CHALLENGE_MINUTES: i64 = 5;
const MFA_CHALLENGE_ATTEMPTS: i32 = 5;

/// Counts a failed password or second-factor attempt. After 5 failures in a row the account is
/// locked for 15 minutes, doubling with every further failure up to a day. The counter forgets
/// failures older than an hour once the lock has run out, so occasional typos never add up.
/// Returns true when this failure locked the account.
async fn record_failure(state: &AppState, user_id: Uuid) -> ApiResult<bool> {
    let (failed,): (i32,) = sqlx::query_as(
        "UPDATE users SET
           failed_logins = CASE WHEN last_failed_at < now() - interval '1 hour'
                                 AND (locked_until IS NULL OR locked_until < now())
                                THEN 1 ELSE failed_logins + 1 END,
           last_failed_at = now()
         WHERE id = $1
         RETURNING failed_logins",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await?;
    if failed < MAX_FAILURES {
        return Ok(false);
    }
    let minutes = (15i64 << (failed - MAX_FAILURES).min(7)).min(24 * 60);
    sqlx::query(
        "UPDATE users SET locked_until = now() + make_interval(mins => $2::int) WHERE id = $1",
    )
    .bind(user_id)
    .bind(minutes as i32)
    .execute(&state.db)
    .await?;
    Ok(failed == MAX_FAILURES)
}

async fn clear_failures(state: &AppState, user_id: Uuid) -> ApiResult<()> {
    sqlx::query("UPDATE users SET failed_logins = 0, locked_until = NULL WHERE id = $1 AND (failed_logins <> 0 OR locked_until IS NOT NULL)")
        .bind(user_id)
        .execute(&state.db)
        .await?;
    Ok(())
}

fn notify_locked(state: &AppState, user_id: Uuid) {
    // Best effort: tell the owner that somebody is guessing.
    let state = state.clone();
    tokio::spawn(async move {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT email::text, first_name FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten();
        if let Some((email, first)) = row {
            let html = format!(
                "<p>Hello {},</p><p>There were several failed sign-in attempts for your work account, so it is locked for a while. If that was not you, consider <a href=\"{}\">changing your password</a> and turning on two-factor authentication.</p>",
                mailer::escape_html(&first),
                mailer::escape_html(&format!("{}/forgot-password", state.config.frontend_url)),
            );
            mailer::send(&state, email, "Your work account was locked".into(), html);
        }
    });
}

#[utoipa::path(post, path="/auth/login", tag="auth", request_body = LoginBody, responses((status=200, body=AuthOut), (status=401), (status=429)))]
async fn login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<LoginBody>,
) -> ApiResult<Authed> {
    let email = body.email.trim().to_lowercase();
    // Two buckets: per source address and per target account (slows distributed guessing).
    throttle(
        &state,
        &format!("login-ip:{}", client_ip(&state, &headers, peer)),
    )
    .await?;
    throttle(&state, &format!("login-acct:{email}")).await?;
    if body.password.len() > MAX_PASSWORD_LEN * 4 {
        return Err(ApiError::Unauthorized);
    }

    let row: Option<(Uuid, String, bool, bool)> = sqlx::query_as(
        "SELECT id, password_hash, COALESCE(locked_until > now(), false) OR disabled_at IS NOT NULL, mfa_enabled_at IS NOT NULL
         FROM users WHERE email = $1 AND deleted_at IS NULL",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await?;

    // Always run one Argon2 verification, with a dummy hash if the account is unknown.
    let (known, hash) = match row {
        Some((id, h, locked, mfa)) => (Some((id, locked, mfa)), h),
        None => (None, state.dummy_hash.as_ref().clone()),
    };
    let password = body.password;
    let (ok, upgrade) = tokio::task::spawn_blocking(move || {
        let ok = verify_password(&password, &hash);
        // A migrated bcrypt hash is replaced by Argon2id now that the password is at hand.
        let upgrade = if ok && needs_rehash(&hash) {
            hash_password(&password).ok()
        } else {
            None
        };
        (ok, upgrade)
    })
    .await
    .map_err(|e| ApiError::Internal(Box::new(e)))?;

    // Same answer for unknown account, wrong password and locked account.
    let Some((id, locked, mfa)) = known else {
        return Err(ApiError::Unauthorized);
    };
    if locked {
        return Err(ApiError::Unauthorized);
    }
    if !ok {
        if record_failure(&state, id).await? {
            notify_locked(&state, id);
        }
        return Err(ApiError::Unauthorized);
    }
    if mfa {
        // The failure counter stays until the second factor succeeds, so a stolen password does
        // not give unlimited tries at the code.
        let token = random_token();
        sqlx::query("DELETE FROM mfa_challenges WHERE user_id = $1 OR expires_at < now()")
            .bind(id)
            .execute(&state.db)
            .await?;
        sqlx::query(
            "INSERT INTO mfa_challenges (token_hash, user_id, expires_at) VALUES ($1,$2,$3)",
        )
        .bind(hash_token(&token))
        .bind(id)
        .bind(Utc::now() + Duration::minutes(MFA_CHALLENGE_MINUTES))
        .execute(&state.db)
        .await?;
        return Ok((
            HeaderMap::new(),
            Json(AuthOut {
                mfa_required: true,
                mfa_token: Some(token),
                ..Default::default()
            }),
        ));
    }
    if let Some(new_hash) = upgrade {
        sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
            .bind(id)
            .bind(new_hash)
            .execute(&state.db)
            .await?;
    }
    clear_failures(&state, id).await?;
    let (pair, refresh) = start_session(
        &state,
        id,
        Uuid::new_v4(),
        &SessionMeta::from_request(&state, &headers, peer),
        &state.db,
    )
    .await?;
    Ok((cookie_headers(&state, &refresh), Json(pair.into())))
}

#[derive(Deserialize, ToSchema)]
struct MfaVerifyBody {
    mfa_token: String,
    /// Code from the authenticator app, or a recovery code.
    code: String,
}

/// Second step of a sign-in. A challenge allows 5 tries; every miss also counts toward the
/// account lock-out.
#[utoipa::path(post, path="/auth/mfa/verify", tag="auth", request_body = MfaVerifyBody, responses((status=200, body=AuthOut), (status=401), (status=429)))]
async fn mfa_verify(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<MfaVerifyBody>,
) -> ApiResult<Authed> {
    throttle(
        &state,
        &format!("mfa-ip:{}", client_ip(&state, &headers, peer)),
    )
    .await?;
    let hash = hash_token(body.mfa_token.trim());
    // Take one attempt atomically; a challenge that is used up or expired is gone.
    let claimed: Option<(Uuid, i32)> = sqlx::query_as(
        "UPDATE mfa_challenges SET attempts = attempts + 1 WHERE token_hash = $1 AND expires_at > now() AND attempts < $2
         RETURNING user_id, attempts",
    )
    .bind(&hash)
    .bind(MFA_CHALLENGE_ATTEMPTS)
    .fetch_optional(&state.db)
    .await?;
    let (user_id, _attempts) = claimed.ok_or(ApiError::Unauthorized)?;
    let locked: (bool,) = sqlx::query_as("SELECT COALESCE(locked_until > now(), false) OR disabled_at IS NOT NULL FROM users WHERE id = $1 AND deleted_at IS NULL")
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
    if locked.0 {
        return Err(ApiError::Unauthorized);
    }
    if !second_factor_ok(&state, user_id, &body.code).await? {
        if record_failure(&state, user_id).await? {
            notify_locked(&state, user_id);
        }
        return Err(ApiError::Unauthorized);
    }
    sqlx::query("DELETE FROM mfa_challenges WHERE token_hash = $1")
        .bind(&hash)
        .execute(&state.db)
        .await?;
    clear_failures(&state, user_id).await?;
    let (pair, refresh) = start_session(
        &state,
        user_id,
        Uuid::new_v4(),
        &SessionMeta::from_request(&state, &headers, peer),
        &state.db,
    )
    .await?;
    Ok((cookie_headers(&state, &refresh), Json(pair.into())))
}

/// Rotating refresh tokens. Presenting an already-used token means it leaked (or was
/// replayed), so the whole token family is revoked.
#[utoipa::path(post, path="/auth/refresh", tag="auth", responses((status=200, body=TokenPair), (status=401), (status=429)))]
async fn refresh(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> ApiResult<(HeaderMap, Json<TokenPair>)> {
    throttle(
        &state,
        &format!("refresh:{}", client_ip(&state, &headers, peer)),
    )
    .await?;
    let presented = read_refresh_cookie(&headers).ok_or(ApiError::Unauthorized)?;
    let mut tx = state.db.begin().await?;
    let row: Option<(Uuid, Uuid, Uuid, bool, bool)> = sqlx::query_as(
        "SELECT s.id, s.family_id, s.user_id, s.revoked_at IS NOT NULL, s.expires_at <= now()
         FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = $1 AND u.deleted_at IS NULL AND u.disabled_at IS NULL FOR UPDATE OF s",
    )
    .bind(hash_token(&presented))
    .fetch_optional(&mut *tx)
    .await?;
    let (id, family, user_id, revoked, expired) = row.ok_or(ApiError::Unauthorized)?;

    if revoked {
        sqlx::query(
            "UPDATE sessions SET revoked_at = now() WHERE family_id = $1 AND revoked_at IS NULL",
        )
        .bind(family)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Err(ApiError::Unauthorized);
    }
    if expired {
        return Err(ApiError::Unauthorized);
    }
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let (pair, refresh) = start_session(
        &state,
        user_id,
        family,
        &SessionMeta::from_request(&state, &headers, peer),
        &mut *tx,
    )
    .await?;
    tx.commit().await?;
    Ok((cookie_headers(&state, &refresh), Json(pair)))
}

#[utoipa::path(post, path="/auth/logout", tag="auth", responses((status=200, body=OkOut)), security(("bearer" = [])))]
async fn logout(
    State(state): State<AppState>,
    user: AuthUser,
) -> ApiResult<(HeaderMap, Json<OkOut>)> {
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1 AND user_id = $2")
        .bind(user.require_session()?)
        .bind(user.id)
        .execute(&state.db)
        .await?;
    let mut h = HeaderMap::new();
    h.insert(SET_COOKIE, refresh_cookie(&state, "", 0));
    Ok((h, Json(OkOut { ok: true })))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
struct Me {
    id: Uuid,
    email: String,
    first_name: String,
    last_name: String,
    /// Two-factor authentication is switched on for this account.
    mfa_enabled: bool,
    /// May open the admin panel.
    is_instance_admin: bool,
    /// How this request was authenticated: `session` or `api_token`.
    #[sqlx(skip)]
    auth_method: String,
}

#[utoipa::path(get, path="/me", tag="auth", responses((status=200, body=Me)), security(("bearer" = [])))]
async fn me(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Me>> {
    let mut me: Me = sqlx::query_as(
        "SELECT id, email::text AS email, first_name, last_name, mfa_enabled_at IS NOT NULL AS mfa_enabled, is_instance_admin FROM users WHERE id = $1",
    )
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;
    me.auth_method = if user.token_id.is_some() {
        "api_token"
    } else {
        "session"
    }
    .into();
    Ok(Json(me))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_refresh_cookie_among_others() {
        let mut h = HeaderMap::new();
        h.insert(
            COOKIE,
            HeaderValue::from_static("a=1; work_refresh=abc_DEF-1; b=2"),
        );
        assert_eq!(read_refresh_cookie(&h).as_deref(), Some("abc_DEF-1"));
        assert_eq!(read_refresh_cookie(&HeaderMap::new()), None);
    }
}
