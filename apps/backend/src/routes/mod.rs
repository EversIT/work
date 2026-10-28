pub mod account_mail;
pub mod activities;
pub mod admin;
pub mod ai;
pub mod ai_reports;
pub mod api_tokens;
pub mod auth;
pub mod card_types;
pub mod cards;
pub mod files;
pub mod lists;
pub mod mcp;
pub mod mfa;
pub mod notifications;
pub mod passkeys;
pub mod projects;
pub mod sessions;
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
