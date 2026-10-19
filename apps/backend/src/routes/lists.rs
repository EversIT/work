use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json as SqlJson;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::{
    spaces::{check_grant_role, GrantBody, GrantOut},
    OkOut,
};
use crate::{
    auth::AuthUser,
    authz::{
        is_project_member, require_list_role, require_space_role, require_workspace_role, Role,
        Scope,
    },
    error::{ApiError, ApiResult},
    state::AppState,
    util::{short, slugify},
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_list))
        .routes(routes!(get_list, update_list, delete_list))
        .routes(routes!(get_list_by_slug))
        .routes(routes!(create_stage, reorder_stages))
        .routes(routes!(update_stage, delete_stage))
        .routes(routes!(list_groups, create_group))
        .routes(routes!(update_group, delete_group))
        .routes(routes!(list_views, create_view))
        .routes(routes!(update_view, delete_view))
        .routes(routes!(set_list_fields))
        .routes(routes!(list_list_grants))
        .routes(routes!(put_list_grant, delete_list_grant))
}

#[derive(Serialize, ToSchema, sqlx::FromRow, Clone)]
pub struct StageOut {
    pub id: Uuid,
    pub list_id: Uuid,
    pub name: String,
    pub color: String,
    pub sort_order: i32,
    pub is_completed: bool,
}

