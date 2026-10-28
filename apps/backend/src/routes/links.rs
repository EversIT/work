//! Links between issues: "DPU-1 blocks DPU-2", "relates to", "duplicates", "clones". A link is one
//! row; each end sees it with its own wording (`outward`: "blocks", `inward`: "is blocked by").
//! Both ends must be in one workspace, and the caller must be allowed to see the other end.

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::types::Json as SqlJson;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::{
    cards::{load_card, CardOut, CARD_COLS},
    OkOut,
};
use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthUser,
    security::authz::{require_card_role, require_role, visible_cards_sql, Role},
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_links, create_link))
        .routes(routes!(delete_link))
}

const TYPES: [&str; 4] = ["blocks", "duplicates", "relates", "clones"];
const MAX_LINKS: i64 = 200;

/// One link as seen from the issue it was asked for.
#[derive(Serialize, ToSchema)]
pub struct CardLinkOut {
    pub id: Uuid,
    /// `blocks`, `duplicates`, `relates` or `clones`.
    pub link_type: String,
    /// `outward`: this issue blocks/duplicates/clones the other. `inward`: it is blocked,
    /// duplicated or cloned by it. `relates` is the same both ways and always `outward`.
    pub direction: String,
    /// The issue at the other end.
    pub card: CardOut,
}

#[derive(sqlx::FromRow)]
struct LinkRow {
    link_id: Uuid,
    link_type: String,
    outward: bool,
    #[sqlx(flatten)]
    card: CardOut,
}

impl From<LinkRow> for CardLinkOut {
    fn from(r: LinkRow) -> Self {
        let direction = if r.outward || r.link_type == "relates" {
            "outward"
        } else {
            "inward"
        };
        CardLinkOut {
            id: r.link_id,
            link_type: r.link_type,
            direction: direction.to_owned(),
            card: r.card,
        }
    }
}

#[utoipa::path(get, operation_id = "listCardLinks", path = "/cards/{id}/links", tag = "cards",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<CardLinkOut>), (status = 404)), security(("bearer" = [])))]
async fn list_links(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<CardLinkOut>>> {
    let scope = require_card_role(&s.db, user.id, id, Role::Viewer).await?;
    let role = require_role(&s.db, user.id, scope.project_id, Role::Viewer).await?;
    // The other end can sit in a list the caller has no access to: it is left out, not shown.
    let vis = visible_cards_sql("c", 2, 3);
    let sql = format!(
        "SELECT l.id AS link_id, l.link_type, (l.source_card_id = $1) AS outward, {CARD_COLS}
         FROM card_links l
         JOIN cards c ON c.id = CASE WHEN l.source_card_id = $1 THEN l.target_card_id ELSE l.source_card_id END
         WHERE (l.source_card_id = $1 OR l.target_card_id = $1) AND c.deleted_at IS NULL AND {vis}
         ORDER BY l.link_type, l.created_at LIMIT {MAX_LINKS}"
    );
    let rows: Vec<LinkRow> = sqlx::query_as(&sql)
        .bind(id)
        .bind(role >= Role::Admin)
        .bind(user.id)
        .fetch_all(&s.db)
        .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[derive(Deserialize, ToSchema)]
pub struct CreateLinkBody {
    /// `blocks`, `duplicates`, `relates` or `clones`.
    pub link_type: String,
    /// `outward` (default): the issue in the path blocks/duplicates/clones the target. `inward`:
    /// the target does so to the issue in the path.
    pub direction: Option<String>,
    /// The issue at the other end.
    pub target_id: Uuid,
}

#[utoipa::path(post, operation_id = "createCardLink", path = "/cards/{id}/links", tag = "cards",
    params(("id" = Uuid, Path)), request_body = CreateLinkBody,
    responses((status = 200, body = CardLinkOut), (status = 400), (status = 403), (status = 404), (status = 409)),
    security(("bearer" = [])))]
