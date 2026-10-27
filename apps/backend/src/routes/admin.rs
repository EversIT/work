//! The instance administration: what the instance looks like (branding), and an overview of
//! everything that lives on it. Only accounts flagged `is_instance_admin` get in, and only with a
//! real session (never with an API token). Every change an admin makes is written to an audit log.
//!
//! Admins see accounts, projects and counts; they do not read project content (cards, comments,
//! files) through these endpoints, and no endpoint here returns a secret.

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::ToSchema;
use utoipa_axum::{
    router::{OpenApiRouter, UtoipaMethodRouterExt},
    routes,
};
use uuid::Uuid;

use super::OkOut;
use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthUser,
    state::AppState,
};

const MAX_FAVICON: usize = 256 * 1024;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_branding))
        .routes(routes!(get_favicon))
        .routes(routes!(update_branding))
        .routes(routes!(put_favicon).layer(DefaultBodyLimit::max(MAX_FAVICON + 1024)))
        .routes(routes!(delete_favicon))
        .routes(routes!(overview))
        .routes(routes!(list_users))
        .routes(routes!(update_user))
        .routes(routes!(list_projects))
        .routes(routes!(audit_log))
}

/// Admin rights are read from the database on every call, never from the token.
async fn require_admin(s: &AppState, user: &AuthUser) -> ApiResult<()> {
    user.require_session()?;
    let ok: Option<(bool,)> = sqlx::query_as("SELECT is_instance_admin FROM users WHERE id = $1 AND deleted_at IS NULL AND disabled_at IS NULL")
        .bind(user.id)
        .fetch_optional(&s.db)
        .await?;
    match ok {
        Some((true,)) => Ok(()),
        _ => Err(ApiError::NotFound), // the admin area does not announce itself
    }
}

async fn audit(
    s: &AppState,
    actor: Uuid,
    action: &str,
    target: Option<String>,
    detail: Value,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO admin_audit (actor, action, target, detail) VALUES ($1,$2,$3,$4)")
        .bind(actor)
        .bind(action)
        .bind(target)
        .bind(detail)
        .execute(&s.db)
        .await?;
    Ok(())
}

// ---- branding ---------------------------------------------------------------------------------

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct Branding {
    pub title: String,
    pub description: String,
    /// `#RRGGBB`; the interface derives its accent colour from it.
    pub accent: String,
    pub has_favicon: bool,
    /// Changes whenever the branding does, so caches can be bypassed.
    pub version: i64,
}

/// What every visitor sees: the instance's name, description, accent colour and whether it has its
/// own favicon. Public on purpose (the sign-in page needs it).
#[utoipa::path(get, operation_id = "getBranding", path = "/branding", tag = "admin",
    responses((status = 200, body = Branding)))]
async fn get_branding(State(s): State<AppState>) -> ApiResult<Response> {
    let row: Branding = sqlx::query_as(
        "SELECT title, description, accent, favicon IS NOT NULL AS has_favicon, (extract(epoch FROM updated_at) * 1000)::bigint AS version FROM branding",
    )
    .fetch_one(&s.db)
    .await?;
    Ok((
        [(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"))],
        Json(row),
    )
        .into_response())
}

#[utoipa::path(get, operation_id = "getFavicon", path = "/branding/favicon", tag = "admin",
    responses((status = 200, description = "the favicon bytes"), (status = 404)))]
async fn get_favicon(State(s): State<AppState>) -> ApiResult<Response> {
    let row: Option<(Option<Vec<u8>>, Option<String>)> =
        sqlx::query_as("SELECT favicon, favicon_type FROM branding")
            .fetch_optional(&s.db)
            .await?;
    let (Some(bytes), Some(ty)) = row.map(|r| (r.0, r.1)).unwrap_or((None, None)) else {
        return Err(ApiError::NotFound);
    };
    let mut resp = Response::new(axum::body::Body::from(bytes));
    let h = resp.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&ty).map_err(|e| ApiError::Internal(Box::new(e)))?,
    );
    // An uploaded SVG is only ever shown as an image; if somebody opens it directly it is sandboxed.
    h.insert(
        "content-security-policy",
        HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'; sandbox"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    Ok(resp)
}

