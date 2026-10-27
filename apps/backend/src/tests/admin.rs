//! The admin panel's API: who may use it, branding, user management and the audit log.

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::tests::harness::Client;

const PNG: [u8; 12] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4];

async fn put_raw(c: &Client, token: &str, uri: &str, body: Vec<u8>) -> StatusCode {
    c.raw("PUT", uri, token, "application/octet-stream", body)
        .await
        .status()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn only_admins_get_in_and_everything_they_change_is_logged(db: PgPool) {
    let c = Client::new(db.clone());
    // The first account of an instance is its administrator; later ones are not.
    let (admin, _, admin_id) = c.register("admin@example.test").await;
    let (user, _, user_id) = c.register("user@example.test").await;
    assert_eq!(
        c.ok("GET", "/me", &admin, None).await["is_instance_admin"],
        true
    );
    assert_eq!(
        c.ok("GET", "/me", &user, None).await["is_instance_admin"],
        false
    );

    // The admin area does not exist for everybody else: 404, not 403.
    for (method, uri) in [
        ("GET", "/admin/overview"),
        ("GET", "/admin/users"),
        ("GET", "/admin/projects"),
        ("GET", "/admin/audit"),
    ] {
        assert_eq!(
            c.api(method, uri, &user, None).await.0,
            StatusCode::NOT_FOUND,
            "{uri}"
        );
        assert_eq!(
            c.call(method, uri, None, None, None).await.0,
            StatusCode::UNAUTHORIZED,
            "{uri}"
        );
    }
    assert_eq!(
        c.api(
            "PATCH",
            "/admin/branding",
            &user,
            Some(json!({"title": "Mine"}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        put_raw(&c, &user, "/admin/branding/favicon", PNG.to_vec()).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        c.api(
            "PATCH",
            &format!("/admin/users/{admin_id}"),
            &user,
            Some(json!({"disabled": true}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    // An API token of an admin is not a session either.
    let created = c
        .ok(
            "POST",
            "/me/api-tokens",
            &admin,
            Some(json!({"name": "ci", "scope": "write"})),
        )
        .await;
    let api_token = created["token"].as_str().unwrap();
    assert_eq!(
        c.api("GET", "/admin/overview", api_token, None).await.0,
        StatusCode::FORBIDDEN
    );

    // Overview: counts and the (secret-free) configuration.
    let o = c.ok("GET", "/admin/overview", &admin, None).await;
    assert_eq!(
        (
            o["counts"]["users"].as_i64(),
            o["counts"]["admins"].as_i64(),
            o["counts"]["projects"].as_i64()
        ),
        (Some(2), Some(1), Some(2))
    );
    assert_eq!(o["config"]["storage"], "local");
    let text = o.to_string();
    for secret in ["secret", "password", "jwt", "token"] {
        assert!(
            !text.to_lowercase().contains(&format!("\"{secret}")),
            "{text}"
        );
    }

    // Branding: public to read, admin-only to change, validated.
    let b = c.call("GET", "/branding", None, None, None).await.1;
    assert_eq!(
        (b["title"].as_str(), b["has_favicon"].as_bool()),
        (Some("work"), Some(false))
    );
    assert_eq!(
        c.call("GET", "/branding/favicon", None, None, None).await.0,
        StatusCode::NOT_FOUND
    );
    for bad in [
        json!({"title": ""}),
        json!({"title": "x".repeat(61)}),
        json!({"accent": "red"}),
        json!({"accent": "#12345"}),
        json!({"description": "x".repeat(301)}),
    ] {
        assert_eq!(
            c.api("PATCH", "/admin/branding", &admin, Some(bad.clone()))
                .await
                .0,
            StatusCode::BAD_REQUEST,
            "{bad}"
        );
    }
    let b = c
        .ok(
            "PATCH",
            "/admin/branding",
            &admin,
            Some(
                json!({"title": "  Acme Work ", "description": "Our tracker", "accent": "#1f845a"}),
            ),
        )
        .await;
    assert_eq!(
        (b["title"].as_str(), b["accent"].as_str()),
        (Some("Acme Work"), Some("#1F845A"))
    );
    assert_eq!(
        c.call("GET", "/branding", None, None, None).await.1["title"],
        "Acme Work"
    );

    // Favicon: only real, passive images; served with a locked-down policy; removable.
    assert_eq!(
        put_raw(
            &c,
            &admin,
            "/admin/branding/favicon",
            b"<html></html>".to_vec()
        )
        .await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        put_raw(
            &c,
            &admin,
            "/admin/branding/favicon",
            b"<svg><script>alert(1)</script></svg>".to_vec()
        )
        .await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        put_raw(&c, &admin, "/admin/branding/favicon", vec![0x89; 300_000]).await,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        put_raw(&c, &admin, "/admin/branding/favicon", PNG.to_vec()).await,
        StatusCode::OK
    );
    let (st, _, resp) = c.call("GET", "/branding/favicon", None, None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(resp.headers()["content-type"], "image/png");
    assert!(resp.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .contains("sandbox"));
    assert_eq!(resp.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        c.call("GET", "/branding", None, None, None).await.1["has_favicon"],
        true
    );
    c.ok("DELETE", "/admin/branding/favicon", &admin, None)
        .await;
    assert_eq!(
        c.call("GET", "/branding/favicon", None, None, None).await.0,
        StatusCode::NOT_FOUND
    );

    // Users: search, and the guard rails of changing them.
    let page = c
        .ok("GET", "/admin/users?query=USER%40", &admin, None)
        .await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["users"][0]["email"], "user@example.test");
    assert_eq!(
        c.ok("GET", "/admin/users?query=%25", &admin, None).await["total"],
        0,
        "wildcards are text, not patterns"
    );
    assert_eq!(
        c.api(
            "PATCH",
            &format!("/admin/users/{admin_id}"),
            &admin,
            Some(json!({"disabled": true}))
        )
        .await
        .0,
        StatusCode::CONFLICT,
        "not yourself"
    );
    assert_eq!(
        c.api(
            "PATCH",
            &format!("/admin/users/{admin_id}"),
            &admin,
            Some(json!({"is_instance_admin": false}))
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        c.api(
            "PATCH",
            &format!("/admin/users/{}", uuid::Uuid::new_v4()),
            &admin,
            Some(json!({"disabled": true}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    c.ok(
        "PATCH",
        &format!("/admin/users/{user_id}"),
        &admin,
        Some(json!({"is_instance_admin": true})),
    )
    .await;
    assert_eq!(
        c.api("GET", "/admin/overview", &user, None).await.0,
        StatusCode::OK,
        "a new admin has access at once"
    );
    // With two admins, one may step down; the last one never.
    c.ok(
        "PATCH",
        &format!("/admin/users/{admin_id}"),
        &user,
        Some(json!({"is_instance_admin": false})),
    )
    .await;
    assert_eq!(
        c.api("GET", "/admin/overview", &admin, None).await.0,
        StatusCode::NOT_FOUND
    );
    c.ok(
        "PATCH",
        &format!("/admin/users/{admin_id}"),
        &user,
        Some(json!({"is_instance_admin": true})),
    )
    .await;

    // Disabling ends sessions and blocks every way back in; enabling restores it.
    let (victim, victim_cookie, victim_id) = c.register("victim@example.test").await;
    c.ok(
        "PATCH",
        &format!("/admin/users/{victim_id}"),
        &admin,
        Some(json!({"disabled": true})),
    )
    .await;
    assert_eq!(
        c.api("GET", "/me", &victim, None).await.0,
        StatusCode::UNAUTHORIZED,
        "the session ended"
    );
    assert_eq!(
        c.call("POST", "/auth/refresh", None, Some(&victim_cookie), None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let login = |pw: &str| {
        let body = json!({"email": "victim@example.test", "password": pw});
        let c = &c;
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
    c.ok(
        "PATCH",
        &format!("/admin/users/{victim_id}"),
        &admin,
        Some(json!({"disabled": false})),
    )
    .await;
    assert_eq!(login("correct-horse-battery").await, StatusCode::OK);

    // Unlocking clears a lock-out.
    for _ in 0..5 {
        login("wrong-password-here").await;
    }
    assert_eq!(
        login("correct-horse-battery").await,
        StatusCode::UNAUTHORIZED,
        "locked"
    );
    assert_eq!(
        c.ok("GET", "/admin/users?query=victim", &admin, None).await["users"][0]["locked"],
        true
    );
    c.ok(
        "PATCH",
        &format!("/admin/users/{victim_id}"),
        &admin,
        Some(json!({"unlock": true})),
    )
    .await;
    assert_eq!(login("correct-horse-battery").await, StatusCode::OK);

    // Projects overview.
    let projects = c.ok("GET", "/admin/projects", &admin, None).await;
    assert_eq!(projects.as_array().unwrap().len(), 3);
    assert!(projects[0]["owner_email"].is_string());

    // The audit log names who did what, newest first.
    let log = c.ok("GET", "/admin/audit", &admin, None).await;
    let actions: Vec<&str> = log
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["action"].as_str().unwrap())
        .collect();
    assert!(
        actions.contains(&"branding.update")
            && actions.contains(&"branding.favicon")
            && actions.contains(&"user.update"),
        "{actions:?}"
    );
    assert_eq!(log[0]["actor_email"], "admin@example.test");
    let _: Value = log;
}