async fn create_link(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<CreateLinkBody>,
) -> ApiResult<Json<CardLinkOut>> {
    if !TYPES.contains(&b.link_type.as_str()) {
        return Err(ApiError::BadRequest("unknown link type".into()));
    }
    let inward = match b.direction.as_deref() {
        None | Some("outward") => false,
        Some("inward") => true,
        Some(_) => {
            return Err(ApiError::BadRequest(
                "direction must be outward or inward".into(),
            ))
        }
    };
    if b.target_id == id {
        return Err(ApiError::BadRequest(
            "an issue cannot be linked to itself".into(),
        ));
    }
    let scope = require_card_role(&s.db, user.id, id, Role::Member).await?;
    // Seeing the other end is enough: linking does not change it.
    let other = require_card_role(&s.db, user.id, b.target_id, Role::Viewer).await?;
    if other.workspace_id != scope.workspace_id {
        return Err(ApiError::BadRequest(
            "issues can only be linked within one workspace".into(),
        ));
    }
    let (source, target) = if inward {
        (b.target_id, id)
    } else {
        (id, b.target_id)
    };

    let mut tx = s.db.begin().await?;
    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM card_links WHERE source_card_id = $1 OR target_card_id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if count >= MAX_LINKS {
        return Err(ApiError::BadRequest("too many links on this issue".into()));
    }
    // The same pair with the same type in either direction is already there ("A blocks B" and
    // "B blocks A" would contradict each other).
    let (taken,): (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM card_links WHERE link_type = $3
           AND ((source_card_id = $1 AND target_card_id = $2) OR (source_card_id = $2 AND target_card_id = $1)))",
    )
    .bind(source)
    .bind(target)
    .bind(&b.link_type)
    .fetch_one(&mut *tx)
    .await?;
    if taken {
        return Err(ApiError::Conflict);
    }
    let link_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO card_links (id, source_card_id, target_card_id, link_type, created_by) VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(link_id)
    .bind(source)
    .bind(target)
    .bind(&b.link_type)
    .bind(user.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    log(&s, &user, "added", &b.link_type, id, b.target_id, inward).await?;
    let out = linked(&s, &user, id, link_id).await?;
    Ok(Json(out))
}

#[utoipa::path(delete, operation_id = "deleteCardLink", path = "/cards/{id}/links/{link_id}", tag = "cards",
    params(("id" = Uuid, Path), ("link_id" = Uuid, Path)),
    responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn delete_link(
    State(s): State<AppState>,
    user: AuthUser,
    Path((id, link_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<OkOut>> {
    require_card_role(&s.db, user.id, id, Role::Member).await?;
    let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "DELETE FROM card_links WHERE id = $1 AND (source_card_id = $2 OR target_card_id = $2)
         RETURNING source_card_id, target_card_id, link_type",
    )
    .bind(link_id)
    .bind(id)
    .fetch_optional(&s.db)
    .await?;
    let (source, target, link_type) = row.ok_or(ApiError::NotFound)?;
    let (other, inward) = if source == id {
        (target, false)
    } else {
        (source, true)
    };
    log(&s, &user, "removed", &link_type, id, other, inward).await?;
    Ok(Json(OkOut { ok: true }))
}

/// The link as `card_id` sees it (re-read through the same visibility rules as the list).
async fn linked(
    s: &AppState,
    user: &AuthUser,
    card_id: Uuid,
    link_id: Uuid,
) -> ApiResult<CardLinkOut> {
    let scope = require_card_role(&s.db, user.id, card_id, Role::Viewer).await?;
    let role = require_role(&s.db, user.id, scope.project_id, Role::Viewer).await?;
    let vis = visible_cards_sql("c", 3, 4);
    let sql = format!(
        "SELECT l.id AS link_id, l.link_type, (l.source_card_id = $1) AS outward, {CARD_COLS}
         FROM card_links l
         JOIN cards c ON c.id = CASE WHEN l.source_card_id = $1 THEN l.target_card_id ELSE l.source_card_id END
         WHERE l.id = $2 AND c.deleted_at IS NULL AND {vis}"
    );
    let row: Option<LinkRow> = sqlx::query_as(&sql)
        .bind(card_id)
        .bind(link_id)
        .bind(role >= Role::Admin)
        .bind(user.id)
        .fetch_optional(&s.db)
        .await?;
    row.map(Into::into).ok_or(ApiError::NotFound)
}

/// Writes the history entry on both issues: "linked: blocks DPU-2" here, "is blocked by DPU-1" there.
async fn log(
    s: &AppState,
    user: &AuthUser,
    action: &str,
    link_type: &str,
    card_id: Uuid,
    other_id: Uuid,
    inward: bool,
) -> ApiResult<()> {
    let (a, b) = (
        load_card(s, card_id).await.ok(),
        load_card(s, other_id).await.ok(),
    );
    for (on, key, inward) in [
        (card_id, b.as_ref().map(|c| c.key.clone()), inward),
        (other_id, a.as_ref().map(|c| c.key.clone()), !inward),
    ] {
        let Some(key) = key else { continue };
        let content = SqlJson(serde_json::json!({
            "link": {"action": action, "type": link_type, "direction": if inward { "inward" } else { "outward" }, "key": key}
        }));
        sqlx::query("INSERT INTO card_activities (id, card_id, type, content, created_by, created_by_type) VALUES ($1,$2,'update',$3,$4,'user')")
            .bind(Uuid::new_v4())
            .bind(on)
            .bind(content)
            .bind(user.id)
            .execute(&s.db)
            .await?;
    }
    Ok(())
}
