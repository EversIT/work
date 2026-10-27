//! SAML 2.0 single sign-on (SP-initiated, HTTP-Redirect request, HTTP-POST response).
//!
//! XML signature handling is done by the pure-Rust `saml` crate, which rejects duplicate IDs and
//! only ever hands out the signed element (the classic signature-wrapping attacks). Beyond that:
//! * the IdP is the one in the operator's metadata file; nothing about it comes from a request;
//! * assertions must be signed, are checked against the audience, destination, validity window
//!   and the `InResponseTo` of the request this very browser started;
//! * the request tracker is sealed with a key derived from the server secret and kept server-side
//!   together with the one-time `state` (the SAML `RelayState`), which a cookie ties to the browser;
//! * unsolicited (IdP-initiated) responses are refused;
//! * the address from the assertion is trusted like a verified one, because the operator chose
//!   this IdP. It is still never used to merge into an existing account.

use std::{
    net::SocketAddr,
    sync::OnceLock,
    time::{Duration, SystemTime},
};

use axum::{
    extract::{ConnectInfo, State},
    http::{header::CONTENT_TYPE, HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
    Form,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac};
use saml::{
    Binding, ConsumeResponse, Dispatch, IdpDescriptor, InMemoryReplayCache, KeyPair, LoginTracker,
    NameIdFormat, PeerCryptoPolicy, ReplayMode, ServiceProvider, ServiceProviderConfig,
    SpWantSigned, SsoResponseBinding, SsoResponseEndpoint, StartLogin,
};
use serde::Deserialize;
use sha2::Sha256;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::{
    config::SamlConfig,
    error::{ApiError, ApiResult},
    identity::oauth::{binding_cookie, finish, redeem_state, store_state, ExternalIdentity},
    routes::auth::{client_ip, throttle},
    state::AppState,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(saml_metadata))
        .routes(routes!(saml_acs))
}

const TRACKER_MAX_AGE: Duration = Duration::from_secs(10 * 60);

fn acs_url(state: &AppState) -> String {
    format!("{}/api/v1/auth/saml/acs", state.config.frontend_url)
}

/// Key for sealing request trackers, derived so the server secret itself is never used directly.
fn tracker_key(state: &AppState) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(&state.config.jwt_secret).expect("any key length");
    mac.update(b"work saml login tracker v1");
    mac.finalize().into_bytes().into()
}

fn service_provider(state: &AppState, cfg: &SamlConfig) -> ApiResult<ServiceProvider> {
    let signing_key = cfg
        .sp_key_pem
        .as_deref()
        .map(|pem| KeyPair::from_pkcs8_pem(pem.as_bytes()))
        .transpose()
        .map_err(|e| ApiError::Internal(Box::new(e)))?;
    ServiceProvider::new(ServiceProviderConfig {
        entity_id: cfg.entity_id.clone(),
        acs: vec![SsoResponseEndpoint::post(acs_url(state), 0, true)],
        slo: vec![],
        name_id_formats: vec![NameIdFormat::EmailAddress, NameIdFormat::Persistent],
        sign_authn_requests: signing_key.is_some(),
        signing_key,
        decryption_key: None,
        want_signed: SpWantSigned {
            response: false,
            assertions: true,
        },
        allow_unsolicited: false,
        logout_signing: Default::default(),
        logout_want_signed: Default::default(),
        default_peer_crypto_policy: PeerCryptoPolicy::strong_defaults(),
        outbound_signature_algorithm: saml::SignatureAlgorithm::RsaSha256,
        outbound_digest_algorithm: saml::DigestAlgorithm::Sha256,
    })
    .map_err(|e| ApiError::Internal(Box::new(e)))
}

fn idp(cfg: &SamlConfig) -> ApiResult<IdpDescriptor> {
    IdpDescriptor::from_metadata_xml(cfg.idp_metadata_xml.as_bytes())
        .map_err(|e| ApiError::Internal(Box::new(e)))
}

