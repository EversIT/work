use super::OkOut;
use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;
use validator::Validate;

use crate::{
    auth::{hash_token, random_token, AuthUser},
    authz::{require_role, Role},
    error::{ApiError, ApiResult},
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list, create))
        .routes(routes!(get_one, remove))
        .routes(routes!(members))
        .routes(routes!(set_member_role, remove_member))
        .routes(routes!(create_invite))
        .routes(routes!(accept_invite))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
struct ProjectOut {
    id: Uuid,
    name: String,
    role: String,
    created_at: DateTime<Utc>,
}

#[utoipa::path(operation_id="listProjects", get, path="/projects", tag="projects", responses((status=200, body=Vec<ProjectOut>)), security(("bearer" = [])))]
async fn list(State(s): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<ProjectOut>>> {
    let rows = sqlx::query_as(
        "SELECT p.id, p.name, m.role, p.created_at FROM projects p
         JOIN project_members m ON m.project_id = p.id WHERE m.user_id = $1 ORDER BY p.created_at",
    )
    .bind(user.id)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

#[derive(Deserialize, Validate, ToSchema)]
pub struct NameBody {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
}

#[utoipa::path(operation_id="createProject", post, path="/projects", tag="projects", request_body = NameBody, responses((status=200, body=ProjectOut)), security(("bearer" = [])))]
async fn create(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<NameBody>,
) -> ApiResult<Json<ProjectOut>> {
    b.validate()?;
    let id = Uuid::new_v4();
    let mut tx = s.db.begin().await?;
    sqlx::query("INSERT INTO projects (id, name, created_by) VALUES ($1,$2,$3)")
        .bind(id)
        .bind(b.name.trim())
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES ($1,$2,'owner')")
        .bind(id)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    get_one(State(s), user, Path(id)).await
}

#[utoipa::path(operation_id="getProject", get, path="/projects/{id}", tag="projects", params(("id"=Uuid,Path)), responses((status=200, body=ProjectOut), (status=404)), security(("bearer" = [])))]
async fn get_one(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ProjectOut>> {
    require_role(&s.db, user.id, id, Role::Viewer).await?;
    let row = sqlx::query_as(
        "SELECT p.id, p.name, m.role, p.created_at FROM projects p
         JOIN project_members m ON m.project_id = p.id AND m.user_id = $2 WHERE p.id = $1",
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&s.db)
    .await?;
    Ok(Json(row))
}

#[utoipa::path(operation_id="deleteProject", delete, path="/projects/{id}", tag="projects", params(("id"=Uuid,Path)), responses((status=200, body=OkOut), (status=403), (status=404)), security(("bearer" = [])))]
async fn remove(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    require_role(&s.db, user.id, id, Role::Owner).await?;
    sqlx::query("DELETE FROM projects WHERE id = $1")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(Json(OkOut { ok: true }))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
struct MemberOut {
    user_id: Uuid,
    first_name: String,
    last_name: String,
    email: String,
    role: String,
}

#[utoipa::path(operation_id="listProjectMembers", get, path="/projects/{id}/members", tag="projects", params(("id"=Uuid,Path)), responses((status=200, body=Vec<MemberOut>), (status=404)), security(("bearer" = [])))]
async fn members(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<MemberOut>>> {
    require_role(&s.db, user.id, id, Role::Viewer).await?;
    let rows = sqlx::query_as(
        "SELECT u.id AS user_id, u.first_name, u.last_name, u.email::text AS email, m.role
         FROM project_members m JOIN users u ON u.id = m.user_id
         WHERE m.project_id = $1 AND u.deleted_at IS NULL ORDER BY u.first_name",
    )
    .bind(id)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(rows))
}

async fn member_role(s: &AppState, project: Uuid, user: Uuid) -> ApiResult<Role> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT role FROM project_members WHERE project_id = $1 AND user_id = $2")
            .bind(project)
            .bind(user)
            .fetch_optional(&s.db)
            .await?;
    row.and_then(|(r,)| Role::parse(&r))
        .ok_or(ApiError::NotFound)
}

#[derive(Deserialize, ToSchema)]
struct RoleBody {
    role: String,
}

/// Rule: you can only manage members ranked strictly below you, and only grant roles
/// strictly below your own. Ownership is never granted or removed through this API.
#[utoipa::path(operation_id="setProjectMemberRole", patch, path="/projects/{id}/members/{user_id}", tag="projects", params(("id"=Uuid,Path),("user_id"=Uuid,Path)), request_body = RoleBody, responses((status=200, body=OkOut), (status=403), (status=404)), security(("bearer" = [])))]
async fn set_member_role(
    State(s): State<AppState>,
    user: AuthUser,
    Path((id, target)): Path<(Uuid, Uuid)>,
    Json(b): Json<RoleBody>,
) -> ApiResult<Json<OkOut>> {
    let actor = require_role(&s.db, user.id, id, Role::Admin).await?;
    let new_role =
        Role::parse(&b.role).ok_or_else(|| ApiError::BadRequest("unknown role".into()))?;
    let current = member_role(&s, id, target).await?;
    if current >= actor || new_role >= actor {
        return Err(ApiError::Forbidden);
    }
    sqlx::query("UPDATE project_members SET role = $3 WHERE project_id = $1 AND user_id = $2")
        .bind(id)
        .bind(target)
        .bind(new_role.as_str())
        .execute(&s.db)
        .await?;
    Ok(Json(OkOut { ok: true }))
}

#[utoipa::path(operation_id="removeProjectMember", delete, path="/projects/{id}/members/{user_id}", tag="projects", params(("id"=Uuid,Path),("user_id"=Uuid,Path)), responses((status=200, body=OkOut), (status=403), (status=404)), security(("bearer" = [])))]
async fn remove_member(
    State(s): State<AppState>,
    user: AuthUser,
    Path((id, target)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<OkOut>> {
    let actor = require_role(&s.db, user.id, id, Role::Viewer).await?;
    let current = member_role(&s, id, target).await?;
    if current == Role::Owner {
        return Err(ApiError::Forbidden); // an owner cannot be removed or leave: transfer/delete first
    }
    let allowed = target == user.id || (actor >= Role::Admin && current < actor);
    if !allowed {
        return Err(ApiError::Forbidden);
    }
    sqlx::query("DELETE FROM project_members WHERE project_id = $1 AND user_id = $2")
        .bind(id)
        .bind(target)
        .execute(&s.db)
        .await?;
    Ok(Json(OkOut { ok: true }))
}

#[derive(Deserialize, ToSchema)]
struct InviteBody {
    role: String,
    #[serde(default = "default_hours")]
    expires_in_hours: i64,
    #[serde(default = "default_uses")]
    max_uses: i32,
}
fn default_hours() -> i64 {
    72
}
fn default_uses() -> i32 {
    1
}

#[derive(Serialize, ToSchema)]
struct InviteOut {
    /// Shown exactly once; only its hash is stored.
    token: String,
    expires_at: DateTime<Utc>,
}

#[utoipa::path(operation_id="createProjectInvite", post, path="/projects/{id}/invites", tag="projects", params(("id"=Uuid,Path)), request_body = InviteBody, responses((status=200, body=InviteOut), (status=403), (status=404)), security(("bearer" = [])))]
async fn create_invite(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<InviteBody>,
) -> ApiResult<Json<InviteOut>> {
    let actor = require_role(&s.db, user.id, id, Role::Admin).await?;
    let role = Role::parse(&b.role).ok_or_else(|| ApiError::BadRequest("unknown role".into()))?;
    if role >= actor {
        return Err(ApiError::Forbidden);
    }
    if !(1..=24 * 30).contains(&b.expires_in_hours) || !(1..=100).contains(&b.max_uses) {
        return Err(ApiError::BadRequest(
            "expires_in_hours 1-720, max_uses 1-100".into(),
        ));
    }
    let token = random_token();
    let expires_at = Utc::now() + Duration::hours(b.expires_in_hours);
    sqlx::query(
        "INSERT INTO invites (id, project_id, token_hash, role, expires_at, max_uses, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(Uuid::new_v4())
    .bind(id)
    .bind(hash_token(&token))
    .bind(role.as_str())
    .bind(expires_at)
    .bind(b.max_uses)
    .bind(user.id)
    .execute(&s.db)
    .await?;
    Ok(Json(InviteOut { token, expires_at }))
}

#[derive(Serialize, ToSchema)]
struct AcceptOut {
    project_id: Uuid,
}

#[derive(Deserialize, ToSchema)]
struct AcceptBody {
    token: String,
}

#[utoipa::path(operation_id="acceptInvite", post, path="/invites/accept", tag="projects", request_body = AcceptBody, responses((status=200, body=AcceptOut), (status=404), (status=429)), security(("bearer" = [])))]
async fn accept_invite(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<AcceptBody>,
) -> ApiResult<Json<AcceptOut>> {
    // Guessing is infeasible (256-bit secrets) but cap attempts per user anyway.
    if !s.auth_limiter.check(&format!("invite:{}", user.id)) {
        return Err(ApiError::TooManyRequests);
    }
    let mut tx = s.db.begin().await?;
    // Atomic claim: expiry and use-count are checked and consumed in one statement.
    let claimed: Option<(Uuid, String)> = sqlx::query_as(
        "UPDATE invites SET uses = uses + 1
         WHERE token_hash = $1 AND expires_at > now() AND uses < max_uses
           AND (email IS NULL OR email = (SELECT u.email FROM users u WHERE u.id = $2))
         RETURNING project_id, role",
    )
    .bind(hash_token(&b.token))
    .bind(user.id)
    .fetch_optional(&mut *tx)
    .await?;
    let (project_id, role) = claimed.ok_or(ApiError::NotFound)?; // generic: unknown/expired/used up
    sqlx::query(
        "INSERT INTO project_members (project_id, user_id, role) VALUES ($1,$2,$3)
         ON CONFLICT (project_id, user_id) DO NOTHING",
    )
    .bind(project_id)
    .bind(user.id)
    .bind(role)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(AcceptOut { project_id }))
}
