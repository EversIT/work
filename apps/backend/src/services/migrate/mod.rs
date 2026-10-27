//! One-off import from an upstream database (the NestJS/TypeORM project this was rewritten from).
//!
//! The upstream schema (integer ids, a per-resource ACL table, 79 migrations) does not map 1:1 onto
//! this one, and it differs between deployments. So the importer does not read upstream tables.
//! It reads a small, documented **export contract**: twelve views an operator creates in the source
//! database (or in a copy of it), each mapping the deployment's own tables onto fixed columns. See
//! `docs/DATA-MIGRATION.md`.
//!
//! Safety properties:
//! * Writes go through this server's own HTTP API, in process, acting as the project owner (or the
//!   original author where they are still allowed to write). Every rule of the API applies: field
//!   validation, issue keys, default issue types, views and stages. Only users, projects, memberships
//!   and the original timestamps are written directly.
//! * It is idempotent: every created object is recorded in `migration_ids` (source id -> new id), so
//!   a second run skips what exists and carries on with what failed or was added since.
//! * Nothing is dropped silently. Rows that cannot be imported, and values that had to be left out so
//!   that a card could still be created, are listed in the report.
//! * Passwords: bcrypt hashes are kept and verify as before; they are replaced by Argon2id at the next
//!   sign-in. Anything that is not a bcrypt or Argon2 hash becomes "no password" (use "forgot password").
//! * No notifications, e-mails or automations run during the import: the event worker is not started.
//! * The schema name that is spliced into SQL is checked against a strict pattern.

mod api;
mod cards;
mod contract;
mod ids;
mod normalize;
mod report;

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};

use axum::http::{Method, StatusCode};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use self::{
    api::{id_of, Api},
    cards::{import_card, post_comment},
    ids::IdMap,
    normalize::{acceptable_hash, clip, normalize_items, normalize_role, slugify},
};
pub use self::{
    contract::{check_contract, MigrateError},
    report::Report,
};
use crate::{routes::workspaces::suffixed_key, state::AppState};

pub struct Options {
    /// Schema of the export views in the source database.
    pub schema: String,
    /// Read and check everything, write nothing.
    pub dry_run: bool,
}

/// The whole export, read up front: a bad contract fails before anything is written.
struct Export {
    users: Vec<sqlx::postgres::PgRow>,
    projects: Vec<sqlx::postgres::PgRow>,
    members: Vec<sqlx::postgres::PgRow>,
    workspaces: Vec<sqlx::postgres::PgRow>,
    spaces: Vec<sqlx::postgres::PgRow>,
    lists: Vec<sqlx::postgres::PgRow>,
    stages: Vec<sqlx::postgres::PgRow>,
    card_types: Vec<sqlx::postgres::PgRow>,
    fields: Vec<sqlx::postgres::PgRow>,
    cards: Vec<sqlx::postgres::PgRow>,
    card_lists: Vec<sqlx::postgres::PgRow>,
    comments: Vec<sqlx::postgres::PgRow>,
}

