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
    error::{ApiError, ApiResult},
    security::auth::AuthUser,
    security::authz::{require_list_role, Role},
    state::AppState,
    util::short,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_sprints, create_sprint))
        .routes(routes!(update_sprint, delete_sprint))
        .routes(routes!(start_sprint))
        .routes(routes!(complete_sprint))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct SprintOut {
    pub id: Uuid,
    pub list_id: Uuid,
    pub name: String,
    pub goal: Option<String>,
    /// `planned`, `active` or `completed`
    pub state: String,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// Issues in the sprint.
    pub issue_count: i64,
    /// Issues in a completed stage.
    pub done_count: i64,
    /// Sum of the `story_points` field over the sprint's issues.
    pub points: f64,
}

const COLS: &str = "s.id, s.list_id, s.name, s.goal, s.state, s.starts_at, s.ends_at, s.completed_at, s.created_at,
  (SELECT count(*) FROM card_lists cl JOIN cards c ON c.id = cl.card_id AND c.deleted_at IS NULL WHERE cl.sprint_id = s.id) AS issue_count,
  (SELECT count(*) FROM card_lists cl JOIN cards c ON c.id = cl.card_id AND c.deleted_at IS NULL JOIN list_stages st ON st.id = cl.list_stage_id WHERE cl.sprint_id = s.id AND st.is_completed) AS done_count,
  COALESCE((SELECT sum(CASE WHEN jsonb_typeof(c.data -> 'story_points') = 'number' THEN (c.data ->> 'story_points')::numeric ELSE 0 END)
            FROM card_lists cl JOIN cards c ON c.id = cl.card_id AND c.deleted_at IS NULL WHERE cl.sprint_id = s.id), 0)::float8 AS points";

async fn load(s: &AppState, id: Uuid) -> ApiResult<SprintOut> {
    let sql = format!("SELECT {COLS} FROM sprints s WHERE s.id = $1");
    sqlx::query_as(&sql)
        .bind(id)
        .fetch_optional(&s.db)
        .await?
        .ok_or(ApiError::NotFound)
}

async fn sprint_list(s: &AppState, id: Uuid) -> ApiResult<(Uuid, String)> {
    let r: Option<(Uuid, String)> =
        sqlx::query_as("SELECT list_id, state FROM sprints WHERE id = $1")
            .bind(id)
            .fetch_optional(&s.db)
            .await?;
    r.ok_or(ApiError::NotFound)
}

