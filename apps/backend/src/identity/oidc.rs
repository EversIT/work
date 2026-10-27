//! Generic OpenID Connect sign-in (Keycloak, Authentik, Okta, Auth0, Entra ID with a pinned issuer, ...).
//!
//! It is the OAuth flow of `sso.rs` (authorization code + PKCE, one-time state tied to the browser)
//! plus what makes it OpenID Connect: the endpoints come from the issuer's discovery document, the
//! authorization request carries a `nonce`, and the ID token that the token endpoint returns is
//! verified before anything in it is believed:
//! * signature against the issuer's published keys (only RS256 and ES256; `none` and HMAC are refused,
//!   so a token cannot be forged by confusing the algorithm);
//! * `iss` equals the configured issuer, `aud` contains this client (and `azp` matches when there are
//!   several audiences), `exp` has not passed, `sub` exists, `nonce` is the one of this very sign-in;
//! * the `sub` of the userinfo response, when there is one, equals the token's.

use std::time::Duration;

use jsonwebtoken::{
    decode, decode_header,
    jwk::{Jwk, JwkSet},
    Algorithm, DecodingKey, Validation,
};
use serde_json::Value;

use crate::config::{OAuthProvider, OidcMeta, OidcSettings};

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())
}

fn endpoint(doc: &Value, key: &str, allow_http: bool) -> Result<String, String> {
    let url = doc[key]
        .as_str()
        .ok_or_else(|| format!("the discovery document has no {key}"))?;
    if url.starts_with("https://") || (allow_http && url.starts_with("http://")) {
        Ok(url.to_owned())
    } else {
        Err(format!("{key} is not an https address"))
    }
}

/// Reads the issuer's discovery document and builds the provider. Called once at startup; a wrong
/// issuer or an unsuitable provider stops the server with a clear message instead of failing later.
pub async fn discover(s: &OidcSettings) -> Result<OAuthProvider, String> {
    let url = format!("{}/.well-known/openid-configuration", s.issuer);
    let doc: Value = client()?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("cannot reach {url}: {e}"))?
        .error_for_status()
        .map_err(|e| format!("{url}: {e}"))?
        .json()
        .await
        .map_err(|e| format!("{url} is not JSON: {e}"))?;
    // The document must be about the issuer that was asked for, or somebody else's endpoints could be injected.
    if doc["issuer"].as_str() != Some(s.issuer.as_str()) {
        return Err(format!(
            "the discovery document names another issuer than {}",
            s.issuer
        ));
    }
    let supports = |key: &str, wanted: &[&str]| {
        doc[key].as_array().is_none_or(|list| {
            list.iter()
                .any(|v| v.as_str().is_some_and(|x| wanted.contains(&x)))
        })
    };
    if !supports("code_challenge_methods_supported", &["S256"]) {
        return Err("the provider does not support PKCE (S256)".into());
    }
    if !supports("id_token_signing_alg_values_supported", &["RS256", "ES256"]) {
        return Err("the provider signs ID tokens with neither RS256 nor ES256".into());
    }
    Ok(OAuthProvider {
        name: "oidc",
        client_id: s.client_id.clone(),
        client_secret: s.client_secret.clone(),
        auth_url: endpoint(&doc, "authorization_endpoint", s.allow_http)?,
        token_url: endpoint(&doc, "token_endpoint", s.allow_http)?,
        userinfo_url: endpoint(&doc, "userinfo_endpoint", s.allow_http).unwrap_or_default(),
        emails_url: None,
        scope: "openid email profile",
        trusts_email: s.trusts_email,
        oidc: Some(OidcMeta {
            issuer: s.issuer.clone(),
            jwks_uri: endpoint(&doc, "jwks_uri", s.allow_http)?,
            display_name: s.display_name.clone(),
        }),
    })
}

fn pick_key<'a>(set: &'a JwkSet, kid: Option<&str>) -> Option<&'a Jwk> {
    match kid {
        Some(k) => set.find(k),
        None if set.keys.len() == 1 => set.keys.first(),
        None => None,
    }
}

/// Verifies an ID token and returns its claims.
pub async fn verify_id_token(
    meta: &OidcMeta,
    client_id: &str,
    id_token: &str,
    nonce: Option<&str>,
) -> Result<Value, &'static str> {
    let header = decode_header(id_token).map_err(|_| "failed")?;
    if !matches!(header.alg, Algorithm::RS256 | Algorithm::ES256) {
        return Err("failed");
    }
    let set: JwkSet = client()
        .map_err(|_| "failed")?
        .get(&meta.jwks_uri)
        .send()
        .await
        .map_err(|_| "provider_unreachable")?
        .json()
        .await
        .map_err(|_| "failed")?;
    let jwk = pick_key(&set, header.kid.as_deref()).ok_or("failed")?;
    let key = DecodingKey::from_jwk(jwk).map_err(|_| "failed")?;
    let mut validation = Validation::new(header.alg); // pinned to the algorithm of a key we hold
    validation.set_audience(&[client_id]);
    validation.set_issuer(&[meta.issuer.as_str()]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    validation.leeway = 60;
    let claims = decode::<Value>(id_token, &key, &validation)
        .map_err(|_| "failed")?
        .claims;
    // Several audiences are only acceptable when the token says it was issued to this client.
    if claims["aud"].as_array().is_some_and(|a| a.len() > 1)
        && claims["azp"].as_str() != Some(client_id)
    {
        return Err("failed");
    }
    // The nonce binds the token to the sign-in this browser started: no replay of somebody else's token.
    match nonce {
        Some(n) if claims["nonce"].as_str() == Some(n) => {}
        _ => return Err("failed"),
    }
    if claims["sub"].as_str().is_none_or(str::is_empty) {
        return Err("failed");
    }
    Ok(claims)
}