#[derive(Deserialize, ToSchema)]
pub struct BrandingPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub accent: Option<String>,
}

fn valid_accent(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
}

#[utoipa::path(patch, operation_id = "updateBranding", path = "/admin/branding", tag = "admin",
    request_body = BrandingPatch, responses((status = 200, body = Branding), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn update_branding(
    State(s): State<AppState>,
    user: AuthUser,
    Json(b): Json<BrandingPatch>,
) -> ApiResult<Json<Branding>> {
    require_admin(&s, &user).await?;
    if let Some(t) = &b.title {
        if t.trim().is_empty() || t.chars().count() > 60 || t.chars().any(char::is_control) {
            return Err(ApiError::BadRequest(
                "the title must be 1-60 characters".into(),
            ));
        }
    }
    if let Some(d) = &b.description {
        if d.chars().count() > 300 || d.chars().any(|c| c.is_control() && c != '\n') {
            return Err(ApiError::BadRequest(
                "the description must be at most 300 characters".into(),
            ));
        }
    }
    if b.accent.as_deref().is_some_and(|a| !valid_accent(a)) {
        return Err(ApiError::BadRequest(
            "the accent colour must look like #0C66E4".into(),
        ));
    }
    sqlx::query("UPDATE branding SET title = COALESCE($1,title), description = COALESCE($2,description), accent = COALESCE($3,accent), updated_at = now(), updated_by = $4")
        .bind(b.title.as_deref().map(str::trim))
        .bind(b.description.as_deref().map(str::trim))
        .bind(b.accent.as_deref().map(str::to_uppercase))
        .bind(user.id)
        .execute(&s.db)
        .await?;
    audit(
        &s,
        user.id,
        "branding.update",
        None,
        json!({"title": b.title, "description": b.description, "accent": b.accent}),
    )
    .await?;
    let row = sqlx::query_as("SELECT title, description, accent, favicon IS NOT NULL AS has_favicon, (extract(epoch FROM updated_at) * 1000)::bigint AS version FROM branding")
        .fetch_one(&s.db)
        .await?;
    Ok(Json(row))
}

/// What an uploaded favicon really is, from its bytes. SVG is accepted only without anything active.
pub fn favicon_type(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if b.len() > 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if b.len() > 6 && b[..4] == [0, 0, 1, 0] && u16::from_le_bytes([b[4], b[5]]) > 0 {
        return Some("image/x-icon");
    }
    let text = std::str::from_utf8(b).ok()?;
    let lower = text.to_ascii_lowercase();
    let head = lower.trim_start();
    if !(head.starts_with("<svg") || head.starts_with("<?xml")) || !lower.contains("<svg") {
        return None;
    }
    // Nothing that runs, loads, or embeds: scripts, handlers, foreign content, external references.
    let forbidden = [
        "<script",
        "<foreignobject",
        "<!entity",
        "<!doctype",
        "javascript:",
        "data:text",
        "<iframe",
        "<object",
        "<embed",
        "<use",
        "<image",
        "xlink:href=\"http",
        "href=\"http",
        "href='http",
        "@import",
        "url(http",
    ];
    if forbidden.iter().any(|f| lower.contains(f)) || has_event_handler(&lower) {
        return None;
    }
    Some("image/svg+xml")
}

fn has_event_handler(lower: &str) -> bool {
    // ` onload=`, ` onclick =` ... any attribute that starts with "on" followed by letters and `=`.
    let bytes = lower.as_bytes();
    let mut i = 0;
    while let Some(pos) = lower[i..].find(" on") {
        let start = i + pos + 3;
        let rest = &bytes[start..];
        let letters = rest.iter().take_while(|c| c.is_ascii_alphabetic()).count();
        if letters > 0 && rest[letters..].iter().find(|c| !c.is_ascii_whitespace()) == Some(&b'=') {
            return true;
        }
        i = start;
    }
    false
}

#[utoipa::path(put, operation_id = "putFavicon", path = "/admin/branding/favicon", tag = "admin",
    request_body(content = String, content_type = "application/octet-stream", description = "PNG, JPEG, WebP, ICO or a plain SVG, at most 256 KB"),
    responses((status = 200, body = Branding), (status = 400), (status = 404)), security(("bearer" = [])))]
