use super::{spaces::check_grant_role, OkOut};
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
    error::{ApiError, ApiResult},
    security::auth::{hash_token, random_token, AuthUser},
    security::authz::{list_access, require_membership, require_role, Role, Scope},
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list, create))
        .routes(routes!(get_one, remove))
        .routes(routes!(members))
        .routes(routes!(set_member_role, remove_member))
        .routes(routes!(create_invite))
        .routes(routes!(create_list_invite))
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
    /// Limited access: sees only the lists and spaces they were explicitly added to.
    restricted: bool,
}

#[utoipa::path(operation_id="listProjectMembers", get, path="/projects/{id}/members", tag="projects", params(("id"=Uuid,Path)), responses((status=200, body=Vec<MemberOut>), (status=404)), security(("bearer" = [])))]
async fn members(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<MemberOut>>> {
    let (_, restricted) = require_membership(&s.db, user.id, id, Role::Viewer).await?;
    // A restricted caller sees themselves, the project's admins and the people who share a list
    // with them (directly or through a space grant), not the whole organisation.
    let sql = format!(
        "SELECT u.id AS user_id, u.first_name, u.last_name, u.email::text AS email, m.role, m.restricted
         FROM project_members m JOIN users u ON u.id = m.user_id
         WHERE m.project_id = $1 AND u.deleted_at IS NULL
           AND (NOT $2 OR m.user_id = $3 OR m.role IN ('admin','owner') OR EXISTS (
                SELECT 1 FROM ({mine}) a JOIN ({theirs}) b ON a.id = b.id))
         ORDER BY u.first_name",
        mine = reachable_lists("$3"),
        theirs = reachable_lists("m.user_id"),
    );
    let rows = sqlx::query_as(&sql)
        .bind(id)
        .bind(restricted)
        .bind(user.id)
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows))
}

/// SQL for the ids of the lists a user holds a grant on, directly or through their space.
fn reachable_lists(user: &str) -> String {
    format!(
        "SELECT g.resource_id AS id FROM resource_grants g WHERE g.user_id = {user} AND g.resource_type = 'list'
         UNION SELECT l.id FROM resource_grants g JOIN lists l ON l.space_id = g.resource_id AND l.deleted_at IS NULL
         WHERE g.user_id = {user} AND g.resource_type = 'space'"
    )
}

async fn member_role(s: &AppState, project: Uuid, user: Uuid) -> ApiResult<(Role, bool)> {
    let row: Option<(String, bool)> = sqlx::query_as(
        "SELECT role, restricted FROM project_members WHERE project_id = $1 AND user_id = $2",
    )
    .bind(project)
    .bind(user)
    .fetch_optional(&s.db)
    .await?;
    row.and_then(|(r, restricted)| Role::parse(&r).map(|r| (r, restricted)))
        .ok_or(ApiError::NotFound)
}

#[derive(Deserialize, ToSchema)]
struct RoleBody {
    /// New project role. Granting `member` or higher ends limited access.
    role: Option<String>,
    /// Limit the member to the lists and spaces they hold a grant on. Needs the role `viewer`.
    restricted: Option<bool>,
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
    let requested = match &b.role {
        Some(r) => Some(Role::parse(r).ok_or_else(|| ApiError::BadRequest("unknown role".into()))?),
        None => None,
    };
    if requested.is_none() && b.restricted.is_none() {
        return Err(ApiError::BadRequest("nothing to change".into()));
    }
    let (current, was_restricted) = member_role(&s, id, target).await?;
    if current >= actor || requested.is_some_and(|r| r >= actor) {
        return Err(ApiError::Forbidden);
    }
    let new_role = requested.unwrap_or(current);
    // Only a viewer can have limited access; a higher role ends it.
    let restricted = match b.restricted {
        Some(r) => r,
        None => was_restricted && new_role == Role::Viewer,
    };
    if restricted && new_role != Role::Viewer {
        return Err(ApiError::BadRequest(
            "limited access needs the viewer role".into(),
        ));
    }
    sqlx::query("UPDATE project_members SET role = $3, restricted = $4 WHERE project_id = $1 AND user_id = $2")
        .bind(id)
        .bind(target)
        .bind(new_role.as_str())
        .bind(restricted)
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
    let (current, _) = member_role(&s, id, target).await?;
    if current == Role::Owner {
        return Err(ApiError::Forbidden); // an owner cannot be removed or leave: transfer/delete first
    }
    let allowed = target == user.id || (actor >= Role::Admin && current < actor);
    if !allowed {
        return Err(ApiError::Forbidden);
    }
    let mut tx = s.db.begin().await?;
    sqlx::query("DELETE FROM project_members WHERE project_id = $1 AND user_id = $2")
        .bind(id)
        .bind(target)
        .execute(&mut *tx)
        .await?;
    // Grants must not outlive the membership: they would revive if the person is added again.
    sqlx::query(
        "DELETE FROM resource_grants g WHERE g.user_id = $2 AND (
           (g.resource_type = 'list' AND g.resource_id IN (SELECT l.id FROM lists l JOIN workspaces w ON w.id = l.workspace_id WHERE w.project_id = $1))
        OR (g.resource_type = 'space' AND g.resource_id IN (SELECT sp.id FROM spaces sp JOIN workspaces w ON w.id = sp.workspace_id WHERE w.project_id = $1)))",
    )
    .bind(id)
    .bind(target)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
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
    store_invite(&s, user.id, id, None, role, &b)
        .await
        .map(Json)
}

