use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json as SqlJson;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::OkOut;
use crate::{
    auth::AuthUser,
    authz::{
        is_project_member, require_card_role, require_role, require_workspace_role,
        visible_cards_sql, Role,
    },
    error::{ApiError, ApiResult},
    events::{emit, Event},
    state::AppState,
    util::clamp_page,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_activities, create_activity))
        .routes(routes!(update_activity, delete_activity))
        .routes(routes!(workspace_activities))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct ActivityOut {
    pub id: Uuid,
    pub card_id: Uuid,
    pub r#type: String,
    #[schema(value_type = Object)]
    pub content: SqlJson<Value>,
    pub created_by: Option<Uuid>,
    pub created_by_type: String,
    pub created_at: DateTime<Utc>,
}

const COLS: &str =
    "a.id, a.card_id, a.type, a.content, a.created_by, a.created_by_type, a.created_at";
const TYPES: &[&str] = &["comment", "email", "task", "call", "message", "meeting"];

fn bad(m: impl Into<String>) -> ApiError {
    ApiError::BadRequest(m.into())
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub r#type: Option<String>,
}

#[utoipa::path(get, operation_id = "listCardActivities", path = "/cards/{id}/activities", tag = "activities",
    params(("id" = Uuid, Path), ("type" = Option<String>, Query)),
    responses((status = 200, body = Vec<ActivityOut>), (status = 404)), security(("bearer" = [])))]
