//! Background scheduler. Today it fires the date-based automation trigger `date_arrives`:
//! once a minute it looks for cards whose date field (plus the automation's offset) fell into the
//! time since the previous tick and runs the automation for each of them.
//!
//! Several instances may run it: a tick takes a Postgres advisory lock for its duration, so only one
//! of them works at a time, and `automation_date_fires` makes every (automation, card, date) fire
//! at most once. After downtime the next tick catches up, but only over the last hour, so a long
//! outage cannot flood a workspace with stale runs.

use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::{json, Value};
use sqlx::types::Json as SqlJson;
use uuid::Uuid;

use crate::{domain::automations, state::AppState};

const JOB: &str = "date_triggers";
const TICK: Duration = Duration::from_secs(60);
/// First tick after a fresh start looks back this far; later ticks resume from the last one.
const FIRST_LOOKBACK: ChronoDuration = ChronoDuration::minutes(5);
const MAX_CATCH_UP: ChronoDuration = ChronoDuration::hours(1);
const MAX_PER_TICK: i64 = 500;
const LOCK_KEY: i64 = 0x7767_6b5f_7363_6864; // "wgk_schd": any constant, only used for this lock

/// `{"field": "due_at", "offset_minutes": -60}`
pub struct DateConfig {
    pub field: String,
    pub offset_minutes: i64,
}

impl DateConfig {
    pub fn parse(cfg: &Value) -> Result<Self, String> {
        let field = match cfg.get("field") {
            None | Some(Value::Null) => "due_at".to_owned(),
            Some(Value::String(f)) if !f.is_empty() => f.clone(),
            Some(_) => return Err("field must be the slug of a date field".into()),
        };
        let ok = field.len() <= 63
            && field.starts_with(|c: char| c.is_ascii_lowercase())
            && field
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !ok {
            return Err("field must be the slug of a date field".into());
        }
        let offset_minutes = match cfg.get("offset_minutes") {
            None | Some(Value::Null) => 0,
            Some(v) => v
                .as_i64()
                .filter(|m| (-43_200..=43_200).contains(m))
                .ok_or(
                    "offset_minutes must be a whole number between -43200 and 43200 (30 days)",
                )?,
        };
        Ok(Self {
            field,
            offset_minutes,
        })
    }
}

pub async fn run(state: AppState) {
    // Let the server finish starting before the first tick.
    tokio::time::sleep(Duration::from_secs(5)).await;
    loop {
        if let Err(e) = tick(&state, Utc::now()).await {
            tracing::warn!(error = %e, "scheduler tick failed");
        }
        tokio::time::sleep(TICK).await;
    }
}

/// One pass. `now` is a parameter so tests can move time.
pub async fn tick(s: &AppState, now: DateTime<Utc>) -> Result<(), sqlx::Error> {
    let mut tx = s.db.begin().await?;
    let (locked,): (bool,) = sqlx::query_as("SELECT pg_try_advisory_xact_lock($1)")
        .bind(LOCK_KEY)
        .fetch_one(&mut *tx)
        .await?;
    if !locked {
        return Ok(()); // another instance is on it
    }
    let last: Option<(DateTime<Utc>,)> =
        sqlx::query_as("SELECT last_run FROM scheduler_state WHERE job = $1")
            .bind(JOB)
            .fetch_optional(&mut *tx)
            .await?;
    let from = match last {
        Some((t,)) => t.max(now - MAX_CATCH_UP),
        None => now - FIRST_LOOKBACK,
    };
    if from >= now {
        return Ok(());
    }

    let autos: Vec<(Uuid, Uuid, Option<Uuid>, SqlJson<Value>)> = sqlx::query_as(
        "SELECT id, workspace_id, created_by, trigger_config FROM automations
         WHERE is_enabled AND deleted_at IS NULL AND trigger_type = 'date_arrives'",
    )
    .fetch_all(&mut *tx)
    .await?;
    for (automation, workspace_id, creator, SqlJson(cfg)) in autos {
        let Ok(cfg) = DateConfig::parse(&cfg) else {
            continue; // validated on save; a hand-edited row is skipped, not fatal
        };
        // The moment a card fires is its date plus the offset; date-only values mean 00:00 UTC.
        let due: Vec<(Uuid, DateTime<Utc>, String)> = sqlx::query_as(
            "WITH dated AS (
               SELECT c.id,
                      CASE WHEN c.data->>$2 ~ '^\\d{4}-\\d{2}-\\d{2}$'
                           THEN ((c.data->>$2)::date)::timestamp AT TIME ZONE 'UTC'
                           ELSE (c.data->>$2)::timestamptz END AS due,
                      c.data->>$2 AS raw
                 FROM cards c
                WHERE c.workspace_id = $1 AND c.deleted_at IS NULL AND c.data ? $2
                  AND c.data->>$2 ~ '^\\d{4}-\\d{2}-\\d{2}'
             )
             SELECT id, due + make_interval(mins => $5::int) AS fire_at, raw FROM dated
              WHERE due + make_interval(mins => $5::int) > $3 AND due + make_interval(mins => $5::int) <= $4
              ORDER BY fire_at LIMIT $6",
        )
        .bind(workspace_id)
        .bind(&cfg.field)
        .bind(from)
        .bind(now)
        .bind(cfg.offset_minutes as i32)
        .bind(MAX_PER_TICK)
        .fetch_all(&mut *tx)
        .await?;
        for (card_id, fire_at, raw) in due {
            let fresh = sqlx::query(
                "INSERT INTO automation_date_fires (automation_id, card_id, fire_at) VALUES ($1,$2,$3)
                 ON CONFLICT DO NOTHING",
            )
            .bind(automation)
            .bind(card_id)
            .bind(fire_at)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if fresh == 0 || !automations::card_in_scope(s, automation, card_id).await? {
                continue;
            }
            let trigger = json!({
                "type": "date_arrives",
                "field": cfg.field,
                "date": raw,
                "offsetMinutes": cfg.offset_minutes,
                "firedFor": fire_at,
            });
            if let Err(e) =
                automations::run_scheduled(s, automation, creator, card_id, workspace_id, trigger)
                    .await
            {
                tracing::warn!(error = %e, %automation, %card_id, "scheduled automation run failed");
            }
        }
    }
    sqlx::query(
        "INSERT INTO scheduler_state (job, last_run) VALUES ($1,$2)
         ON CONFLICT (job) DO UPDATE SET last_run = EXCLUDED.last_run",
    )
    .bind(JOB)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    // Fired rows of dates that are long past are of no use any more.
    sqlx::query("DELETE FROM automation_date_fires WHERE fire_at < $1")
        .bind(now - ChronoDuration::days(60))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_and_limits() {
        let c = DateConfig::parse(&json!({})).unwrap();
        assert_eq!((c.field.as_str(), c.offset_minutes), ("due_at", 0));
        let c = DateConfig::parse(&json!({"field": "starts_at", "offset_minutes": -1440})).unwrap();
        assert_eq!((c.field.as_str(), c.offset_minutes), ("starts_at", -1440));
        for bad in [
            json!({"field": "Due At"}),
            json!({"field": "x'; drop table cards;--"}),
            json!({"field": 5}),
            json!({"offset_minutes": 1.5}),
            json!({"offset_minutes": 99999999}),
            json!({"offset_minutes": "soon"}),
        ] {
            assert!(DateConfig::parse(&bad).is_err(), "{bad}");
        }
    }
}
