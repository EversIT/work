use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::{types::Json as SqlJson, Postgres, QueryBuilder};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::{
    card_types::{fields_for_card_type, FieldOut},
    OkOut,
};
use crate::{
    domain::filters::{self, Ctx},
    error::{ApiError, ApiResult},
    security::auth::AuthUser,
    security::authz::{require_card_role, require_list_role, require_role, Role, Scope},
    services::events::{emit, Event},
    state::AppState,
    util::clamp_page,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_card))
        .routes(routes!(search_cards))
        .routes(routes!(get_card, update_card, delete_card))
        .routes(routes!(card_children))
        .routes(routes!(put_card_list, remove_card_list))
        .routes(routes!(workspace_card_search))
        .routes(routes!(card_by_key))
        .routes(routes!(resolve_issue))
        .routes(routes!(my_issues))
        .routes(routes!(put_card_sprint))
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct CardListOut {
    pub id: Uuid,
    pub list_id: Uuid,
    pub list_stage_id: Uuid,
    pub sort_order: i32,
    pub sprint_id: Option<Uuid>,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct CardOut {
    pub id: Uuid,
    /// Issue number within the workspace.
    pub number: i32,
    /// Issue key such as `WORK-12`.
    pub key: String,
    pub workspace_id: Uuid,
    pub card_type_id: Uuid,
    pub parent_id: Option<Uuid>,
    #[schema(value_type = Object)]
    pub data: SqlJson<Value>,
    pub created_by: Option<Uuid>,
    pub created_by_type: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[schema(value_type = Vec<CardListOut>)]
    pub lists: SqlJson<Vec<CardListOut>>,
    pub children_count: i64,
}

pub const CARD_COLS: &str = "c.id, c.number, (SELECT w.key_prefix || '-' || c.number FROM workspaces w WHERE w.id = c.workspace_id) AS key, c.workspace_id, c.card_type_id, c.parent_id, c.data, c.created_by, c.created_by_type, c.created_at, c.updated_at,
  COALESCE((SELECT json_agg(json_build_object('id', cl2.id, 'list_id', cl2.list_id, 'list_stage_id', cl2.list_stage_id, 'sort_order', cl2.sort_order, 'sprint_id', cl2.sprint_id) ORDER BY cl2.created_at)
            FROM card_lists cl2 WHERE cl2.card_id = c.id), '[]'::json) AS lists,
  (SELECT count(*) FROM cards ch WHERE ch.parent_id = c.id AND ch.deleted_at IS NULL) AS children_count";

pub async fn load_card(s: &AppState, id: Uuid) -> ApiResult<CardOut> {
    let sql = format!("SELECT {CARD_COLS} FROM cards c WHERE c.id = $1 AND c.deleted_at IS NULL");
    sqlx::query_as(&sql)
        .bind(id)
        .fetch_optional(&s.db)
        .await?
        .ok_or(ApiError::NotFound)
}

// ---- data validation ------------------------------------------------------------------------

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError::BadRequest(msg.into())
}

fn string_values(v: &Value, name: &str) -> ApiResult<Vec<String>> {
    match v {
        Value::String(s) => Ok(vec![s.clone()]),
        Value::Array(a) => a
            .iter()
            .map(|x| {
                x.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| bad(format!("{name}: expected strings")))
            })
            .collect(),
        _ => Err(bad(format!(
            "{name}: expected a string or a list of strings"
        ))),
    }
}

