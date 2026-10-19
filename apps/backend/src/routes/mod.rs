pub mod account_mail;
pub mod activities;
pub mod api_tokens;
pub mod auth;
pub mod card_types;
pub mod cards;
pub mod files;
pub mod lists;
pub mod notifications;
pub mod projects;
pub mod spaces;
pub mod sprints;
pub mod users;
pub mod workspaces;

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct OkOut {
    pub ok: bool,
}
