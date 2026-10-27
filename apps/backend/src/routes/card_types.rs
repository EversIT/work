use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{types::Json as SqlJson, PgPool, Postgres, Transaction};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::OkOut;
use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthUser,
    security::authz::{require_workspace_role, require_workspace_scoped, Role},
    state::AppState,
    util::{field_slug, short},
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_card_types, create_card_type))
        .routes(routes!(update_card_type, delete_card_type))
        .routes(routes!(list_fields, create_field))
        .routes(routes!(update_field, delete_field))
}

pub const FIELD_TYPES: &[&str] = &[
    "text",
    "text_area",
    "dropdown",
    "label",
    "user",
    "date",
    "date_time",
    "number",
    "percentage",
    "checkbox",
    "email",
    "url",
    "currency",
    "stage",
    "rich",
    "card",
];

#[derive(Serialize, ToSchema, sqlx::FromRow, Clone)]
pub struct FieldOut {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub card_type_id: Option<Uuid>,
    pub data_card_type_id: Option<Uuid>,
    pub name: String,
    pub slug: String,
    pub r#type: String,
    pub icon: String,
    pub required: bool,
    pub multiple: bool,
    #[schema(value_type = Option<Object>)]
    pub items: Option<SqlJson<Value>>,
    pub is_title: bool,
    pub is_description: bool,
    pub is_photo: bool,
    pub is_assignee: bool,
    pub is_pinned: bool,
    pub created_by_type: String,
}

const FIELD_COLS: &str = "id, workspace_id, card_type_id, data_card_type_id, name, slug, type, icon, required, multiple, items, is_title, is_description, is_photo, is_assignee, is_pinned, created_by_type";

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct CardTypeRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub layout: String,
    pub title_template: Option<String>,
    pub has_children: bool,
    pub icon: String,
    pub color: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize, ToSchema)]
pub struct CardTypeOut {
    #[serde(flatten)]
    pub card_type: CardTypeRow,
    pub fields: Vec<FieldOut>,
}

const CT_COLS: &str =
    "id, workspace_id, name, layout, title_template, has_children, icon, color, created_at";

/// Jira-style defaults for a new workspace: five issue types (Task first, it is the default) that
/// share one set of workspace-wide fields. Returns the id of the default type.
pub async fn seed_default_card_type(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
    // (name, icon, colour, can have children)
    let types = [
        ("Task", "task", "blue", true),
        ("Epic", "epic", "purple", true),
        ("Story", "story", "green", true),
        ("Bug", "bug", "red", true),
        ("Sub-task", "subtask", "blue", false),
    ];
    let mut default_id = None;
    for (name, icon, color, children) in types {
        let id = Uuid::new_v4();
        default_id.get_or_insert(id);
        sqlx::query(
            "INSERT INTO card_types (id, workspace_id, name, icon, color, has_children, created_by, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,clock_timestamp())",
        )
        .bind(id)
        .bind(workspace_id)
        .bind(name)
        .bind(icon)
        .bind(color)
        .bind(children)
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    }
    // (name, slug, type, icon, title, description, assignee, multiple, items)
    type Seed = (
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        bool,
        bool,
        bool,
        bool,
        Option<&'static str>,
    );
    let fields: [Seed; 10] = [
        (
            "Summary", "title", "text", "text", true, false, false, false, None,
        ),
        (
            "Description",
            "description",
            "rich",
            "text-box",
            false,
            true,
            false,
            false,
            None,
        ),
        (
            "Assignee", "assignee", "user", "account", false, false, true, false, None,
        ),
        (
            "Reporter", "reporter", "user", "account", false, false, false, false, None,
        ),
        (
            "Priority",
            "priority",
            "dropdown",
            "priority",
            false,
            false,
            false,
            false,
            Some(
                r#"[{"item":"Highest"},{"item":"High"},{"item":"Medium"},{"item":"Low"},{"item":"Lowest"}]"#,
            ),
        ),
        (
            "Story points",
            "story_points",
            "number",
            "points",
            false,
            false,
            false,
            false,
            None,
        ),
        (
            "Labels", "labels", "label", "tag", false, false, false, true, None,
        ),
        (
            "Start date",
            "starts_at",
            "date",
            "calendar-start",
            false,
            false,
            false,
            false,
            None,
        ),
        (
            "Due date", "due_at", "date", "calendar", false, false, false, false, None,
        ),
        (
            "Original estimate (hours)",
            "estimate_hours",
            "number",
            "clock",
            false,
            false,
            false,
            false,
            None,
        ),
    ];
    for (name, slug, ty, icon, title, desc, assignee, multiple, items) in fields {
        sqlx::query(
            "INSERT INTO fields (id, workspace_id, card_type_id, name, slug, type, icon, multiple, is_title, is_description, is_assignee, items, created_by_type)
             VALUES ($1,$2,NULL,$3,$4,$5,$6,$7,$8,$9,$10,$11::jsonb,'system')",
        )
        .bind(Uuid::new_v4())
        .bind(workspace_id)
        .bind(name)
        .bind(slug)
        .bind(ty)
        .bind(icon)
        .bind(multiple)
        .bind(title)
        .bind(desc)
        .bind(assignee)
        .bind(items)
        .execute(&mut **tx)
        .await?;
    }
    Ok(default_id.expect("at least one type"))
}

