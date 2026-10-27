//! Cards and comments: the two kinds of rows that need the most care when they are imported.

use std::collections::{HashMap, HashSet};

use axum::http::{Method, StatusCode};
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use sqlx::Row;
use uuid::Uuid;

use super::{
    api::{id_of, Api},
    contract::MigrateError,
    ids::IdMap,
    report::Report,
};
use crate::state::AppState;

pub(super) async fn post_comment(
    api: &mut Api,
    ids: &mut IdMap,
    report: &mut Report,
    state: &AppState,
    cm: &sqlx::postgres::PgRow,
    card: Uuid,
    author: Uuid,
) -> Result<(), MigrateError> {
    let sid: String = cm.get("id");
    let text = cm.get::<Option<String>, _>("content").unwrap_or_default();
    if text.trim().is_empty() {
        report.problem(format!("comment {sid}: empty, skipped"));
        return Ok(());
    }
    let body = json!({"type": "comment", "content": {"content": text.chars().take(20_000).collect::<String>()}});
    match api
        .call(
            author,
            Method::POST,
            &format!("/cards/{card}/activities"),
            Some(body),
        )
        .await
    {
        Ok(v) => {
            if let Some(id) = id_of(&v) {
                let created: Option<DateTime<Utc>> = cm.get("created_at");
                sqlx::query("UPDATE card_activities SET created_at = COALESCE($2, created_at) WHERE id = $1")
                    .bind(id)
                    .bind(created)
                    .execute(&state.db)
                    .await?;
                ids.put("comment", &sid, id).await?;
            }
            report.created("comments");
        }
        Err((_, why)) => report.problem(format!("comment {sid}: {why}")),
    }
    Ok(())
}

