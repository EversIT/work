//! End-to-end tests for the account hardening: password policy, lock-out, two-factor
//! authentication, e-mail verification at sign-up and the shared rate limiter.
//! Tests that need mail talk to Mailpit like the ones in `e2e_hooks` and skip without it.

use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    security::ratelimit::RateLimiter, security::totp, services::integrations::decrypt,
    tests::harness::Client,
};

const MFA_KEY: [u8; 32] = [7u8; 32]; // test-only key, never used outside these tests
const PASSWORD: &str = "correct-horse-battery";

fn uniq(local: &str) -> String {
    format!("{local}-{}@example.test", Uuid::new_v4().simple())
}

async fn login(c: &Client, email: &str, password: &str) -> (StatusCode, Value) {
    let (st, v, _) = c
        .call(
            "POST",
            "/auth/login",
            None,
            None,
            Some(json!({"email": email, "password": password})),
        )
        .await;
    (st, v)
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn password_policy_applies_to_every_way_of_setting_a_password(db: PgPool) {
    let c = Client::new(db);
    let reg = |email: &str, pw: &str| {
        let body =
            json!({"email": email, "password": pw, "first_name": "Jane", "last_name": "Doe"});
        let c = &c;
        async move {
            c.call("POST", "/auth/register", None, None, Some(body))
                .await
                .0
        }
    };
    assert_eq!(reg(&uniq("a"), "short").await, StatusCode::BAD_REQUEST);
    assert_eq!(
        reg(&uniq("a"), "password1234").await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        reg(&uniq("a"), "aaaaaaaaaaaaaa").await,
        StatusCode::BAD_REQUEST
    );
    let email = uniq("jane");
    assert_eq!(
        reg(&email, "my-JANE-passphrase-1").await,
        StatusCode::BAD_REQUEST,
        "contains the first name"
    );
    assert_eq!(
        reg(&uniq("a"), "a-fine-long-passphrase").await,
        StatusCode::OK
    );

    // Changing the password goes through the same policy, and a rejected one changes nothing.
    let (token, _, _) = c.register(&uniq("pol")).await;
    let change = |new: &str| {
        let body = json!({"current_password": PASSWORD, "new_password": new});
        let (c, token) = (&c, token.clone());
        async move { c.api("POST", "/me/password", &token, Some(body)).await.0 }
    };
    assert_eq!(change("Password-1234").await, StatusCode::BAD_REQUEST);
    assert_eq!(change("another-long-passphrase").await, StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn repeated_failures_lock_the_account_without_revealing_it(db: PgPool) {
    let c = Client::new(db.clone());
    let email = uniq("lock");
    c.register(&email).await;

    for _ in 0..4 {
        assert_eq!(
            login(&c, &email, "wrong-password-here").await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    // Still open: the fifth try with the right password works and resets the counter.
    assert_eq!(login(&c, &email, PASSWORD).await.0, StatusCode::OK);
    for _ in 0..5 {
        assert_eq!(
            login(&c, &email, "wrong-password-here").await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    // Locked: even the correct password is refused, with the same answer as a wrong one.
    let (st, body) = login(&c, &email, PASSWORD).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({"error": "unauthorized"}));
    let (until,): (Option<chrono::DateTime<chrono::Utc>>,) =
        sqlx::query_as("SELECT locked_until FROM users WHERE email = $1")
            .bind(&email)
            .fetch_one(&db)
            .await
            .unwrap();
    let minutes = (until.unwrap() - chrono::Utc::now()).num_minutes();
    assert!((13..=15).contains(&minutes), "locked for {minutes} min");

    // The lock runs out by itself.
    sqlx::query("UPDATE users SET locked_until = now() - interval '1 minute' WHERE email = $1")
        .bind(&email)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(login(&c, &email, PASSWORD).await.0, StatusCode::OK);
    let (failed,): (i32,) = sqlx::query_as("SELECT failed_logins FROM users WHERE email = $1")
        .bind(&email)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(failed, 0, "a successful sign-in clears the counter");

    // Old failures are forgotten once the lock is over, so typos never add up across weeks.
    sqlx::query("UPDATE users SET failed_logins = 4, last_failed_at = now() - interval '2 hours' WHERE email = $1")
        .bind(&email)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        login(&c, &email, "wrong-password-here").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&c, &email, PASSWORD).await.0,
        StatusCode::OK,
        "not locked after one new failure"
    );

    // Unknown accounts answer identically.
    let (st, body) = login(&c, &uniq("ghost"), "wrong-password-here").await;
    assert_eq!(
        (st, body),
        (StatusCode::UNAUTHORIZED, json!({"error": "unauthorized"}))
    );
}

async fn secret_of(db: &PgPool, user_id: &str) -> Vec<u8> {
    let id: Uuid = user_id.parse().unwrap();
    let (sealed,): (Vec<u8>,) = sqlx::query_as("SELECT mfa_secret FROM users WHERE id = $1")
        .bind(id)
        .fetch_one(db)
        .await
        .unwrap();
    assert!(
        sealed.len() > 20 + 12,
        "stored as nonce + ciphertext + tag, not raw"
    );
    decrypt(&MFA_KEY, id.as_bytes(), &sealed).expect("secret decrypts with the instance key")
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn two_factor_enrolment_sign_in_replay_and_recovery(db: PgPool) {
    let c = Client::new_with(db.clone(), |cfg| cfg.encryption_key = Some(MFA_KEY));
    let info = c.call("GET", "/system/info", None, None, None).await.1;
    assert_eq!(info["mfa_available"], true);
    let email = uniq("mfa");
    let (token, _, user_id) = c.register(&email).await;

    // Enrolment needs the password and a session.
    let (st, _) = c
        .api(
            "POST",
            "/me/mfa/setup",
            &token,
            Some(json!({"password": "wrong-password-here"})),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let setup = c
        .ok(
            "POST",
            "/me/mfa/setup",
            &token,
            Some(json!({"password": PASSWORD})),
        )
        .await;
    assert!(setup["otpauth_uri"]
        .as_str()
        .unwrap()
        .starts_with("otpauth://totp/work:"));
    let secret = secret_of(&db, &user_id).await;
    assert_eq!(setup["secret"].as_str().unwrap(), totp::base32(&secret));
    let me = c.ok("GET", "/me", &token, None).await;
    assert_eq!(
        me["mfa_enabled"], false,
        "pending until a code is confirmed"
    );

    let now = chrono::Utc::now().timestamp();
    let (st, _) = c
        .api(
            "POST",
            "/me/mfa/enable",
            &token,
            Some(json!({"code": "000000"})),
        )
        .await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let enabled = c
        .ok(
            "POST",
            "/me/mfa/enable",
            &token,
            Some(json!({"code": totp::code_at(&secret, now)})),
        )
        .await;
    let recovery: Vec<String> = enabled["recovery_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(recovery.len(), 10);
    assert_eq!(c.ok("GET", "/me", &token, None).await["mfa_enabled"], true);

    // Password alone no longer gives a session.
    let (st, first) = login(&c, &email, PASSWORD).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(first["mfa_required"], true);
    assert!(first.get("access_token").is_none());
    let challenge = first["mfa_token"].as_str().unwrap().to_owned();
    let verify = |challenge: &str, code: &str| {
        let body = json!({"mfa_token": challenge, "code": code});
        let c = &c;
        async move {
            c.call("POST", "/auth/mfa/verify", None, None, Some(body))
                .await
        }
    };
    assert_eq!(
        verify(&challenge, "000000").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        verify("not-a-challenge", "000000").await.0,
        StatusCode::UNAUTHORIZED
    );

    // The code used to enrol is spent; the next time step works exactly once.
    assert_eq!(
        verify(&challenge, &totp::code_at(&secret, now)).await.0,
        StatusCode::UNAUTHORIZED
    );
    let next = totp::code_at(&secret, now + 30);
    let (st, body, resp) = verify(&challenge, &next).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert!(body["access_token"].is_string());
    assert!(resp.headers().get("set-cookie").is_some());
    let (_, second) = login(&c, &email, PASSWORD).await;
    let again = verify(second["mfa_token"].as_str().unwrap(), &next).await.0;
    assert_eq!(again, StatusCode::UNAUTHORIZED, "a code cannot be replayed");
    assert_eq!(
        verify(&challenge, &next).await.0,
        StatusCode::UNAUTHORIZED,
        "challenge is single use"
    );

    // A recovery code signs in once, in any spelling.
    let (_, third) = login(&c, &email, PASSWORD).await;
    let spelled = recovery[0].to_uppercase().replace('-', " ");
    assert_eq!(
        verify(third["mfa_token"].as_str().unwrap(), &spelled)
            .await
            .0,
        StatusCode::OK
    );
    let (_, fourth) = login(&c, &email, PASSWORD).await;
    assert_eq!(
        verify(fourth["mfa_token"].as_str().unwrap(), &recovery[0])
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );

    // Disabling needs both factors and clears everything.
    let (st, _) = c
        .api(
            "POST",
            "/me/mfa/disable",
            &token,
            Some(json!({"password": PASSWORD, "code": "000000"})),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let (st, _) = c
        .api(
            "POST",
            "/me/mfa/disable",
            &token,
            Some(json!({"password": "wrong-password-here", "code": recovery[1]})),
        )
        .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    c.ok(
        "POST",
        "/me/mfa/disable",
        &token,
        Some(json!({"password": PASSWORD, "code": recovery[1]})),
    )
    .await;
    assert_eq!(c.ok("GET", "/me", &token, None).await["mfa_enabled"], false);
    let (_, plain) = login(&c, &email, PASSWORD).await;
    assert!(plain["access_token"].is_string());
    let (left,): (i64,) = sqlx::query_as("SELECT count(*) FROM mfa_recovery_codes")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn guessing_second_factor_codes_is_capped_and_locks_the_account(db: PgPool) {
    let c = Client::new_with(db.clone(), |cfg| cfg.encryption_key = Some(MFA_KEY));
    let email = uniq("guess");
    let (token, _, user_id) = c.register(&email).await;
    c.ok(
        "POST",
        "/me/mfa/setup",
        &token,
        Some(json!({"password": PASSWORD})),
    )
    .await;
    let secret = secret_of(&db, &user_id).await;
    c.ok(
        "POST",
        "/me/mfa/enable",
        &token,
        Some(json!({"code": totp::code_at(&secret, chrono::Utc::now().timestamp())})),
    )
    .await;

    let verify = |challenge: &str, code: &str| {
        let body = json!({"mfa_token": challenge, "code": code});
        let c = &c;
        async move {
            c.call("POST", "/auth/mfa/verify", None, None, Some(body))
                .await
                .0
        }
    };
    // Five misses burn a challenge, even when the sixth guess would have been right.
    let (_, first) = login(&c, &email, PASSWORD).await;
    let challenge = first["mfa_token"].as_str().unwrap();
    for _ in 0..4 {
        assert_eq!(verify(challenge, "000000").await, StatusCode::UNAUTHORIZED);
    }
    let good = totp::code_at(&secret, chrono::Utc::now().timestamp() + 30);
    // The fifth miss is also the account's fifth failure: it locks the account.
    assert_eq!(verify(challenge, "000000").await, StatusCode::UNAUTHORIZED);
    assert_eq!(
        verify(challenge, &good).await,
        StatusCode::UNAUTHORIZED,
        "challenge used up"
    );
    let (st, _) = login(&c, &email, PASSWORD).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED, "account is locked");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn mfa_is_unavailable_without_an_encryption_key(db: PgPool) {
    let c = Client::new(db);
    assert_eq!(
        c.call("GET", "/system/info", None, None, None).await.1["mfa_available"],
        false
    );
    let (token, _, _) = c.register(&uniq("nokey")).await;
    let (st, _) = c
        .api(
            "POST",
            "/me/mfa/setup",
            &token,
            Some(json!({"password": PASSWORD})),
        )
        .await;
    assert_eq!(
        st,
        StatusCode::CONFLICT,
        "secrets are never stored unencrypted"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn shared_rate_limiter_counts_in_the_database(db: PgPool) {
    let window = Duration::from_secs(60);
    // Two limiters with the same name stand in for two instances behind one load balancer.
    let a = RateLimiter::shared("test", 3, window, db.clone());
    let b = RateLimiter::shared("test", 3, window, db.clone());
    assert!(a.check("k").await);
    assert!(b.check("k").await);
    assert!(a.check("k").await);
    assert!(!b.check("k").await, "the budget is shared");
    assert!(b.check("other").await, "keys are independent");
    let other_name = RateLimiter::shared("elsewhere", 3, window, db.clone());
    assert!(other_name.check("k").await, "limiters do not share rows");

    // A new window starts from scratch.
    sqlx::query("UPDATE rate_limits SET window_start = now() - interval '2 minutes'")
        .execute(&db)
        .await
        .unwrap();
    assert!(a.check("k").await);
    crate::security::ratelimit::purge_shared(&db).await;

    // A broken database denies instead of letting everything through.
    let broken = RateLimiter::shared(
        "x",
        3,
        window,
        PgPool::connect_lazy("postgres://unused@127.0.0.1:1/none").unwrap(),
    );
    assert!(!broken.check("k").await);
}

// ---- e-mail verification (needs Mailpit) -------------------------------------------------------

fn mailpit() -> Option<String> {
    std::env::var("TEST_MAILPIT_API")
        .ok()
        .filter(|_| std::env::var("TEST_SMTP_PORT").is_ok())
}

/// Waits until `n` messages to `to` arrived; returns (subject, text, html) oldest first.
async fn mails_to(api: &str, to: &str, n: usize) -> Vec<(String, String)> {
    let http = reqwest::Client::new();
    for _ in 0..100 {
        let found: Value = http
            .get(format!("{api}/api/v1/search"))
            .query(&[("query", format!("to:{to}"))])
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let messages = found["messages"].as_array().cloned().unwrap_or_default();
        if messages.len() >= n {
            let mut out = Vec::new();
            for m in messages.iter().rev() {
                let full: Value = http
                    .get(format!(
                        "{api}/api/v1/message/{}",
                        m["ID"].as_str().unwrap()
                    ))
                    .send()
                    .await
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                out.push((
                    full["Subject"].as_str().unwrap().to_owned(),
                    format!(
                        "{}\n{}",
                        full["Text"].as_str().unwrap_or(""),
                        full["HTML"].as_str().unwrap_or("")
                    ),
                ));
            }
            return out;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Vec::new()
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
async fn sign_up_needs_a_confirmed_address_and_hides_existing_accounts(db: PgPool) {
    let Some(api) = mailpit() else {
        eprintln!("skipped: no Mailpit");
        return;
    };
    let c = Client::new_with(db.clone(), |cfg| cfg.require_email_verification = true);
    let info = c.call("GET", "/system/info", None, None, None).await.1;
    assert_eq!(info["email_verification_required"], true);
    let email = uniq("newbie");
    let register = |email: &str, password: &str| {
        let body =
            json!({"email": email, "password": password, "first_name": "New", "last_name": "User"});
        let c = &c;
        async move {
            c.call("POST", "/auth/register", None, None, Some(body))
                .await
        }
    };

    // Nothing but a pending registration exists until the link is used.
    let (st, body, resp) = register(&email, PASSWORD).await;
    assert_eq!(
        (st, &body),
        (
            StatusCode::OK,
            &json!({"mfa_required": false, "verification_required": true})
        )
    );
    assert!(resp.headers().get("set-cookie").is_none());
    let (users,): (i64,) = sqlx::query_as("SELECT count(*) FROM users WHERE email = $1")
        .bind(&email)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(users, 0);
    assert_eq!(
        login(&c, &email, PASSWORD).await.0,
        StatusCode::UNAUTHORIZED
    );

    // Pre-registration of somebody else's address cannot take the account over: the owner's own
    // registration replaces it and kills the earlier link.
    let attacker_pw = "attackers-chosen-passphrase";
    register(&email, attacker_pw).await;
    register(&email, PASSWORD).await;
    let mails = mails_to(&api, &email, 3).await;
    assert_eq!(mails.len(), 3, "{mails:?}");
    let stale = token_in(&mails[0].1);
    let fresh = token_in(&mails[2].1);
    let confirm = |t: &str| {
        let body = json!({"token": t});
        let c = &c;
        async move {
            c.call("POST", "/auth/verify-email", None, None, Some(body))
                .await
        }
    };
    assert_eq!(confirm("not-a-token").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        confirm(&stale).await.0,
        StatusCode::BAD_REQUEST,
        "an older link stops working"
    );
    let (st, body, resp) = confirm(&fresh).await;
    assert_eq!(st, StatusCode::OK, "{body}");
    assert!(body["access_token"].is_string());
    assert!(resp.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .contains("HttpOnly"));
    assert_eq!(
        confirm(&fresh).await.0,
        StatusCode::BAD_REQUEST,
        "the link works once"
    );
    assert_eq!(
        login(&c, &email, attacker_pw).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(login(&c, &email, PASSWORD).await.0, StatusCode::OK);

    // A taken address looks exactly the same from the outside and gets a notice instead.
    let (st, again, _) = register(&email, "yet-another-passphrase").await;
    assert_eq!((st, &again), (StatusCode::OK, &body_of_pending()));
    let notice = mails_to(&api, &email, 4).await;
    assert!(notice[3].0.contains("Your work account"), "{notice:?}");
    assert!(
        !notice[3].1.contains("token="),
        "the notice carries no sign-up link"
    );
    let (users,): (i64,) = sqlx::query_as("SELECT count(*) FROM users WHERE email = $1")
        .bind(&email)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(users, 1);
}

fn body_of_pending() -> Value {
    json!({"mfa_required": false, "verification_required": true})
}
