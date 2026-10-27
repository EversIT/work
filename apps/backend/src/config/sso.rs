//! Single sign-on settings: OAuth 2.0 providers, generic OpenID Connect and SAML.

use std::env;

use super::{flag, ConfigError};

/// An OAuth 2.0 / OpenID Connect sign-in provider. The endpoints are fixed per provider (never taken
/// from user input); they are fields so tests can point them at a local stand-in.
#[derive(Clone)]
pub struct OAuthProvider {
    pub name: &'static str,
    pub client_id: String,
    pub client_secret: String,
    pub auth_url: String,
    pub token_url: String,
    pub userinfo_url: String,
    /// GitHub only: the list of e-mail addresses with their verification state.
    pub emails_url: Option<String>,
    pub scope: &'static str,
    /// The provider vouches for the e-mail address without an explicit `email_verified` claim
    /// (Microsoft, where the operator pinned a single tenant).
    pub trusts_email: bool,
    /// Set for a generic OpenID Connect provider: its sign-ins are checked against a signed ID token.
    pub oidc: Option<OidcMeta>,
}

#[derive(Clone)]
pub struct OidcMeta {
    pub issuer: String,
    pub jwks_uri: String,
    /// Shown on the sign-in button (e.g. "Keycloak").
    pub display_name: String,
}

/// What the operator configured for a generic OpenID Connect provider; the endpoints come from the
/// issuer's discovery document when the server starts.
#[derive(Clone)]
pub struct OidcSettings {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub display_name: String,
    /// The provider does not send `email_verified` but is run by the operator (Keycloak, Authentik, ...).
    pub trusts_email: bool,
    pub allow_http: bool,
}

#[derive(Clone)]
pub struct SamlConfig {
    pub entity_id: String,
    pub idp_metadata_xml: String,
    /// PKCS#8 PEM of the key that signs authentication requests (optional).
    pub sp_key_pem: Option<String>,
}

#[derive(Clone, Default)]
pub struct SsoConfig {
    pub oauth: Vec<OAuthProvider>,
    pub saml: Option<SamlConfig>,
    /// Resolved into an `OAuthProvider` named `oidc` by `sso::discover_oidc` at startup.
    pub oidc_pending: Option<OidcSettings>,
}

impl SsoConfig {
    pub fn provider(&self, name: &str) -> Option<&OAuthProvider> {
        self.oauth.iter().find(|p| p.name == name)
    }

    /// Names shown on the sign-in page.
    /// Label of the generic OpenID Connect button, if one is configured.
    pub fn oidc_name(&self) -> Option<String> {
        self.provider("oidc")
            .and_then(|p| p.oidc.as_ref().map(|o| o.display_name.clone()))
    }

    pub fn names(&self) -> Vec<String> {
        self.oauth
            .iter()
            .map(|p| p.name.to_owned())
            .chain(self.saml.iter().map(|_| "saml".to_owned()))
            .collect()
    }
}

fn pair(prefix: &str) -> Option<(String, String)> {
    match (
        env::var(format!("{prefix}_CLIENT_ID")),
        env::var(format!("{prefix}_CLIENT_SECRET")),
    ) {
        (Ok(id), Ok(secret)) if !id.trim().is_empty() && !secret.trim().is_empty() => {
            Some((id.trim().to_owned(), secret.trim().to_owned()))
        }
        _ => None,
    }
}

/// A tenant id or domain, not one of the multi-tenant aliases.
fn pinned_tenant(tenant: &str) -> bool {
    !tenant.is_empty()
        && tenant.len() <= 100
        && tenant
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        && !matches!(
            tenant.to_ascii_lowercase().as_str(),
            "common" | "organizations" | "consumers"
        )
}

