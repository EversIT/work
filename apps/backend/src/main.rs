mod auth;
mod authz;
mod automations;
mod config;
mod error;
mod events;
mod filters;
mod hub;
mod integrations;
mod mailer;
mod notify;
mod openapi;
mod ratelimit;
mod routes;
mod ssrf;
mod state;
mod util;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    http::{HeaderName, HeaderValue, StatusCode},
    Json, Router,
};
use sqlx::postgres::PgPoolOptions;
use tower_http::{
    limit::RequestBodyLimitLayer, set_header::SetResponseHeaderLayer, timeout::TimeoutLayer,
    trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use crate::{config::Config, ratelimit::RateLimiter, state::AppState};

/// All API routes plus the generated OpenAPI document. Paths are nested under `/api/v1`,
/// which is the prefix the frontend proxy forwards. The backend itself is never exposed
/// directly: it binds to loopback and is reached only through the frontend.
pub fn api(max_upload: usize) -> (Router<AppState>, utoipa::openapi::OpenApi) {
    OpenApiRouter::with_openapi(openapi::ApiDoc::openapi())
        .merge(routes::auth::router())
        .merge(routes::users::router())
        .merge(routes::api_tokens::router())
        .merge(routes::account_mail::router())
        .merge(routes::sprints::router())
        .merge(routes::projects::router())
        .merge(routes::workspaces::router())
        .merge(routes::spaces::router())
        .merge(routes::lists::router())
        .merge(routes::card_types::router())
        .merge(routes::cards::router())
        .merge(routes::activities::router())
        .merge(routes::notifications::router())
        .merge(routes::files::router(max_upload))
        .merge(automations::router())
        .merge(integrations::router())
        .split_for_parts()
}

pub fn app(state: AppState) -> Router {
    let sec = |name: &'static str, value: &'static str| {
        SetResponseHeaderLayer::overriding(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        )
    };
    let (api_routes, spec) = api(state.config.max_upload_bytes);
    let spec = Arc::new(spec);

    // Same-origin through the proxy, so there is deliberately no CORS layer:
    // browsers from other origins get no cross-origin access.
    Router::new()
        .nest("/api/v1", api_routes)
        .route(
            "/api/v1/openapi.json",
            axum::routing::get(move || {
                let spec = spec.clone();
                async move { Json((*spec).clone()) }
            }),
        )
        .route(
            "/api/v1/health",
            axum::routing::get(|| async { Json(serde_json::json!({ "status": "ok" })) }),
        )
        .with_state(state)
        .layer(sec("x-content-type-options", "nosniff"))
        .layer(sec("x-frame-options", "DENY"))
        .layer(sec("referrer-policy", "no-referrer"))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("cache-control"),
            HeaderValue::from_static("no-store"),
        ))
        .layer(sec(
            "content-security-policy",
            "default-src 'none'; frame-ancestors 'none'",
        ))
        .layer(RequestBodyLimitLayer::new(1024 * 1024))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(TraceLayer::new_for_http()) // logs method/path/status only, never bodies or headers
}

