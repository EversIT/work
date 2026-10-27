//! Passkey ceremonies end to end, with a software authenticator.

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::{
    security::webauthn::{b64, soft::Authenticator, unb64, Rp},
    tests::harness::Client,
};

const PASSWORD: &str = "correct-horse-battery";
const UP_UV_AT: u8 = 0x01 | 0x04 | 0x40;
const UP_UV: u8 = 0x01 | 0x04;

fn rp() -> Rp {
    Rp {
        id: "example.test".into(),
        origin: "https://example.test".into(),
    }
}

async fn add_passkey(
    c: &Client,
    token: &str,
    a: &Authenticator,
    name: &str,
) -> (StatusCode, Value) {
    let (st, opts) = c
        .api(
            "POST",
            "/me/passkeys/options",
            token,
            Some(json!({"password": PASSWORD})),
        )
        .await;
    assert_eq!(st, StatusCode::OK, "{opts}");
    let challenge = unb64(opts["options"]["challenge"].as_str().unwrap()).unwrap();
    let (cd, att) = a.register(&rp(), &challenge, UP_UV_AT);
    c.api(
        "POST",
        "/me/passkeys",
        token,
        Some(json!({"challenge_id": opts["challenge_id"], "name": name, "response": {"client_data_json": b64(&cd), "attestation_object": b64(&att)}})),
    )
    .await
}

struct Attempt {
    challenge_id: Value,
    challenge: Vec<u8>,
}

async fn login_options(c: &Client) -> Attempt {
    let (st, body, _) = c
        .call("POST", "/auth/passkey/options", None, None, None)
        .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(body["options"]["userVerification"], "required");
    assert_eq!(body["options"]["rpId"], "example.test");
    Attempt {
        challenge_id: body["challenge_id"].clone(),
        challenge: unb64(body["options"]["challenge"].as_str().unwrap()).unwrap(),
    }
}

