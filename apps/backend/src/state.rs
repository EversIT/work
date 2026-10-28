use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use sqlx::PgPool;

use crate::{
    config::{AiConfig, Config},
    security::{
        auth::{hash_password, random_token},
        ratelimit::RateLimiter,
    },
    services::{
        ai::Ai,
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
    /// AI requests, per user: the local model is slow and shared by everyone.
    pub ai_limiter: Arc<RateLimiter>,
    /// The self-hosted language model; see [`AppState::ai`]. Empty: the AI features are off.
    ai: Arc<tokio::sync::RwLock<AiSlot>>,
    pub events: EventSender,
    pub hub: Arc<Hub>,
    pub storage: Storage,
}

/// How long a loaded model setting is trusted before the database is asked again, so a change an
/// admin makes on one instance reaches the others within seconds.
const AI_REFRESH: Duration = Duration::from_secs(15);

struct AiSlot {
    client: Option<Arc<Ai>>,
    loaded: Instant,
}

impl AiSlot {
    fn from_env(config: &Config) -> Result<Self, String> {
        Ok(Self {
            client: config.ai.clone().map(Ai::new).transpose()?.map(Arc::new),
            loaded: Instant::now(),
        })
    }
}

impl AppState {
    /// The language model the AI features use, or `None`: then they are off. An admin's setting in
    /// the database wins over the environment; it is re-read every few seconds.
    pub async fn ai(&self) -> Option<Arc<Ai>> {
        {
            let slot = self.ai.read().await;
            if slot.loaded.elapsed() < AI_REFRESH {
                return slot.client.clone();
            }
        }
        self.refresh_ai().await
    }

    /// Re-reads the model setting now (after an admin changed it). A database error keeps what was
    /// there.
    pub async fn refresh_ai(&self) -> Option<Arc<Ai>> {
        let row: Result<Option<(String, String, i32)>, _> =
            sqlx::query_as("SELECT model_url, model_name, max_output_tokens FROM ai_settings")
                .fetch_optional(&self.db)
                .await;
        let mut slot = self.ai.write().await;
        let config: Option<AiConfig> = match row {
            Ok(Some((url, name, max))) => AiConfig::parse(&url, &name, max.max(0) as u32).ok(),
            Ok(None) => self.config.ai.clone(),
            Err(e) => {
                tracing::warn!("reading the AI setting failed: {e}");
                slot.loaded = Instant::now();
                return slot.client.clone();
            }
        };
        slot.client = config.and_then(|c| Ai::new(c).ok()).map(Arc::new);
        slot.loaded = Instant::now();
        slot.client.clone()
    }

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
        let (auth_limiter, api_limiter, hook_limiter, email_limiter, reset_limiter, ai_limiter) = (
            limiter("auth", config.auth_rate_limit, 60),
            limiter("api", 600, 60),
            limiter("hook", 120, 60),
            limiter("email", 20, 3600),
            limiter("reset", 3, 3600),
            limiter("ai", 20, 60),
        );
        let ai = AiSlot::from_env(&config).map_err(|e| format!("AI client: {e}"))?;
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
            ai_limiter,
            ai: Arc::new(tokio::sync::RwLock::new(ai)),
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
        let ai = AiSlot::from_env(&config).expect("test AI client");
        let state = Self {
            db,
            config: Arc::new(config),
            dummy_hash: Arc::new(hash_password("dummy-dummy-dummy").expect("hashing works")),
            auth_limiter: big(1000, 60),
            api_limiter: big(100_000, 60),
            hook_limiter: big(100_000, 60),
            email_limiter: big(100_000, 3600),
            reset_limiter: big(100_000, 3600),
            ai_limiter: big(100_000, 60),
            ai: Arc::new(tokio::sync::RwLock::new(ai)),
            events,
            hub: Arc::new(Hub::default()),
            storage,
        };
        (state, receiver)
    }
}
