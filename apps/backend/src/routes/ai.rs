//! The assistant's endpoints. They exist only while the instance has a self-hosted language model
//! (set by an admin in the app, or in the environment); without one every endpoint here answers
//! 503 and `/ai/status` says so, which is what switches the AI features off in the app. Two
//! engines sit behind them:
//! * the language model (`services::ai`): rewriting, translating, summaries and child-issue ideas;
//! * the built-in analysis (`services::intel`): runs in this process on the issues the caller may
//!   see: related issues and duplicates, field suggestions, the planning and board reports.
//!
//! Everything here only *returns suggestions*. Nothing is written; applying one is an ordinary,
//! permission-checked API call by the person.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::types::Json as SqlJson;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::{
    admin::{audit, require_admin},
    card_types::fields_for_card_type,
};
use crate::{
    config::{AiConfig, DEFAULT_MAX_OUTPUT_TOKENS, DEFAULT_MODEL_NAME},
    domain::rich,
    error::{ApiError, ApiResult},
    security::{
        auth::AuthUser,
        authz::{require_card_role, require_role, require_workspace_role, visible_cards_sql, Role},
    },
    services::{
        ai::{self, Rewrite},
        intel::{self, Doc, Index},
    },
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(status))
        .routes(routes!(admin_get, admin_set, admin_remove))
        .routes(routes!(admin_test))
        .routes(routes!(summarize))
        .routes(routes!(related))
        .routes(routes!(suggest_fields))
        .routes(routes!(suggest_subtasks))
        .routes(routes!(rewrite))
        .routes(routes!(rewrite_text))
        .routes(routes!(draft_description))
        .routes(routes!(check_tone))
        .routes(routes!(suggest_replies))
        .routes(routes!(similar_issues))
        .routes(routes!(suggest_draft_fields))
}

// ---- the admin's setting: which self-hosted model -------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct AiSettingsOut {
    /// The AI features are on (a model is in use, from the setting or the environment).
    pub enabled: bool,
    /// `settings` (entered in the app), `environment` (WORK_AI_MODEL_URL) or `none`.
    pub source: String,
    pub model_url: Option<String>,
    pub model_name: Option<String>,
    pub max_output_tokens: u32,
    /// The environment's model address, when there is one: what removing the setting falls back to.
    pub environment_url: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct AiSettingsBody {
    /// Address of the server, e.g. `http://model:8081` or `http://localhost:11434`. It must speak
    /// the OpenAI chat protocol (`/v1/chat/completions`): llama.cpp, Ollama, vLLM, LocalAI.
    pub model_url: String,
    /// The model name sent along; many servers ignore it, Ollama needs it.
    pub model_name: Option<String>,
    pub max_output_tokens: Option<u32>,
}

impl AiSettingsBody {
    fn config(&self) -> ApiResult<AiConfig> {
        AiConfig::parse(
            &self.model_url,
            self.model_name
                .as_deref()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or(DEFAULT_MODEL_NAME),
            self.max_output_tokens.unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS),
        )
        .map_err(bad)
    }
}

async fn settings_out(s: &AppState) -> ApiResult<AiSettingsOut> {
    let row: Option<(String, String, i32)> =
        sqlx::query_as("SELECT model_url, model_name, max_output_tokens FROM ai_settings")
            .fetch_optional(&s.db)
            .await?;
    let environment_url = s.config.ai.as_ref().map(|c| c.model_url.clone());
    let (source, active) = match (row, &s.config.ai) {
        (Some((u, n, m)), _) => ("settings", Some((u, n, m.max(0) as u32))),
        (None, Some(c)) => (
            "environment",
            Some((
                c.model_url.clone(),
                c.model_name.clone(),
                c.max_output_tokens,
            )),
        ),
        (None, None) => ("none", None),
    };
    Ok(AiSettingsOut {
        enabled: s.ai().await.is_some(),
        source: source.into(),
        model_url: active.as_ref().map(|a| a.0.clone()),
        model_name: active.as_ref().map(|a| a.1.clone()),
        max_output_tokens: active.map(|a| a.2).unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS),
        environment_url,
    })
}

/// The model setting. Instance admins only.
#[utoipa::path(get, operation_id = "getAiSettings", path = "/admin/ai", tag = "ai",
    responses((status = 200, body = AiSettingsOut), (status = 404)), security(("bearer" = [])))]
async fn admin_get(State(s): State<AppState>, user: AuthUser) -> ApiResult<Json<AiSettingsOut>> {
    require_admin(&s, &user).await?;
    Ok(Json(settings_out(&s).await?))
}

