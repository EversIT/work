//! Turns domain events into notifications (in-app, realtime, email, Slack), honouring each
//! user's channel preferences. Recipients are always derived from data the server owns
//! (watchers, assignees, validated mentions), never from a client-supplied user id.

use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::{events::Event, mailer, state::AppState};

pub struct NewNotification {
    pub recipient: Uuid,
    pub workspace_id: Uuid,
    pub kind: &'static str,
    pub message: String,
    pub card_id: Option<Uuid>,
    pub color: Option<&'static str>,
}

async fn channel_enabled(
    s: &AppState,
    user: Uuid,
    channel: &str,
    default: bool,
) -> Result<bool, sqlx::Error> {
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT enabled FROM notification_preferences WHERE user_id = $1 AND channel = $2",
    )
    .bind(user)
    .bind(channel)
    .fetch_optional(&s.db)
    .await?;
    Ok(row.map(|r| r.0).unwrap_or(default))
}

pub fn card_url(s: &AppState, workspace_id: Uuid, card_id: Uuid) -> String {
    format!(
        "{}/workspaces/{}/cards/{}",
        s.config.frontend_url, workspace_id, card_id
    )
}

pub async fn create(s: &AppState, n: NewNotification) -> Result<(), sqlx::Error> {
    if channel_enabled(s, n.recipient, "in_app", true).await? {
        let id = Uuid::new_v4();
        let row = sqlx::query("INSERT INTO notifications (id, recipient_id, workspace_id, type, message, related_resource_type, related_resource_id, color)
                               VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING created_at")
            .bind(id).bind(n.recipient).bind(n.workspace_id).bind(n.kind).bind(&n.message)
            .bind(n.card_id.map(|_| "card")).bind(n.card_id).bind(n.color)
            .fetch_one(&s.db).await?;
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        s.hub.publish(
            n.recipient,
            json!({
                "type": "notification", "id": id, "kind": n.kind, "message": n.message,
                "workspace_id": n.workspace_id, "card_id": n.card_id, "created_at": created_at,
            })
            .to_string(),
        );
    }
    if mailer::enabled(s)
        && matches!(n.kind, "mention" | "assigned")
        && channel_enabled(s, n.recipient, "email", false).await?
    {
        if let Some(card) = n.card_id {
            let to: Option<(String, String)> = sqlx::query_as(
                "SELECT email::text, first_name FROM users WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(n.recipient)
            .fetch_optional(&s.db)
            .await?;
            if let Some((email, first)) = to {
                let url = card_url(s, n.workspace_id, card);
                let html = format!(
                    "<p>Hello {},</p><p>{}</p><p><a href=\"{}\">Open in work</a></p>",
                    mailer::escape_html(&first),
                    mailer::escape_html(&n.message),
                    mailer::escape_html(&url)
                );
                mailer::send(s, email, "work notification".into(), html);
            }
        }
    }
    if matches!(n.kind, "mention" | "assigned" | "comment" | "stage_updated") {
        if let Some(card) = n.card_id {
            crate::integrations::notify_slack(
                s,
                n.recipient,
                &n.message,
                &card_url(s, n.workspace_id, card),
            )
            .await;
        }
    }
    Ok(())
}

async fn card_title(s: &AppState, card_id: Uuid) -> Result<String, sqlx::Error> {
    let t: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT c.data ->> f.slug FROM cards c JOIN fields f ON (f.card_type_id = c.card_type_id OR f.card_type_id IS NULL) AND f.is_title AND f.deleted_at IS NULL WHERE c.id = $1 LIMIT 1")
        .bind(card_id).fetch_optional(&s.db).await?;
    Ok(t.and_then(|x| x.0)
        .filter(|x| !x.is_empty())
        .map(|x| x.chars().take(120).collect())
        .unwrap_or_else(|| "Untitled".into()))
}

async fn actor_name(s: &AppState, actor: Option<Uuid>) -> Result<String, sqlx::Error> {
    let Some(a) = actor else {
        return Ok("Automation".into());
    };
    let n: Option<(String, String)> =
        sqlx::query_as("SELECT first_name, last_name FROM users WHERE id = $1")
            .bind(a)
            .fetch_optional(&s.db)
            .await?;
    Ok(n.map(|(f, l)| format!("{f} {l}"))
        .unwrap_or_else(|| "Someone".into()))
}

fn ids(v: &Value) -> Vec<Uuid> {
    let list: Vec<&Value> = match v {
        Value::Array(a) => a.iter().collect(),
        Value::Null => vec![],
        other => vec![other],
    };
    list.into_iter()
        .filter_map(|x| x.as_str().and_then(|x| Uuid::parse_str(x).ok()))
        .collect()
}

async fn watchers_of(s: &AppState, card_id: Uuid) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT w.user_id FROM watchers w WHERE (w.resource_type = 'card' AND w.resource_id = $1)
            OR (w.resource_type = 'list' AND w.resource_id IN (SELECT list_id FROM card_lists WHERE card_id = $1))")
        .bind(card_id).fetch_all(&s.db).await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

/// Only people who can still see the card get told about it.
async fn can_see(s: &AppState, user: Uuid, card_id: Uuid) -> bool {
    crate::authz::require_card_role(&s.db, user, card_id, crate::authz::Role::Viewer)
        .await
        .is_ok()
}

pub async fn handle_event(s: &AppState, event: &Event) -> Result<(), sqlx::Error> {
    match event {
        Event::FieldUpdated {
            card_id,
            workspace_id,
            slug,
            old,
            new,
            actor,
            ..
        } => {
            let is_assignee: Option<(bool,)> = sqlx::query_as(
                "SELECT f.is_assignee FROM fields f JOIN cards c ON c.id = $1 AND (f.card_type_id = c.card_type_id OR f.card_type_id IS NULL)
                 WHERE f.workspace_id = $2 AND f.slug = $3 AND f.deleted_at IS NULL LIMIT 1")
                .bind(card_id).bind(workspace_id).bind(slug).fetch_optional(&s.db).await?;
            if is_assignee.is_some_and(|x| x.0) {
                let (before, after) = (ids(old), ids(new));
                let title = card_title(s, *card_id).await?;
                let who = actor_name(s, *actor).await?;
                for u in after
                    .iter()
                    .filter(|u| !before.contains(u) && Some(**u) != *actor)
                {
                    if can_see(s, *u, *card_id).await {
                        create(
                            s,
                            NewNotification {
                                recipient: *u,
                                workspace_id: *workspace_id,
                                kind: "assigned",
                                message: format!("{who} assigned you to \"{title}\""),
                                card_id: Some(*card_id),
                                color: Some("blue"),
                            },
                        )
                        .await?;
                    }
                }
                for u in before
                    .iter()
                    .filter(|u| !after.contains(u) && Some(**u) != *actor)
                {
                    create(
                        s,
                        NewNotification {
                            recipient: *u,
                            workspace_id: *workspace_id,
                            kind: "unassigned",
                            message: format!("{who} removed you from \"{title}\""),
                            card_id: Some(*card_id),
                            color: Some("grey"),
                        },
                    )
                    .await?;
                }
            }
        }
        Event::StageUpdated {
            card_id,
            workspace_id,
            to,
            actor,
            ..
        } => {
            let stage: Option<(String,)> =
                sqlx::query_as("SELECT name FROM list_stages WHERE id = $1")
                    .bind(to)
                    .fetch_optional(&s.db)
                    .await?;
            let title = card_title(s, *card_id).await?;
            let who = actor_name(s, *actor).await?;
            let stage = stage.map(|x| x.0).unwrap_or_default();
            for u in watchers_of(s, *card_id)
                .await?
                .into_iter()
                .filter(|u| Some(*u) != *actor)
            {
                if can_see(s, u, *card_id).await {
                    create(
                        s,
                        NewNotification {
                            recipient: u,
                            workspace_id: *workspace_id,
                            kind: "stage_updated",
                            message: format!("{who} moved \"{title}\" to {stage}"),
                            card_id: Some(*card_id),
                            color: Some("purple"),
                        },
                    )
                    .await?;
                }
            }
        }
        Event::CommentCreated {
            card_id,
            workspace_id,
            actor,
            mentions,
            ..
        } => {
            let title = card_title(s, *card_id).await?;
            let who = actor_name(s, *actor).await?;
            let mut told = vec![];
            for u in mentions.iter().filter(|u| Some(**u) != *actor) {
                if can_see(s, *u, *card_id).await {
                    create(
                        s,
                        NewNotification {
                            recipient: *u,
                            workspace_id: *workspace_id,
                            kind: "mention",
                            message: format!("{who} mentioned you in \"{title}\""),
                            card_id: Some(*card_id),
                            color: Some("orange"),
                        },
                    )
                    .await?;
                    told.push(*u);
                }
            }
            for u in watchers_of(s, *card_id)
                .await?
                .into_iter()
                .filter(|u| Some(*u) != *actor && !told.contains(u))
            {
                if can_see(s, u, *card_id).await {
                    create(
                        s,
                        NewNotification {
                            recipient: u,
                            workspace_id: *workspace_id,
                            kind: "comment",
                            message: format!("{who} commented on \"{title}\""),
                            card_id: Some(*card_id),
                            color: Some("teal"),
                        },
                    )
                    .await?;
                }
            }
        }
        Event::CardCreated { .. } => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_accepts_scalar_array_and_ignores_garbage() {
        let a = Uuid::new_v4();
        assert_eq!(ids(&json!(a.to_string())), vec![a]);
        assert_eq!(ids(&json!([a.to_string(), "nope", 5])), vec![a]);
        assert!(ids(&Value::Null).is_empty());
    }
}