async fn put_favicon(
    State(s): State<AppState>,
    user: AuthUser,
    body: Bytes,
) -> ApiResult<Json<Branding>> {
    require_admin(&s, &user).await?;
    if body.is_empty() || body.len() > MAX_FAVICON {
        return Err(ApiError::BadRequest(
            "the favicon must be between 1 byte and 256 KB".into(),
        ));
    }
    let ty = favicon_type(&body)
        .ok_or_else(|| ApiError::BadRequest("use a PNG, JPEG, WebP, ICO or a plain SVG".into()))?;
    sqlx::query(
        "UPDATE branding SET favicon = $1, favicon_type = $2, updated_at = now(), updated_by = $3",
    )
    .bind(body.to_vec())
    .bind(ty)
    .bind(user.id)
    .execute(&s.db)
    .await?;
    audit(
        &s,
        user.id,
        "branding.favicon",
        None,
        json!({"type": ty, "bytes": body.len()}),
    )
    .await?;
    let row = sqlx::query_as("SELECT title, description, accent, favicon IS NOT NULL AS has_favicon, (extract(epoch FROM updated_at) * 1000)::bigint AS version FROM branding")
        .fetch_one(&s.db)
        .await?;
    Ok(Json(row))
}

#[utoipa::path(delete, operation_id = "deleteFavicon", path = "/admin/branding/favicon", tag = "admin",
    responses((status = 200, body = Branding), (status = 404)), security(("bearer" = [])))]
async fn delete_favicon(State(s): State<AppState>, user: AuthUser) -> ApiResult<Json<Branding>> {
    require_admin(&s, &user).await?;
    sqlx::query("UPDATE branding SET favicon = NULL, favicon_type = NULL, updated_at = now(), updated_by = $1")
        .bind(user.id)
        .execute(&s.db)
        .await?;
    audit(&s, user.id, "branding.favicon.reset", None, json!({})).await?;
    let row = sqlx::query_as("SELECT title, description, accent, favicon IS NOT NULL AS has_favicon, (extract(epoch FROM updated_at) * 1000)::bigint AS version FROM branding")
        .fetch_one(&s.db)
        .await?;
    Ok(Json(row))
}

// ---- overview -----------------------------------------------------------------------------------

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct Counts {
    pub users: i64,
    pub users_disabled: i64,
    pub admins: i64,
    pub users_with_mfa: i64,
    pub users_with_passkeys: i64,
    pub projects: i64,
    pub workspaces: i64,
    pub lists: i64,
    pub cards: i64,
    pub comments: i64,
    pub files: i64,
    pub file_bytes: i64,
    pub automations_enabled: i64,
    pub active_sessions: i64,
    pub api_tokens: i64,
}

#[derive(Serialize, ToSchema)]
pub struct ConfigSummary {
    pub version: String,
    pub registration_enabled: bool,
    pub email_enabled: bool,
    pub email_verification_required: bool,
    pub sso_providers: Vec<String>,
    pub mfa_available: bool,
    pub storage: String,
    pub scheduler: bool,
    pub shared_rate_limit: bool,
}

#[derive(Serialize, ToSchema)]
pub struct Overview {
    pub counts: Counts,
    /// Settings that matter for running the instance. Never contains a secret.
    pub config: ConfigSummary,
}

#[utoipa::path(get, operation_id = "adminOverview", path = "/admin/overview", tag = "admin",
    responses((status = 200, body = Overview), (status = 404)), security(("bearer" = [])))]
