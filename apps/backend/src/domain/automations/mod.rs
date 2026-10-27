//! Automations: "when X happens on a card, do Y". The engine runs in-process from domain events.
//!
//! Security notes, versus the original implementation:
//! * Creating/editing automations needs project admin rights, because actions run without
//!   per-list permission checks.
//! * A run only happens while the automation's creator is still a member (member role or higher)
//!   of the project; otherwise the run fails and says so.
//! * Placeholders (`{{trigger.path}}`) are plain path lookups into the event, never evaluated.
//! * Events caused by automations carry an incremented depth and stop at `MAX_DEPTH`.

mod engine;
mod routes;

#[cfg(test)]
pub(crate) use self::engine::sign;
pub(crate) use self::engine::{card_in_scope, run_scheduled};
pub use self::{engine::handle_event, routes::router};
use crate::error::ApiError;

const TRIGGERS: &[&str] = &[
    "card_created",
    "field_updated",
    "stage_changed",
    "comment_created",
    "assignee_added",
    "assignee_removed",
    "webhook",
    "date_arrives",
];
const ACTIONS: &[&str] = &[
    "create_card",
    "create_sub_card",
    "set_field",
    "create_comment",
    "change_assignee",
    "set_stage",
    "send_webhook",
    "send_email",
];

fn bad(m: impl Into<String>) -> ApiError {
    ApiError::BadRequest(m.into())
}
