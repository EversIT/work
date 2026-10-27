//! The commands besides "serve": printing the API description, importing an upstream database and
//! promoting an account to instance administrator.

use sqlx::postgres::PgPoolOptions;

use crate::{
    app,
    services::migrate::{self, Options},
    state::AppState,
};

pub enum Command {
    Serve,
    /// Print the OpenAPI document. Needs neither a database nor secrets, so CI and the frontend's
    /// client generator can run it.
    Openapi,
    /// `migrate-upstream [--dry-run] [--schema NAME]`, source in `WORK_MIGRATE_SOURCE_URL`.
    MigrateUpstream {
        dry_run: bool,
        schema: String,
    },
    /// `make-admin <e-mail>`
    MakeAdmin {
        email: String,
    },
}

impl Command {
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Command {
        let args: Vec<String> = args.into_iter().collect();
        match args.first().map(String::as_str) {
            Some("openapi") => Command::Openapi,
            Some("migrate-upstream") => Command::MigrateUpstream {
                dry_run: args.iter().any(|a| a == "--dry-run"),
                schema: args
                    .iter()
                    .position(|a| a == "--schema")
                    .and_then(|i| args.get(i + 1))
                    .cloned()
                    .unwrap_or_else(|| "work_export".into()),
            },
            Some("make-admin") => match args.get(1) {
                Some(email) => Command::MakeAdmin {
                    email: email.trim().to_lowercase(),
                },
                None => fail(2, "usage: work-backend make-admin <e-mail>"),
            },
            _ => Command::Serve,
        }
    }
}

fn fail(code: i32, message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(code);
}

pub fn print_openapi() {
    let (_, spec) = app::api(5 * 1024 * 1024);
    println!("{}", spec.to_pretty_json().expect("serializable"));
}

pub async fn make_admin(state: &AppState, email: &str) {
    let done = sqlx::query(
        "UPDATE users SET is_instance_admin = true WHERE email = $1 AND deleted_at IS NULL",
    )
    .bind(email)
    .execute(&state.db)
    .await
    .map(|r| r.rows_affected())
    .unwrap_or(0);
    if done == 0 {
        fail(1, &format!("no account with the e-mail address {email}"));
    }
    println!("{email} is an instance administrator now");
}

/// See `docs/DATA-MIGRATION.md`. The exit status is 0 for a clean import, 1 when the report lists
/// problems (the import still went through) and 2 when it could not run.
pub async fn migrate_upstream(state: &AppState, dry_run: bool, schema: String) {
    let Some(url) = std::env::var("WORK_MIGRATE_SOURCE_URL")
        .ok()
        .filter(|v| !v.is_empty())
    else {
        fail(2, "WORK_MIGRATE_SOURCE_URL must point at the source database (read-only access is enough)");
    };
    let source = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .unwrap_or_else(|e| fail(2, &format!("cannot connect to the source database: {e}")));
    match migrate::run(state, &source, &Options { schema, dry_run }).await {
        Ok(report) => {
            println!(
                "{}{report}",
                if dry_run {
                    "DRY RUN (nothing written)\n"
                } else {
                    ""
                }
            );
            if !report.problems.is_empty() {
                std::process::exit(1);
            }
        }
        Err(e) => fail(2, &format!("import failed: {e}")),
    }
}
