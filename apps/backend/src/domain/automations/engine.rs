//! The engine: matches domain events to automations, runs their steps with the owner's rights,
//! and substitutes placeholders (plain path lookups, never evaluated).

use serde_json::{json, Map, Value};
use sqlx::types::Json as SqlJson;
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    routes::cards::{create_card_in_list, set_card_stage, update_card_data, CreateCardBody},
    security::authz::{require_role, Role, Scope},
    services::events::{emit, Event, MAX_DEPTH},
    state::AppState,
};

use super::bad;

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
        // `date_arrives` cards are selected by the scheduler's query, not by an event.
        "card_created" | "comment_created" | "webhook" | "date_arrives" => true,
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
        if !card_in_scope(s, id, card_id).await? {
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

/// Location filter: the card must be in a list covered by the automation (or none are configured).
pub(crate) async fn card_in_scope(
    s: &AppState,
    automation: Uuid,
    card_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let (in_scope,): (bool,) = sqlx::query_as(
        "SELECT (NOT EXISTS (SELECT 1 FROM automation_locations WHERE automation_id = $1)) OR EXISTS (
            SELECT 1 FROM card_lists cl JOIN lists l ON l.id = cl.list_id JOIN automation_locations al ON al.automation_id = $1
             AND ((al.location_type = 'list' AND al.location_id = l.id) OR (al.location_type = 'space' AND al.location_id = l.space_id))
            WHERE cl.card_id = $2)")
        .bind(automation)
        .bind(card_id)
        .fetch_one(&s.db)
        .await?;
    Ok(in_scope)
}

/// Runs an automation for a card on behalf of the scheduler (depth 0, like a webhook).
pub(crate) async fn run_scheduled(
    s: &AppState,
    automation: Uuid,
    creator: Option<Uuid>,
    card_id: Uuid,
    workspace_id: Uuid,
    trigger: Value,
) -> Result<Uuid, sqlx::Error> {
    run(
        s,
        RunInput {
            automation,
            creator,
            card_id: Some(card_id),
            workspace_id,
            trigger,
            depth: 0,
        },
    )
    .await
}

/// Everything a run needs to know about what triggered it.
pub(super) struct RunInput {
    pub(super) automation: Uuid,
    pub(super) creator: Option<Uuid>,
    /// Absent for webhook triggers that do not name a card.
    pub(super) card_id: Option<Uuid>,
    pub(super) workspace_id: Uuid,
    pub(super) trigger: Value,
    pub(super) depth: u8,
}

pub(super) async fn start_run(s: &AppState, input: &RunInput) -> Result<Uuid, sqlx::Error> {
    let run_id = Uuid::new_v4();
    sqlx::query("INSERT INTO automation_runs (id, automation_id, card_id, status) VALUES ($1,$2,$3,'running')")
        .bind(run_id)
        .bind(input.automation)
        .bind(input.card_id)
        .execute(&s.db)
        .await?;
    Ok(run_id)
}

pub(super) async fn finish_run(
    s: &AppState,
    run_id: Uuid,
    input: &RunInput,
) -> Result<(), sqlx::Error> {
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
        crate::services::mailer::escape_html(text)
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
                crate::security::authz::require_list_role(&s.db, creator, list, Role::Member)
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
                let status = crate::security::ssrf::deliver(crate::security::ssrf::Delivery {
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
                if !crate::services::mailer::enabled(s) {
                    return Err(bad("e-mail is not configured on this instance"));
                }
                if !s.email_limiter.check(&automation.to_string()).await {
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
                    let url = crate::services::notify::card_url(s, workspace_id, *card);
                    html.push_str(&format!(
                        "<p><a href=\"{}\">Open in work</a></p>",
                        crate::services::mailer::escape_html(&url)
                    ));
                }
                crate::services::mailer::send(s, address, subject, html);
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