/// Sets the self-hosted model. This switches the AI features on for everyone. The address is
/// checked for form only; use the test to see whether the server answers.
#[utoipa::path(put, operation_id = "setAiSettings", path = "/admin/ai", tag = "ai",
    request_body = AiSettingsBody,
    responses((status = 200, body = AiSettingsOut), (status = 400), (status = 404)),
    security(("bearer" = [])))]
async fn admin_set(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<AiSettingsBody>,
) -> ApiResult<Json<AiSettingsOut>> {
    require_admin(&s, &user).await?;
    let c = b.config()?;
    sqlx::query(
        "INSERT INTO ai_settings (singleton, model_url, model_name, max_output_tokens, updated_by, updated_at)
         VALUES (true, $1, $2, $3, $4, now())
         ON CONFLICT (singleton) DO UPDATE SET model_url = $1, model_name = $2, max_output_tokens = $3,
             updated_by = $4, updated_at = now()",
    )
    .bind(&c.model_url)
    .bind(&c.model_name)
    .bind(c.max_output_tokens as i32)
    .bind(user.id)
    .execute(&s.db)
    .await?;
    audit(
        &s,
        user.id,
        "ai.set",
        None,
        json!({"model_url": c.model_url, "model_name": c.model_name}),
    )
    .await?;
    s.refresh_ai().await;
    Ok(Json(settings_out(&s).await?))
}

/// Removes the setting. The AI features then follow the environment: on if `WORK_AI_MODEL_URL` is
/// set, off otherwise.
#[utoipa::path(delete, operation_id = "removeAiSettings", path = "/admin/ai", tag = "ai",
    responses((status = 200, body = AiSettingsOut), (status = 404)), security(("bearer" = [])))]
async fn admin_remove(State(s): State<AppState>, user: AuthUser) -> ApiResult<Json<AiSettingsOut>> {
    require_admin(&s, &user).await?;
    sqlx::query("DELETE FROM ai_settings")
        .execute(&s.db)
        .await?;
    audit(&s, user.id, "ai.remove", None, json!({})).await?;
    s.refresh_ai().await;
    Ok(Json(settings_out(&s).await?))
}

#[derive(Serialize, ToSchema)]
pub struct AiTestOut {
    pub ok: bool,
    /// Why it failed, in a few fixed words; empty when `ok`.
    pub message: String,
}

/// Asks the model at this address for one word, without saving anything.
#[utoipa::path(post, operation_id = "testAiSettings", path = "/admin/ai/test", tag = "ai",
    request_body = AiSettingsBody,
    responses((status = 200, body = AiTestOut), (status = 400), (status = 404), (status = 429)),
    security(("bearer" = [])))]
async fn admin_test(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<AiSettingsBody>,
) -> ApiResult<Json<AiTestOut>> {
    require_admin(&s, &user).await?;
    let c = b.config()?;
    limit(&s, &user).await?;
    let client = ai::Ai::new(c).map_err(|e| ApiError::Internal(e.into()))?;
    Ok(Json(match client.probe().await {
        Ok(()) => AiTestOut {
            ok: true,
            message: String::new(),
        },
        Err(m) => AiTestOut {
            ok: false,
            message: m.into(),
        },
    }))
}

/// Issues the analysis looks at per request: the most recently changed ones.
const CORPUS_LIMIT: i64 = 3000;
/// Characters of an issue and its discussion handed to the model.
const MODEL_INPUT_CHARS: usize = 16_000;

/// The language model, or 503: the AI features are off until an admin sets one.
pub(crate) async fn model(s: &AppState) -> ApiResult<std::sync::Arc<ai::Ai>> {
    s.ai().await.ok_or(ApiError::Unavailable(
        "the AI features are off: no language model is set up on this instance",
    ))
}

/// Every AI endpoint starts with this: the features must be on, and the caller within the limit.
async fn throttle(s: &AppState, user: &AuthUser) -> ApiResult<()> {
    model(s).await?;
    limit(s, user).await
}

async fn limit(s: &AppState, user: &AuthUser) -> ApiResult<()> {
    if s.ai_limiter.check(&user.id.to_string()).await {
        Ok(())
    } else {
        Err(ApiError::TooManyRequests)
    }
}

fn bad(m: impl Into<String>) -> ApiError {
    ApiError::BadRequest(m.into())
}

fn language(code: Option<&str>) -> ApiResult<Option<&'static str>> {
    match code {
        None => Ok(None),
        Some(c) => ai::language_name(c)
            .map(Some)
            .ok_or_else(|| bad("unsupported language")),
    }
}

// ---- status ---------------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct LanguageOut {
    pub code: String,
    pub name: String,
}

#[derive(Serialize, ToSchema)]
pub struct AiStatus {
    /// A self-hosted language model is set up: the AI features are on. Otherwise the app hides them.
    pub enabled: bool,
    pub model_name: Option<String>,
    pub languages: Vec<LanguageOut>,
}

