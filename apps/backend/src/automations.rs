//! Automations: "when X happens on a card, do Y". The engine runs in-process from domain events.
//!
//! Security notes, versus the original implementation:
//! * Creating/editing automations needs project admin rights, because actions run without
//!   per-list permission checks.
//! * A run only happens while the automation's creator is still a member (member role or higher)
//!   of the project; otherwise the run fails and says so.
//! * Placeholders (`{{trigger.path}}`) are plain path lookups into the event, never evaluated.
//! * Events caused by automations carry an incremented depth and stop at `MAX_DEPTH`.

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::types::Json as SqlJson;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    authz::{require_role, require_workspace_role, Role, Scope},
    error::{ApiError, ApiResult},
    events::{emit, Event, MAX_DEPTH},
    routes::{
        cards::{create_card_in_list, set_card_stage, update_card_data, CreateCardBody},
        OkOut,
    },
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(automation_catalog))
        .routes(routes!(list_automations, create_automation))
        .routes(routes!(
            get_automation,
            update_automation,
            delete_automation
        ))
        .routes(routes!(list_runs))
        .routes(routes!(regenerate_webhook_token))
        .routes(routes!(get_signing_secret))
        .routes(routes!(incoming_webhook))
}

const TRIGGERS: &[&str] = &[
    "card_created",
    "field_updated",
    "stage_changed",
    "comment_created",
    "assignee_added",
    "assignee_removed",
    "webhook",
];
const ACTIONS: &[&str] = &[
    "create_card",
    "create_sub_card",
    "set_field",
    "create_comment",
    "change_assignee",
    "set_stage",
    "send_webhook",
    "send_email",
];

fn bad(m: impl Into<String>) -> ApiError {
    ApiError::BadRequest(m.into())
}

// ---- catalog (what the UI offers) -----------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct HandlerInfo {
    pub value: String,
    pub title: String,
    pub description: String,
    /// JSON keys the step expects in `config`.
    pub config_keys: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct Catalog {
    pub triggers: Vec<HandlerInfo>,
    pub actions: Vec<HandlerInfo>,
}

#[utoipa::path(get, operation_id = "getAutomationCatalog", path = "/automations/catalog", tag = "automations",
    responses((status = 200, body = Catalog)), security(("bearer" = [])))]
async fn automation_catalog(_user: AuthUser) -> Json<Catalog> {
    let h = |v: &str, t: &str, d: &str, k: &[&str]| HandlerInfo {
        value: v.into(),
        title: t.into(),
        description: d.into(),
        config_keys: k.iter().map(|s| s.to_string()).collect(),
    };
    Json(Catalog {
        triggers: vec![
            h(
                "card_created",
                "Card created",
                "A card is created in a list.",
                &[],
            ),
            h(
                "field_updated",
                "Field updated",
                "A field changes. Optional `field` (slug), `from`, `to`.",
                &["field", "from", "to"],
            ),
            h(
                "stage_changed",
                "Stage changed",
                "A card moves to another stage. Optional `from`, `to` (stage ids).",
                &["from", "to"],
            ),
            h(
                "comment_created",
                "Comment created",
                "A comment is added to a card.",
                &[],
            ),
            h(
                "assignee_added",
                "Assignee added",
                "Someone is assigned. Optional `user_id`.",
                &["user_id"],
            ),
            h(
                "assignee_removed",
                "Assignee removed",
                "Someone is unassigned. Optional `user_id`.",
                &["user_id"],
            ),
        ],
        actions: vec![
            h(
                "set_field",
                "Set a field",
                "Set `field` (slug) to `value`.",
                &["field", "value"],
            ),
            h(
                "set_stage",
                "Set stage",
                "Move the card to `stage_id`.",
                &["stage_id"],
            ),
            h(
                "change_assignee",
                "Change assignee",
                "`add` / `remove` lists of user ids.",
                &["add", "remove"],
            ),
            h(
                "create_comment",
                "Create a comment",
                "Adds `content` as a comment.",
                &["content"],
            ),
            h(
                "create_card",
                "Create a card",
                "Create in `list_id` with `data` and optional `stage_id`.",
                &["list_id", "stage_id", "data"],
            ),
            h(
                "create_sub_card",
                "Create a sub card",
                "Create a child card with `data`.",
                &["data", "stage_id"],
            ),
        ],
    })
}

