//! Passkeys (WebAuthn): add them to an account, list and remove them, and sign in with one.
//! The cryptographic checks live in `webauthn.rs`; this module owns challenges, storage and policy.
//!
//! Policy:
//! * adding a passkey needs a real session and, for accounts that have a password, that password
//!   (a hijacked session must not be able to plant a permanent way in);
//! * a challenge is random, single use and lives five minutes;
//! * passkey sign-in is usernameless (the authenticator offers its resident credentials) and always
//!   requires user verification, so it also stands in for the second factor;
//! * the last way to sign in cannot be removed.

use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Path, State},
    http::HeaderMap,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::{
    auth::{client_ip, cookie_headers, start_session, throttle, AuthOut, SessionMeta},
    users::verify_current,
    OkOut,
};
use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthUser,
    security::webauthn::{self, b64, unb64, Rp},
    state::AppState,
};

const CHALLENGE_MINUTES: i64 = 5;
const MAX_PASSKEYS: i64 = 10;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(registration_options))
        .routes(routes!(list_passkeys, finish_registration))
        .routes(routes!(delete_passkey))
        .routes(routes!(login_options))
        .routes(routes!(passkey_login))
}

fn rp(s: &AppState) -> ApiResult<Rp> {
    Rp::from_config(&s.config).map_err(|e| ApiError::Internal(e.into()))
}

fn bad(msg: &'static str) -> ApiError {
    ApiError::BadRequest(msg.into())
}

async fn new_challenge(s: &AppState, kind: &str, user: Option<Uuid>) -> ApiResult<(Uuid, Vec<u8>)> {
    let mut challenge = vec![0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut challenge);
    let id = Uuid::new_v4();
    sqlx::query("DELETE FROM webauthn_challenges WHERE expires_at < now()")
        .execute(&s.db)
        .await?;
    sqlx::query("INSERT INTO webauthn_challenges (id, kind, user_id, challenge, expires_at) VALUES ($1,$2,$3,$4,$5)")
        .bind(id)
        .bind(kind)
        .bind(user)
        .bind(&challenge)
        .bind(Utc::now() + Duration::minutes(CHALLENGE_MINUTES))
        .execute(&s.db)
        .await?;
    Ok((id, challenge))
}

/// Takes the challenge: it exists, is of this kind and for this user, is not expired, and is gone afterwards.
async fn take_challenge(
    s: &AppState,
    id: Uuid,
    kind: &str,
    user: Option<Uuid>,
) -> ApiResult<Vec<u8>> {
    let row: Option<(Vec<u8>,)> = sqlx::query_as(
        "DELETE FROM webauthn_challenges WHERE id = $1 AND kind = $2 AND user_id IS NOT DISTINCT FROM $3 AND expires_at > now()
         RETURNING challenge",
    )
    .bind(id)
    .bind(kind)
    .bind(user)
    .fetch_optional(&s.db)
    .await?;
    row.map(|r| r.0)
        .ok_or_else(|| bad("the request expired, please try again"))
}

#[derive(Serialize, ToSchema)]
pub struct CeremonyOptions {
    pub challenge_id: Uuid,
    /// Pass to `navigator.credentials.create()` / `.get()` as `publicKey` (binary values are base64url).
    #[schema(value_type = Object)]
    pub options: Value,
}

#[derive(Deserialize, ToSchema)]
pub struct RegistrationStart {
    /// Required for accounts that have a password.
    pub password: Option<String>,
}

#[utoipa::path(post, operation_id = "passkeyRegistrationOptions", path = "/me/passkeys/options", tag = "auth",
    request_body = RegistrationStart,
    responses((status = 200, body = CeremonyOptions), (status = 403), (status = 409), (status = 429)), security(("bearer" = [])))]
