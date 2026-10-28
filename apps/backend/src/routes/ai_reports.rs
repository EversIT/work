//! Reports on a board, a sprint, a backlog and a person's day: what moved, what is stuck, what a sprint
//! delivered, which issues to take next, what to estimate, and what happened since someone was last
//! here. They are computed from the issues (counts, ages, similar issues), not generated: the
//! answers are numbers and lists, and the frontend puts them into words in the person's language.
//! Everything respects what the caller may see, like the rest of the API.

use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::types::Json as SqlJson;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    domain::rich,
    error::{ApiError, ApiResult},
    security::{
        auth::AuthUser,
        authz::{require_list_role, require_role, require_workspace_role, visible_cards_sql, Role},
    },
    services::intel::{self, Doc, Index},
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(board_summary))
        .routes(routes!(sprint_summary))
        .routes(routes!(estimate_backlog))
        .routes(routes!(plan_sprint))
        .routes(routes!(group_backlog))
        .routes(routes!(catch_up))
}

const TITLE: &str = "COALESCE((SELECT c.data ->> tf.slug FROM fields tf WHERE (tf.card_type_id = c.card_type_id OR tf.card_type_id IS NULL) AND tf.is_title AND tf.deleted_at IS NULL LIMIT 1), '')";
const KEY: &str =
    "(SELECT w.key_prefix || '-' || c.number FROM workspaces w WHERE w.id = c.workspace_id)";
/// The signed-in user (`$2`) is among the assignees of the issue.
const MINE: &str = "EXISTS (SELECT 1 FROM fields f WHERE (f.card_type_id = c.card_type_id OR f.card_type_id IS NULL) AND f.is_assignee AND f.deleted_at IS NULL \
    AND CASE WHEN jsonb_typeof(c.data -> f.slug) = 'array' THEN (c.data -> f.slug) ? ($2::uuid)::text ELSE (c.data ->> f.slug) = ($2::uuid)::text END)";

/// Priorities from most to least urgent; an unknown one sorts last.
const PRIORITIES: [&str; 5] = ["highest", "high", "medium", "low", "lowest"];
/// Story points assumed for an issue nobody has estimated and nothing similar can estimate.
const DEFAULT_POINTS: f64 = 3.0;

#[derive(Serialize, ToSchema, Clone)]
pub struct IssueBrief {
    pub card_id: Uuid,
    pub key: String,
    pub title: String,
    pub points: Option<f64>,
}

struct Row {
    id: Uuid,
    key: String,
    title: String,
    body: String,
    stage: String,
    completed: bool,
    sprint: Option<Uuid>,
    parent: Option<Uuid>,
    created: DateTime<Utc>,
    updated: DateTime<Utc>,
    points: Option<f64>,
    priority: Option<String>,
}

impl Row {
    fn brief(&self) -> IssueBrief {
        IssueBrief {
            card_id: self.id,
            key: self.key.clone(),
            title: self.title.clone(),
            points: self.points,
        }
    }

    fn doc(&self) -> Doc {
        Doc {
            key: self.key.clone(),
            title: self.title.clone(),
            body: self.body.clone(),
        }
    }

    fn priority_rank(&self) -> usize {
        self.priority
            .as_deref()
            .and_then(|p| PRIORITIES.iter().position(|x| x.eq_ignore_ascii_case(p)))
            .unwrap_or(PRIORITIES.len())
    }
}

