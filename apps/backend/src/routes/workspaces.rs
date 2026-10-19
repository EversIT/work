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
    auth::AuthUser,
    authz::{require_role, require_workspace_role, Role},
    error::{ApiError, ApiResult},
    state::AppState,
    util::short,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list, create))
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
    let key = match &b.key_prefix {
        Some(k) => {
            check_key(k)?;
            k.clone()
        }
        None => derive_key_prefix(&b.name),
    };
    let mut tx = s.db.begin().await?;
    let sql = format!("INSERT INTO workspaces (id, project_id, name, key_prefix) VALUES ($1,$2,$3,$4) RETURNING {COLS}");
    let row: WorkspaceOut = sqlx::query_as(&sql)
        .bind(Uuid::new_v4())
        .bind(pid)
        .bind(b.name.trim())
        .bind(&key)
        .fetch_one(&mut *tx)
        .await?;
    // Every workspace starts with Jira-style issue types and fields.
    super::card_types::seed_default_card_type(&mut tx, row.id, user.id).await?;
    tx.commit().await?;
    Ok(Json(row))
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
        assert!(check_key("WORK").is_ok() && check_key("W2").is_ok());
        assert!(
            check_key("w").is_err()
                && check_key("1AB").is_err()
                && check_key("A-B").is_err()
                && check_key("TOOLONGKEY1").is_err()
        );
    }
}
