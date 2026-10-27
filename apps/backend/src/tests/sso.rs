//! End-to-end tests for single sign-on against a local stand-in for the provider.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Form, Json, Router,
};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::{config::OAuthProvider, tests::harness::Client};

#[derive(Clone, Default)]
struct Mock {
    userinfo: Arc<Mutex<Value>>,
    emails: Arc<Mutex<Value>>,
    challenge: Arc<Mutex<Option<String>>>,
}

async fn mock_provider(mock: Mock) -> String {
    async fn token(
        State(m): State<Mock>,
        Form(f): Form<HashMap<String, String>>,
    ) -> (StatusCode, Json<Value>) {
        let challenge = m.challenge.lock().unwrap().clone().unwrap_or_default();
        let verifier = f.get("code_verifier").cloned().unwrap_or_default();
        let proof_ok = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) == challenge;
        if f.get("code").map(String::as_str) == Some("good")
            && proof_ok
            && f.contains_key("client_secret")
        {
            (StatusCode::OK, Json(json!({"access_token": "at-1"})))
        } else {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid_grant"})),
            )
        }
    }
    fn authorized(h: &HeaderMap) -> bool {
        h.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer at-1")
    }
    async fn userinfo(State(m): State<Mock>, h: HeaderMap) -> (StatusCode, Json<Value>) {
        if authorized(&h) {
            (StatusCode::OK, Json(m.userinfo.lock().unwrap().clone()))
        } else {
            (StatusCode::UNAUTHORIZED, Json(json!({})))
        }
    }
    async fn emails(State(m): State<Mock>, h: HeaderMap) -> (StatusCode, Json<Value>) {
        if authorized(&h) {
            (StatusCode::OK, Json(m.emails.lock().unwrap().clone()))
        } else {
            (StatusCode::UNAUTHORIZED, Json(json!([])))
        }
    }
    let app = Router::new()
        .route("/token", post(token))
        .route("/userinfo", get(userinfo))
        .route("/emails", get(emails))
        .with_state(mock);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn provider(name: &'static str, base: &str, github: bool) -> OAuthProvider {
    OAuthProvider {
        name,
        client_id: "client-id".into(),
        client_secret: "client-secret".into(),
        auth_url: "https://idp.example/authorize".into(),
        token_url: format!("{base}/token"),
        userinfo_url: format!("{base}/userinfo"),
        emails_url: github.then(|| format!("{base}/emails")),
        scope: "openid email profile",
        trusts_email: false,
        oidc: None,
    }
}

struct Flow {
    state: String,
    cookie: String,
}

/// The browser's side of "start": follows nothing, just reads what the server asked for.
async fn start(c: &Client, mock: &Mock, provider: &str, next: Option<&str>) -> Flow {
    let uri = match next {
        Some(n) => format!(
            "/auth/sso/{provider}/start?next={}",
            url::form_urlencoded::byte_serialize(n.as_bytes()).collect::<String>()
        ),
        None => format!("/auth/sso/{provider}/start"),
    };
    let (st, _, resp) = c.call("GET", &uri, None, None, None).await;
    assert_eq!(st, StatusCode::FOUND);
    let loc = url::Url::parse(resp.headers()["location"].to_str().unwrap()).unwrap();
    assert_eq!(loc.host_str(), Some("idp.example"));
    let q: HashMap<_, _> = loc.query_pairs().into_owned().collect();
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["client_id"], "client-id");
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(
        q["redirect_uri"],
        format!("https://example.test/api/v1/auth/sso/{provider}/callback")
    );
    *mock.challenge.lock().unwrap() = Some(q["code_challenge"].clone());
    let cookie = resp.headers()["set-cookie"].to_str().unwrap().to_owned();
    assert!(
        cookie.contains("HttpOnly")
            && cookie.contains("Secure")
            && cookie.contains("Path=/api/v1/auth"),
        "{cookie}"
    );
    Flow {
        state: q["state"].clone(),
        cookie: cookie.split(';').next().unwrap().to_owned(),
    }
}

/// Returns (location, cookies set by the response).
async fn callback(
    c: &Client,
    provider: &str,
    flow: &Flow,
    code: &str,
    with_cookie: bool,
) -> (String, Vec<String>) {
    let uri = format!(
        "/auth/sso/{provider}/callback?code={code}&state={}",
        flow.state
    );
    let (st, _, resp) = c
        .call(
            "GET",
            &uri,
            None,
            with_cookie.then_some(flow.cookie.as_str()),
            None,
        )
        .await;
    assert_eq!(st, StatusCode::FOUND);
    let cookies = resp
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect();
    (
        resp.headers()["location"].to_str().unwrap().to_owned(),
        cookies,
    )
}

fn refresh_cookie(cookies: &[String]) -> Option<String> {
    cookies
        .iter()
        .find(|c| c.starts_with("work_refresh=") && !c.starts_with("work_refresh=;"))
        .map(|c| c.split(';').next().unwrap().to_owned())
}

