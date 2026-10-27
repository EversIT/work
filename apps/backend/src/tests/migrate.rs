//! The one-off import from an upstream database, against a source schema that follows the export
//! contract (with integer ids, like the upstream). See `migrate.rs` and `docs/DATA-MIGRATION.md`.

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

use crate::{
    services::migrate::{self, Options},
    tests::harness::Client,
};

/// A small upstream: two projects (one without members), a private space, a custom stage set,
/// cards with a parent, a card in no list, and values the new model refuses.
async fn source(db: &PgPool, old_hash: &str) {
    for stmt in [
        "CREATE SCHEMA work_export",
        "CREATE TABLE work_export.users (id bigint, email text, first_name text, last_name text, password_hash text, created_at timestamptz)",
        "CREATE TABLE work_export.projects (id bigint, name text)",
        "CREATE TABLE work_export.project_members (project_id bigint, user_id bigint, role text)",
        "CREATE TABLE work_export.workspaces (id bigint, project_id bigint, name text, key_prefix text)",
        "CREATE TABLE work_export.spaces (id bigint, workspace_id bigint, name text, is_private boolean)",
        "CREATE TABLE work_export.lists (id bigint, space_id bigint, name text, methodology text)",
        "CREATE TABLE work_export.stages (id bigint, list_id bigint, name text, sort_order int, is_completed boolean)",
        "CREATE TABLE work_export.card_types (id bigint, workspace_id bigint, name text, layout text)",
        "CREATE TABLE work_export.fields (id bigint, workspace_id bigint, card_type_id bigint, name text, slug text, type text, items jsonb, multiple boolean, required boolean)",
        "CREATE TABLE work_export.cards (id bigint, workspace_id bigint, card_type_id bigint, parent_id bigint, data jsonb, created_by bigint, created_at timestamptz)",
        "CREATE TABLE work_export.card_lists (card_id bigint, list_id bigint, stage_id bigint)",
        "CREATE TABLE work_export.comments (id bigint, card_id bigint, user_id bigint, content text, created_at timestamptz)",
        "INSERT INTO work_export.users VALUES
            (1,'Alice@Upstream.test','Alice','Archer',$1,'2019-05-01T10:00:00Z'),
            (2,'bob@upstream.test','Bob','',  'plain-text-not-a-hash','2019-06-01T10:00:00Z'),
            (3,'carol@upstream.test','Carol','Cole',$1,'2019-07-01T10:00:00Z'),
            (4,'not-an-address','No','Mail',$1,'2019-08-01T10:00:00Z')",
        "INSERT INTO work_export.projects VALUES (10,'Upstream Co'),(11,'Orphan project')",
        "INSERT INTO work_export.project_members VALUES (10,1,'owner'),(10,2,'guest'),(10,3,'admin'),(10,4,'owner')",
        "INSERT INTO work_export.workspaces VALUES (20,10,'Main','UPS'),(21,10,'Second','x!'),(22,11,'Lost','LST')",
        "INSERT INTO work_export.spaces VALUES (30,20,'Team',false),(31,20,'Secret',true)",
        "INSERT INTO work_export.card_types VALUES (40,20,'Task','default'),(41,20,'Contact','person')",
        "INSERT INTO work_export.fields VALUES
            (50,20,NULL,'Customer','customer','dropdown','[\"Gold\",\"Silver\"]',false,false),
            (51,20,NULL,'Summary','title','text',NULL,false,true),
            (52,20,NULL,'Odd','oops','weird',NULL,false,false),
            (53,20,41,'Phone','phone','text',NULL,false,false)",
        "INSERT INTO work_export.lists VALUES (60,30,'Backlog','kanban'),(61,30,'Sprints','scrum')",
        "INSERT INTO work_export.stages VALUES (70,60,'Open',1,false),(71,60,'Review',2,false),(72,60,'Shipped',3,true)",
        "INSERT INTO work_export.cards VALUES
            (80,20,40,NULL,'{\"title\":\"First\",\"customer\":\"Gold\",\"assignee\":[3,777]}',1,'2020-01-01T09:00:00Z'),
            (81,20,40,80,'{\"title\":\"Child\",\"made_up\":\"x\"}',2,'2020-01-02T09:00:00Z'),
            (82,20,40,NULL,'{\"title\":\"Odd value\",\"customer\":\"Platinum\"}',1,'2020-01-03T09:00:00Z'),
            (83,20,40,NULL,'{\"title\":\"Nowhere\"}',1,'2020-01-04T09:00:00Z'),
            (84,20,41,999,'{\"title\":\"Ada Lovelace\",\"phone\":\"123\"}',1,'2020-01-05T09:00:00Z')",
        "INSERT INTO work_export.card_lists VALUES (80,60,71),(80,61,NULL),(81,60,70),(82,60,72),(84,60,NULL)",
        "INSERT INTO work_export.comments VALUES
            (90,80,1,'Looks good','2020-02-02T08:00:00Z'),
            (91,80,2,'A viewer wrote this','2020-02-03T08:00:00Z'),
            (92,12345,1,'On a card that is gone','2020-02-04T08:00:00Z'),
            (93,80,1,'   ','2020-02-05T08:00:00Z')",
    ] {
        let q = sqlx::query(stmt);
        let q = if stmt.contains("$1") { q.bind(old_hash) } else { q };
        q.execute(db).await.unwrap_or_else(|e| panic!("{stmt}: {e}"));
    }
}

