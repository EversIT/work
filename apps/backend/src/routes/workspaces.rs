use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::OkOut;
use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthUser,
    security::authz::{require_role, require_workspace_role, Role},
    state::AppState,
    util::short,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list, create))
        .routes(routes!(by_key))
        .routes(routes!(get_one, update, remove))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct WorkspaceOut {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    /// Prefix of issue keys, e.g. `WORK` for `WORK-12`.
    pub key_prefix: String,
    pub created_at: DateTime<Utc>,
}

const COLS: &str = "id, project_id, name, key_prefix, created_at";

/// `Acme Corp` -> `ACME`; falls back to `WRK` when the name has no usable letters.
pub fn derive_key_prefix(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase();
    let prefix: String = cleaned.chars().take(4).collect();
    if prefix.len() >= 2 && prefix.starts_with(|c: char| c.is_ascii_uppercase()) {
        prefix
    } else {
        "WRK".into()
    }
}

fn check_key(k: &str) -> ApiResult<()> {
    let ok = (2..=10).contains(&k.len())
        && k.starts_with(|c: char| c.is_ascii_uppercase())
        && k.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    if ok {
        Ok(())
    } else {
        Err(ApiError::BadRequest(
            "key must be 2-10 characters: capital letters and digits, starting with a letter"
                .into(),
        ))
    }
}

#[utoipa::path(operation_id = "listWorkspaces", get, path = "/projects/{id}/workspaces", tag = "workspaces",
    params(("id" = Uuid, Path)), responses((status = 200, body = Vec<WorkspaceOut>), (status = 404)), security(("bearer" = [])))]