async fn overview(State(s): State<AppState>, user: AuthUser) -> ApiResult<Json<Overview>> {
    require_admin(&s, &user).await?;
    let counts: Counts = sqlx::query_as(
        "SELECT
           (SELECT count(*) FROM users WHERE deleted_at IS NULL) AS users,
           (SELECT count(*) FROM users WHERE deleted_at IS NULL AND disabled_at IS NOT NULL) AS users_disabled,
           (SELECT count(*) FROM users WHERE deleted_at IS NULL AND is_instance_admin) AS admins,
           (SELECT count(*) FROM users WHERE deleted_at IS NULL AND mfa_enabled_at IS NOT NULL) AS users_with_mfa,
           (SELECT count(DISTINCT user_id) FROM passkeys) AS users_with_passkeys,
           (SELECT count(*) FROM projects) AS projects,
           (SELECT count(*) FROM workspaces) AS workspaces,
           (SELECT count(*) FROM lists WHERE deleted_at IS NULL) AS lists,
           (SELECT count(*) FROM cards WHERE deleted_at IS NULL) AS cards,
           (SELECT count(*) FROM card_activities WHERE type = 'comment') AS comments,
           (SELECT count(*) FROM files) AS files,
           (SELECT COALESCE(sum(size), 0)::bigint FROM files) AS file_bytes,
           (SELECT count(*) FROM automations WHERE is_enabled AND deleted_at IS NULL) AS automations_enabled,
           (SELECT count(*) FROM sessions WHERE revoked_at IS NULL AND expires_at > now()) AS active_sessions,
           (SELECT count(*) FROM api_tokens WHERE revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())) AS api_tokens",
    )
    .fetch_one(&s.db)
    .await?;
    let c = &s.config;
    Ok(Json(Overview {
        counts,
        config: ConfigSummary {
            version: env!("CARGO_PKG_VERSION").into(),
            registration_enabled: c.registration_enabled,
            email_enabled: c.smtp.is_some(),
            email_verification_required: c.require_email_verification,
            sso_providers: c.sso.names(),
            mfa_available: c.encryption_key.is_some(),
            storage: match c.storage {
                crate::config::StorageConfig::Local => "local",
                crate::config::StorageConfig::S3(_) => "s3",
                crate::config::StorageConfig::Azure(_) => "azure",
            }
            .into(),
            scheduler: c.scheduler,
            shared_rate_limit: c.shared_rate_limit,
        },
    }))
}

// ---- users ------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct UserQuery {
    query: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct AdminUser {
    pub id: Uuid,
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub created_at: DateTime<Utc>,
    pub is_instance_admin: bool,
    pub disabled: bool,
    pub locked: bool,
    pub mfa_enabled: bool,
    pub passkeys: i64,
    pub has_password: bool,
    pub sso: Vec<String>,
    pub projects: i64,
    pub last_active_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, ToSchema)]
pub struct UserPage {
    pub total: i64,
    pub users: Vec<AdminUser>,
}

#[utoipa::path(get, operation_id = "adminListUsers", path = "/admin/users", tag = "admin",
    params(("query" = Option<String>, Query), ("limit" = Option<i64>, Query), ("offset" = Option<i64>, Query)),
    responses((status = 200, body = UserPage), (status = 404)), security(("bearer" = [])))]
async fn list_users(
    State(s): State<AppState>,
    user: AuthUser,
    Query(q): Query<UserQuery>,
) -> ApiResult<Json<UserPage>> {
    require_admin(&s, &user).await?;
    let pattern = q
        .query
        .as_deref()
        .map(|t| {
            t.trim()
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        })
        .filter(|t| !t.is_empty() && t.chars().count() <= 100)
        .map(|t| format!("%{t}%"));
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0).max(0);
    let filter = "u.deleted_at IS NULL AND ($1::text IS NULL OR u.email::text ILIKE $1 ESCAPE '\\' OR (u.first_name || ' ' || u.last_name) ILIKE $1 ESCAPE '\\')";
    let (total,): (i64,) = sqlx::query_as(&format!("SELECT count(*) FROM users u WHERE {filter}"))
        .bind(&pattern)
        .fetch_one(&s.db)
        .await?;
    let users = sqlx::query_as(&format!(
        "SELECT u.id, u.email::text AS email, u.first_name, u.last_name, u.created_at, u.is_instance_admin,
                u.disabled_at IS NOT NULL AS disabled, COALESCE(u.locked_until > now(), false) AS locked,
                u.mfa_enabled_at IS NOT NULL AS mfa_enabled,
                (SELECT count(*) FROM passkeys p WHERE p.user_id = u.id) AS passkeys,
                u.password_hash <> '' AS has_password,
                COALESCE((SELECT array_agg(i.provider ORDER BY i.provider) FROM sso_identities i WHERE i.user_id = u.id), '{{}}') AS sso,
                (SELECT count(*) FROM project_members m WHERE m.user_id = u.id) AS projects,
                (SELECT max(created_at) FROM sessions x WHERE x.user_id = u.id) AS last_active_at
           FROM users u WHERE {filter} ORDER BY u.created_at, u.id LIMIT $2 OFFSET $3"
    ))
    .bind(&pattern)
    .bind(limit)
    .bind(offset)
    .fetch_all(&s.db)
    .await?;
    Ok(Json(UserPage { total, users }))
}