pub(super) fn sso_from_env(frontend_url: &str) -> Result<SsoConfig, ConfigError> {
    let mut oauth = Vec::new();
    if let Some((client_id, client_secret)) = pair("WORK_SSO_GOOGLE") {
        oauth.push(OAuthProvider {
            name: "google",
            client_id,
            client_secret,
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            userinfo_url: "https://openidconnect.googleapis.com/v1/userinfo".into(),
            emails_url: None,
            scope: "openid email profile",
            trusts_email: false,
            oidc: None,
        });
    }
    if let Some((client_id, client_secret)) = pair("WORK_SSO_GITHUB") {
        oauth.push(OAuthProvider {
            name: "github",
            client_id,
            client_secret,
            auth_url: "https://github.com/login/oauth/authorize".into(),
            token_url: "https://github.com/login/oauth/access_token".into(),
            userinfo_url: "https://api.github.com/user".into(),
            emails_url: Some("https://api.github.com/user/emails".into()),
            scope: "read:user user:email",
            trusts_email: false,
            oidc: None,
        });
    }
    if let Some((client_id, client_secret)) = pair("WORK_SSO_MICROSOFT") {
        // `common`/`organizations`/`consumers` accept accounts from any tenant, and a tenant admin
        // chooses the e-mail claim freely. The tenant must be pinned to one the operator trusts.
        let tenant = env::var("WORK_SSO_MICROSOFT_TENANT").unwrap_or_default();
        let tenant = tenant.trim();
        if !pinned_tenant(tenant) {
            return Err(ConfigError::Invalid(
                "WORK_SSO_MICROSOFT_TENANT must be your tenant id or domain (not common, organizations or consumers)".into(),
            ));
        }
        oauth.push(OAuthProvider {
            name: "microsoft",
            client_id,
            client_secret,
            auth_url: format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize"),
            token_url: format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/token"),
            userinfo_url: "https://graph.microsoft.com/oidc/userinfo".into(),
            emails_url: None,
            scope: "openid email profile",
            trusts_email: true,
            oidc: None,
        });
    }
    if let Some((client_id, client_secret)) = pair("WORK_SSO_SLACK") {
        oauth.push(OAuthProvider {
            name: "slack",
            client_id,
            client_secret,
            auth_url: "https://slack.com/openid/connect/authorize".into(),
            token_url: "https://slack.com/api/openid.connect.token".into(),
            userinfo_url: "https://slack.com/api/openid.connect.userInfo".into(),
            emails_url: None,
            scope: "openid email profile",
            trusts_email: false,
            oidc: None,
        });
    }
    let saml = match env::var("WORK_SAML_IDP_METADATA_FILE") {
        Ok(path) if !path.trim().is_empty() => {
            let idp_metadata_xml = std::fs::read_to_string(path.trim())
                .map_err(|e| ConfigError::Invalid(format!("WORK_SAML_IDP_METADATA_FILE: {e}")))?;
            let sp_key_pem = match env::var("WORK_SAML_SP_KEY_FILE") {
                Ok(p) if !p.trim().is_empty() => Some(
                    std::fs::read_to_string(p.trim())
                        .map_err(|e| ConfigError::Invalid(format!("WORK_SAML_SP_KEY_FILE: {e}")))?,
                ),
                _ => None,
            };
            Some(SamlConfig {
                entity_id: env::var("WORK_SAML_ENTITY_ID")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
                    .unwrap_or_else(|| format!("{frontend_url}/api/v1/auth/saml/metadata")),
                idp_metadata_xml,
                sp_key_pem,
            })
        }
        _ => None,
    };
    let oidc_pending = match (
        env::var("WORK_SSO_OIDC_ISSUER").ok().map(|v| v.trim().trim_end_matches('/').to_owned()).filter(|v| !v.is_empty()),
        pair("WORK_SSO_OIDC"),
    ) {
        (Some(issuer), Some((client_id, client_secret))) => {
            let allow_http = flag("WORK_SSO_OIDC_ALLOW_HTTP");
            if !(issuer.starts_with("https://") || (allow_http && issuer.starts_with("http://"))) {
                return Err(ConfigError::Invalid(
                    "WORK_SSO_OIDC_ISSUER must be an https:// address (http:// needs WORK_SSO_OIDC_ALLOW_HTTP=true, for local testing)".into(),
                ));
            }
            Some(OidcSettings {
                issuer,
                client_id,
                client_secret,
                display_name: env::var("WORK_SSO_OIDC_NAME")
                    .ok()
                    .map(|v| v.trim().chars().take(40).collect::<String>())
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| "OpenID Connect".into()),
                trusts_email: flag("WORK_SSO_OIDC_TRUST_EMAIL"),
                allow_http,
            })
        }
        (None, None) => None,
        _ => {
            return Err(ConfigError::Invalid(
                "OpenID Connect needs WORK_SSO_OIDC_ISSUER, WORK_SSO_OIDC_CLIENT_ID and WORK_SSO_OIDC_CLIENT_SECRET together".into(),
            ))
        }
    };
    Ok(SsoConfig {
        oauth,
        saml,
        oidc_pending,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microsoft_tenant_must_be_pinned() {
        assert!(pinned_tenant("8a3c2e1f-0b7d-4c55-9a11-2f6d4e8b9c01"));
        assert!(pinned_tenant("corp.onmicrosoft.com"));
        for bad in [
            "",
            "common",
            "Organizations",
            "consumers",
            "a/b",
            "evil.test/../x",
            "a b",
        ] {
            assert!(!pinned_tenant(bad), "{bad}");
        }
    }
}