#[utoipa::path(get, operation_id = "getAiStatus", path = "/ai/status", tag = "ai",
    responses((status = 200, body = AiStatus)), security(("bearer" = [])))]
async fn status(State(s): State<AppState>, _user: AuthUser) -> Json<AiStatus> {
    let ai = s.ai().await;
    Json(AiStatus {
        enabled: ai.is_some(),
        model_name: ai.as_ref().map(|a| a.model().to_owned()),
        languages: ai::LANGUAGES
            .iter()
            .map(|(c, n)| LanguageOut {
                code: (*c).into(),
                name: (*n).into(),
            })
            .collect(),
    })
}

// ---- what the engines read ------------------------------------------------------------------

/// An issue of the corpus with the data suggestions are voted from.
pub struct Row {
    pub id: Uuid,
    pub doc: Doc,
    pub data: Map<String, Value>,
}

/// The issues of a workspace the caller may see, newest change first.
async fn corpus(
    s: &AppState,
    user: &AuthUser,
    workspace_id: Uuid,
    admin: bool,
) -> ApiResult<Vec<Row>> {
    let vis = visible_cards_sql("c", 2, 3);
    let sql = format!(
        "SELECT c.id,
            (SELECT w.key_prefix || '-' || c.number FROM workspaces w WHERE w.id = c.workspace_id),
            COALESCE((SELECT c.data ->> tf.slug FROM fields tf WHERE (tf.card_type_id = c.card_type_id OR tf.card_type_id IS NULL) AND tf.is_title AND tf.deleted_at IS NULL LIMIT 1), ''),
            (SELECT c.data -> df.slug FROM fields df WHERE (df.card_type_id = c.card_type_id OR df.card_type_id IS NULL) AND df.is_description AND df.deleted_at IS NULL LIMIT 1),
            c.data
         FROM cards c WHERE c.workspace_id = $1 AND c.deleted_at IS NULL AND {vis}
         ORDER BY c.updated_at DESC LIMIT $4"
    );
    type CorpusRow = (Uuid, String, String, Option<SqlJson<Value>>, SqlJson<Value>);
    let rows: Vec<CorpusRow> = sqlx::query_as(&sql)
        .bind(workspace_id)
        .bind(admin)
        .bind(user.id)
        .bind(CORPUS_LIMIT)
        .fetch_all(&s.db)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(id, key, title, desc, data)| Row {
            id,
            doc: Doc {
                key,
                title,
                body: desc.map(|d| rich::to_markdown(&d.0)).unwrap_or_default(),
            },
            data: data.0.as_object().cloned().unwrap_or_default(),
        })
        .collect())
}

struct Issue {
    key: String,
    title: String,
    description: String,
    comments: Vec<String>,
    children: Vec<String>,
}

impl Issue {
    /// Title, description, discussion and child issues as one text for the engines.
    fn as_text(&self) -> String {
        let mut out = format!("{}: {}\n", self.key, self.title);
        if !self.description.is_empty() {
            out += &format!("\nDescription:\n{}\n", self.description);
        }
        if !self.children.is_empty() {
            out += "\nChild issues:\n";
            for c in &self.children {
                out += &format!("- {c}\n");
            }
        }
        if !self.comments.is_empty() {
            out += "\nComments, oldest first:\n";
            for c in &self.comments {
                out += &format!("- {c}\n");
            }
        }
        out.chars().take(MODEL_INPUT_CHARS).collect()
    }

    /// Only what a person wrote, without the issue key line or headings: the extractive summary
    /// picks sentences from it.
    fn prose(&self) -> String {
        let mut out = self.description.clone();
        for c in &self.comments {
            out += &format!("\n{c}");
        }
        out
    }
}

