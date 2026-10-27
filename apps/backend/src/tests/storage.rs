//! File storage backends. The local disk always runs; S3 and Azure run when a test endpoint is
//! given (`TEST_S3_ENDPOINT` + `TEST_S3_BUCKET`, `TEST_AZURE_ENDPOINT` + `TEST_AZURE_ACCESS_KEY`),
//! like the mail tests with Mailpit. The credentials are made up per run and handed to a local
//! emulator; no secret lives in the source.

use axum::http::StatusCode;
use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::PgPool;

use crate::{
    config::{
        storage::{AzureConfig, S3Config},
        Config, StorageConfig,
    },
    tests::harness::Client,
};

const PNG: [u8; 12] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3, 4];

fn multipart(name: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    let boundary = "----work-test-boundary";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

async fn download(c: &Client, token: &str, url: &str) -> (StatusCode, Vec<u8>) {
    let resp = c
        .raw(
            "GET",
            url.trim_start_matches("/api/v1"),
            token,
            "application/json",
            vec![],
        )
        .await;
    let status = resp.status();
    (
        status,
        axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
}

/// Upload, download, isolation and persistence across a "restart" (a second client on the same store).
async fn exercise(db: PgPool, config: impl Fn(&mut Config) + Clone) {
    let c = Client::new_with(db.clone(), config.clone());
    let (owner, _, _) = c
        .register(&format!("o-{}@example.test", uuid::Uuid::new_v4().simple()))
        .await;
    let (stranger, _, _) = c
        .register(&format!("s-{}@example.test", uuid::Uuid::new_v4().simple()))
        .await;
    let project = c.first_project(&owner).await;

    let (ct, body) = multipart("../../evil name.png", &PNG);
    let resp = c
        .raw(
            "POST",
            &format!("/projects/{project}/files"),
            &owner,
            &ct,
            body,
        )
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let file: Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        file["name"], "evil name.png",
        "the client's name is metadata only"
    );
    let url = file["url"].as_str().unwrap().to_owned();

    let (st, bytes) = download(&c, &owner, &url).await;
    assert_eq!((st, bytes.as_slice()), (StatusCode::OK, &PNG[..]));
    assert_eq!(
        download(&c, &stranger, &url).await.0,
        StatusCode::NOT_FOUND,
        "other tenants cannot read it"
    );
    let (st, _) = c
        .api("GET", &url.replace("/api/v1", ""), "not-a-token", None)
        .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // A different server process over the same store sees the file.
    let again = Client::new_with(db, config);
    let (st, bytes) = download(&again, &owner, &url).await;
    assert_eq!((st, bytes.as_slice()), (StatusCode::OK, &PNG[..]));

    // Content the sniffer refuses never reaches the store.
    let (ct, body) = multipart("x.html", b"<script>alert(1)</script>\x00\x01");
    let resp = c
        .raw(
            "POST",
            &format!("/projects/{project}/files"),
            &owner,
            &ct,
            body,
        )
        .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let _ = json!(null);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn local_disk_storage(db: PgPool) {
    let dir = std::env::temp_dir().join(format!(
        "work-e2e-storage-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let dir = dir.to_string_lossy().into_owned();
    exercise(db, move |cfg| cfg.upload_dir = dir.clone()).await;
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL and an S3 emulator"]
async fn s3_storage(db: PgPool) {
    let (Ok(endpoint), Ok(bucket)) = (
        std::env::var("TEST_S3_ENDPOINT"),
        std::env::var("TEST_S3_BUCKET"),
    ) else {
        eprintln!("skipped: no S3 emulator");
        return;
    };
    // Made-up credentials for the emulator, different on every run.
    let (id, secret) = (
        uuid::Uuid::new_v4().simple().to_string(),
        uuid::Uuid::new_v4().simple().to_string(),
    );
    exercise(db, move |cfg| {
        cfg.storage = StorageConfig::S3(S3Config {
            bucket: bucket.clone(),
            region: "us-east-1".into(),
            endpoint: Some(endpoint.clone()),
            access_key_id: Some(id.clone()),
            secret_access_key: Some(secret.clone()),
            allow_http: true,
        });
    })
    .await;
}

/// Shared Key request to create the container in the emulator (the app itself never creates one).
async fn create_azure_container(endpoint: &str, account: &str, key_b64: &str, container: &str) {
    let date = chrono::Utc::now()
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    let version = "2021-08-06";
    let string_to_sign = format!(
        "PUT\n\n\n\n\n\n\n\n\n\n\n\nx-ms-date:{date}\nx-ms-version:{version}\n/{account}/{account}/{container}\nrestype:container"
    );
    let mut mac = Hmac::<Sha256>::new_from_slice(&STANDARD.decode(key_b64).unwrap()).unwrap();
    mac.update(string_to_sign.as_bytes());
    let signature = STANDARD.encode(mac.finalize().into_bytes());
    let resp = reqwest::Client::new()
        .put(format!(
            "{endpoint}/{account}/{container}?restype=container"
        ))
        .header("x-ms-date", date)
        .header("x-ms-version", version)
        .header("content-length", "0")
        .header("authorization", format!("SharedKey {account}:{signature}"))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success() || resp.status() == StatusCode::CONFLICT,
        "creating the container: {}",
        resp.status()
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL and Azurite"]
async fn azure_storage(db: PgPool) {
    let (Ok(endpoint), Ok(key)) = (
        std::env::var("TEST_AZURE_ENDPOINT"),
        std::env::var("TEST_AZURE_ACCESS_KEY"),
    ) else {
        eprintln!("skipped: no Azure emulator");
        return;
    };
    let account = std::env::var("TEST_AZURE_ACCOUNT").unwrap_or_else(|_| "work".into());
    let container = format!("work-{}", uuid::Uuid::new_v4().simple());
    create_azure_container(&endpoint, &account, &key, &container).await;
    exercise(db, move |cfg| {
        cfg.storage = StorageConfig::Azure(AzureConfig {
            account: account.clone(),
            container: container.clone(),
            access_key: Some(key.clone()),
            endpoint: Some(format!("{endpoint}/{account}")),
            allow_http: true,
        });
    })
    .await;
}