/// The issues of a list the caller may see, in board order.
async fn list_rows(s: &AppState, user: &AuthUser, list_id: Uuid) -> ApiResult<Vec<Row>> {
    let scope = require_list_role(&s.db, user.id, list_id, Role::Viewer).await?;
    let admin = require_role(&s.db, user.id, scope.project_id, Role::Viewer).await? >= Role::Admin;
    type Raw = (
        Uuid,
        String,
        String,
        Option<SqlJson<Value>>,
        String,
        bool,
        Option<Uuid>,
        Option<Uuid>,
        DateTime<Utc>,
        DateTime<Utc>,
        SqlJson<Value>,
    );
    let vis = visible_cards_sql("c", 3, 2); // $3 is the admin flag, $2 the user
    let sql = format!(
        "SELECT c.id, {KEY}, {TITLE},
            (SELECT c.data -> df.slug FROM fields df WHERE (df.card_type_id = c.card_type_id OR df.card_type_id IS NULL) AND df.is_description AND df.deleted_at IS NULL LIMIT 1),
            st.name, st.is_completed, cl.sprint_id, c.parent_id, c.created_at, c.updated_at, c.data
         FROM cards c JOIN card_lists cl ON cl.card_id = c.id AND cl.list_id = $1
         JOIN list_stages st ON st.id = cl.list_stage_id
         WHERE c.deleted_at IS NULL AND {vis}
         ORDER BY cl.sort_order, c.created_at LIMIT 3000"
    );
    let raw: Vec<Raw> = sqlx::query_as(&sql)
        .bind(list_id)
        .bind(user.id)
        .bind(admin)
        .fetch_all(&s.db)
        .await?;
    Ok(raw
        .into_iter()
        .map(
            |(id, key, title, desc, stage, completed, sprint, parent, created, updated, data)| {
                Row {
                    id,
                    key,
                    title,
                    body: desc.map(|d| rich::to_markdown(&d.0)).unwrap_or_default(),
                    stage,
                    completed,
                    sprint,
                    parent,
                    created,
                    updated,
                    points: data.0.get("story_points").and_then(Value::as_f64),
                    priority: data
                        .0
                        .get("priority")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                }
            },
        )
        .collect())
}

async fn throttle(s: &AppState, user: &AuthUser) -> ApiResult<()> {
    super::ai::model(s).await?;
    if s.ai_limiter.check(&user.id.to_string()).await {
        Ok(())
    } else {
        Err(ApiError::TooManyRequests)
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> ApiResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError::Internal(Box::new(e)))
}

fn median(mut v: Vec<i64>) -> i64 {
    v.sort_unstable();
    v.get(v.len() / 2).copied().unwrap_or(0)
}

// ---- board ----------------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct StageCount {
    pub name: String,
    pub count: i64,
    pub completed: bool,
}

#[derive(Serialize, ToSchema)]
pub struct StaleCard {
    pub card_id: Uuid,
    pub key: String,
    pub title: String,
    pub stage: String,
    /// Days since the issue last changed.
    pub days: i64,
    /// The usual number of days an issue sits in that column untouched.
    pub median_days: i64,
}

#[derive(Serialize, ToSchema)]
pub struct BoardSummary {
    pub total: i64,
    pub done: i64,
    /// Issues that changed in the last seven days.
    pub updated_week: i64,
    /// Of those, the ones that are done.
    pub done_week: i64,
    pub by_stage: Vec<StageCount>,
    /// Open issues that have sat untouched much longer than is usual for their column.
    pub stale: Vec<StaleCard>,
}

/// What moved on the board, and what is stuck. "Stuck" is measured by the last change of an issue, since
/// the time an issue entered its column is not recorded: untouched for at least a week and for at
/// least twice as long as is usual in that column.
#[utoipa::path(post, operation_id = "summarizeBoard", path = "/lists/{id}/ai/board", tag = "ai",
    params(("id" = Uuid, Path)), responses((status = 200, body = BoardSummary), (status = 404), (status = 429)),
    security(("bearer" = [])))]