async fn load_issue(
    s: &AppState,
    card_id: Uuid,
    workspace_id: Uuid,
) -> ApiResult<(Issue, Value, Uuid)> {
    let card = super::cards::load_card(s, card_id).await?;
    let fields = fields_for_card_type(&s.db, workspace_id, card.card_type_id).await?;
    let slug_of = |pick: fn(&super::card_types::FieldOut) -> bool| {
        fields.iter().find(|f| pick(f)).map(|f| f.slug.clone())
    };
    let title = slug_of(|f| f.is_title)
        .and_then(|k| {
            card.data
                .0
                .get(&k)
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let description = slug_of(|f| f.is_description)
        .and_then(|k| card.data.0.get(&k).map(rich::to_markdown))
        .unwrap_or_default();
    let comments: Vec<(SqlJson<Value>,)> = sqlx::query_as(
        "SELECT content FROM card_activities WHERE card_id = $1 AND type = 'comment' ORDER BY created_at LIMIT 40",
    )
    .bind(card_id)
    .fetch_all(&s.db)
    .await?;
    let comments = comments
        .into_iter()
        .filter_map(|(c,)| c.0.get("content").map(rich::to_markdown))
        .map(|t| t.replace('\n', " "))
        .filter(|t| !t.trim().is_empty())
        .collect();
    let kids: Vec<(String,)> = sqlx::query_as(
        "SELECT COALESCE((SELECT ch.data ->> tf.slug FROM fields tf WHERE (tf.card_type_id = ch.card_type_id OR tf.card_type_id IS NULL) AND tf.is_title AND tf.deleted_at IS NULL LIMIT 1), '')
         FROM cards ch WHERE ch.parent_id = $1 AND ch.deleted_at IS NULL ORDER BY ch.created_at LIMIT 50",
    )
    .bind(card_id)
    .fetch_all(&s.db)
    .await?;
    let issue = Issue {
        key: card.key.clone(),
        title,
        description,
        comments,
        children: kids
            .into_iter()
            .map(|(t,)| t)
            .filter(|t| !t.is_empty())
            .collect(),
    };
    Ok((issue, card.data.0, card.card_type_id))
}

// ---- summarize ------------------------------------------------------------------------------

#[derive(Deserialize, ToSchema)]
pub struct SummarizeBody {
    /// Answer language, for the model (`en`, `de`, ...). Default: the language of the issue.
    pub language: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct SummaryOut {
    /// Markdown.
    pub text: String,
    /// The same text as an editor document, to show it formatted.
    #[schema(value_type = Object)]
    pub document: Value,
    /// `model` (written by the local language model) or `extractive` (the most telling
    /// sentences of the issue, chosen by the built-in analysis).
    pub source: String,
    pub keywords: Vec<String>,
}

#[utoipa::path(post, operation_id = "summarizeCard", path = "/cards/{id}/ai/summarize", tag = "ai",
    params(("id" = Uuid, Path)), request_body = SummarizeBody,
    responses((status = 200, body = SummaryOut), (status = 400), (status = 404), (status = 429), (status = 502)),
    security(("bearer" = [])))]
async fn summarize(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<SummarizeBody>,
) -> ApiResult<Json<SummaryOut>> {
    let scope = require_card_role(&s.db, user.id, id, Role::Viewer).await?;
    let lang = language(b.language.as_deref())?;
    throttle(&s, &user).await?;
    let (issue, ..) = load_issue(&s, id, scope.workspace_id).await?;
    let keywords = intel::keywords(&format!("{} {}", issue.title, issue.prose()), 6);
    let ai = model(&s).await?;
    let (system, user_msg) = ai::summary_prompt(lang, &issue.as_text());
    let text = ai.complete(&system, &user_msg).await?;
    Ok(Json(SummaryOut {
        document: rich::from_markdown(&text)
            .map_err(|e| bad(format!("the model's answer cannot be used: {e}")))?,
        text,
        source: "model".into(),
        keywords,
    }))
}

// ---- related issues and duplicates ----------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct RelatedOut {
    pub card_id: Uuid,
    pub key: String,
    pub title: String,
    /// `duplicate` (very probably the same issue) or `related`.
    pub relation: String,
    /// 0 to 1.
    pub score: f32,
    /// The words the two issues have in common, strongest first.
    pub shared: Vec<String>,
}

#[utoipa::path(post, operation_id = "relatedCards", path = "/cards/{id}/ai/related", tag = "ai",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<RelatedOut>), (status = 404), (status = 429)),
    security(("bearer" = [])))]
async fn related(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<RelatedOut>>> {
    model(&s).await?;
    related_issues(&s, &user, id).await
}

/// The analysis behind `related`. The MCP tool `find_related_issues` calls it directly: agents get
/// it whether or not a language model is set up.
pub(crate) async fn related_issues(
    s: &AppState,
    user: &AuthUser,
    id: Uuid,
) -> ApiResult<Json<Vec<RelatedOut>>> {
    let scope = require_card_role(&s.db, user.id, id, Role::Viewer).await?;
    let admin = require_role(&s.db, user.id, scope.project_id, Role::Viewer).await? >= Role::Admin;
    limit(s, user).await?;
    let rows = corpus(s, user, scope.workspace_id, admin).await?;
    let out = blocking(move || {
        let me = rows.iter().position(|r| r.id == id)?;
        let docs: Vec<Doc> = rows.iter().map(|r| r.doc.clone()).collect();
        let idx = Index::build(&docs);
        let d = &docs[me];
        Some(
            idx.similar(&d.title, &d.body, Some(me), 8)
                .into_iter()
                .filter(|h| h.score >= intel::RELATED_SCORE)
                .map(|h| related_out(&rows, h))
                .collect::<Vec<_>>(),
        )
    })
    .await?;
    // The issue is not in the newest `CORPUS_LIMIT`: nothing to compare yet, not an error.
    Ok(Json(out.unwrap_or_default()))
}

fn related_out(rows: &[Row], h: intel::Hit) -> RelatedOut {
    RelatedOut {
        card_id: rows[h.doc].id,
        key: rows[h.doc].doc.key.clone(),
        title: rows[h.doc].doc.title.clone(),
        relation: if h.score >= intel::DUPLICATE_SCORE {
            "duplicate"
        } else {
            "related"
        }
        .into(),
        score: (h.score * 100.0).round() / 100.0,
        shared: h.shared,
    }
}

/// CPU-bound work off the async threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> ApiResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError::Internal(Box::new(e)))
}

