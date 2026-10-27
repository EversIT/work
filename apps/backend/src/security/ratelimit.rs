//! Small fixed-window limiter, keyed by an arbitrary string (IP, IP+email, ...).
//! By default the counters live in the process (one instance). With `WORK_RATE_LIMIT_BACKEND=postgres`
//! they live in the database, so several instances share one budget.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use sqlx::PgPool;

enum Store {
    Memory(Mutex<HashMap<String, (Instant, u32)>>),
    Postgres(PgPool),
}

pub struct RateLimiter {
    /// Namespace in the shared table, so two limiters never count into the same row.
    name: &'static str,
    max: u32,
    window: Duration,
    store: Store,
}

impl RateLimiter {
    pub fn new(max: u32, window: Duration) -> Self {
        Self {
            name: "",
            max,
            window,
            store: Store::Memory(Mutex::new(HashMap::new())),
        }
    }

    /// Counts in the database instead of in the process.
    pub fn shared(name: &'static str, max: u32, window: Duration, db: PgPool) -> Self {
        Self {
            name,
            max,
            window,
            store: Store::Postgres(db),
        }
    }

    /// Returns true if the request is allowed. A database failure denies the request: the limiter
    /// guards credentials and mail, so it must not fail open.
    pub async fn check(&self, key: &str) -> bool {
        match &self.store {
            Store::Memory(hits) => self.check_memory(hits, key),
            Store::Postgres(db) => match self.check_shared(db, key).await {
                Ok(allowed) => allowed,
                Err(e) => {
                    tracing::warn!(error = %e, "shared rate limiter unavailable, denying");
                    false
                }
            },
        }
    }

    fn check_memory(&self, hits: &Mutex<HashMap<String, (Instant, u32)>>, key: &str) -> bool {
        let now = Instant::now();
        let mut hits = hits.lock().unwrap_or_else(|p| p.into_inner());
        if hits.len() > 10_000 {
            hits.retain(|_, (start, _)| now.duration_since(*start) < self.window);
        }
        let entry = hits.entry(key.to_owned()).or_insert((now, 0));
        if now.duration_since(entry.0) >= self.window {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 <= self.max
    }

    async fn check_shared(&self, db: &PgPool, key: &str) -> Result<bool, sqlx::Error> {
        let (hits,): (i32,) = sqlx::query_as(
            "INSERT INTO rate_limits (key, window_start, hits) VALUES ($1, now(), 1)
             ON CONFLICT (key) DO UPDATE SET
               hits = CASE WHEN rate_limits.window_start <= now() - make_interval(secs => $2)
                           THEN 1 ELSE rate_limits.hits + 1 END,
               window_start = CASE WHEN rate_limits.window_start <= now() - make_interval(secs => $2)
                                   THEN now() ELSE rate_limits.window_start END
             RETURNING hits",
        )
        .bind(format!("{}:{key}", self.name))
        .bind(self.window.as_secs_f64())
        .fetch_one(db)
        .await?;
        Ok(hits as u32 <= self.max)
    }
}

/// Drops counters whose window ended long ago (the longest window is one hour).
pub async fn purge_shared(db: &PgPool) {
    if let Err(e) =
        sqlx::query("DELETE FROM rate_limits WHERE window_start < now() - interval '1 day'")
            .execute(db)
            .await
    {
        tracing::warn!(error = %e, "purging rate limits failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn blocks_after_max_and_isolates_keys() {
        let rl = RateLimiter::new(2, Duration::from_secs(60));
        assert!(rl.check("a").await);
        assert!(rl.check("a").await);
        assert!(!rl.check("a").await);
        assert!(rl.check("b").await);
    }
}
