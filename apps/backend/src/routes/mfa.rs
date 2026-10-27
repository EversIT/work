//! Multi-factor authentication with authenticator apps (TOTP) and recovery codes.
//!
//! Everything here needs a real session (never an API token) and the current password, so a
//! stolen access token cannot enrol the attacker's authenticator. The secret is encrypted at rest
//! with `WORK_ENCRYPTION_KEY` and bound to the user id; without that key MFA is unavailable.

use axum::{extract::State, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::{users::verify_current, OkOut};
use crate::{
    error::{ApiError, ApiResult},
    security::auth::{hash_token, AuthUser},
    security::totp,
    services::integrations::{decrypt, encrypt},
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(mfa_setup))
        .routes(routes!(mfa_enable))
        .routes(routes!(mfa_disable))
        .routes(routes!(mfa_recovery_codes))
}

fn key(s: &AppState) -> ApiResult<&[u8; 32]> {
    // No key, no MFA: a secret must never be stored unencrypted.
    s.config.encryption_key.as_ref().ok_or(ApiError::Conflict)
}

/// Checks a TOTP code or a recovery code for an account that has MFA enabled. A TOTP code is
/// accepted once; a recovery code is consumed. Callers throttle and count failures.
pub(crate) async fn second_factor_ok(s: &AppState, user_id: Uuid, code: &str) -> ApiResult<bool> {
    let Some(enc_key) = s.config.encryption_key.as_ref() else {
        return Ok(false);
    };
    let row: Option<(Vec<u8>, Option<i64>)> = sqlx::query_as(
        "SELECT mfa_secret, mfa_last_step FROM users
         WHERE id = $1 AND mfa_enabled_at IS NOT NULL AND mfa_secret IS NOT NULL AND deleted_at IS NULL",
    )
    .bind(user_id)
    .fetch_optional(&s.db)
    .await?;
    let Some((sealed, last_step)) = row else {
        return Ok(false);
    };
    let Some(secret) = decrypt(enc_key, user_id.as_bytes(), &sealed) else {
        return Ok(false);
    };
    if let Some(step) = totp::verify(&secret, code, Utc::now().timestamp(), last_step) {
        // The guard makes the step single-use even when two requests race.
        let claimed = sqlx::query(
            "UPDATE users SET mfa_last_step = $2 WHERE id = $1 AND (mfa_last_step IS NULL OR mfa_last_step < $2)",
        )
        .bind(user_id)
        .bind(step)
        .execute(&s.db)
        .await?
        .rows_affected();
        return Ok(claimed == 1);
    }
    let normalized = totp::normalize_recovery_code(code);
    if normalized.len() != 10 {
        return Ok(false);
    }
    let used = sqlx::query(
        "UPDATE mfa_recovery_codes SET used_at = now() WHERE user_id = $1 AND code_hash = $2 AND used_at IS NULL",
    )
    .bind(user_id)
    .bind(hash_token(&normalized))
    .execute(&s.db)
    .await?
    .rows_affected();
    Ok(used == 1)
}

async fn throttle(s: &AppState, user: &AuthUser) -> ApiResult<()> {
    if s.auth_limiter.check(&format!("mfa:{}", user.id)).await {
        Ok(())
    } else {
        Err(ApiError::TooManyRequests)
    }
}

#[derive(Deserialize, ToSchema)]
pub struct PasswordOnly {
    pub password: String,
}

#[derive(Serialize, ToSchema)]
pub struct MfaSetupOut {
    /// Base32 secret for manual entry.
    pub secret: String,
    /// `otpauth://` URI; render it as a QR code.
    pub otpauth_uri: String,
}

/// Starts enrolment: stores a fresh pending secret (replacing an unconfirmed one). MFA is only
/// active once `/me/mfa/enable` has seen a valid code.
#[utoipa::path(post, operation_id = "setupMfa", path = "/me/mfa/setup", tag = "auth",
    request_body = PasswordOnly,
    responses((status = 200, body = MfaSetupOut), (status = 403), (status = 409)), security(("bearer" = [])))]