impl Export {
    async fn read(source: &PgPool, schema: &str) -> Result<Self, sqlx::Error> {
        let q = |view: &str, cols: &str| format!("SELECT {cols} FROM {schema}.{view}");
        // Read the whole export up front: a bad contract fails before anything is written.
        let users = sqlx::query(&q("users", "id::text AS id, email::text AS email, first_name::text AS first_name, last_name::text AS last_name, password_hash::text AS password_hash, created_at::timestamptz AS created_at")).fetch_all(source).await?;
        let projects = sqlx::query(&q("projects", "id::text AS id, name::text AS name"))
            .fetch_all(source)
            .await?;
        let members = sqlx::query(&q(
            "project_members",
            "project_id::text AS project_id, user_id::text AS user_id, role::text AS role",
        ))
        .fetch_all(source)
        .await?;
        let workspaces = sqlx::query(&q("workspaces", "id::text AS id, project_id::text AS project_id, name::text AS name, key_prefix::text AS key_prefix")).fetch_all(source).await?;
        let spaces = sqlx::query(&q("spaces", "id::text AS id, workspace_id::text AS workspace_id, name::text AS name, COALESCE(is_private, false) AS is_private")).fetch_all(source).await?;
        let lists = sqlx::query(&q("lists", "id::text AS id, space_id::text AS space_id, name::text AS name, methodology::text AS methodology")).fetch_all(source).await?;
        let stages = sqlx::query(&format!("{} ORDER BY sort_order, id", q("stages", "id::text AS id, list_id::text AS list_id, name::text AS name, sort_order::int AS sort_order, COALESCE(is_completed, false) AS is_completed"))).fetch_all(source).await?;
        let card_types = sqlx::query(&q("card_types", "id::text AS id, workspace_id::text AS workspace_id, name::text AS name, layout::text AS layout")).fetch_all(source).await?;
        let fields = sqlx::query(&q("fields", "id::text AS id, workspace_id::text AS workspace_id, card_type_id::text AS card_type_id, name::text AS name, slug::text AS slug, type::text AS type, items::jsonb AS items, COALESCE(multiple, false) AS multiple, COALESCE(required, false) AS required")).fetch_all(source).await?;
        let cards = sqlx::query(&format!("{} ORDER BY created_at, id", q("cards", "id::text AS id, workspace_id::text AS workspace_id, card_type_id::text AS card_type_id, parent_id::text AS parent_id, COALESCE(data::jsonb, '{}'::jsonb) AS data, created_by::text AS created_by, created_at::timestamptz AS created_at"))).fetch_all(source).await?;
        let card_lists = sqlx::query(&q(
            "card_lists",
            "card_id::text AS card_id, list_id::text AS list_id, stage_id::text AS stage_id",
        ))
        .fetch_all(source)
        .await?;
        let comments = sqlx::query(&format!("{} ORDER BY created_at, id", q("comments", "id::text AS id, card_id::text AS card_id, user_id::text AS user_id, content::text AS content, created_at::timestamptz AS created_at"))).fetch_all(source).await?;
        Ok(Self {
            users,
            projects,
            members,
            workspaces,
            spaces,
            lists,
            stages,
            card_types,
            fields,
            cards,
            card_lists,
            comments,
        })
    }

    fn counts(&self) -> [(&'static str, usize); 12] {
        [
            ("users", self.users.len()),
            ("projects", self.projects.len()),
            ("project_members", self.members.len()),
            ("workspaces", self.workspaces.len()),
            ("spaces", self.spaces.len()),
            ("lists", self.lists.len()),
            ("stages", self.stages.len()),
            ("card_types", self.card_types.len()),
            ("fields", self.fields.len()),
            ("cards", self.cards.len()),
            ("card_lists", self.card_lists.len()),
            ("comments", self.comments.len()),
        ]
    }
}

/// One run of the import: the in-process API client, the id map, the report, and what earlier phases
/// learned that later ones need (who owns which project, which workspace belongs to which project).
struct Importer<'a> {
    state: &'a AppState,
    export: &'a Export,
    api: Api,
    ids: IdMap,
    report: Report,
    /// source project id -> the user the import acts as there
    project_owner: HashMap<String, Uuid>,
    /// (source project id, user) -> role
    project_roles: HashMap<(String, Uuid), &'static str>,
    /// source workspace id -> source project id
    ws_project: HashMap<String, String>,
    /// source space id -> source workspace id
    space_ws: HashMap<String, String>,
}

impl<'a> Importer<'a> {
    fn new(state: &'a AppState, export: &'a Export) -> Self {
        Self {
            state,
            export,
            api: Api::new(state),
            ids: IdMap::new(state.db.clone()),
            report: Report::default(),
            project_owner: HashMap::new(),
            project_roles: HashMap::new(),
            ws_project: HashMap::new(),
            space_ws: HashMap::new(),
        }
    }

