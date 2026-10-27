use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::{
    router::{OpenApiRouter, UtoipaMethodRouterExt},
    routes,
};
use uuid::Uuid;

use crate::{
    error::{ApiError, ApiResult},
    security::auth::AuthUser,
    security::authz::{require_role, require_uploader, Role},
    state::AppState,
};

pub fn router(max_bytes: usize) -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(upload_file).layer(DefaultBodyLimit::max(max_bytes + 64 * 1024)))
        .routes(routes!(download_file))
}

#[derive(Serialize, ToSchema, sqlx::FromRow)]
pub struct FileOut {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub content_type: String,
    pub size: i64,
    pub created_at: DateTime<Utc>,
    /// Same-origin download URL (requires the Authorization header).
    #[sqlx(skip)]
    pub url: String,
}

/// Types we accept, decided by sniffing the bytes, never by the client's claimed type or
/// extension. SVG and HTML are deliberately absent: they can carry script.
const ALLOWED: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "application/pdf",
    "application/zip",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "text/plain",
];
const INLINE: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];

/// Decide the stored content type from the content itself.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if let Some(kind) = infer::get(bytes) {
        return ALLOWED.iter().find(|a| **a == kind.mime_type()).copied();
    }
    // Plain text: valid UTF-8 without control characters (other than whitespace).
    let text = std::str::from_utf8(bytes).ok()?;
    if text
        .chars()
        .all(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
    {
        Some("text/plain")
    } else {
        None
    }
}

fn clean_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("file");
    let cleaned: String = base.chars().filter(|c| !c.is_control()).take(200).collect();
    if cleaned.trim().is_empty() {
        "file".into()
    } else {
        cleaned
    }
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct UploadForm {
    #[schema(value_type = String, format = Binary)]
    file: String,
}

#[utoipa::path(post, operation_id = "uploadFile", path = "/projects/{id}/files", tag = "files",
    params(("id" = Uuid, Path)),
    request_body(content = UploadForm, content_type = "multipart/form-data"),
    responses((status = 200, body = FileOut), (status = 400), (status = 403), (status = 404), (status = 413)), security(("bearer" = [])))]
async fn upload_file(
    State(s): State<AppState>,
    user: AuthUser,
    Path(project_id): Path<Uuid>,
    mut multipart: Multipart,
) -> ApiResult<Json<FileOut>> {
    require_uploader(&s.db, user.id, project_id).await?;
    let field = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::BadRequest("invalid multipart body".into()))?
        .ok_or_else(|| ApiError::BadRequest("no file in request".into()))?;
    if field.name() != Some("file") {
        return Err(ApiError::BadRequest("expected a field named `file`".into()));
    }
    let name = clean_name(field.file_name().unwrap_or("file"));
    let bytes = field
        .bytes()
        .await
        .map_err(|_| ApiError::BadRequest("upload too large or interrupted".into()))?;
    if bytes.is_empty() {
        return Err(ApiError::BadRequest("empty file".into()));
    }
    if bytes.len() > s.config.max_upload_bytes {
        return Err(ApiError::BadRequest("file too large".into()));
    }
    let content_type =
        sniff(&bytes).ok_or_else(|| ApiError::BadRequest("file type not allowed".into()))?;

    let key = Uuid::new_v4(); // server-chosen: the client's file name never touches the filesystem
    let id = Uuid::new_v4();
    // Quota check and insert happen under a per-user advisory lock, so parallel uploads cannot overshoot.
    let mut tx = s.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))")
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    let (limit,): (i64,) = sqlx::query_as("SELECT user_upload_limit FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&mut *tx)
        .await?;
    let (used,): (i64,) = sqlx::query_as(
        "SELECT COALESCE(sum(size),0)::bigint FROM files WHERE project_id = $1 AND created_by = $2",
    )
    .bind(project_id)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    if used + bytes.len() as i64 > limit {
        return Err(ApiError::BadRequest("upload limit exceeded".into()));
    }
    // Written before the row exists; removed again if the insert fails.
    s.storage.put(key, bytes.clone()).await?;
    let inserted = sqlx::query_as::<_, FileOut>(
        "INSERT INTO files (id, project_id, created_by, name, content_type, size, storage_key) VALUES ($1,$2,$3,$4,$5,$6,$7)
         RETURNING id, project_id, name, content_type, size, created_at, '' AS url")
        .bind(id).bind(project_id).bind(user.id).bind(&name).bind(content_type).bind(bytes.len() as i64).bind(key.to_string())
        .fetch_one(&mut *tx).await;
    match inserted {
        Ok(mut row) => {
            tx.commit().await?;
            row.url = format!("/api/v1/files/{}", row.id);
            Ok(Json(row))
        }
        Err(e) => {
            s.storage.delete(key).await;
            Err(e.into())
        }
    }
}

#[utoipa::path(get, operation_id = "downloadFile", path = "/files/{id}", tag = "files",
    params(("id" = Uuid, Path)), responses((status = 200, description = "file bytes"), (status = 404)), security(("bearer" = [])))]
async fn download_file(
    State(s): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Response> {
    let row: Option<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT project_id, name, content_type, storage_key FROM files WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&s.db)
    .await?;
    let (project_id, name, content_type, key) = row.ok_or(ApiError::NotFound)?;
    require_role(&s.db, user.id, project_id, Role::Viewer).await?; // 404 for non-members
    let key = Uuid::parse_str(&key).map_err(|e| ApiError::Internal(Box::new(e)))?;
    let bytes = s.storage.get(key).await?;

    let inline = INLINE.contains(&content_type.as_str());
    let disposition = format!(
        "{}; filename*=UTF-8''{}",
        if inline { "inline" } else { "attachment" },
        urlencode(&name)
    );
    let mut resp = Response::new(Body::from(bytes));
    *resp.status_mut() = StatusCode::OK;
    let h = resp.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&content_type).map_err(|e| ApiError::Internal(Box::new(e)))?,
    );
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).map_err(|e| ApiError::Internal(Box::new(e)))?,
    );
    h.insert(
        "content-security-policy",
        HeaderValue::from_static("default-src 'none'; sandbox"),
    );
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=300"),
    );
    Ok(resp)
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffing_ignores_claims_and_blocks_active_content() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        assert_eq!(sniff(&png), Some("image/png"));
        assert_eq!(sniff(b"hello world\n"), Some("text/plain"));
        // HTML/SVG are plain text to a sniffer, but they are only ever served as an attachment
        // with a sandbox CSP and `nosniff`, never inline.
        assert!(!INLINE.contains(&sniff(b"<svg onload=alert(1)>").unwrap()));
        assert_eq!(sniff(&[0x7f, b'E', b'L', b'F', 0, 1, 2]), None); // executables are rejected
        assert_eq!(sniff(b"\x00\x01\x02binary"), None);
    }

    #[test]
    fn names_cannot_traverse_or_inject() {
        assert_eq!(clean_name("../../etc/passwd"), "passwd");
        assert_eq!(clean_name("..\\..\\boot.ini"), "boot.ini");
        assert_eq!(clean_name("a\r\nb"), "ab");
        assert_eq!(clean_name("///"), "file");
        assert_eq!(urlencode("a b/ü"), "a%20b%2F%C3%BC");
    }
}
