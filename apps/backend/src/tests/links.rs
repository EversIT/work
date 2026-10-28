//! Links between issues, end to end. They need a Postgres (`DATABASE_URL`).

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::tests::harness::Client;

struct World {
    owner: String,
    project: String,
    ws: String,
    list: String,
    card_type: String,
}

async fn workspace(c: &Client, owner: &str, project: &str, name: &str) -> (String, String, String) {
    let ws = c
        .ok(
            "POST",
            &format!("/projects/{project}/workspaces"),
            owner,
            Some(json!({"name": name})),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let space = c
        .ok(
            "POST",
            &format!("/workspaces/{ws}/spaces"),
            owner,
            Some(json!({"name": "Team"})),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let list = c
        .ok(
            "POST",
            &format!("/spaces/{space}/lists"),
            owner,
            Some(json!({"name": "Backlog"})),
        )
        .await;
    let card_type = list["default_card_type_id"].as_str().unwrap().to_owned();
    (ws, list["id"].as_str().unwrap().to_owned(), card_type)
}

async fn world(c: &Client) -> World {
    let (owner, _, _) = c.register("owner@example.test").await;
    let project = c.first_project(&owner).await;
    let (ws, list, card_type) = workspace(c, &owner, &project, "Main").await;
    World {
        owner,
        project,
        ws,
        list,
        card_type,
    }
}

async fn card(c: &Client, token: &str, list: &str, card_type: &str, title: &str) -> String {
    c.ok(
        "POST",
        &format!("/lists/{list}/cards"),
        token,
        Some(json!({"card_type_id": card_type, "data": {"title": title}})),
    )
    .await["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn links(c: &Client, token: &str, card: &str) -> Vec<Value> {
    c.ok("GET", &format!("/cards/{card}/links"), token, None)
        .await
        .as_array()
        .unwrap()
        .clone()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn a_link_reads_from_both_ends_and_can_be_removed(db: PgPool) {
    let c = Client::new(db);
    let w = world(&c).await;
    let a = card(&c, &w.owner, &w.list, &w.card_type, "A").await;
    let b = card(&c, &w.owner, &w.list, &w.card_type, "B").await;

    let made = c
        .ok(
            "POST",
            &format!("/cards/{a}/links"),
            &w.owner,
            Some(json!({"link_type": "blocks", "target_id": b})),
        )
        .await;
    assert_eq!(made["link_type"], "blocks");
    assert_eq!(made["direction"], "outward");
    assert_eq!(made["card"]["id"], b.as_str());

    // The other end sees the same row worded the other way round.
    let from_b = links(&c, &w.owner, &b).await;
    assert_eq!(from_b.len(), 1);
    assert_eq!(from_b[0]["direction"], "inward");
    assert_eq!(from_b[0]["card"]["id"], a.as_str());
    assert_eq!(from_b[0]["id"], made["id"]);

    // "inward" creates the same relation from the other side: B is blocked by C.
    let cc = card(&c, &w.owner, &w.list, &w.card_type, "C").await;
    let inward = c
        .ok(
            "POST",
            &format!("/cards/{b}/links"),
            &w.owner,
            Some(json!({"link_type": "blocks", "direction": "inward", "target_id": cc})),
        )
        .await;
    assert_eq!(inward["direction"], "inward");
    assert_eq!(links(&c, &w.owner, &cc).await[0]["direction"], "outward");

    // Both issues got a history entry.
    let acts = c
        .ok("GET", &format!("/cards/{a}/activities"), &w.owner, None)
        .await
        .to_string();
    assert!(acts.contains("\"link\""), "{acts}");

    // Removing it from either end removes it for both.
    let id = made["id"].as_str().unwrap();
    c.ok("DELETE", &format!("/cards/{b}/links/{id}"), &w.owner, None)
        .await;
    assert!(links(&c, &w.owner, &a).await.is_empty());
    let (st, _) = c
        .api("DELETE", &format!("/cards/{a}/links/{id}"), &w.owner, None)
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn bad_links_are_refused(db: PgPool) {
    let c = Client::new(db);
    let w = world(&c).await;
    let a = card(&c, &w.owner, &w.list, &w.card_type, "A").await;
    let b = card(&c, &w.owner, &w.list, &w.card_type, "B").await;
    let post = |card: String, body: Value| {
        let (c, t) = (c.clone_client(), w.owner.clone());
        async move {
            c.api("POST", &format!("/cards/{card}/links"), &t, Some(body))
                .await
                .0
        }
    };

    assert_eq!(
        post(a.clone(), json!({"link_type": "blocks", "target_id": a})).await,
        StatusCode::BAD_REQUEST,
        "self link"
    );
    assert_eq!(
        post(a.clone(), json!({"link_type": "likes", "target_id": b})).await,
        StatusCode::BAD_REQUEST,
        "unknown type"
    );
    assert_eq!(
        post(
            a.clone(),
            json!({"link_type": "relates", "direction": "up", "target_id": b})
        )
        .await,
        StatusCode::BAD_REQUEST,
        "unknown direction"
    );
    assert_eq!(
        post(a.clone(), json!({"link_type": "blocks", "target_id": b})).await,
        StatusCode::OK
    );
    // The same pair and type again, in either direction, contradicts or repeats the first.
    assert_eq!(
        post(a.clone(), json!({"link_type": "blocks", "target_id": b})).await,
        StatusCode::CONFLICT
    );
    assert_eq!(
        post(b.clone(), json!({"link_type": "blocks", "target_id": a})).await,
        StatusCode::CONFLICT
    );
    // A different type between the same two issues is fine.
    assert_eq!(
        post(a.clone(), json!({"link_type": "relates", "target_id": b})).await,
        StatusCode::OK
    );

    // Another workspace of the same project is out of reach.
    let (_, list2, type2) = workspace(&c, &w.owner, &w.project, "Other").await;
    let other = card(&c, &w.owner, &list2, &type2, "X").await;
    assert_eq!(
        post(
            a.clone(),
            json!({"link_type": "relates", "target_id": other})
        )
        .await,
        StatusCode::BAD_REQUEST
    );
    let _ = &w.ws;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn only_members_link_and_deleted_issues_disappear(db: PgPool) {
    let c = Client::new(db);
    let w = world(&c).await;
    let a = card(&c, &w.owner, &w.list, &w.card_type, "A").await;
    let b = card(&c, &w.owner, &w.list, &w.card_type, "B").await;
    c.ok(
        "POST",
        &format!("/cards/{a}/links"),
        &w.owner,
        Some(json!({"link_type": "relates", "target_id": b})),
    )
    .await;

    // A viewer reads links but cannot change them.
    let (viewer, _) = c
        .join(&w.owner, &w.project, "viewer@example.test", "viewer")
        .await;
    assert_eq!(links(&c, &viewer, &a).await.len(), 1);
    let (st, _) = c
        .api(
            "POST",
            &format!("/cards/{a}/links"),
            &viewer,
            Some(json!({"link_type": "blocks", "target_id": b})),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let id = links(&c, &w.owner, &a).await[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (st, _) = c
        .api("DELETE", &format!("/cards/{a}/links/{id}"), &viewer, None)
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // A deleted issue no longer shows up as a link.
    c.ok("DELETE", &format!("/cards/{b}"), &w.owner, None).await;
    assert!(links(&c, &w.owner, &a).await.is_empty());
}
