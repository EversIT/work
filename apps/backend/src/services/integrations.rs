//! Third-party connections (Slack). Differences from the original implementation:
//! * OAuth `state` is a random one-time value stored server-side, bound to the user who started
//!   the flow, and redeemed by an authenticated request: it cannot be forged or replayed.
//! * Tokens are encrypted at rest with AES-256-GCM; the API never returns them.

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use axum::{extract::State, Json};
use chrono::{Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::types::Json as SqlJson;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    routes::OkOut,
    security::auth::{hash_token, random_token, AuthUser},
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_integrations))
        .routes(routes!(slack_auth_url))
        .routes(routes!(slack_callback))
        .routes(routes!(slack_disconnect))
        .routes(routes!(slack_channels))
}

pub fn encrypt(key: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let mut out = nonce.to_vec();
    out.extend(
        cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .expect("AES-GCM encryption cannot fail for in-memory buffers"),
    );
    out
}

pub fn decrypt(key: &[u8; 32], aad: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 12 + 16 {
        return None;
    }
    let (nonce, ct) = data.split_at(12);
    Aes256Gcm::new(key.into())
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ct, aad })
        .ok()
}

#[derive(Serialize, ToSchema)]
pub struct IntegrationOut {
    pub r#type: String,
    pub connected: bool,
    #[schema(value_type = Object)]
    pub config: Value,
}

#[derive(Serialize, ToSchema)]
pub struct IntegrationList {
    /// Integrations this instance has configured. Others cannot be connected.
    pub enabled: Vec<String>,
    pub connected: Vec<IntegrationOut>,
}

#[utoipa::path(get, operation_id = "listIntegrations", path = "/integrations", tag = "integrations",
    responses((status = 200, body = IntegrationList)), security(("bearer" = [])))]
async fn list_integrations(
    State(s): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<IntegrationList>> {
    let rows: Vec<(String, SqlJson<Value>)> =
        sqlx::query_as("SELECT type, public_config FROM user_integrations WHERE user_id = $1")
            .bind(user.id)
            .fetch_all(&s.db)
            .await?;
    let enabled = if s.config.slack.is_some() {
        vec!["slack".to_owned()]
    } else {
        vec![]
    };
    Ok(Json(IntegrationList {
        enabled,
        connected: rows
            .into_iter()
            .map(|(t, c)| IntegrationOut {
                r#type: t,
                connected: true,
                config: c.0,
            })
            .collect(),
    }))
}

#[derive(Serialize, ToSchema)]
pub struct AuthUrlOut {
    pub url: String,
}

fn slack_cfg(s: &AppState) -> ApiResult<&crate::config::SlackConfig> {
    s.config.slack.as_ref().ok_or(ApiError::NotFound)
}

fn redirect_uri(s: &AppState) -> String {
    format!("{}/integrations/slack/callback", s.config.frontend_url)
}

#[utoipa::path(get, operation_id = "getSlackAuthUrl", path = "/integrations/slack/auth-url", tag = "integrations",
    responses((status = 200, body = AuthUrlOut), (status = 404)), security(("bearer" = [])))]
async fn slack_auth_url(State(s): State<AppState>, user: AuthUser) -> ApiResult<Json<AuthUrlOut>> {
    user.require_session()?; // linking third parties is a session-only action
    let cfg = slack_cfg(&s)?;
    let state = random_token();
    sqlx::query("DELETE FROM oauth_states WHERE expires_at < now() OR (user_id = $1 AND integration = 'slack')").bind(user.id).execute(&s.db).await?;
    sqlx::query("INSERT INTO oauth_states (state_hash, user_id, integration, expires_at) VALUES ($1,$2,'slack',$3)")
        .bind(hash_token(&state)).bind(user.id).bind(Utc::now() + Duration::minutes(10)).execute(&s.db).await?;
    let q = [
        ("client_id", cfg.client_id.as_str()),
        (
            "scope",
            "chat:write,users:read,im:write,channels:read,groups:read",
        ),
        ("redirect_uri", &redirect_uri(&s)),
        ("state", &state),
    ];
    let query = q
        .iter()
        .map(|(k, v)| format!("{k}={}", urlenc(v)))
        .collect::<Vec<_>>()
        .join("&");
    Ok(Json(AuthUrlOut {
        url: format!("https://slack.com/oauth/v2/authorize?{query}"),
    }))
}

