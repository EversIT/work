//! Authentication: Argon2id passwords, short-lived JWT access tokens and rotating,
//! revocable refresh tokens. The JWT carries only `sub`, `sid` and `exp`; roles and
//! membership are always read from the database.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{extract::FromRequestParts, http::request::Parts};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

pub const ACCESS_TTL_MINUTES: i64 = 15;
pub const REFRESH_TTL_DAYS: i64 = 30;
pub const MIN_PASSWORD_LEN: usize = 12;
pub const MAX_PASSWORD_LEN: usize = 128;

pub fn hash_password(password: &str) -> Result<String, ApiError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| ApiError::Internal(Box::new(std::io::Error::other(e.to_string()))))
}

/// Hashes from a migrated upstream database are bcrypt. They verify, and `needs_rehash` tells the
/// sign-in to replace them with Argon2id right after a successful check.
fn is_bcrypt(hash: &str) -> bool {
    ["$2a$", "$2b$", "$2x$", "$2y$"]
        .iter()
        .any(|p| hash.starts_with(p))
}

pub fn needs_rehash(hash: &str) -> bool {
    is_bcrypt(hash)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    if is_bcrypt(hash) {
        // bcrypt only looks at the first 72 bytes; that is its own property, kept as it was.
        return bcrypt::verify(password, hash).unwrap_or(false);
    }
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// 256-bit random secret, URL-safe base64. Returned to the client once.
pub fn random_token() -> String {
    let mut buf = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Tokens are high-entropy, so a plain SHA-256 is sufficient for storage.
pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    sid: Uuid,
    exp: i64,
    iat: i64,
}

pub fn issue_access_token(
    secret: &[u8],
    user_id: Uuid,
    session_id: Uuid,
) -> Result<String, ApiError> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id,
        sid: session_id,
        iat: now.timestamp(),
        exp: (now + Duration::minutes(ACCESS_TTL_MINUTES)).timestamp(),
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .map_err(|e| ApiError::Internal(Box::new(e)))
}

fn decode_access_token(secret: &[u8], token: &str) -> Result<Claims, ApiError> {
    let mut validation = Validation::new(Algorithm::HS256); // algorithm pinned: no `alg` confusion
    validation.set_required_spec_claims(&["exp", "sub"]);
    validation.leeway = 5;
    decode::<Claims>(token, &DecodingKey::from_secret(secret), &validation)
        .map(|d| d.claims)
        .map_err(|_| ApiError::Unauthorized)
}

pub const API_TOKEN_PREFIX: &str = "wrk_";

/// Generates a personal API token (`wrk_` + 256 bits) and returns it with its display prefix.
pub fn new_api_token() -> (String, String) {
    let token = format!("{API_TOKEN_PREFIX}{}", random_token());
    let prefix = token.chars().take(API_TOKEN_PREFIX.len() + 4).collect();
    (token, prefix)
}

/// The authenticated caller. Every protected handler takes this as an argument, so
/// there is no way to reach user data without going through it.
///
/// Callers authenticate with a session (JWT) or a personal API token. Tokens act as the user
/// with the same permissions, but handlers that change credentials call `require_session`.
#[derive(Debug, Clone, Copy)]
pub struct AuthUser {
    pub id: Uuid,
    /// Set when authenticated through a browser session.
    pub session_id: Option<Uuid>,
    /// Set when authenticated through an API token.
    pub token_id: Option<Uuid>,
}

impl AuthUser {
    /// For endpoints that must never be reachable with a long-lived API token (managing tokens,
    /// passwords, account deletion): returns the session id or `Forbidden`.
    pub fn require_session(&self) -> Result<Uuid, ApiError> {
        self.session_id.ok_or(ApiError::Forbidden)
    }
}

