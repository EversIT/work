//! Real-time collaborative editing of rich-text fields (Yjs).
//!
//! Clients edit a Yjs document and exchange updates with the server over plain HTTPS: a `sync`
//! request pushes local updates and pulls what the client lacks, and a server-sent event stream
//! delivers the updates of the other editors. There is no WebSocket, so it works through the
//! frontend's proxy like the rest of the API.
//!
//! * The server keeps the merged document (`collab_docs`), applies every update with `yrs`, and
//!   broadcasts the accepted update. Updates are applied on a blocking thread: a malformed update
//!   can fail or even panic inside the library without touching the server.
//! * Who may do what is decided per request from the card: viewers can read and follow, members can
//!   write. Only `rich` fields of the card's own type can be edited collaboratively.
//! * The first editor seeds the document from the card's current content; a second seed is ignored,
//!   so the content never doubles. The card's data remains the source of truth: editors save the
//!   document's JSON through the normal card update (see `keep_collab`), and any other write to the
//!   field discards the session document and tells editors to restart from the new content.
//! * Rooms are in-process (like notifications), so collaborators of one card should reach the same
//!   instance; the saved content is shared through the database either way.

use std::{convert::Infallible, time::Duration};

use axum::{
    extract::{Path, State},
    response::sse::{Event as SseEvent, KeepAlive, Sse},
    Json,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_stream::wrappers::BroadcastStream;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;
use yrs::{
    updates::{decoder::Decode, encoder::Encode},
    Doc, ReadTxn, StateVector, Transact, Update,
};

use crate::{
    error::{ApiError, ApiResult},
    routes::card_types::fields_for_card_type,
    security::auth::AuthUser,
    security::authz::{require_card_role, Role},
    state::AppState,
};

const MAX_UPDATE: usize = 256 * 1024;
const MAX_STATE: usize = 2 * 1024 * 1024;
/// Streams end after this long, so a revoked member is cut off at the next reconnect.
const STREAM_LIFETIME: Duration = Duration::from_secs(10 * 60);

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(sync))
        .routes(routes!(events))
}

/// The hub is keyed by user id; a room uses a key derived from card and field that cannot be a user id.
pub(crate) fn room(card: Uuid, slug: &str) -> Uuid {
    let digest = Sha256::digest(format!("collab:{card}:{slug}").as_bytes());
    Uuid::from_slice(&digest[..16]).expect("16 bytes")
}

fn b64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

fn unb64(s: &str, max: usize) -> ApiResult<Vec<u8>> {
    if s.len() > max * 2 {
        return Err(ApiError::BadRequest("update too large".into()));
    }
    STANDARD
        .decode(s)
        .ok()
        .filter(|b| b.len() <= max)
        .ok_or_else(|| ApiError::BadRequest("invalid base64".into()))
}

/// Applies `update` (if any) to the document stored in `state` and returns the new full state.
fn merge(state: Option<&[u8]>, update: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let doc = Doc::new();
    {
        let mut txn = doc.transact_mut();
        for part in [state, update].into_iter().flatten() {
            let u = Update::decode_v1(part).map_err(|e| e.to_string())?;
            txn.apply_update(u).map_err(|e| e.to_string())?;
        }
    }
    let txn = doc.transact();
    Ok(txn.encode_state_as_update_v1(&StateVector::default()))
}

/// What a client with `client_sv` lacks, plus the server's own state vector.
fn diff(state: &[u8], client_sv: Option<&[u8]>) -> Result<(Vec<u8>, Vec<u8>), String> {
    let sv = match client_sv {
        Some(b) if !b.is_empty() => StateVector::decode_v1(b).map_err(|e| e.to_string())?,
        _ => StateVector::default(),
    };
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    txn.apply_update(Update::decode_v1(state).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok((
        txn.encode_state_as_update_v1(&sv),
        txn.state_vector().encode_v1(),
    ))
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> ApiResult<T> {
    match tokio::task::spawn_blocking(f).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(_)) => Err(ApiError::BadRequest("invalid document update".into())),
        Err(_) => Err(ApiError::BadRequest("invalid document update".into())), // the library panicked
    }
}

/// The rich field `slug` of this card's type, or 404.
async fn rich_field(s: &AppState, card: Uuid, slug: &str, workspace: Uuid) -> ApiResult<()> {
    let row: Option<(Uuid,)> =
        sqlx::query_as("SELECT card_type_id FROM cards WHERE id = $1 AND deleted_at IS NULL")
            .bind(card)
            .fetch_optional(&s.db)
            .await?;
    let (ct,) = row.ok_or(ApiError::NotFound)?;
    let fields = fields_for_card_type(&s.db, workspace, ct).await?;
    if fields.iter().any(|f| f.slug == slug && f.r#type == "rich") {
        Ok(())
    } else {
        Err(ApiError::NotFound)
    }
}