async fn board_summary(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BoardSummary>> {
    let rows = list_rows(&s, &user, id).await?;
    throttle(&s, &user).await?;
    let stages: Vec<(String, bool)> = sqlx::query_as(
        "SELECT name, is_completed FROM list_stages WHERE list_id = $1 ORDER BY sort_order, name",
    )
    .bind(id)
    .fetch_all(&s.db)
    .await?;
    let now = Utc::now();
    let week = now - Duration::days(7);
    let by_stage = stages
        .iter()
        .map(|(name, completed)| StageCount {
            name: name.clone(),
            count: rows.iter().filter(|r| &r.stage == name).count() as i64,
            completed: *completed,
        })
        .collect();
    let usual: HashMap<&str, i64> = stages
        .iter()
        .filter(|(_, done)| !done)
        .map(|(name, _)| {
            let days = rows
                .iter()
                .filter(|r| &r.stage == name && !r.completed)
                .map(|r| (now - r.updated).num_days())
                .collect();
            (name.as_str(), median(days))
        })
        .collect();
    let mut stale: Vec<StaleCard> = rows
        .iter()
        .filter(|r| !r.completed)
        .filter_map(|r| {
            let days = (now - r.updated).num_days();
            let usual = usual.get(r.stage.as_str()).copied().unwrap_or(0);
            (days >= 7 && days >= 2 * usual.max(1)).then(|| StaleCard {
                card_id: r.id,
                key: r.key.clone(),
                title: r.title.clone(),
                stage: r.stage.clone(),
                days,
                median_days: usual,
            })
        })
        .collect();
    stale.sort_by_key(|c| std::cmp::Reverse(c.days));
    stale.truncate(8);
    Ok(Json(BoardSummary {
        total: rows.len() as i64,
        done: rows.iter().filter(|r| r.completed).count() as i64,
        updated_week: rows.iter().filter(|r| r.updated >= week).count() as i64,
        done_week: rows
            .iter()
            .filter(|r| r.completed && r.updated >= week)
            .count() as i64,
        by_stage,
        stale,
    }))
}

// ---- sprint ---------------------------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct SlowCard {
    pub card_id: Uuid,
    pub key: String,
    pub title: String,
    /// Days from creation to the last change.
    pub days: i64,
}

#[derive(Serialize, ToSchema)]
pub struct SprintSummary {
    pub completed: Vec<IssueBrief>,
    /// Not done: these are carried over (or dropped) when the sprint is completed.
    pub open: Vec<IssueBrief>,
    pub points_done: f64,
    pub points_open: f64,
    /// Words that come up in the sprint's issues, most frequent first.
    pub themes: Vec<String>,
    /// The finished issues that took longest from creation to their last change.
    pub slowest: Vec<SlowCard>,
}

/// What a sprint delivered and what is left: the material for retro notes.
#[utoipa::path(post, operation_id = "summarizeSprint", path = "/sprints/{id}/ai/summary", tag = "ai",
    params(("id" = Uuid, Path)), responses((status = 200, body = SprintSummary), (status = 404), (status = 429)),
    security(("bearer" = [])))]
async fn sprint_summary(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SprintSummary>> {
    let list: Option<(Uuid,)> = sqlx::query_as("SELECT list_id FROM sprints WHERE id = $1")
        .bind(id)
        .fetch_optional(&s.db)
        .await?;
    let (list_id,) = list.ok_or(ApiError::NotFound)?;
    let rows = list_rows(&s, &user, list_id).await?;
    throttle(&s, &user).await?;
    let mine: Vec<&Row> = rows.iter().filter(|r| r.sprint == Some(id)).collect();
    let sum = |done: bool| {
        mine.iter()
            .filter(|r| r.completed == done)
            .filter_map(|r| r.points)
            .sum::<f64>()
    };
    let text: String = mine
        .iter()
        .map(|r| format!("{} {}", r.title, r.body))
        .collect::<Vec<_>>()
        .join("\n");
    let mut slowest: Vec<SlowCard> = mine
        .iter()
        .filter(|r| r.completed)
        .map(|r| SlowCard {
            card_id: r.id,
            key: r.key.clone(),
            title: r.title.clone(),
            days: (r.updated - r.created).num_days(),
        })
        .collect();
    slowest.sort_by_key(|c| std::cmp::Reverse(c.days));
    slowest.truncate(3);
    Ok(Json(SprintSummary {
        completed: mine
            .iter()
            .filter(|r| r.completed)
            .map(|r| r.brief())
            .collect(),
        open: mine
            .iter()
            .filter(|r| !r.completed)
            .map(|r| r.brief())
            .collect(),
        points_done: sum(true),
        points_open: sum(false),
        themes: intel::keywords(&text, 5),
        slowest,
    }))
}

// ---- backlog --------------------------------------------------------------------------------

/// Backlog issues without points get the median of the similar issues that have some.
fn estimate_all(rows: &[Row], index: &Index, docs: &[Doc]) -> HashMap<Uuid, (f64, f32, usize)> {
    let mut out = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        if r.points.is_some() || r.completed {
            continue;
        }
        let hits = index.similar(&docs[i].title, &docs[i].body, Some(i), 10);
        let known: Vec<(f64, f32)> = hits
            .iter()
            .filter(|h| h.score >= intel::RELATED_SCORE)
            .filter_map(|h| rows[h.doc].points.map(|p| (p, h.score)))
            .collect();
        if let Some((points, near)) = intel::weighted_median(&known) {
            out.insert(r.id, (points, near, known.len()));
        }
    }
    out
}