    /// Users are written directly; a user whose address already has an account is that person.
    async fn users(&mut self) -> Result<(), MigrateError> {
        let export = self.export;
        // -- users (direct) ----------------------------------------------------------------------------
        for u in &export.users {
            let sid: String = u.get("id");
            if self.ids.get("user", &sid).await?.is_some() {
                self.report.existed("users");
                continue;
            }
            let email = u.get::<String, _>("email").trim().to_lowercase();
            if email.len() > 254 || !email.contains('@') || email.contains(char::is_whitespace) {
                self.report
                    .problem(format!("user {sid}: invalid e-mail address, not imported"));
                continue;
            }
            let local = email.split('@').next().unwrap_or("user").to_owned();
            let first = clip(
                &u.get::<Option<String>, _>("first_name").unwrap_or_default(),
                100,
            );
            let last = clip(
                &u.get::<Option<String>, _>("last_name").unwrap_or_default(),
                100,
            );
            let first = if first.is_empty() {
                clip(&local, 100)
            } else {
                first
            };
            let last = if last.is_empty() {
                "-".to_owned()
            } else {
                last
            };
            let hash = u
                .get::<Option<String>, _>("password_hash")
                .unwrap_or_default();
            let created: Option<DateTime<Utc>> = u.get("created_at");
            // An address that already has an account here is the same person: map, do not overwrite.
            let existing: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM users WHERE email = $1 AND deleted_at IS NULL")
                    .bind(&email)
                    .fetch_optional(&self.state.db)
                    .await?;
            let (id, fresh) = match existing {
                Some((id,)) => (id, false),
                None => {
                    let id = Uuid::new_v4();
                    sqlx::query("INSERT INTO users (id, email, password_hash, first_name, last_name, created_at) VALUES ($1,$2,$3,$4,$5,COALESCE($6, now()))")
                    .bind(id)
                    .bind(&email)
                    .bind(acceptable_hash(&hash))
                    .bind(&first)
                    .bind(&last)
                    .bind(created)
                    .execute(&self.state.db)
                    .await?;
                    (id, true)
                }
            };
            self.ids.put("user", &sid, id).await?;
            if fresh {
                self.report.created("users");
                if acceptable_hash(&hash).is_empty() {
                    self.report.problem(format!(
                        "user {sid}: no usable password hash, they must use \"forgot password\""
                    ));
                }
            } else {
                self.report.existed("users");
            }
        }

        Ok(())
    }