fn urlenc(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[derive(Deserialize, ToSchema)]
pub struct CallbackBody {
    pub code: String,
    pub state: String,
}

#[utoipa::path(post, operation_id = "slackCallback", path = "/integrations/slack/callback", tag = "integrations",
    request_body = CallbackBody, responses((status = 200, body = OkOut), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn slack_callback(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<CallbackBody>,
) -> ApiResult<Json<OkOut>> {
    user.require_session()?; // linking third parties is a session-only action
    let cfg = slack_cfg(&s)?;
    let key = s.config.encryption_key.ok_or(ApiError::NotFound)?;
    // Redeem the state: must exist, belong to this user, be unexpired, and is consumed (single use).
    let redeemed = sqlx::query("DELETE FROM oauth_states WHERE state_hash = $1 AND user_id = $2 AND integration = 'slack' AND expires_at > now()")
        .bind(hash_token(&b.state)).bind(user.id).execute(&s.db).await?;
    if redeemed.rows_affected() == 0 {
        return Err(ApiError::BadRequest("invalid or expired state".into()));
    }

    let res: Value = reqwest::Client::new()
        .post("https://slack.com/api/oauth.v2.access")
        .form(&[
            ("code", b.code.as_str()),
            ("client_id", cfg.client_id.as_str()),
            ("client_secret", cfg.client_secret.as_str()),
            ("redirect_uri", &redirect_uri(&s)),
        ])
        .send()
        .await
        .map_err(|e| ApiError::Internal(Box::new(e)))?
        .json()
        .await
        .map_err(|e| ApiError::Internal(Box::new(e)))?;
    if res["ok"] != json!(true) {
        return Err(ApiError::BadRequest(
            "Slack refused the authorization code".into(),
        ));
    }
    let token = res["access_token"]
        .as_str()
        .ok_or_else(|| ApiError::BadRequest("Slack returned no token".into()))?;
    let slack_user = res["authed_user"]["id"].as_str().unwrap_or_default();
    let team = res["team"]["name"].as_str().unwrap_or_default();

    let secret = encrypt(
        &key,
        user.id.as_bytes(),
        json!({"access_token": token, "slack_user_id": slack_user})
            .to_string()
            .as_bytes(),
    );
    sqlx::query("INSERT INTO user_integrations (id, user_id, type, secret, public_config) VALUES ($1,$2,'slack',$3,$4)
                 ON CONFLICT (user_id, type) DO UPDATE SET secret = EXCLUDED.secret, public_config = EXCLUDED.public_config")
        .bind(Uuid::new_v4()).bind(user.id).bind(secret).bind(SqlJson(json!({"team": team}))).execute(&s.db).await?;
    Ok(Json(OkOut { ok: true }))
}

#[utoipa::path(delete, operation_id = "slackDisconnect", path = "/integrations/slack", tag = "integrations",
    responses((status = 200, body = OkOut)), security(("bearer" = [])))]
async fn slack_disconnect(State(s): State<AppState>, user: AuthUser) -> ApiResult<Json<OkOut>> {
    user.require_session()?; // linking third parties is a session-only action
    sqlx::query("DELETE FROM user_integrations WHERE user_id = $1 AND type = 'slack'")
        .bind(user.id)
        .execute(&s.db)
        .await?;
    Ok(Json(OkOut { ok: true }))
}

async fn slack_token(s: &AppState, user: Uuid) -> Option<(String, String)> {
    let key = s.config.encryption_key?;
    s.config.slack.as_ref()?;
    let row: Option<(Vec<u8>,)> = sqlx::query_as(
        "SELECT secret FROM user_integrations WHERE user_id = $1 AND type = 'slack'",
    )
    .bind(user)
    .fetch_optional(&s.db)
    .await
    .ok()?;
    let plain = decrypt(&key, user.as_bytes(), &row?.0)?;
    let v: Value = serde_json::from_slice(&plain).ok()?;
    Some((
        v["access_token"].as_str()?.to_owned(),
        v["slack_user_id"].as_str().unwrap_or_default().to_owned(),
    ))
}

#[derive(Serialize, ToSchema)]
pub struct ChannelOut {
    pub id: String,
    pub name: String,
}

#[utoipa::path(get, operation_id = "listSlackChannels", path = "/integrations/slack/channels", tag = "integrations",
    responses((status = 200, body = Vec<ChannelOut>), (status = 404)), security(("bearer" = [])))]