#[derive(Serialize, ToSchema)]
pub struct Estimate {
    pub card_id: Uuid,
    pub key: String,
    pub title: String,
    pub points: f64,
    /// Share of the similar issues' weight within one step of the suggestion, 0 to 1.
    pub confidence: f32,
    pub based_on: usize,
}

/// Story points for unestimated backlog issues, from similar issues that have some.
#[utoipa::path(post, operation_id = "estimateBacklog", path = "/lists/{id}/ai/estimate", tag = "ai",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<Estimate>), (status = 404), (status = 429)),
    security(("bearer" = [])))]
async fn estimate_backlog(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<Estimate>>> {
    let rows = list_rows(&s, &user, id).await?;
    throttle(&s, &user).await?;
    let out = blocking(move || {
        let docs: Vec<Doc> = rows.iter().map(Row::doc).collect();
        let index = Index::build(&docs);
        let est = estimate_all(&rows, &index, &docs);
        rows.iter()
            .filter(|r| r.sprint.is_none())
            .filter_map(|r| {
                let (points, near, n) = est.get(&r.id)?;
                Some(Estimate {
                    card_id: r.id,
                    key: r.key.clone(),
                    title: r.title.clone(),
                    points: *points,
                    confidence: (near * 100.0).round() / 100.0,
                    based_on: *n,
                })
            })
            .take(40)
            .collect::<Vec<_>>()
    })
    .await?;
    Ok(Json(out))
}

#[derive(Serialize, ToSchema)]
pub struct PlanPick {
    pub card_id: Uuid,
    pub key: String,
    pub title: String,
    pub points: f64,
    /// The points are a guess (from similar issues, or the default), not set on the issue.
    pub estimated: bool,
    pub priority: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct SprintPlan {
    /// Average points finished per sprint over the last completed sprints; `null` without history.
    pub velocity: Option<f64>,
    pub sprints_used: i64,
    pub capacity: f64,
    pub total: f64,
    pub picks: Vec<PlanPick>,
}

/// A proposal for the next sprint: the most urgent backlog issues that fit the team's recent velocity.
#[utoipa::path(post, operation_id = "planSprint", path = "/lists/{id}/ai/plan", tag = "ai",
    params(("id" = Uuid, Path)), responses((status = 200, body = SprintPlan), (status = 404), (status = 429)),
    security(("bearer" = [])))]
async fn plan_sprint(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SprintPlan>> {
    let rows = list_rows(&s, &user, id).await?;
    throttle(&s, &user).await?;
    let past: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM sprints WHERE list_id = $1 AND state = 'completed' ORDER BY completed_at DESC NULLS LAST, created_at DESC LIMIT 3",
    )
    .bind(id)
    .fetch_all(&s.db)
    .await?;
    let per_sprint: Vec<f64> = past
        .iter()
        .map(|(sp,)| {
            rows.iter()
                .filter(|r| r.sprint == Some(*sp) && r.completed)
                .filter_map(|r| r.points)
                .sum()
        })
        .collect();
    let velocity =
        (!per_sprint.is_empty()).then(|| per_sprint.iter().sum::<f64>() / per_sprint.len() as f64);
    let capacity = velocity.unwrap_or(0.0);
    let out = blocking(move || {
        let docs: Vec<Doc> = rows.iter().map(Row::doc).collect();
        let est = estimate_all(&rows, &Index::build(&docs), &docs);
        let mut candidates: Vec<&Row> = rows
            .iter()
            .filter(|r| r.sprint.is_none() && !r.completed && r.parent.is_none())
            .collect();
        candidates.sort_by_key(|r| r.priority_rank()); // stable: board order within a priority
        let (mut total, mut picks) = (0.0, vec![]);
        for r in candidates {
            let (points, estimated) = match r.points {
                Some(p) => (p, false),
                None => (est.get(&r.id).map_or(DEFAULT_POINTS, |e| e.0), true),
            };
            if points > 0.0 && total + points <= capacity {
                total += points;
                picks.push(PlanPick {
                    card_id: r.id,
                    key: r.key.clone(),
                    title: r.title.clone(),
                    points,
                    estimated,
                    priority: r.priority.clone(),
                });
            }
        }
        (total, picks)
    })
    .await?;
    Ok(Json(SprintPlan {
        velocity,
        sprints_used: per_sprint.len() as i64,
        capacity,
        total: out.0,
        picks: out.1,
    }))
}