/// Starts a sign-in: stores state, tracker and browser binding, returns the redirect.
pub async fn begin(
    state: &AppState,
    user_id: Option<Uuid>,
    next: &str,
) -> ApiResult<(String, HeaderValue)> {
    let cfg = state.config.sso.saml.as_ref().ok_or(ApiError::NotFound)?;
    let sp = service_provider(state, cfg)?;
    let idp = idp(cfg)?;
    // The state doubles as RelayState, so it must exist before the request is built.
    let placeholder = store_state(state, "saml", None, Some(""), None, user_id, next).await?;
    let (token, binding) = placeholder;
    let started = sp
        .start_login(
            &idp,
            StartLogin {
                relay_state: Some(&token),
                binding: Binding::HttpRedirect,
                force_authn: false,
                is_passive: false,
                requested_name_id_format: None,
                requested_authn_context: None,
                acs_index: None,
                acs_url: None,
                response_binding: None,
            },
        )
        .map_err(|e| ApiError::Internal(Box::new(e)))?;
    let sealed = started
        .tracker
        .to_payload()
        .seal(&tracker_key(state))
        .map_err(|e| ApiError::Internal(Box::new(e)))?;
    sqlx::query("UPDATE sso_states SET saml_tracker = $2 WHERE state_hash = $1")
        .bind(crate::security::auth::hash_token(&token))
        .bind(&sealed)
        .execute(&state.db)
        .await?;
    let Dispatch::Redirect(url) = started.dispatch else {
        return Err(ApiError::Internal("SAML redirect binding expected".into()));
    };
    Ok((url.into(), binding_cookie(state, &binding, 600, true)))
}

#[utoipa::path(get, operation_id = "samlMetadata", path = "/auth/saml/metadata", tag = "auth",
    responses((status = 200, content_type = "application/samlmetadata+xml"), (status = 404)))]
async fn saml_metadata(State(s): State<AppState>) -> ApiResult<Response> {
    let cfg = s.config.sso.saml.as_ref().ok_or(ApiError::NotFound)?;
    let xml = service_provider(&s, cfg)?
        .metadata_xml(false)
        .map_err(|e| ApiError::Internal(Box::new(e)))?;
    Ok((
        [(
            CONTENT_TYPE,
            HeaderValue::from_static("application/samlmetadata+xml"),
        )],
        xml,
    )
        .into_response())
}

#[derive(Deserialize)]
struct AcsForm {
    #[serde(rename = "SAMLResponse")]
    saml_response: String,
    #[serde(rename = "RelayState")]
    relay_state: Option<String>,
}

fn replay_cache() -> &'static InMemoryReplayCache {
    static CACHE: OnceLock<InMemoryReplayCache> = OnceLock::new();
    CACHE.get_or_init(InMemoryReplayCache::default)
}

const EMAIL_ATTRS: &[&str] = &[
    "email",
    "mail",
    "emailaddress",
    "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/emailaddress",
    "urn:oid:0.9.2342.19200300.100.1.3",
];
const FIRST_ATTRS: &[&str] = &[
    "givenname",
    "firstname",
    "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/givenname",
    "urn:oid:2.5.4.42",
];
const LAST_ATTRS: &[&str] = &[
    "surname",
    "lastname",
    "sn",
    "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/surname",
    "urn:oid:2.5.4.4",
];

fn attribute(attrs: &[saml::Attribute], names: &[&str]) -> Option<String> {
    names.iter().find_map(|wanted| {
        attrs
            .iter()
            .find(|a| {
                a.name.eq_ignore_ascii_case(wanted)
                    || a.friendly_name
                        .as_deref()
                        .is_some_and(|f| f.eq_ignore_ascii_case(wanted))
            })
            .and_then(|a| a.values.first())
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    })
}

/// Maps a validated assertion to an identity. Exposed for tests.
pub(crate) fn to_identity(
    name_id: &str,
    name_id_is_email: bool,
    attrs: &[saml::Attribute],
) -> ExternalIdentity {
    let email = attribute(attrs, EMAIL_ATTRS)
        .or_else(|| name_id_is_email.then(|| name_id.to_owned()))
        .filter(|e| e.contains('@'));
    ExternalIdentity {
        provider: "saml".into(),
        subject: name_id.to_owned(),
        email,
        first_name: attribute(attrs, FIRST_ATTRS),
        last_name: attribute(attrs, LAST_ATTRS),
    }
}