fn opts(dry_run: bool) -> Options {
    Options {
        schema: "work_export".into(),
        dry_run,
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn the_import_keeps_structure_authors_and_dates_and_reports_what_it_cannot_take(db: PgPool) {
    let c = Client::new(db.clone());
    // Carol already has an account here: she is the same person, not a second one.
    let (_, _, carol_id) = c.register("carol@upstream.test").await;
    let old_hash = bcrypt::hash("the old upstream password", 4).unwrap();
    source(&db, &old_hash).await;

    let report = migrate::run(&c.state, &db, &opts(false))
        .await
        .expect("import runs");
    let text = report.to_string();
    let created = |k: &str| report.counts.get(k).map(|c| c.0).unwrap_or(0);
    assert_eq!(
        (created("users"), created("projects"), created("workspaces")),
        (2, 1, 2),
        "{text}"
    );
    assert_eq!(created("spaces"), 2, "{text}");
    assert_eq!(created("lists"), 2, "{text}");
    assert_eq!(created("stages"), 3, "{text}");
    assert_eq!(created("cards"), 4, "{text}"); // 80, 81, 82, 84 (83 sits in no list)
    assert_eq!(created("comments"), 2, "{text}");
    for needle in [
        "user 4: invalid e-mail address",
        "user 2: no usable password hash",
        "project 11 ('Orphan project'): no importable members",
        "workspace 21: key 'X!' refused",
        "workspace 22: its project 11 was not imported",
        "field 52 ('oops')",
        "card 83: it sits in no list",
        "card 84: parent 999 was not imported",
        "card 81: ", // the made-up key is reported once per workspace
        "comment 92: its card 12345 was not imported",
        "comment 93: empty",
    ] {
        assert!(
            text.contains(needle) || text.contains(&needle.replace("card 81: ", "'made_up'")),
            "missing: {needle}\n{text}"
        );
    }
    assert!(text.contains("'made_up'"), "{text}");
    assert!(
        text.contains("card 82: value of 'customer' dropped"),
        "{text}"
    );

    // Alice signs in with her old bcrypt password; the hash is upgraded to Argon2id on the way.
    let (st, body, _) = c
        .call(
            "POST",
            "/auth/login",
            None,
            None,
            Some(json!({"email": "alice@upstream.test", "password": "the old upstream password"})),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let token = body["access_token"].as_str().unwrap().to_owned();
    let (hash,): (String,) =
        sqlx::query_as("SELECT password_hash FROM users WHERE email = 'alice@upstream.test'")
            .fetch_one(&db)
            .await
            .unwrap();
    assert!(hash.starts_with("$argon2id$"), "{hash}");
    let (st, _, _) = c
        .call(
            "POST",
            "/auth/login",
            None,
            None,
            Some(json!({"email": "alice@upstream.test", "password": "the old upstream password"})),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "still works after the upgrade");
    // Bob had no usable hash; Carol kept her existing account and password.
    assert_eq!(
        c.call(
            "POST",
            "/auth/login",
            None,
            None,
            Some(json!({"email": "bob@upstream.test", "password": ""}))
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (carol_count,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM users WHERE email = 'carol@upstream.test'")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(carol_count, 1);

    // Structure: the project, its roles, the workspace key, the spaces, lists and stages.
    let projects = c.ok("GET", "/projects", &token, None).await;
    let project = projects
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "Upstream Co")
        .unwrap();
    assert_eq!(project["role"], "owner");
    let members = c
        .ok(
            "GET",
            &format!("/projects/{}/members", project["id"].as_str().unwrap()),
            &token,
            None,
        )
        .await;
    let role_of = |email: &str| {
        members
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["email"] == email)
            .map(|m| m["role"].as_str().unwrap().to_owned())
    };
    assert_eq!(role_of("bob@upstream.test").as_deref(), Some("viewer"));
    assert_eq!(role_of("carol@upstream.test").as_deref(), Some("admin"));
    let wss = c
        .ok(
            "GET",
            &format!("/projects/{}/workspaces", project["id"].as_str().unwrap()),
            &token,
            None,
        )
        .await;
    let main = wss
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["name"] == "Main")
        .unwrap();
    assert_eq!(main["key_prefix"], "UPS");
    let ws = main["id"].as_str().unwrap().to_owned();
    let tree = c
        .ok("GET", &format!("/workspaces/{ws}/tree"), &token, None)
        .await;
    let tree_text = tree.to_string();
    assert!(
        tree_text.contains("Team")
            && tree_text.contains("Secret")
            && tree_text.contains("Backlog")
            && tree_text.contains("Sprints"),
        "{tree_text}"
    );
    let list = c
        .ok(
            "GET",
            &format!("/workspaces/{ws}/lists/by-slug/backlog"),
            &token,
            None,
        )
        .await;
    let stage_names: Vec<&str> = list["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        stage_names,
        ["Open", "Review", "Shipped"],
        "the default To Do/In Progress/Done are replaced"
    );
    assert_eq!(list["stages"][2]["is_completed"], true);

    // Cards: keys follow the original order, placement, parent, dates and dropped values.
    let first = c
        .ok(
            "GET",
            &format!("/workspaces/{ws}/cards/by-key/UPS-1"),
            &token,
            None,
        )
        .await;
    assert_eq!(first["data"]["title"], "First");
    assert_eq!(first["data"]["customer"], "Gold");
    // People fields are translated to the imported users; unknown people are left out and reported.
    assert_eq!(first["data"]["assignee"], json!([carol_id]));
    assert!(
        text.contains("person 777 in 'assignee' is not an imported user"),
        "{text}"
    );
    assert!(
        first["created_at"]
            .as_str()
            .unwrap()
            .starts_with("2020-01-01T09:00:00"),
        "{}",
        first["created_at"]
    );
    assert_eq!(
        first["lists"].as_array().unwrap().len(),
        2,
        "a card in two lists stays in two lists"
    );
    let review = list["stages"][1]["id"].as_str().unwrap();
    assert!(first["lists"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["list_stage_id"] == review));
    let child = c
        .ok(
            "GET",
            &format!("/workspaces/{ws}/cards/by-key/UPS-2"),
            &token,
            None,
        )
        .await;
    assert_eq!(child["parent_id"], first["id"]);
    assert!(child["data"].get("made_up").is_none());
    let odd = c
        .ok(
            "GET",
            &format!("/workspaces/{ws}/cards/by-key/UPS-3"),
            &token,
            None,
        )
        .await;
    assert!(odd["data"].get("customer").is_none() && odd["data"]["title"] == "Odd value");
    let contact = c
        .ok(
            "GET",
            &format!("/workspaces/{ws}/cards/by-key/UPS-4"),
            &token,
            None,
        )
        .await;
    assert_eq!(contact["data"]["phone"], "123");
    assert!(contact["parent_id"].is_null());

    // Comments keep their text, time and (where the author may write) the author.
    let acts = c
        .ok(
            "GET",
            &format!("/cards/{}/activities", first["id"].as_str().unwrap()),
            &token,
            None,
        )
        .await;
    let acts = acts.to_string();
    assert!(
        acts.contains("Looks good") && acts.contains("A viewer wrote this"),
        "{acts}"
    );
    assert!(acts.contains("2020-02-02T08:00:00"), "{acts}");

    // Running it again changes nothing and creates nothing.
    let (cards_before,): (i64,) = sqlx::query_as("SELECT count(*) FROM cards")
        .fetch_one(&db)
        .await
        .unwrap();
    let again = migrate::run(&c.state, &db, &opts(false)).await.unwrap();
    assert!(again.counts.values().all(|c| c.0 == 0), "{again}");
    let (cards_after,): (i64,) = sqlx::query_as("SELECT count(*) FROM cards")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(cards_before, cards_after);
    // Sessions the import opened to act as project owners are revoked; what is left is Carol's
    // registration and Alice's two sign-ins above.
    let (live,): (i64,) = sqlx::query_as("SELECT count(*) FROM sessions WHERE revoked_at IS NULL")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(live, 3);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn a_dry_run_writes_nothing_and_a_wrong_contract_is_explained(db: PgPool) {
    let c = Client::new(db.clone());
    source(&db, &bcrypt::hash("x", 4).unwrap()).await;
    let report = migrate::run(&c.state, &db, &opts(true)).await.unwrap();
    assert_eq!(report.counts["cards"].0, 5);
    assert_eq!(report.counts["users"].0, 4);
    let (users,): (i64,) = sqlx::query_as("SELECT count(*) FROM users")
        .fetch_one(&db)
        .await
        .unwrap();
    let (mapped,): (i64,) = sqlx::query_as("SELECT count(*) FROM migration_ids")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!((users, mapped), (0, 0));

    sqlx::query("ALTER TABLE work_export.cards DROP COLUMN parent_id")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("DROP TABLE work_export.comments")
        .execute(&db)
        .await
        .unwrap();
    let err = migrate::run(&c.state, &db, &opts(false))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("work_export.cards lacks column parent_id")
            && err.contains("work_export.comments does not exist"),
        "{err}"
    );
    let err = migrate::check_contract(&db, "x; drop schema public")
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("schema name"), "{err}");
}
