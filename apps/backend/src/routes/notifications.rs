use std::{convert::Infallible, time::Duration};

use axum::{
    extract::{Path, Query, State},
    response::sse::{Event as SseEvent, KeepAlive, Sse},
    Json,
};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json as SqlJson;
use tokio_stream::wrappers::BroadcastStream;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::{users::UserBrief, OkOut};
use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthUser,
    security::authz::{require_card_role, require_list_role, Role},
    state::AppState,
    util::clamp_page,
};

/// Streams end after this long, so a disabled or signed-out user is cut off at the next reconnect.
const STREAM_LIFETIME: Duration = Duration::from_secs(10 * 60);

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_notifications))
        .routes(routes!(mark_all_read))
        .routes(routes!(mark_read))
        .routes(routes!(get_preferences, put_preference))
        .routes(routes!(list_watchers, add_watcher, remove_watcher))
        .routes(routes!(is_watching))
        .routes(routes!(events_stream))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct NotificationOut {
    pub id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub r#type: String,
    pub message: String,
    pub related_resource_type: Option<String>,
    pub related_resource_id: Option<Uuid>,
    pub color: Option<String>,
    pub is_read: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct NotificationQuery {
    pub workspace_id: Option<Uuid>,
    pub is_read: Option<bool>,
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

/// Always scoped to the caller: there is no way to ask for another user's notifications.
#[utoipa::path(get, operation_id = "listNotifications", path = "/notifications", tag = "notifications",
    params(("workspace_id" = Option<Uuid>, Query), ("is_read" = Option<bool>, Query), ("page" = Option<i64>, Query), ("limit" = Option<i64>, Query)),
    responses((status = 200, body = Vec<NotificationOut>)), security(("bearer" = [])))]
async fn list_notifications(
    State(s): State<AppState>,
    user: AuthUser,
    Query(q): Query<NotificationQuery>,
) -> ApiResult<Json<Vec<NotificationOut>>> {
    let (page, limit) = clamp_page(q.page, q.limit)?;
    let rows = sqlx::query_as(
        "SELECT id, workspace_id, type, message, related_resource_type, related_resource_id, color, is_read, created_at
         FROM notifications WHERE recipient_id = $1 AND ($2::uuid IS NULL OR workspace_id = $2) AND ($3::boolean IS NULL OR is_read = $3)
         ORDER BY created_at DESC LIMIT $4 OFFSET $5")
        .bind(user.id).bind(q.workspace_id).bind(q.is_read).bind(limit).bind((page - 1) * limit).fetch_all(&s.db).await?;
    Ok(Json(rows))
}

#[utoipa::path(post, operation_id = "markAllNotificationsRead", path = "/notifications/read-all", tag = "notifications",
    responses((status = 200, body = OkOut)), security(("bearer" = [])))]
async fn mark_all_read(State(s): State<AppState>, user: AuthUser) -> ApiResult<Json<OkOut>> {
    sqlx::query("UPDATE notifications SET is_read = true WHERE recipient_id = $1 AND NOT is_read")
        .bind(user.id)
        .execute(&s.db)
        .await?;
    Ok(Json(OkOut { ok: true }))
}

#[utoipa::path(post, operation_id = "markNotificationRead", path = "/notifications/{id}/read", tag = "notifications",
    params(("id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 404)), security(("bearer" = [])))]
async fn mark_read(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    let r =
        sqlx::query("UPDATE notifications SET is_read = true WHERE id = $1 AND recipient_id = $2")
            .bind(id)
            .bind(user.id)
            .execute(&s.db)
            .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(Json(OkOut { ok: true }))
}

// ---- preferences ----------------------------------------------------------------------------

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct PreferenceOut {
    pub channel: String,
    pub enabled: bool,
    #[schema(value_type = Object)]
    pub config: SqlJson<Value>,
}

#[utoipa::path(get, operation_id = "getNotificationPreferences", path = "/notification-preferences", tag = "notifications",
    responses((status = 200, body = Vec<PreferenceOut>)), security(("bearer" = [])))]
async fn get_preferences(
    State(s): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<PreferenceOut>>> {
    Ok(Json(sqlx::query_as("SELECT channel, enabled, config FROM notification_preferences WHERE user_id = $1 ORDER BY channel").bind(user.id).fetch_all(&s.db).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct PreferenceBody {
    pub enabled: bool,
    /// e.g. `{"isDmEnabled": true, "channelId": "C123"}` for Slack
    #[schema(value_type = Option<Object>)]
    pub config: Option<Value>,
}

#[utoipa::path(put, operation_id = "putNotificationPreference", path = "/notification-preferences/{channel}", tag = "notifications",
    params(("channel" = String, Path)), request_body = PreferenceBody,
    responses((status = 200, body = PreferenceOut), (status = 400)), security(("bearer" = [])))]
async fn put_preference(
    State(s): State<AppState>,
    user: AuthUser,
    Path(channel): Path<String>,
    Json(b): Json<PreferenceBody>,
) -> ApiResult<Json<PreferenceOut>> {
    if !matches!(channel.as_str(), "in_app" | "slack" | "email") {
        return Err(ApiError::BadRequest("unknown channel".into()));
    }
    let cfg = b.config.unwrap_or_else(|| serde_json::json!({}));
    if !cfg.is_object() || cfg.to_string().len() > 2048 {
        return Err(ApiError::BadRequest("config must be a small object".into()));
    }
    let row = sqlx::query_as(
        "INSERT INTO notification_preferences (user_id, channel, enabled, config) VALUES ($1,$2,$3,$4)
         ON CONFLICT (user_id, channel) DO UPDATE SET enabled = EXCLUDED.enabled, config = EXCLUDED.config
         RETURNING channel, enabled, config")
        .bind(user.id).bind(&channel).bind(b.enabled).bind(SqlJson(cfg)).fetch_one(&s.db).await?;
    Ok(Json(row))
}

// ---- watchers -------------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct WatchBody {
    /// `card` or `list`
    pub resource_type: String,
    pub resource_id: Uuid,
}

#[derive(Deserialize)]
pub struct WatchQuery {
    pub resource_type: String,
    pub resource_id: Uuid,
}

async fn check_resource(s: &AppState, user: Uuid, ty: &str, id: Uuid) -> ApiResult<()> {
    match ty {
        "card" => {
            require_card_role(&s.db, user, id, Role::Viewer).await?;
        }
        "list" => {
            require_list_role(&s.db, user, id, Role::Viewer).await?;
        }
        _ => {
            return Err(ApiError::BadRequest(
                "resource_type must be card or list".into(),
            ))
        }
    }
    Ok(())
}

#[utoipa::path(post, operation_id = "addWatcher", path = "/watchers", tag = "notifications",
    request_body = WatchBody, responses((status = 200, body = OkOut), (status = 404)), security(("bearer" = [])))]