/// Fields that make a type with a CRM layout useful from the start. They belong to that type only;
/// summary and description are the workspace-wide ones every type shares.
/// (name, slug, type, icon)
pub fn layout_fields(
    layout: &str,
) -> &'static [(&'static str, &'static str, &'static str, &'static str)] {
    match layout {
        "person" => &[
            ("Job title", "job_title", "text", "text"),
            ("Email", "email", "email", "email"),
            ("Phone", "phone", "text", "phone"),
            ("Company", "company", "text", "text"),
        ],
        "organization" => &[
            ("Website", "website", "url", "link"),
            ("Industry", "industry", "text", "text"),
            ("Employees", "employees", "number", "points"),
            ("Phone", "phone", "text", "phone"),
        ],
        "deal" => &[
            ("Amount", "amount", "currency", "currency"),
            ("Probability", "probability", "percentage", "points"),
            ("Expected close", "expected_close", "date", "calendar"),
        ],
        _ => &[],
    }
}

pub async fn fields_for_card_type(
    db: &PgPool,
    workspace_id: Uuid,
    card_type_id: Uuid,
) -> ApiResult<Vec<FieldOut>> {
    let sql = format!("SELECT {FIELD_COLS} FROM fields WHERE workspace_id = $1 AND deleted_at IS NULL AND (card_type_id = $2 OR card_type_id IS NULL) ORDER BY created_at");
    Ok(sqlx::query_as(&sql)
        .bind(workspace_id)
        .bind(card_type_id)
        .fetch_all(db)
        .await?)
}

#[utoipa::path(get, operation_id = "listCardTypes", path = "/workspaces/{id}/card-types", tag = "card-types",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<CardTypeOut>), (status = 404)), security(("bearer" = [])))]
async fn list_card_types(
    State(s): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
) -> ApiResult<Json<Vec<CardTypeOut>>> {
    require_workspace_role(&s.db, user.id, wid, Role::Viewer).await?;
    let sql = format!("SELECT {CT_COLS} FROM card_types WHERE workspace_id = $1 AND deleted_at IS NULL ORDER BY created_at");
    let types: Vec<CardTypeRow> = sqlx::query_as(&sql).bind(wid).fetch_all(&s.db).await?;
    let sql = format!("SELECT {FIELD_COLS} FROM fields WHERE workspace_id = $1 AND deleted_at IS NULL ORDER BY created_at");
    let fields: Vec<FieldOut> = sqlx::query_as(&sql).bind(wid).fetch_all(&s.db).await?;
    let out = types
        .into_iter()
        .map(|t| {
            let f = fields
                .iter()
                .filter(|f| f.card_type_id == Some(t.id) || f.card_type_id.is_none())
                .cloned()
                .collect();
            CardTypeOut {
                card_type: t,
                fields: f,
            }
        })
        .collect();
    Ok(Json(out))
}