/// Validates card data against the field definitions. Unknown keys are rejected so card data
/// cannot be used to store arbitrary blobs, and so filters/sorts only ever see known keys.
pub async fn validate_data(
    s: &AppState,
    scope: &Scope,
    fields: &[FieldOut],
    data: &Map<String, Value>,
    check_required: bool,
) -> ApiResult<()> {
    let by_slug: HashMap<&str, &FieldOut> = fields.iter().map(|f| (f.slug.as_str(), f)).collect();
    if data.len() > 200 || Value::Object(data.clone()).to_string().len() > 512 * 1024 {
        return Err(bad("card data too large"));
    }
    let (mut users, mut cards): (Vec<Uuid>, Vec<Uuid>) = (vec![], vec![]);
    for (slug, v) in data {
        let f = by_slug.get(slug.as_str()).ok_or_else(|| {
            bad(format!(
                "unknown field `{}`",
                slug.chars().take(40).collect::<String>()
            ))
        })?;
        if v.is_null() {
            continue;
        }
        match f.r#type.as_str() {
            "text" => {
                if v.as_str().is_none_or(|s| s.chars().count() > 500) {
                    return Err(bad(format!("{slug}: text up to 500 characters")));
                }
            }
            "text_area" => {
                if v.as_str().is_none_or(|s| s.chars().count() > 20_000) {
                    return Err(bad(format!("{slug}: text up to 20000 characters")));
                }
            }
            "email" => {
                if v.as_str().is_none_or(|s| {
                    s.len() > 254 || !s.contains('@') || s.contains(char::is_whitespace)
                }) {
                    return Err(bad(format!("{slug}: invalid email")));
                }
            }
            "url" => {
                if v.as_str().is_none_or(|s| {
                    s.len() > 2000 || !(s.starts_with("https://") || s.starts_with("http://"))
                }) {
                    return Err(bad(format!("{slug}: only http(s) urls")));
                }
            }
            "rich" => {
                // Plain text (older content) or a document the editor could have produced.
                crate::domain::rich::validate_value(v).map_err(|e| bad(format!("{slug}: {e}")))?;
            }
            "number" | "percentage" | "currency" => {
                if !v.as_f64().is_some_and(f64::is_finite) {
                    return Err(bad(format!("{slug}: expected a number")));
                }
            }
            "checkbox" => {
                if !v.is_boolean() {
                    return Err(bad(format!("{slug}: expected true/false")));
                }
            }
            "date" | "date_time" => {
                let ok = v.as_str().is_some_and(|s| {
                    DateTime::parse_from_rfc3339(s).is_ok()
                        || NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
                });
                if !ok {
                    return Err(bad(format!("{slug}: expected an ISO date")));
                }
            }
            "dropdown" | "label" => {
                let vals = string_values(v, slug)?;
                if !f.multiple && vals.len() > 1 {
                    return Err(bad(format!("{slug}: only one value allowed")));
                }
                if let Some(items) = f
                    .items
                    .as_ref()
                    .and_then(|i| i.as_array())
                    .filter(|a| !a.is_empty())
                {
                    let allowed: Vec<&str> = items
                        .iter()
                        .filter_map(|i| i.get("item").and_then(Value::as_str))
                        .collect();
                    if vals.iter().any(|x| !allowed.contains(&x.as_str())) {
                        return Err(bad(format!("{slug}: value is not one of the options")));
                    }
                }
            }
            "user" => {
                let vals = string_values(v, slug)?;
                if !f.multiple && vals.len() > 1 {
                    return Err(bad(format!("{slug}: only one value allowed")));
                }
                for x in vals {
                    users.push(
                        Uuid::parse_str(&x).map_err(|_| bad(format!("{slug}: invalid user id")))?,
                    );
                }
            }
            "card" => {
                let vals = string_values(v, slug)?;
                for x in vals {
                    cards.push(
                        Uuid::parse_str(&x).map_err(|_| bad(format!("{slug}: invalid card id")))?,
                    );
                }
            }
            "stage" => {
                string_values(v, slug)?;
            }
            _ => return Err(bad(format!("{slug}: unsupported field type"))),
        }
    }
    if !users.is_empty() {
        users.sort();
        users.dedup();
        let n: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM project_members WHERE project_id = $1 AND user_id = ANY($2)",
        )
        .bind(scope.project_id)
        .bind(&users)
        .fetch_one(&s.db)
        .await?;
        if n.0 as usize != users.len() {
            return Err(bad("a referenced user is not a member of this project"));
        }
    }
    if !cards.is_empty() {
        cards.sort();
        cards.dedup();
        let n: (i64,) = sqlx::query_as("SELECT count(*) FROM cards WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL")
            .bind(scope.workspace_id).bind(&cards).fetch_one(&s.db).await?;
        if n.0 as usize != cards.len() {
            return Err(bad("a referenced card does not exist in this workspace"));
        }
    }
    if check_required {
        for f in fields.iter().filter(|f| f.required) {
            if data.get(&f.slug).is_none_or(Value::is_null) {
                return Err(bad(format!("field `{}` is required", f.slug)));
            }
        }
    }
    Ok(())
}

async fn field_types(s: &AppState, workspace_id: Uuid) -> ApiResult<HashMap<String, String>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT slug, type FROM fields WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(workspace_id)
    .fetch_all(&s.db)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Empty values mean "not set": `null`, `[]` and `""` are all stored as an absent key, so
/// `isNull`/`isNotNull` filters and required-field checks behave the same for every client.
pub fn normalize_data(data: Map<String, Value>, keep_nulls: bool) -> Map<String, Value> {
    data.into_iter()
        .map(|(k, v)| {
            let empty = match &v {
                Value::Array(a) => a.is_empty(),
                Value::String(s) => s.is_empty(),
                _ => false,
            };
            (k, if empty { Value::Null } else { v })
        })
        .filter(|(_, v)| keep_nulls || !v.is_null())
        .collect()
}

// ---- create ---------------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct CreateCardBody {
    /// Defaults to the list's default card type.
    pub card_type_id: Option<Uuid>,
    #[schema(value_type = Object)]
    pub data: Map<String, Value>,
    /// Defaults to the first stage of the list.
    pub stage_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
}

