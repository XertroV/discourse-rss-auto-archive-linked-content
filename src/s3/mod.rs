use std::path::Path;

use anyhow::{Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_s3::config::Credentials;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use tracing::debug;

use crate::config::Config;

/// S3 client wrapper.
#[derive(Clone)]
pub struct S3Client {
    client: Client,
    bucket: String,
}

impl S3Client {
    /// Create a new S3 client from configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if client initialization fails.
    pub async fn new(config: &Config) -> Result<Self> {
        let access_key =
            std::env::var("AWS_ACCESS_KEY_ID").context("AWS_ACCESS_KEY_ID not set")?;
        let secret_key =
            std::env::var("AWS_SECRET_ACCESS_KEY").context("AWS_SECRET_ACCESS_KEY not set")?;

        let credentials = Credentials::new(access_key, secret_key, None, None, "env");

        let mut s3_config_builder = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new(config.s3_region.clone()))
            .credentials_provider(credentials);

        // Set custom endpoint if provided (for MinIO, R2, etc.)
        if let Some(ref endpoint) = config.s3_endpoint {
            s3_config_builder = s3_config_builder
                .endpoint_url(endpoint)
                .force_path_style(true);
        }

        let s3_config = s3_config_builder.build();
        let client = Client::from_conf(s3_config);

        Ok(Self {
            client,
            bucket: config.s3_bucket.clone(),
        })
    }

    /// Upload a file to S3.
    ///
    /// # Errors
    ///
    /// Returns an error if the upload fails.
    pub async fn upload_file(&self, local_path: &Path, s3_key: &str) -> Result<()> {
        let body = ByteStream::from_path(local_path)
            .await
            .context("Failed to read file for upload")?;

        let content_type = mime_guess::from_path(local_path)
            .first_or_octet_stream()
            .to_string();

        debug!(key = %s3_key, content_type = %content_type, "Uploading file to S3");

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(s3_key)
            .body(body)
            .content_type(content_type)
            .send()
            .await
            .context("Failed to upload file to S3")?;

        Ok(())
    }

    /// Upload bytes to S3.
    ///
    /// # Errors
    ///
    /// Returns an error if the upload fails.
    pub async fn upload_bytes(
        &self,
        data: &[u8],
        s3_key: &str,
        content_type: &str,
    ) -> Result<()> {
        let body = ByteStream::from(data.to_vec());

        debug!(key = %s3_key, content_type = %content_type, "Uploading bytes to S3");

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(s3_key)
            .body(body)
            .content_type(content_type)
            .send()
            .await
            .context("Failed to upload bytes to S3")?;

        Ok(())
    }

    /// Check if an object exists in S3.
    ///
    /// # Errors
    ///
    /// Returns an error if the head request fails for reasons other than not found.
    pub async fn object_exists(&self, s3_key: &str) -> Result<bool> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(s3_key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let service_error = e.into_service_error();
                if service_error.is_not_found() {
                    Ok(false)
                } else {
                    Err(anyhow::anyhow!("S3 head object failed: {:?}", service_error))
                }
            }
        }
    }

    /// Get the public URL for an object.
    #[must_use]
    pub fn get_public_url(&self, s3_key: &str) -> String {
        format!("https://{}.s3.amazonaws.com/{}", self.bucket, s3_key)
    }
}

impl std::fmt::Debug for S3Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Client")
            .field("bucket", &self.bucket)
            .finish()
    }
}
