//! HTTP side of automations: the catalog the UI offers, create/edit/delete, runs, and webhooks.

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::types::Json as SqlJson;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    routes::OkOut,
    security::auth::AuthUser,
    security::authz::{require_workspace_role, Role},
    state::AppState,
};

use super::engine::{finish_run, signing_key, start_run, RunInput};
use super::{bad, ACTIONS, TRIGGERS};

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
            h(
                "date_arrives",
                "Date arrives",
                "A date field of a card (`field`, default `due_at`) is reached. `offset_minutes` shifts the moment (negative = before). Date-only values count as 00:00 UTC.",
                &["field", "offset_minutes"],
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
        if trigger == Some("date_arrives") {
            crate::services::scheduler::DateConfig::parse(c).map_err(bad)?;
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
                crate::security::ssrf::validate_url(url, allow_http)
                    .map_err(|e| bad(e.to_string()))?;
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
    let token = crate::security::auth::random_token();
    sqlx::query("UPDATE automations SET webhook_token_hash = $2 WHERE id = $1")
        .bind(id)
        .bind(crate::security::auth::hash_token(&token))
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
    if !s.hook_limiter.check(&format!("ip:{ip}")).await
        || !s.hook_limiter.check(&format!("automation:{id}")).await
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
    if !bool::from(stored.ct_eq(&crate::security::auth::hash_token(&token))) {
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
