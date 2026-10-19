//! End-to-end tests for webhooks (incoming trigger, outgoing action, SSRF protection) and every
//! flow that sends e-mail. Mail tests talk to a local Mailpit (`TEST_SMTP_PORT` and
//! `TEST_MAILPIT_API`); without it they skip themselves.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Router,
};
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::e2e::Client;

fn id(v: &Value) -> String {
    v["id"].as_str().unwrap().to_owned()
}

struct World {
    owner: String,
    project: String,
    ws: String,
    list: String,
}

async fn world(c: &Client) -> World {
    let (owner, _, _) = c.register("owner@example.test").await;
    let project = c.first_project(&owner).await;
    let ws = id(&c
        .ok(
            "POST",
            &format!("/projects/{project}/workspaces"),
            &owner,
            Some(json!({"name": "Main"})),
        )
        .await);
    let space = id(&c
        .ok(
            "POST",
            &format!("/workspaces/{ws}/spaces"),
            &owner,
            Some(json!({"name": "Team"})),
        )
        .await);
    let list = id(&c
        .ok(
            "POST",
            &format!("/spaces/{space}/lists"),
            &owner,
            Some(json!({"name": "Backlog"})),
        )
        .await);
    World {
        owner,
        project,
        ws,
        list,
    }
}

/// A tiny HTTP server that records what it receives (stands in for an external webhook receiver).
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<(HeaderMap, Bytes)>>>);

