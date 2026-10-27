//! In-process domain events. Handlers emit events after a successful write; a background worker
//! turns them into notifications and automation runs. Events carry a `depth` so that
//! automation-caused changes cannot trigger automations forever.

use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::state::AppState;

pub const MAX_DEPTH: u8 = 3;

#[derive(Debug, Clone)]
pub enum Event {
    CardCreated {
        card_id: Uuid,
        workspace_id: Uuid,
        list_id: Uuid,
        depth: u8,
    },
    FieldUpdated {
        card_id: Uuid,
        workspace_id: Uuid,
        slug: String,
        old: Value,
        new: Value,
        actor: Option<Uuid>,
        depth: u8,
    },
    StageUpdated {
        card_id: Uuid,
        workspace_id: Uuid,
        list_id: Uuid,
        from: Option<Uuid>,
        to: Uuid,
        actor: Option<Uuid>,
        depth: u8,
    },
    CommentCreated {
        card_id: Uuid,
        workspace_id: Uuid,
        activity_id: Uuid,
        actor: Option<Uuid>,
        mentions: Vec<Uuid>,
        depth: u8,
    },
}

pub type EventSender = mpsc::UnboundedSender<Event>;
pub type EventReceiver = mpsc::UnboundedReceiver<Event>;

pub fn channel() -> (EventSender, EventReceiver) {
    mpsc::unbounded_channel()
}

pub fn emit(state: &AppState, event: Event) {
    // The receiver only disappears on shutdown; losing an event then is fine.
    let _ = state.events.send(event);
}

pub async fn run_worker(state: AppState, mut rx: EventReceiver) {
    while let Some(event) = rx.recv().await {
        if let Err(e) = crate::services::notify::handle_event(&state, &event).await {
            tracing::warn!(error = %e, "notification handler failed");
        }
        if let Err(e) = crate::domain::automations::handle_event(&state, &event).await {
            tracing::warn!(error = %e, "automation handler failed");
        }
    }
}
