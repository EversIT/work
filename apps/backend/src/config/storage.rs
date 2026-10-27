//! Where uploaded files live: local disk, S3 or Azure Blob. Credentials come from the environment only.

use std::env;

use super::{flag, ConfigError};

/// Where uploaded files are stored. Credentials come from the environment only.
#[derive(Clone, Default)]
pub enum StorageConfig {
    /// `WORK_UPLOAD_DIR` on the local disk.
    #[default]
    Local,
    S3(S3Config),
    Azure(AzureConfig),
}

#[derive(Clone)]
pub struct S3Config {
    pub bucket: String,
    pub region: String,
    /// For S3-compatible services (MinIO, Cloudflare R2, ...). Unset = AWS.
    pub endpoint: Option<String>,
    /// Both unset = use the credentials of the environment (instance role, web identity).
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub allow_http: bool,
}

#[derive(Clone)]
pub struct AzureConfig {
    pub account: String,
    pub container: String,
    /// Unset = use the managed identity of the environment.
    pub access_key: Option<String>,
    pub endpoint: Option<String>,
    pub allow_http: bool,
}

pub(super) fn storage_from_env() -> Result<StorageConfig, ConfigError> {
    let opt = |name: &str| {
        env::var(name)
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    match opt("WORK_STORAGE").as_deref() {
        None | Some("local") => Ok(StorageConfig::Local),
        Some("s3") => {
            let (id, secret) = (
                opt("WORK_S3_ACCESS_KEY_ID"),
                opt("WORK_S3_SECRET_ACCESS_KEY"),
            );
            if id.is_some() != secret.is_some() {
                return Err(ConfigError::Invalid(
                    "WORK_S3_ACCESS_KEY_ID and WORK_S3_SECRET_ACCESS_KEY must be set together"
                        .into(),
                ));
            }
            let endpoint = opt("WORK_S3_ENDPOINT");
            let allow_http = flag("WORK_S3_ALLOW_HTTP");
            if endpoint
                .as_deref()
                .is_some_and(|e| e.starts_with("http://"))
                && !allow_http
            {
                return Err(ConfigError::Invalid(
                    "WORK_S3_ENDPOINT is http://; set WORK_S3_ALLOW_HTTP=true to accept that (local testing only)".into(),
                ));
            }
            Ok(StorageConfig::S3(S3Config {
                bucket: opt("WORK_S3_BUCKET").ok_or(ConfigError::Missing("WORK_S3_BUCKET"))?,
                region: opt("WORK_S3_REGION").unwrap_or_else(|| "us-east-1".into()),
                endpoint,
                access_key_id: id,
                secret_access_key: secret,
                allow_http,
            }))
        }
        Some("azure") => {
            let endpoint = opt("WORK_AZURE_ENDPOINT");
            let allow_http = flag("WORK_AZURE_ALLOW_HTTP");
            if endpoint
                .as_deref()
                .is_some_and(|e| e.starts_with("http://"))
                && !allow_http
            {
                return Err(ConfigError::Invalid(
                    "WORK_AZURE_ENDPOINT is http://; set WORK_AZURE_ALLOW_HTTP=true to accept that (local testing only)".into(),
                ));
            }
            Ok(StorageConfig::Azure(AzureConfig {
                account: opt("WORK_AZURE_ACCOUNT")
                    .ok_or(ConfigError::Missing("WORK_AZURE_ACCOUNT"))?,
                container: opt("WORK_AZURE_CONTAINER")
                    .ok_or(ConfigError::Missing("WORK_AZURE_CONTAINER"))?,
                access_key: opt("WORK_AZURE_ACCESS_KEY"),
                endpoint,
                allow_http,
            }))
        }
        Some(other) => Err(ConfigError::Invalid(format!(
            "WORK_STORAGE must be local, s3 or azure, got {other}"
        ))),
    }
}