/// Shared by the HTTP handler and the automation engine.
pub async fn create_card_in_list(
    s: &AppState,
    list_id: Uuid,
    scope: &Scope,
    b: CreateCardBody,
    actor: Option<Uuid>,
    by_system: bool,
    depth: u8,
) -> ApiResult<CardOut> {
    let ct: Uuid = match b.card_type_id {
        Some(ct) => ct,
        None => {
            let d: (Option<Uuid>,) =
                sqlx::query_as("SELECT default_card_type_id FROM lists WHERE id = $1")
                    .bind(list_id)
                    .fetch_one(&s.db)
                    .await?;
            d.0.ok_or_else(|| bad("list has no default card type"))?
        }
    };
    let ok: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM card_types WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
    )
    .bind(ct)
    .bind(scope.workspace_id)
    .fetch_optional(&s.db)
    .await?;
    if ok.is_none() {
        return Err(bad("card type not found in this workspace"));
    }
    if let Some(p) = b.parent_id {
        let ok: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM cards WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        )
        .bind(p)
        .bind(scope.workspace_id)
        .fetch_optional(&s.db)
        .await?;
        if ok.is_none() {
            return Err(bad("parent card not found in this workspace"));
        }
    }
    let stage: Uuid = match b.stage_id {
        Some(st) => {
            let ok: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM list_stages WHERE id = $1 AND list_id = $2")
                    .bind(st)
                    .bind(list_id)
                    .fetch_optional(&s.db)
                    .await?;
            ok.ok_or_else(|| bad("stage does not belong to this list"))?
                .0
        }
        None => {
            let f: Option<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM list_stages WHERE list_id = $1 ORDER BY sort_order LIMIT 1",
            )
            .bind(list_id)
            .fetch_optional(&s.db)
            .await?;
            f.ok_or_else(|| bad("list has no stages"))?.0
        }
    };
    let fields = fields_for_card_type(&s.db, scope.workspace_id, ct).await?;
    let data = normalize_data(b.data, false);
    validate_data(s, scope, &fields, &data, true).await?;

    let id = Uuid::new_v4();
    let mut tx = s.db.begin().await?;
    // The row lock on the workspace serializes concurrent creators, so numbers are gap-free and unique.
    let (number,): (i32,) = sqlx::query_as("UPDATE workspaces SET next_issue_number = next_issue_number + 1 WHERE id = $1 RETURNING next_issue_number - 1")
        .bind(scope.workspace_id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO cards (id, number, workspace_id, card_type_id, parent_id, data, created_by, created_by_type) VALUES ($1,$8,$2,$3,$4,$5,$6,$7)")
        .bind(id).bind(scope.workspace_id).bind(ct).bind(b.parent_id).bind(SqlJson(Value::Object(data)))
        .bind(actor).bind(if by_system { "system" } else { "user" }).bind(number).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO card_lists (id, card_id, list_id, list_stage_id, sort_order)
                 VALUES ($1,$2,$3,$4,(SELECT COALESCE(max(sort_order),0)+100 FROM card_lists WHERE list_id = $3 AND list_stage_id = $4))")
        .bind(Uuid::new_v4()).bind(id).bind(list_id).bind(stage).execute(&mut *tx).await?;
    tx.commit().await?;
    emit(
        s,
        Event::CardCreated {
            card_id: id,
            workspace_id: scope.workspace_id,
            list_id,
            depth,
        },
    );
    load_card(s, id).await
}

