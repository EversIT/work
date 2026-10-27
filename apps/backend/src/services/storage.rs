//! Where uploaded files live: the local disk, an S3-compatible bucket or an Azure Blob container.
//! Objects are always named by a server-generated UUID, never by anything the client sent, and the
//! bucket or container is private: files are only ever served through the authenticated API.

use std::sync::Arc;

use bytes::Bytes;
use object_store::{
    aws::AmazonS3Builder, azure::MicrosoftAzureBuilder, local::LocalFileSystem, path::Path,
    ObjectStore, ObjectStoreExt, PutMode, PutOptions, PutPayload,
};
use uuid::Uuid;

use crate::{
    config::{Config, StorageConfig},
    error::ApiError,
};

#[derive(Clone)]
pub struct Storage(Arc<dyn ObjectStore>);

fn internal(e: impl std::error::Error + Send + Sync + 'static) -> ApiError {
    ApiError::Internal(Box::new(e))
}

impl Storage {
    pub fn from_config(config: &Config) -> Result<Self, String> {
        let store: Arc<dyn ObjectStore> = match &config.storage {
            StorageConfig::Local => {
                std::fs::create_dir_all(&config.upload_dir)
                    .map_err(|e| format!("upload directory {}: {e}", config.upload_dir))?;
                Arc::new(
                    LocalFileSystem::new_with_prefix(&config.upload_dir)
                        .map_err(|e| e.to_string())?,
                )
            }
            StorageConfig::S3(s3) => {
                let mut b = AmazonS3Builder::new()
                    .with_bucket_name(&s3.bucket)
                    .with_region(&s3.region)
                    .with_allow_http(s3.allow_http);
                if let Some(endpoint) = &s3.endpoint {
                    // Custom endpoints (MinIO, R2, ...) use path-style addressing.
                    b = b
                        .with_endpoint(endpoint)
                        .with_virtual_hosted_style_request(false);
                }
                // Without keys the instance role / web identity of the environment is used.
                if let (Some(id), Some(secret)) = (&s3.access_key_id, &s3.secret_access_key) {
                    b = b.with_access_key_id(id).with_secret_access_key(secret);
                }
                Arc::new(b.build().map_err(|e| e.to_string())?)
            }
            StorageConfig::Azure(az) => {
                let mut b = MicrosoftAzureBuilder::new()
                    .with_account(&az.account)
                    .with_container_name(&az.container)
                    .with_allow_http(az.allow_http);
                if let Some(endpoint) = &az.endpoint {
                    b = b.with_endpoint(endpoint.clone());
                }
                // Without a key, a managed identity of the environment is used.
                if let Some(key) = &az.access_key {
                    b = b.with_access_key(key);
                }
                Arc::new(b.build().map_err(|e| e.to_string())?)
            }
        };
        Ok(Self(store))
    }

    fn path(key: Uuid) -> Path {
        Path::from(key.to_string())
    }

    /// Fails if the object exists: a key is written exactly once.
    pub async fn put(&self, key: Uuid, bytes: Bytes) -> Result<(), ApiError> {
        self.0
            .put_opts(
                &Self::path(key),
                PutPayload::from_bytes(bytes),
                PutOptions {
                    mode: PutMode::Create,
                    ..Default::default()
                },
            )
            .await
            .map(|_| ())
            .map_err(internal)
    }

    pub async fn get(&self, key: Uuid) -> Result<Bytes, ApiError> {
        match self.0.get(&Self::path(key)).await {
            Ok(r) => r.bytes().await.map_err(internal),
            Err(object_store::Error::NotFound { .. }) => Err(ApiError::NotFound),
            Err(e) => Err(internal(e)),
        }
    }

    pub async fn delete(&self, key: Uuid) {
        if let Err(e) = self.0.delete(&Self::path(key)).await {
            tracing::warn!(error = %e, %key, "deleting a stored file failed");
        }
    }
}
