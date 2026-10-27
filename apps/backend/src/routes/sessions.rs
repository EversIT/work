use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::OkOut;
use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthUser,
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_sessions, revoke_other_sessions))
        .routes(routes!(revoke_session))
}

/// One sign-in. Refresh tokens rotate, so a sign-in is a token family: it is identified by the
/// family id, which stays the same for as long as the sign-in lives.
#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct SessionOut {
    pub id: Uuid,
    /// This is the session making the request.
    pub current: bool,
    /// Browser / device, as reported when the session was last used.
    pub user_agent: Option<String>,
    pub ip: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

async fn current_family(s: &AppState, user: &AuthUser) -> ApiResult<Uuid> {
    let sid = user.require_session()?;
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT family_id FROM sessions WHERE id = $1 AND user_id = $2")
            .bind(sid)
            .bind(user.id)
            .fetch_optional(&s.db)
            .await?;
    row.map(|r| r.0).ok_or(ApiError::Unauthorized)
}

/// The caller's live sign-ins, most recently active first. Needs a real session, so an API
/// token cannot enumerate them.
#[utoipa::path(get, operation_id = "listSessions", path = "/me/sessions", tag = "auth",
    responses((status = 200, body = Vec<SessionOut>), (status = 403)), security(("bearer" = [])))]
async fn list_sessions(
    State(s): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<SessionOut>>> {
    let family = current_family(&s, &user).await?;
    // Only the newest token of a family is unrevoked, so this yields one row per sign-in.
    let rows = sqlx::query_as(
        "SELECT s.family_id AS id, s.family_id = $2 AS current, s.user_agent, s.ip,
                f.started AS created_at, s.created_at AS last_active_at, s.expires_at
         FROM sessions s
         JOIN (SELECT family_id, min(created_at) AS started FROM sessions WHERE user_id = $1 GROUP BY family_id) f
           ON f.family_id = s.family_id
         WHERE s.user_id = $1 AND s.revoked_at IS NULL AND s.expires_at > now()
         ORDER BY current DESC, s.created_at DESC",
    )
    .bind(user.id)
    .bind(family)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

/// Signs out one device. Its access token stops working immediately because the session is
/// re-checked on every request.
#[utoipa::path(delete, operation_id = "revokeSession", path = "/me/sessions/{id}", tag = "auth",
    params(("id" = Uuid, Path)),
    responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn revoke_session(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    user.require_session()?;
    let r = sqlx::query(
        "UPDATE sessions SET revoked_at = now() WHERE family_id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(user.id)
    .execute(&s.db)
    .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(Json(OkOut { ok: true }))
}

/// "Sign out everywhere else": every sign-in except the one making the request.
#[utoipa::path(delete, operation_id = "revokeOtherSessions", path = "/me/sessions", tag = "auth",
    responses((status = 200, body = OkOut), (status = 403)), security(("bearer" = [])))]
async fn revoke_other_sessions(
    State(s): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<OkOut>> {
    let family = current_family(&s, &user).await?;
    sqlx::query(
        "UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND family_id <> $2 AND revoked_at IS NULL",
    )
    .bind(user.id)
    .bind(family)
    .execute(&s.db)
    .await?;
    Ok(Json(OkOut { ok: true }))
}
