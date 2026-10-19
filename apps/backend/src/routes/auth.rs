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
    auth::*,
    authz::Role,
    error::{ApiError, ApiResult},
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(register))
        .routes(routes!(login))
        .routes(routes!(refresh))
        .routes(routes!(logout))
        .routes(routes!(me))
}

fn client_ip(state: &AppState, headers: &HeaderMap, peer: SocketAddr) -> String {
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

fn throttle(state: &AppState, key: &str) -> ApiResult<()> {
    if state.auth_limiter.check(key) {
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

fn cookie_headers(state: &AppState, refresh: &str) -> HeaderMap {
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

type Authed = (HeaderMap, Json<TokenPair>);

async fn start_session(
    state: &AppState,
    user_id: Uuid,
    family_id: Uuid,
    exec: impl sqlx::PgExecutor<'_>,
) -> ApiResult<(TokenPair, String)> {
    let session_id = Uuid::new_v4();
    let refresh = random_token();
    sqlx::query(
        "INSERT INTO sessions (id, family_id, user_id, token_hash, expires_at) VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(session_id)
    .bind(family_id)
    .bind(user_id)
    .bind(hash_token(&refresh))
    .bind(Utc::now() + Duration::days(REFRESH_TTL_DAYS))
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

fn check_password(p: &str) -> ApiResult<()> {
    let n = p.chars().count();
    if !(MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&n) {
        return Err(ApiError::BadRequest(format!(
            "password must be {MIN_PASSWORD_LEN}-{MAX_PASSWORD_LEN} characters"
        )));
    }
    Ok(())
}

#[utoipa::path(post, path="/auth/register", tag="auth", request_body = RegisterBody, responses((status=200, body=TokenPair), (status=409), (status=429)))]
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
    )?;
    body.validate()?;
    check_password(&body.password)?;

    // Hashing is CPU-heavy; keep it off the async workers.
    let password = body.password.clone();
    let hash = tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|e| ApiError::Internal(Box::new(e)))??;

    let user_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO users (id, email, password_hash, first_name, last_name) VALUES ($1,$2,$3,$4,$5)")
        .bind(user_id)
        .bind(body.email.trim())
        .bind(&hash)
        .bind(body.first_name.trim())
        .bind(body.last_name.trim())
        .execute(&mut *tx)
        .await?; // unique violation -> 409
    sqlx::query("INSERT INTO projects (id, name, created_by) VALUES ($1,$2,$3)")
        .bind(project_id)
        .bind(format!("{}'s Project", body.first_name.trim()))
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES ($1,$2,$3)")
        .bind(project_id)
        .bind(user_id)
        .bind(Role::Owner.as_str())
        .execute(&mut *tx)
        .await?;
    let (pair, refresh) = start_session(&state, user_id, Uuid::new_v4(), &mut *tx).await?;
    tx.commit().await?;
    Ok((cookie_headers(&state, &refresh), Json(pair)))
}

#[derive(Deserialize, ToSchema)]
struct LoginBody {
    email: String,
    password: String,
}

#[utoipa::path(post, path="/auth/login", tag="auth", request_body = LoginBody, responses((status=200, body=TokenPair), (status=401), (status=429)))]
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
    )?;
    throttle(&state, &format!("login-acct:{email}"))?;
    if body.password.len() > MAX_PASSWORD_LEN * 4 {
        return Err(ApiError::Unauthorized);
    }

    let row: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, password_hash FROM users WHERE email = $1 AND deleted_at IS NULL",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await?;

    // Always run one Argon2 verification, with a dummy hash if the account is unknown.
    let (user_id, hash) = match row {
        Some((id, h)) => (Some(id), h),
        None => (None, state.dummy_hash.as_ref().clone()),
    };
    let password = body.password;
    let ok = tokio::task::spawn_blocking(move || verify_password(&password, &hash))
        .await
        .map_err(|e| ApiError::Internal(Box::new(e)))?;

    match (user_id, ok) {
        (Some(id), true) => {
            let (pair, refresh) = start_session(&state, id, Uuid::new_v4(), &state.db).await?;
            Ok((cookie_headers(&state, &refresh), Json(pair)))
        }
        _ => Err(ApiError::Unauthorized), // same answer for unknown account and wrong password
    }
}

/// Rotating refresh tokens. Presenting an already-used token means it leaked (or was
/// replayed), so the whole token family is revoked.
#[utoipa::path(post, path="/auth/refresh", tag="auth", responses((status=200, body=TokenPair), (status=401), (status=429)))]
async fn refresh(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> ApiResult<Authed> {
    throttle(
        &state,
        &format!("refresh:{}", client_ip(&state, &headers, peer)),
    )?;
    let presented = read_refresh_cookie(&headers).ok_or(ApiError::Unauthorized)?;
    let mut tx = state.db.begin().await?;
    let row: Option<(Uuid, Uuid, Uuid, bool, bool)> = sqlx::query_as(
        "SELECT s.id, s.family_id, s.user_id, s.revoked_at IS NOT NULL, s.expires_at <= now()
         FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = $1 AND u.deleted_at IS NULL FOR UPDATE OF s",
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
    let (pair, refresh) = start_session(&state, user_id, family, &mut *tx).await?;
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
    /// How this request was authenticated: `session` or `api_token`.
    #[sqlx(skip)]
    auth_method: String,
}

#[utoipa::path(get, path="/me", tag="auth", responses((status=200, body=Me)), security(("bearer" = [])))]
async fn me(State(state): State<AppState>, user: AuthUser) -> ApiResult<Json<Me>> {
    let mut me: Me = sqlx::query_as(
        "SELECT id, email::text AS email, first_name, last_name FROM users WHERE id = $1",
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