async fn slack_channels(
    State(s): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<ChannelOut>>> {
    let (token, _) = slack_token(&s, user.id).await.ok_or(ApiError::NotFound)?;
    let res: Value = reqwest::Client::new()
        .get("https://slack.com/api/conversations.list")
        .query(&[
            ("types", "public_channel,private_channel"),
            ("exclude_archived", "true"),
            ("limit", "200"),
        ])
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| ApiError::Internal(Box::new(e)))?
        .json()
        .await
        .map_err(|e| ApiError::Internal(Box::new(e)))?;
    let list = res["channels"].as_array().cloned().unwrap_or_default();
    Ok(Json(
        list.iter()
            .filter_map(|c| {
                Some(ChannelOut {
                    id: c["id"].as_str()?.to_owned(),
                    name: c["name"].as_str()?.to_owned(),
                })
            })
            .collect(),
    ))
}

/// Best effort: never fails the caller. Sends a DM and/or a channel message depending on the
/// user's Slack notification preference.
pub async fn notify_slack(s: &AppState, user: Uuid, message: &str, url: &str) {
    let pref: Option<(bool, SqlJson<Value>)> = sqlx::query_as("SELECT enabled, config FROM notification_preferences WHERE user_id = $1 AND channel = 'slack'").bind(user).fetch_optional(&s.db).await.ok().flatten();
    let Some((true, SqlJson(cfg))) = pref else {
        return;
    };
    let Some((token, slack_user)) = slack_token(s, user).await else {
        return;
    };
    let text = format!("{message}\n<{url}|Open in work>");
    let client = reqwest::Client::new();
    let mut targets: Vec<String> = vec![];
    if cfg["isDmEnabled"].as_bool().unwrap_or(false) && !slack_user.is_empty() {
        targets.push(slack_user);
    }
    if let Some(ch) = cfg["channelId"].as_str().filter(|c| !c.is_empty()) {
        targets.push(ch.to_owned());
    }
    for t in targets {
        let r = client
            .post("https://slack.com/api/chat.postMessage")
            .bearer_auth(&token)
            .json(&json!({"channel": t, "text": text}))
            .send()
            .await;
        if let Err(e) = r {
            tracing::warn!(error = %e, "slack notification failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7u8; 32];

    #[test]
    fn encryption_roundtrip_binds_to_user() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let ct = encrypt(&KEY, a.as_bytes(), b"xoxb-secret-token");
        assert!(!String::from_utf8_lossy(&ct).contains("xoxb"));
        assert_eq!(
            decrypt(&KEY, a.as_bytes(), &ct).unwrap(),
            b"xoxb-secret-token"
        );
        // Another user's row cannot be decrypted with the wrong context, nor with a wrong key, nor tampered.
        assert!(decrypt(&KEY, b.as_bytes(), &ct).is_none());
        assert!(decrypt(&[8u8; 32], a.as_bytes(), &ct).is_none());
        let mut bad = ct.clone();
        *bad.last_mut().unwrap() ^= 1;
        assert!(decrypt(&KEY, a.as_bytes(), &bad).is_none());
        // Nonces are random.
        assert_ne!(ct, encrypt(&KEY, a.as_bytes(), b"xoxb-secret-token"));
    }
}
