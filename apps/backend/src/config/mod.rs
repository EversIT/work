//! Configuration is read from the environment only. No secret has a default value:
//! a missing or weak secret stops the server at startup.

mod sso;
pub mod storage;

use std::{env, net::SocketAddr};

use self::{sso::sso_from_env, storage::storage_from_env};
pub use self::{
    sso::{OAuthProvider, OidcMeta, OidcSettings, SamlConfig, SsoConfig},
    storage::StorageConfig,
};

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: Vec<u8>,
    pub frontend_url: String,
    pub bind_addr: SocketAddr,
    pub registration_enabled: bool,
    /// Use the last `X-Forwarded-For` entry as client IP. Enable only behind a proxy you control.
    pub trust_forwarded_for: bool,
    pub upload_dir: String,
    pub max_upload_bytes: usize,
    pub smtp: Option<SmtpConfig>,
    /// 32-byte key (base64) used to encrypt third-party tokens at rest. Integrations stay
    /// disabled while it is unset.
    pub encryption_key: Option<[u8; 32]>,
    pub slack: Option<SlackConfig>,
    /// Allow automations to call internal addresses. Off by default (SSRF protection); for local development only.
    pub webhook_allow_private: bool,
    /// Allow plain `http://` webhook targets. Off by default.
    pub webhook_allow_http: bool,
    /// Allow automations to e-mail addresses that are not project members. Off by default (spam relay protection).
    pub allow_external_email: bool,
    /// Sign-in, registration and similar attempts allowed per source address and minute.
    pub auth_rate_limit: u32,
    /// Keep rate-limit counters in Postgres so several instances share one budget.
    pub shared_rate_limit: bool,
    /// New accounts must confirm their e-mail address before they exist. Needs SMTP; on by default
    /// whenever SMTP is configured.
    pub require_email_verification: bool,
    /// Sign in with Google, GitHub, Microsoft, Slack or a SAML identity provider.
    pub sso: SsoConfig,
    pub storage: StorageConfig,
    /// Run the background scheduler (date-based automations) in this process. Safe on several
    /// instances at once; switch it off on instances that should not do background work.
    pub scheduler: bool,
    /// Passkeys are bound to this domain (default: the host of `WORK_FRONTEND_URL`). May name a
    /// parent domain so one passkey works across subdomains.
    pub webauthn_rp_id: Option<String>,
}

#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub from: String,
    /// `starttls` (default), `tls` or `none` (only sensible for a local test relay)
    pub tls: String,
}

#[derive(Clone)]
pub struct SlackConfig {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("environment variable {0} is required")]
    Missing(&'static str),
    #[error("{0}")]
    Invalid(String),
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    match env::var(name) {
        Ok(v) if !v.trim().is_empty() => Ok(v),
        _ => Err(ConfigError::Missing(name)),
    }
}

