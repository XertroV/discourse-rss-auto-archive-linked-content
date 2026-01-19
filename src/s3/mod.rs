use std::path::Path;

use anyhow::{Context, Result};
use s3::creds::Credentials;
use s3::region::Region;
use s3::Bucket;
use tracing::debug;

use crate::config::Config;

/// S3 client wrapper.
#[derive(Clone)]
pub struct S3Client {
    bucket: Box<Bucket>,
}

impl S3Client {
    /// Create a new S3 client from configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if client initialization fails.
    pub async fn new(config: &Config) -> Result<Self> {
        let access_key = std::env::var("AWS_ACCESS_KEY_ID").context("AWS_ACCESS_KEY_ID not set")?;
        let secret_key =
            std::env::var("AWS_SECRET_ACCESS_KEY").context("AWS_SECRET_ACCESS_KEY not set")?;

        let credentials = Credentials::new(Some(&access_key), Some(&secret_key), None, None, None)
            .context("Failed to create S3 credentials")?;

        let region = if let Some(ref endpoint) = config.s3_endpoint {
            Region::Custom {
                region: config.s3_region.clone(),
                endpoint: endpoint.clone(),
            }
        } else {
            config.s3_region.parse().unwrap_or(Region::UsEast1)
        };

        let bucket = Bucket::new(&config.s3_bucket, region, credentials)
            .context("Failed to create S3 bucket")?;

        // Use path-style for custom endpoints (MinIO, R2, etc.)
        let bucket = if config.s3_endpoint.is_some() {
            bucket.with_path_style()
        } else {
            bucket
        };

        Ok(Self { bucket })
    }

    /// Upload a file to S3.
    ///
    /// # Errors
    ///
    /// Returns an error if the upload fails.
    pub async fn upload_file(&self, local_path: &Path, s3_key: &str) -> Result<()> {
        let content = tokio::fs::read(local_path)
            .await
            .context("Failed to read file for upload")?;

        let content_type = mime_guess::from_path(local_path)
            .first_or_octet_stream()
            .to_string();

        debug!(key = %s3_key, content_type = %content_type, "Uploading file to S3");

        self.bucket
            .put_object_with_content_type(s3_key, &content, &content_type)
            .await
            .context("Failed to upload file to S3")?;

        Ok(())
    }

    /// Upload bytes to S3.
    ///
    /// # Errors
    ///
    /// Returns an error if the upload fails.
    pub async fn upload_bytes(&self, data: &[u8], s3_key: &str, content_type: &str) -> Result<()> {
        debug!(key = %s3_key, content_type = %content_type, "Uploading bytes to S3");

        self.bucket
            .put_object_with_content_type(s3_key, data, content_type)
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
        match self.bucket.head_object(s3_key).await {
            Ok(_) => Ok(true),
            Err(s3::error::S3Error::HttpFailWithBody(404, _)) => Ok(false),
            Err(s3::error::S3Error::HttpFail) => {
                // Check if it was a 404
                Ok(false)
            }
            Err(e) => Err(anyhow::anyhow!("S3 head object failed: {}", e)),
        }
    }

    /// Get the public URL for an object.
    #[must_use]
    pub fn get_public_url(&self, s3_key: &str) -> String {
        format!("https://{}.s3.amazonaws.com/{}", self.bucket.name(), s3_key)
    }

    /// List objects with a given prefix.
    ///
    /// # Errors
    ///
    /// Returns an error if the list request fails.
    pub async fn list_objects(&self, prefix: &str) -> Result<Vec<String>> {
        let results = self
            .bucket
            .list(prefix.to_string(), None)
            .await
            .context("Failed to list S3 objects")?;

        let keys: Vec<String> = results
            .into_iter()
            .flat_map(|result| result.contents)
            .map(|object| object.key)
            .collect();

        debug!(count = keys.len(), prefix = %prefix, "Listed S3 objects");
        Ok(keys)
    }

    /// Delete an object from S3.
    ///
    /// # Errors
    ///
    /// Returns an error if the delete request fails.
    pub async fn delete_object(&self, s3_key: &str) -> Result<()> {
        debug!(key = %s3_key, "Deleting S3 object");

        self.bucket
            .delete_object(s3_key)
            .await
            .context("Failed to delete S3 object")?;

        Ok(())
    }
}

impl std::fmt::Debug for S3Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Client")
            .field("bucket", &self.bucket.name())
            .finish()
    }
}