async fn registration_options(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<RegistrationStart>,
) -> ApiResult<Json<CeremonyOptions>> {
    user.require_session()?;
    let (email, first, last, has_password): (String, String, String, bool) = sqlx::query_as(
        "SELECT email::text, first_name, last_name, password_hash <> '' FROM users WHERE id = $1",
    )
    .bind(user.id)
    .fetch_one(&s.db)
    .await?;
    if has_password {
        verify_current(&s, &user, b.password.unwrap_or_default()).await?;
    }
    let existing: Vec<(Vec<u8>,)> =
        sqlx::query_as("SELECT credential_id FROM passkeys WHERE user_id = $1")
            .bind(user.id)
            .fetch_all(&s.db)
            .await?;
    if existing.len() as i64 >= MAX_PASSKEYS {
        return Err(ApiError::Conflict);
    }
    let rp = rp(&s)?;
    let (challenge_id, challenge) = new_challenge(&s, "register", Some(user.id)).await?;
    Ok(Json(CeremonyOptions {
        challenge_id,
        options: json!({
            "rp": {"id": rp.id, "name": "work"},
            "user": {"id": b64(user.id.as_bytes()), "name": email, "displayName": format!("{first} {last}").trim()},
            "challenge": b64(&challenge),
            "pubKeyCredParams": [{"type": "public-key", "alg": -7}],
            "timeout": 120_000,
            "attestation": "none",
            "authenticatorSelection": {"residentKey": "required", "requireResidentKey": true, "userVerification": "required"},
            // Authenticators that already hold one of the user's passkeys are not offered again.
            "excludeCredentials": existing.iter().map(|(id,)| json!({"type": "public-key", "id": b64(id)})).collect::<Vec<_>>(),
        }),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct AttestationResponse {
    pub client_data_json: String,
    pub attestation_object: String,
}

#[derive(Deserialize, ToSchema)]
pub struct RegistrationFinish {
    pub challenge_id: Uuid,
    /// A name to tell the passkeys apart ("Laptop", "Phone").
    pub name: String,
    pub response: AttestationResponse,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct PasskeyOut {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

#[utoipa::path(post, operation_id = "finishPasskeyRegistration", path = "/me/passkeys", tag = "auth",
    request_body = RegistrationFinish,
    responses((status = 200, body = PasskeyOut), (status = 400), (status = 409)), security(("bearer" = [])))]
async fn finish_registration(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<RegistrationFinish>,
) -> ApiResult<Json<PasskeyOut>> {
    user.require_session()?;
    let name = b.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(bad("the name must be 1-100 characters"));
    }
    let challenge = take_challenge(&s, b.challenge_id, "register", Some(user.id)).await?;
    let client_data = unb64(&b.response.client_data_json).map_err(bad)?;
    let attestation = unb64(&b.response.attestation_object).map_err(bad)?;
    let cred = webauthn::verify_registration(&rp(&s)?, &challenge, &client_data, &attestation)
        .map_err(|e| ApiError::BadRequest(format!("the passkey was refused: {e}")))?;
    // The unique index also stops one credential from being registered for two accounts.
    let row = sqlx::query_as(
        "INSERT INTO passkeys (id, user_id, credential_id, public_key, sign_count, name) VALUES ($1,$2,$3,$4,$5,$6)
         RETURNING id, name, created_at, last_used_at",
    )
    .bind(Uuid::new_v4())
    .bind(user.id)
    .bind(&cred.id)
    .bind(&cred.public_key)
    .bind(cred.sign_count as i64)
    .bind(name)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(row))
}

#[utoipa::path(get, operation_id = "listPasskeys", path = "/me/passkeys", tag = "auth",
    responses((status = 200, body = Vec<PasskeyOut>)), security(("bearer" = [])))]
async fn list_passkeys(
    State(s): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<PasskeyOut>>> {
    Ok(Json(
        sqlx::query_as("SELECT id, name, created_at, last_used_at FROM passkeys WHERE user_id = $1 ORDER BY created_at")
            .bind(user.id)
            .fetch_all(&s.db)
            .await?,
    ))
}

#[utoipa::path(delete, operation_id = "deletePasskey", path = "/me/passkeys/{id}", tag = "auth",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = OkOut), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn delete_passkey(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    user.require_session()?;
    let mut tx = s.db.begin().await?;
    let (has_password,): (bool,) =
        sqlx::query_as("SELECT password_hash <> '' FROM users WHERE id = $1 FOR UPDATE")
            .bind(user.id)
            .fetch_one(&mut *tx)
            .await?;
    let (others,): (i64,) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM passkeys WHERE user_id = $1 AND id <> $2) + (SELECT count(*) FROM sso_identities WHERE user_id = $1)",
    )
    .bind(user.id)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if !has_password && others == 0 {
        return Err(ApiError::Conflict);
    }
    let gone = sqlx::query("DELETE FROM passkeys WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user.id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if gone == 0 {
        return Err(ApiError::NotFound);
    }
    tx.commit().await?;
    Ok(Json(OkOut { ok: true }))
}