/// Invites to a single list: whoever accepts becomes a limited-access member of the project (or
/// keeps their membership) and is granted `role` on this list only.
#[utoipa::path(operation_id="createListInvite", post, path="/lists/{id}/invites", tag="lists", params(("id"=Uuid,Path)), request_body = InviteBody, responses((status=200, body=InviteOut), (status=400), (status=403), (status=404)), security(("bearer" = [])))]
async fn create_list_invite(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<InviteBody>,
) -> ApiResult<Json<InviteOut>> {
    let (scope, role) = list_invite_role(&s, user.id, id, &b.role).await?;
    store_invite(&s, user.id, scope.project_id, Some(id), role, &b)
        .await
        .map(Json)
}

/// Checks that the caller administers the list and may hand out `wanted` on it (never more than
/// their own role there). Shared with the e-mail invite.
pub(super) async fn list_invite_role(
    s: &AppState,
    user: Uuid,
    list: Uuid,
    wanted: &str,
) -> ApiResult<(Scope, Role)> {
    let (scope, caller) = list_access(&s.db, user, list, Role::Admin).await?;
    check_grant_role(wanted)?;
    let role = Role::parse(wanted).ok_or_else(|| ApiError::BadRequest("unknown role".into()))?;
    if role > caller {
        return Err(ApiError::Forbidden);
    }
    Ok((scope, role))
}

async fn store_invite(
    s: &AppState,
    user: Uuid,
    project_id: Uuid,
    list_id: Option<Uuid>,
    role: Role,
    b: &InviteBody,
) -> ApiResult<InviteOut> {
    if !(1..=24 * 30).contains(&b.expires_in_hours) || !(1..=100).contains(&b.max_uses) {
        return Err(ApiError::BadRequest(
            "expires_in_hours 1-720, max_uses 1-100".into(),
        ));
    }
    let token = random_token();
    let expires_at = Utc::now() + Duration::hours(b.expires_in_hours);
    sqlx::query(
        "INSERT INTO invites (id, project_id, list_id, token_hash, role, expires_at, max_uses, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(Uuid::new_v4())
    .bind(project_id)
    .bind(list_id)
    .bind(hash_token(&token))
    .bind(role.as_str())
    .bind(expires_at)
    .bind(b.max_uses)
    .bind(user)
    .execute(&s.db)
    .await?;
    Ok(InviteOut { token, expires_at })
}

#[derive(Serialize, ToSchema)]
struct AcceptOut {
    project_id: Uuid,
    /// Set for list invites, so the client can open the list afterwards.
    workspace_id: Option<Uuid>,
    list_slug: Option<String>,
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
    if !s.auth_limiter.check(&format!("invite:{}", user.id)).await {
        return Err(ApiError::TooManyRequests);
    }
    let mut tx = s.db.begin().await?;
    // Atomic claim: expiry and use-count are checked and consumed in one statement.
    let claimed: Option<(Uuid, String, Option<Uuid>)> = sqlx::query_as(
        "UPDATE invites SET uses = uses + 1
         WHERE token_hash = $1 AND expires_at > now() AND uses < max_uses
           AND (email IS NULL OR email = (SELECT u.email FROM users u WHERE u.id = $2))
         RETURNING project_id, role, list_id",
    )
    .bind(hash_token(&b.token))
    .bind(user.id)
    .fetch_optional(&mut *tx)
    .await?;
    let (project_id, role, list_id) = claimed.ok_or(ApiError::NotFound)?; // generic: unknown/expired/used up
    let mut target = (None, None);
    match list_id {
        None => {
            // A project invite also lifts limited access, keeping the higher of both roles.
            sqlx::query(
                "INSERT INTO project_members (project_id, user_id, role) VALUES ($1,$2,$3)
                 ON CONFLICT (project_id, user_id) DO UPDATE SET restricted = false,
                   role = CASE WHEN EXCLUDED.role = 'viewer' THEN project_members.role ELSE EXCLUDED.role END
                 WHERE project_members.restricted",
            )
            .bind(project_id)
            .bind(user.id)
            .bind(role)
            .execute(&mut *tx)
            .await?;
        }
        Some(list_id) => {
            // The list may have been deleted since the invite was written.
            let list: Option<(Uuid, String)> = sqlx::query_as(
                "SELECT workspace_id, slug FROM lists WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(list_id)
            .fetch_optional(&mut *tx)
            .await?;
            let (workspace_id, slug) = list.ok_or(ApiError::NotFound)?;
            // New people get limited access; existing members keep whatever they have.
            sqlx::query(
                "INSERT INTO project_members (project_id, user_id, role, restricted) VALUES ($1,$2,'viewer',true)
                 ON CONFLICT (project_id, user_id) DO NOTHING",
            )
            .bind(project_id)
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO resource_grants (resource_type, resource_id, user_id, role) VALUES ('list',$1,$2,$3)
                 ON CONFLICT (resource_type, resource_id, user_id) DO UPDATE SET role = EXCLUDED.role
                 WHERE array_position(ARRAY['viewer','member','admin'], EXCLUDED.role)
                     > array_position(ARRAY['viewer','member','admin'], resource_grants.role)",
            )
            .bind(list_id)
            .bind(user.id)
            .bind(role)
            .execute(&mut *tx)
            .await?;
            target = (Some(workspace_id), Some(slug));
        }
    }
    tx.commit().await?;
    Ok(Json(AcceptOut {
        project_id,
        workspace_id: target.0,
        list_slug: target.1,
    }))
}