#[derive(Deserialize, ToSchema)]
pub struct UserPatch {
    pub is_instance_admin: Option<bool>,
    /// Blocks sign-in everywhere (sessions end at once) or lifts the block.
    pub disabled: Option<bool>,
    /// Clears a lock-out after failed sign-ins.
    pub unlock: Option<bool>,
}

#[utoipa::path(patch, operation_id = "adminUpdateUser", path = "/admin/users/{id}", tag = "admin",
    params(("id" = Uuid, Path)), request_body = UserPatch,
    responses((status = 200, body = OkOut), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn update_user(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(b): Json<UserPatch>,
) -> ApiResult<Json<OkOut>> {
    require_admin(&s, &user).await?;
    // Nobody locks themselves out of the admin area, and the instance always keeps an administrator.
    if id == user.id && (b.disabled == Some(true) || b.is_instance_admin == Some(false)) {
        return Err(ApiError::Conflict);
    }
    let mut tx = s.db.begin().await?;
    let exists: Option<(bool,)> = sqlx::query_as(
        "SELECT is_instance_admin FROM users WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (was_admin,) = exists.ok_or(ApiError::NotFound)?;
    if was_admin && (b.is_instance_admin == Some(false) || b.disabled == Some(true)) {
        let (others,): (i64,) = sqlx::query_as("SELECT count(*) FROM users WHERE is_instance_admin AND disabled_at IS NULL AND deleted_at IS NULL AND id <> $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if others == 0 {
            return Err(ApiError::Conflict);
        }
    }
    if let Some(a) = b.is_instance_admin {
        sqlx::query("UPDATE users SET is_instance_admin = $2 WHERE id = $1")
            .bind(id)
            .bind(a)
            .execute(&mut *tx)
            .await?;
    }
    match b.disabled {
        Some(true) => {
            sqlx::query(
                "UPDATE users SET disabled_at = now() WHERE id = $1 AND disabled_at IS NULL",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE sessions SET revoked_at = now() WHERE user_id = $1 AND revoked_at IS NULL",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        Some(false) => {
            sqlx::query("UPDATE users SET disabled_at = NULL WHERE id = $1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        None => {}
    }
    if b.unlock == Some(true) {
        sqlx::query("UPDATE users SET failed_logins = 0, locked_until = NULL WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    audit(&s, user.id, "user.update", Some(id.to_string()), json!({"is_instance_admin": b.is_instance_admin, "disabled": b.disabled, "unlock": b.unlock})).await?;
    Ok(Json(OkOut { ok: true }))
}

// ---- projects and audit ---------------------------------------------------------------------------

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct AdminProject {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub owner_email: Option<String>,
    pub members: i64,
    pub workspaces: i64,
    pub cards: i64,
    pub file_bytes: i64,
}

#[utoipa::path(get, operation_id = "adminListProjects", path = "/admin/projects", tag = "admin",
    responses((status = 200, body = Vec<AdminProject>), (status = 404)), security(("bearer" = [])))]
async fn list_projects(
    State(s): State<AppState>,
    user: AuthUser,
) -> ApiResult<Json<Vec<AdminProject>>> {
    require_admin(&s, &user).await?;
    Ok(Json(
        sqlx::query_as(
            "SELECT p.id, p.name, p.created_at,
                    (SELECT u.email::text FROM project_members m JOIN users u ON u.id = m.user_id WHERE m.project_id = p.id AND m.role = 'owner' ORDER BY m.created_at LIMIT 1) AS owner_email,
                    (SELECT count(*) FROM project_members m WHERE m.project_id = p.id) AS members,
                    (SELECT count(*) FROM workspaces w WHERE w.project_id = p.id) AS workspaces,
                    (SELECT count(*) FROM cards c JOIN workspaces w ON w.id = c.workspace_id WHERE w.project_id = p.id AND c.deleted_at IS NULL) AS cards,
                    (SELECT COALESCE(sum(f.size), 0)::bigint FROM files f WHERE f.project_id = p.id) AS file_bytes
               FROM projects p ORDER BY p.created_at LIMIT 500",
        )
        .fetch_all(&s.db)
        .await?,
    ))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: i64,
    pub at: DateTime<Utc>,
    pub actor_email: Option<String>,
    pub action: String,
    pub target: Option<String>,
    #[schema(value_type = Object)]
    pub detail: sqlx::types::Json<Value>,
}

#[utoipa::path(get, operation_id = "adminAuditLog", path = "/admin/audit", tag = "admin",
    responses((status = 200, body = Vec<AuditEntry>), (status = 404)), security(("bearer" = [])))]
async fn audit_log(State(s): State<AppState>, user: AuthUser) -> ApiResult<Json<Vec<AuditEntry>>> {
    require_admin(&s, &user).await?;
    Ok(Json(
        sqlx::query_as(
            "SELECT a.id, a.at, u.email::text AS actor_email, a.action, a.target, a.detail
               FROM admin_audit a LEFT JOIN users u ON u.id = a.actor ORDER BY a.at DESC, a.id DESC LIMIT 200",
        )
        .fetch_all(&s.db)
        .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favicons_are_recognised_by_content() {
        assert_eq!(
            favicon_type(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]),
            Some("image/png")
        );
        assert_eq!(favicon_type(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(
            favicon_type(b"RIFF\x00\x00\x00\x00WEBPVP8 "),
            Some("image/webp")
        );
        assert_eq!(
            favicon_type(&[0, 0, 1, 0, 1, 0, 16, 16]),
            Some("image/x-icon")
        );
        assert_eq!(
            favicon_type(
                b"<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"4\" height=\"4\"/></svg>"
            ),
            Some("image/svg+xml")
        );
        assert_eq!(
            favicon_type(
                b"<?xml version=\"1.0\"?><svg viewBox=\"0 0 1 1\"><path d=\"M0 0\"/></svg>"
            ),
            Some("image/svg+xml")
        );
        for bad in [
            &b"MZ\x90\x00 an executable"[..],
            b"<html><body>hi</body></html>",
            b"GIF89a",
            b"",
        ] {
            assert_eq!(favicon_type(bad), None);
        }
    }

    #[test]
    fn active_svg_is_refused() {
        let wrap = |inner: &str| {
            format!("<svg xmlns=\"http://www.w3.org/2000/svg\">{inner}</svg>").into_bytes()
        };
        for inner in [
            "<script>alert(1)</script>",
            "<rect onload=\"alert(1)\"/>",
            "<rect ONCLICK = 'x'/>",
            "<foreignObject><div/></foreignObject>",
            "<a href=\"javascript:alert(1)\"><rect/></a>",
            "<image href=\"https://evil.test/x.png\"/>",
            "<use href=\"https://evil.test/x.svg#a\"/>",
            "<style>@import url(https://evil.test/x.css);</style>",
        ] {
            assert_eq!(favicon_type(&wrap(inner)), None, "{inner}");
        }
        assert!(favicon_type(b"<svg><!DOCTYPE x [<!ENTITY a 'b'>]></svg>").is_none());
        // Words that merely contain "on" are not event handlers.
        assert_eq!(
            favicon_type(&wrap(
                "<path d=\"M0 0\" fill=\"none\" stroke-linecap=\"round\"/>"
            )),
            Some("image/svg+xml")
        );
    }

    #[test]
    fn accent_colours_are_strict() {
        assert!(valid_accent("#0C66E4") && valid_accent("#abcdef"));
        for bad in [
            "0C66E4",
            "#0C66E",
            "#0C66E4A",
            "#GGGGGG",
            "red",
            "url(x)",
            "#0C66E4;x",
        ] {
            assert!(!valid_accent(bad), "{bad}");
        }
    }
}