#[derive(Deserialize, ToSchema)]
pub struct CardTypeBody {
    pub name: String,
    /// `default`, `person`, `organization` or `deal`
    pub layout: Option<String>,
    pub title_template: Option<String>,
    pub has_children: Option<bool>,
    /// Issue type icon: `epic`, `story`, `task`, `bug`, `subtask`
    pub icon: Option<String>,
    /// Issue type colour: `purple`, `green`, `blue`, `red`, `orange`, `teal`, `grey`
    pub color: Option<String>,
}

const ICONS: &[&str] = &["epic", "story", "task", "bug", "subtask"];
const COLORS: &[&str] = &["purple", "green", "blue", "red", "orange", "teal", "grey"];

fn check_look(icon: &Option<String>, color: &Option<String>) -> ApiResult<()> {
    if icon.as_deref().is_some_and(|i| !ICONS.contains(&i)) {
        return Err(ApiError::BadRequest("unknown icon".into()));
    }
    if color.as_deref().is_some_and(|c| !COLORS.contains(&c)) {
        return Err(ApiError::BadRequest("unknown colour".into()));
    }
    Ok(())
}

fn check_layout(l: &str) -> ApiResult<()> {
    if matches!(l, "default" | "person" | "organization" | "deal") {
        Ok(())
    } else {
        Err(ApiError::BadRequest("unknown layout".into()))
    }
}