#[utoipa::path(post, operation_id = "samlAcs", path = "/auth/saml/acs", tag = "auth",
    responses((status = 302)))]
async fn saml_acs(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Form(form): Form<AcsForm>,
) -> Response {
    let clear = binding_cookie(&s, "", 0, true);
    let fail = |code: &str| {
        let mut resp = crate::identity::oauth::redirect_to(
            &format!("{}/login?sso_error={code}", s.config.frontend_url),
            vec![],
        );
        resp.headers_mut()
            .append(axum::http::header::SET_COOKIE, clear.clone());
        resp
    };
    if throttle(&s, &format!("sso:{}", client_ip(&s, &headers, peer)))
        .await
        .is_err()
    {
        return fail("too_many");
    }
    let result = async {
        let cfg = s.config.sso.saml.as_ref().ok_or("failed")?;
        let relay = form.relay_state.as_deref().ok_or("expired")?;
        let redeemed = redeem_state(&s, &headers, "saml", relay).await?;
        let sealed = redeemed
            .saml_tracker
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or("failed")?;
        let now = SystemTime::now();
        let tracker = LoginTracker::open(sealed, &tracker_key(&s), now, TRACKER_MAX_AGE)
            .map_err(|_| "expired")?;
        let raw = STANDARD
            .decode(form.saml_response.trim())
            .map_err(|_| "failed")?;
        let sp = service_provider(&s, cfg).map_err(|_| "failed")?;
        let idp = idp(cfg).map_err(|_| "failed")?;
        let acs = acs_url(&s);
        let identity = sp
            .consume_response(ConsumeResponse {
                idp: &idp,
                peer_crypto_policy: None,
                saml_response: &raw,
                binding: SsoResponseBinding::HttpPost,
                relay_state: Some(relay),
                tracker: Some(&tracker),
                expected_destination: &acs,
                now,
                clock_skew: Duration::from_secs(60),
                replay_cache: Some(replay_cache()),
                replay_mode: ReplayMode::All,
                holder_of_key_cert: None,
            })
            .map_err(|e| {
                tracing::warn!(error = %e, "SAML response rejected");
                "saml_invalid"
            })?;
        let name_id = identity.name_id();
        let ext = to_identity(
            &name_id.value,
            matches!(name_id.format, NameIdFormat::EmailAddress),
            identity.attributes(),
        );
        finish(&s, ext, redeemed.user_id, &redeemed.next).await
    }
    .await;
    match result {
        Ok((location, mut cookies)) => {
            cookies.push(clear.clone());
            crate::identity::oauth::redirect_to(&location, cookies)
        }
        Err(code) => fail(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attr(name: &str, value: &str) -> saml::Attribute {
        saml::Attribute {
            name: name.into(),
            name_format: None,
            friendly_name: None,
            values: vec![value.into()],
        }
    }

    #[test]
    fn identity_prefers_attributes_and_falls_back_to_an_email_name_id() {
        let attrs = [
            attr(
                "http://schemas.xmlsoap.org/ws/2005/05/identity/claims/emailaddress",
                "ada@corp.test",
            ),
            attr("givenName", "Ada"),
            attr("sn", "Lovelace"),
        ];
        let id = to_identity("opaque-1", false, &attrs);
        assert_eq!(id.subject, "opaque-1");
        assert_eq!(id.email.as_deref(), Some("ada@corp.test"));
        assert_eq!(
            (id.first_name.as_deref(), id.last_name.as_deref()),
            (Some("Ada"), Some("Lovelace"))
        );

        let id = to_identity("bob@corp.test", true, &[]);
        assert_eq!(id.email.as_deref(), Some("bob@corp.test"));
        // A persistent, opaque NameID is not an address.
        assert_eq!(to_identity("opaque-2", false, &[]).email, None);
        assert_eq!(to_identity("no-at-sign", true, &[]).email, None);
    }
}
