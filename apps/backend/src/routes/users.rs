use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use super::OkOut;
use crate::{
    auth::{hash_password, verify_password, AuthUser, MAX_PASSWORD_LEN, MIN_PASSWORD_LEN},
    error::{ApiError, ApiResult},
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(update_me, delete_me))
        .routes(routes!(change_password))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct UserBrief {
    pub id: Uuid,
    pub first_name: String,
    pub last_name: String,
    pub email: String,
}

#[derive(Deserialize, ToSchema)]
pub struct ProfilePatch {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

/// Only these two fields are editable here. Email changes would need re-verification and roles are
/// never client-writable.
#[utoipa::path(patch, operation_id = "updateMe", path = "/me", tag = "auth",
    request_body = ProfilePatch, responses((status = 200, body = UserBrief), (status = 400)), security(("bearer" = [])))]
async fn update_me(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<ProfilePatch>,
) -> ApiResult<Json<UserBrief>> {
    for n in [&b.first_name, &b.last_name].into_iter().flatten() {
        if n.trim().is_empty() || n.chars().count() > 100 {
            return Err(ApiError::BadRequest(
                "names must be 1-100 characters".into(),
            ));
        }
    }
    let row = sqlx::query_as("UPDATE users SET first_name = COALESCE($2, first_name), last_name = COALESCE($3, last_name) WHERE id = $1 RETURNING id, first_name, last_name, email::text AS email")
        .bind(user.id).bind(b.first_name.as_deref().map(str::trim)).bind(b.last_name.as_deref().map(str::trim)).fetch_one(&s.db).await?;
    Ok(Json(row))
}

#[derive(Deserialize, ToSchema)]
pub struct PasswordBody {
    pub current_password: String,
    pub new_password: String,
}

async fn verify_current(s: &AppState, user: &AuthUser, password: String) -> ApiResult<()> {
    // Per-account throttle so a stolen access token cannot be used to brute-force the password.
    if !s.auth_limiter.check(&format!("pwcheck:{}", user.id)) {
        return Err(ApiError::TooManyRequests);
    }
    let (hash,): (String,) = sqlx::query_as("SELECT password_hash FROM users WHERE id = $1")
        .bind(user.id)
        .fetch_one(&s.db)
        .await?;
    let ok = tokio::task::spawn_blocking(move || verify_password(&password, &hash))
        .await
        .map_err(|e| ApiError::Internal(Box::new(e)))?;
    if ok {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

/// Requires the current password. Every other session is revoked, so a leaked token or cookie
/// stops working the moment the password changes.
#[utoipa::path(post, operation_id = "changePassword", path = "/me/password", tag = "auth",
    request_body = PasswordBody, responses((status = 200, body = OkOut), (status = 400), (status = 403)), security(("bearer" = [])))]
async fn change_password(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<PasswordBody>,
) -> ApiResult<Json<OkOut>> {
    // Credentials can only be changed from a real session, never with an API token.
    let session = user.require_session()?;
    let n = b.new_password.chars().count();
    if !(MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&n) {
        return Err(ApiError::BadRequest(format!(
            "password must be {MIN_PASSWORD_LEN}-{MAX_PASSWORD_LEN} characters"
        )));
    }
    verify_current(&s, &user, b.current_password).await?;
    let new = b.new_password;
    let hash = tokio::task::spawn_blocking(move || hash_password(&new))
        .await
        .map_err(|e| ApiError::Internal(Box::new(e)))??;
    let mut tx = s.db.begin().await?;
    sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
        .bind(user.id)
        .bind(hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND id <> $2 AND revoked_at IS NULL").bind(user.id).bind(session).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(OkOut { ok: true }))
}

#[derive(Deserialize, ToSchema)]
pub struct DeleteMeBody {
    pub password: String,
}

/// Deletes the account. Refused while the user is the only owner of a project that other people
/// still use: transfer or delete the project first.
#[utoipa::path(delete, operation_id = "deleteMe", path = "/me", tag = "auth",
    request_body = DeleteMeBody, responses((status = 200, body = OkOut), (status = 403), (status = 409)), security(("bearer" = [])))]
async fn delete_me(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<DeleteMeBody>,
) -> ApiResult<Json<OkOut>> {
    user.require_session()?;
    verify_current(&s, &user, b.password).await?;
    let blocking: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM project_members m WHERE m.user_id = $1 AND m.role = 'owner'
           AND NOT EXISTS (SELECT 1 FROM project_members o WHERE o.project_id = m.project_id AND o.role = 'owner' AND o.user_id <> $1)
           AND EXISTS (SELECT 1 FROM project_members x WHERE x.project_id = m.project_id AND x.user_id <> $1)")
        .bind(user.id).fetch_one(&s.db).await?;
    if blocking.0 > 0 {
        return Err(ApiError::Conflict);
    }
    let mut tx = s.db.begin().await?;
    // Projects only this user belongs to disappear with them.
    sqlx::query("DELETE FROM projects p WHERE EXISTS (SELECT 1 FROM project_members m WHERE m.project_id = p.id AND m.user_id = $1)
                 AND NOT EXISTS (SELECT 1 FROM project_members o WHERE o.project_id = p.id AND o.user_id <> $1)").bind(user.id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM project_members WHERE user_id = $1")
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM user_integrations WHERE user_id = $1")
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL")
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    // Keep the row (authorship of activities) but make the account unusable and anonymous.
    sqlx::query("UPDATE users SET deleted_at = now(), email = 'deleted-' || id::text || '@invalid', first_name = 'Deleted', last_name = 'user', password_hash = '' WHERE id = $1")
        .bind(user.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(OkOut { ok: true }))
}