pub(super) fn flag(name: &str) -> bool {
    env::var(name).map(|v| v == "true").unwrap_or(false)
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let jwt_secret = required("WORK_JWT_SECRET")?.into_bytes();
        if jwt_secret.len() < 32 {
            return Err(ConfigError::Invalid(
                "WORK_JWT_SECRET must be at least 32 bytes (e.g. `openssl rand -base64 48`)".into(),
            ));
        }
        let frontend_url = required("WORK_FRONTEND_URL")?
            .trim_end_matches('/')
            .to_owned();
        if !(frontend_url.starts_with("https://") || frontend_url.starts_with("http://localhost")) {
            return Err(ConfigError::Invalid(
                "WORK_FRONTEND_URL must be https:// (http:// is only accepted for localhost)"
                    .into(),
            ));
        }
        let bind_addr = env::var("WORK_BIND_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:3001".into())
            .parse()
            .map_err(|e| ConfigError::Invalid(format!("WORK_BIND_ADDR: {e}")))?;
        let registration_enabled = env::var("WORK_REGISTRATION_ENABLED")
            .map(|v| v != "false")
            .unwrap_or(true);

        let trust_forwarded_for = env::var("WORK_TRUST_FORWARDED_FOR")
            .map(|v| v == "true")
            .unwrap_or(false);

        let upload_dir = env::var("WORK_UPLOAD_DIR").unwrap_or_else(|_| "uploads".into());
        let max_upload_bytes = env::var("WORK_MAX_UPLOAD_BYTES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5 * 1024 * 1024);
        let smtp = match env::var("WORK_SMTP_HOST") {
            Ok(host) if !host.trim().is_empty() => {
                let tls = env::var("WORK_SMTP_TLS").unwrap_or_else(|_| "starttls".into());
                if !matches!(tls.as_str(), "starttls" | "tls" | "none") {
                    return Err(ConfigError::Invalid(
                        "WORK_SMTP_TLS must be starttls, tls or none".into(),
                    ));
                }
                Some(SmtpConfig {
                    host,
                    port: env::var("WORK_SMTP_PORT")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(587),
                    user: env::var("WORK_SMTP_USER").ok().filter(|v| !v.is_empty()),
                    pass: env::var("WORK_SMTP_PASS").ok().filter(|v| !v.is_empty()),
                    from: required("WORK_SMTP_FROM")?,
                    tls,
                })
            }
            _ => None,
        };
        let encryption_key = match env::var("WORK_ENCRYPTION_KEY") {
            Ok(v) if !v.trim().is_empty() => {
                use base64::Engine;
                let raw = base64::engine::general_purpose::STANDARD
                    .decode(v.trim())
                    .map_err(|_| {
                        ConfigError::Invalid("WORK_ENCRYPTION_KEY must be base64".into())
                    })?;
                let key: [u8; 32] = raw.try_into().map_err(|_| {
                    ConfigError::Invalid(
                        "WORK_ENCRYPTION_KEY must decode to 32 bytes (openssl rand -base64 32)"
                            .into(),
                    )
                })?;
                Some(key)
            }
            _ => None,
        };
        let slack = match (
            env::var("WORK_SLACK_CLIENT_ID"),
            env::var("WORK_SLACK_CLIENT_SECRET"),
        ) {
            (Ok(id), Ok(secret)) if !id.is_empty() && !secret.is_empty() => {
                if encryption_key.is_none() {
                    return Err(ConfigError::Invalid(
                        "Slack needs WORK_ENCRYPTION_KEY to store tokens safely".into(),
                    ));
                }
                Some(SlackConfig {
                    client_id: id,
                    client_secret: secret,
                })
            }
            _ => None,
        };

        let shared_rate_limit = match env::var("WORK_RATE_LIMIT_BACKEND").as_deref() {
            Ok("postgres") => true,
            Ok("memory") | Err(_) => false,
            Ok(other) => {
                return Err(ConfigError::Invalid(format!(
                    "WORK_RATE_LIMIT_BACKEND must be memory or postgres, got {other}"
                )))
            }
        };
        let require_email_verification =
            match env::var("WORK_REQUIRE_EMAIL_VERIFICATION").as_deref() {
                Ok("true") => true,
                Ok("false") => false,
                Err(_) | Ok("") => smtp.is_some(),
                Ok(other) => {
                    return Err(ConfigError::Invalid(format!(
                        "WORK_REQUIRE_EMAIL_VERIFICATION must be true or false, got {other}"
                    )))
                }
            };
        if require_email_verification && smtp.is_none() {
            return Err(ConfigError::Invalid(
                "WORK_REQUIRE_EMAIL_VERIFICATION=true needs WORK_SMTP_HOST (nothing could send the link)".into(),
            ));
        }

        let sso = sso_from_env(&frontend_url)?;

        Ok(Self {
            database_url: required("WORK_DATABASE_URL")?,
            jwt_secret,
            frontend_url,
            bind_addr,
            registration_enabled,
            trust_forwarded_for,
            upload_dir,
            max_upload_bytes,
            smtp,
            encryption_key,
            slack,
            webhook_allow_private: flag("WORK_WEBHOOK_ALLOW_PRIVATE"),
            webhook_allow_http: flag("WORK_WEBHOOK_ALLOW_HTTP"),
            allow_external_email: flag("WORK_ALLOW_EXTERNAL_EMAIL"),
            auth_rate_limit: env::var("WORK_AUTH_RATE_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .filter(|n| *n >= 1)
                .unwrap_or(10),
            shared_rate_limit,
            require_email_verification,
            sso,
            storage: storage_from_env()?,
            scheduler: env::var("WORK_SCHEDULER")
                .map(|v| v != "false")
                .unwrap_or(true),
            webauthn_rp_id: env::var("WORK_WEBAUTHN_RP_ID")
                .ok()
                .map(|v| v.trim().to_lowercase())
                .filter(|v| !v.is_empty()),
        })
    }
}

#[cfg(test)]
impl Config {
    /// A complete configuration for tests: nothing optional is switched on, limits are generous
    /// and webhooks may reach local receivers. Tests change what they need.
    pub fn for_tests() -> Self {
        Self {
            database_url: String::new(),
            jwt_secret: b"test-only-secret-test-only-secret-0123".to_vec(),
            frontend_url: "https://example.test".into(),
            bind_addr: "127.0.0.1:0".parse().expect("valid address"),
            registration_enabled: true,
            trust_forwarded_for: true,
            upload_dir: std::env::temp_dir()
                .join("work-test-uploads")
                .to_string_lossy()
                .into_owned(),
            max_upload_bytes: 1024 * 1024,
            smtp: None,
            encryption_key: None,
            slack: None,
            webhook_allow_private: true,
            webhook_allow_http: true,
            allow_external_email: false,
            auth_rate_limit: 1000,
            shared_rate_limit: false,
            require_email_verification: false,
            sso: Default::default(),
            storage: Default::default(),
            scheduler: false,
            webauthn_rp_id: None,
        }
    }
}