// ---- field suggestions ----------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct FieldSuggestion {
    pub slug: String,
    pub name: String,
    /// A string, or a list of strings for fields that hold several values.
    #[schema(value_type = Object)]
    pub value: Value,
    /// Share of the similar issues that agree, 0 to 1.
    pub confidence: f32,
    /// How many similar issues it was voted from.
    pub based_on: usize,
}

/// Values of one choice field among the neighbours: the single-value majority, or for a field that
/// holds several, every value most neighbours carry.
fn strings_of(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect(),
        _ => vec![],
    }
}

/// Empty choice fields (dropdowns, labels) of an issue, filled with what the most similar issues
/// agree on. `skip` names fields that are filled in already.
fn vote_fields(
    rows: &[Row],
    hits: &[intel::Hit],
    fields: &[super::card_types::FieldOut],
    data: &Value,
    skip: &[String],
) -> Vec<FieldSuggestion> {
    let mut out = vec![];
    for f in fields
        .iter()
        .filter(|f| matches!(f.r#type.as_str(), "dropdown" | "label") && !f.is_title)
    {
        if skip.contains(&f.slug) || data.get(&f.slug).is_some_and(|v| !v.is_null()) {
            continue; // only fill what is empty
        }
        let allowed: Option<Vec<String>> = f
            .items
            .as_ref()
            .and_then(|i| i.0.as_array())
            .filter(|a| !a.is_empty())
            .map(|a| {
                a.iter()
                    .filter_map(|i| i.get("item").and_then(Value::as_str).map(str::to_owned))
                    .collect()
            });
        let ok = |v: &String| allowed.as_ref().is_none_or(|a| a.contains(v));
        let votes: Vec<(&intel::Hit, Vec<String>)> = hits
            .iter()
            .map(|h| {
                let vals = rows[h.doc]
                    .data
                    .get(&f.slug)
                    .map(strings_of)
                    .unwrap_or_default();
                (h, vals.into_iter().filter(ok).collect())
            })
            .filter(|(_, v): &(_, Vec<String>)| !v.is_empty())
            .collect();
        if f.multiple || f.r#type == "label" {
            let total: f32 = votes.iter().map(|(h, _)| h.score).sum();
            let mut all: Vec<String> = votes.iter().flat_map(|(_, v)| v.clone()).collect();
            all.sort();
            all.dedup();
            let picked: Vec<(String, f32)> = all
                .into_iter()
                .filter_map(|val| {
                    let w: f32 = votes
                        .iter()
                        .filter(|(_, v)| v.contains(&val))
                        .map(|(h, _)| h.score)
                        .sum();
                    (votes.len() >= 2 && total > 0.0 && w / total > 0.5).then(|| (val, w / total))
                })
                .collect();
            if !picked.is_empty() {
                let conf = picked.iter().map(|(_, c)| c).sum::<f32>() / picked.len() as f32;
                out.push(FieldSuggestion {
                    slug: f.slug.clone(),
                    name: f.name.clone(),
                    value: json!(picked.into_iter().map(|(v, _)| v).collect::<Vec<_>>()),
                    confidence: (conf * 100.0).round() / 100.0,
                    based_on: votes.len(),
                });
            }
        } else {
            let ballots: Vec<(String, f32)> =
                votes.iter().map(|(h, v)| (v[0].clone(), h.score)).collect();
            if let Some((value, share)) = intel::vote(&ballots) {
                out.push(FieldSuggestion {
                    slug: f.slug.clone(),
                    name: f.name.clone(),
                    value: json!(value),
                    confidence: (share * 100.0).round() / 100.0,
                    based_on: ballots.len(),
                });
            }
        }
    }
    out
}

#[utoipa::path(post, operation_id = "suggestCardFields", path = "/cards/{id}/ai/suggest", tag = "ai",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<FieldSuggestion>), (status = 404), (status = 429)),
    security(("bearer" = [])))]