async fn session_from(c: &Client, cookies: &[String]) -> String {
    let cookie = refresh_cookie(cookies).expect("a refresh cookie");
    let (st, body, _) = c
        .call("POST", "/auth/refresh", None, Some(&cookie), None)
        .await;
    assert_eq!(st, StatusCode::OK);
    body["access_token"].as_str().unwrap().to_owned()
}

fn uniq(local: &str) -> String {
    format!("{local}-{}@example.test", uuid::Uuid::new_v4().simple())
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn oauth_sign_in_creates_and_reuses_an_account_and_rejects_forgeries(db: PgPool) {
    let mock = Mock::default();
    let base = mock_provider(mock.clone()).await;
    let c = Client::new_with(db.clone(), |cfg| {
        cfg.sso.oauth = vec![provider("google", &base, false)]
    });
    let info = c.call("GET", "/system/info", None, None, None).await.1;
    assert_eq!(info["sso_providers"], json!(["google"]));
    let (st, _, _) = c
        .call("GET", "/auth/sso/github/start", None, None, None)
        .await;
    assert_eq!(
        st,
        StatusCode::NOT_FOUND,
        "unconfigured providers do not exist"
    );

    let email = uniq("ada");
    *mock.userinfo.lock().unwrap() = json!({"sub": "g-1", "email": email, "email_verified": true, "given_name": "Ada", "family_name": "Lovelace"});

    // A first sign-in creates the account; an off-site `next` is replaced by `/`.
    let flow = start(&c, &mock, "google", Some("//evil.test")).await;
    let (loc, cookies) = callback(&c, "google", &flow, "good", true).await;
    assert_eq!(loc, "https://example.test/sso/complete?next=%2F");
    let token = session_from(&c, &cookies).await;
    let me = c.ok("GET", "/me", &token, None).await;
    assert_eq!(
        (
            me["email"].as_str(),
            me["first_name"].as_str(),
            me["last_name"].as_str()
        ),
        (Some(email.as_str()), Some("Ada"), Some("Lovelace"))
    );
    assert!(
        !c.ok("GET", "/projects", &token, None)
            .await
            .as_array()
            .unwrap()
            .is_empty(),
        "gets a personal project"
    );
    let (hash,): (String,) = sqlx::query_as("SELECT password_hash FROM users WHERE email = $1")
        .bind(&email)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(hash, "", "no password: password sign-in is impossible");
    let (st, _, _) = c
        .call(
            "POST",
            "/auth/login",
            None,
            None,
            Some(json!({"email": email, "password": ""})),
        )
        .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // Same subject again: same account, even though the provider now reports another address.
    let flow = start(&c, &mock, "google", Some("/invite?token=abc")).await;
    *mock.userinfo.lock().unwrap() =
        json!({"sub": "g-1", "email": uniq("renamed"), "email_verified": true});
    let (loc, cookies) = callback(&c, "google", &flow, "good", true).await;
    assert_eq!(
        loc,
        "https://example.test/sso/complete?next=%2Finvite%3Ftoken%3Dabc"
    );
    let again = session_from(&c, &cookies).await;
    assert_eq!(c.ok("GET", "/me", &again, None).await["id"], me["id"]);
    let (users,): (i64,) = sqlx::query_as("SELECT count(*) FROM users")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(users, 1);

    // The state is single use.
    let (loc, _) = callback(&c, "google", &flow, "good", true).await;
    assert!(loc.ends_with("/login?sso_error=expired"), "{loc}");

    // A callback in a different browser (no binding cookie) is refused, and burns the state.
    let flow = start(&c, &mock, "google", None).await;
    let (loc, cookies) = callback(&c, "google", &flow, "good", false).await;
    assert!(loc.ends_with("/login?sso_error=browser_mismatch"), "{loc}");
    assert!(refresh_cookie(&cookies).is_none());
    let (loc, _) = callback(&c, "google", &flow, "good", true).await;
    assert!(loc.ends_with("/login?sso_error=expired"), "{loc}");

    // A wrong code, a provider error and a forged state all end at the sign-in page.
    let flow = start(&c, &mock, "google", None).await;
    let (loc, _) = callback(&c, "google", &flow, "bad", true).await;
    assert!(loc.ends_with("/login?sso_error=failed"), "{loc}");
    let (st, _, resp) = c
        .call(
            "GET",
            "/auth/sso/google/callback?error=access_denied&state=x",
            None,
            None,
            None,
        )
        .await;
    assert_eq!(st, StatusCode::FOUND);
    assert!(resp.headers()["location"]
        .to_str()
        .unwrap()
        .ends_with("sso_error=denied"));
    let forged = Flow {
        state: "not-a-state".into(),
        cookie: flow.cookie.clone(),
    };
    let (loc, _) = callback(&c, "google", &forged, "good", true).await;
    assert!(loc.ends_with("/login?sso_error=expired"), "{loc}");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn oauth_never_merges_accounts_by_email_and_linking_needs_a_session(db: PgPool) {
    let mock = Mock::default();
    let base = mock_provider(mock.clone()).await;
    let c = Client::new_with(db.clone(), |cfg| {
        cfg.sso.oauth = vec![provider("google", &base, false)]
    });

    // An unverified address cannot create an account.
    *mock.userinfo.lock().unwrap() =
        json!({"sub": "g-9", "email": uniq("nope"), "email_verified": false});
    let flow = start(&c, &mock, "google", None).await;
    let (loc, _) = callback(&c, "google", &flow, "good", true).await;
    assert!(loc.ends_with("sso_error=email_unverified"), "{loc}");

    // A verified address that belongs to a local account does not sign into it.
    let email = uniq("owner");
    let (token, _, user_id) = c.register(&email).await;
    *mock.userinfo.lock().unwrap() = json!({"sub": "g-2", "email": email, "email_verified": true});
    let flow = start(&c, &mock, "google", None).await;
    let (loc, cookies) = callback(&c, "google", &flow, "good", true).await;
    assert!(loc.ends_with("sso_error=account_exists"), "{loc}");
    assert!(refresh_cookie(&cookies).is_none());

    // The owner links the provider from a session; an API token cannot start that.
    let (st, _) = c.api("POST", "/me/sso/google/link", &token, None).await;
    assert_eq!(st, StatusCode::OK);
    let (_, body, resp) = c
        .call("POST", "/me/sso/google/link", Some(&token), None, None)
        .await;
    let loc = url::Url::parse(body["url"].as_str().unwrap()).unwrap();
    let q: HashMap<_, _> = loc.query_pairs().into_owned().collect();
    *mock.challenge.lock().unwrap() = Some(q["code_challenge"].clone());
    let flow = Flow {
        state: q["state"].clone(),
        cookie: resp.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned(),
    };
    let (loc, _) = callback(&c, "google", &flow, "good", true).await;
    assert_eq!(loc, "https://example.test/settings?sso=linked");
    let links = c.ok("GET", "/me/sso", &token, None).await;
    assert_eq!(links[0]["provider"], "google");

    // Now the provider signs in to that account.
    let flow = start(&c, &mock, "google", None).await;
    let (_, cookies) = callback(&c, "google", &flow, "good", true).await;
    let via_sso = session_from(&c, &cookies).await;
    assert_eq!(
        c.ok("GET", "/me", &via_sso, None).await["id"].as_str(),
        Some(user_id.as_str())
    );

    // The same provider account cannot be linked to a second user.
    let (other, _, _) = c.register(&uniq("other")).await;
    let (_, body, resp) = c
        .call("POST", "/me/sso/google/link", Some(&other), None, None)
        .await;
    let q: HashMap<_, _> = url::Url::parse(body["url"].as_str().unwrap())
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect();
    *mock.challenge.lock().unwrap() = Some(q["code_challenge"].clone());
    let flow = Flow {
        state: q["state"].clone(),
        cookie: resp.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned(),
    };
    let (loc, _) = callback(&c, "google", &flow, "good", true).await;
    assert!(loc.ends_with("sso_error=identity_in_use"), "{loc}");

    // Unlinking works while a password exists.
    c.ok("DELETE", "/me/sso/google", &token, None).await;
    assert_eq!(c.ok("GET", "/me/sso", &token, None).await, json!([]));
    assert_eq!(
        c.api("DELETE", "/me/sso/google", &token, None).await.0,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn an_sso_only_account_cannot_remove_its_last_way_in(db: PgPool) {
    let mock = Mock::default();
    let base = mock_provider(mock.clone()).await;
    let c = Client::new_with(db, |cfg| {
        cfg.sso.oauth = vec![provider("google", &base, false)]
    });
    *mock.userinfo.lock().unwrap() =
        json!({"sub": "g-5", "email": uniq("solo"), "email_verified": true});
    let flow = start(&c, &mock, "google", None).await;
    let (_, cookies) = callback(&c, "google", &flow, "good", true).await;
    let token = session_from(&c, &cookies).await;
    assert_eq!(
        c.api("DELETE", "/me/sso/google", &token, None).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        c.ok("GET", "/me/sso", &token, None)
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "needs DATABASE_URL"]
async fn github_only_trusts_the_primary_verified_address(db: PgPool) {
    let mock = Mock::default();
    let base = mock_provider(mock.clone()).await;
    let c = Client::new_with(db, |cfg| {
        let mut p = provider("github", &base, true);
        p.userinfo_url = format!("{base}/userinfo");
        cfg.sso.oauth = vec![p];
    });
    *mock.userinfo.lock().unwrap() = json!({"id": 4242, "login": "ada", "name": "Ada Lovelace"});
    let good = uniq("gh");
    *mock.emails.lock().unwrap() = json!([
        {"email": uniq("secondary"), "primary": false, "verified": true},
        {"email": good, "primary": true, "verified": true}
    ]);
    let flow = start(&c, &mock, "github", None).await;
    let (_, cookies) = callback(&c, "github", &flow, "good", true).await;
    let token = session_from(&c, &cookies).await;
    let me = c.ok("GET", "/me", &token, None).await;
    assert_eq!(
        (
            me["email"].as_str(),
            me["first_name"].as_str(),
            me["last_name"].as_str()
        ),
        (Some(good.as_str()), Some("Ada"), Some("Lovelace"))
    );

    // A primary address that GitHub has not verified is not an identity.
    *mock.userinfo.lock().unwrap() = json!({"id": 777, "login": "eve"});
    *mock.emails.lock().unwrap() =
        json!([{"email": uniq("victim"), "primary": true, "verified": false}]);
    let flow = start(&c, &mock, "github", None).await;
    let (loc, _) = callback(&c, "github", &flow, "good", true).await;
    assert!(loc.ends_with("sso_error=email_unverified"), "{loc}");
}

// ---- SAML ---------------------------------------------------------------------------------------

mod saml_flow {
    use std::time::{Duration, SystemTime};

    use super::*;
    use crate::config::SamlConfig;
    use saml::{
        Attribute, AuthnContextClassRef, Binding, C14nAlgorithm, ConsumeAuthnRequestWire,
        DataEncryptionAlgorithm, DigestAlgorithm, Endpoint, IdentityProvider,
        IdentityProviderConfig, IdpAssertionSigning, IdpLogoutSigning, IdpLogoutWantSigned,
        IssueResponse, KeyPair, KeyTransportAlgorithm, NameId, NameIdFormat, PeerCryptoPolicy,
        SignatureAlgorithm, SpDescriptor, SsoResponseDispatch,
    };

    const SSO_URL: &str = "https://idp.example/saml/sso";

    fn idp(entity: &str) -> IdentityProvider {
        let ck = rcgen::generate_simple_self_signed(vec!["idp.example".into()]).unwrap();
        let key = KeyPair::from_pkcs8_pem(ck.key_pair.serialize_pem().as_bytes())
            .unwrap()
            .with_certificate(saml::X509Certificate::from_pem(ck.cert.pem().as_bytes()).unwrap())
            .unwrap();
        IdentityProvider::new(IdentityProviderConfig {
            entity_id: entity.into(),
            sso: vec![Endpoint::redirect(SSO_URL, 0, true)],
            slo: vec![],
            artifact_resolution: vec![],
            supported_name_id_formats: vec![NameIdFormat::Persistent, NameIdFormat::EmailAddress],
            default_name_id_format: NameIdFormat::Persistent,
            signing_key: key,
            decryption_key: None,
            want_authn_requests_signed: false,
            assertion_signing: IdpAssertionSigning {
                sign_responses: false,
                sign_assertions: true,
            },
            encrypt_assertions_when_possible: false,
            logout_signing: IdpLogoutSigning {
                sign_requests: false,
                sign_responses: false,
            },
            logout_want_signed: IdpLogoutWantSigned {
                requests: false,
                responses: false,
            },
            default_session_duration: Duration::from_secs(3600),
            max_authn_request_age: IdentityProviderConfig::DEFAULT_MAX_AUTHN_REQUEST_AGE,
            default_peer_crypto_policy: PeerCryptoPolicy::strong_defaults(),
            outbound_signature_algorithm: SignatureAlgorithm::EcdsaSha256,
            outbound_digest_algorithm: DigestAlgorithm::Sha256,
            outbound_c14n: C14nAlgorithm::ExclusiveCanonical,
            outbound_data_encryption_algorithm: DataEncryptionAlgorithm::Aes256Gcm,
            outbound_key_transport_algorithm: KeyTransportAlgorithm::RsaOaep,
        })
        .unwrap()
    }

    /// What the IdP's login page does after authenticating `email`: read the redirect, answer it.
    fn answer(
        idp: &IdentityProvider,
        sp: &SpDescriptor,
        redirect: &str,
        subject: &str,
        email: &str,
    ) -> (String, String) {
        let url = url::Url::parse(redirect).unwrap();
        let query = url.query().unwrap().to_owned();
        let parsed = idp
            .consume_authn_request_wire(ConsumeAuthnRequestWire {
                sp,
                peer_crypto_policy: None,
                max_authn_request_age: None,
                wire_body: query.as_bytes(),
                binding: Binding::HttpRedirect,
                relay_state: None,
                expected_destination: SSO_URL,
                now: SystemTime::now(),
                clock_skew: Duration::from_secs(60),
            })
            .expect("the IdP accepts the SP's request");
        let SsoResponseDispatch::Post(form) = idp
            .issue_response(IssueResponse {
                sp,
                in_response_to: &parsed,
                name_id: NameId::persistent_for_sp(subject, &sp.entity_id),
                attributes: vec![Attribute::email(email)],
                authn_instant: SystemTime::now(),
                session_index: "s-1".into(),
                session_not_on_or_after: None,
                authn_context_class_ref: AuthnContextClassRef::PasswordProtectedTransport,
                force_encrypt_assertion: Some(false),
                now: SystemTime::now(),
                assertion_lifetime: Duration::from_secs(300),
                subject_confirmation_lifetime: Duration::from_secs(300),
                holder_of_key_cert: None,
            })
            .expect("the IdP issues a response")
        else {
            panic!("POST binding expected");
        };
        (
            form.saml_response,
            form.relay_state.expect("relay state is echoed"),
        )
    }

    fn form(response: &str, relay: &str) -> String {
        let enc = |v: &str| url::form_urlencoded::byte_serialize(v.as_bytes()).collect::<String>();
        format!("SAMLResponse={}&RelayState={}", enc(response), enc(relay))
    }

    async fn run(
        c: &Client,
        idp: &IdentityProvider,
        sp: &SpDescriptor,
        subject: &str,
        email: &str,
    ) -> (String, String, String) {
        let (st, _, resp) = c
            .call("GET", "/auth/sso/saml/start", None, None, None)
            .await;
        assert_eq!(st, StatusCode::FOUND);
        let redirect = resp.headers()["location"].to_str().unwrap().to_owned();
        assert!(redirect.starts_with(SSO_URL));
        let cookie_line = resp.headers()["set-cookie"].to_str().unwrap().to_owned();
        // The response arrives as a cross-site POST, so the cookie must survive that.
        assert!(
            cookie_line.contains("SameSite=None") && cookie_line.contains("Secure"),
            "{cookie_line}"
        );
        let cookie = cookie_line.split(';').next().unwrap().to_owned();
        let (response, relay) = answer(idp, sp, &redirect, subject, email);
        (response, relay, cookie)
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "needs DATABASE_URL"]
    async fn saml_sign_in_accepts_only_signed_answers_to_this_browsers_request(db: PgPool) {
        let idp_a = idp("https://idp.example/saml");
        let metadata = idp_a.metadata_xml(false).unwrap();
        let c = Client::new_with(db.clone(), |cfg| {
            cfg.sso.saml = Some(SamlConfig {
                entity_id: "https://example.test/api/v1/auth/saml/metadata".into(),
                idp_metadata_xml: metadata,
                sp_key_pem: None,
            })
        });
        assert_eq!(
            c.call("GET", "/system/info", None, None, None).await.1["sso_providers"],
            json!(["saml"])
        );
        let (st, sp_xml) = c.get_text("/auth/saml/metadata").await;
        assert_eq!(st, StatusCode::OK);
        let sp = SpDescriptor::from_metadata_xml(sp_xml.as_bytes()).unwrap();

        // A genuine answer creates the account.
        let email = uniq("saml");
        let (response, relay, cookie) = run(&c, &idp_a, &sp, "persistent-1", &email).await;
        let resp = c
            .form_post("/auth/saml/acs", Some(&cookie), form(&response, &relay))
            .await;
        assert_eq!(resp.status(), StatusCode::FOUND);
        let loc = resp.headers()["location"].to_str().unwrap().to_owned();
        assert_eq!(loc, "https://example.test/sso/complete?next=%2F");
        let cookies: Vec<String> = resp
            .headers()
            .get_all("set-cookie")
            .iter()
            .map(|v| v.to_str().unwrap().to_owned())
            .collect();
        let token = session_from(&c, &cookies).await;
        assert_eq!(
            c.ok("GET", "/me", &token, None).await["email"].as_str(),
            Some(email.as_str())
        );

        // The same answer cannot be used twice.
        let resp = c
            .form_post("/auth/saml/acs", Some(&cookie), form(&response, &relay))
            .await;
        assert!(resp.headers()["location"]
            .to_str()
            .unwrap()
            .ends_with("sso_error=expired"));

        // Another browser (no cookie) cannot use a captured answer.
        let (response, relay, _) = run(&c, &idp_a, &sp, "persistent-1", &email).await;
        let resp = c
            .form_post("/auth/saml/acs", None, form(&response, &relay))
            .await;
        assert!(resp.headers()["location"]
            .to_str()
            .unwrap()
            .ends_with("sso_error=browser_mismatch"));

        // Assertions edited after signing (another subject, another address) are rejected.
        let victim = uniq("victim");
        type Edit = Box<dyn Fn(&str) -> String>;
        let edits: [Edit; 2] = [
            Box::new(|xml| xml.replace("persistent-2", "persistent-1")),
            Box::new(move |xml| {
                xml.replacen(
                    "@example.test",
                    &format!("@example.test</saml:AttributeValue><saml:AttributeValue>{victim}"),
                    1,
                )
            }),
        ];
        for edit in edits {
            let (response, relay, cookie) =
                run(&c, &idp_a, &sp, "persistent-2", &uniq("honest")).await;
            let xml = String::from_utf8(STANDARD.decode(&response).unwrap()).unwrap();
            let bad = edit(&xml);
            assert_ne!(bad, xml, "the edit changed something");
            let resp = c
                .form_post(
                    "/auth/saml/acs",
                    Some(&cookie),
                    form(&STANDARD.encode(bad.as_bytes()), &relay),
                )
                .await;
            let loc = resp.headers()["location"].to_str().unwrap().to_owned();
            assert!(loc.ends_with("sso_error=saml_invalid"), "{loc}");
        }

        // A well-formed answer signed by some other IdP's key is rejected.
        let evil = idp("https://idp.example/saml");
        let (response, relay, cookie) = run(&c, &evil, &sp, "persistent-9", &uniq("evil")).await;
        let resp = c
            .form_post("/auth/saml/acs", Some(&cookie), form(&response, &relay))
            .await;
        assert!(resp.headers()["location"]
            .to_str()
            .unwrap()
            .ends_with("sso_error=saml_invalid"));

        // An unsolicited answer (no state of ours) is rejected.
        let (response, _, cookie) =
            run(&c, &idp_a, &sp, "persistent-7", &uniq("unsolicited")).await;
        let resp = c
            .form_post(
                "/auth/saml/acs",
                Some(&cookie),
                form(&response, "made-up-relay-state"),
            )
            .await;
        assert!(resp.headers()["location"]
            .to_str()
            .unwrap()
            .ends_with("sso_error=expired"));

        let (users,): (i64,) = sqlx::query_as("SELECT count(*) FROM users")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(users, 1, "only the genuine sign-in created an account");
    }
}

// ---- generic OpenID Connect ---------------------------------------------------------------------

mod oidc_flow {
    use super::*;
    use crate::config::OidcSettings;
    use axum::{extract::State, routing::get};
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

    #[derive(Clone)]
    struct Idp {
        base: Arc<Mutex<String>>,
        jwks: Value,
        /// What the token endpoint answers with (set by each test).
        id_token: Arc<Mutex<String>>,
        userinfo: Arc<Mutex<Value>>,
        challenge: Arc<Mutex<Option<String>>>,
        issuer_in_doc: Arc<Mutex<Option<String>>>,
    }

    struct Keys {
        enc: EncodingKey,
        jwks: Value,
        /// The same key material under another `kid`: a signature that must not be accepted.
        other: EncodingKey,
    }

    fn keys() -> Keys {
        let pair = |_| rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let (k, o) = (pair(0), pair(1));
        let raw = k.public_key_raw();
        let b = |x: &[u8]| URL_SAFE_NO_PAD.encode(x);
        Keys {
            enc: EncodingKey::from_ec_pem(k.serialize_pem().as_bytes()).unwrap(),
            jwks: json!({"keys": [{"kty": "EC", "crv": "P-256", "kid": "k1", "use": "sig", "alg": "ES256", "x": b(&raw[1..33]), "y": b(&raw[33..65])}]}),
            other: EncodingKey::from_ec_pem(o.serialize_pem().as_bytes()).unwrap(),
        }
    }

    async fn idp_server(idp: Idp) -> String {
        async fn doc(State(i): State<Idp>) -> Json<Value> {
            let base = i.base.lock().unwrap().clone();
            let issuer = i
                .issuer_in_doc
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| base.clone());
            Json(json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{base}/authorize"),
                "token_endpoint": format!("{base}/token"),
                "userinfo_endpoint": format!("{base}/userinfo"),
                "jwks_uri": format!("{base}/jwks"),
                "code_challenge_methods_supported": ["S256"],
                "id_token_signing_alg_values_supported": ["ES256"],
            }))
        }
        async fn jwks(State(i): State<Idp>) -> Json<Value> {
            Json(i.jwks.clone())
        }
        async fn token(
            State(i): State<Idp>,
            Form(f): Form<HashMap<String, String>>,
        ) -> (StatusCode, Json<Value>) {
            let challenge = i.challenge.lock().unwrap().clone().unwrap_or_default();
            let proof_ok = URL_SAFE_NO_PAD.encode(Sha256::digest(
                f.get("code_verifier")
                    .cloned()
                    .unwrap_or_default()
                    .as_bytes(),
            )) == challenge;
            if f.get("code").map(String::as_str) == Some("good") && proof_ok {
                (
                    StatusCode::OK,
                    Json(
                        json!({"access_token": "at-1", "id_token": i.id_token.lock().unwrap().clone()}),
                    ),
                )
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": "invalid_grant"})),
                )
            }
        }
        async fn userinfo(State(i): State<Idp>, h: HeaderMap) -> (StatusCode, Json<Value>) {
            if h.get("authorization").and_then(|v| v.to_str().ok()) == Some("Bearer at-1") {
                (StatusCode::OK, Json(i.userinfo.lock().unwrap().clone()))
            } else {
                (StatusCode::UNAUTHORIZED, Json(json!({})))
            }
        }
        let app = Router::new()
            .route("/.well-known/openid-configuration", get(doc))
            .route("/jwks", get(jwks))
            .route("/token", post(token))
            .route("/userinfo", get(userinfo))
            .with_state(idp.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        *idp.base.lock().unwrap() = base.clone();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        base
    }

    fn settings(base: &str, trust: bool) -> OidcSettings {
        OidcSettings {
            issuer: base.to_owned(),
            client_id: "client-id".into(),
            client_secret: "client-secret".into(),
            display_name: "Acme SSO".into(),
            trusts_email: trust,
            allow_http: true,
        }
    }

    fn claims(base: &str, nonce: &str, email: &str) -> Value {
        let now = chrono::Utc::now().timestamp();
        json!({"iss": base, "aud": "client-id", "sub": "oidc-sub-1", "exp": now + 300, "iat": now, "nonce": nonce,
               "email": email, "email_verified": true, "given_name": "Olive", "family_name": "Idc"})
    }

    fn sign(claims: &Value, key: &EncodingKey, alg: Algorithm, kid: &str) -> String {
        let mut h = Header::new(alg);
        h.kid = Some(kid.into());
        encode(&h, claims, key).unwrap()
    }

    async fn begin(c: &Client, idp: &Idp) -> (Flow, String) {
        let (st, _, resp) = c
            .call("GET", "/auth/sso/oidc/start", None, None, None)
            .await;
        assert_eq!(st, StatusCode::FOUND);
        let loc = url::Url::parse(resp.headers()["location"].to_str().unwrap()).unwrap();
        assert!(loc.path().ends_with("/authorize"));
        let q: HashMap<_, _> = loc.query_pairs().into_owned().collect();
        assert_eq!(q["scope"], "openid email profile");
        assert_eq!(q["code_challenge_method"], "S256");
        *idp.challenge.lock().unwrap() = Some(q["code_challenge"].clone());
        let cookie = resp.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        (
            Flow {
                state: q["state"].clone(),
                cookie,
            },
            q["nonce"].clone(),
        )
    }

    fn new_idp() -> (Idp, Keys) {
        let k = keys();
        (
            Idp {
                base: Default::default(),
                jwks: k.jwks.clone(),
                id_token: Default::default(),
                userinfo: Arc::new(Mutex::new(json!({}))),
                challenge: Default::default(),
                issuer_in_doc: Default::default(),
            },
            k,
        )
    }

    #[tokio::test]
    async fn discovery_insists_on_the_right_issuer_and_https() {
        let (idp, _) = new_idp();
        let base = idp_server(idp.clone()).await;
        let p = crate::identity::oidc::discover(&settings(&base, false))
            .await
            .expect("discovers");
        assert_eq!(
            (p.name, p.userinfo_url.as_str()),
            ("oidc", format!("{base}/userinfo").as_str())
        );
        assert_eq!(p.oidc.as_ref().unwrap().display_name, "Acme SSO");
        // A document that names another issuer is somebody else's.
        *idp.issuer_in_doc.lock().unwrap() = Some("https://evil.example".into());
        assert!(crate::identity::oidc::discover(&settings(&base, false))
            .await
            .err()
            .expect("refused")
            .contains("another issuer"));
        *idp.issuer_in_doc.lock().unwrap() = None;
        // Plain http endpoints are refused unless the operator opted in.
        let mut strict = settings(&base, false);
        strict.allow_http = false;
        assert!(crate::identity::oidc::discover(&strict)
            .await
            .err()
            .expect("refused")
            .contains("https"));
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "needs DATABASE_URL"]
    async fn openid_connect_verifies_the_id_token_before_believing_it(db: PgPool) {
        let (idp, k) = new_idp();
        let base = idp_server(idp.clone()).await;
        let provider = crate::identity::oidc::discover(&settings(&base, false))
            .await
            .unwrap();
        let c = Client::new_with(db.clone(), |cfg| cfg.sso.oauth = vec![provider.clone()]);
        let info = c.call("GET", "/system/info", None, None, None).await.1;
        assert_eq!(
            (info["sso_providers"].clone(), info["sso_oidc_name"].clone()),
            (json!(["oidc"]), json!("Acme SSO"))
        );
        let accounts = || async {
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM users")
                .fetch_one(&db)
                .await
                .unwrap()
        };
        let email = uniq("oidc");

        // Every way of presenting a token that should not be believed ends at the sign-in page.
        let now = chrono::Utc::now().timestamp();
        let good = |nonce: &str| claims(&base, nonce, &email);
        let with = |nonce: &str, f: &dyn Fn(&mut Value)| {
            let mut v = good(nonce);
            f(&mut v);
            v
        };
        let hs_secret = EncodingKey::from_secret(k.jwks.to_string().as_bytes());
        type Attempt<'a> = (&'static str, Box<dyn Fn(&str) -> String + 'a>);
        let attempts: Vec<Attempt> = vec![
            (
                "another nonce",
                Box::new(|_| sign(&good("not-the-nonce"), &k.enc, Algorithm::ES256, "k1")),
            ),
            (
                "no nonce",
                Box::new(|_| {
                    sign(
                        &with("", &|v| {
                            v.as_object_mut().unwrap().remove("nonce");
                        }),
                        &k.enc,
                        Algorithm::ES256,
                        "k1",
                    )
                }),
            ),
            (
                "another audience",
                Box::new(|n| {
                    sign(
                        &with(n, &|v| v["aud"] = json!("someone-else")),
                        &k.enc,
                        Algorithm::ES256,
                        "k1",
                    )
                }),
            ),
            (
                "another issuer",
                Box::new(|n| {
                    sign(
                        &with(n, &|v| v["iss"] = json!("https://evil.example")),
                        &k.enc,
                        Algorithm::ES256,
                        "k1",
                    )
                }),
            ),
            (
                "expired",
                Box::new(|n| {
                    sign(
                        &with(n, &|v| v["exp"] = json!(now - 3600)),
                        &k.enc,
                        Algorithm::ES256,
                        "k1",
                    )
                }),
            ),
            (
                "no subject",
                Box::new(|n| {
                    sign(
                        &with(n, &|v| {
                            v.as_object_mut().unwrap().remove("sub");
                        }),
                        &k.enc,
                        Algorithm::ES256,
                        "k1",
                    )
                }),
            ),
            (
                "signed by another key",
                Box::new(|n| sign(&good(n), &k.other, Algorithm::ES256, "k1")),
            ),
            (
                "unknown key id",
                Box::new(|n| sign(&good(n), &k.enc, Algorithm::ES256, "nope")),
            ),
            (
                "HMAC with a public key as secret",
                Box::new(|n| sign(&good(n), &hs_secret, Algorithm::HS256, "k1")),
            ),
            ("not a token", Box::new(|_| "x.y.z".to_owned())),
        ];
        for (what, make) in &attempts {
            let (flow, nonce) = begin(&c, &idp).await;
            *idp.id_token.lock().unwrap() = make(&nonce);
            *idp.userinfo.lock().unwrap() =
                json!({"sub": "oidc-sub-1", "email": email, "email_verified": true});
            let (loc, cookies) = callback(&c, "oidc", &flow, "good", true).await;
            assert!(loc.ends_with("/login?sso_error=failed"), "{what}: {loc}");
            assert!(refresh_cookie(&cookies).is_none(), "{what}");
        }
        assert_eq!(accounts().await, 0, "none of them created an account");

        // A userinfo answer about somebody else is refused too.
        let (flow, nonce) = begin(&c, &idp).await;
        *idp.id_token.lock().unwrap() = sign(&good(&nonce), &k.enc, Algorithm::ES256, "k1");
        *idp.userinfo.lock().unwrap() = json!({"sub": "another-person", "email": email});
        assert!(callback(&c, "oidc", &flow, "good", true)
            .await
            .0
            .ends_with("sso_error=failed"));

        // A genuine token signs in and creates the account; names come from the token.
        let (flow, nonce) = begin(&c, &idp).await;
        *idp.id_token.lock().unwrap() = sign(&good(&nonce), &k.enc, Algorithm::ES256, "k1");
        *idp.userinfo.lock().unwrap() = json!({"sub": "oidc-sub-1"});
        let (loc, cookies) = callback(&c, "oidc", &flow, "good", true).await;
        assert_eq!(loc, "https://example.test/sso/complete?next=%2F");
        let token = session_from(&c, &cookies).await;
        let me = c.ok("GET", "/me", &token, None).await;
        assert_eq!(
            (
                me["email"].as_str(),
                me["first_name"].as_str(),
                me["last_name"].as_str()
            ),
            (Some(email.as_str()), Some("Olive"), Some("Idc"))
        );
        assert_eq!(accounts().await, 1);

        // A token cannot be replayed into a second sign-in: its nonce belongs to the first.
        let (flow2, _) = begin(&c, &idp).await;
        assert!(callback(&c, "oidc", &flow2, "good", true)
            .await
            .0
            .ends_with("sso_error=failed"));
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "needs DATABASE_URL"]
    async fn unverified_addresses_need_the_operator_to_vouch_for_the_provider(db: PgPool) {
        let (idp, k) = new_idp();
        let base = idp_server(idp.clone()).await;
        let unverified = |nonce: &str, email: &str| {
            let mut v = claims(&base, nonce, email);
            v["email_verified"] = json!(false);
            sign(&v, &k.enc, Algorithm::ES256, "k1")
        };
        for (trust, expect_ok) in [(false, false), (true, true)] {
            let provider = crate::identity::oidc::discover(&settings(&base, trust))
                .await
                .unwrap();
            let c = Client::new_with(db.clone(), |cfg| cfg.sso.oauth = vec![provider.clone()]);
            let email = uniq("kc");
            let (flow, nonce) = begin(&c, &idp).await;
            *idp.id_token.lock().unwrap() = unverified(&nonce, &email);
            let (loc, _) = callback(&c, "oidc", &flow, "good", true).await;
            assert_eq!(
                loc.contains("/sso/complete"),
                expect_ok,
                "trust={trust}: {loc}"
            );
            if !expect_ok {
                assert!(loc.ends_with("sso_error=email_unverified"), "{loc}");
            }
        }
    }
}