async fn add_watcher(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<WatchBody>,
) -> ApiResult<Json<OkOut>> {
    check_resource(&s, user.id, &b.resource_type, b.resource_id).await?;
    sqlx::query("INSERT INTO watchers (id, user_id, resource_type, resource_id) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING")
        .bind(Uuid::new_v4()).bind(user.id).bind(&b.resource_type).bind(b.resource_id).execute(&s.db).await?;
    Ok(Json(OkOut { ok: true }))
}

#[utoipa::path(delete, operation_id = "removeWatcher", path = "/watchers", tag = "notifications",
    params(("resource_type" = String, Query), ("resource_id" = Uuid, Query)), responses((status = 200, body = OkOut)), security(("bearer" = [])))]
async fn remove_watcher(
    State(s): State<AppState>,
    user: AuthUser,
    Query(q): Query<WatchQuery>,
) -> ApiResult<Json<OkOut>> {
    sqlx::query(
        "DELETE FROM watchers WHERE user_id = $1 AND resource_type = $2 AND resource_id = $3",
    )
    .bind(user.id)
    .bind(&q.resource_type)
    .bind(q.resource_id)
    .execute(&s.db)
    .await?;
    Ok(Json(OkOut { ok: true }))
}

#[utoipa::path(get, operation_id = "listWatchers", path = "/watchers", tag = "notifications",
    params(("resource_type" = String, Query), ("resource_id" = Uuid, Query)), responses((status = 200, body = Vec<UserBrief>), (status = 404)), security(("bearer" = [])))]
async fn list_watchers(
    State(s): State<AppState>,
    user: AuthUser,
    Query(q): Query<WatchQuery>,
) -> ApiResult<Json<Vec<UserBrief>>> {
    check_resource(&s, user.id, &q.resource_type, q.resource_id).await?;
    let rows = sqlx::query_as(
        "SELECT u.id, u.first_name, u.last_name, u.email::text AS email FROM watchers w JOIN users u ON u.id = w.user_id
         WHERE w.resource_type = $1 AND w.resource_id = $2 AND u.deleted_at IS NULL ORDER BY u.first_name")
        .bind(&q.resource_type).bind(q.resource_id).fetch_all(&s.db).await?;
    Ok(Json(rows))
}

#[derive(Serialize, ToSchema)]
pub struct WatchingOut {
    pub watching: bool,
}

#[utoipa::path(get, operation_id = "isWatching", path = "/watchers/is-watching", tag = "notifications",
    params(("resource_type" = String, Query), ("resource_id" = Uuid, Query)), responses((status = 200, body = WatchingOut)), security(("bearer" = [])))]
async fn is_watching(
    State(s): State<AppState>,
    user: AuthUser,
    Query(q): Query<WatchQuery>,
) -> ApiResult<Json<WatchingOut>> {
    let n: (i64,) = sqlx::query_as("SELECT count(*) FROM watchers WHERE user_id = $1 AND resource_type = $2 AND resource_id = $3").bind(user.id).bind(&q.resource_type).bind(q.resource_id).fetch_one(&s.db).await?;
    Ok(Json(WatchingOut { watching: n.0 > 0 }))
}

// ---- realtime -------------------------------------------------------------------------------

/// Server-sent events for the caller's own notifications. The stream is bound to the user id in
/// the verified token; clients cannot name or join any other channel.
#[utoipa::path(get, operation_id = "streamEvents", path = "/events", tag = "notifications",
    responses((status = 200, description = "text/event-stream")), security(("bearer" = [])))]
async fn events_stream(
    State(s): State<AppState>,
    user: AuthUser,
) -> impl axum::response::IntoResponse {
    let rx = s.hub.subscribe(user.id);
    let stream = BroadcastStream::new(rx)
        .filter_map(|m| async move { m.ok() })
        .map(|m| Ok::<_, Infallible>(SseEvent::default().data(m)))
        .take_until(tokio::time::sleep(STREAM_LIFETIME));
    // `no-transform` keeps proxies (e.g. the Next.js rewrite proxy) from compressing and thereby buffering the stream.
    (
        [
            (axum::http::header::CACHE_CONTROL, "no-store, no-transform"),
            (
                axum::http::HeaderName::from_static("x-accel-buffering"),
                "no",
            ),
        ],
        Sse::new(stream).keep_alive(KeepAlive::default()),
    )
}
