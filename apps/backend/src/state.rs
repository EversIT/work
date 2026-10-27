use std::{sync::Arc, time::Duration};

use sqlx::PgPool;

use crate::{
    config::Config,
    security::{
        auth::{hash_password, random_token},
        ratelimit::RateLimiter,
    },
    services::{
        events::{self, EventReceiver, EventSender},
        hub::Hub,
        storage::Storage,
    },
};

/// Everything a request handler can reach. Cheap to clone: it is all handles.
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
    pub storage: Storage,
}

impl AppState {
    /// The production wiring. The receiver belongs to the event worker, which the caller starts
    /// (or deliberately does not, for one-off commands that must not send notifications).
    pub fn new(config: Config, db: PgPool) -> Result<(Self, EventReceiver), String> {
        let limiter = |name: &'static str, max: u32, secs: u64| {
            let window = Duration::from_secs(secs);
            Arc::new(if config.shared_rate_limit {
                // Counters in the database: several instances share one budget.
                RateLimiter::shared(name, max, window, db.clone())
            } else {
                RateLimiter::new(max, window)
            })
        };
        let (auth_limiter, api_limiter, hook_limiter, email_limiter, reset_limiter) = (
            limiter("auth", config.auth_rate_limit, 60),
            limiter("api", 600, 60),
            limiter("hook", 120, 60),
            limiter("email", 20, 3600),
            limiter("reset", 3, 3600),
        );
        let dummy_hash = hash_password(&random_token()).map_err(|e| e.to_string())?;
        let storage = Storage::from_config(&config).map_err(|e| format!("file storage: {e}"))?;
        let (events, receiver) = events::channel();
        let state = Self {
            db,
            config: Arc::new(config),
            dummy_hash: Arc::new(dummy_hash),
            auth_limiter,
            api_limiter,
            hook_limiter,
            email_limiter,
            reset_limiter,
            events,
            hub: Arc::new(Hub::default()),
            storage,
        };
        Ok((state, receiver))
    }

    /// Like `new`, with limits that tests never run into and a cheap dummy hash.
    #[cfg(test)]
    pub fn for_tests(config: Config, db: PgPool) -> (Self, EventReceiver) {
        let big = |max: u32, secs: u64| Arc::new(RateLimiter::new(max, Duration::from_secs(secs)));
        let storage = Storage::from_config(&config).expect("test storage");
        let (events, receiver) = events::channel();
        let state = Self {
            db,
            config: Arc::new(config),
            dummy_hash: Arc::new(hash_password("dummy-dummy-dummy").expect("hashing works")),
            auth_limiter: big(1000, 60),
            api_limiter: big(100_000, 60),
            hook_limiter: big(100_000, 60),
            email_limiter: big(100_000, 3600),
            reset_limiter: big(100_000, 3600),
            events,
            hub: Arc::new(Hub::default()),
            storage,
        };
        (state, receiver)
    }
}
