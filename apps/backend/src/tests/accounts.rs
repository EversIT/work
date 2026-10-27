//! Sign-up, sessions, invitations and tenant isolation.

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use super::harness::Client;

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