#[derive(Deserialize, ToSchema)]
pub struct SyncBody {
    /// Base64 state vector of the client's document, so the answer only holds what it lacks.
    pub state_vector: Option<String>,
    /// Base64 Yjs update with local changes (needs write access).
    pub update: Option<String>,
    /// Base64 initial document, used only if no session document exists yet (needs write access).
    pub seed: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct SyncOut {
    /// No session document exists yet and none was seeded: the caller should seed it.
    pub empty: bool,
    /// Base64 update with everything the client lacks.
    pub update: String,
    /// Base64 state vector of the server's document.
    pub state_vector: String,
}

/// Pushes local changes and pulls the ones the client lacks.
#[utoipa::path(post, operation_id = "syncCollab", path = "/cards/{id}/collab/{slug}/sync", tag = "cards",
    params(("id" = Uuid, Path), ("slug" = String, Path)), request_body = SyncBody,
    responses((status = 200, body = SyncOut), (status = 400), (status = 403), (status = 404), (status = 409)), security(("bearer" = [])))]
async fn sync(
    State(s): State<AppState>,
    user: AuthUser,
    Path((card, slug)): Path<(Uuid, String)>,
    Json(b): Json<SyncBody>,
) -> ApiResult<Json<SyncOut>> {
    let writes = b.update.is_some() || b.seed.is_some();
    let scope = require_card_role(
        &s.db,
        user.id,
        card,
        if writes { Role::Member } else { Role::Viewer },
    )
    .await?;
    rich_field(&s, card, &slug, scope.workspace_id).await?;
    let client_sv = b
        .state_vector
        .as_deref()
        .map(|v| unb64(v, MAX_UPDATE))
        .transpose()?;

    let mut tx = s.db.begin().await?;
    let mut state: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT state FROM collab_docs WHERE card_id = $1 AND slug = $2 FOR UPDATE",
    )
    .bind(card)
    .bind(&slug)
    .fetch_optional(&mut *tx)
    .await?;

    if state.is_none() {
        if let Some(seed) = &b.seed {
            let seed = unb64(seed, MAX_UPDATE)?;
            let merged = blocking(move || merge(None, Some(&seed))).await?;
            // A concurrent seed wins by being first; ours is then simply ignored.
            sqlx::query("INSERT INTO collab_docs (card_id, slug, state) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
                .bind(card)
                .bind(&slug)
                .bind(&merged)
                .execute(&mut *tx)
                .await?;
            state = sqlx::query_scalar(
                "SELECT state FROM collab_docs WHERE card_id = $1 AND slug = $2",
            )
            .bind(card)
            .bind(&slug)
            .fetch_optional(&mut *tx)
            .await?;
        }
    }

    if let Some(update) = &b.update {
        let Some(current) = state.clone() else {
            return Err(ApiError::Conflict); // nothing to apply it to: seed first
        };
        let raw = unb64(update, MAX_UPDATE)?;
        let (apply_raw, current_copy) = (raw.clone(), current);
        let merged = blocking(move || merge(Some(&current_copy), Some(&apply_raw))).await?;
        if merged.len() > MAX_STATE {
            return Err(ApiError::BadRequest("document too large".into()));
        }
        sqlx::query("UPDATE collab_docs SET state = $3, updated_at = now() WHERE card_id = $1 AND slug = $2")
            .bind(card)
            .bind(&slug)
            .bind(&merged)
            .execute(&mut *tx)
            .await?;
        state = Some(merged);
        tx.commit().await?;
        s.hub.publish(
            room(card, &slug),
            serde_json::json!({"t": "update", "u": b64(&raw), "from": user.id}).to_string(),
        );
    } else {
        tx.commit().await?;
    }

    Ok(Json(match state {
        None => SyncOut {
            empty: true,
            update: String::new(),
            state_vector: String::new(),
        },
        Some(state) => {
            let (update, sv) = blocking(move || diff(&state, client_sv.as_deref())).await?;
            SyncOut {
                empty: false,
                update: b64(&update),
                state_vector: b64(&sv),
            }
        }
    }))
}

/// Updates of the other editors as server-sent events: `{"t":"update","u":<base64>,"from":<user>}`,
/// and `{"t":"reset"}` when the field was changed outside the editor.
#[utoipa::path(get, operation_id = "collabEvents", path = "/cards/{id}/collab/{slug}/events", tag = "cards",
    params(("id" = Uuid, Path), ("slug" = String, Path)),
    responses((status = 200, description = "text/event-stream"), (status = 404)), security(("bearer" = [])))]
