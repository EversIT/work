//! End-to-end tests against a real Postgres. ``#[sqlx::test]` creates a throwaway database per
//! test and applies the migrations. Run with `DATABASE_URL=postgres://... cargo test -- --ignored`
//! (CI does this with a Postgres service container).
use super::*;
use axum::{body::Body, http::Request, response::Response};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

pub(crate) struct Client {
    pub(crate) app: Router,
    peer: u8,
}

impl Client {
    pub(crate) fn clone_client(&self) -> Client {
        Client {
            app: self.app.clone(),
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
        let mut config = Config {
            database_url: String::new(),
            jwt_secret: b"test-only-secret-test-only-secret-0123".to_vec(),
            frontend_url: "https://example.test".into(),
            bind_addr: "127.0.0.1:0".parse().unwrap(),
            registration_enabled: true,
            trust_forwarded_for: true,
            upload_dir: std::env::temp_dir()
                .join("work-e2e-uploads")
                .to_string_lossy()
                .into_owned(),
            max_upload_bytes: 1024 * 1024,
            smtp: None,
            encryption_key: None,
            slack: None,
            webhook_allow_private: true,
            webhook_allow_http: true,
            allow_external_email: false,
            auth_rate_limit: 1000,
        };
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
        let (events, rx) = events::channel();
        let state = AppState {
            db,
            config: Arc::new(config),
            dummy_hash: Arc::new(auth::hash_password("dummy-dummy-dummy").unwrap()),
            auth_limiter: Arc::new(RateLimiter::new(1000, Duration::from_secs(60))),
            api_limiter: Arc::new(RateLimiter::new(100_000, Duration::from_secs(60))),
            hook_limiter: Arc::new(RateLimiter::new(100_000, Duration::from_secs(60))),
            email_limiter: Arc::new(RateLimiter::new(100_000, Duration::from_secs(3600))),
            reset_limiter: Arc::new(RateLimiter::new(100_000, Duration::from_secs(3600))),
            events,
            hub: Arc::new(hub::Hub::default()),
        };
        tokio::spawn(events::run_worker(state.clone(), rx));
        Client {
            app: app(state),
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

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn tenants_are_isolated(db: PgPool) {
    let c = Client::new(db);
    let (a, _, _) = c.register("a@example.test").await;
    let (b, _, _) = c.register("b@example.test").await;
    let project_a = c.first_project(&a).await;

    let (_, ws, _) = c
        .call(
            "POST",
            &format!("/projects/{project_a}/workspaces"),
            Some(&a),
            None,
            Some(json!({"name":"W"})),
        )
        .await;
    let ws_id = ws["id"].as_str().unwrap();

    // B cannot see, list, modify or delete anything of A, and learns nothing about existence.
    for (m, p, body) in [
        ("GET", format!("/projects/{project_a}"), None),
        ("DELETE", format!("/projects/{project_a}"), None),
        ("GET", format!("/projects/{project_a}/members"), None),
        ("GET", format!("/projects/{project_a}/workspaces"), None),
        (
            "POST",
            format!("/projects/{project_a}/workspaces"),
            Some(json!({"name":"x"})),
        ),
        ("GET", format!("/workspaces/{ws_id}"), None),
        (
            "PATCH",
            format!("/workspaces/{ws_id}"),
            Some(json!({"name":"x"})),
        ),
        ("DELETE", format!("/workspaces/{ws_id}"), None),
        (
            "POST",
            format!("/projects/{project_a}/invites"),
            Some(json!({"role":"member"})),
        ),
    ] {
        let (st, _, _) = c.call(m, &p, Some(&b), None, body).await;
        assert_eq!(st, StatusCode::NOT_FOUND, "{m} {p}");
    }
    let (_, mine, _) = c.call("GET", "/projects", Some(&b), None, None).await;
    assert_eq!(
        mine.as_array().unwrap().len(),
        1,
        "B sees only their own project"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn invites_and_roles_cannot_escalate(db: PgPool) {
    let c = Client::new(db);
    let (owner, _, _) = c.register("owner@example.test").await;
    let (joiner, _, joiner_id) = c.register("joiner@example.test").await;
    let project = c.first_project(&owner).await;

    // An invite may not grant owner, and the invite grants exactly the stated role.
    let (st, _, _) = c
        .call(
            "POST",
            &format!("/projects/{project}/invites"),
            Some(&owner),
            None,
            Some(json!({"role":"owner"})),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, inv, _) = c
        .call(
            "POST",
            &format!("/projects/{project}/invites"),
            Some(&owner),
            None,
            Some(json!({"role":"viewer","max_uses":1})),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    let token = inv["token"].as_str().unwrap();

    let (st, _, _) = c
        .call(
            "POST",
            "/invites/accept",
            Some(&joiner),
            None,
            Some(json!({"token": token})),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    let (_, p, _) = c
        .call(
            "GET",
            &format!("/projects/{project}"),
            Some(&joiner),
            None,
            None,
        )
        .await;
    assert_eq!(p["role"], "viewer");

    // Single-use: a third user cannot reuse it.
    let (third, _, _) = c.register("third@example.test").await;
    let (st, _, _) = c
        .call(
            "POST",
            "/invites/accept",
            Some(&third),
            None,
            Some(json!({"token": token})),
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // A viewer cannot write, cannot invite, cannot promote themselves.
    let (st, _, _) = c
        .call(
            "POST",
            &format!("/projects/{project}/workspaces"),
            Some(&joiner),
            None,
            Some(json!({"name":"x"})),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _, _) = c
        .call(
            "POST",
            &format!("/projects/{project}/invites"),
            Some(&joiner),
            None,
            Some(json!({"role":"viewer"})),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _, _) = c
        .call(
            "PATCH",
            &format!("/projects/{project}/members/{joiner_id}"),
            Some(&joiner),
            None,
            Some(json!({"role":"admin"})),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Even the owner cannot hand out "owner" through the member endpoint.
    let (st, _, _) = c
        .call(
            "PATCH",
            &format!("/projects/{project}/members/{joiner_id}"),
            Some(&owner),
            None,
            Some(json!({"role":"owner"})),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn sessions_rotate_revoke_and_detect_reuse(db: PgPool) {
    let c = Client::new(db);
    let (access, cookie1, _) = c.register("s@example.test").await;

    // Refresh rotates the cookie.
    let (st, body, resp) = c
        .call("POST", "/auth/refresh", None, Some(&cookie1), None)
        .await;
    assert_eq!(st, StatusCode::OK);
    let cookie2 = resp.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    assert_ne!(cookie1, cookie2);
    let access2 = body["access_token"].as_str().unwrap().to_owned();

    // Replaying the old cookie fails and burns the whole family, including the new one.
    let (st, _, _) = c
        .call("POST", "/auth/refresh", None, Some(&cookie1), None)
        .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, _, _) = c
        .call("POST", "/auth/refresh", None, Some(&cookie2), None)
        .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let (st, _, _) = c.call("GET", "/me", Some(&access2), None, None).await;
    assert_eq!(
        st,
        StatusCode::UNAUTHORIZED,
        "access token of a revoked session is rejected"
    );

    // A fresh login works (case-insensitive email); logout then revokes it immediately,
    // long before the 15-minute access token would expire.
    let (st, body, _) = c
        .call(
            "POST",
            "/auth/login",
            None,
            None,
            Some(json!({"email":"S@Example.test","password":"correct-horse-battery"})),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "login is case-insensitive on email");
    let fresh = body["access_token"].as_str().unwrap().to_owned();
    let (st, _, _) = c.call("GET", "/me", Some(&fresh), None, None).await;
    assert_eq!(st, StatusCode::OK);
    let (st, _, _) = c
        .call("POST", "/auth/logout", Some(&fresh), None, None)
        .await;
    assert_eq!(st, StatusCode::OK);
    let (st, _, _) = c.call("GET", "/me", Some(&fresh), None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    let _ = access;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn login_does_not_leak_account_existence_or_accept_wildcards(db: PgPool) {
    let c = Client::new(db);
    c.register("real@example.test").await;
    let (s1, b1, _) = c
        .call(
            "POST",
            "/auth/login",
            None,
            None,
            Some(json!({"email":"real@example.test","password":"wrong-wrong-wrong"})),
        )
        .await;
    let (s2, b2, _) = c
        .call(
            "POST",
            "/auth/login",
            None,
            None,
            Some(json!({"email":"nobody@example.test","password":"wrong-wrong-wrong"})),
        )
        .await;
    assert_eq!((s1, &b1), (s2, &b2));
    assert_eq!(s1, StatusCode::UNAUTHORIZED);
    // `%` must not act as a LIKE wildcard.
    let (s, _, _) = c
        .call(
            "POST",
            "/auth/login",
            None,
            None,
            Some(json!({"email":"%@example.test","password":"correct-horse-battery"})),
        )
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // Weak passwords are rejected at registration.
    let (s, _, _) = c.call("POST", "/auth/register", None, None, Some(json!({"email":"w@example.test","password":"short","first_name":"a","last_name":"b"}))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}