async fn authenticate_token(
    state: &AppState,
    token: &str,
    email: Option<&str>,
    method: &axum::http::Method,
) -> Result<AuthUser, ApiError> {
    if !token.starts_with(API_TOKEN_PREFIX) || token.len() > 200 {
        return Err(ApiError::Unauthorized);
    }
    let row: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT t.id, t.user_id, t.scope, u.email::text FROM api_tokens t JOIN users u ON u.id = t.user_id
         WHERE t.token_hash = $1 AND t.revoked_at IS NULL AND (t.expires_at IS NULL OR t.expires_at > now())
           AND u.deleted_at IS NULL AND u.disabled_at IS NULL",
    )
    .bind(hash_token(token))
    .fetch_optional(&state.db)
    .await?;
    let (token_id, user_id, scope, user_email) = row.ok_or(ApiError::Unauthorized)?;
    // Basic auth (Jira-style `email:token`) must name the token's owner.
    if let Some(e) = email {
        if !e.trim().eq_ignore_ascii_case(&user_email) {
            return Err(ApiError::Unauthorized);
        }
    }
    if !state.api_limiter.check(&token_id.to_string()).await {
        return Err(ApiError::TooManyRequests);
    }
    let safe = matches!(
        *method,
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    );
    if scope == "read" && !safe {
        return Err(ApiError::Forbidden);
    }
    sqlx::query("UPDATE api_tokens SET last_used_at = now() WHERE id = $1 AND (last_used_at IS NULL OR last_used_at < now() - interval '1 minute')")
        .bind(token_id)
        .execute(&state.db)
        .await?;
    Ok(AuthUser {
        id: user_id,
        session_id: None,
        token_id: Some(token_id),
    })
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(ApiError::Unauthorized)?;

        if let Some(basic) = header.strip_prefix("Basic ") {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(basic.trim())
                .map_err(|_| ApiError::Unauthorized)?;
            let text = String::from_utf8(decoded).map_err(|_| ApiError::Unauthorized)?;
            let (email, token) = text.split_once(':').ok_or(ApiError::Unauthorized)?;
            return authenticate_token(state, token, Some(email), &parts.method).await;
        }

        let token = header
            .strip_prefix("Bearer ")
            .ok_or(ApiError::Unauthorized)?;
        if token.starts_with(API_TOKEN_PREFIX) {
            return authenticate_token(state, token, None, &parts.method).await;
        }
        let claims = decode_access_token(&state.config.jwt_secret, token)?;

        // Revocation: the session must still be live and the account active.
        let live: Option<(Uuid,)> = sqlx::query_as(
            "SELECT s.id FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.id = $1 AND s.user_id = $2 AND s.revoked_at IS NULL
               AND s.expires_at > now() AND u.deleted_at IS NULL AND u.disabled_at IS NULL",
        )
        .bind(claims.sid)
        .bind(claims.sub)
        .fetch_optional(&state.db)
        .await?;
        live.ok_or(ApiError::Unauthorized)?;

        Ok(AuthUser {
            id: claims.sub,
            session_id: Some(claims.sid),
            token_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"test-only-secret-test-only-secret-0123";

    #[test]
    fn password_roundtrip() {
        let h = hash_password("correct horse battery").unwrap();
        assert!(verify_password("correct horse battery", &h));
        assert!(!verify_password("wrong", &h));
        assert!(!verify_password("x", "not-a-hash"));
        assert!(!needs_rehash(&h));
    }

    #[test]
    fn upstream_bcrypt_hashes_verify_and_are_flagged_for_upgrade() {
        let h = bcrypt::hash("an old upstream password", 4).unwrap();
        assert!(needs_rehash(&h));
        assert!(verify_password("an old upstream password", &h));
        assert!(!verify_password("another password", &h));
        assert!(!verify_password("", &h));
        assert!(!verify_password("x", "$2b$not-a-real-hash"));
    }

    #[test]
    fn jwt_roundtrip_and_tamper() {
        let (u, s) = (Uuid::new_v4(), Uuid::new_v4());
        let t = issue_access_token(SECRET, u, s).unwrap();
        let c = decode_access_token(SECRET, &t).unwrap();
        assert_eq!((c.sub, c.sid), (u, s));
        assert!(decode_access_token(b"another-secret-another-secret-0123456", &t).is_err());
        assert!(decode_access_token(SECRET, &format!("{t}x")).is_err());
    }

    #[test]
    fn jwt_rejects_alg_none() {
        // header {"alg":"none","typ":"JWT"}, payload with far-future exp, empty signature
        let t = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJzdWIiOiIwMDAwMDAwMC0wMDAwLTAwMDAtMDAwMC0wMDAwMDAwMDAwMDAiLCJzaWQiOiIwMDAwMDAwMC0wMDAwLTAwMDAtMDAwMC0wMDAwMDAwMDAwMDAiLCJleHAiOjQxMDI0NDQ4MDAsImlhdCI6MH0.";
        assert!(decode_access_token(SECRET, t).is_err());
    }

    #[test]
    fn api_tokens_have_prefix_and_entropy() {
        let (t, p) = new_api_token();
        assert!(t.starts_with("wrk_") && t.len() > 40);
        assert!(t.starts_with(&p) && p.len() == 8);
        assert_ne!(t, new_api_token().0);
    }

    #[test]
    fn tokens_are_unique_and_hash_is_stable() {
        assert_ne!(random_token(), random_token());
        assert_eq!(hash_token("a"), hash_token("a"));
        assert_ne!(hash_token("a"), hash_token("b"));
    }
}
