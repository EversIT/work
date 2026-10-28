use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::OkOut;
use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthUser,
    security::authz::{require_membership, require_space_role, require_workspace_role, Role},
    state::AppState,
    util::short,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(tree))
        .routes(routes!(create_space))
        .routes(routes!(get_space, update_space, delete_space))
        .routes(routes!(list_space_grants))
        .routes(routes!(put_space_grant, delete_space_grant))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct SpaceOut {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub icon: String,
    pub icon_color: String,
    pub access_type: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct ListBrief {
    pub id: Uuid,
    pub space_id: Uuid,
    pub name: String,
    pub slug: String,
    pub icon: String,
    pub icon_color: String,
    pub r#type: String,
    pub access_type: String,
    pub methodology: String,
}

#[derive(Serialize, ToSchema)]
pub struct SpaceTree {
    #[serde(flatten)]
    pub space: SpaceOut,
    pub lists: Vec<ListBrief>,
}

#[derive(Serialize, ToSchema)]
pub struct TreeOut {
    pub spaces: Vec<SpaceTree>,
}

/// Spaces and lists of a workspace that the caller is allowed to see.
#[utoipa::path(get, operation_id = "getWorkspaceTree", path = "/workspaces/{id}/tree", tag = "spaces",
    params(("id" = Uuid, Path)), responses((status = 200, body = TreeOut), (status = 404)),
    security(("bearer" = [])))]
pub(crate) async fn tree(
    State(s): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
) -> ApiResult<Json<TreeOut>> {
    let project_id = require_workspace_role(&s.db, user.id, wid, Role::Viewer).await?;
    let (role, restricted) = require_membership(&s.db, user.id, project_id, Role::Viewer).await?;
    let admin = role >= Role::Admin;

    let spaces: Vec<SpaceOut> = sqlx::query_as(
        "SELECT s.id, s.workspace_id, s.name, s.icon, s.icon_color, s.access_type, s.created_at
         FROM spaces s WHERE s.workspace_id = $1 AND s.deleted_at IS NULL
           AND ($2 OR (NOT $4 AND s.access_type = 'public') OR EXISTS (
                SELECT 1 FROM resource_grants g WHERE g.user_id = $3 AND g.resource_type = 'space' AND g.resource_id = s.id)
                -- a list grant shows its space as the container, but nothing else in it
                OR EXISTS (SELECT 1 FROM resource_grants g JOIN lists gl ON gl.id = g.resource_id AND gl.deleted_at IS NULL
                           WHERE g.user_id = $3 AND g.resource_type = 'list' AND gl.space_id = s.id))
         ORDER BY s.created_at",
    )
    .bind(wid)
    .bind(admin)
    .bind(user.id)
    .bind(restricted)
    .fetch_all(&s.db)
    .await?;
    let ids: Vec<Uuid> = spaces.iter().map(|s| s.id).collect();
    let lists: Vec<ListBrief> = sqlx::query_as(
        "SELECT l.id, l.space_id, l.name, l.slug, l.icon, l.icon_color, l.type, l.access_type, l.methodology
         FROM lists l WHERE l.space_id = ANY($1) AND l.deleted_at IS NULL
           AND ($2 OR (NOT $4 AND l.access_type = 'public')
                OR EXISTS (SELECT 1 FROM resource_grants g WHERE g.user_id = $3 AND
                           ((g.resource_type = 'list' AND g.resource_id = l.id) OR (g.resource_type = 'space' AND g.resource_id = l.space_id))))
         ORDER BY l.created_at",
    )
    .bind(&ids)
    .bind(admin)
    .bind(user.id)
    .bind(restricted)
    .fetch_all(&s.db)
    .await?;
    let spaces = spaces
        .into_iter()
        .map(|space| {
            let lists = lists
                .iter()
                .filter(|l| l.space_id == space.id)
                .map(|l| ListBrief {
                    id: l.id,
                    space_id: l.space_id,
                    name: l.name.clone(),
                    slug: l.slug.clone(),
                    icon: l.icon.clone(),
                    icon_color: l.icon_color.clone(),
                    r#type: l.r#type.clone(),
                    access_type: l.access_type.clone(),
                    methodology: l.methodology.clone(),
                })
                .collect();
            SpaceTree { space, lists }
        })
        .collect();
    Ok(Json(TreeOut { spaces }))
}

