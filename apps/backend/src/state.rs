use std::sync::Arc;

use sqlx::PgPool;

use crate::{config::Config, events::EventSender, hub::Hub, ratelimit::RateLimiter};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// Hash verified when the account does not exist, so login timing does not reveal it.
    pub dummy_hash: Arc<String>,
    pub auth_limiter: Arc<RateLimiter>,
    /// Per API token, so one integration cannot hog the instance.
    pub api_limiter: Arc<RateLimiter>,
    /// Public webhook endpoints, per IP and per automation.
    pub hook_limiter: Arc<RateLimiter>,
    /// Outgoing automation e-mails, per automation.
    pub email_limiter: Arc<RateLimiter>,
    /// Password-reset e-mails, per address.
    pub reset_limiter: Arc<RateLimiter>,
    pub events: EventSender,
    pub hub: Arc<Hub>,
}
