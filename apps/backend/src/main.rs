//! work backend: the HTTP API of the work-management server. See docs/ARCHITECTURE.md for the map.

mod app;
mod cli;
mod config;
mod domain;
mod error;
mod identity;
mod openapi;
mod routes;
mod security;
mod services;
mod state;
mod util;

#[cfg(test)]
mod tests;

use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

use crate::{cli::Command, config::Config, state::AppState};

fn exit_with(message: String) -> ! {
    eprintln!("{message}");
    std::process::exit(2);
}

/// Configuration, OpenID discovery, database and migrations: everything a command needs to run.
async fn start() -> (AppState, services::events::EventReceiver) {
    let mut config =
        Config::from_env().unwrap_or_else(|e| exit_with(format!("configuration error: {e}")));

    // OpenID Connect providers describe themselves; ask once, and refuse to start on a bad answer.
    if let Some(settings) = config.sso.oidc_pending.take() {
        match identity::oidc::discover(&settings).await {
            Ok(provider) => config.sso.oauth.push(provider),
            Err(e) => exit_with(format!(
                "configuration error: OpenID Connect ({}): {e}",
                settings.issuer
            )),
        }
    }

    let db = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await
        .unwrap_or_else(|e| exit_with(format!("cannot connect to the database: {e}")));
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .unwrap_or_else(|e| exit_with(format!("migrations failed: {e}")));

    AppState::new(config, db).unwrap_or_else(|e| exit_with(format!("configuration error: {e}")))
}

#[tokio::main]
async fn main() {
    let command = Command::from_args(std::env::args().skip(1));
    if let Command::Openapi = command {
        return cli::print_openapi();
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let (state, events) = start().await;
    // One-off commands never start the event worker: they send no notifications and run no automations.
    match command {
        Command::Serve => app::serve(state, events).await,
        Command::MakeAdmin { email } => cli::make_admin(&state, &email).await,
        Command::MigrateUpstream { dry_run, schema } => {
            cli::migrate_upstream(&state, dry_run, schema).await
        }
        Command::Openapi => unreachable!("handled above"),
    }
}