#[derive(Serialize, ToSchema, sqlx::FromRow, Clone)]
pub struct ViewOut {
    pub id: Uuid,
    pub list_id: Uuid,
    pub name: String,
    pub r#type: String,
    #[schema(value_type = Object)]
    pub options: SqlJson<Value>,
    #[schema(value_type = Option<Object>)]
    pub filters: Option<SqlJson<Value>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct ListRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub space_id: Uuid,
    pub default_card_type_id: Option<Uuid>,
    pub name: String,
    pub slug: String,
    pub icon: String,
    pub icon_color: String,
    pub r#type: String,
    pub access_type: String,
    /// `kanban` (board) or `scrum` (backlog and sprints)
    pub methodology: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct ListDetail {
    #[serde(flatten)]
    pub list: ListRow,
    pub stages: Vec<StageOut>,
    pub views: Vec<ViewOut>,
    pub field_ids: Vec<Uuid>,
}

const LIST_COLS: &str = "id, workspace_id, space_id, default_card_type_id, name, slug, icon, icon_color, type, access_type, methodology, created_at";
const VIEW_COLS: &str = "id, list_id, name, type, options, filters, created_at, updated_at";

async fn detail(s: &AppState, list: ListRow) -> ApiResult<ListDetail> {
    let stages = sqlx::query_as("SELECT id, list_id, name, color, sort_order, is_completed FROM list_stages WHERE list_id = $1 ORDER BY sort_order, name")
        .bind(list.id).fetch_all(&s.db).await?;
    let sql = format!("SELECT {VIEW_COLS} FROM views WHERE list_id = $1 ORDER BY created_at");
    let views = sqlx::query_as(&sql).bind(list.id).fetch_all(&s.db).await?;
    let fids: Vec<(Uuid,)> = sqlx::query_as("SELECT field_id FROM list_fields WHERE list_id = $1")
        .bind(list.id)
        .fetch_all(&s.db)
        .await?;
    Ok(ListDetail {
        list,
        stages,
        views,
        field_ids: fids.into_iter().map(|f| f.0).collect(),
    })
}

fn check_list_type(t: &str) -> ApiResult<()> {
    if matches!(
        t,
        "default_list" | "crm_contacts" | "crm_organizations" | "crm_deals"
    ) {
        Ok(())
    } else {
        Err(ApiError::BadRequest("unknown list type".into()))
    }
}
fn check_access(t: &str) -> ApiResult<()> {
    if matches!(t, "public" | "private") {
        Ok(())
    } else {
        Err(ApiError::BadRequest(
            "access_type must be public or private".into(),
        ))
    }
}

#[derive(Deserialize, ToSchema)]
pub struct ListBody {
    pub name: String,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub r#type: Option<String>,
    pub access_type: Option<String>,
    pub default_card_type_id: Option<Uuid>,
    /// `kanban` (default) or `scrum`
    pub methodology: Option<String>,
}

#[utoipa::path(post, operation_id = "createList", path = "/spaces/{id}/lists", tag = "lists",
    params(("id" = Uuid, Path)), request_body = ListBody,
    responses((status = 200, body = ListDetail), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn create_list(
    State(s): State<AppState>,
    user: AuthUser,
    Path(space_id): Path<Uuid>,
    Json(b): Json<ListBody>,
) -> ApiResult<Json<ListDetail>> {
    let Scope { workspace_id, .. } =
        require_space_role(&s.db, user.id, space_id, Role::Member).await?;
    short(&b.name, 100, "name")?;
    let ty = b.r#type.as_deref().unwrap_or("default_list");
    check_list_type(ty)?;
    let access = b.access_type.as_deref().unwrap_or("public");
    check_access(access)?;
    let methodology = b.methodology.as_deref().unwrap_or("kanban");
    if !matches!(methodology, "kanban" | "scrum") {
        return Err(ApiError::BadRequest(
            "methodology must be kanban or scrum".into(),
        ));
    }

    let card_type: Uuid = match b.default_card_type_id {
        Some(ct) => {
            let ok: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM card_types WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL")
                .bind(ct).bind(workspace_id).fetch_optional(&s.db).await?;
            ok.ok_or_else(|| ApiError::BadRequest("card type not found in this workspace".into()))?
                .0
        }
        None => {
            let first: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM card_types WHERE workspace_id = $1 AND deleted_at IS NULL ORDER BY created_at LIMIT 1")
                .bind(workspace_id).fetch_optional(&s.db).await?;
            first
                .ok_or_else(|| ApiError::BadRequest("workspace has no card type".into()))?
                .0
        }
    };

    let base = slugify(&b.name);
    let id = Uuid::new_v4();
    let mut tx = s.db.begin().await?;
    // Slug is unique per workspace: take the first free `name`, `name-2`, ...
    let mut slug = base.clone();
    for n in 2..100 {
        let taken: (i64,) = sqlx::query_as("SELECT count(*) FROM lists WHERE workspace_id = $1 AND slug = $2 AND deleted_at IS NULL")
            .bind(workspace_id).bind(&slug).fetch_one(&mut *tx).await?;
        if taken.0 == 0 {
            break;
        }
        slug = format!("{base}-{n}");
    }
    let sql = format!("INSERT INTO lists (id, workspace_id, space_id, default_card_type_id, name, slug, icon, icon_color, type, access_type, methodology, created_by)
        VALUES ($1,$2,$3,$4,$5,$6,COALESCE($7,'list'),COALESCE($8,'default'),$9,$10,$12,$11) RETURNING {LIST_COLS}");
    let list: ListRow = sqlx::query_as(&sql)
        .bind(id)
        .bind(workspace_id)
        .bind(space_id)
        .bind(card_type)
        .bind(b.name.trim())
        .bind(&slug)
        .bind(&b.icon)
        .bind(&b.icon_color)
        .bind(ty)
        .bind(access)
        .bind(user.id)
        .bind(methodology)
        .fetch_one(&mut *tx)
        .await?;
    for (i, (name, color, done)) in [
        ("To Do", "grey", false),
        ("In Progress", "blue", false),
        ("Done", "green", true),
    ]
    .into_iter()
    .enumerate()
    {
        sqlx::query("INSERT INTO list_stages (id, list_id, name, color, sort_order, is_completed) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(Uuid::new_v4()).bind(id).bind(name).bind(color).bind(i as i32 * 100).bind(done).execute(&mut *tx).await?;
    }
    // Jira-style navigation: board, list, calendar and timeline are views of every project.
    for (name, ty, options) in [
        ("Board", "board", serde_json::json!({"hideChildren": true})),
        ("List", "list", serde_json::json!({})),
        ("Calendar", "calendar", serde_json::json!({})),
        ("Timeline", "gantt", serde_json::json!({})),
    ] {
        sqlx::query("INSERT INTO views (id, list_id, name, type, options, created_at) VALUES ($1,$2,$3,$4,$5,clock_timestamp())")
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(name)
            .bind(ty)
            .bind(SqlJson(options))
            .execute(&mut *tx)
            .await?;
    }
    if access == "private" {
        sqlx::query("INSERT INTO resource_grants (resource_type, resource_id, user_id, role) VALUES ('list',$1,$2,'admin')")
            .bind(id).bind(user.id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(detail(&s, list).await?))
}

#[utoipa::path(get, operation_id = "getList", path = "/lists/{id}", tag = "lists",
    params(("id" = Uuid, Path)), responses((status = 200, body = ListDetail), (status = 404)), security(("bearer" = [])))]
async fn get_list(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ListDetail>> {
    require_list_role(&s.db, user.id, id, Role::Viewer).await?;
    let sql = format!("SELECT {LIST_COLS} FROM lists WHERE id = $1");
    let list = sqlx::query_as(&sql).bind(id).fetch_one(&s.db).await?;
    Ok(Json(detail(&s, list).await?))
}

#[utoipa::path(get, operation_id = "getListBySlug", path = "/workspaces/{id}/lists/by-slug/{slug}", tag = "lists",
    params(("id" = Uuid, Path), ("slug" = String, Path)), responses((status = 200, body = ListDetail), (status = 404)), security(("bearer" = [])))]
async fn get_list_by_slug(
    State(s): State<AppState>,
    user: AuthUser,
    Path((wid, slug)): Path<(Uuid, String)>,
) -> ApiResult<Json<ListDetail>> {
    require_workspace_role(&s.db, user.id, wid, Role::Viewer).await?;
    let found: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM lists WHERE workspace_id = $1 AND slug = $2 AND deleted_at IS NULL",
    )
    .bind(wid)
    .bind(&slug)
    .fetch_optional(&s.db)
    .await?;
    let (id,) = found.ok_or(ApiError::NotFound)?;
    get_list(State(s), user, Path(id)).await // same visibility rules as by id
}

#[derive(Deserialize, ToSchema)]
pub struct ListPatch {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub access_type: Option<String>,
    pub default_card_type_id: Option<Uuid>,
}

#[utoipa::path(patch, operation_id = "updateList", path = "/lists/{id}", tag = "lists",
    params(("id" = Uuid, Path)), request_body = ListPatch,
    responses((status = 200, body = ListDetail), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update_list(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<ListPatch>,
) -> ApiResult<Json<ListDetail>> {
    let min = if b.access_type.is_some() {
        Role::Admin
    } else {
        Role::Member
    };
    let scope = require_list_role(&s.db, user.id, id, min).await?;
    if let Some(n) = &b.name {
        short(n, 100, "name")?;
    }
    if let Some(a) = &b.access_type {
        check_access(a)?;
    }
    if let Some(ct) = b.default_card_type_id {
        let ok: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM card_types WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        )
        .bind(ct)
        .bind(scope.workspace_id)
        .fetch_optional(&s.db)
        .await?;
        if ok.is_none() {
            return Err(ApiError::BadRequest(
                "card type not found in this workspace".into(),
            ));
        }
    }
    let sql = format!("UPDATE lists SET name = COALESCE($2,name), icon = COALESCE($3,icon), icon_color = COALESCE($4,icon_color), access_type = COALESCE($5,access_type), default_card_type_id = COALESCE($6,default_card_type_id) WHERE id = $1 RETURNING {LIST_COLS}");
    let list = sqlx::query_as(&sql)
        .bind(id)
        .bind(b.name.as_deref().map(str::trim))
        .bind(&b.icon)
        .bind(&b.icon_color)
        .bind(&b.access_type)
        .bind(b.default_card_type_id)
        .fetch_one(&s.db)
        .await?;
    Ok(Json(detail(&s, list).await?))
}

#[utoipa::path(delete, operation_id = "deleteList", path = "/lists/{id}", tag = "lists",
    params(("id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn delete_list(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    require_list_role(&s.db, user.id, id, Role::Admin).await?;
    sqlx::query("UPDATE lists SET deleted_at = now() WHERE id = $1")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(Json(OkOut { ok: true }))
}

// ---- stages ---------------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct StageBody {
    pub name: String,
    pub color: Option<String>,
    pub is_completed: Option<bool>,
}

#[utoipa::path(post, operation_id = "createStage", path = "/lists/{id}/stages", tag = "lists",
    params(("id" = Uuid, Path)), request_body = StageBody,
    responses((status = 200, body = StageOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn create_stage(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<StageBody>,
) -> ApiResult<Json<StageOut>> {
    require_list_role(&s.db, user.id, id, Role::Admin).await?;
    short(&b.name, 100, "name")?;
    let row = sqlx::query_as(
        "INSERT INTO list_stages (id, list_id, name, color, sort_order, is_completed)
         VALUES ($1,$2,$3,COALESCE($4,'default'),(SELECT COALESCE(max(sort_order),-100)+100 FROM list_stages WHERE list_id = $2),$5)
         RETURNING id, list_id, name, color, sort_order, is_completed",
    )
    .bind(Uuid::new_v4()).bind(id).bind(b.name.trim()).bind(&b.color).bind(b.is_completed.unwrap_or(false))
    .fetch_one(&s.db).await?;
    Ok(Json(row))
}

async fn stage_list(s: &AppState, stage_id: Uuid) -> ApiResult<Uuid> {
    let r: Option<(Uuid,)> = sqlx::query_as("SELECT list_id FROM list_stages WHERE id = $1")
        .bind(stage_id)
        .fetch_optional(&s.db)
        .await?;
    Ok(r.ok_or(ApiError::NotFound)?.0)
}

#[derive(Deserialize, ToSchema)]
pub struct StagePatch {
    pub name: Option<String>,
    pub color: Option<String>,
    pub is_completed: Option<bool>,
}

#[utoipa::path(patch, operation_id = "updateStage", path = "/stages/{id}", tag = "lists",
    params(("id" = Uuid, Path)), request_body = StagePatch,
    responses((status = 200, body = StageOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update_stage(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<StagePatch>,
) -> ApiResult<Json<StageOut>> {
    let list_id = stage_list(&s, id).await?;
    require_list_role(&s.db, user.id, list_id, Role::Admin).await?;
    if let Some(n) = &b.name {
        short(n, 100, "name")?;
    }
    let row = sqlx::query_as(
        "UPDATE list_stages SET name = COALESCE($2,name), color = COALESCE($3,color), is_completed = COALESCE($4,is_completed)
         WHERE id = $1 RETURNING id, list_id, name, color, sort_order, is_completed",
    )
    .bind(id).bind(b.name.as_deref().map(str::trim)).bind(&b.color).bind(b.is_completed)
    .fetch_one(&s.db).await?;
    Ok(Json(row))
}

#[utoipa::path(delete, operation_id = "deleteStage", path = "/stages/{id}", tag = "lists",
    params(("id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 403), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn delete_stage(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    let list_id = stage_list(&s, id).await?;
    require_list_role(&s.db, user.id, list_id, Role::Admin).await?;
    let n: (i64,) = sqlx::query_as("SELECT count(*) FROM card_lists WHERE list_stage_id = $1")
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    if n.0 > 0 {
        return Err(ApiError::Conflict);
    } // move the cards first
    let stages: (i64,) = sqlx::query_as("SELECT count(*) FROM list_stages WHERE list_id = $1")
        .bind(list_id)
        .fetch_one(&s.db)
        .await?;
    if stages.0 <= 1 {
        return Err(ApiError::Conflict);
    } // a list always needs one stage
    sqlx::query("DELETE FROM list_stages WHERE id = $1")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(Json(OkOut { ok: true }))
}

#[derive(Deserialize, ToSchema)]
pub struct OrderBody {
    pub ids: Vec<Uuid>,
}

#[utoipa::path(put, operation_id = "reorderStages", path = "/lists/{id}/stages/order", tag = "lists",
    params(("id" = Uuid, Path)), request_body = OrderBody,
    responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn reorder_stages(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<OrderBody>,
) -> ApiResult<Json<OkOut>> {
    require_list_role(&s.db, user.id, id, Role::Admin).await?;
    let existing: Vec<(Uuid,)> = sqlx::query_as("SELECT id FROM list_stages WHERE list_id = $1")
        .bind(id)
        .fetch_all(&s.db)
        .await?;
    let mut have: Vec<Uuid> = existing.into_iter().map(|e| e.0).collect();
    let mut want = b.ids.clone();
    have.sort();
    want.sort();
    if have != want {
        return Err(ApiError::BadRequest(
            "ids must contain every stage of the list exactly once".into(),
        ));
    }
    let mut tx = s.db.begin().await?;
    for (i, sid) in b.ids.iter().enumerate() {
        sqlx::query("UPDATE list_stages SET sort_order = $2 WHERE id = $1 AND list_id = $3")
            .bind(sid)
            .bind(i as i32 * 100)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(OkOut { ok: true }))
}

// ---- groups ---------------------------------------------------------------------------------

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct GroupOut {
    pub id: Uuid,
    pub list_id: Uuid,
    pub field_id: Option<Uuid>,
    pub name: String,
    pub r#type: String,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub sort_order: i32,
    pub is_expanded: bool,
    #[schema(value_type = Option<Object>)]
    pub filter: Option<SqlJson<Value>>,
}

const GROUP_COLS: &str =
    "id, list_id, field_id, name, type, color, icon, sort_order, is_expanded, filter";

#[utoipa::path(get, operation_id = "listGroups", path = "/lists/{id}/groups", tag = "lists",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<GroupOut>), (status = 404)), security(("bearer" = [])))]
async fn list_groups(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<GroupOut>>> {
    require_list_role(&s.db, user.id, id, Role::Viewer).await?;
    let sql =
        format!("SELECT {GROUP_COLS} FROM list_groups WHERE list_id = $1 ORDER BY sort_order");
    Ok(Json(sqlx::query_as(&sql).bind(id).fetch_all(&s.db).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct GroupBody {
    pub name: String,
    /// `all`, `list_stage` or `field`
    pub r#type: String,
    pub field_id: Option<Uuid>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub is_expanded: Option<bool>,
    #[schema(value_type = Option<Object>)]
    pub filter: Option<Value>,
}

#[utoipa::path(post, operation_id = "createGroup", path = "/lists/{id}/groups", tag = "lists",
    params(("id" = Uuid, Path)), request_body = GroupBody,
    responses((status = 200, body = GroupOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn create_group(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<GroupBody>,
) -> ApiResult<Json<GroupOut>> {
    let scope = require_list_role(&s.db, user.id, id, Role::Member).await?;
    short(&b.name, 100, "name")?;
    if !matches!(b.r#type.as_str(), "all" | "list_stage" | "field") {
        return Err(ApiError::BadRequest("unknown group type".into()));
    }
    if b.r#type == "field" {
        let fid = b
            .field_id
            .ok_or_else(|| ApiError::BadRequest("field_id required for field groups".into()))?;
        let ok: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM fields WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        )
        .bind(fid)
        .bind(scope.workspace_id)
        .fetch_optional(&s.db)
        .await?;
        if ok.is_none() {
            return Err(ApiError::BadRequest(
                "field not found in this workspace".into(),
            ));
        }
    }
    if let Some(f) = &b.filter {
        crate::filters::validate_json(f)?;
    }
    let sql = format!("INSERT INTO list_groups (id, list_id, field_id, name, type, color, icon, sort_order, is_expanded, filter)
        VALUES ($1,$2,$3,$4,$5,$6,$7,(SELECT COALESCE(max(sort_order),-1)+1 FROM list_groups WHERE list_id = $2),$8,$9) RETURNING {GROUP_COLS}");
    let row = sqlx::query_as(&sql)
        .bind(Uuid::new_v4())
        .bind(id)
        .bind(b.field_id)
        .bind(b.name.trim())
        .bind(&b.r#type)
        .bind(&b.color)
        .bind(&b.icon)
        .bind(b.is_expanded.unwrap_or(true))
        .bind(b.filter.map(SqlJson))
        .fetch_one(&s.db)
        .await?;
    Ok(Json(row))
}

#[derive(Deserialize, ToSchema)]
pub struct GroupPatch {
    pub name: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub is_expanded: Option<bool>,
    pub sort_order: Option<i32>,
    #[schema(value_type = Option<Object>)]
    pub filter: Option<Value>,
}

async fn group_list(s: &AppState, group_id: Uuid) -> ApiResult<Uuid> {
    let r: Option<(Uuid,)> = sqlx::query_as("SELECT list_id FROM list_groups WHERE id = $1")
        .bind(group_id)
        .fetch_optional(&s.db)
        .await?;
    Ok(r.ok_or(ApiError::NotFound)?.0)
}

#[utoipa::path(patch, operation_id = "updateGroup", path = "/groups/{id}", tag = "lists",
    params(("id" = Uuid, Path)), request_body = GroupPatch,
    responses((status = 200, body = GroupOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update_group(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<GroupPatch>,
) -> ApiResult<Json<GroupOut>> {
    let list_id = group_list(&s, id).await?;
    require_list_role(&s.db, user.id, list_id, Role::Member).await?;
    if let Some(n) = &b.name {
        short(n, 100, "name")?;
    }
    if let Some(f) = &b.filter {
        crate::filters::validate_json(f)?;
    }
    let sql = format!("UPDATE list_groups SET name = COALESCE($2,name), color = COALESCE($3,color), icon = COALESCE($4,icon), is_expanded = COALESCE($5,is_expanded), sort_order = COALESCE($6,sort_order), filter = COALESCE($7,filter) WHERE id = $1 RETURNING {GROUP_COLS}");
    let row = sqlx::query_as(&sql)
        .bind(id)
        .bind(b.name.as_deref().map(str::trim))
        .bind(&b.color)
        .bind(&b.icon)
        .bind(b.is_expanded)
        .bind(b.sort_order)
        .bind(b.filter.map(SqlJson))
        .fetch_one(&s.db)
        .await?;
    Ok(Json(row))
}

#[utoipa::path(delete, operation_id = "deleteGroup", path = "/groups/{id}", tag = "lists",
    params(("id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn delete_group(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    let list_id = group_list(&s, id).await?;
    require_list_role(&s.db, user.id, list_id, Role::Member).await?;
    sqlx::query("DELETE FROM list_groups WHERE id = $1")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(Json(OkOut { ok: true }))
}

// ---- views ----------------------------------------------------------------------------------

#[utoipa::path(get, operation_id = "listViews", path = "/lists/{id}/views", tag = "lists",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<ViewOut>), (status = 404)), security(("bearer" = [])))]
async fn list_views(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<ViewOut>>> {
    require_list_role(&s.db, user.id, id, Role::Viewer).await?;
    let sql = format!("SELECT {VIEW_COLS} FROM views WHERE list_id = $1 ORDER BY created_at");
    Ok(Json(sqlx::query_as(&sql).bind(id).fetch_all(&s.db).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct ViewBody {
    pub name: String,
    /// `table`, `board`, `calendar`, `gantt` or `list`
    pub r#type: String,
    #[schema(value_type = Option<Object>)]
    pub options: Option<Value>,
    #[schema(value_type = Option<Object>)]
    pub filters: Option<Value>,
}

fn check_view(
    b_type: Option<&str>,
    options: &Option<Value>,
    filters: &Option<Value>,
) -> ApiResult<()> {
    if let Some(t) = b_type {
        if !matches!(t, "table" | "board" | "calendar" | "gantt" | "list") {
            return Err(ApiError::BadRequest("unknown view type".into()));
        }
    }
    if let Some(o) = options {
        if !o.is_object() || o.to_string().len() > 8192 {
            return Err(ApiError::BadRequest(
                "options must be an object under 8 KB".into(),
            ));
        }
    }
    if let Some(f) = filters {
        crate::filters::validate_json(f)?;
    }
    Ok(())
}

#[utoipa::path(post, operation_id = "createView", path = "/lists/{id}/views", tag = "lists",
    params(("id" = Uuid, Path)), request_body = ViewBody,
    responses((status = 200, body = ViewOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn create_view(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<ViewBody>,
) -> ApiResult<Json<ViewOut>> {
    require_list_role(&s.db, user.id, id, Role::Member).await?;
    short(&b.name, 100, "name")?;
    check_view(Some(&b.r#type), &b.options, &b.filters)?;
    let sql = format!("INSERT INTO views (id, list_id, name, type, options, filters) VALUES ($1,$2,$3,$4,$5,$6) RETURNING {VIEW_COLS}");
    let row = sqlx::query_as(&sql)
        .bind(Uuid::new_v4())
        .bind(id)
        .bind(b.name.trim())
        .bind(&b.r#type)
        .bind(SqlJson(b.options.unwrap_or_else(|| serde_json::json!({}))))
        .bind(b.filters.map(SqlJson))
        .fetch_one(&s.db)
        .await?;
    Ok(Json(row))
}

#[derive(Deserialize, ToSchema)]
pub struct ViewPatch {
    pub name: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub options: Option<Value>,
    #[schema(value_type = Option<Object>)]
    pub filters: Option<Value>,
}

async fn view_list(s: &AppState, view_id: Uuid) -> ApiResult<Uuid> {
    let r: Option<(Uuid,)> = sqlx::query_as("SELECT list_id FROM views WHERE id = $1")
        .bind(view_id)
        .fetch_optional(&s.db)
        .await?;
    Ok(r.ok_or(ApiError::NotFound)?.0)
}

#[utoipa::path(patch, operation_id = "updateView", path = "/views/{id}", tag = "lists",
    params(("id" = Uuid, Path)), request_body = ViewPatch,
    responses((status = 200, body = ViewOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update_view(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<ViewPatch>,
) -> ApiResult<Json<ViewOut>> {
    let list_id = view_list(&s, id).await?;
    require_list_role(&s.db, user.id, list_id, Role::Member).await?;
    if let Some(n) = &b.name {
        short(n, 100, "name")?;
    }
    check_view(None, &b.options, &b.filters)?;
    let sql = format!("UPDATE views SET name = COALESCE($2,name), options = COALESCE($3,options), filters = COALESCE($4,filters), updated_at = now() WHERE id = $1 RETURNING {VIEW_COLS}");
    let row = sqlx::query_as(&sql)
        .bind(id)
        .bind(b.name.as_deref().map(str::trim))
        .bind(b.options.map(SqlJson))
        .bind(b.filters.map(SqlJson))
        .fetch_one(&s.db)
        .await?;
    Ok(Json(row))
}

#[utoipa::path(delete, operation_id = "deleteView", path = "/views/{id}", tag = "lists",
    params(("id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 403), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn delete_view(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    let list_id = view_list(&s, id).await?;
    require_list_role(&s.db, user.id, list_id, Role::Member).await?;
    let n: (i64,) = sqlx::query_as("SELECT count(*) FROM views WHERE list_id = $1")
        .bind(list_id)
        .fetch_one(&s.db)
        .await?;
    if n.0 <= 1 {
        return Err(ApiError::Conflict);
    }
    sqlx::query("DELETE FROM views WHERE id = $1")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(Json(OkOut { ok: true }))
}

// ---- list fields & grants -------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct FieldIdsBody {
    pub field_ids: Vec<Uuid>,
}

#[utoipa::path(put, operation_id = "setListFields", path = "/lists/{id}/fields", tag = "lists",
    params(("id" = Uuid, Path)), request_body = FieldIdsBody,
    responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn set_list_fields(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<FieldIdsBody>,
) -> ApiResult<Json<OkOut>> {
    let scope = require_list_role(&s.db, user.id, id, Role::Admin).await?;
    let ok: (i64,) = sqlx::query_as("SELECT count(*) FROM fields WHERE id = ANY($1) AND workspace_id = $2 AND deleted_at IS NULL")
        .bind(&b.field_ids).bind(scope.workspace_id).fetch_one(&s.db).await?;
    let unique: std::collections::HashSet<_> = b.field_ids.iter().collect();
    if ok.0 as usize != unique.len() {
        return Err(ApiError::BadRequest("unknown field id".into()));
    }
    let mut tx = s.db.begin().await?;
    sqlx::query("DELETE FROM list_fields WHERE list_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for f in unique {
        sqlx::query("INSERT INTO list_fields (list_id, field_id) VALUES ($1,$2)")
            .bind(id)
            .bind(f)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(OkOut { ok: true }))
}

#[utoipa::path(get, operation_id = "listListGrants", path = "/lists/{id}/grants", tag = "lists",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<GrantOut>), (status = 404)), security(("bearer" = [])))]
async fn list_list_grants(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<GrantOut>>> {
    require_list_role(&s.db, user.id, id, Role::Admin).await?;
    Ok(Json(sqlx::query_as("SELECT user_id, role FROM resource_grants WHERE resource_type = 'list' AND resource_id = $1").bind(id).fetch_all(&s.db).await?))
}

#[utoipa::path(put, operation_id = "putListGrant", path = "/lists/{id}/grants/{user_id}", tag = "lists",
    params(("id" = Uuid, Path), ("user_id" = Uuid, Path)), request_body = GrantBody,
    responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn put_list_grant(
    State(s): State<AppState>,
    user: AuthUser,
    Path((id, target)): Path<(Uuid, Uuid)>,
    Json(b): Json<GrantBody>,
) -> ApiResult<Json<OkOut>> {
    let scope = require_list_role(&s.db, user.id, id, Role::Admin).await?;
    check_grant_role(&b.role)?;
    if !is_project_member(&s.db, scope.project_id, target).await? {
        return Err(ApiError::BadRequest(
            "user is not a member of this project".into(),
        ));
    }
    sqlx::query("INSERT INTO resource_grants (resource_type, resource_id, user_id, role) VALUES ('list',$1,$2,$3)
                 ON CONFLICT (resource_type, resource_id, user_id) DO UPDATE SET role = EXCLUDED.role")
        .bind(id).bind(target).bind(&b.role).execute(&s.db).await?;
    Ok(Json(OkOut { ok: true }))
}

#[utoipa::path(delete, operation_id = "deleteListGrant", path = "/lists/{id}/grants/{user_id}", tag = "lists",
    params(("id" = Uuid, Path), ("user_id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 404)), security(("bearer" = [])))]
async fn delete_list_grant(
    State(s): State<AppState>,
    user: AuthUser,
    Path((id, target)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<OkOut>> {
    require_list_role(&s.db, user.id, id, Role::Admin).await?;
    sqlx::query("DELETE FROM resource_grants WHERE resource_type = 'list' AND resource_id = $1 AND user_id = $2").bind(id).bind(target).execute(&s.db).await?;
    Ok(Json(OkOut { ok: true }))
}
