//! The HTTP application: the route table, the security headers every response carries, and the
//! server loop. Handlers live in `routes/` and the domain modules; this file only assembles them.

use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    http::{HeaderName, HeaderValue, StatusCode},
    Json, Router,
};
use tower_http::{
    limit::RequestBodyLimitLayer, set_header::SetResponseHeaderLayer, timeout::TimeoutLayer,
    trace::TraceLayer,
};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use crate::{
    domain, identity,
    security::ratelimit,
    services::{
        self,
        events::{self, EventReceiver},
    },
    state::AppState,
};

/// All API routes plus the generated OpenAPI document. Paths are nested under `/api/v1`,
/// which is the prefix the frontend proxy forwards. The backend itself is never exposed
/// directly: it binds to loopback and is reached only through the frontend.
pub fn api(max_upload: usize) -> (Router<AppState>, utoipa::openapi::OpenApi) {
    OpenApiRouter::with_openapi(crate::openapi::ApiDoc::openapi())
        .merge(crate::routes::auth::router())
        .merge(crate::routes::admin::router())
        .merge(crate::routes::ai::router())
        .merge(crate::routes::ai_reports::router())
        .merge(crate::routes::users::router())
        .merge(crate::routes::mfa::router())
        .merge(crate::routes::passkeys::router())
        .merge(crate::routes::api_tokens::router())
        .merge(crate::routes::sessions::router())
        .merge(crate::routes::account_mail::router())
        .merge(crate::routes::sprints::router())
        .merge(crate::routes::projects::router())
        .merge(crate::routes::workspaces::router())
        .merge(crate::routes::spaces::router())
        .merge(crate::routes::lists::router())
        .merge(crate::routes::card_types::router())
        .merge(crate::routes::cards::router())
        .merge(services::collab::router())
        .merge(crate::routes::activities::router())
        .merge(crate::routes::notifications::router())
        .merge(crate::routes::files::router(max_upload))
        .merge(domain::automations::router())
        .merge(services::integrations::router())
        .merge(identity::oauth::router())
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
        // A handler that serves files sets a stricter, sandboxed policy of its own; this is the default.
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
        ))
        .layer(RequestBodyLimitLayer::new(1024 * 1024))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(TraceLayer::new_for_http()) // logs method/path/status only, never bodies or headers
}

/// Starts the background workers and serves until the process is told to stop.
pub async fn serve(state: AppState, events: EventReceiver) {
    let bind = state.config.bind_addr;
    tokio::spawn(events::run_worker(state.clone(), events));
    if state.config.scheduler {
        tokio::spawn(services::scheduler::run(state.clone()));
    }
    if state.config.shared_rate_limit {
        let db = state.db.clone();
        tokio::spawn(async move {
            loop {
                ratelimit::purge_shared(&db).await;
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        });
    }
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
    use crate::config::Config;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    fn test_state() -> AppState {
        // A lazy pool never connects: authentication fails before any query runs.
        let db =
            sqlx::PgPool::connect_lazy("postgres://unused@127.0.0.1:1/none").expect("lazy pool");
        AppState::for_tests(Config::for_tests(), db).0
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