async fn list(
    State(s): State<AppState>,
    user: AuthUser,
    Path(pid): Path<Uuid>,
) -> ApiResult<Json<Vec<WorkspaceOut>>> {
    require_role(&s.db, user.id, pid, Role::Viewer).await?;
    let sql = format!("SELECT {COLS} FROM workspaces WHERE project_id = $1 ORDER BY created_at");
    Ok(Json(sqlx::query_as(&sql).bind(pid).fetch_all(&s.db).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct WorkspaceCreate {
    pub name: String,
    /// Optional issue key prefix; derived from the name when omitted.
    pub key_prefix: Option<String>,
}

/// `base` with a numeric suffix (`n` >= 2), shortened so the result stays within 10 characters:
/// `WORK` -> `WORK2`, `ABCDEFGHIJ` -> `ABCDEFGHI2`. `n` = 1 is the key itself.
pub fn suffixed_key(base: &str, n: u32) -> String {
    if n <= 1 {
        return base.to_owned();
    }
    let suffix = n.to_string();
    let keep = 10usize.saturating_sub(suffix.len());
    format!("{}{suffix}", &base[..base.len().min(keep)])
}

/// Insert a workspace plus its default issue types. Keys are unique instance-wide: with
/// `derived` the first free `base`, `base2`, `base3`, ... is taken (a concurrent creation that
/// wins the race is retried); otherwise a taken key is a 409.
async fn insert_workspace(
    s: &AppState,
    project_id: Uuid,
    user_id: Uuid,
    name: &str,
    base: &str,
    derived: bool,
) -> ApiResult<WorkspaceOut> {
    let sql = format!("INSERT INTO workspaces (id, project_id, name, key_prefix) VALUES ($1,$2,$3,$4) RETURNING {COLS}");
    for _ in 0..5 {
        let key = if derived {
            let candidates: Vec<String> = (1..=1000).map(|n| suffixed_key(base, n)).collect();
            let taken: Vec<(String,)> =
                sqlx::query_as("SELECT key_prefix FROM workspaces WHERE key_prefix = ANY($1)")
                    .bind(&candidates)
                    .fetch_all(&s.db)
                    .await?;
            candidates
                .into_iter()
                .find(|c| !taken.iter().any(|(t,)| t == c))
                .ok_or(ApiError::Conflict)?
        } else {
            base.to_owned()
        };
        let mut tx = s.db.begin().await?;
        let row: WorkspaceOut = match sqlx::query_as(&sql)
            .bind(Uuid::new_v4())
            .bind(project_id)
            .bind(name)
            .bind(&key)
            .fetch_one(&mut *tx)
            .await
        {
            Ok(r) => r,
            Err(sqlx::Error::Database(e)) if derived && e.is_unique_violation() => continue,
            Err(e) => return Err(e.into()),
        };
        // Every workspace starts with Jira-style issue types and fields.
        super::card_types::seed_default_card_type(&mut tx, row.id, user_id).await?;
        tx.commit().await?;
        return Ok(row);
    }
    Err(ApiError::Conflict)
}

#[utoipa::path(operation_id = "createWorkspace", post, path = "/projects/{id}/workspaces", tag = "workspaces",
    params(("id" = Uuid, Path)), request_body = WorkspaceCreate,
    responses((status = 200, body = WorkspaceOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn create(
    State(s): State<AppState>,
    user: AuthUser,
    Path(pid): Path<Uuid>,
    Json(b): Json<WorkspaceCreate>,
) -> ApiResult<Json<WorkspaceOut>> {
    require_role(&s.db, user.id, pid, Role::Member).await?;
    short(&b.name, 100, "name")?;
    let row = match &b.key_prefix {
        // An explicit key that is taken is a conflict (the unique index maps to 409).
        Some(k) => {
            check_key(k)?;
            insert_workspace(&s, pid, user.id, b.name.trim(), k, false).await?
        }
        None => {
            insert_workspace(
                &s,
                pid,
                user.id,
                b.name.trim(),
                &derive_key_prefix(&b.name),
                true,
            )
            .await?
        }
    };
    Ok(Json(row))
}

/// Look a workspace up by its issue-key prefix (`ACME`), case-insensitively. Unknown keys and
/// workspaces of projects the caller is not in look identical.
#[utoipa::path(get, operation_id = "getWorkspaceByKey", path = "/workspaces/by-key/{key}", tag = "workspaces",
    params(("key" = String, Path)), responses((status = 200, body = WorkspaceOut), (status = 404)), security(("bearer" = [])))]
async fn by_key(
    State(s): State<AppState>,
    user: AuthUser,
    Path(key): Path<String>,
) -> ApiResult<Json<WorkspaceOut>> {
    let id = workspace_id_by_key(&s.db, &key)
        .await?
        .ok_or(ApiError::NotFound)?;
    get_one(State(s), user, Path(id)).await
}

/// The workspace owning an issue-key prefix (`acme` and `ACME` are the same key).
pub async fn workspace_id_by_key(db: &sqlx::PgPool, key: &str) -> ApiResult<Option<Uuid>> {
    let row: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM workspaces WHERE key_prefix = $1")
        .bind(key.to_uppercase())
        .fetch_optional(db)
        .await?;
    Ok(row.map(|(id,)| id))
}

#[utoipa::path(get, operation_id = "getWorkspace", path = "/workspaces/{id}", tag = "workspaces",
    params(("id" = Uuid, Path)), responses((status = 200, body = WorkspaceOut), (status = 404)), security(("bearer" = [])))]
async fn get_one(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<WorkspaceOut>> {
    require_workspace_role(&s.db, user.id, id, Role::Viewer).await?;
    let sql = format!("SELECT {COLS} FROM workspaces WHERE id = $1");
    Ok(Json(
        sqlx::query_as(&sql)
            .bind(id)
            .fetch_optional(&s.db)
            .await?
            .ok_or(ApiError::NotFound)?,
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct WorkspacePatch {
    pub name: Option<String>,
    pub key_prefix: Option<String>,
}

#[utoipa::path(operation_id = "updateWorkspace", patch, path = "/workspaces/{id}", tag = "workspaces",
    params(("id" = Uuid, Path)), request_body = WorkspacePatch,
    responses((status = 200, body = WorkspaceOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn update(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<WorkspacePatch>,
) -> ApiResult<Json<WorkspaceOut>> {
    require_workspace_role(&s.db, user.id, id, Role::Member).await?;
    if let Some(n) = &b.name {
        short(n, 100, "name")?;
    }
    if let Some(k) = &b.key_prefix {
        check_key(k)?; // changing the key renames every issue key; keep it an admin action below
        require_workspace_role(&s.db, user.id, id, Role::Admin).await?;
    }
    let sql = format!("UPDATE workspaces SET name = COALESCE($2, name), key_prefix = COALESCE($3, key_prefix) WHERE id = $1 RETURNING {COLS}");
    let row = sqlx::query_as(&sql)
        .bind(id)
        .bind(b.name.as_deref().map(str::trim))
        .bind(&b.key_prefix)
        .fetch_optional(&s.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(row))
}

#[utoipa::path(delete, operation_id = "deleteWorkspace", path = "/workspaces/{id}", tag = "workspaces",
    params(("id" = Uuid, Path)), responses((status = 200, body = OkOut), (status = 403), (status = 404)), security(("bearer" = [])))]
async fn remove(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<OkOut>> {
    require_workspace_role(&s.db, user.id, id, Role::Admin).await?;
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(Json(OkOut { ok: true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_prefixes() {
        assert_eq!(derive_key_prefix("Acme Corp"), "ACME");
        assert_eq!(derive_key_prefix("work"), "WORK");
        assert_eq!(derive_key_prefix("1st"), "WRK");
        assert_eq!(derive_key_prefix("!!"), "WRK");
        assert_eq!(suffixed_key("WORK", 1), "WORK");
        assert_eq!(suffixed_key("WORK", 2), "WORK2");
        assert_eq!(suffixed_key("ABCDEFGHIJ", 2), "ABCDEFGHI2");
        assert_eq!(suffixed_key("ABCDEFGHIJ", 12), "ABCDEFGH12");
        assert!(check_key("WORK").is_ok() && check_key("W2").is_ok());
        assert!(
            check_key("w").is_err()
                && check_key("1AB").is_err()
                && check_key("A-B").is_err()
                && check_key("TOOLONGKEY1").is_err()
        );
    }
}