    /// Projects and memberships are written directly. Every project gets an owner.
    async fn projects(&mut self) -> Result<(), MigrateError> {
        let export = self.export;
        // -- projects and memberships (direct) ---------------------------------------------------------
        let mut by_project: HashMap<String, Vec<(Uuid, &'static str)>> = HashMap::new();
        for m in &export.members {
            let (pid, uid, role): (String, String, String) =
                (m.get("project_id"), m.get("user_id"), m.get("role"));
            let Some(user) = self.ids.get("user", &uid).await? else {
                continue;
            };
            let role = normalize_role(&role).unwrap_or_else(|| {
                self.report.problem(format!(
                    "project {pid}: unknown role '{role}' for user {uid}, imported as member"
                ));
                "member"
            });
            by_project.entry(pid).or_default().push((user, role));
        }
        for p in &export.projects {
            let sid: String = p.get("id");
            let name = clip(&p.get::<String, _>("name"), 100);
            let mut team = by_project.remove(&sid).unwrap_or_default();
            if team.is_empty() {
                self.report.problem(format!(
                "project {sid} ('{name}'): no importable members, skipped with everything in it"
            ));
                continue;
            }
            // A project always needs an owner: promote the first admin, else the first member.
            if !team.iter().any(|(_, r)| *r == "owner") {
                let i = team.iter().position(|(_, r)| *r == "admin").unwrap_or(0);
                team[i].1 = "owner";
                self.report.problem(format!(
                    "project {sid} ('{name}'): had no owner, promoted the first admin/member"
                ));
            }
            let owner = team
                .iter()
                .find(|(_, r)| *r == "owner")
                .map(|(u, _)| *u)
                .expect("an owner exists");
            self.project_owner.insert(sid.clone(), owner);
            let pid = match self.ids.get("project", &sid).await? {
                Some(id) => {
                    self.report.existed("projects");
                    id
                }
                None => {
                    let id = Uuid::new_v4();
                    sqlx::query("INSERT INTO projects (id, name, created_by) VALUES ($1,$2,$3)")
                        .bind(id)
                        .bind(if name.is_empty() {
                            "Imported project"
                        } else {
                            name.as_str()
                        })
                        .bind(owner)
                        .execute(&self.state.db)
                        .await?;
                    self.ids.put("project", &sid, id).await?;
                    self.report.created("projects");
                    id
                }
            };
            for (user, role) in team {
                sqlx::query("INSERT INTO project_members (project_id, user_id, role) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
                .bind(pid)
                .bind(user)
                .bind(role)
                .execute(&self.state.db)
                .await?;
                self.project_roles.insert((sid.clone(), user), role);
            }
        }

        Ok(())
    }

    /// Workspaces go through the API, so issue keys and the default issue types come with them.
    async fn workspaces(&mut self) -> Result<(), MigrateError> {
        let export = self.export;
        // -- workspaces, spaces, lists, stages (API) ---------------------------------------------------
        for w in &export.workspaces {
            let (sid, spid): (String, String) = (w.get("id"), w.get("project_id"));
            let Some(owner) = self.project_owner.get(&spid).copied() else {
                self.report.problem(format!(
                    "workspace {sid}: its project {spid} was not imported"
                ));
                continue;
            };
            self.ws_project.insert(sid.clone(), spid.clone());
            if self.ids.get("workspace", &sid).await?.is_some() {
                self.report.existed("workspaces");
                continue;
            }
            let project = self
                .ids
                .get("project", &spid)
                .await?
                .expect("project mapped");
            let mut body = json!({"name": clip(&w.get::<String, _>("name"), 100)});
            let key = w
                .get::<Option<String>, _>("key_prefix")
                .unwrap_or_default()
                .to_uppercase();
            if !key.is_empty() {
                body["key_prefix"] = json!(key);
            }
            let made = match self
                .api
                .call(
                    owner,
                    Method::POST,
                    &format!("/projects/{project}/workspaces"),
                    Some(body.clone()),
                )
                .await
            {
                Err((StatusCode::BAD_REQUEST, why)) if body.get("key_prefix").is_some() => {
                    self.report.problem(format!(
                    "workspace {sid}: key '{key}' refused ({why}), a key was derived from the name"
                ));
                    body.as_object_mut().expect("object").remove("key_prefix");
                    self.api
                        .call(
                            owner,
                            Method::POST,
                            &format!("/projects/{project}/workspaces"),
                            Some(body),
                        )
                        .await
                }
                // The key is taken instance-wide: keep it recognisable with a numeric suffix.
                Err((StatusCode::CONFLICT, _)) if body.get("key_prefix").is_some() => {
                    let mut last = Err((StatusCode::CONFLICT, String::new()));
                    for n in 2..=99 {
                        body["key_prefix"] = json!(suffixed_key(&key, n));
                        last = self
                            .api
                            .call(
                                owner,
                                Method::POST,
                                &format!("/projects/{project}/workspaces"),
                                Some(body.clone()),
                            )
                            .await;
                        if !matches!(last, Err((StatusCode::CONFLICT, _))) {
                            break;
                        }
                    }
                    last
                }
                other => other,
            };
            match made.ok().as_ref().and_then(id_of) {
                Some(id) => {
                    self.ids.put("workspace", &sid, id).await?;
                    self.report.created("workspaces");
                }
                None => self
                    .report
                    .problem(format!("workspace {sid}: could not be created")),
            }
        }

        Ok(())
    }

    /// Spaces, the folders projects are grouped in.
    async fn spaces(&mut self) -> Result<(), MigrateError> {
        let export = self.export;
        for s in &export.spaces {
            let (sid, swid): (String, String) = (s.get("id"), s.get("workspace_id"));
            self.space_ws.insert(sid.clone(), swid.clone());
            if self.ids.get("space", &sid).await?.is_some() {
                self.report.existed("spaces");
                continue;
            }
            let (Some(ws), Some(owner)) = (
                self.ids.get("workspace", &swid).await?,
                self.ws_project
                    .get(&swid)
                    .and_then(|p| self.project_owner.get(p))
                    .copied(),
            ) else {
                self.report.problem(format!(
                    "space {sid}: its workspace {swid} was not imported"
                ));
                continue;
            };
            let private: bool = s.get("is_private");
            let body = json!({"name": clip(&s.get::<String, _>("name"), 100), "access_type": if private { "private" } else { "public" }});
            match self
                .api
                .call(
                    owner,
                    Method::POST,
                    &format!("/workspaces/{ws}/spaces"),
                    Some(body),
                )
                .await
                .ok()
                .as_ref()
                .and_then(id_of)
            {
                Some(id) => {
                    self.ids.put("space", &sid, id).await?;
                    self.report.created("spaces");
                }
                None => self
                    .report
                    .problem(format!("space {sid}: could not be created")),
            }
        }

        // Card types first (lists want a default type), looked up by name among the seeded ones.
        Ok(())
    }

    /// Card types: matched by name among the seeded ones (lists want a default type), else created.
    async fn card_types(&mut self) -> Result<(), MigrateError> {
        let export = self.export;
        for ct in &export.card_types {
            let (sid, swid): (String, String) = (ct.get("id"), ct.get("workspace_id"));
            let (Some(ws), Some(owner)) = (
                self.ids.get("workspace", &swid).await?,
                self.ws_project
                    .get(&swid)
                    .and_then(|p| self.project_owner.get(p))
                    .copied(),
            ) else {
                self.report.problem(format!(
                    "card type {sid}: its workspace {swid} was not imported"
                ));
                continue;
            };
            if self.ids.get("card_type", &sid).await?.is_some() {
                self.report.existed("card_types");
                continue;
            }
            let name = clip(&ct.get::<String, _>("name"), 100);
            let existing = self
                .api
                .call(
                    owner,
                    Method::GET,
                    &format!("/workspaces/{ws}/card-types"),
                    None,
                )
                .await
                .unwrap_or(Value::Null);
            let same_name = existing
                .as_array()
                .and_then(|a| {
                    a.iter().find(|t| {
                        t["name"]
                            .as_str()
                            .is_some_and(|n| n.eq_ignore_ascii_case(&name))
                    })
                })
                .and_then(id_of);
            let id = match same_name {
                Some(id) => {
                    self.report.existed("card_types"); // e.g. Task, Epic, Bug come with every workspace
                    id
                }
                None => {
                    let layout = ct.get::<Option<String>, _>("layout").unwrap_or_default();
                    let layout = if matches!(layout.as_str(), "person" | "organization" | "deal") {
                        layout
                    } else {
                        "default".into()
                    };
                    match self
                        .api
                        .call(
                            owner,
                            Method::POST,
                            &format!("/workspaces/{ws}/card-types"),
                            Some(json!({"name": name, "layout": layout, "has_children": true})),
                        )
                        .await
                    {
                        Ok(v) => {
                            self.report.created("card_types");
                            match id_of(&v) {
                                Some(id) => id,
                                None => continue,
                            }
                        }
                        Err((_, why)) => {
                            self.report
                                .problem(format!("card type {sid} ('{name}'): {why}"));
                            continue;
                        }
                    }
                }
            };
            self.ids.put("card_type", &sid, id).await?;
        }

        Ok(())
    }

    /// Lists, with the source's stages replacing the three default ones.
    async fn lists(&mut self) -> Result<(), MigrateError> {
        let export = self.export;
        for l in &export.lists {
            let (sid, sspace): (String, String) = (l.get("id"), l.get("space_id"));
            if self.ids.get("list", &sid).await?.is_some() {
                self.report.existed("lists");
                continue;
            }
            let Some(space) = self.ids.get("space", &sspace).await? else {
                self.report
                    .problem(format!("list {sid}: its space {sspace} was not imported"));
                continue;
            };
            let owner = self
                .space_ws
                .get(&sspace)
                .and_then(|w| self.ws_project.get(w))
                .and_then(|p| self.project_owner.get(p))
                .copied();
            let Some(owner) = owner else { continue };
            let method = l
                .get::<Option<String>, _>("methodology")
                .unwrap_or_default()
                .to_lowercase();
            let body = json!({"name": clip(&l.get::<String, _>("name"), 100), "methodology": if method == "scrum" { "scrum" } else { "kanban" }});
            let Some(list) = self
                .api
                .call(
                    owner,
                    Method::POST,
                    &format!("/spaces/{space}/lists"),
                    Some(body),
                )
                .await
                .ok()
                .as_ref()
                .and_then(id_of)
            else {
                self.report
                    .problem(format!("list {sid}: could not be created"));
                continue;
            };
            self.ids.put("list", &sid, list).await?;
            self.report.created("lists");
            // The list starts with To Do / In Progress / Done; replace them by the source's stages.
            let own: Vec<_> = export
                .stages
                .iter()
                .filter(|s| s.get::<String, _>("list_id") == sid)
                .collect();
            if own.is_empty() {
                continue;
            }
            let defaults: Vec<Uuid> = self
                .api
                .call(owner, Method::GET, &format!("/lists/{list}"), None)
                .await
                .ok()
                .and_then(|v| {
                    v["stages"]
                        .as_array()
                        .map(|a| a.iter().filter_map(id_of).collect())
                })
                .unwrap_or_default();
            for s in own {
                let ssid: String = s.get("id");
                let body = json!({"name": clip(&s.get::<String, _>("name"), 100), "is_completed": s.get::<bool, _>("is_completed")});
                match self
                    .api
                    .call(
                        owner,
                        Method::POST,
                        &format!("/lists/{list}/stages"),
                        Some(body),
                    )
                    .await
                    .ok()
                    .as_ref()
                    .and_then(id_of)
                {
                    Some(id) => {
                        self.ids.put("stage", &ssid, id).await?;
                        self.report.created("stages");
                    }
                    None => self
                        .report
                        .problem(format!("stage {ssid}: could not be created")),
                }
            }
            for d in defaults {
                let _ = self
                    .api
                    .call(owner, Method::DELETE, &format!("/stages/{d}"), None)
                    .await;
            }
        }

        Ok(())
    }

    /// Fields: the seeded workspace-wide ones exist already, the rest is created.
    async fn fields(&mut self) -> Result<(), MigrateError> {
        let export = self.export;
        // -- fields -------------------------------------------------------------------------------------
        for f in &export.fields {
            let (sid, swid): (String, String) = (f.get("id"), f.get("workspace_id"));
            if self.ids.get("field", &sid).await?.is_some() {
                self.report.existed("fields");
                continue;
            }
            let (Some(ws), Some(owner)) = (
                self.ids.get("workspace", &swid).await?,
                self.ws_project
                    .get(&swid)
                    .and_then(|p| self.project_owner.get(p))
                    .copied(),
            ) else {
                self.report.problem(format!(
                    "field {sid}: its workspace {swid} was not imported"
                ));
                continue;
            };
            let name = clip(&f.get::<String, _>("name"), 100);
            let slug = f
                .get::<Option<String>, _>("slug")
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| slugify(&name));
            let ty: String = f.get("type");
            let card_type = match f.get::<Option<String>, _>("card_type_id") {
                Some(t) => self.ids.get("card_type", &t).await?,
                None => None,
            };
            // Seeded workspace-wide fields (title, description, assignee, priority, ...) already exist.
            let current = self
                .api
                .call(
                    owner,
                    Method::GET,
                    &format!("/workspaces/{ws}/card-types"),
                    None,
                )
                .await
                .unwrap_or(Value::Null);
            let known = current.as_array().is_some_and(|types| {
                types.iter().any(|t| {
                    t["fields"].as_array().is_some_and(|fs| {
                        fs.iter().any(|x| {
                            x["slug"] == slug.as_str()
                                && (x["card_type_id"].is_null()
                                    || card_type
                                        .is_some_and(|c| x["card_type_id"] == c.to_string()))
                        })
                    })
                })
            });
            if known {
                self.report.existed("fields");
                continue;
            }
            let mut body = json!({"name": name, "slug": slug, "type": ty, "multiple": f.get::<bool, _>("multiple"), "required": f.get::<bool, _>("required")});
            if let Some(items) = f
                .get::<Option<Value>, _>("items")
                .as_ref()
                .and_then(normalize_items)
            {
                body["items"] = items;
            }
            if let Some(c) = card_type {
                body["card_type_id"] = json!(c);
            }
            match self
                .api
                .call(
                    owner,
                    Method::POST,
                    &format!("/workspaces/{ws}/fields"),
                    Some(body),
                )
                .await
            {
                Ok(v) => {
                    if let Some(id) = id_of(&v) {
                        self.ids.put("field", &sid, id).await?;
                    }
                    self.report.created("fields");
                }
                Err((_, why)) => self.report.problem(format!(
                    "field {sid} ('{slug}'): {why}; values of it are left out of the cards"
                )),
            }
        }

        Ok(())
    }

    /// Cards, parents first, then their list placements; the original timestamps are restored.
    async fn cards(&mut self) -> Result<(), MigrateError> {
        let export = self.export;
        // -- cards ----------------------------------------------------------------------------------------
        let mut placements: HashMap<String, Vec<(String, Option<String>)>> = HashMap::new();
        for cl in &export.card_lists {
            placements
                .entry(cl.get("card_id"))
                .or_default()
                .push((cl.get("list_id"), cl.get("stage_id")));
        }
        // Which slugs exist per (workspace, type), refreshed once after the fields were created.
        let mut slugs: HashMap<(Uuid, Uuid), HashSet<String>> = HashMap::new();
        let mut dropped_note: HashSet<(String, String)> = HashSet::new();
        let mut card_meta: Vec<(Uuid, DateTime<Utc>)> = Vec::new();
        let mut card_project: HashMap<String, String> = HashMap::new(); // source card -> source project
        let mut ws_types: HashMap<Uuid, Value> = HashMap::new();

        let mut pending: Vec<&sqlx::postgres::PgRow> = export.cards.iter().collect();
        while !pending.is_empty() {
            let before = pending.len();
            let mut deferred = Vec::new();
            for c in pending {
                let sid: String = c.get("id");
                if self.ids.get("card", &sid).await?.is_some() {
                    self.report.existed("cards");
                    continue;
                }
                let parent_sid: Option<String> = c.get("parent_id");
                let parent = match &parent_sid {
                    Some(p) => match self.ids.get("card", p).await? {
                        Some(id) => Some(id),
                        None => {
                            deferred.push(c); // the parent comes later in this pass or the next
                            continue;
                        }
                    },
                    None => None,
                };
                if let Some(done) = import_card(
                    &mut self.api,
                    &mut self.ids,
                    &mut self.report,
                    c,
                    parent,
                    &placements,
                    &self.ws_project,
                    &self.project_owner,
                    &self.project_roles,
                    &mut slugs,
                    &mut dropped_note,
                    &mut card_project,
                    &mut ws_types,
                )
                .await?
                {
                    card_meta.push(done);
                }
            }
            if deferred.len() == before {
                // Parents that never appear (cycles or missing rows): import the children without them.
                for c in deferred {
                    let sid: String = c.get("id");
                    self.report.problem(format!(
                        "card {sid}: parent {} was not imported, the card was imported without it",
                        c.get::<Option<String>, _>("parent_id").unwrap_or_default()
                    ));
                    if let Some(done) = import_card(
                        &mut self.api,
                        &mut self.ids,
                        &mut self.report,
                        c,
                        None,
                        &placements,
                        &self.ws_project,
                        &self.project_owner,
                        &self.project_roles,
                        &mut slugs,
                        &mut dropped_note,
                        &mut card_project,
                        &mut ws_types,
                    )
                    .await?
                    {
                        card_meta.push(done);
                    }
                }
                break;
            }
            pending = deferred;
        }
        // Keep the original timestamps: the API stamps "now".
        for (id, created) in card_meta {
            sqlx::query("UPDATE cards SET created_at = $2, updated_at = $2 WHERE id = $1")
                .bind(id)
                .bind(created)
                .execute(&self.state.db)
                .await?;
        }

        Ok(())
    }

    /// Comments, by the original author where that person may still write.
    async fn comments(&mut self) -> Result<(), MigrateError> {
        let export = self.export;
        // -- comments ------------------------------------------------------------------------------------
        for cm in &export.comments {
            let (sid, scard): (String, String) = (cm.get("id"), cm.get("card_id"));
            if self.ids.get("comment", &sid).await?.is_some() {
                self.report.existed("comments");
                continue;
            }
            let Some(card) = self.ids.get("card", &scard).await? else {
                self.report
                    .problem(format!("comment {sid}: its card {scard} was not imported"));
                continue;
            };
            // Project and owner come from the database, so comments of cards from an earlier run work too.
            let row: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
            "SELECT w.project_id, (SELECT user_id FROM project_members m WHERE m.project_id = w.project_id AND m.role = 'owner' ORDER BY created_at LIMIT 1)
               FROM cards c JOIN workspaces w ON w.id = c.workspace_id WHERE c.id = $1",
        )
        .bind(card)
        .fetch_optional(&self.state.db)
        .await?;
            let Some((project, Some(owner))) = row else {
                continue;
            };
            let mut author = owner;
            if let Some(u) = cm.get::<Option<String>, _>("user_id") {
                if let Some(user) = self.ids.get("user", &u).await? {
                    let role: Option<(String,)> = sqlx::query_as(
                        "SELECT role FROM project_members WHERE project_id = $1 AND user_id = $2",
                    )
                    .bind(project)
                    .bind(user)
                    .fetch_optional(&self.state.db)
                    .await?;
                    if role.is_some_and(|r| matches!(r.0.as_str(), "owner" | "admin" | "member")) {
                        author = user;
                    }
                }
            }
            post_comment(
                &mut self.api,
                &mut self.ids,
                &mut self.report,
                self.state,
                cm,
                card,
                author,
            )
            .await?;
        }

        Ok(())
    }

    async fn finish(self) -> Report {
        self.api.finish().await;
        self.report
    }
}

pub async fn run(
    state: &AppState,
    source: &PgPool,
    opts: &Options,
) -> Result<Report, MigrateError> {
    check_contract(source, &opts.schema).await?;
    let export = Export::read(source, &opts.schema).await?;
    if opts.dry_run {
        let mut report = Report::default();
        for (kind, rows) in export.counts() {
            report.counts.insert(kind, (rows, 0));
        }
        return Ok(report);
    }
    let mut import = Importer::new(state, &export);
    import.users().await?;
    import.projects().await?;
    import.workspaces().await?;
    import.spaces().await?;
    import.card_types().await?;
    import.lists().await?;
    import.fields().await?;
    import.cards().await?;
    import.comments().await?;
    Ok(import.finish().await)
}