async fn events(
    State(s): State<AppState>,
    user: AuthUser,
    Path((card, slug)): Path<(Uuid, String)>,
) -> ApiResult<impl axum::response::IntoResponse> {
    let scope = require_card_role(&s.db, user.id, card, Role::Viewer).await?;
    rich_field(&s, card, &slug, scope.workspace_id).await?;
    let rx = s.hub.subscribe(room(card, &slug));
    let stream = BroadcastStream::new(rx)
        .filter_map(|m| async move { m.ok() })
        .map(|m| Ok::<_, Infallible>(SseEvent::default().data(m)))
        .take_until(tokio::time::sleep(STREAM_LIFETIME));
    Ok((
        [
            (axum::http::header::CACHE_CONTROL, "no-store, no-transform"),
            (
                axum::http::HeaderName::from_static("x-accel-buffering"),
                "no",
            ),
        ],
        Sse::new(stream).keep_alive(KeepAlive::default()),
    ))
}

/// Called when a rich field was written through the API: the session document is stale.
pub async fn discard(s: &AppState, card: Uuid, slugs: &[String]) -> Result<(), sqlx::Error> {
    if slugs.is_empty() {
        return Ok(());
    }
    let gone = sqlx::query("DELETE FROM collab_docs WHERE card_id = $1 AND slug = ANY($2)")
        .bind(card)
        .bind(slugs)
        .execute(&s.db)
        .await?
        .rows_affected();
    if gone > 0 {
        for slug in slugs {
            s.hub.publish(
                room(card, slug),
                serde_json::json!({"t": "reset"}).to_string(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use yrs::{GetString, Text};

    fn doc_with(text: &str) -> Vec<u8> {
        let d = Doc::new();
        let t = d.get_or_insert_text("t");
        t.insert(&mut d.transact_mut(), 0, text);
        let txn = d.transact();
        txn.encode_state_as_update_v1(&StateVector::default())
    }

    fn text_of(state: &[u8]) -> String {
        let d = Doc::new();
        let t = d.get_or_insert_text("t");
        d.transact_mut()
            .apply_update(Update::decode_v1(state).unwrap())
            .unwrap();
        let txn = d.transact();
        t.get_string(&txn)
    }

    #[test]
    fn merges_concurrent_edits_without_losing_either() {
        let seed = doc_with("hello");
        let base = merge(None, Some(&seed)).unwrap();
        // Two clients start from the same state and edit at the same time.
        let edit = |suffix: &str| {
            let d = Doc::new();
            let t = d.get_or_insert_text("t");
            d.transact_mut()
                .apply_update(Update::decode_v1(&base).unwrap())
                .unwrap();
            let before = d.transact().state_vector();
            t.insert(&mut d.transact_mut(), 5, suffix);
            let txn = d.transact();
            txn.encode_state_as_update_v1(&before)
        };
        let (a, b) = (edit(" world"), edit("!!"));
        let one = merge(Some(&merge(Some(&base), Some(&a)).unwrap()), Some(&b)).unwrap();
        let two = merge(Some(&merge(Some(&base), Some(&b)).unwrap()), Some(&a)).unwrap();
        assert_eq!(text_of(&one), text_of(&two), "order does not matter");
        let merged = text_of(&one);
        assert!(
            merged.contains(" world") && merged.contains("!!") && merged.starts_with("hello"),
            "{merged}"
        );
    }

    #[test]
    fn a_diff_only_holds_what_the_client_lacks() {
        let full = doc_with("abcdef");
        let (everything, sv) = diff(&full, None).unwrap();
        assert_eq!(text_of(&everything), "abcdef");
        let (nothing, _) = diff(&full, Some(&sv)).unwrap();
        // The state vector says the client has it all: the update carries no new content.
        let probe = Doc::new();
        let t = probe.get_or_insert_text("t");
        probe
            .transact_mut()
            .apply_update(Update::decode_v1(&everything).unwrap())
            .unwrap();
        probe
            .transact_mut()
            .apply_update(Update::decode_v1(&nothing).unwrap())
            .unwrap();
        let txn = probe.transact();
        assert_eq!(t.get_string(&txn), "abcdef");
    }

    #[test]
    fn garbage_is_an_error_not_a_crash() {
        // Whatever the bytes are, the answer is a value, never a panic.
        for junk in [&b""[..], b"\x00\x01\x02", &[0xff; 64], &[0x01; 1000]] {
            let _ = merge(None, Some(junk));
            let _ = diff(junk, None);
        }
        assert!(merge(None, Some(&[0xff; 64])).is_err());
        assert!(Update::decode_v1(&[0xff; 64]).is_err());
        assert!(diff(&doc_with("x"), Some(&[0xff; 8])).is_err());
    }

    #[test]
    fn rooms_cannot_collide_with_users_or_each_other() {
        let card = Uuid::new_v4();
        assert_ne!(room(card, "description"), room(card, "notes"));
        assert_ne!(
            room(card, "description"),
            room(Uuid::new_v4(), "description")
        );
        assert_eq!(room(card, "description"), room(card, "description"));
    }
}
