use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::OkOut;
use crate::{
    auth::{hash_token, new_api_token, AuthUser},
    error::{ApiError, ApiResult},
    state::AppState,
    util::short,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_tokens, create_token))
        .routes(routes!(revoke_token))
}

const MAX_ACTIVE: i64 = 25;

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct ApiTokenOut {
    pub id: Uuid,
    pub name: String,
    /// First characters of the token, to recognise it. The full token is shown only once.
    pub token_prefix: String,
    /// `read` (GET only) or `write`
    pub scope: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

const COLS: &str = "id, name, token_prefix, scope, expires_at, last_used_at, created_at";

/// Managing tokens needs a real session, so a leaked token cannot mint more tokens.
#[utoipa::path(get, operation_id = "listApiTokens", path = "/me/api-tokens", tag = "api-tokens",
    responses((status = 200, body = Vec<ApiTokenOut>), (status = 403)), security(("bearer" = [])))]
async fn list_tokens(
    State(s): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<ApiTokenOut>>> {
    user.require_session()?;
    let sql = format!("SELECT {COLS} FROM api_tokens WHERE user_id = $1 AND revoked_at IS NULL ORDER BY created_at DESC");
    Ok(Json(
        sqlx::query_as(&sql).bind(user.id).fetch_all(&s.db).await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct CreateTokenBody {
    pub name: String,
    /// `read` or `write`
    pub scope: String,
    /// 1-365 days; omit for a token that never expires.
    pub expires_in_days: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct ApiTokenCreated {
    #[serde(flatten)]
    pub info: ApiTokenOut,
    /// The secret. It is not stored and cannot be shown again.
    pub token: String,
}

#[utoipa::path(post, operation_id = "createApiToken", path = "/me/api-tokens", tag = "api-tokens",
    request_body = CreateTokenBody,
    responses((status = 200, body = ApiTokenCreated), (status = 400), (status = 403), (status = 409)), security(("bearer" = [])))]
async fn create_token(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<CreateTokenBody>,
) -> ApiResult<Json<ApiTokenCreated>> {
    user.require_session()?;
    short(&b.name, 100, "name")?;
    if !matches!(b.scope.as_str(), "read" | "write") {
        return Err(ApiError::BadRequest("scope must be read or write".into()));
    }
    if b.expires_in_days.is_some_and(|d| !(1..=365).contains(&d)) {
        return Err(ApiError::BadRequest("expires_in_days must be 1-365".into()));
    }
    let (active,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM api_tokens WHERE user_id = $1 AND revoked_at IS NULL")
            .bind(user.id)
            .fetch_one(&s.db)
            .await?;
    if active >= MAX_ACTIVE {
        return Err(ApiError::Conflict);
    }
    let (token, prefix) = new_api_token();
    let sql = format!(
        "INSERT INTO api_tokens (id, user_id, name, token_hash, token_prefix, scope, expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING {COLS}"
    );
    let info: ApiTokenOut = sqlx::query_as(&sql)
        .bind(Uuid::new_v4())
        .bind(user.id)
        .bind(b.name.trim())
        .bind(hash_token(&token))
        .bind(prefix)
        .bind(&b.scope)
        .bind(b.expires_in_days.map(|d| Utc::now() + Duration::days(d)))
        .fetch_one(&s.db)
        .await?;
    Ok(Json(ApiTokenCreated { info, token }))
}

#[utoipa::path(delete, operation_id = "revokeApiToken", path = "/me/api-tokens/{id}", tag = "api-tokens",
    params(("id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn revoke_token(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    user.require_session()?;
    let r = sqlx::query("UPDATE api_tokens SET revoked_at = now() WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL").bind(id).bind(user.id).execute(&s.db).await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(Json(OkOut { ok: true }))
}