async fn mfa_setup(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<PasswordOnly>,
) -> ApiResult<Json<MfaSetupOut>> {
    user.require_session()?;
    let enc_key = key(&s)?;
    verify_current(&s, &user, b.password).await?;
    let (email, enabled): (String, bool) =
        sqlx::query_as("SELECT email::text, mfa_enabled_at IS NOT NULL FROM users WHERE id = $1")
            .bind(user.id)
            .fetch_one(&s.db)
            .await?;
    if enabled {
        return Err(ApiError::Conflict);
    }
    let secret = totp::generate_secret();
    sqlx::query("UPDATE users SET mfa_secret = $2, mfa_last_step = NULL WHERE id = $1")
        .bind(user.id)
        .bind(encrypt(enc_key, user.id.as_bytes(), &secret))
        .execute(&s.db)
        .await?;
    Ok(Json(MfaSetupOut {
        secret: totp::base32(&secret),
        otpauth_uri: totp::otpauth_uri("work", &email, &secret),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct CodeBody {
    pub code: String,
}

#[derive(Serialize, ToSchema)]
pub struct RecoveryCodesOut {
    /// Shown once. Each code works one time.
    pub recovery_codes: Vec<String>,
}

async fn replace_recovery_codes(
    tx: &mut sqlx::PgConnection,
    user_id: Uuid,
) -> ApiResult<Vec<String>> {
    sqlx::query("DELETE FROM mfa_recovery_codes WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    let codes = totp::generate_recovery_codes();
    for c in &codes {
        sqlx::query("INSERT INTO mfa_recovery_codes (user_id, code_hash) VALUES ($1,$2)")
            .bind(user_id)
            .bind(hash_token(&totp::normalize_recovery_code(c)))
            .execute(&mut *tx)
            .await?;
    }
    Ok(codes)
}

/// Confirms enrolment with a code from the authenticator and returns the recovery codes.
#[utoipa::path(post, operation_id = "enableMfa", path = "/me/mfa/enable", tag = "auth",
    request_body = CodeBody,
    responses((status = 200, body = RecoveryCodesOut), (status = 400), (status = 409), (status = 429)), security(("bearer" = [])))]
async fn mfa_enable(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<CodeBody>,
) -> ApiResult<Json<RecoveryCodesOut>> {
    user.require_session()?;
    let enc_key = key(&s)?;
    throttle(&s, &user).await?;
    let row: Option<(Vec<u8>,)> = sqlx::query_as(
        "SELECT mfa_secret FROM users WHERE id = $1 AND mfa_secret IS NOT NULL AND mfa_enabled_at IS NULL",
    )
    .bind(user.id)
    .fetch_optional(&s.db)
    .await?;
    let (sealed,) = row.ok_or(ApiError::Conflict)?; // no enrolment in progress, or already on
    let secret = decrypt(enc_key, user.id.as_bytes(), &sealed).ok_or(ApiError::Conflict)?;
    let step = totp::verify(&secret, &b.code, Utc::now().timestamp(), None)
        .ok_or_else(|| ApiError::BadRequest("the code is not correct".into()))?;
    let mut tx = s.db.begin().await?;
    let done = sqlx::query(
        "UPDATE users SET mfa_enabled_at = now(), mfa_last_step = $2 WHERE id = $1 AND mfa_enabled_at IS NULL",
    )
    .bind(user.id)
    .bind(step)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if done != 1 {
        return Err(ApiError::Conflict);
    }
    let recovery_codes = replace_recovery_codes(&mut tx, user.id).await?;
    tx.commit().await?;
    Ok(Json(RecoveryCodesOut { recovery_codes }))
}

#[derive(Deserialize, ToSchema)]
pub struct PasswordAndCode {
    pub password: String,
    /// Authenticator code or recovery code.
    pub code: String,
}

/// Both factors are needed to switch MFA off again.
#[utoipa::path(post, operation_id = "disableMfa", path = "/me/mfa/disable", tag = "auth",
    request_body = PasswordAndCode,
    responses((status = 200, body = OkOut), (status = 403), (status = 429)), security(("bearer" = [])))]
async fn mfa_disable(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<PasswordAndCode>,
) -> ApiResult<Json<OkOut>> {
    user.require_session()?;
    throttle(&s, &user).await?;
    verify_current(&s, &user, b.password).await?;
    if !second_factor_ok(&s, user.id, &b.code).await? {
        return Err(ApiError::Forbidden);
    }
    let mut tx = s.db.begin().await?;
    sqlx::query("UPDATE users SET mfa_secret = NULL, mfa_enabled_at = NULL, mfa_last_step = NULL WHERE id = $1")
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM mfa_recovery_codes WHERE user_id = $1")
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(OkOut { ok: true }))
}

/// Replaces all recovery codes with a fresh set.
#[utoipa::path(post, operation_id = "regenerateRecoveryCodes", path = "/me/mfa/recovery-codes", tag = "auth",
    request_body = PasswordAndCode,
    responses((status = 200, body = RecoveryCodesOut), (status = 403), (status = 429)), security(("bearer" = [])))]
async fn mfa_recovery_codes(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<PasswordAndCode>,
) -> ApiResult<Json<RecoveryCodesOut>> {
    user.require_session()?;
    throttle(&s, &user).await?;
    verify_current(&s, &user, b.password).await?;
    if !second_factor_ok(&s, user.id, &b.code).await? {
        return Err(ApiError::Forbidden);
    }
    let mut tx = s.db.begin().await?;
    let recovery_codes = replace_recovery_codes(&mut tx, user.id).await?;
    tx.commit().await?;
    Ok(Json(RecoveryCodesOut { recovery_codes }))
}