#[derive(Serialize, ToSchema)]
pub struct IssueGroup {
    /// What the issues of the group have in common, as words.
    pub keywords: Vec<String>,
    pub cards: Vec<IssueBrief>,
}

/// Loose backlog issues grouped by topic (candidates for an epic). Only groups of at least two.
#[utoipa::path(post, operation_id = "groupBacklog", path = "/lists/{id}/ai/groups", tag = "ai",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<IssueGroup>), (status = 404), (status = 429)),
    security(("bearer" = [])))]
async fn group_backlog(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<IssueGroup>>> {
    let rows = list_rows(&s, &user, id).await?;
    throttle(&s, &user).await?;
    let out = blocking(move || {
        let loose: Vec<&Row> = rows
            .iter()
            .filter(|r| r.sprint.is_none() && !r.completed && r.parent.is_none())
            .collect();
        let docs: Vec<Doc> = loose.iter().map(|r| r.doc()).collect();
        let index = Index::build(&docs);
        let mut taken: HashSet<usize> = HashSet::new();
        let mut groups = vec![];
        for i in 0..loose.len() {
            if taken.contains(&i) {
                continue;
            }
            let hits: Vec<_> = index
                .similar(&docs[i].title, &docs[i].body, Some(i), 12)
                .into_iter()
                .filter(|h| h.score >= 0.3 && !taken.contains(&h.doc))
                .collect();
            if hits.is_empty() {
                continue;
            }
            let members: Vec<usize> = std::iter::once(i)
                .chain(hits.iter().map(|h| h.doc))
                .collect();
            taken.extend(members.iter().copied());
            let text = members
                .iter()
                .map(|m| docs[*m].title.clone())
                .collect::<Vec<_>>()
                .join(" ");
            let mut keywords = intel::keywords(&text, 3);
            if keywords.is_empty() {
                keywords = hits[0].shared.iter().take(3).cloned().collect();
            }
            groups.push(IssueGroup {
                keywords,
                cards: members.iter().map(|m| loose[*m].brief()).collect(),
            });
        }
        groups.sort_by_key(|g| std::cmp::Reverse(g.cards.len()));
        groups.truncate(8);
        groups
    })
    .await?;
    Ok(Json(out))
}

// ---- catch up -------------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CatchUpQuery {
    /// Since when; default and at most 30 days back.
    pub since: Option<DateTime<Utc>>,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct IssueTitle {
    pub card_id: Uuid,
    pub key: String,
    pub title: String,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct Commented {
    pub card_id: Uuid,
    pub key: String,
    pub title: String,
    /// New comments by other people since then.
    pub count: i64,
}

#[derive(Serialize, ToSchema)]
pub struct CatchUp {
    pub since: DateTime<Utc>,
    /// Open issues assigned to the caller.
    pub assigned_open: i64,
    /// Assigned to the caller and finished since then.
    pub finished: Vec<IssueTitle>,
    /// Assigned to the caller and commented on by someone else since then.
    pub commented: Vec<Commented>,
    /// Issues where the caller was mentioned since then.
    pub mentioned: Vec<IssueTitle>,
}

/// "Since you were last here": the caller's open work, what got finished, new comments and mentions.
#[utoipa::path(get, operation_id = "catchUp", path = "/workspaces/{id}/ai/catch-up", tag = "ai",
    params(("id" = Uuid, Path), ("since" = Option<DateTime<Utc>>, Query)),
    responses((status = 200, body = CatchUp), (status = 404), (status = 429)),
    security(("bearer" = [])))]