#[utoipa::path(post, operation_id = "createCard", path = "/lists/{id}/cards", tag = "cards",
    params(("id" = Uuid, Path)), request_body = CreateCardBody,
    responses((status = 200, body = CardOut), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn create_card(
    State(s): State<AppState>,
    user: AuthUser,
    Path(list_id): Path<Uuid>,
    Json(b): Json<CreateCardBody>,
) -> ApiResult<Json<CardOut>> {
    let scope = require_list_role(&s.db, user.id, list_id, Role::Member).await?;
    Ok(Json(
        create_card_in_list(&s, list_id, &scope, b, Some(user.id), false, 0).await?,
    ))
}

// ---- search ---------------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct SearchBody {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    /// `order` (default), `created_at`, `updated_at` or `data.<field slug>`
    pub sort_by: Option<String>,
    /// `asc` (default) or `desc`
    pub sort_order: Option<String>,
    pub hide_completed: Option<bool>,
    pub hide_children: Option<bool>,
    /// Filter tree, see the filter documentation: `{"and":[{"field":"data.title","operator":"like","value":"x"}]}`
    #[schema(value_type = Option<Object>)]
    pub filters: Option<Value>,
    /// Case-insensitive match on the issue title or key (`WORK-12`).
    pub keyword: Option<String>,
    /// Scrum lists: `backlog` (not in any sprint), `active` (the running sprint) or a sprint id.
    pub sprint: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CardPage {
    pub total: i64,
    pub cards: Vec<CardOut>,
}

fn push_list_base<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    list_id: Uuid,
    b: &SearchBody,
    ctx: &Ctx<'_>,
    parsed: &Option<filters::Group>,
) -> ApiResult<()> {
    qb.push(" FROM cards c JOIN card_lists cl ON cl.card_id = c.id AND cl.list_id = ")
        .push_bind(list_id)
        .push(" JOIN list_stages st ON st.id = cl.list_stage_id WHERE c.deleted_at IS NULL");
    if b.hide_completed.unwrap_or(false) {
        qb.push(" AND NOT st.is_completed");
    }
    if b.hide_children.unwrap_or(false) {
        qb.push(" AND c.parent_id IS NULL");
    }
    if let Some(kw) = b.keyword.as_deref().filter(|k| !k.trim().is_empty()) {
        if kw.chars().count() > 200 {
            return Err(bad("keyword too long"));
        }
        let pat = format!(
            "%{}%",
            kw.trim()
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        qb.push(" AND (EXISTS (SELECT 1 FROM fields tf WHERE (tf.card_type_id = c.card_type_id OR tf.card_type_id IS NULL) AND tf.is_title AND tf.deleted_at IS NULL AND (c.data ->> tf.slug) ILIKE ")
            .push_bind(pat.clone())
            .push(" ESCAPE '\\') OR (SELECT w.key_prefix || '-' || c.number FROM workspaces w WHERE w.id = c.workspace_id) ILIKE ")
            .push_bind(pat)
            .push(" ESCAPE '\\')");
    }
    match b.sprint.as_deref() {
        None => {}
        Some("backlog") => {
            qb.push(" AND cl.sprint_id IS NULL");
        }
        Some("active") => {
            qb.push(" AND cl.sprint_id IN (SELECT id FROM sprints WHERE list_id = cl.list_id AND state = 'active')");
        }
        Some(other) => {
            let id = Uuid::parse_str(other)
                .map_err(|_| bad("sprint must be backlog, active or a sprint id"))?;
            qb.push(" AND cl.sprint_id = ").push_bind(id);
        }
    }
    if let Some(g) = parsed {
        qb.push(" AND ");
        filters::push_group(qb, g, ctx)?;
    }
    Ok(())
}

#[utoipa::path(post, operation_id = "searchCards", path = "/lists/{id}/cards/search", tag = "cards",
    params(("id" = Uuid, Path)), request_body = SearchBody,
    responses((status = 200, body = CardPage), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn search_cards(
    State(s): State<AppState>,
    user: AuthUser,
    Path(list_id): Path<Uuid>,
    Json(b): Json<SearchBody>,
) -> ApiResult<Json<CardPage>> {
    let scope = require_list_role(&s.db, user.id, list_id, Role::Viewer).await?;
    let (page, limit) = clamp_page(b.page, b.limit)?;
    let types = field_types(&s, scope.workspace_id).await?;
    let ctx = Ctx {
        field_types: &types,
    };
    let parsed = match &b.filters {
        Some(f) => Some(filters::parse_root(f)?),
        None => None,
    };

    let mut count = QueryBuilder::<Postgres>::new("SELECT count(*)");
    push_list_base(&mut count, list_id, &b, &ctx, &parsed)?;
    let total: (i64,) = count.build_query_as().fetch_one(&s.db).await?;

    let mut qb = QueryBuilder::<Postgres>::new(format!("SELECT {CARD_COLS}"));
    push_list_base(&mut qb, list_id, &b, &ctx, &parsed)?;
    let dir = match b.sort_order.as_deref().unwrap_or("asc") {
        "asc" => " ASC",
        "desc" => " DESC",
        _ => return Err(bad("sort_order must be asc or desc")),
    };
    match b.sort_by.as_deref().unwrap_or("order") {
        "order" => {
            qb.push(" ORDER BY cl.sort_order").push(dir);
        }
        "created_at" => {
            qb.push(" ORDER BY c.created_at").push(dir);
        }
        "updated_at" => {
            qb.push(" ORDER BY c.updated_at").push(dir);
        }
        other => {
            let slug = other
                .strip_prefix("data.")
                .ok_or_else(|| bad("unknown sort_by"))?;
            let ty = types.get(slug).ok_or_else(|| bad("unknown sort field"))?;
            if matches!(ty.as_str(), "number" | "percentage" | "currency") {
                qb.push(" ORDER BY (c.data ->> ")
                    .push_bind(slug.to_owned())
                    .push(")::numeric")
                    .push(dir);
            } else {
                qb.push(" ORDER BY (c.data ->> ")
                    .push_bind(slug.to_owned())
                    .push(")")
                    .push(dir);
            }
        }
    }
    qb.push(" NULLS LAST, c.id LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind((page - 1) * limit);
    let cards: Vec<CardOut> = qb.build_query_as().fetch_all(&s.db).await?;
    Ok(Json(CardPage {
        total: total.0,
        cards,
    }))
}

// ---- read / update / delete -----------------------------------------------------------------

#[utoipa::path(get, operation_id = "getCard", path = "/cards/{id}", tag = "cards",
    params(("id" = Uuid, Path)), responses((status = 200, body = CardOut), (status = 404)), security(("bearer" = [])))]
async fn get_card(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<CardOut>> {
    require_card_role(&s.db, user.id, id, Role::Viewer).await?;
    Ok(Json(load_card(&s, id).await?))
}

#[utoipa::path(get, operation_id = "getCardChildren", path = "/cards/{id}/children", tag = "cards",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<CardOut>), (status = 404)), security(("bearer" = [])))]
async fn card_children(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<CardOut>>> {
    let scope = require_card_role(&s.db, user.id, id, Role::Viewer).await?;
    let role = require_role(&s.db, user.id, scope.project_id, Role::Viewer).await?;
    // A child can sit in lists the caller has no access to.
    let vis = crate::security::authz::visible_cards_sql("c", 2, 3);
    let sql = format!("SELECT {CARD_COLS} FROM cards c WHERE c.parent_id = $1 AND c.deleted_at IS NULL AND {vis} ORDER BY c.created_at LIMIT 500");
    Ok(Json(
        sqlx::query_as(&sql)
            .bind(id)
            .bind(role >= Role::Admin)
            .bind(user.id)
            .fetch_all(&s.db)
            .await?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateCardBody {
    /// Merge patch: keys are set, `null` clears a key. Unknown keys are rejected.
    #[schema(value_type = Option<Object>)]
    pub data: Option<Map<String, Value>>,
    pub parent_id: Option<Uuid>,
    /// Rich-text fields this save belongs to a live collaborative editing session of the caller.
    /// Writes to any other rich field end the field's session; see `collab.rs`.
    pub collab_keep: Option<Vec<String>>,
}

/// Shared by the HTTP handler and the automation engine. Returns the keys that changed.
pub async fn update_card_data(
    s: &AppState,
    card_id: Uuid,
    scope: &Scope,
    patch: Map<String, Value>,
    actor: Option<Uuid>,
    by_system: bool,
    depth: u8,
) -> ApiResult<CardOut> {
    update_card_data_keeping(s, card_id, scope, patch, actor, by_system, depth, &[]).await
}

/// Like `update_card_data`. `keep_collab` names rich fields whose collaborative session this write
/// belongs to (an editor saving its own document); for every other changed rich field the session
/// document is discarded, because it no longer matches the card.
#[allow(clippy::too_many_arguments)]
pub async fn update_card_data_keeping(
    s: &AppState,
    card_id: Uuid,
    scope: &Scope,
    patch: Map<String, Value>,
    actor: Option<Uuid>,
    by_system: bool,
    depth: u8,
    keep_collab: &[String],
) -> ApiResult<CardOut> {
    let mut tx = s.db.begin().await?;
    let row: Option<(Uuid, SqlJson<Value>)> = sqlx::query_as(
        "SELECT card_type_id, data FROM cards WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(card_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (ct, SqlJson(old)) = row.ok_or(ApiError::NotFound)?;
    let fields = fields_for_card_type(&s.db, scope.workspace_id, ct).await?;
    validate_data(s, scope, &fields, &patch, false).await?;

    let mut new = old.as_object().cloned().unwrap_or_default();
    let mut changes = vec![];
    for (k, v) in patch {
        let before = new.get(&k).cloned().unwrap_or(Value::Null);
        if before == v {
            continue;
        }
        if v.is_null() {
            new.remove(&k);
        } else {
            new.insert(k.clone(), v.clone());
        }
        changes.push((k, before, v));
    }
    // Required fields must stay filled.
    for f in fields.iter().filter(|f| f.required) {
        if new.get(&f.slug).is_none_or(Value::is_null) {
            return Err(bad(format!("field `{}` is required", f.slug)));
        }
    }
    if !changes.is_empty() {
        sqlx::query("UPDATE cards SET data = $2, updated_at = now() WHERE id = $1")
            .bind(card_id)
            .bind(SqlJson(Value::Object(new)))
            .execute(&mut *tx)
            .await?;
        let diff: Vec<Value> = changes
            .iter()
            .map(|(k, a, b)| serde_json::json!({"field": k, "from": a, "to": b}))
            .collect();
        sqlx::query("INSERT INTO card_activities (id, card_id, type, content, created_by, created_by_type) VALUES ($1,$2,'update',$3,$4,$5)")
            .bind(Uuid::new_v4()).bind(card_id).bind(SqlJson(serde_json::json!({"changes": diff}))).bind(actor).bind(if by_system { "system" } else { "user" })
            .execute(&mut *tx).await?;
    }
    tx.commit().await?;
    let stale: Vec<String> = changes
        .iter()
        .map(|(k, ..)| k.clone())
        .filter(|k| {
            !keep_collab.contains(k) && fields.iter().any(|f| &f.slug == k && f.r#type == "rich")
        })
        .collect();
    crate::services::collab::discard(s, card_id, &stale).await?;
    for (slug, old, new) in changes {
        emit(
            s,
            Event::FieldUpdated {
                card_id,
                workspace_id: scope.workspace_id,
                slug,
                old,
                new,
                actor,
                depth,
            },
        );
    }
    load_card(s, card_id).await
}

#[utoipa::path(patch, operation_id = "updateCard", path = "/cards/{id}", tag = "cards",
    params(("id" = Uuid, Path)), request_body = UpdateCardBody,
    responses((status = 200, body = CardOut), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update_card(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<UpdateCardBody>,
) -> ApiResult<Json<CardOut>> {
    let scope = require_card_role(&s.db, user.id, id, Role::Member).await?;
    if let Some(p) = b.parent_id {
        if p == id {
            return Err(bad("a card cannot be its own parent"));
        }
        let ok: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM cards WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
        )
        .bind(p)
        .bind(scope.workspace_id)
        .fetch_optional(&s.db)
        .await?;
        if ok.is_none() {
            return Err(bad("parent card not found in this workspace"));
        }
        // No cycles: the new parent must not be a descendant of this card.
        let cyc: (bool,) = sqlx::query_as(
            "WITH RECURSIVE d AS (SELECT id FROM cards WHERE parent_id = $1 UNION SELECT c.id FROM cards c JOIN d ON c.parent_id = d.id)
             SELECT EXISTS (SELECT 1 FROM d WHERE id = $2)").bind(id).bind(p).fetch_one(&s.db).await?;
        if cyc.0 {
            return Err(bad("parent would create a cycle"));
        }
        sqlx::query("UPDATE cards SET parent_id = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(p)
            .execute(&s.db)
            .await?;
    }
    if let Some(patch) = b.data {
        return Ok(Json(
            update_card_data_keeping(
                &s,
                id,
                &scope,
                patch,
                Some(user.id),
                false,
                0,
                &b.collab_keep.unwrap_or_default(),
            )
            .await?,
        ));
    }
    Ok(Json(load_card(&s, id).await?))
}

#[utoipa::path(delete, operation_id = "deleteCard", path = "/cards/{id}", tag = "cards",
    params(("id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn delete_card(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    require_card_role(&s.db, user.id, id, Role::Member).await?;
    sqlx::query(
        "WITH RECURSIVE d AS (SELECT id FROM cards WHERE id = $1 UNION SELECT c.id FROM cards c JOIN d ON c.parent_id = d.id)
         UPDATE cards SET deleted_at = now() WHERE id IN (SELECT id FROM d) AND deleted_at IS NULL").bind(id).execute(&s.db).await?;
    Ok(Json(OkOut { ok: true }))
}

// ---- list membership / stage moves ----------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct CardListBody {
    pub stage_id: Uuid,
    /// Position within the stage (lower first). Defaults to the end.
    pub order: Option<i32>,
}

/// Shared with automations.
#[allow(clippy::too_many_arguments)]
pub async fn set_card_stage(
    s: &AppState,
    card_id: Uuid,
    list_id: Uuid,
    scope: &Scope,
    stage_id: Uuid,
    order: Option<i32>,
    actor: Option<Uuid>,
    depth: u8,
) -> ApiResult<()> {
    let ok: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM list_stages WHERE id = $1 AND list_id = $2")
            .bind(stage_id)
            .bind(list_id)
            .fetch_optional(&s.db)
            .await?;
    if ok.is_none() {
        return Err(bad("stage does not belong to this list"));
    }
    let prev: Option<(Uuid,)> =
        sqlx::query_as("SELECT list_stage_id FROM card_lists WHERE card_id = $1 AND list_id = $2")
            .bind(card_id)
            .bind(list_id)
            .fetch_optional(&s.db)
            .await?;
    sqlx::query(
        "INSERT INTO card_lists (id, card_id, list_id, list_stage_id, sort_order)
         VALUES ($1,$2,$3,$4,COALESCE($5,(SELECT COALESCE(max(sort_order),0)+100 FROM card_lists WHERE list_id = $3 AND list_stage_id = $4)))
         ON CONFLICT (card_id, list_id) DO UPDATE SET list_stage_id = EXCLUDED.list_stage_id, sort_order = EXCLUDED.sort_order",
    )
    .bind(Uuid::new_v4()).bind(card_id).bind(list_id).bind(stage_id).bind(order).execute(&s.db).await?;
    sqlx::query("UPDATE cards SET updated_at = now() WHERE id = $1")
        .bind(card_id)
        .execute(&s.db)
        .await?;
    if prev.map(|p| p.0) != Some(stage_id) {
        emit(
            s,
            Event::StageUpdated {
                card_id,
                workspace_id: scope.workspace_id,
                list_id,
                from: prev.map(|p| p.0),
                to: stage_id,
                actor,
                depth,
            },
        );
    }
    Ok(())
}

#[utoipa::path(put, operation_id = "putCardList", path = "/cards/{id}/lists/{list_id}", tag = "cards",
    params(("id" = Uuid, Path), ("list_id" = Uuid, Path)), request_body = CardListBody,
    responses((status = 200, body = CardOut), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn put_card_list(
    State(s): State<AppState>,
    user: AuthUser,
    Path((id, list_id)): Path<(Uuid, Uuid)>,
    Json(b): Json<CardListBody>,
) -> ApiResult<Json<CardOut>> {
    let card_scope = require_card_role(&s.db, user.id, id, Role::Member).await?;
    let list_scope = require_list_role(&s.db, user.id, list_id, Role::Member).await?;
    if card_scope.workspace_id != list_scope.workspace_id {
        return Err(ApiError::NotFound);
    }
    set_card_stage(
        &s,
        id,
        list_id,
        &list_scope,
        b.stage_id,
        b.order,
        Some(user.id),
        0,
    )
    .await?;
    Ok(Json(load_card(&s, id).await?))
}

#[utoipa::path(delete, operation_id = "removeCardList", path = "/cards/{id}/lists/{list_id}", tag = "cards",
    params(("id" = Uuid, Path), ("list_id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn remove_card_list(
    State(s): State<AppState>,
    user: AuthUser,
    Path((id, list_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<OkOut>> {
    require_card_role(&s.db, user.id, id, Role::Member).await?;
    require_list_role(&s.db, user.id, list_id, Role::Member).await?;
    sqlx::query(
        "DELETE FROM card_lists cl USING cards c, lists l
         WHERE cl.card_id = $1 AND cl.list_id = $2 AND c.id = cl.card_id AND l.id = cl.list_id
           AND c.workspace_id = l.workspace_id",
    )
    .bind(id)
    .bind(list_id)
    .execute(&s.db)
    .await?;
    Ok(Json(OkOut { ok: true }))
}

// ---- workspace-wide search (card pickers, mentions) -----------------------------------------

#[derive(Deserialize)]
pub struct WsSearchQuery {
    pub keyword: String,
    pub card_type_id: Option<Uuid>,
}

#[utoipa::path(get, operation_id = "searchWorkspaceCards", path = "/workspaces/{id}/cards/search", tag = "cards",
    params(("id" = Uuid, Path), ("keyword" = String, Query), ("card_type_id" = Option<Uuid>, Query)),
    responses((status = 200, body = Vec<CardOut>), (status = 404)), security(("bearer" = [])))]
async fn workspace_card_search(
    State(s): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
    Query(q): Query<WsSearchQuery>,
) -> ApiResult<Json<Vec<CardOut>>> {
    let project_id =
        crate::security::authz::require_workspace_role(&s.db, user.id, wid, Role::Viewer).await?;
    let role = require_role(&s.db, user.id, project_id, Role::Viewer).await?;
    if q.keyword.chars().count() > 200 {
        return Err(bad("keyword too long"));
    }
    let pat = format!(
        "%{}%",
        q.keyword
            .trim()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let vis = crate::security::authz::visible_cards_sql("c", 4, 5);
    let sql = format!(
        "SELECT {CARD_COLS} FROM cards c
         WHERE c.workspace_id = $1 AND c.deleted_at IS NULL AND ($2::uuid IS NULL OR c.card_type_id = $2)
           AND (EXISTS (SELECT 1 FROM fields tf WHERE (tf.card_type_id = c.card_type_id OR tf.card_type_id IS NULL) AND tf.is_title AND tf.deleted_at IS NULL AND (c.data ->> tf.slug) ILIKE $3 ESCAPE '\\') OR (SELECT w.key_prefix || '-' || c.number FROM workspaces w WHERE w.id = c.workspace_id) ILIKE $3 ESCAPE '\\')
           AND {vis}
         ORDER BY c.updated_at DESC LIMIT 20");
    let rows = sqlx::query_as(&sql)
        .bind(wid)
        .bind(q.card_type_id)
        .bind(pat)
        .bind(role >= Role::Admin)
        .bind(user.id)
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_values_are_unset() {
        let m = json!({"a": [], "b": "", "c": null, "d": 0, "e": false, "f": ["x"]});
        let n = normalize_data(m.as_object().unwrap().clone(), false);
        assert_eq!(
            Value::Object(n),
            json!({"d": 0, "e": false, "f": ["x"]}),
            "0 and false are real values"
        );
        let keep = normalize_data(json!({"a": []}).as_object().unwrap().clone(), true);
        assert_eq!(
            keep["a"],
            Value::Null,
            "patches keep the key as null so it clears the field"
        );
    }
}

/// Open an issue by its key (`WORK-12`), like Jira's /browse/KEY-12.
#[utoipa::path(get, operation_id = "getCardByKey", path = "/workspaces/{id}/cards/by-key/{key}", tag = "cards",
    params(("id" = Uuid, Path), ("key" = String, Path)), responses((status = 200, body = CardOut), (status = 404)), security(("bearer" = [])))]
async fn card_by_key(
    State(s): State<AppState>,
    user: AuthUser,
    Path((wid, key)): Path<(Uuid, String)>,
) -> ApiResult<Json<CardOut>> {
    crate::security::authz::require_workspace_role(&s.db, user.id, wid, Role::Viewer).await?;
    open_by_key(&s, &user, wid, &key).await
}

/// Open an issue (`ACME-12`) of a workspace the caller already passed the project check for.
async fn open_by_key(
    s: &AppState,
    user: &AuthUser,
    wid: Uuid,
    key: &str,
) -> ApiResult<Json<CardOut>> {
    let (prefix, number) = key.rsplit_once('-').ok_or(ApiError::NotFound)?;
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::NotFound);
    }
    let number: i32 = number.parse().map_err(|_| ApiError::NotFound)?;
    if number < 1 {
        return Err(ApiError::NotFound);
    }
    let found: Option<(Uuid,)> = sqlx::query_as(
        "SELECT c.id FROM cards c JOIN workspaces w ON w.id = c.workspace_id
         WHERE c.workspace_id = $1 AND c.number = $2 AND upper(w.key_prefix) = upper($3) AND c.deleted_at IS NULL",
    )
    .bind(wid)
    .bind(number)
    .bind(prefix)
    .fetch_optional(&s.db)
    .await?;
    let (id,) = found.ok_or(ApiError::NotFound)?;
    require_card_role(&s.db, user.id, id, Role::Viewer).await?; // private lists stay private
    Ok(Json(load_card(s, id).await?))
}

/// Open an issue by its key alone (`ACME-12`), like Jira's /browse/KEY-12. Malformed keys,
/// unknown keys and issues the caller may not see are all the same 404.
#[utoipa::path(get, operation_id = "resolveIssue", path = "/issues/{key}", tag = "cards",
    params(("key" = String, Path)), responses((status = 200, body = CardOut), (status = 404)), security(("bearer" = [])))]
async fn resolve_issue(
    State(s): State<AppState>,
    user: AuthUser,
    Path(key): Path<String>,
) -> ApiResult<Json<CardOut>> {
    let (prefix, _) = key.rsplit_once('-').ok_or(ApiError::NotFound)?;
    let wid = super::workspaces::workspace_id_by_key(&s.db, prefix)
        .await?
        .ok_or(ApiError::NotFound)?;
    crate::security::authz::require_workspace_role(&s.db, user.id, wid, Role::Viewer).await?;
    open_by_key(&s, &user, wid, &key).await
}

#[derive(Deserialize, ToSchema)]
pub struct SprintAssign {
    pub list_id: Uuid,
    /// Target sprint, or `null` to move the issue back to the backlog.
    pub sprint_id: Option<Uuid>,
}

#[utoipa::path(put, operation_id = "putCardSprint", path = "/cards/{id}/sprint", tag = "cards",
    params(("id" = Uuid, Path)), request_body = SprintAssign,
    responses((status = 200, body = CardOut), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn put_card_sprint(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<SprintAssign>,
) -> ApiResult<Json<CardOut>> {
    require_card_role(&s.db, user.id, id, Role::Member).await?;
    require_list_role(&s.db, user.id, b.list_id, Role::Member).await?;
    if let Some(sp) = b.sprint_id {
        // The sprint must belong to this very list and must not be finished.
        let ok: Option<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM sprints WHERE id = $1 AND list_id = $2 AND state <> 'completed'",
        )
        .bind(sp)
        .bind(b.list_id)
        .fetch_optional(&s.db)
        .await?;
        if ok.is_none() {
            return Err(bad("sprint not found in this list or already completed"));
        }
    }
    let r = sqlx::query(
        "UPDATE card_lists cl SET sprint_id = $3 FROM cards c, lists l
         WHERE cl.card_id = $1 AND cl.list_id = $2 AND c.id = cl.card_id AND l.id = cl.list_id
           AND c.workspace_id = l.workspace_id",
    )
    .bind(id)
    .bind(b.list_id)
    .bind(b.sprint_id)
    .execute(&s.db)
    .await?;
    if r.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(Json(load_card(&s, id).await?))
}

#[derive(Deserialize)]
pub struct MineQuery {
    /// Include issues in a completed stage (default: only open issues).
    pub include_done: Option<bool>,
}

/// "Your work": issues of this workspace assigned to the caller, newest activity first.
#[utoipa::path(get, operation_id = "listMyIssues", path = "/workspaces/{id}/issues/mine", tag = "cards",
    params(("id" = Uuid, Path), ("include_done" = Option<bool>, Query)),
    responses((status = 200, body = Vec<CardOut>), (status = 404)), security(("bearer" = [])))]
async fn my_issues(
    State(s): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
    Query(q): Query<MineQuery>,
) -> ApiResult<Json<Vec<CardOut>>> {
    let project_id =
        crate::security::authz::require_workspace_role(&s.db, user.id, wid, Role::Viewer).await?;
    let role = require_role(&s.db, user.id, project_id, Role::Viewer).await?;
    let vis = crate::security::authz::visible_cards_sql("c", 3, 2);
    let sql = format!(
        "SELECT {CARD_COLS} FROM cards c
         WHERE c.workspace_id = $1 AND c.deleted_at IS NULL
           AND EXISTS (SELECT 1 FROM fields f WHERE (f.card_type_id = c.card_type_id OR f.card_type_id IS NULL) AND f.is_assignee AND f.deleted_at IS NULL
                       AND CASE WHEN jsonb_typeof(c.data -> f.slug) = 'array' THEN (c.data -> f.slug) ? ($2::uuid)::text ELSE (c.data ->> f.slug) = ($2::uuid)::text END)
           AND ($4 OR NOT EXISTS (SELECT 1 FROM card_lists cl JOIN list_stages st ON st.id = cl.list_stage_id WHERE cl.card_id = c.id AND st.is_completed))
           AND {vis}
         ORDER BY c.updated_at DESC LIMIT 100"
    );
    let rows = sqlx::query_as(&sql)
        .bind(wid)
        .bind(user.id)
        .bind(role >= Role::Admin)
        .bind(q.include_done.unwrap_or(false))
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows))
}