async fn list_activities(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<ActivityOut>>> {
    require_card_role(&s.db, user.id, id, Role::Viewer).await?;
    let sql = format!("SELECT {COLS} FROM card_activities a WHERE a.card_id = $1 AND ($2::text IS NULL OR a.type = $2) ORDER BY a.created_at LIMIT 1000");
    Ok(Json(
        sqlx::query_as(&sql)
            .bind(id)
            .bind(q.r#type)
            .fetch_all(&s.db)
            .await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct ActivityBody {
    /// `comment`, `email`, `task`, `call`, `message` or `meeting` (`update` entries are written by the system)
    pub r#type: String,
    #[schema(value_type = Object)]
    pub content: Value,
    /// Users to notify, for comments. Only members of the project are notified.
    pub mentions: Option<Vec<Uuid>>,
}

async fn check_content(s: &AppState, project_id: Uuid, ty: &str, content: &Value) -> ApiResult<()> {
    let obj = content
        .as_object()
        .ok_or_else(|| bad("content must be an object"))?;
    if content.to_string().len() > 64 * 1024 {
        return Err(bad("content too large"));
    }
    match ty {
        "comment" => {
            let ok = matches!(
                obj.get("content"),
                Some(Value::Object(_)) | Some(Value::String(_))
            );
            if !ok {
                return Err(bad("comment needs `content`"));
            }
        }
        "task" => {
            let title_ok = obj
                .get("title")
                .and_then(Value::as_str)
                .is_some_and(|t| !t.trim().is_empty() && t.chars().count() <= 300);
            if !title_ok {
                return Err(bad("task needs a title (max 300 characters)"));
            }
            if let Some(st) = obj.get("status") {
                if !st
                    .as_str()
                    .is_some_and(|x| matches!(x, "pending" | "in_progress" | "completed"))
                {
                    return Err(bad("invalid task status"));
                }
            }
            if obj.get("isCompleted").is_some_and(|v| !v.is_boolean()) {
                return Err(bad("isCompleted must be a boolean"));
            }
            if let Some(d) = obj.get("dueAt") {
                if !d
                    .as_str()
                    .is_some_and(|x| DateTime::parse_from_rfc3339(x).is_ok())
                {
                    return Err(bad("dueAt must be an RFC 3339 timestamp"));
                }
            }
            if let Some(a) = obj.get("assignee") {
                let ids: Vec<Uuid> = a
                    .as_array()
                    .ok_or_else(|| bad("assignee must be a list"))?
                    .iter()
                    .map(|x| {
                        x.as_str()
                            .and_then(|x| Uuid::parse_str(x).ok())
                            .ok_or_else(|| bad("invalid assignee id"))
                    })
                    .collect::<ApiResult<_>>()?;
                for u in ids {
                    if !is_project_member(&s.db, project_id, u).await? {
                        return Err(bad("assignee is not a member of this project"));
                    }
                }
            }
        }
        "email" => {
            let ok = obj.get("body").and_then(Value::as_str).is_some()
                && obj.get("to").and_then(Value::as_str).is_some();
            if !ok {
                return Err(bad("email needs `to` and `body`"));
            }
        }
        _ => {}
    }
    Ok(())
}

#[utoipa::path(post, operation_id = "createCardActivity", path = "/cards/{id}/activities", tag = "activities",
    params(("id" = Uuid, Path)), request_body = ActivityBody,
    responses((status = 200, body = ActivityOut), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn create_activity(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<ActivityBody>,
) -> ApiResult<Json<ActivityOut>> {
    let scope = require_card_role(&s.db, user.id, id, Role::Member).await?;
    if !TYPES.contains(&b.r#type.as_str()) {
        return Err(bad("unknown activity type"));
    }
    check_content(&s, scope.project_id, &b.r#type, &b.content).await?;
    let aid = Uuid::new_v4();
    let sql = "INSERT INTO card_activities (id, card_id, type, content, created_by) VALUES ($1,$2,$3,$4,$5) RETURNING id, card_id, type, content, created_by, created_by_type, created_at";
    let row: ActivityOut = sqlx::query_as(sql)
        .bind(aid)
        .bind(id)
        .bind(&b.r#type)
        .bind(SqlJson(b.content))
        .bind(user.id)
        .fetch_one(&s.db)
        .await?;
    if b.r#type == "comment" {
        let mut mentions = vec![];
        for m in b.mentions.unwrap_or_default().into_iter().take(20) {
            if is_project_member(&s.db, scope.project_id, m).await? {
                mentions.push(m);
            }
        }
        emit(
            &s,
            Event::CommentCreated {
                card_id: id,
                workspace_id: scope.workspace_id,
                activity_id: aid,
                actor: Some(user.id),
                mentions,
                depth: 0,
            },
        );
    }
    Ok(Json(row))
}

async fn writable_activity(
    s: &AppState,
    user: &AuthUser,
    activity_id: Uuid,
) -> ApiResult<(Uuid, String, crate::authz::Scope)> {
    let row: Option<(Uuid, String, Option<Uuid>)> =
        sqlx::query_as("SELECT card_id, type, created_by FROM card_activities WHERE id = $1")
            .bind(activity_id)
            .fetch_optional(&s.db)
            .await?;
    let (card_id, ty, creator) = row.ok_or(ApiError::NotFound)?;
    let scope = require_card_role(&s.db, user.id, card_id, Role::Member).await?;
    if ty == "update" {
        return Err(ApiError::Forbidden);
    } // system history is immutable
      // Authors can edit their own entries; project admins can moderate.
    if creator != Some(user.id)
        && require_role(&s.db, user.id, scope.project_id, Role::Admin)
            .await
            .is_err()
    {
        return Err(ApiError::Forbidden);
    }
    Ok((card_id, ty, scope))
}

#[derive(Deserialize, ToSchema)]
pub struct ActivityPatch {
    #[schema(value_type = Object)]
    pub content: Value,
}

#[utoipa::path(patch, operation_id = "updateActivity", path = "/activities/{id}", tag = "activities",
    params(("id" = Uuid, Path)), request_body = ActivityPatch,
    responses((status = 200, body = ActivityOut), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update_activity(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<ActivityPatch>,
) -> ApiResult<Json<ActivityOut>> {
    let (_, ty, scope) = writable_activity(&s, &user, id).await?;
    check_content(&s, scope.project_id, &ty, &b.content).await?;
    let row = sqlx::query_as("UPDATE card_activities SET content = $2 WHERE id = $1 RETURNING id, card_id, type, content, created_by, created_by_type, created_at")
        .bind(id).bind(SqlJson(b.content)).fetch_one(&s.db).await?;
    Ok(Json(row))
}

#[utoipa::path(delete, operation_id = "deleteActivity", path = "/activities/{id}", tag = "activities",
    params(("id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn delete_activity(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    writable_activity(&s, &user, id).await?;
    sqlx::query("DELETE FROM card_activities WHERE id = $1")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(Json(OkOut { ok: true }))
}

#[derive(Deserialize)]
pub struct WsQuery {
    pub r#type: Option<String>,
    pub assignee: Option<Uuid>,
    pub completed: Option<bool>,
    pub due_start: Option<DateTime<Utc>>,
    pub due_end: Option<DateTime<Utc>>,
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

#[utoipa::path(get, operation_id = "listWorkspaceActivities", path = "/workspaces/{id}/activities", tag = "activities",
    params(("id" = Uuid, Path), ("type" = Option<String>, Query), ("assignee" = Option<Uuid>, Query), ("completed" = Option<bool>, Query),
           ("due_start" = Option<DateTime<Utc>>, Query), ("due_end" = Option<DateTime<Utc>>, Query), ("page" = Option<i64>, Query), ("limit" = Option<i64>, Query)),
    responses((status = 200, body = Vec<ActivityOut>), (status = 404)), security(("bearer" = [])))]
async fn workspace_activities(
    State(s): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
    Query(q): Query<WsQuery>,
) -> ApiResult<Json<Vec<ActivityOut>>> {
    let project_id = require_workspace_role(&s.db, user.id, wid, Role::Viewer).await?;
    let role = require_role(&s.db, user.id, project_id, Role::Viewer).await?;
    let (page, limit) = clamp_page(q.page, q.limit)?;
    let vis = visible_cards_sql("c", 9, 10);
    let sql = format!(
        "SELECT {COLS} FROM card_activities a JOIN cards c ON c.id = a.card_id
         WHERE c.workspace_id = $1 AND c.deleted_at IS NULL AND a.type <> 'update'
           AND ($2::text IS NULL OR a.type = $2)
           AND ($3::text IS NULL OR a.content -> 'assignee' ? $3)
           AND ($4::boolean IS NULL OR COALESCE((a.content ->> 'isCompleted')::boolean, false) = $4)
           AND ($5::timestamptz IS NULL OR (a.content ->> 'dueAt')::timestamptz >= $5)
           AND ($6::timestamptz IS NULL OR (a.content ->> 'dueAt')::timestamptz <= $6)
           AND {vis}
         ORDER BY a.created_at DESC LIMIT $7 OFFSET $8"
    );
    let rows = sqlx::query_as(&sql)
        .bind(wid)
        .bind(q.r#type)
        .bind(q.assignee.map(|u| u.to_string()))
        .bind(q.completed)
        .bind(q.due_start)
        .bind(q.due_end)
        .bind(limit)
        .bind((page - 1) * limit)
        .bind(role >= Role::Admin)
        .bind(user.id)
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows))
}