#[utoipa::path(post, operation_id = "passkeyLoginOptions", path = "/auth/passkey/options", tag = "auth",
    responses((status = 200, body = CeremonyOptions), (status = 429)))]
async fn login_options(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> ApiResult<Json<CeremonyOptions>> {
    throttle(&s, &format!("passkey:{}", client_ip(&s, &headers, peer))).await?;
    let rp = rp(&s)?;
    let (challenge_id, challenge) = new_challenge(&s, "login", None).await?;
    Ok(Json(CeremonyOptions {
        challenge_id,
        options: json!({
            "challenge": b64(&challenge),
            "rpId": rp.id,
            "timeout": 120_000,
            "userVerification": "required",
            "allowCredentials": [], // usernameless: the authenticator offers its own passkeys
        }),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct AssertionResponse {
    pub client_data_json: String,
    pub authenticator_data: String,
    pub signature: String,
    pub user_handle: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct PasskeyLogin {
    pub challenge_id: Uuid,
    /// The credential id (base64url).
    pub id: String,
    pub response: AssertionResponse,
}

#[utoipa::path(post, operation_id = "passkeyLogin", path = "/auth/passkey/login", tag = "auth",
    request_body = PasskeyLogin,
    responses((status = 200, body = AuthOut), (status = 401), (status = 429)))]
async fn passkey_login(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(b): Json<PasskeyLogin>,
) -> ApiResult<(HeaderMap, Json<AuthOut>)> {
    throttle(&s, &format!("passkey:{}", client_ip(&s, &headers, peer))).await?;
    // Whatever goes wrong, the caller learns nothing beyond "no".
    let denied = || ApiError::Unauthorized;
    let challenge = take_challenge(&s, b.challenge_id, "login", None)
        .await
        .map_err(|_| denied())?;
    let credential_id = unb64(&b.id).map_err(|_| denied())?;
    let row: Option<(Uuid, Uuid, Vec<u8>, i64)> = sqlx::query_as(
        "SELECT p.id, p.user_id, p.public_key, p.sign_count FROM passkeys p JOIN users u ON u.id = p.user_id
         WHERE p.credential_id = $1 AND u.deleted_at IS NULL AND u.disabled_at IS NULL
           AND (u.locked_until IS NULL OR u.locked_until < now())",
    )
    .bind(&credential_id)
    .fetch_optional(&s.db)
    .await?;
    let (passkey_id, user_id, public_key, stored) = row.ok_or_else(denied)?;
    // A resident credential reports whose it is; it must be the account the credential belongs to.
    if let Some(handle) = &b.response.user_handle {
        if unb64(handle).map_err(|_| denied())? != user_id.as_bytes() {
            return Err(denied());
        }
    }
    let new_count = webauthn::verify_assertion(
        &rp(&s)?,
        &challenge,
        &unb64(&b.response.client_data_json).map_err(|_| denied())?,
        &unb64(&b.response.authenticator_data).map_err(|_| denied())?,
        &unb64(&b.response.signature).map_err(|_| denied())?,
        &public_key,
        u32::try_from(stored).unwrap_or(u32::MAX),
    )
    .map_err(|e| {
        tracing::info!(error = e, %user_id, "passkey sign-in refused");
        denied()
    })?;
    sqlx::query("UPDATE passkeys SET sign_count = $2, last_used_at = now() WHERE id = $1")
        .bind(passkey_id)
        .bind(new_count as i64)
        .execute(&s.db)
        .await?;
    let (pair, refresh) = start_session(
        &s,
        user_id,
        Uuid::new_v4(),
        &SessionMeta::from_request(&s, &headers, peer),
        &s.db,
    )
    .await?;
    Ok((cookie_headers(&s, &refresh), Json(pair.into())))
}