#[utoipa::path(post, operation_id = "createCardType", path = "/workspaces/{id}/card-types", tag = "card-types",
    params(("id" = Uuid, Path)), request_body = CardTypeBody,
    responses((status = 200, body = CardTypeRow), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn create_card_type(
    State(s): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
    Json(b): Json<CardTypeBody>,
) -> ApiResult<Json<CardTypeRow>> {
    require_workspace_role(&s.db, user.id, wid, Role::Admin).await?;
    short(&b.name, 100, "name")?;
    let layout = b.layout.as_deref().unwrap_or("default");
    check_layout(layout)?;
    check_look(&b.icon, &b.color)?;
    if let Some(t) = &b.title_template {
        if t.chars().count() > 255 {
            return Err(ApiError::BadRequest("title_template too long".into()));
        }
    }
    let sql = format!("INSERT INTO card_types (id, workspace_id, name, layout, title_template, has_children, icon, color, created_by) VALUES ($1,$2,$3,$4,$5,$6,COALESCE($7,'task'),COALESCE($8,'blue'),$9) RETURNING {CT_COLS}");
    let id = Uuid::new_v4();
    let mut tx = s.db.begin().await?;
    let row: CardTypeRow = sqlx::query_as(&sql)
        .bind(id)
        .bind(wid)
        .bind(b.name.trim())
        .bind(layout)
        .bind(&b.title_template)
        .bind(b.has_children.unwrap_or(false))
        .bind(&b.icon)
        .bind(&b.color)
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
    for (name, slug, ty, icon) in layout_fields(layout) {
        sqlx::query(
            "INSERT INTO fields (id, workspace_id, card_type_id, name, slug, type, icon, created_by, created_by_type, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'system',clock_timestamp())",
        )
        .bind(Uuid::new_v4())
        .bind(wid)
        .bind(id)
        .bind(name)
        .bind(slug)
        .bind(ty)
        .bind(icon)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Json(row))
}

#[utoipa::path(patch, operation_id = "updateCardType", path = "/card-types/{id}", tag = "card-types",
    params(("id" = Uuid, Path)), request_body = CardTypeBody,
    responses((status = 200, body = CardTypeRow), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update_card_type(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<CardTypeBody>,
) -> ApiResult<Json<CardTypeRow>> {
    require_workspace_scoped(&s.db, user.id, "card_types", id, Role::Admin).await?;
    short(&b.name, 100, "name")?;
    if let Some(l) = &b.layout {
        check_layout(l)?;
    }
    check_look(&b.icon, &b.color)?;
    let sql = format!("UPDATE card_types SET name = $2, layout = COALESCE($3, layout), title_template = $4, has_children = COALESCE($5, has_children), icon = COALESCE($6, icon), color = COALESCE($7, color) WHERE id = $1 AND deleted_at IS NULL RETURNING {CT_COLS}");
    let row = sqlx::query_as(&sql)
        .bind(id)
        .bind(b.name.trim())
        .bind(&b.layout)
        .bind(&b.title_template)
        .bind(b.has_children)
        .bind(&b.icon)
        .bind(&b.color)
        .fetch_optional(&s.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(row))
}

#[utoipa::path(delete, operation_id = "deleteCardType", path = "/card-types/{id}", tag = "card-types",
    params(("id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 403), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn delete_card_type(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    require_workspace_scoped(&s.db, user.id, "card_types", id, Role::Admin).await?;
    let in_use: (i64,) =
        sqlx::query_as("SELECT count(*) FROM cards WHERE card_type_id = $1 AND deleted_at IS NULL")
            .bind(id)
            .fetch_one(&s.db)
            .await?;
    if in_use.0 > 0 {
        return Err(ApiError::Conflict);
    }
    sqlx::query("UPDATE card_types SET deleted_at = now() WHERE id = $1")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(Json(OkOut { ok: true }))
}

#[derive(Deserialize)]
pub struct FieldQuery {
    pub card_type_id: Option<Uuid>,
}

#[utoipa::path(get, operation_id = "listFields", path = "/workspaces/{id}/fields", tag = "fields",
    params(("id" = Uuid, Path), ("card_type_id" = Option<Uuid>, Query)),
    responses((status = 200, body = Vec<FieldOut>), (status = 404)), security(("bearer" = [])))]
async fn list_fields(
    State(s): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
    Query(q): Query<FieldQuery>,
) -> ApiResult<Json<Vec<FieldOut>>> {
    require_workspace_role(&s.db, user.id, wid, Role::Viewer).await?;
    let sql = format!("SELECT {FIELD_COLS} FROM fields WHERE workspace_id = $1 AND deleted_at IS NULL AND ($2::uuid IS NULL OR card_type_id = $2 OR card_type_id IS NULL) ORDER BY created_at");
    Ok(Json(
        sqlx::query_as(&sql)
            .bind(wid)
            .bind(q.card_type_id)
            .fetch_all(&s.db)
            .await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct FieldBody {
    pub name: String,
    pub slug: Option<String>,
    pub r#type: String,
    pub icon: Option<String>,
    pub required: Option<bool>,
    pub multiple: Option<bool>,
    #[schema(value_type = Option<Object>)]
    pub items: Option<Value>,
    pub card_type_id: Option<Uuid>,
    pub data_card_type_id: Option<Uuid>,
    pub is_title: Option<bool>,
    pub is_description: Option<bool>,
    pub is_photo: Option<bool>,
    pub is_assignee: Option<bool>,
    pub is_pinned: Option<bool>,
}

fn check_items(items: &Option<Value>) -> ApiResult<()> {
    let Some(v) = items else { return Ok(()) };
    let arr = v
        .as_array()
        .ok_or_else(|| ApiError::BadRequest("items must be an array".into()))?;
    if arr.len() > 200 {
        return Err(ApiError::BadRequest("at most 200 items".into()));
    }
    for it in arr {
        let ok = it
            .get("item")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty() && s.len() <= 100);
        if !ok {
            return Err(ApiError::BadRequest(
                "every item needs a non-empty `item` string (max 100)".into(),
            ));
        }
    }
    Ok(())
}

#[utoipa::path(post, operation_id = "createField", path = "/workspaces/{id}/fields", tag = "fields",
    params(("id" = Uuid, Path)), request_body = FieldBody,
    responses((status = 200, body = FieldOut), (status = 403), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn create_field(
    State(s): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
    Json(b): Json<FieldBody>,
) -> ApiResult<Json<FieldOut>> {
    require_workspace_role(&s.db, user.id, wid, Role::Admin).await?;
    short(&b.name, 100, "name")?;
    if !FIELD_TYPES.contains(&b.r#type.as_str()) {
        return Err(ApiError::BadRequest("unknown field type".into()));
    }
    check_items(&b.items)?;
    for ct in [b.card_type_id, b.data_card_type_id].into_iter().flatten() {
        let ok: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM card_types WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        )
        .bind(ct)
        .bind(wid)
        .fetch_optional(&s.db)
        .await?;
        if ok.is_none() {
            return Err(ApiError::BadRequest(
                "card type not found in this workspace".into(),
            ));
        }
    }
    let slug = field_slug(b.slug.as_deref().unwrap_or(&b.name));
    // Card data is keyed by slug, so a slug must be unambiguous across workspace-wide and per-type fields.
    let clash: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM fields WHERE workspace_id = $1 AND slug = $2 AND deleted_at IS NULL
           AND ($3::uuid IS NULL OR card_type_id IS NULL OR card_type_id = $3)",
    )
    .bind(wid)
    .bind(&slug)
    .bind(b.card_type_id)
    .fetch_one(&s.db)
    .await?;
    if clash.0 > 0 {
        return Err(ApiError::Conflict);
    }
    if b.is_title == Some(true) {
        let titles: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM fields WHERE workspace_id = $1 AND is_title AND deleted_at IS NULL AND (card_type_id IS NULL OR card_type_id = $2)",
        )
        .bind(wid)
        .bind(b.card_type_id)
        .fetch_one(&s.db)
        .await?;
        if titles.0 > 0 {
            return Err(ApiError::Conflict);
        }
    }
    let sql = format!(
        "INSERT INTO fields (id, workspace_id, card_type_id, data_card_type_id, name, slug, type, icon, required, multiple, items, is_title, is_description, is_photo, is_assignee, is_pinned, created_by, created_by_type)
         VALUES ($1,$2,$3,$4,$5,$6,$7,COALESCE($8,'text'),$9,$10,$11,$12,$13,$14,$15,$16,$17,'user') RETURNING {FIELD_COLS}");
    let row = sqlx::query_as(&sql)
        .bind(Uuid::new_v4())
        .bind(wid)
        .bind(b.card_type_id)
        .bind(b.data_card_type_id)
        .bind(b.name.trim())
        .bind(&slug)
        .bind(&b.r#type)
        .bind(&b.icon)
        .bind(b.required.unwrap_or(false))
        .bind(b.multiple.unwrap_or(false))
        .bind(b.items.map(SqlJson))
        .bind(b.is_title.unwrap_or(false))
        .bind(b.is_description.unwrap_or(false))
        .bind(b.is_photo.unwrap_or(false))
        .bind(b.is_assignee.unwrap_or(false))
        .bind(b.is_pinned.unwrap_or(false))
        .bind(user.id)
        .fetch_one(&s.db)
        .await?; // duplicate slug / second title field -> 409
    Ok(Json(row))
}

/// Slug and type are immutable: card data is keyed by slug and validated by type.
#[derive(Deserialize, ToSchema)]
pub struct FieldPatch {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub required: Option<bool>,
    pub multiple: Option<bool>,
    #[schema(value_type = Option<Object>)]
    pub items: Option<Value>,
    pub is_pinned: Option<bool>,
}

#[utoipa::path(patch, operation_id = "updateField", path = "/fields/{id}", tag = "fields",
    params(("id" = Uuid, Path)), request_body = FieldPatch,
    responses((status = 200, body = FieldOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update_field(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<FieldPatch>,
) -> ApiResult<Json<FieldOut>> {
    require_workspace_scoped(&s.db, user.id, "fields", id, Role::Admin).await?;
    if let Some(n) = &b.name {
        short(n, 100, "name")?;
    }
    check_items(&b.items)?;
    let sql = format!("UPDATE fields SET name = COALESCE($2,name), icon = COALESCE($3,icon), required = COALESCE($4,required), multiple = COALESCE($5,multiple), items = COALESCE($6,items), is_pinned = COALESCE($7,is_pinned) WHERE id = $1 AND deleted_at IS NULL RETURNING {FIELD_COLS}");
    let row = sqlx::query_as(&sql)
        .bind(id)
        .bind(b.name.as_deref().map(str::trim))
        .bind(&b.icon)
        .bind(b.required)
        .bind(b.multiple)
        .bind(b.items.map(SqlJson))
        .bind(b.is_pinned)
        .fetch_optional(&s.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(row))
}

#[utoipa::path(delete, operation_id = "deleteField", path = "/fields/{id}", tag = "fields",
    params(("id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn delete_field(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    require_workspace_scoped(&s.db, user.id, "fields", id, Role::Admin).await?;
    sqlx::query("UPDATE fields SET deleted_at = now() WHERE id = $1")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(Json(OkOut { ok: true }))
}