async fn suggest_fields(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<FieldSuggestion>>> {
    let scope = require_card_role(&s.db, user.id, id, Role::Member).await?;
    let admin = require_role(&s.db, user.id, scope.project_id, Role::Viewer).await? >= Role::Admin;
    throttle(&s, &user).await?;
    let (_, data, card_type) = load_issue(&s, id, scope.workspace_id).await?;
    let fields = fields_for_card_type(&s.db, scope.workspace_id, card_type).await?;
    let rows = corpus(&s, &user, scope.workspace_id, admin).await?;
    let out = blocking(move || {
        let Some(me) = rows.iter().position(|r| r.id == id) else {
            return vec![];
        };
        let docs: Vec<Doc> = rows.iter().map(|r| r.doc.clone()).collect();
        let idx = Index::build(&docs);
        let hits: Vec<_> = idx
            .similar(&docs[me].title, &docs[me].body, Some(me), 8)
            .into_iter()
            .filter(|h| h.score >= intel::RELATED_SCORE)
            .collect();
        vote_fields(&rows, &hits, &fields, &data, &[])
    })
    .await?;
    Ok(Json(out))
}

// ---- model: child issues, rewriting ----------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct SubtasksOut {
    pub suggestions: Vec<String>,
}

#[utoipa::path(post, operation_id = "suggestSubtasks", path = "/cards/{id}/ai/subtasks", tag = "ai",
    params(("id" = Uuid, Path)), responses((status = 200, body = SubtasksOut), (status = 404), (status = 429), (status = 502), (status = 503)),
    security(("bearer" = [])))]
async fn suggest_subtasks(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SubtasksOut>> {
    let scope = require_card_role(&s.db, user.id, id, Role::Member).await?;
    let ai = model(&s).await?;
    throttle(&s, &user).await?;
    let (issue, ..) = load_issue(&s, id, scope.workspace_id).await?;
    let (system, msg) = ai::subtasks_prompt(&issue.as_text());
    let answer = ai.complete(&system, &msg).await?;
    Ok(Json(SubtasksOut {
        suggestions: ai::parse_suggestions(&answer),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct RewriteBody {
    /// `improve`, `fix_spelling`, `shorten`, `expand`, `professional`, `friendly` or `translate`.
    pub action: String,
    /// Target language for `translate`.
    pub language: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct RewriteOut {
    /// Markdown.
    pub text: String,
    /// The same text as an editor document, ready to be saved as the description.
    #[schema(value_type = Object)]
    pub document: Value,
}

#[utoipa::path(post, operation_id = "rewriteCardDescription", path = "/cards/{id}/ai/rewrite", tag = "ai",
    params(("id" = Uuid, Path)), request_body = RewriteBody,
    responses((status = 200, body = RewriteOut), (status = 400), (status = 403), (status = 404), (status = 429), (status = 502), (status = 503)),
    security(("bearer" = [])))]
async fn rewrite(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<RewriteBody>,
) -> ApiResult<Json<RewriteOut>> {
    let scope = require_card_role(&s.db, user.id, id, Role::Member).await?;
    let action = Rewrite::parse(&b.action).ok_or_else(|| bad("unknown action"))?;
    let lang = language(b.language.as_deref())?;
    if action == Rewrite::Translate && lang.is_none() {
        return Err(bad("translate needs a language"));
    }
    let ai = model(&s).await?;
    throttle(&s, &user).await?;
    let (issue, ..) = load_issue(&s, id, scope.workspace_id).await?;
    if issue.description.trim().is_empty() {
        return Err(bad("the issue has no description to work on"));
    }
    let (system, msg) = ai::rewrite_prompt(action, lang, &issue.description);
    let text = ai.complete(&system, &msg).await?;
    // A model's Markdown can be too long for a document, or odd: refuse instead of saving it.
    let document = rich::from_markdown(&text)
        .map_err(|e| bad(format!("the model's answer cannot be used: {e}")))?;
    Ok(Json(RewriteOut { text, document }))
}

// ---- text without an issue: the editor, the comment box, the create dialog -------------------

/// The text a request brings: Markdown, or a document from the editor (which is converted).
fn input_markdown(text: Option<String>, document: Option<Value>) -> ApiResult<String> {
    let md = match (text, document) {
        (Some(t), None) => t,
        (None, Some(d)) => {
            rich::validate_value(&d).map_err(|e| bad(format!("document: {e}")))?;
            rich::to_markdown(&d)
        }
        _ => return Err(bad("send either `text` or `document`")),
    };
    if md.trim().is_empty() {
        return Err(bad("there is no text to work on"));
    }
    if md.chars().count() > 20_000 {
        return Err(bad("the text is longer than 20000 characters"));
    }
    Ok(md)
}

fn answer(text: String) -> ApiResult<Json<RewriteOut>> {
    // A model's Markdown can be too long for a document, or odd: refuse instead of passing it on.
    let document =
        rich::from_markdown(&text).map_err(|e| bad(format!("the answer cannot be used: {e}")))?;
    Ok(Json(RewriteOut { text, document }))
}

#[derive(Deserialize, ToSchema)]
pub struct TextBody {
    /// `improve`, `fix_spelling`, `shorten`, `expand`, `professional`, `friendly` or `translate`.
    pub action: String,
    pub language: Option<String>,
    /// Markdown. Send this or `document`.
    pub text: Option<String>,
    /// A document from the editor.
    #[schema(value_type = Option<Object>)]
    pub document: Option<Value>,
}

/// Rewrites or translates a text that is not (yet) part of an issue: the draft of a comment, a selection
/// in the editor, someone else's comment. Nothing is stored.
#[utoipa::path(post, operation_id = "rewriteText", path = "/ai/text", tag = "ai",
    request_body = TextBody,
    responses((status = 200, body = RewriteOut), (status = 400), (status = 429), (status = 502), (status = 503)),
    security(("bearer" = [])))]
async fn rewrite_text(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<TextBody>,
) -> ApiResult<Json<RewriteOut>> {
    let action = Rewrite::parse(&b.action).ok_or_else(|| bad("unknown action"))?;
    let lang = language(b.language.as_deref())?;
    if action == Rewrite::Translate && lang.is_none() {
        return Err(bad("translate needs a language"));
    }
    let md = input_markdown(b.text, b.document)?;
    let ai = model(&s).await?;
    throttle(&s, &user).await?;
    let (system, msg) = ai::rewrite_prompt(action, lang, &md);
    let text = ai.complete(&system, &msg).await?;
    answer(text)
}

#[derive(Deserialize, ToSchema)]
pub struct DraftBody {
    pub title: String,
    /// The issue type's name (Bug, Story, Task, ...); it decides the layout of the draft.
    pub kind: Option<String>,
    /// Language of the draft (`de`, `en`, ...). Default: the language of the title.
    pub language: Option<String>,
}

/// A first draft of a description, from the title and the type of an issue. Needs the language model.
#[utoipa::path(post, operation_id = "draftDescription", path = "/ai/draft", tag = "ai",
    request_body = DraftBody,
    responses((status = 200, body = RewriteOut), (status = 400), (status = 429), (status = 502), (status = 503)),
    security(("bearer" = [])))]