/// Creates one card (and its extra list placements). Returns what is needed to restore its timestamp.
#[allow(clippy::too_many_arguments)]
pub(super) async fn import_card(
    api: &mut Api,
    ids: &mut IdMap,
    report: &mut Report,
    c: &sqlx::postgres::PgRow,
    parent: Option<Uuid>,
    placements: &HashMap<String, Vec<(String, Option<String>)>>,
    ws_project: &HashMap<String, String>,
    project_owner: &HashMap<String, Uuid>,
    project_roles: &HashMap<(String, Uuid), &'static str>,
    slugs: &mut HashMap<(Uuid, Uuid), HashSet<String>>,
    dropped_note: &mut HashSet<(String, String)>,
    card_project: &mut HashMap<String, String>,
    ws_types: &mut HashMap<Uuid, Value>,
) -> Result<Option<(Uuid, DateTime<Utc>)>, MigrateError> {
    let sid: String = c.get("id");
    let swid: String = c.get("workspace_id");
    let Some(spid) = ws_project.get(&swid).cloned() else {
        report.problem(format!("card {sid}: its workspace {swid} was not imported"));
        return Ok(None);
    };
    let (Some(ws), Some(owner)) = (
        ids.get("workspace", &swid).await?,
        project_owner.get(&spid).copied(),
    ) else {
        report.problem(format!("card {sid}: its workspace {swid} was not imported"));
        return Ok(None);
    };
    let Some(places) = placements.get(&sid).filter(|p| !p.is_empty()) else {
        report.problem(format!("card {sid}: it sits in no list, skipped"));
        return Ok(None);
    };
    let Some(first_list) = ids.get("list", &places[0].0).await? else {
        report.problem(format!(
            "card {sid}: its list {} was not imported, skipped",
            places[0].0
        ));
        return Ok(None);
    };
    let author = match c.get::<Option<String>, _>("created_by") {
        Some(u) => match ids.get("user", &u).await? {
            Some(user)
                if matches!(
                    project_roles.get(&(spid.clone(), user)).copied(),
                    Some("owner" | "admin" | "member")
                ) =>
            {
                user
            }
            _ => owner,
        },
        None => owner,
    };
    let card_type = match c.get::<Option<String>, _>("card_type_id") {
        Some(t) => ids.get("card_type", &t).await?,
        None => None,
    };

    // The slugs this type accepts, so unknown keys can be reported instead of failing the card.
    let types = match ws_types.get(&ws).cloned() {
        Some(t) => t,
        None => {
            let fetched = api
                .call(
                    owner,
                    Method::GET,
                    &format!("/workspaces/{ws}/card-types"),
                    None,
                )
                .await
                .unwrap_or(Value::Null);
            ws_types.insert(ws, fetched.clone());
            fetched
        }
    };
    let type_id = card_type.or_else(|| types.as_array().and_then(|t| t.first()).and_then(id_of));
    if let Some(t) = type_id {
        slugs.entry((ws, t)).or_insert_with(|| {
            types
                .as_array()
                .and_then(|a| a.iter().find(|x| x["id"] == t.to_string()))
                .and_then(|x| x["fields"].as_array())
                .map(|fs| {
                    fs.iter()
                        .filter_map(|f| f["slug"].as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        });
    }
    let known = type_id
        .and_then(|t| slugs.get(&(ws, t)))
        .cloned()
        .unwrap_or_default();

    let mut data: Map<String, Value> = c
        .get::<Value, _>("data")
        .as_object()
        .cloned()
        .unwrap_or_default();
    data.retain(|k, v| {
        let keep = known.contains(k) && !v.is_null();
        if !keep && !v.is_null() && dropped_note.insert((swid.clone(), k.clone())) {
            report.problem(format!("workspace {swid}: card values under '{k}' are left out (no such field; add it to the fields view)"));
        }
        keep
    });
    // People fields hold source user ids; translate them to the imported users, drop unknown ones.
    let user_slugs: Vec<String> = type_id
        .and_then(|t| {
            types
                .as_array()
                .and_then(|a| a.iter().find(|x| x["id"] == t.to_string()))
        })
        .and_then(|x| x["fields"].as_array())
        .map(|fs| {
            fs.iter()
                .filter(|f| f["type"] == "user")
                .filter_map(|f| f["slug"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    for slug in user_slugs {
        let Some(v) = data.get(&slug).cloned() else {
            continue;
        };
        let raw: Vec<String> = match &v {
            Value::Array(a) => a
                .iter()
                .filter_map(|x| {
                    x.as_str()
                        .map(str::to_owned)
                        .or_else(|| x.as_i64().map(|n| n.to_string()))
                })
                .collect(),
            Value::String(s) => vec![s.clone()],
            Value::Number(n) => vec![n.to_string()],
            _ => vec![],
        };
        let mut mapped = Vec::new();
        for r in raw {
            match ids.get("user", &r).await? {
                Some(u) => mapped.push(json!(u)),
                None => report.problem(format!(
                    "card {sid}: person {r} in '{slug}' is not an imported user, left out"
                )),
            }
        }
        if mapped.is_empty() {
            data.remove(&slug);
        } else {
            data.insert(slug, Value::Array(mapped));
        }
    }
    let stage = match &places[0].1 {
        Some(s) => ids.get("stage", s).await?,
        None => None,
    };
    let mut attempt = 0;
    let created = loop {
        let mut body = json!({"data": Value::Object(data.clone())});
        if let Some(t) = card_type {
            body["card_type_id"] = json!(t);
        }
        if let Some(s) = stage {
            body["stage_id"] = json!(s);
        }
        if let Some(p) = parent {
            body["parent_id"] = json!(p);
        }
        match api
            .call(
                author,
                Method::POST,
                &format!("/lists/{first_list}/cards"),
                Some(body),
            )
            .await
        {
            Ok(v) => break Some(v),
            Err((StatusCode::BAD_REQUEST, why)) if attempt < 8 => {
                // "{slug}: expected ..." names the value that was refused; leave just that one out.
                let slug = why.split(':').next().unwrap_or_default().trim().to_owned();
                if data.remove(&slug).is_some() {
                    report.problem(format!("card {sid}: value of '{slug}' dropped ({why})"));
                    attempt += 1;
                    continue;
                }
                report.problem(format!("card {sid}: {why}"));
                break None;
            }
            Err((_, why)) => {
                report.problem(format!("card {sid}: {why}"));
                break None;
            }
        }
    };
    let Some(v) = created else { return Ok(None) };
    let Some(id) = id_of(&v) else { return Ok(None) };
    ids.put("card", &sid, id).await?;
    card_project.insert(sid.clone(), spid);
    report.created("cards");
    // Further lists (a card can live in several).
    for (list, stage) in places.iter().skip(1) {
        let Some(list_id) = ids.get("list", list).await? else {
            continue;
        };
        let stage_id = match stage {
            Some(s) => ids.get("stage", s).await?,
            None => None,
        };
        // A placement needs a stage: the source's, or else the first stage of that list.
        let stage_id = match stage_id {
            Some(s) => Some(s),
            None => api
                .call(owner, Method::GET, &format!("/lists/{list_id}"), None)
                .await
                .ok()
                .and_then(|v| {
                    v["stages"]
                        .as_array()
                        .and_then(|a| a.first())
                        .and_then(id_of)
                }),
        };
        let Some(stage_id) = stage_id else {
            report.problem(format!("card {sid}: list {list} has no stage to put it in"));
            continue;
        };
        let body = json!({"stage_id": stage_id});
        if let Err((_, why)) = api
            .call(
                owner,
                Method::PUT,
                &format!("/cards/{id}/lists/{list_id}"),
                Some(body),
            )
            .await
        {
            report.problem(format!(
                "card {sid}: could not be added to list {list}: {why}"
            ));
        }
    }
    let when: Option<DateTime<Utc>> = c.get("created_at");
    Ok(when.map(|w| (id, w)))
}