// ---- model ----------------------------------------------------------------------------------

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct StepIo {
    /// One of the action types from the catalog.
    pub action_type: String,
    #[schema(value_type = Object)]
    pub config: Value,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct LocationIo {
    /// `space` or `list`
    pub location_type: String,
    pub location_id: Uuid,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct AutomationRow {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub is_enabled: bool,
    pub trigger_type: String,
    #[schema(value_type = Object)]
    pub trigger_config: SqlJson<Value>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// A webhook URL exists for this automation (trigger type `webhook`).
    pub has_webhook: bool,
}

#[derive(Serialize, ToSchema)]
pub struct AutomationOut {
    #[serde(flatten)]
    pub automation: AutomationRow,
    pub steps: Vec<StepIo>,
    pub locations: Vec<LocationIo>,
}

const COLS: &str = "id, workspace_id, name, is_enabled, trigger_type, trigger_config, created_by, created_at, updated_at, webhook_token_hash IS NOT NULL AS has_webhook";

async fn load(s: &AppState, id: Uuid) -> ApiResult<AutomationOut> {
    let sql = format!("SELECT {COLS} FROM automations WHERE id = $1 AND deleted_at IS NULL");
    let automation: AutomationRow = sqlx::query_as(&sql)
        .bind(id)
        .fetch_optional(&s.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let steps: Vec<(String, SqlJson<Value>)> = sqlx::query_as("SELECT action_type, config FROM automation_steps WHERE automation_id = $1 ORDER BY sort_order").bind(id).fetch_all(&s.db).await?;
    let locations: Vec<(String, Uuid)> = sqlx::query_as(
        "SELECT location_type, location_id FROM automation_locations WHERE automation_id = $1",
    )
    .bind(id)
    .fetch_all(&s.db)
    .await?;
    Ok(AutomationOut {
        automation,
        steps: steps
            .into_iter()
            .map(|(a, c)| StepIo {
                action_type: a,
                config: c.0,
            })
            .collect(),
        locations: locations
            .into_iter()
            .map(|(t, i)| LocationIo {
                location_type: t,
                location_id: i,
            })
            .collect(),
    })
}

fn check_def(
    name: Option<&str>,
    trigger: Option<&str>,
    trigger_config: &Option<Value>,
    steps: &Option<Vec<StepIo>>,
    allow_http: bool,
) -> ApiResult<()> {
    if let Some(n) = name {
        if n.trim().is_empty() || n.chars().count() > 100 {
            return Err(bad("name must be 1-100 characters"));
        }
    }
    if let Some(t) = trigger {
        if !TRIGGERS.contains(&t) {
            return Err(bad("unknown trigger type"));
        }
    }
    if let Some(c) = trigger_config {
        if !c.is_object() || c.to_string().len() > 8192 {
            return Err(bad("trigger_config must be a small object"));
        }
    }
    if let Some(steps) = steps {
        if steps.is_empty() || steps.len() > 20 {
            return Err(bad("1-20 steps required"));
        }
        for st in steps {
            if !ACTIONS.contains(&st.action_type.as_str()) {
                return Err(bad("unknown action type"));
            }
            if !st.config.is_object() || st.config.to_string().len() > 64 * 1024 {
                return Err(bad("step config must be an object under 64 KB"));
            }
            if st.action_type == "send_webhook" {
                let url = st
                    .config
                    .get("url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| bad("send_webhook needs a `url`"))?;
                crate::ssrf::validate_url(url, allow_http).map_err(|e| bad(e.to_string()))?;
            }
            if st.action_type == "send_email"
                && st
                    .config
                    .get("to")
                    .and_then(Value::as_str)
                    .is_none_or(|t| t.trim().is_empty())
            {
                return Err(bad("send_email needs a recipient (`to`)"));
            }
        }
    }
    Ok(())
}

async fn check_locations(s: &AppState, workspace_id: Uuid, locs: &[LocationIo]) -> ApiResult<()> {
    for l in locs {
        let sql = match l.location_type.as_str() {
            "space" => "SELECT count(*) FROM spaces WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
            "list" => "SELECT count(*) FROM lists WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
            _ => return Err(bad("location_type must be space or list")),
        };
        let n: (i64,) = sqlx::query_as(sql)
            .bind(l.location_id)
            .bind(workspace_id)
            .fetch_one(&s.db)
            .await?;
        if n.0 == 0 {
            return Err(bad("location not found in this workspace"));
        }
    }
    Ok(())
}

#[utoipa::path(get, operation_id = "listAutomations", path = "/workspaces/{id}/automations", tag = "automations",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<AutomationOut>), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn list_automations(
    State(s): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
) -> ApiResult<Json<Vec<AutomationOut>>> {
    require_workspace_role(&s.db, user.id, wid, Role::Admin).await?;
    let ids: Vec<(Uuid,)> = sqlx::query_as("SELECT id FROM automations WHERE workspace_id = $1 AND deleted_at IS NULL ORDER BY created_at").bind(wid).fetch_all(&s.db).await?;
    let mut out = vec![];
    for (id,) in ids {
        out.push(load(&s, id).await?);
    }
    Ok(Json(out))
}

#[derive(Deserialize, ToSchema)]
pub struct AutomationBody {
    pub name: String,
    pub trigger_type: String,
    #[schema(value_type = Option<Object>)]
    pub trigger_config: Option<Value>,
    pub steps: Vec<StepIo>,
    pub locations: Option<Vec<LocationIo>>,
    pub is_enabled: Option<bool>,
}

async fn write_children(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    steps: &[StepIo],
    locations: &[LocationIo],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM automation_steps WHERE automation_id = $1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    for (i, st) in steps.iter().enumerate() {
        sqlx::query("INSERT INTO automation_steps (id, automation_id, sort_order, action_type, config) VALUES ($1,$2,$3,$4,$5)")
            .bind(Uuid::new_v4()).bind(id).bind(i as i32).bind(&st.action_type).bind(SqlJson(&st.config)).execute(&mut **tx).await?;
    }
    sqlx::query("DELETE FROM automation_locations WHERE automation_id = $1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    for l in locations {
        sqlx::query("INSERT INTO automation_locations (automation_id, location_type, location_id) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(id).bind(&l.location_type).bind(l.location_id).execute(&mut **tx).await?;
    }
    Ok(())
}

#[utoipa::path(post, operation_id = "createAutomation", path = "/workspaces/{id}/automations", tag = "automations",
    params(("id" = Uuid, Path)), request_body = AutomationBody,
    responses((status = 200, body = AutomationOut), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn create_automation(
    State(s): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
    Json(b): Json<AutomationBody>,
) -> ApiResult<Json<AutomationOut>> {
    require_workspace_role(&s.db, user.id, wid, Role::Admin).await?;
    check_def(
        Some(&b.name),
        Some(&b.trigger_type),
        &b.trigger_config,
        &Some(b.steps.clone()),
        s.config.webhook_allow_http,
    )?;
    let locs = b.locations.clone().unwrap_or_default();
    check_locations(&s, wid, &locs).await?;
    let id = Uuid::new_v4();
    let mut tx = s.db.begin().await?;
    sqlx::query("INSERT INTO automations (id, workspace_id, name, is_enabled, trigger_type, trigger_config, created_by) VALUES ($1,$2,$3,$4,$5,$6,$7)")
        .bind(id).bind(wid).bind(b.name.trim()).bind(b.is_enabled.unwrap_or(false)).bind(&b.trigger_type)
        .bind(SqlJson(b.trigger_config.unwrap_or_else(|| json!({})))).bind(user.id).execute(&mut *tx).await?;
    write_children(&mut tx, id, &b.steps, &locs).await?;
    tx.commit().await?;
    Ok(Json(load(&s, id).await?))
}

async fn authorize(s: &AppState, user: &AuthUser, id: Uuid) -> ApiResult<Uuid> {
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT workspace_id FROM automations WHERE id = $1 AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(&s.db)
            .await?;
    let (wid,) = row.ok_or(ApiError::NotFound)?;
    require_workspace_role(&s.db, user.id, wid, Role::Admin).await?;
    Ok(wid)
}

#[utoipa::path(get, operation_id = "getAutomation", path = "/automations/{id}", tag = "automations",
    params(("id" = Uuid, Path)), responses((status = 200, body = AutomationOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn get_automation(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<AutomationOut>> {
    authorize(&s, &user, id).await?;
    Ok(Json(load(&s, id).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct AutomationPatch {
    pub name: Option<String>,
    pub is_enabled: Option<bool>,
    pub trigger_type: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub trigger_config: Option<Value>,
    pub steps: Option<Vec<StepIo>>,
    pub locations: Option<Vec<LocationIo>>,
}

#[utoipa::path(patch, operation_id = "updateAutomation", path = "/automations/{id}", tag = "automations",
    params(("id" = Uuid, Path)), request_body = AutomationPatch,
    responses((status = 200, body = AutomationOut), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update_automation(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<AutomationPatch>,
) -> ApiResult<Json<AutomationOut>> {
    let wid = authorize(&s, &user, id).await?;
    check_def(
        b.name.as_deref(),
        b.trigger_type.as_deref(),
        &b.trigger_config,
        &b.steps,
        s.config.webhook_allow_http,
    )?;
    if let Some(l) = &b.locations {
        check_locations(&s, wid, l).await?;
    }
    let mut tx = s.db.begin().await?;
    // Whoever last edits an automation becomes the identity it runs as (and is accountable for).
    sqlx::query("UPDATE automations SET name = COALESCE($2,name), is_enabled = COALESCE($3,is_enabled), trigger_type = COALESCE($4,trigger_type), trigger_config = COALESCE($5,trigger_config), created_by = $6, updated_at = now() WHERE id = $1")
        .bind(id).bind(b.name.as_deref().map(str::trim)).bind(b.is_enabled).bind(&b.trigger_type).bind(b.trigger_config.map(SqlJson)).bind(user.id).execute(&mut *tx).await?;
    if b.steps.is_some() || b.locations.is_some() {
        let current = load(&s, id).await?;
        let steps = b.steps.unwrap_or(current.steps);
        let locs = b.locations.unwrap_or(current.locations);
        write_children(&mut tx, id, &steps, &locs).await?;
    }
    tx.commit().await?;
    Ok(Json(load(&s, id).await?))
}

#[utoipa::path(delete, operation_id = "deleteAutomation", path = "/automations/{id}", tag = "automations",
    params(("id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn delete_automation(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    authorize(&s, &user, id).await?;
    sqlx::query("UPDATE automations SET deleted_at = now(), is_enabled = false WHERE id = $1")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(Json(OkOut { ok: true }))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct RunOut {
    pub id: Uuid,
    pub card_id: Option<Uuid>,
    pub status: String,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[utoipa::path(get, operation_id = "listAutomationRuns", path = "/automations/{id}/runs", tag = "automations",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<RunOut>), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn list_runs(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<RunOut>>> {
    authorize(&s, &user, id).await?;
    Ok(Json(sqlx::query_as("SELECT id, card_id, status, error, started_at, finished_at FROM automation_runs WHERE automation_id = $1 ORDER BY started_at DESC LIMIT 50").bind(id).fetch_all(&s.db).await?))
}

// ---- webhooks -------------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct WebhookTokenOut {
    /// Path to POST to, relative to the site origin. It contains the secret and is shown only once.
    pub path: String,
}

/// Creates (or rotates) the secret URL of an automation with the `webhook` trigger. The previous
/// URL stops working immediately. Only a hash of the secret is stored.
#[utoipa::path(post, operation_id = "regenerateWebhookToken", path = "/automations/{id}/webhook-token", tag = "automations",
    params(("id" = Uuid, Path)), responses((status = 200, body = WebhookTokenOut), (status = 400), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn regenerate_webhook_token(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<WebhookTokenOut>> {
    user.require_session()?;
    authorize(&s, &user, id).await?;
    let (ty,): (String,) = sqlx::query_as("SELECT trigger_type FROM automations WHERE id = $1")
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    if ty != "webhook" {
        return Err(bad(
            "only automations with the `webhook` trigger have a webhook URL",
        ));
    }
    let token = crate::auth::random_token();
    sqlx::query("UPDATE automations SET webhook_token_hash = $2 WHERE id = $1")
        .bind(id)
        .bind(crate::auth::hash_token(&token))
        .execute(&s.db)
        .await?;
    Ok(Json(WebhookTokenOut {
        path: format!("/api/v1/hooks/{id}/{token}"),
    }))
}

#[derive(Serialize, ToSchema)]
pub struct SigningSecretOut {
    /// Hex key. Outbound webhooks carry `X-Work-Signature: sha256=HMAC_SHA256(key, raw body)`.
    pub secret: String,
}

#[utoipa::path(get, operation_id = "getSigningSecret", path = "/automations/{id}/signing-secret", tag = "automations",
    params(("id" = Uuid, Path)), responses((status = 200, body = SigningSecretOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn get_signing_secret(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SigningSecretOut>> {
    user.require_session()?;
    authorize(&s, &user, id).await?;
    Ok(Json(SigningSecretOut {
        secret: hex::encode(signing_key(&s.config.jwt_secret, id)),
    }))
}

#[derive(Serialize, ToSchema)]
pub struct HookAccepted {
    pub run_id: Uuid,
}

/// Public endpoint: the secret in the URL is the credential. Unknown id, wrong secret, disabled and
/// deleted automations all look the same (404). The run happens in the background.
#[utoipa::path(post, operation_id = "incomingWebhook", path = "/hooks/{id}/{token}", tag = "automations",
    params(("id" = Uuid, Path), ("token" = String, Path)),
    request_body(content = serde_json::Value, description = "Any JSON object. Available as {{trigger.body.*}}; include `card_id` to act on an issue."),
    responses((status = 202, body = HookAccepted), (status = 404), (status = 429)))]
async fn incoming_webhook(
    State(s): State<AppState>,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Path((id, token)): Path<(Uuid, String)>,
    body: axum::body::Bytes,
) -> ApiResult<(axum::http::StatusCode, Json<HookAccepted>)> {
    let ip = if s.config.trust_forwarded_for {
        headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.rsplit(',').next())
            .map(|v| v.trim().to_owned())
            .unwrap_or_else(|| peer.ip().to_string())
    } else {
        peer.ip().to_string()
    };
    if !s.hook_limiter.check(&format!("ip:{ip}"))
        || !s.hook_limiter.check(&format!("automation:{id}"))
    {
        return Err(ApiError::TooManyRequests);
    }
    if token.len() > 100 || body.len() > 64 * 1024 {
        return Err(ApiError::NotFound);
    }
    let row: Option<(Uuid, Option<Uuid>, Vec<u8>)> = sqlx::query_as(
        "SELECT workspace_id, created_by, webhook_token_hash FROM automations
         WHERE id = $1 AND is_enabled AND deleted_at IS NULL AND trigger_type = 'webhook' AND webhook_token_hash IS NOT NULL",
    )
    .bind(id)
    .fetch_optional(&s.db)
    .await?;
    let (workspace_id, creator, stored) = row.ok_or(ApiError::NotFound)?;
    use subtle::ConstantTimeEq;
    if !bool::from(stored.ct_eq(&crate::auth::hash_token(&token))) {
        return Err(ApiError::NotFound);
    }
    let payload: Value = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&body).map_err(|_| bad("body must be JSON"))?
    };
    // An optional `card_id` selects the issue to act on; it must live in the automation's workspace.
    let card_id = match payload.get("card_id").and_then(Value::as_str) {
        Some(c) => {
            let c = Uuid::parse_str(c).map_err(|_| bad("card_id must be a uuid"))?;
            let ok: Option<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM cards WHERE id = $1 AND workspace_id = $2 AND deleted_at IS NULL",
            )
            .bind(c)
            .bind(workspace_id)
            .fetch_optional(&s.db)
            .await?;
            Some(ok.ok_or_else(|| bad("card not found in this workspace"))?.0)
        }
        None => None,
    };
    let input = RunInput {
        automation: id,
        creator,
        card_id,
        workspace_id,
        trigger: json!({"type": "webhook", "body": payload}),
        depth: 0,
    };
    // The run row exists before we answer, so the caller can look the run up by this id.
    let run_id = start_run(&s, &input).await?;
    let st = s.clone();
    tokio::spawn(async move {
        if let Err(e) = finish_run(&st, run_id, &input).await {
            tracing::warn!(error = %e, "webhook automation failed");
        }
    });
    Ok((
        axum::http::StatusCode::ACCEPTED,
        Json(HookAccepted { run_id }),
    ))
}

// ---- engine ---------------------------------------------------------------------------------

/// Replace `{{trigger.a.b}}` with the (stringified) value found at that path. No expressions.
pub fn substitute(value: &Value, trigger: &Value) -> Value {
    match value {
        Value::String(s) => {
            // Whole-string placeholder keeps the original JSON type (e.g. a user id array).
            if let Some(path) = s
                .strip_prefix("{{trigger.")
                .and_then(|r| r.strip_suffix("}}"))
                .filter(|p| !p.contains("{{"))
            {
                return lookup(trigger, path).cloned().unwrap_or(Value::Null);
            }
            let mut out = String::new();
            let mut rest = s.as_str();
            while let Some(start) = rest.find("{{trigger.") {
                out.push_str(&rest[..start]);
                let after = &rest[start + 10..];
                match after.find("}}") {
                    Some(end) => {
                        let repl = lookup(trigger, &after[..end])
                            .map(|v| match v {
                                Value::String(x) => x.clone(),
                                Value::Null => String::new(),
                                o => o.to_string(),
                            })
                            .unwrap_or_default();
                        out.push_str(&repl);
                        rest = &after[end + 2..];
                    }
                    None => {
                        out.push_str(&rest[start..]);
                        rest = "";
                    }
                }
            }
            out.push_str(rest);
            Value::String(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(|v| substitute(v, trigger)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), substitute(v, trigger)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn lookup<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(root, |cur, part| match cur {
        Value::Object(o) => o.get(part),
        Value::Array(a) => part.parse::<usize>().ok().and_then(|i| a.get(i)),
        _ => None,
    })
}

fn ids_of(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        Value::String(s) => vec![s.clone()],
        _ => vec![],
    }
}

/// Does the event satisfy the trigger configuration?
fn trigger_matches(kind: &str, cfg: &Value, ev: &Value) -> bool {
    let want = |k: &str| {
        cfg.get(k)
            .filter(|v| !v.is_null() && v.as_str() != Some(""))
    };
    match kind {
        "card_created" | "comment_created" | "webhook" => true,
        "field_updated" => {
            if let Some(f) = want("field") {
                if ev["field"].as_str() != f.as_str() {
                    return false;
                }
            }
            if let Some(f) = want("from") {
                if &ev["oldValue"] != f {
                    return false;
                }
            }
            if let Some(t) = want("to") {
                if &ev["newValue"] != t {
                    return false;
                }
            }
            true
        }
        "stage_changed" => {
            let ok = |k: &str, actual: &Value| {
                want(k).is_none_or(|w| ids_of(w).iter().any(|i| actual.as_str() == Some(i)))
            };
            ok("from", &ev["oldValue"]) && ok("to", &ev["newValue"])
        }
        "assignee_added" | "assignee_removed" => {
            let (old, new) = (ids_of(&ev["oldValue"]), ids_of(&ev["newValue"]));
            let changed: Vec<&String> = if kind == "assignee_added" {
                new.iter().filter(|u| !old.contains(u)).collect()
            } else {
                old.iter().filter(|u| !new.contains(u)).collect()
            };
            !changed.is_empty()
                && want("user_id")
                    .is_none_or(|w| changed.iter().any(|u| w.as_str() == Some(u.as_str())))
        }
        _ => false,
    }
}

pub async fn handle_event(s: &AppState, event: &Event) -> Result<(), sqlx::Error> {
    let (card_id, workspace_id, depth, _list_hint, mut kinds, payload): (
        Uuid,
        Uuid,
        u8,
        Option<Uuid>,
        Vec<&str>,
        Value,
    ) = match event {
        Event::CardCreated {
            card_id,
            workspace_id,
            list_id,
            depth,
            ..
        } => (
            *card_id,
            *workspace_id,
            *depth,
            Some(*list_id),
            vec!["card_created"],
            json!({"type": "card_created"}),
        ),
        Event::FieldUpdated {
            card_id,
            workspace_id,
            slug,
            old,
            new,
            depth,
            ..
        } => {
            let mut k = vec!["field_updated"];
            // Assignee triggers are field updates on the assignee field.
            let is_assignee: Option<(bool,)> = sqlx::query_as("SELECT f.is_assignee FROM fields f WHERE f.workspace_id = $1 AND f.slug = $2 AND f.deleted_at IS NULL LIMIT 1").bind(workspace_id).bind(slug).fetch_optional(&s.db).await?;
            if is_assignee.is_some_and(|x| x.0) {
                k.push("assignee_added");
                k.push("assignee_removed");
            }
            (
                *card_id,
                *workspace_id,
                *depth,
                None,
                k,
                json!({"type": "field_updated", "field": slug, "oldValue": old, "newValue": new}),
            )
        }
        Event::StageUpdated {
            card_id,
            workspace_id,
            list_id,
            from,
            to,
            depth,
            ..
        } => (
            *card_id,
            *workspace_id,
            *depth,
            Some(*list_id),
            vec!["stage_changed"],
            json!({"type": "stage_changed", "oldValue": from, "newValue": to}),
        ),
        Event::CommentCreated {
            card_id,
            workspace_id,
            activity_id,
            depth,
            ..
        } => (
            *card_id,
            *workspace_id,
            *depth,
            None,
            vec!["comment_created"],
            json!({"type": "comment_created", "activityId": activity_id}),
        ),
    };
    if depth >= MAX_DEPTH {
        return Ok(());
    }
    kinds.dedup();

    let autos: Vec<(Uuid, String, SqlJson<Value>, Option<Uuid>)> = sqlx::query_as(
        "SELECT id, trigger_type, trigger_config, created_by FROM automations
         WHERE workspace_id = $1 AND is_enabled AND deleted_at IS NULL AND trigger_type = ANY($2)",
    )
    .bind(workspace_id)
    .bind(kinds.iter().map(|k| k.to_string()).collect::<Vec<_>>())
    .fetch_all(&s.db)
    .await?;
    for (id, kind, SqlJson(cfg), creator) in autos {
        if !trigger_matches(&kind, &cfg, &payload) {
            continue;
        }
        // Location filter: the card must be in a list covered by the automation (or none are configured).
        let in_scope: (bool,) = sqlx::query_as(
            "SELECT (NOT EXISTS (SELECT 1 FROM automation_locations WHERE automation_id = $1)) OR EXISTS (
                SELECT 1 FROM card_lists cl JOIN lists l ON l.id = cl.list_id JOIN automation_locations al ON al.automation_id = $1
                 AND ((al.location_type = 'list' AND al.location_id = l.id) OR (al.location_type = 'space' AND al.location_id = l.space_id))
                WHERE cl.card_id = $2)")
            .bind(id).bind(card_id).fetch_one(&s.db).await?;
        if !in_scope.0 {
            continue;
        }
        run(
            s,
            RunInput {
                automation: id,
                creator,
                card_id: Some(card_id),
                workspace_id,
                trigger: payload.clone(),
                depth,
            },
        )
        .await?;
    }
    Ok(())
}

/// Everything a run needs to know about what triggered it.
struct RunInput {
    automation: Uuid,
    creator: Option<Uuid>,
    /// Absent for webhook triggers that do not name a card.
    card_id: Option<Uuid>,
    workspace_id: Uuid,
    trigger: Value,
    depth: u8,
}

async fn start_run(s: &AppState, input: &RunInput) -> Result<Uuid, sqlx::Error> {
    let run_id = Uuid::new_v4();
    sqlx::query("INSERT INTO automation_runs (id, automation_id, card_id, status) VALUES ($1,$2,$3,'running')")
        .bind(run_id)
        .bind(input.automation)
        .bind(input.card_id)
        .execute(&s.db)
        .await?;
    Ok(run_id)
}

async fn finish_run(s: &AppState, run_id: Uuid, input: &RunInput) -> Result<(), sqlx::Error> {
    let result = execute_steps(s, run_id, input).await;
    let (status, error) = match result {
        Ok(()) => ("completed", None),
        Err(e) => (
            "failed",
            Some(e.to_string().chars().take(500).collect::<String>()),
        ),
    };
    sqlx::query(
        "UPDATE automation_runs SET status = $2, error = $3, finished_at = now() WHERE id = $1",
    )
    .bind(run_id)
    .bind(status)
    .bind(error)
    .execute(&s.db)
    .await?;
    Ok(())
}

async fn run(s: &AppState, input: RunInput) -> Result<Uuid, sqlx::Error> {
    let run_id = start_run(s, &input).await?;
    finish_run(s, run_id, &input).await?;
    Ok(run_id)
}

/// Key used to sign outbound webhooks of one automation. Derived from the server secret, so it
/// needs no storage; admins can read it (session only) to verify signatures on the receiving side.
pub fn signing_key(jwt_secret: &[u8], automation: Uuid) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    let mut mac =
        Hmac::<sha2::Sha256>::new_from_slice(jwt_secret).expect("HMAC accepts any key length");
    mac.update(format!("webhook-signing:{automation}").as_bytes());
    mac.finalize().into_bytes().to_vec()
}

pub fn sign(key: &[u8], body: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

fn plain_text_to_html(text: &str) -> String {
    format!(
        "<p>{}</p>",
        crate::mailer::escape_html(text)
            .replace("\r\n", "\n")
            .replace('\n', "<br>")
    )
}

async fn execute_steps(s: &AppState, run_id: Uuid, input: &RunInput) -> ApiResult<()> {
    let RunInput {
        automation,
        creator,
        card_id,
        workspace_id,
        trigger,
        depth,
    } = input;
    let (automation, workspace_id, depth) = (*automation, *workspace_id, *depth);
    let project: (Uuid,) = sqlx::query_as("SELECT project_id FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .fetch_one(&s.db)
        .await?;
    // The automation acts with its creator's authority, and only while they still have it.
    let creator = creator.ok_or_else(|| bad("automation has no owner any more"))?;
    require_role(&s.db, creator, project.0, Role::Member)
        .await
        .map_err(|_| bad("automation owner no longer has access"))?;
    let scope = Scope {
        project_id: project.0,
        workspace_id,
    };
    let steps: Vec<(String, SqlJson<Value>)> =
        sqlx::query_as("SELECT action_type, config FROM automation_steps WHERE automation_id = $1 ORDER BY sort_order").bind(automation).fetch_all(&s.db).await?;

    let mut ctx = json!({});
    ctx.as_object_mut()
        .unwrap()
        .extend(trigger.as_object().cloned().unwrap_or_default());
    if let Some(card) = card_id {
        ctx["card"] = json!({"id": card});
        // Current card data for placeholders, e.g. {{trigger.card.data.title}}.
        if let Some((SqlJson(data), key)) = sqlx::query_as::<_, (SqlJson<Value>, String)>(
            "SELECT c.data, w.key_prefix || '-' || c.number FROM cards c JOIN workspaces w ON w.id = c.workspace_id WHERE c.id = $1",
        )
        .bind(card)
        .fetch_optional(&s.db)
        .await?
        {
            ctx["card"]["data"] = data;
            ctx["card"]["key"] = json!(key);
        }
    }
    let need_card = || {
        card_id.ok_or_else(|| bad("this action needs a card: send `card_id` in the webhook body"))
    };
    let next = depth + 1;
    for (action, SqlJson(raw)) in steps {
        let cfg = substitute(&raw, &ctx);
        match action.as_str() {
            "set_field" => {
                let card_id = need_card()?;
                let field = cfg["field"]
                    .as_str()
                    .ok_or_else(|| bad("set_field needs `field`"))?;
                let mut patch = Map::new();
                patch.insert(
                    field.to_owned(),
                    cfg.get("value").cloned().unwrap_or(Value::Null),
                );
                update_card_data(s, card_id, &scope, patch, None, true, next).await?;
            }
            "set_stage" => {
                let card_id = need_card()?;
                let stage = cfg["stage_id"]
                    .as_str()
                    .and_then(|x| Uuid::parse_str(x).ok())
                    .ok_or_else(|| bad("set_stage needs `stage_id`"))?;
                let list: Option<(Uuid,)> =
                    sqlx::query_as("SELECT list_id FROM list_stages WHERE id = $1")
                        .bind(stage)
                        .fetch_optional(&s.db)
                        .await?;
                let list = list.ok_or(ApiError::NotFound)?.0;
                let in_list: Option<(Uuid,)> = sqlx::query_as(
                    "SELECT list_id FROM card_lists WHERE card_id = $1 AND list_id = $2",
                )
                .bind(card_id)
                .bind(list)
                .fetch_optional(&s.db)
                .await?;
                if in_list.is_none() {
                    return Err(bad("card is not in the list of that stage"));
                }
                set_card_stage(s, card_id, list, &scope, stage, None, None, next).await?;
            }
            "change_assignee" => {
                let card_id = need_card()?;
                let f: Option<(String,)> = sqlx::query_as("SELECT f.slug FROM fields f JOIN cards c ON c.id = $1 AND (f.card_type_id = c.card_type_id OR f.card_type_id IS NULL) WHERE f.workspace_id = $2 AND f.is_assignee AND f.deleted_at IS NULL LIMIT 1").bind(card_id).bind(workspace_id).fetch_optional(&s.db).await?;
                let slug = f.ok_or_else(|| bad("card type has no assignee field"))?.0;
                let (cur,): (SqlJson<Value>,) =
                    sqlx::query_as("SELECT data FROM cards WHERE id = $1")
                        .bind(card_id)
                        .fetch_one(&s.db)
                        .await?;
                let mut users = ids_of(cur.0.get(&slug).unwrap_or(&Value::Null));
                let remove = ids_of(cfg.get("remove").unwrap_or(&Value::Null));
                users.retain(|u| !remove.contains(u));
                for a in ids_of(cfg.get("add").unwrap_or(&Value::Null)) {
                    if !users.contains(&a) {
                        users.push(a);
                    }
                }
                let mut patch = Map::new();
                patch.insert(
                    slug,
                    if users.is_empty() {
                        Value::Null
                    } else {
                        json!(users)
                    },
                );
                update_card_data(s, card_id, &scope, patch, None, true, next).await?;
            }
            "create_comment" => {
                let card_id = need_card()?;
                let content = cfg
                    .get("content")
                    .cloned()
                    .filter(|c| c.is_string() || c.is_object())
                    .ok_or_else(|| bad("create_comment needs `content`"))?;
                let aid = Uuid::new_v4();
                sqlx::query("INSERT INTO card_activities (id, card_id, type, content, created_by_type) VALUES ($1,$2,'comment',$3,'system')")
                    .bind(aid).bind(card_id).bind(SqlJson(json!({"content": content}))).execute(&s.db).await?;
                emit(
                    s,
                    Event::CommentCreated {
                        card_id,
                        workspace_id,
                        activity_id: aid,
                        actor: None,
                        mentions: vec![],
                        depth: next,
                    },
                );
            }
            "create_card" | "create_sub_card" => {
                let data = cfg
                    .get("data")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                let stage_id = cfg
                    .get("stage_id")
                    .and_then(Value::as_str)
                    .and_then(|x| Uuid::parse_str(x).ok());
                let (list, parent) = if action == "create_card" {
                    (
                        cfg["list_id"]
                            .as_str()
                            .and_then(|x| Uuid::parse_str(x).ok())
                            .ok_or_else(|| bad("create_card needs `list_id`"))?,
                        None,
                    )
                } else {
                    let card_id = need_card()?;
                    let l: Option<(Uuid,)> = sqlx::query_as("SELECT list_id FROM card_lists WHERE card_id = $1 ORDER BY created_at LIMIT 1").bind(card_id).fetch_optional(&s.db).await?;
                    (l.ok_or_else(|| bad("card is in no list"))?.0, Some(card_id))
                };
                // The target list must belong to the same workspace and be reachable by the automation owner.
                let l: Option<(Uuid,)> = sqlx::query_as(
                    "SELECT workspace_id FROM lists WHERE id = $1 AND deleted_at IS NULL",
                )
                .bind(list)
                .fetch_optional(&s.db)
                .await?;
                if l.map(|x| x.0) != Some(workspace_id) {
                    return Err(ApiError::NotFound);
                }
                crate::authz::require_list_role(&s.db, creator, list, Role::Member)
                    .await
                    .map_err(|_| bad("automation owner cannot write to that list"))?;
                create_card_in_list(
                    s,
                    list,
                    &scope,
                    CreateCardBody {
                        card_type_id: None,
                        data,
                        stage_id,
                        parent_id: parent,
                    },
                    None,
                    true,
                    next,
                )
                .await?;
            }
            "send_webhook" => {
                let url = raw["url"]
                    .as_str()
                    .ok_or_else(|| bad("send_webhook needs `url`"))?; // the URL itself is never templated
                let payload = match cfg.get("body") {
                    Some(b) if !b.is_null() => b.clone(),
                    _ => json!({"automation": automation, "trigger": ctx}),
                };
                let body = serde_json::to_vec(
                    &json!({"event": "automation.run", "run_id": run_id, "data": payload}),
                )
                .map_err(|e| ApiError::Internal(Box::new(e)))?;
                if body.len() > 256 * 1024 {
                    return Err(bad("webhook payload too large"));
                }
                let sig = sign(&signing_key(&s.config.jwt_secret, automation), &body);
                let status = crate::ssrf::deliver(crate::ssrf::Delivery {
                    url,
                    body,
                    headers: vec![
                        ("x-work-event", "automation.run".into()),
                        ("x-work-delivery", run_id.to_string()),
                        ("x-work-signature", sig),
                    ],
                    allow_http: s.config.webhook_allow_http,
                    allow_private: s.config.webhook_allow_private,
                })
                .await
                .map_err(|e| bad(e.to_string()))?;
                if !(200..300).contains(&status) {
                    return Err(bad(format!("webhook target answered HTTP {status}")));
                }
            }
            "send_email" => {
                if !crate::mailer::enabled(s) {
                    return Err(bad("e-mail is not configured on this instance"));
                }
                if !s.email_limiter.check(&automation.to_string()) {
                    return Err(bad(
                        "this automation sent too many e-mails in the last hour",
                    ));
                }
                let to = cfg["to"]
                    .as_str()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .ok_or_else(|| bad("send_email needs `to`"))?;
                // Recipients: a member of this project (by id or e-mail), or anyone if the operator allows it.
                let member: Option<(String,)> = sqlx::query_as(
                    "SELECT u.email::text FROM users u JOIN project_members m ON m.user_id = u.id AND m.project_id = $1
                     WHERE u.deleted_at IS NULL AND (u.id::text = $2 OR u.email = $2)",
                )
                .bind(project.0)
                .bind(to)
                .fetch_optional(&s.db)
                .await?;
                let address = match member {
                    Some((email,)) => email,
                    None if s.config.allow_external_email
                        && to.contains('@')
                        && !to.contains(char::is_whitespace) =>
                    {
                        to.to_owned()
                    }
                    None => return Err(bad("recipient is not a member of this project")),
                };
                let subject: String = cfg["subject"]
                    .as_str()
                    .unwrap_or("Notification from work")
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(200)
                    .collect();
                let text = cfg["body"].as_str().unwrap_or_default();
                if text.chars().count() > 20_000 {
                    return Err(bad("email body too long"));
                }
                let mut html = plain_text_to_html(text);
                if let Some(card) = card_id {
                    let url = crate::notify::card_url(s, workspace_id, *card);
                    html.push_str(&format!(
                        "<p><a href=\"{}\">Open in work</a></p>",
                        crate::mailer::escape_html(&url)
                    ));
                }
                crate::mailer::send(s, address, subject, html);
            }
            other => return Err(bad(format!("unknown action {other}"))),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_lookups_not_expressions() {
        let t = json!({"card": {"data": {"title": "Fix it"}}, "newValue": ["u1"], "type": "x"});
        assert_eq!(
            substitute(&json!("Re: {{trigger.card.data.title}}!"), &t),
            json!("Re: Fix it!")
        );
        assert_eq!(
            substitute(&json!("{{trigger.newValue}}"), &t),
            json!(["u1"])
        ); // type preserved
        assert_eq!(
            substitute(&json!("{{trigger.missing.path}}"), &t),
            Value::Null
        );
        // Anything that looks like code is just text.
        assert_eq!(
            substitute(&json!("{{constructor.constructor('x')()}}"), &t),
            json!("{{constructor.constructor('x')()}}")
        );
        assert_eq!(
            substitute(&json!("{{trigger.card.data.title"), &t),
            json!("{{trigger.card.data.title")
        );
        assert_eq!(
            substitute(&json!({"a": ["{{trigger.type}}", 5]}), &t),
            json!({"a": ["x", 5]})
        );
    }

    #[test]
    fn trigger_matching() {
        let ev = json!({"field": "status", "oldValue": "a", "newValue": "b"});
        assert!(trigger_matches("field_updated", &json!({}), &ev));
        assert!(trigger_matches(
            "field_updated",
            &json!({"field": "status", "to": "b"}),
            &ev
        ));
        assert!(!trigger_matches(
            "field_updated",
            &json!({"field": "other"}),
            &ev
        ));
        assert!(!trigger_matches(
            "field_updated",
            &json!({"from": "z"}),
            &ev
        ));
        let st = json!({"oldValue": "s1", "newValue": "s2"});
        assert!(trigger_matches(
            "stage_changed",
            &json!({"to": ["s2", "s3"]}),
            &st
        ));
        assert!(!trigger_matches(
            "stage_changed",
            &json!({"to": ["s9"]}),
            &st
        ));
        let asg = json!({"oldValue": ["u1"], "newValue": ["u1", "u2"]});
        assert!(trigger_matches(
            "assignee_added",
            &json!({"user_id": "u2"}),
            &asg
        ));
        assert!(!trigger_matches(
            "assignee_added",
            &json!({"user_id": "u1"}),
            &asg
        ));
        assert!(!trigger_matches("assignee_removed", &json!({}), &asg));
    }
}
