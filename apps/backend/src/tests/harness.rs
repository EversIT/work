//! End-to-end tests against a real Postgres. ``#[sqlx::test]` creates a throwaway database per
//! test and applies the migrations. Run with `DATABASE_URL=postgres://... cargo test -- --ignored`
//! (CI does this with a Postgres service container).
//! The test client: the whole application behind one function call, with cookies, bearer tokens and
//! a distinct client address per instance (so rate limits do not leak between tests).

use std::net::SocketAddr;

use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::Response,
    Router,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

use crate::{app::app, config::Config, services::events, state::AppState};

pub(crate) struct Client {
    pub(crate) app: Router,
    /// The server's state, for tests that drive background jobs directly.
    pub(crate) state: AppState,
    peer: u8,
}

impl Client {
    pub(crate) fn clone_client(&self) -> Client {
        Client {
            app: self.app.clone(),
            state: self.state.clone(),
            peer: self.peer,
        }
    }

    /// Request with a raw `Authorization` header value (used for Basic auth).
    pub(crate) async fn basic(&self, method: &str, uri: &str, authorization: &str) -> StatusCode {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{uri}"))
            .header("authorization", authorization)
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            "127.0.0.1:9999".parse::<SocketAddr>().unwrap(),
        ));
        self.app.clone().oneshot(req).await.unwrap().status()
    }
}

impl Client {
    pub(crate) fn new(db: PgPool) -> Self {
        Self::new_with(db, |_| {})
    }

    /// Like `new`, with a chance to change the configuration (SSRF settings, mail, ...).
    /// If `TEST_SMTP_PORT` is set, outgoing mail goes to that local test relay (Mailpit).
    pub(crate) fn new_with(db: PgPool, tweak: impl FnOnce(&mut Config)) -> Self {
        let mut config = Config::for_tests();
        if let Some(port) = std::env::var("TEST_SMTP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
        {
            config.smtp = Some(crate::config::SmtpConfig {
                host: "127.0.0.1".into(),
                port,
                user: None,
                pass: None,
                from: "work <noreply@work.test>".into(),
                tls: "none".into(),
            });
        }
        tweak(&mut config);
        let (state, rx) = AppState::for_tests(config, db);
        tokio::spawn(events::run_worker(state.clone(), rx));
        Client {
            app: app(state.clone()),
            state,
            peer: 1,
        }
    }

    pub(crate) async fn call(
        &self,
        method: &str,
        uri: &str,
        token: Option<&str>,
        cookie: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value, Response) {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{uri}"))
            .header("x-forwarded-for", format!("10.0.0.{}", self.peer))
            .header("content-type", "application/json");
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        if let Some(c) = cookie {
            req = req.header("cookie", c);
        }
        let mut req = req
            .body(Body::from(body.map(|b| b.to_string()).unwrap_or_default()))
            .unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            "127.0.0.1:9999".parse::<SocketAddr>().unwrap(),
        ));
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let (parts, body) = resp.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json, Response::from_parts(parts, Body::empty()))
    }

    /// Register and return (access token, refresh cookie header value, user id).
    pub(crate) async fn register(&self, email: &str) -> (String, String, String) {
        let (st, body, resp) = self
            .call("POST", "/auth/register", None, None, Some(json!({
                "email": email, "password": "correct-horse-battery", "first_name": "T", "last_name": "U"
            })))
            .await;
        assert_eq!(st, StatusCode::OK, "{body}");
        let cookie = resp.headers()["set-cookie"].to_str().unwrap().to_owned();
        assert!(
            cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"),
            "{cookie}"
        );
        assert!(
            body.get("refresh_token").is_none(),
            "refresh token must not be in the body"
        );
        let token = body["access_token"].as_str().unwrap().to_owned();
        let (_, me, _) = self.call("GET", "/me", Some(&token), None, None).await;
        (
            token,
            cookie.split(';').next().unwrap().to_owned(),
            me["id"].as_str().unwrap().to_owned(),
        )
    }

    pub(crate) async fn first_project(&self, token: &str) -> String {
        let (_, list, _) = self.call("GET", "/projects", Some(token), None, None).await;
        list[0]["id"].as_str().unwrap().to_owned()
    }
}

impl Client {
    /// JSON request with a bearer token; returns status and parsed body.
    pub(crate) async fn api(
        &self,
        method: &str,
        uri: &str,
        token: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (st, v, _) = self.call(method, uri, Some(token), None, body).await;
        (st, v)
    }

    /// Like `api` but asserts 200 and returns the body.
    pub(crate) async fn ok(
        &self,
        method: &str,
        uri: &str,
        token: &str,
        body: Option<Value>,
    ) -> Value {
        let (st, v) = self.api(method, uri, token, body).await;
        assert_eq!(st, StatusCode::OK, "{method} {uri} -> {st}: {v}");
        v
    }

    /// Raw request, for multipart uploads and binary downloads.
    pub(crate) async fn raw(
        &self,
        method: &str,
        uri: &str,
        token: &str,
        content_type: &str,
        body: Vec<u8>,
    ) -> Response {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{uri}"))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", content_type)
            .body(Body::from(body))
            .unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            "127.0.0.1:9999".parse::<SocketAddr>().unwrap(),
        ));
        self.app.clone().oneshot(req).await.unwrap()
    }

    /// GET without authentication; returns the body as text.
    pub(crate) async fn get_text(&self, uri: &str) -> (StatusCode, String) {
        let mut req = Request::builder()
            .uri(format!("/api/v1{uri}"))
            .body(Body::empty())
            .unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            "127.0.0.1:9999".parse::<SocketAddr>().unwrap(),
        ));
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Browser-style form POST (what an IdP's auto-submitting page does). Returns the response.
    pub(crate) async fn form_post(
        &self,
        uri: &str,
        cookie: Option<&str>,
        body: String,
    ) -> Response {
        let mut req = Request::builder()
            .method("POST")
            .uri(format!("/api/v1{uri}"))
            .header("content-type", "application/x-www-form-urlencoded");
        if let Some(c) = cookie {
            req = req.header("cookie", c);
        }
        let mut req = req.body(Body::from(body)).unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            "127.0.0.1:9999".parse::<SocketAddr>().unwrap(),
        ));
        self.app.clone().oneshot(req).await.unwrap()
    }

    /// Registers `email`, then joins `project` through an invite created by `owner` with `role`.
    pub(crate) async fn join(
        &self,
        owner: &str,
        project: &str,
        email: &str,
        role: &str,
    ) -> (String, String) {
        let inv = self
            .ok(
                "POST",
                &format!("/projects/{project}/invites"),
                owner,
                Some(json!({"role": role})),
            )
            .await;
        let (token, _, id) = self.register(email).await;
        self.ok(
            "POST",
            "/invites/accept",
            &token,
            Some(json!({"token": inv["token"]})),
        )
        .await;
        (token, id)
    }
}