#[derive(Deserialize, ToSchema)]
pub struct SpaceBody {
    pub name: String,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    /// `public` or `private`
    pub access_type: Option<String>,
}

fn check_access_type(v: &str) -> ApiResult<()> {
    if v == "public" || v == "private" {
        Ok(())
    } else {
        Err(ApiError::BadRequest(
            "access_type must be public or private".into(),
        ))
    }
}

#[utoipa::path(post, operation_id = "createSpace", path = "/workspaces/{id}/spaces", tag = "spaces",
    params(("id" = Uuid, Path)), request_body = SpaceBody,
    responses((status = 200, body = SpaceOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn create_space(
    State(s): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
    Json(b): Json<SpaceBody>,
) -> ApiResult<Json<SpaceOut>> {
    require_workspace_role(&s.db, user.id, wid, Role::Member).await?;
    short(&b.name, 100, "name")?;
    let access = b.access_type.as_deref().unwrap_or("public");
    check_access_type(access)?;
    let id = Uuid::new_v4();
    let mut tx = s.db.begin().await?;
    let row: SpaceOut = sqlx::query_as(
        "INSERT INTO spaces (id, workspace_id, name, icon, icon_color, access_type, created_by)
         VALUES ($1,$2,$3,COALESCE($4,'folder'),COALESCE($5,'default'),$6,$7)
         RETURNING id, workspace_id, name, icon, icon_color, access_type, created_at",
    )
    .bind(id)
    .bind(wid)
    .bind(b.name.trim())
    .bind(&b.icon)
    .bind(&b.icon_color)
    .bind(access)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    if access == "private" {
        // The creator must not lock themselves out of their own private space.
        sqlx::query("INSERT INTO resource_grants (resource_type, resource_id, user_id, role) VALUES ('space',$1,$2,'admin')")
            .bind(id).bind(user.id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(row))
}

#[utoipa::path(get, operation_id = "getSpace", path = "/spaces/{id}", tag = "spaces",
    params(("id" = Uuid, Path)), responses((status = 200, body = SpaceOut), (status = 404)), security(("bearer" = [])))]
async fn get_space(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SpaceOut>> {
    require_space_role(&s.db, user.id, id, Role::Viewer).await?;
    let row = sqlx::query_as("SELECT id, workspace_id, name, icon, icon_color, access_type, created_at FROM spaces WHERE id = $1")
        .bind(id).fetch_one(&s.db).await?;
    Ok(Json(row))
}

#[derive(Deserialize, ToSchema)]
pub struct SpacePatch {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub access_type: Option<String>,
}

#[utoipa::path(patch, operation_id = "updateSpace", path = "/spaces/{id}", tag = "spaces",
    params(("id" = Uuid, Path)), request_body = SpacePatch,
    responses((status = 200, body = SpaceOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update_space(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<SpacePatch>,
) -> ApiResult<Json<SpaceOut>> {
    // Changing visibility is an admin action; cosmetic edits need member.
    let min = if b.access_type.is_some() {
        Role::Admin
    } else {
        Role::Member
    };
    require_space_role(&s.db, user.id, id, min).await?;
    if let Some(n) = &b.name {
        short(n, 100, "name")?;
    }
    if let Some(a) = &b.access_type {
        check_access_type(a)?;
    }
    let row = sqlx::query_as(
        "UPDATE spaces SET name = COALESCE($2,name), icon = COALESCE($3,icon), icon_color = COALESCE($4,icon_color),
                access_type = COALESCE($5,access_type)
         WHERE id = $1 RETURNING id, workspace_id, name, icon, icon_color, access_type, created_at",
    )
    .bind(id).bind(b.name.as_deref().map(str::trim)).bind(&b.icon).bind(&b.icon_color).bind(&b.access_type)
    .fetch_one(&s.db).await?;
    Ok(Json(row))
}

#[utoipa::path(delete, operation_id = "deleteSpace", path = "/spaces/{id}", tag = "spaces",
    params(("id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn delete_space(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    require_space_role(&s.db, user.id, id, Role::Admin).await?;
    let mut tx = s.db.begin().await?;
    sqlx::query("UPDATE lists SET deleted_at = now() WHERE space_id = $1 AND deleted_at IS NULL")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE spaces SET deleted_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(OkOut { ok: true }))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct GrantOut {
    pub user_id: Uuid,
    pub role: String,
}

#[derive(Deserialize, ToSchema)]
pub struct GrantBody {
    /// `viewer`, `member` or `admin`
    pub role: String,
}

pub(crate) fn check_grant_role(r: &str) -> ApiResult<()> {
    if matches!(r, "viewer" | "member" | "admin") {
        Ok(())
    } else {
        Err(ApiError::BadRequest(
            "role must be viewer, member or admin".into(),
        ))
    }
}

#[utoipa::path(get, operation_id = "listSpaceGrants", path = "/spaces/{id}/grants", tag = "spaces",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<GrantOut>), (status = 404)), security(("bearer" = [])))]
async fn list_space_grants(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<GrantOut>>> {
    require_space_role(&s.db, user.id, id, Role::Admin).await?;
    let rows = sqlx::query_as("SELECT user_id, role FROM resource_grants WHERE resource_type = 'space' AND resource_id = $1")
        .bind(id).fetch_all(&s.db).await?;
    Ok(Json(rows))
}

#[utoipa::path(put, operation_id = "putSpaceGrant", path = "/spaces/{id}/grants/{user_id}", tag = "spaces",
    params(("id" = Uuid, Path), ("user_id" = Uuid, Path)), request_body = GrantBody,
    responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn put_space_grant(
    State(s): State<AppState>,
    user: AuthUser,
    Path((id, target)): Path<(Uuid, Uuid)>,
    Json(b): Json<GrantBody>,
) -> ApiResult<Json<OkOut>> {
    let scope = require_space_role(&s.db, user.id, id, Role::Admin).await?;
    check_grant_role(&b.role)?;
    // Only members of the owning project can be granted access.
    if !crate::security::authz::is_project_member(&s.db, scope.project_id, target).await? {
        return Err(ApiError::BadRequest(
            "user is not a member of this project".into(),
        ));
    }
    sqlx::query("INSERT INTO resource_grants (resource_type, resource_id, user_id, role) VALUES ('space',$1,$2,$3)
                 ON CONFLICT (resource_type, resource_id, user_id) DO UPDATE SET role = EXCLUDED.role")
        .bind(id).bind(target).bind(&b.role).execute(&s.db).await?;
    Ok(Json(OkOut { ok: true }))
}

#[utoipa::path(delete, operation_id = "deleteSpaceGrant", path = "/spaces/{id}/grants/{user_id}", tag = "spaces",
    params(("id" = Uuid, Path), ("user_id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 404)), security(("bearer" = [])))]
async fn delete_space_grant(
    State(s): State<AppState>,
    user: AuthUser,
    Path((id, target)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<OkOut>> {
    require_space_role(&s.db, user.id, id, Role::Admin).await?;
    sqlx::query("DELETE FROM resource_grants WHERE resource_type = 'space' AND resource_id = $1 AND user_id = $2")
        .bind(id).bind(target).execute(&s.db).await?;
    Ok(Json(OkOut { ok: true }))
}