async fn login(
    c: &Client,
    attempt: &Attempt,
    a: &mut Authenticator,
    user_handle: Option<&[u8]>,
    flags: u8,
) -> (StatusCode, Value) {
    let (cd, ad, sig) = a.assert(&rp(), &attempt.challenge, flags);
    let (st, body, _) = c
        .call(
            "POST",
            "/auth/passkey/login",
            None,
            None,
            Some(json!({
                "challenge_id": attempt.challenge_id, "id": b64(&a.credential_id),
                "response": {"client_data_json": b64(&cd), "authenticator_data": b64(&ad), "signature": b64(&sig), "user_handle": user_handle.map(b64)}
            })),
        )
        .await;
    (st, body)
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn passkeys_register_sign_in_and_reject_forgeries(db: PgPool) {
    let c = Client::new(db.clone());
    let (token, _, user_id) = c.register("pk@example.test").await;
    let handle = uuid::Uuid::parse_str(&user_id).unwrap();
    assert_eq!(
        c.call("GET", "/system/info", None, None, None).await.1["passkeys_available"],
        true
    );

    // Adding one needs the account's password.
    let (st, _) = c
        .api(
            "POST",
            "/me/passkeys/options",
            &token,
            Some(json!({"password": "wrong-password-here"})),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = c
        .api("POST", "/me/passkeys/options", &token, Some(json!({})))
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    let mut laptop = Authenticator::new();
    let (st, added) = add_passkey(&c, &token, &laptop, "Laptop").await;
    assert_eq!(st, StatusCode::OK, "{added}");
    assert_eq!(added["name"], "Laptop");
    let list = c.ok("GET", "/me/passkeys", &token, None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    // The next ceremony does not offer the credential again.
    let (_, opts) = c
        .api(
            "POST",
            "/me/passkeys/options",
            &token,
            Some(json!({"password": PASSWORD})),
        )
        .await;
    assert_eq!(
        opts["options"]["excludeCredentials"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Another account cannot register the same credential.
    let (other, _, _) = c.register("other@example.test").await;
    assert_eq!(
        add_passkey(&c, &other, &laptop, "Stolen").await.0,
        StatusCode::CONFLICT
    );

    // Sign in without a password, usernameless; the session works.
    let attempt = login_options(&c).await;
    let (st, body) = login(&c, &attempt, &mut laptop, Some(handle.as_bytes()), UP_UV).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    let session = body["access_token"].as_str().unwrap();
    assert_eq!(
        c.ok("GET", "/me", session, None).await["email"],
        "pk@example.test"
    );

    // The challenge is single use.
    let (st, _) = login(&c, &attempt, &mut laptop, None, UP_UV).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // Everything else that should not work answers the same 401.
    let wrong_handle = uuid::Uuid::new_v4();
    let a2 = login_options(&c).await;
    assert_eq!(
        login(&c, &a2, &mut laptop, Some(wrong_handle.as_bytes()), UP_UV)
            .await
            .0,
        StatusCode::UNAUTHORIZED,
        "another user's handle"
    );
    let a3 = login_options(&c).await;
    assert_eq!(
        login(&c, &a3, &mut laptop, None, 0x01).await.0,
        StatusCode::UNAUTHORIZED,
        "no user verification"
    );
    let mut stranger = Authenticator::new();
    let a4 = login_options(&c).await;
    assert_eq!(
        login(&c, &a4, &mut stranger, None, UP_UV).await.0,
        StatusCode::UNAUTHORIZED,
        "an unknown credential"
    );
    let mut forged = Authenticator::new();
    forged.credential_id = laptop.credential_id.clone(); // same id, different private key
    let a5 = login_options(&c).await;
    assert_eq!(
        login(&c, &a5, &mut forged, None, UP_UV).await.0,
        StatusCode::UNAUTHORIZED,
        "a forged signature"
    );
    // A cloned authenticator: its counter is behind what the server has seen.
    sqlx::query("UPDATE passkeys SET sign_count = 500")
        .execute(&db)
        .await
        .unwrap();
    let a6 = login_options(&c).await;
    assert_eq!(
        login(&c, &a6, &mut laptop, None, UP_UV).await.0,
        StatusCode::UNAUTHORIZED,
        "counter went backwards"
    );
    sqlx::query("UPDATE passkeys SET sign_count = 0")
        .execute(&db)
        .await
        .unwrap();
    // An expired challenge is refused.
    let a7 = login_options(&c).await;
    sqlx::query("UPDATE webauthn_challenges SET expires_at = now() - interval '1 minute'")
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        login(&c, &a7, &mut laptop, None, UP_UV).await.0,
        StatusCode::UNAUTHORIZED,
        "expired"
    );
    // A registration answered for the wrong ceremony never reaches the store.
    let a8 = login_options(&c).await;
    let (cd, att) = Authenticator::new().register(&rp(), &a8.challenge, UP_UV_AT);
    let (st, _) = c.api("POST", "/me/passkeys", &token, Some(json!({"challenge_id": a8.challenge_id, "name": "x", "response": {"client_data_json": b64(&cd), "attestation_object": b64(&att)}}))).await;
    assert_eq!(
        st,
        StatusCode::BAD_REQUEST,
        "a login challenge cannot be used to register"
    );

    // Removing: possible while another way in exists, refused for the last one.
    let phone = Authenticator::new();
    assert_eq!(
        add_passkey(&c, &token, &phone, "Phone").await.0,
        StatusCode::OK
    );
    let list = c.ok("GET", "/me/passkeys", &token, None).await;
    let id = list[0]["id"].as_str().unwrap().to_owned();
    c.ok("DELETE", &format!("/me/passkeys/{id}"), &token, None)
        .await;
    assert_eq!(
        c.api("DELETE", &format!("/me/passkeys/{id}"), &token, None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    sqlx::query("UPDATE users SET password_hash = '' WHERE email = 'pk@example.test'")
        .execute(&db)
        .await
        .unwrap();
    let last = c.ok("GET", "/me/passkeys", &token, None).await[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        c.api("DELETE", &format!("/me/passkeys/{last}"), &token, None)
            .await
            .0,
        StatusCode::CONFLICT,
        "the last way in stays"
    );
}