async fn catch_up(
    State(s): State<AppState>,
    user: AuthUser,
    Path(wid): Path<Uuid>,
    Query(q): Query<CatchUpQuery>,
) -> ApiResult<Json<CatchUp>> {
    let project = require_workspace_role(&s.db, user.id, wid, Role::Viewer).await?;
    let admin = require_role(&s.db, user.id, project, Role::Viewer).await? >= Role::Admin;
    throttle(&s, &user).await?;
    let floor = Utc::now() - Duration::days(30);
    let since = q
        .since
        .map_or(Utc::now() - Duration::days(7), |t| t.max(floor));
    let vis = visible_cards_sql("c", 3, 2);
    let open = |extra: &str| {
        format!(
            "FROM cards c WHERE c.workspace_id = $1 AND c.deleted_at IS NULL AND {MINE} AND {vis} {extra}"
        )
    };
    let not_done = "AND NOT EXISTS (SELECT 1 FROM card_lists cl JOIN list_stages st ON st.id = cl.list_stage_id WHERE cl.card_id = c.id AND st.is_completed)";
    let (assigned_open,): (i64,) = sqlx::query_as(&format!("SELECT count(*) {}", open(not_done)))
        .bind(wid)
        .bind(user.id)
        .bind(admin)
        .fetch_one(&s.db)
        .await?;
    let finished: Vec<IssueTitle> = sqlx::query_as(&format!(
        "SELECT c.id AS card_id, {KEY} AS key, {TITLE} AS title {} \
         AND EXISTS (SELECT 1 FROM card_lists cl JOIN list_stages st ON st.id = cl.list_stage_id WHERE cl.card_id = c.id AND st.is_completed) \
         AND c.updated_at > $4 ORDER BY c.updated_at DESC LIMIT 10",
        open("")
    ))
    .bind(wid)
    .bind(user.id)
    .bind(admin)
    .bind(since)
    .fetch_all(&s.db)
    .await?;
    let commented: Vec<Commented> = sqlx::query_as(&format!(
        "SELECT c.id AS card_id, {KEY} AS key, {TITLE} AS title, \
            (SELECT count(*) FROM card_activities a WHERE a.card_id = c.id AND a.type = 'comment' AND a.created_at > $4 AND a.created_by IS DISTINCT FROM $2) AS count \
         {} AND EXISTS (SELECT 1 FROM card_activities a WHERE a.card_id = c.id AND a.type = 'comment' AND a.created_at > $4 AND a.created_by IS DISTINCT FROM $2) \
         ORDER BY c.updated_at DESC LIMIT 10",
        open("")
    ))
    .bind(wid)
    .bind(user.id)
    .bind(admin)
    .bind(since)
    .fetch_all(&s.db)
    .await?;
    let mentioned: Vec<IssueTitle> = sqlx::query_as(&format!(
        "SELECT DISTINCT ON (c.id) c.id AS card_id, {KEY} AS key, {TITLE} AS title \
         FROM notifications n JOIN cards c ON c.id = n.related_resource_id \
         WHERE n.recipient_id = $2 AND n.workspace_id = $1 AND n.type = 'mention' AND n.related_resource_type = 'card' \
           AND n.created_at > $4 AND c.deleted_at IS NULL AND {vis} LIMIT 10"
    ))
    .bind(wid)
    .bind(user.id)
    .bind(admin)
    .bind(since)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(CatchUp {
        since,
        assigned_open,
        finished,
        commented,
        mentioned,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_median_of_nothing_is_zero_and_of_many_the_middle() {
        assert_eq!(median(vec![]), 0);
        assert_eq!(median(vec![9, 1, 5]), 5);
        assert_eq!(median(vec![4, 2]), 4);
    }
}