async fn draft_description(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<DraftBody>,
) -> ApiResult<Json<RewriteOut>> {
    let lang = language(b.language.as_deref())?;
    if b.title.trim().is_empty() || b.title.chars().count() > 500 {
        return Err(bad("the title must be 1-500 characters"));
    }
    if b.kind.as_deref().is_some_and(|k| k.chars().count() > 60) {
        return Err(bad("the type name is too long"));
    }
    let ai = model(&s).await?;
    throttle(&s, &user).await?;
    let (system, msg) = ai::draft_prompt(b.title.trim(), b.kind.as_deref(), lang);
    answer(ai.complete(&system, &msg).await?)
}

#[derive(Deserialize, ToSchema)]
pub struct ToneBody {
    pub text: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub document: Option<Value>,
}

#[derive(Serialize, ToSchema)]
pub struct ToneOut {
    /// The comment reads as polite and constructive.
    pub ok: bool,
    /// A friendlier version with the same content, when it is not `ok`.
    pub suggestion: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub document: Option<Value>,
}

/// Checks the tone of a comment before it is posted. Needs the language model.
#[utoipa::path(post, operation_id = "checkTone", path = "/ai/tone", tag = "ai",
    request_body = ToneBody,
    responses((status = 200, body = ToneOut), (status = 400), (status = 429), (status = 502), (status = 503)),
    security(("bearer" = [])))]
async fn check_tone(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<ToneBody>,
) -> ApiResult<Json<ToneOut>> {
    let md = input_markdown(b.text, b.document)?;
    let ai = model(&s).await?;
    throttle(&s, &user).await?;
    let (system, msg) = ai::tone_prompt(&md);
    let reply = ai.complete(&system, &msg).await?;
    if reply
        .trim()
        .trim_matches(['.', '!'])
        .eq_ignore_ascii_case("ok")
    {
        return Ok(Json(ToneOut {
            ok: true,
            suggestion: None,
            document: None,
        }));
    }
    let document = rich::from_markdown(&reply).ok();
    Ok(Json(ToneOut {
        ok: false,
        suggestion: Some(reply),
        document,
    }))
}