async fn receiver() -> (String, Captured) {
    let cap = Captured::default();
    async fn handle(State(c): State<Captured>, h: HeaderMap, b: Bytes) -> StatusCode {
        c.0.lock().unwrap().push((h, b));
        StatusCode::OK
    }
    let app = Router::new()
        .route("/hook", post(handle))
        .with_state(cap.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://127.0.0.1:{port}/hook"), cap)
}

async fn wait_for<T>(mut f: impl FnMut() -> Option<T>) -> Option<T> {
    for _ in 0..80 {
        if let Some(v) = f() {
            return Some(v);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

async fn raw_post(c: &Client, path: &str, body: Vec<u8>) -> (StatusCode, Value) {
    // The public hook endpoint takes no Authorization header.
    let (st, v, _) = {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let mut req = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(
            "127.0.0.1:4242".parse::<std::net::SocketAddr>().unwrap(),
        ));
        let resp = c.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
            (),
        )
    };
    (st, v)
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn incoming_webhooks_trigger_automations(db: PgPool) {
    let c = Client::new(db);
    let w = world(&c).await;
    let auto = c
        .ok("POST", &format!("/workspaces/{}/automations", w.ws), &w.owner, Some(json!({
            "name": "From monitoring", "is_enabled": true, "trigger_type": "webhook",
            "steps": [{"action_type": "create_card", "config": {"list_id": w.list, "data": {"title": "Alert: {{trigger.body.title}}"}}}]
        })))
        .await;
    let aid = id(&auto);
    assert_eq!(auto["has_webhook"], false, "no URL until one is created");

    // The URL contains the secret and is only ever returned when it is created.
    let path = c
        .ok(
            "POST",
            &format!("/automations/{aid}/webhook-token"),
            &w.owner,
            None,
        )
        .await["path"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(path.starts_with(&format!("/api/v1/hooks/{aid}/")));
    assert_eq!(
        c.ok("GET", &format!("/automations/{aid}"), &w.owner, None)
            .await["has_webhook"],
        true
    );
    assert!(!c
        .ok("GET", &format!("/automations/{aid}"), &w.owner, None)
        .await
        .to_string()
        .contains(path.rsplit('/').next().unwrap()));

    let (st, v) = raw_post(&c, &path, br#"{"title":"Deploy failed"}"#.to_vec()).await;
    assert_eq!(st, StatusCode::ACCEPTED, "{v}");
    let run_id = v["run_id"].as_str().unwrap().to_owned();
    let mut cards = Value::Null;
    for _ in 0..60 {
        cards = c
            .ok(
                "POST",
                &format!("/lists/{}/cards/search", w.list),
                &w.owner,
                Some(json!({})),
            )
            .await;
        if cards["total"] == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(cards["cards"][0]["data"]["title"], "Alert: Deploy failed");
    let runs = c
        .ok("GET", &format!("/automations/{aid}/runs"), &w.owner, None)
        .await;
    assert_eq!(
        (runs[0]["id"].as_str(), runs[0]["status"].as_str()),
        (Some(run_id.as_str()), Some("completed")),
        "the returned run id is the real run"
    );

    // Wrong secret, rotated secret, other ids: all the same 404. Junk bodies are refused.
    let bad = path.replace(path.rsplit('/').next().unwrap(), "wrong-secret");
    assert_eq!(
        raw_post(&c, &bad, b"{}".to_vec()).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        raw_post(
            &c,
            &format!("/api/v1/hooks/{}/{}", uuid::Uuid::new_v4(), "x"),
            b"{}".to_vec()
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        raw_post(&c, &path, b"not json".to_vec()).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        raw_post(&c, &path, vec![b' '; 70_000]).await.0,
        StatusCode::NOT_FOUND,
        "oversized bodies are rejected"
    );
    let rotated = c
        .ok(
            "POST",
            &format!("/automations/{aid}/webhook-token"),
            &w.owner,
            None,
        )
        .await["path"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(rotated, path);
    assert_eq!(
        raw_post(&c, &path, b"{}".to_vec()).await.0,
        StatusCode::NOT_FOUND,
        "the old URL died with the rotation"
    );
    assert_eq!(
        raw_post(&c, &rotated, br#"{"title":"again"}"#.to_vec())
            .await
            .0,
        StatusCode::ACCEPTED
    );

    // Disabling or deleting turns the URL off; other trigger types have no URL at all.
    c.ok(
        "PATCH",
        &format!("/automations/{aid}"),
        &w.owner,
        Some(json!({"is_enabled": false})),
    )
    .await;
    assert_eq!(
        raw_post(&c, &rotated, b"{}".to_vec()).await.0,
        StatusCode::NOT_FOUND
    );
    let other = c.ok("POST", &format!("/workspaces/{}/automations", w.ws), &w.owner, Some(json!({"name": "x", "trigger_type": "card_created", "steps": [{"action_type": "create_comment", "config": {"content": "x"}}]}))).await;
    assert_eq!(
        c.api(
            "POST",
            &format!("/automations/{}/webhook-token", id(&other)),
            &w.owner,
            None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );

    // Only admins manage hooks, and an API token cannot mint hook URLs.
    let (member, _) = c
        .join(&w.owner, &w.project, "member@example.test", "member")
        .await;
    assert_eq!(
        c.api(
            "POST",
            &format!("/automations/{aid}/webhook-token"),
            &member,
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let api = c
        .ok(
            "POST",
            "/me/api-tokens",
            &w.owner,
            Some(json!({"name": "t", "scope": "write"})),
        )
        .await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        c.api(
            "POST",
            &format!("/automations/{aid}/webhook-token"),
            &api,
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        c.api(
            "GET",
            &format!("/automations/{aid}/signing-secret"),
            &api,
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    // A webhook can act on an existing issue by naming it; issues of other workspaces are refused.
    let card = id(&c
        .ok(
            "POST",
            &format!("/lists/{}/cards", w.list),
            &w.owner,
            Some(json!({"data": {"title": "Existing"}})),
        )
        .await);
    let tagger = c.ok("POST", &format!("/workspaces/{}/automations", w.ws), &w.owner, Some(json!({
        "name": "Tag it", "is_enabled": true, "trigger_type": "webhook",
        "steps": [{"action_type": "create_comment", "config": {"content": "Build {{trigger.body.build}} finished for {{trigger.card.key}}"}}]}))).await;
    let tp = c
        .ok(
            "POST",
            &format!("/automations/{}/webhook-token", id(&tagger)),
            &w.owner,
            None,
        )
        .await["path"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        raw_post(
            &c,
            &tp,
            serde_json::to_vec(&json!({"card_id": card, "build": 42})).unwrap()
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );
    let mut found = false;
    for _ in 0..60 {
        let acts = c
            .ok(
                "GET",
                &format!("/cards/{card}/activities?type=comment"),
                &w.owner,
                None,
            )
            .await;
        found = acts.as_array().unwrap().iter().any(|a| {
            a["content"]["content"]
                .as_str()
                .is_some_and(|t| t.starts_with("Build 42 finished for "))
        });
        if found {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(found, "comment created through the webhook");
    let (_, _, _) = (0, 0, 0);
    let (other_owner, _, _) = c.register("other@example.test").await;
    let other_project = c.first_project(&other_owner).await;
    let other_ws = id(&c
        .ok(
            "POST",
            &format!("/projects/{other_project}/workspaces"),
            &other_owner,
            Some(json!({"name": "Other"})),
        )
        .await);
    let other_space = id(&c
        .ok(
            "POST",
            &format!("/workspaces/{other_ws}/spaces"),
            &other_owner,
            Some(json!({"name": "S"})),
        )
        .await);
    let other_list = id(&c
        .ok(
            "POST",
            &format!("/spaces/{other_space}/lists"),
            &other_owner,
            Some(json!({"name": "L"})),
        )
        .await);
    let foreign = id(&c
        .ok(
            "POST",
            &format!("/lists/{other_list}/cards"),
            &other_owner,
            Some(json!({"data": {"title": "Foreign"}})),
        )
        .await);
    assert_eq!(
        raw_post(
            &c,
            &tp,
            serde_json::to_vec(&json!({"card_id": foreign})).unwrap()
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    // Actions that need an issue fail, visibly, when the call names none.
    assert_eq!(
        raw_post(&c, &tp, b"{}".to_vec()).await.0,
        StatusCode::ACCEPTED
    );
    let mut failed = None;
    for _ in 0..60 {
        let runs = c
            .ok(
                "GET",
                &format!("/automations/{}/runs", id(&tagger)),
                &w.owner,
                None,
            )
            .await;
        failed = runs
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["status"] == "failed")
            .cloned();
        if failed.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(failed.unwrap()["error"]
        .as_str()
        .unwrap()
        .contains("needs a card"));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn outgoing_webhooks_are_signed(db: PgPool) {
    let c = Client::new(db); // test config allows loopback targets
    let w = world(&c).await;
    let (url, captured) = receiver().await;
    let auto = c
        .ok("POST", &format!("/workspaces/{}/automations", w.ws), &w.owner, Some(json!({
            "name": "Notify chat", "is_enabled": true, "trigger_type": "card_created",
            "steps": [{"action_type": "send_webhook", "config": {"url": url, "body": {"issue": "{{trigger.card.key}}", "title": "{{trigger.card.data.title}}"}}}]
        })))
        .await;
    c.ok(
        "POST",
        &format!("/lists/{}/cards", w.list),
        &w.owner,
        Some(json!({"data": {"title": "Ship it"}})),
    )
    .await;
    let (headers, body) = wait_for(|| captured.0.lock().unwrap().first().cloned())
        .await
        .expect("the receiver was called");

    let payload: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["event"], "automation.run");
    assert_eq!(payload["data"]["title"], "Ship it");
    assert!(payload["data"]["issue"].as_str().unwrap().ends_with("-1"));
    assert_eq!(headers["x-work-event"], "automation.run");
    assert_eq!(
        headers["x-work-delivery"].to_str().unwrap(),
        payload["run_id"].as_str().unwrap()
    );

    // The receiver can verify the payload with the secret shown to the admin.
    let secret = c
        .ok(
            "GET",
            &format!("/automations/{}/signing-secret", id(&auto)),
            &w.owner,
            None,
        )
        .await["secret"]
        .as_str()
        .unwrap()
        .to_owned();
    let expected = crate::automations::sign(&hex::decode(secret).unwrap(), &body);
    assert_eq!(headers["x-work-signature"].to_str().unwrap(), expected);
    let (m, _) = c
        .join(&w.owner, &w.project, "member@example.test", "member")
        .await;
    assert_eq!(
        c.api(
            "GET",
            &format!("/automations/{}/signing-secret", id(&auto)),
            &m,
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let mut ok = false;
    for _ in 0..60 {
        let runs = c
            .ok(
                "GET",
                &format!("/automations/{}/runs", id(&auto)),
                &w.owner,
                None,
            )
            .await;
        ok = runs[0]["status"] == "completed";
        if ok {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ok);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn outgoing_webhooks_cannot_reach_internal_addresses(db: PgPool) {
    // Production defaults: https only, public addresses only.
    let c = Client::new_with(db, |cfg| {
        cfg.webhook_allow_private = false;
        cfg.webhook_allow_http = false;
    });
    let w = world(&c).await;
    let (url, captured) = receiver().await; // reachable on 127.0.0.1, so a successful call would show up here
    let create = |u: &str| {
        let (c, w) = (&c, &w);
        let u = u.to_owned();
        async move {
            c.api(
                "POST",
                &format!("/workspaces/{}/automations", w.ws),
                &w.owner,
                Some(json!({
                "name": "x", "is_enabled": true, "trigger_type": "card_created",
                "steps": [{"action_type": "send_webhook", "config": {"url": u}}]})),
            )
            .await
        }
    };
    // Rejected when saving: wrong scheme, credentials in the URL, non-URLs.
    for bad in [
        url.as_str(),
        "ftp://example.com/x",
        "file:///etc/passwd",
        "https://user:pass@example.com/x",
        "not a url",
    ] {
        assert_eq!(create(bad).await.0, StatusCode::BAD_REQUEST, "{bad}");
    }
    // https to an internal address passes the syntax check but is refused when the run happens.
    for internal in [
        "https://127.0.0.1:9/hook",
        "https://169.254.169.254/latest/meta-data/",
        "https://[::1]/x",
        "https://localhost/x",
        "https://10.0.0.5/x",
    ] {
        let (st, a) = create(internal).await;
        assert_eq!(st, StatusCode::OK, "{internal}: {a}");
    }
    c.ok(
        "POST",
        &format!("/lists/{}/cards", w.list),
        &w.owner,
        Some(json!({"data": {"title": "trigger"}})),
    )
    .await;
    let autos = c
        .ok(
            "GET",
            &format!("/workspaces/{}/automations", w.ws),
            &w.owner,
            None,
        )
        .await;
    let mut all_failed = false;
    for _ in 0..80 {
        let mut statuses = vec![];
        for a in autos.as_array().unwrap() {
            let runs = c
                .ok(
                    "GET",
                    &format!("/automations/{}/runs", id(a)),
                    &w.owner,
                    None,
                )
                .await;
            statuses.push(runs.as_array().unwrap().first().map(|r| {
                (
                    r["status"].as_str().unwrap().to_owned(),
                    r["error"].as_str().unwrap_or("").to_owned(),
                )
            }));
        }
        if statuses
            .iter()
            .all(|s| s.as_ref().is_some_and(|(st, _)| st != "running"))
        {
            all_failed = statuses.iter().all(|s| {
                s.as_ref().is_some_and(|(st, e)| {
                    st == "failed" && (e.contains("public") || e.contains("resolve"))
                })
            });
            assert!(all_failed, "{statuses:?}");
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(all_failed);
    assert!(
        captured.0.lock().unwrap().is_empty(),
        "nothing reached the internal receiver"
    );
}

// ---- e-mail flows (need Mailpit) --------------------------------------------------------------

fn mailpit() -> Option<String> {
    std::env::var("TEST_MAILPIT_API")
        .ok()
        .filter(|_| std::env::var("TEST_SMTP_PORT").is_ok())
}

/// Waits for a message to `to` and returns its plain-text body.
async fn mail_to(api: &str, to: &str) -> Option<(String, String)> {
    mail_parts(api, to)
        .await
        .map(|(s, t, h)| (s, format!("{t}\n{h}")))
}

/// Subject, plain-text part and HTML part.
async fn mail_parts(api: &str, to: &str) -> Option<(String, String, String)> {
    let http = reqwest::Client::new();
    for _ in 0..100 {
        let found: Value = http
            .get(format!("{api}/api/v1/search"))
            .query(&[("query", format!("to:{to}"))])
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;
        if let Some(m) = found["messages"].as_array().and_then(|a| a.first()) {
            let full: Value = http
                .get(format!("{api}/api/v1/message/{}", m["ID"].as_str()?))
                .send()
                .await
                .ok()?
                .json()
                .await
                .ok()?;
            return Some((
                full["Subject"].as_str()?.to_owned(),
                full["Text"].as_str().unwrap_or("").to_owned(),
                full["HTML"].as_str().unwrap_or("").to_owned(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

/// Mailpit keeps messages between runs, so every test uses fresh addresses.
fn uniq(local: &str) -> String {
    format!("{local}-{}@example.test", uuid::Uuid::new_v4().simple())
}

fn token_in(text: &str) -> String {
    let start = text.find("token=").expect("a link with a token") + 6;
    text[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL and Mailpit"]
async fn password_reset_by_email(db: PgPool) {
    let Some(api) = mailpit() else {
        eprintln!("skipped: no Mailpit");
        return;
    };
    let c = Client::new(db);
    let forgot = uniq("forgot");
    let nobody = uniq("nobody");
    let (token, cookie, _) = c.register(&forgot).await;
    let info = c.call("GET", "/system/info", None, None, None).await.1;
    assert_eq!(info["mail_enabled"], true);

    // Unknown and known addresses get the same answer; only the known one receives mail.
    let (st, v, _) = c
        .call(
            "POST",
            "/auth/password-reset/request",
            None,
            None,
            Some(json!({"email": nobody})),
        )
        .await;
    assert_eq!((st, v["ok"].as_bool()), (StatusCode::OK, Some(true)));
    let (st, _, _) = c
        .call(
            "POST",
            "/auth/password-reset/request",
            None,
            None,
            Some(json!({"email": forgot.to_uppercase()})),
        )
        .await;
    assert_eq!(st, StatusCode::OK);
    let (subject, text) = mail_to(&api, &forgot).await.expect("reset mail");
    assert!(subject.contains("Reset"));
    assert!(
        mail_to(&api, &nobody).await.is_none(),
        "no mail for unknown addresses"
    );
    let reset = token_in(&text);

    // Weak passwords and wrong tokens fail without consuming the real token.
    let confirm = |t: &str, pw: &str| {
        let c = &c;
        let body = json!({"token": t, "new_password": pw});
        async move {
            c.call(
                "POST",
                "/auth/password-reset/confirm",
                None,
                None,
                Some(body),
            )
            .await
            .0
        }
    };
    assert_eq!(confirm(&reset, "short").await, StatusCode::BAD_REQUEST);
    assert_eq!(
        confirm("not-a-token", "a-brand-new-password").await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        confirm(&reset, "a-brand-new-password").await,
        StatusCode::OK
    );
    assert_eq!(
        confirm(&reset, "yet-another-password").await,
        StatusCode::BAD_REQUEST,
        "single use"
    );

    // New password works, the old one and all old sessions do not.
    let login = |pw: &str| {
        let c = &c;
        let body = json!({"email": forgot, "password": pw});
        async move {
            c.call("POST", "/auth/login", None, None, Some(body))
                .await
                .0
        }
    };
    assert_eq!(
        login("correct-horse-battery").await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(login("a-brand-new-password").await, StatusCode::OK);
    assert_eq!(
        c.api("GET", "/me", &token, None).await.0,
        StatusCode::UNAUTHORIZED,
        "sessions were revoked"
    );
    assert_eq!(
        c.call("POST", "/auth/refresh", None, Some(&cookie), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL and Mailpit"]
async fn invite_by_email_is_bound_to_the_address(db: PgPool) {
    let Some(api) = mailpit() else {
        eprintln!("skipped: no Mailpit");
        return;
    };
    let c = Client::new(db);
    let guest_mail = uniq("guest");
    let (owner, _, _) = c.register("owner@example.test").await;
    let project = c.first_project(&owner).await;

    assert_eq!(
        c.api(
            "POST",
            &format!("/projects/{project}/invites/email"),
            &owner,
            Some(json!({"email": "owner@example.test", "role": "owner"}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        c.api(
            "POST",
            &format!("/projects/{project}/invites/email"),
            &owner,
            Some(json!({"email": "not an address", "role": "member"}))
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    c.ok(
        "POST",
        &format!("/projects/{project}/invites/email"),
        &owner,
        Some(json!({"email": guest_mail.replacen("guest", "Guest", 1), "role": "member"})),
    )
    .await;
    let (subject, text) = mail_to(&api, &guest_mail).await.expect("invitation mail");
    assert!(subject.contains("invited you"));
    let invite = token_in(&text);

    // Somebody else with the link cannot use it (and it is not consumed by the attempt).
    let (thief, _, _) = c.register("thief@example.test").await;
    assert_eq!(
        c.api(
            "POST",
            "/invites/accept",
            &thief,
            Some(json!({"token": invite}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (guest, _, _) = c.register(&guest_mail).await;
    c.ok(
        "POST",
        "/invites/accept",
        &guest,
        Some(json!({"token": invite})),
    )
    .await;
    assert_eq!(
        c.ok("GET", &format!("/projects/{project}"), &guest, None)
            .await["role"],
        "member"
    );
    assert_eq!(
        c.api(
            "POST",
            "/invites/accept",
            &guest,
            Some(json!({"token": invite}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND,
        "single use"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL and Mailpit"]
async fn automations_send_mail_only_to_project_members(db: PgPool) {
    let Some(api) = mailpit() else {
        eprintln!("skipped: no Mailpit");
        return;
    };
    let c = Client::new(db);
    let teammate = uniq("teammate");
    let external = format!("someone-{}@elsewhere.test", uuid::Uuid::new_v4().simple());
    let w = world(&c).await;
    let (_, member_id) = c.join(&w.owner, &w.project, &teammate, "member").await;
    let mk = |to: &str| {
        let (c, w) = (&c, &w);
        let to = to.to_owned();
        async move {
            c.ok("POST", &format!("/workspaces/{}/automations", w.ws), &w.owner, Some(json!({
                "name": format!("mail {to}"), "is_enabled": true, "trigger_type": "card_created",
                "steps": [{"action_type": "send_email", "config": {"to": to, "subject": "New issue {{trigger.card.key}}", "body": "<script>alert(1)</script>\n{{trigger.card.data.title}}"}}]}))).await
        }
    };
    let a_member = mk(&member_id).await;
    let a_external = mk(&external).await;
    c.ok(
        "POST",
        &format!("/lists/{}/cards", w.list),
        &w.owner,
        Some(json!({"data": {"title": "Printer on fire"}})),
    )
    .await;

    let (subject, _text, html) = mail_parts(&api, &teammate)
        .await
        .expect("mail to the member");
    assert!(subject.starts_with("New issue "), "{subject}");
    assert!(html.contains("Printer on fire"));
    assert!(
        html.contains("&lt;script&gt;") && !html.contains("<script>"),
        "the body is escaped in the HTML part: {html}"
    );
    let mut external_failed = false;
    for _ in 0..80 {
        let runs = c
            .ok(
                "GET",
                &format!("/automations/{}/runs", id(&a_external)),
                &w.owner,
                None,
            )
            .await;
        if let Some(r) = runs.as_array().and_then(|r| r.first()) {
            if r["status"] != "running" {
                external_failed = r["status"] == "failed"
                    && r["error"].as_str().unwrap().contains("not a member");
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        external_failed,
        "the automation cannot be used as a mail relay"
    );
    assert!(mail_to(&api, &external).await.is_none());
    let _ = a_member;
}
