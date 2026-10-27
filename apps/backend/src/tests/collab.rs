//! Rich text documents and collaborative editing.

use axum::http::StatusCode;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use yrs::{
    updates::{decoder::Decode, encoder::Encode},
    Doc, GetString, ReadTxn, StateVector, Text, Transact, Update,
};

use crate::{services::collab::room, tests::harness::Client};

struct Fixture {
    c: Client,
    owner: String,
    member: String,
    viewer: String,
    stranger: String,
    card: String,
}

async fn fixture(db: PgPool) -> Fixture {
    let c = Client::new(db);
    let (owner, _, _) = c.register("owner@example.test").await;
    let project = c.first_project(&owner).await;
    let ws = c
        .ok(
            "POST",
            &format!("/projects/{project}/workspaces"),
            &owner,
            Some(json!({"name": "Main"})),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let space = c
        .ok(
            "POST",
            &format!("/workspaces/{ws}/spaces"),
            &owner,
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
            &owner,
            Some(json!({"name": "Backlog"})),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let card = c
        .ok(
            "POST",
            &format!("/lists/{list}/cards"),
            &owner,
            Some(json!({"data": {"title": "Spec"}})),
        )
        .await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (member, _) = c
        .join(&owner, &project, "member@example.test", "member")
        .await;
    let (viewer, _) = c
        .join(&owner, &project, "viewer@example.test", "viewer")
        .await;
    let (stranger, _, _) = c.register("stranger@example.test").await;
    Fixture {
        c,
        owner,
        member,
        viewer,
        stranger,
        card,
    }
}

fn b64(b: &[u8]) -> String {
    STANDARD.encode(b)
}

fn client_doc(initial: Option<&[u8]>) -> Doc {
    let d = Doc::new();
    d.get_or_insert_text("t");
    if let Some(u) = initial {
        d.transact_mut()
            .apply_update(Update::decode_v1(u).unwrap())
            .unwrap();
    }
    d
}

fn full_state(d: &Doc) -> Vec<u8> {
    d.transact()
        .encode_state_as_update_v1(&StateVector::default())
}

fn text(d: &Doc) -> String {
    let t = d.get_or_insert_text("t");
    let txn = d.transact();
    t.get_string(&txn)
}

/// Types into the document and returns the update that describes just that change.
fn type_at(d: &Doc, at: u32, s: &str) -> Vec<u8> {
    let before = d.transact().state_vector();
    let t = d.get_or_insert_text("t");
    t.insert(&mut d.transact_mut(), at, s);
    let txn = d.transact();
    txn.encode_state_as_update_v1(&before)
}

async fn sync(f: &Fixture, who: &str, slug: &str, body: Value) -> (StatusCode, Value) {
    f.c.api(
        "POST",
        &format!("/cards/{}/collab/{slug}/sync", f.card),
        who,
        Some(body),
    )
    .await
}

fn pulled(out: &Value) -> Doc {
    let d = client_doc(None);
    let bytes = STANDARD.decode(out["update"].as_str().unwrap()).unwrap();
    if !bytes.is_empty() {
        d.transact_mut()
            .apply_update(Update::decode_v1(&bytes).unwrap())
            .unwrap();
    }
    d
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn editors_converge_content_never_doubles_and_roles_are_enforced(db: PgPool) {
    let f = fixture(db).await;
    let card: Uuid = f.card.parse().unwrap();

    // Nothing there yet: a read says so, a viewer cannot seed, the member's seed creates it.
    let (st, out) = sync(&f, &f.viewer, "description", json!({})).await;
    assert_eq!((st, out["empty"].clone()), (StatusCode::OK, json!(true)));
    let seed_a = full_state(&{
        let d = client_doc(None);
        type_at(&d, 0, "hello");
        d
    });
    assert_eq!(
        sync(&f, &f.viewer, "description", json!({"seed": b64(&seed_a)}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (st, out) = sync(&f, &f.member, "description", json!({"seed": b64(&seed_a)})).await;
    assert_eq!(
        (st, out["empty"].clone()),
        (StatusCode::OK, json!(false)),
        "{out}"
    );

    // A second editor that seeds at the same time is ignored: the content does not double.
    let seed_b = full_state(&{
        let d = client_doc(None);
        type_at(&d, 0, "hello");
        d
    });
    let (_, out) = sync(&f, &f.owner, "description", json!({"seed": b64(&seed_b)})).await;
    assert_eq!(text(&pulled(&out)), "hello");

    // Both editors continue from the server's state and type at the same moment.
    let a = pulled(&out);
    let b = pulled(&sync(&f, &f.member, "description", json!({})).await.1);
    let mut events = f.c.state.hub.subscribe(room(card, "description"));
    let ua = type_at(&a, 5, " world");
    let ub = type_at(&b, 0, ">> ");
    let (st, _) = sync(&f, &f.owner, "description", json!({"update": b64(&ua)})).await;
    assert_eq!(st, StatusCode::OK);
    let (st, out) = sync(
        &f,
        &f.member,
        "description",
        json!({"update": b64(&ub), "state_vector": b64(&b.transact().state_vector().encode_v1())}),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    // The answer to the second push already carries the first editor's change.
    b.transact_mut()
        .apply_update(
            Update::decode_v1(&STANDARD.decode(out["update"].as_str().unwrap()).unwrap()).unwrap(),
        )
        .unwrap();
    let ev: Value = serde_json::from_str(&events.recv().await.unwrap()).unwrap();
    assert_eq!(ev["t"], "update");
    a.transact_mut()
        .apply_update(
            Update::decode_v1(
                &STANDARD
                    .decode(
                        serde_json::from_str::<Value>(&events.recv().await.unwrap()).unwrap()["u"]
                            .as_str()
                            .unwrap(),
                    )
                    .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(text(&a), text(&b), "both editors see the same text");
    assert_eq!(text(&a), ">> hello world");
    // A fresh editor pulls the same thing.
    assert_eq!(
        text(&pulled(
            &sync(&f, &f.viewer, "description", json!({})).await.1
        )),
        ">> hello world"
    );

    // Viewers read but cannot write; strangers see nothing.
    let junk = type_at(&a, 0, "x");
    assert_eq!(
        sync(&f, &f.viewer, "description", json!({"update": b64(&junk)}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        sync(&f, &f.stranger, "description", json!({})).await.0,
        StatusCode::NOT_FOUND
    );
    let (st, _) =
        f.c.api(
            "GET",
            &format!("/cards/{}/collab/description/events", f.card),
            &f.stranger,
            None,
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // Only rich fields of this card can be edited this way.
    assert_eq!(
        sync(&f, &f.member, "title", json!({})).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        sync(&f, &f.member, "no_such_field", json!({})).await.0,
        StatusCode::NOT_FOUND
    );

    // Garbage and oversized updates are refused and leave the document alone.
    assert_eq!(
        sync(
            &f,
            &f.member,
            "description",
            json!({"update": b64(&[0xff; 64])})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        sync(
            &f,
            &f.member,
            "description",
            json!({"update": "not base64!"})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let huge = type_at(&client_doc(None), 0, &"x".repeat(300_000));
    assert_eq!(
        sync(&f, &f.member, "description", json!({"update": b64(&huge)}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        text(&pulled(
            &sync(&f, &f.member, "description", json!({})).await.1
        )),
        ">> hello world"
    );
}

fn doc_json(text: &str) -> Value {
    json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": text}]}]})
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn rich_text_is_validated_and_outside_writes_end_the_session(db: PgPool) {
    let f = fixture(db).await;
    let card: Uuid = f.card.parse().unwrap();
    let patch = |who: &str, body: Value| {
        let (c, who, id) = (&f.c, who.to_owned(), f.card.clone());
        async move {
            c.api("PATCH", &format!("/cards/{id}"), &who, Some(body))
                .await
        }
    };

    // Documents the editor could produce are stored; legacy plain text still is accepted.
    assert_eq!(
        patch(
            &f.owner,
            json!({"data": {"description": doc_json("Hello")}})
        )
        .await
        .0,
        StatusCode::OK
    );
    let got =
        f.c.ok("GET", &format!("/cards/{}", f.card), &f.owner, None)
            .await;
    assert_eq!(got["data"]["description"], doc_json("Hello"));
    assert_eq!(
        patch(&f.owner, json!({"data": {"description": "plain old text"}}))
            .await
            .0,
        StatusCode::OK
    );

    // Anything active or unknown is refused, in fields and in comments.
    let evil = json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "x", "marks": [{"type": "link", "attrs": {"href": "javascript:alert(1)"}}]}]}]});
    let (st, body) = patch(&f.owner, json!({"data": {"description": evil.clone()}})).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
    let script = json!({"type": "doc", "content": [{"type": "script"}]});
    assert_eq!(
        patch(&f.owner, json!({"data": {"description": script.clone()}}))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let comment = |who: &str, content: Value| {
        let (c, who, id) = (&f.c, who.to_owned(), f.card.clone());
        async move {
            c.api(
                "POST",
                &format!("/cards/{id}/activities"),
                &who,
                Some(json!({"type": "comment", "content": {"content": content}})),
            )
            .await
            .0
        }
    };
    assert_eq!(
        comment(&f.owner, doc_json("A rich comment")).await,
        StatusCode::OK
    );
    assert_eq!(comment(&f.owner, evil).await, StatusCode::BAD_REQUEST);
    assert_eq!(comment(&f.owner, script).await, StatusCode::BAD_REQUEST);
    assert_eq!(
        comment(&f.owner, json!("plain comment")).await,
        StatusCode::OK
    );

    // A collaborative session exists...
    let seed = full_state(&{
        let d = client_doc(None);
        type_at(&d, 0, "draft");
        d
    });
    assert_eq!(
        sync(&f, &f.owner, "description", json!({"seed": b64(&seed)}))
            .await
            .0,
        StatusCode::OK
    );
    let mut events = f.c.state.hub.subscribe(room(card, "description"));

    // ...an editor saving its own document keeps it...
    let saved = patch(
        &f.owner,
        json!({"data": {"description": doc_json("draft")}, "collab_keep": ["description"]}),
    )
    .await;
    assert_eq!(saved.0, StatusCode::OK);
    let (_, out) = sync(&f, &f.member, "description", json!({})).await;
    assert_eq!(out["empty"], false);

    // ...but any other write (an API client, an automation) ends it and tells the editors.
    assert_eq!(
        patch(
            &f.member,
            json!({"data": {"description": doc_json("changed elsewhere")}})
        )
        .await
        .0,
        StatusCode::OK
    );
    let ev: Value = serde_json::from_str(&events.recv().await.unwrap()).unwrap();
    assert_eq!(ev, json!({"t": "reset"}));
    let (_, out) = sync(&f, &f.member, "description", json!({})).await;
    assert_eq!(
        out["empty"], true,
        "the next session starts from the card's content"
    );
    // Writes to other fields never touch the session.
    assert_eq!(
        sync(&f, &f.owner, "description", json!({"seed": b64(&seed)}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        patch(&f.owner, json!({"data": {"title": "Renamed"}}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        sync(&f, &f.member, "description", json!({})).await.1["empty"],
        false
    );
}