#[utoipa::path(get, operation_id = "listSprints", path = "/lists/{id}/sprints", tag = "sprints",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<SprintOut>), (status = 404)), security(("bearer" = [])))]
async fn list_sprints(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<SprintOut>>> {
    require_list_role(&s.db, user.id, id, Role::Viewer).await?;
    let sql = format!("SELECT {COLS} FROM sprints s WHERE s.list_id = $1 ORDER BY s.created_at");
    Ok(Json(sqlx::query_as(&sql).bind(id).fetch_all(&s.db).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct SprintBody {
    pub name: Option<String>,
    pub goal: Option<String>,
}

#[utoipa::path(post, operation_id = "createSprint", path = "/lists/{id}/sprints", tag = "sprints",
    params(("id" = Uuid, Path)), request_body = SprintBody,
    responses((status = 200, body = SprintOut), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn create_sprint(
    State(s): State<AppState>,
    user: AuthUser,
    Path(list_id): Path<Uuid>,
    Json(b): Json<SprintBody>,
) -> ApiResult<Json<SprintOut>> {
    require_list_role(&s.db, user.id, list_id, Role::Member).await?;
    let (method, key): (String, String) = sqlx::query_as(
        "SELECT l.methodology, w.key_prefix FROM lists l JOIN workspaces w ON w.id = l.workspace_id WHERE l.id = $1",
    )
    .bind(list_id)
    .fetch_one(&s.db)
    .await?;
    if method != "scrum" {
        return Err(ApiError::BadRequest(
            "sprints are only available in scrum lists".into(),
        ));
    }
    if let Some(n) = &b.name {
        short(n, 100, "name")?;
    }
    if b.goal.as_deref().is_some_and(|g| g.chars().count() > 1000) {
        return Err(ApiError::BadRequest("goal too long".into()));
    }
    let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM sprints WHERE list_id = $1")
        .bind(list_id)
        .fetch_one(&s.db)
        .await?;
    let name = b
        .name
        .map(|n| n.trim().to_owned())
        .unwrap_or_else(|| format!("{key} Sprint {}", count + 1));
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO sprints (id, list_id, name, goal) VALUES ($1,$2,$3,$4)")
        .bind(id)
        .bind(list_id)
        .bind(&name)
        .bind(&b.goal)
        .execute(&s.db)
        .await?;
    Ok(Json(load(&s, id).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct SprintPatch {
    pub name: Option<String>,
    pub goal: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
}

#[utoipa::path(patch, operation_id = "updateSprint", path = "/sprints/{id}", tag = "sprints",
    params(("id" = Uuid, Path)), request_body = SprintPatch,
    responses((status = 200, body = SprintOut), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update_sprint(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<SprintPatch>,
) -> ApiResult<Json<SprintOut>> {
    let (list_id, state) = sprint_list(&s, id).await?;
    require_list_role(&s.db, user.id, list_id, Role::Member).await?;
    if state == "completed" {
        return Err(ApiError::Conflict);
    }
    if let Some(n) = &b.name {
        short(n, 100, "name")?;
    }
    if let (Some(a), Some(z)) = (b.starts_at, b.ends_at) {
        if z <= a {
            return Err(ApiError::BadRequest(
                "ends_at must be after starts_at".into(),
            ));
        }
    }
    sqlx::query("UPDATE sprints SET name = COALESCE($2,name), goal = COALESCE($3,goal), starts_at = COALESCE($4,starts_at), ends_at = COALESCE($5,ends_at) WHERE id = $1")
        .bind(id)
        .bind(b.name.as_deref().map(str::trim))
        .bind(&b.goal)
        .bind(b.starts_at)
        .bind(b.ends_at)
        .execute(&s.db)
        .await?;
    Ok(Json(load(&s, id).await?))
}

#[utoipa::path(delete, operation_id = "deleteSprint", path = "/sprints/{id}", tag = "sprints",
    params(("id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 403), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn delete_sprint(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    let (list_id, state) = sprint_list(&s, id).await?;
    require_list_role(&s.db, user.id, list_id, Role::Member).await?;
    if state != "planned" {
        return Err(ApiError::Conflict); // finish an active sprint instead, completed ones are history
    }
    sqlx::query("DELETE FROM sprints WHERE id = $1")
        .bind(id)
        .execute(&s.db)
        .await?; // issues return to the backlog
    Ok(Json(OkOut { ok: true }))
}

#[derive(Deserialize, ToSchema)]
pub struct StartBody {
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
}

#[utoipa::path(post, operation_id = "startSprint", path = "/sprints/{id}/start", tag = "sprints",
    params(("id" = Uuid, Path)), request_body = StartBody,
    responses((status = 200, body = SprintOut), (status = 400), (status = 403), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn start_sprint(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<StartBody>,
) -> ApiResult<Json<SprintOut>> {
    let (list_id, state) = sprint_list(&s, id).await?;
    require_list_role(&s.db, user.id, list_id, Role::Member).await?;
    if state != "planned" {
        return Err(ApiError::Conflict);
    }
    let start = b.starts_at.unwrap_or_else(Utc::now);
    let end = b.ends_at.unwrap_or(start + Duration::days(14));
    if end <= start {
        return Err(ApiError::BadRequest(
            "ends_at must be after starts_at".into(),
        ));
    }
    // The partial unique index allows one active sprint per list; a violation maps to 409.
    sqlx::query("UPDATE sprints SET state = 'active', starts_at = $2, ends_at = $3 WHERE id = $1")
        .bind(id)
        .bind(start)
        .bind(end)
        .execute(&s.db)
        .await?;
    Ok(Json(load(&s, id).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct CompleteBody {
    /// Where unfinished issues go: another sprint of this list, or `null` for the backlog.
    pub move_to: Option<Uuid>,
}

#[utoipa::path(post, operation_id = "completeSprint", path = "/sprints/{id}/complete", tag = "sprints",
    params(("id" = Uuid, Path)), request_body = CompleteBody,
    responses((status = 200, body = SprintOut), (status = 400), (status = 403), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn complete_sprint(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<CompleteBody>,
) -> ApiResult<Json<SprintOut>> {
    let (list_id, state) = sprint_list(&s, id).await?;
    require_list_role(&s.db, user.id, list_id, Role::Member).await?;
    if state != "active" {
        return Err(ApiError::Conflict);
    }
    if let Some(target) = b.move_to {
        let ok: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM sprints WHERE id = $1 AND list_id = $2 AND state = 'planned'",
        )
        .bind(target)
        .bind(list_id)
        .fetch_optional(&s.db)
        .await?;
        if ok.is_none() {
            return Err(ApiError::BadRequest(
                "move_to must be a planned sprint of this list".into(),
            ));
        }
    }
    let mut tx = s.db.begin().await?;
    sqlx::query(
        "UPDATE card_lists SET sprint_id = $2 WHERE sprint_id = $1
           AND list_stage_id IN (SELECT id FROM list_stages WHERE list_id = $3 AND NOT is_completed)",
    )
    .bind(id)
    .bind(b.move_to)
    .bind(list_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE sprints SET state = 'completed', completed_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(load(&s, id).await?))
}