/// Three short replies to the latest comment of an issue. Needs the language model.
#[utoipa::path(post, operation_id = "suggestReplies", path = "/cards/{id}/ai/replies", tag = "ai",
    params(("id" = Uuid, Path)), responses((status = 200, body = SubtasksOut), (status = 404), (status = 429), (status = 502), (status = 503)),
    security(("bearer" = [])))]
async fn suggest_replies(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SubtasksOut>> {
    let scope = require_card_role(&s.db, user.id, id, Role::Member).await?;
    let ai = model(&s).await?;
    throttle(&s, &user).await?;
    let (issue, ..) = load_issue(&s, id, scope.workspace_id).await?;
    let (system, msg) = ai::replies_prompt(&issue.as_text());
    let answer = ai.complete(&system, &msg).await?;
    Ok(Json(SubtasksOut {
        suggestions: ai::parse_suggestions(&answer).into_iter().take(3).collect(),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct DraftIssueBody {
    pub workspace_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    /// The issue type, for the suggestion of field values.
    pub card_type_id: Option<Uuid>,
    /// Fields (slugs) the person has filled in already: no suggestion for those.
    pub filled: Option<Vec<String>>,
}

fn check_draft(b: &DraftIssueBody) -> ApiResult<()> {
    if b.title.trim().is_empty() || b.title.chars().count() > 500 {
        return Err(bad("the title must be 1-500 characters"));
    }
    if b.description
        .as_deref()
        .is_some_and(|d| d.chars().count() > 20_000)
    {
        return Err(bad("the description is longer than 20000 characters"));
    }
    Ok(())
}

/// Issues that look like the one being written, before it exists: against duplicates.
#[utoipa::path(post, operation_id = "similarIssues", path = "/ai/similar", tag = "ai",
    request_body = DraftIssueBody,
    responses((status = 200, body = Vec<RelatedOut>), (status = 400), (status = 404), (status = 429)),
    security(("bearer" = [])))]
async fn similar_issues(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<DraftIssueBody>,
) -> ApiResult<Json<Vec<RelatedOut>>> {
    check_draft(&b)?;
    let project_id = require_workspace_role(&s.db, user.id, b.workspace_id, Role::Viewer).await?;
    let admin = require_role(&s.db, user.id, project_id, Role::Viewer).await? >= Role::Admin;
    throttle(&s, &user).await?;
    let rows = corpus(&s, &user, b.workspace_id, admin).await?;
    let (title, body) = (b.title, b.description.unwrap_or_default());
    let out = blocking(move || {
        let docs: Vec<Doc> = rows.iter().map(|r| r.doc.clone()).collect();
        Index::build(&docs)
            .similar(&title, &body, None, 5)
            .into_iter()
            .filter(|h| h.score >= intel::RELATED_SCORE)
            .map(|h| related_out(&rows, h))
            .collect::<Vec<_>>()
    })
    .await?;
    Ok(Json(out))
}

/// Values for the empty choice fields of an issue that is still being written.
#[utoipa::path(post, operation_id = "suggestDraftFields", path = "/ai/suggest-fields", tag = "ai",
    request_body = DraftIssueBody,
    responses((status = 200, body = Vec<FieldSuggestion>), (status = 400), (status = 403), (status = 404), (status = 429)),
    security(("bearer" = [])))]
async fn suggest_draft_fields(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<DraftIssueBody>,
) -> ApiResult<Json<Vec<FieldSuggestion>>> {
    check_draft(&b)?;
    let project_id = require_workspace_role(&s.db, user.id, b.workspace_id, Role::Member).await?;
    let admin = require_role(&s.db, user.id, project_id, Role::Viewer).await? >= Role::Admin;
    let card_type = b
        .card_type_id
        .ok_or_else(|| bad("`card_type_id` is required"))?;
    throttle(&s, &user).await?;
    let fields = fields_for_card_type(&s.db, b.workspace_id, card_type).await?;
    let rows = corpus(&s, &user, b.workspace_id, admin).await?;
    let (title, body, filled) = (
        b.title,
        b.description.unwrap_or_default(),
        b.filled.unwrap_or_default(),
    );
    let out = blocking(move || {
        let docs: Vec<Doc> = rows.iter().map(|r| r.doc.clone()).collect();
        let hits: Vec<_> = Index::build(&docs)
            .similar(&title, &body, None, 8)
            .into_iter()
            .filter(|h| h.score >= intel::RELATED_SCORE)
            .collect();
        vote_fields(&rows, &hits, &fields, &Value::Null, &filled)
    })
    .await?;
    Ok(Json(out))
}