#[tokio::main]
async fn main() {
    // `work-backend openapi` prints the OpenAPI document and exits. It needs neither a
    // database nor any secrets, so it can run in CI and in the frontend's orval step.
    if std::env::args().nth(1).as_deref() == Some("openapi") {
        let (_, spec) = api(5 * 1024 * 1024);
        println!("{}", spec.to_pretty_json().expect("serializable"));
        return;
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("configuration error: {e}");
            std::process::exit(2);
        }
    };

    let db = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await
        .expect("cannot connect to database");
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("migrations failed");

    let dummy_hash = auth::hash_password(&auth::random_token()).expect("hashing works");
    let bind = config.bind_addr;
    let auth_rate = config.auth_rate_limit;
    let (events_tx, events_rx) = events::channel();
    let state = AppState {
        db,
        config: Arc::new(config),
        dummy_hash: Arc::new(dummy_hash),
        auth_limiter: Arc::new(RateLimiter::new(auth_rate, Duration::from_secs(60))),
        api_limiter: Arc::new(RateLimiter::new(600, Duration::from_secs(60))),
        hook_limiter: Arc::new(RateLimiter::new(120, Duration::from_secs(60))),
        email_limiter: Arc::new(RateLimiter::new(20, Duration::from_secs(3600))),
        reset_limiter: Arc::new(RateLimiter::new(3, Duration::from_secs(3600))),
        events: events_tx,
        hub: Arc::new(hub::Hub::default()),
    };
    tokio::spawn(events::run_worker(state.clone(), events_rx));

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .expect("cannot bind");
    tracing::info!(%bind, "work backend listening");
    axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
    .expect("server error");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    fn test_state() -> AppState {
        let config = Config {
            database_url: "postgres://unused".into(),
            jwt_secret: b"test-only-secret-test-only-secret-0123".to_vec(),
            frontend_url: "https://example.test".into(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            registration_enabled: true,
            trust_forwarded_for: false,
            upload_dir: "uploads-test".into(),
            max_upload_bytes: 1024,
            smtp: None,
            encryption_key: None,
            slack: None,
            webhook_allow_private: false,
            webhook_allow_http: false,
            allow_external_email: false,
            auth_rate_limit: 1000,
        };
        let (events, _rx) = events::channel();
        AppState {
            // Lazy pool: never connects. Auth fails before any query runs.
            db: sqlx::PgPool::connect_lazy("postgres://unused@127.0.0.1:1/none").unwrap(),
            config: Arc::new(config),
            dummy_hash: Arc::new(String::new()),
            auth_limiter: Arc::new(RateLimiter::new(1000, Duration::from_secs(60))),
            api_limiter: Arc::new(RateLimiter::new(100_000, Duration::from_secs(60))),
            hook_limiter: Arc::new(RateLimiter::new(100_000, Duration::from_secs(60))),
            email_limiter: Arc::new(RateLimiter::new(100_000, Duration::from_secs(3600))),
            reset_limiter: Arc::new(RateLimiter::new(100_000, Duration::from_secs(3600))),
            events,
            hub: Arc::new(hub::Hub::default()),
        }
    }

    /// Every operation the OpenAPI spec marks as secured must reject a request without a
    /// token. Derived from the spec, so new routes are covered automatically.
    #[tokio::test]
    async fn secured_routes_reject_anonymous_and_garbage_tokens() {
        let (_, spec) = api(1024);
        let spec = serde_json::to_value(&spec).unwrap();
        let app = app(test_state());
        let id = "00000000-0000-0000-0000-000000000001";
        let mut checked = 0;

        for (path, ops) in spec["paths"].as_object().unwrap() {
            for (method, op) in ops.as_object().unwrap() {
                if op.get("security").is_none() {
                    continue;
                }
                let uri = format!(
                    "/api/v1{}",
                    path.replace("{id}", id).replace("{user_id}", id)
                );
                for auth in [None, Some("Bearer not-a-jwt"), Some("Basic abc")] {
                    let mut req = Request::builder()
                        .method(method.to_uppercase().as_str())
                        .uri(&uri);
                    if let Some(a) = auth {
                        req = req.header("authorization", a);
                    }
                    let resp = app
                        .clone()
                        .oneshot(req.body(Body::empty()).unwrap())
                        .await
                        .unwrap();
                    assert_eq!(
                        resp.status(),
                        StatusCode::UNAUTHORIZED,
                        "{method} {uri} auth={auth:?}"
                    );
                }
                checked += 1;
            }
        }
        assert!(
            checked >= 12,
            "expected to check the secured routes, checked {checked}"
        );
    }

    #[tokio::test]
    async fn api_has_security_headers_and_no_cors() {
        let resp = app(test_state())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .header("origin", "https://evil.test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers()["x-content-type-options"], "nosniff");
        assert!(resp.headers().get("access-control-allow-origin").is_none());
    }

    #[tokio::test]
    async fn serves_openapi_document() {
        let resp = app(test_state())
            .oneshot(
                Request::builder()
                    .uri("/api/v1/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}

#[cfg(test)]
mod e2e;
#[cfg(test)]
mod e2e_domain;
#[cfg(test)]
mod e2e_hooks;
