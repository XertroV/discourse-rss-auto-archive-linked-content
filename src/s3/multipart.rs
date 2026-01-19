use std::path::Path;

use anyhow::{Context, Result};
use aws_sdk_s3::primitives::ByteStream;
use tracing::{debug, info};

use crate::config::Config;

const CHUNK_SIZE: u64 = 5 * 1024 * 1024; // 5MB - minimum S3 multipart chunk size

/// Streaming S3 uploader using AWS SDK with multipart support.
///
/// This uploader eliminates memory constraints by streaming files
/// to S3 without loading them into memory. Large files are uploaded
/// using multipart uploads, while small files use simple PUT.
#[derive(Clone)]
pub struct StreamingUploader {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl StreamingUploader {
    /// Create a new streaming uploader from configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if client initialization fails.
    pub async fn new(config: &Config) -> Result<Self> {
        // Load AWS config
        let aws_config = if let Some(ref endpoint) = config.s3_endpoint {
            // Custom endpoint (MinIO, R2, etc.)
            let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(aws_config::Region::new(config.s3_region.clone()))
                .endpoint_url(endpoint)
                .load()
                .await;

            // For custom endpoints, force path-style addressing
            aws_config
        } else {
            // Standard AWS S3
            aws_config::defaults(aws_config::BehaviorVersion::latest())
                .region(aws_config::Region::new(config.s3_region.clone()))
                .load()
                .await
        };

        let s3_config = aws_sdk_s3::config::Builder::from(&aws_config)
            .force_path_style(config.s3_endpoint.is_some())
            .build();

        let client = aws_sdk_s3::Client::from_conf(s3_config);

        Ok(Self {
            client,
            bucket: config.s3_bucket.clone(),
        })
    }

    /// Upload a file to S3 using streaming upload.
    ///
    /// Small files (<5MB) use simple PUT for efficiency.
    /// Large files use multipart upload to avoid memory constraints.
    ///
    /// # Errors
    ///
    /// Returns an error if the upload fails.
    pub async fn upload_file(
        &self,
        key: &str,
        file_path: &Path,
        content_type: &str,
        archive_id: Option<i64>,
    ) -> Result<()> {
        let metadata = tokio::fs::metadata(file_path)
            .await
            .context("Failed to get file metadata")?;
        let file_size = metadata.len();

        debug!(
            archive_id,
            key = %key,
            content_type = %content_type,
            size_mb = file_size / 1_024_/ 1_024,
            "Uploading file to S3"
        );

        // Small files: use simple put_object
        if file_size < CHUNK_SIZE {
            self.upload_small_file(key, file_path, content_type, archive_id)
                .await
        } else {
            // Large files: use multipart upload
            self.upload_large_file(key, file_path, content_type, archive_id, file_size)
                .await
        }
    }

    /// Upload a small file (<5MB) using simple PUT operation.
    async fn upload_small_file(
        &self,
        key: &str,
        file_path: &Path,
        content_type: &str,
        archive_id: Option<i64>,
    ) -> Result<()> {
        let body = ByteStream::from_path(file_path)
            .await
            .context("Failed to create ByteStream from file")?;

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(body)
            .content_type(content_type)
            .send()
            .await
            .context("Failed to upload small file to S3")?;

        debug!(archive_id, key = %key, "Small file uploaded successfully");
        Ok(())
    }

    /// Upload a large file (>=5MB) using multipart upload.
    ///
    /// This streams the file in 5MB chunks without loading the entire file into memory.
    async fn upload_large_file(
        &self,
        key: &str,
        file_path: &Path,
        content_type: &str,
        archive_id: Option<i64>,
        file_size: u64,
    ) -> Result<()> {
        info!(
            archive_id,
            key = %key,
            size_mb = file_size / 1_024 / 1_024,
            "Starting multipart upload for large file"
        );

        // 1. Initiate multipart upload
        let create_multipart = self
            .client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .send()
            .await
            .context("Failed to create multipart upload")?;

        let upload_id = create_multipart
            .upload_id()
            .context("No upload ID in response")?;

        debug!(archive_id, upload_id = %upload_id, "Multipart upload initiated");

        // 2. Upload parts
        let upload_result = self
            .upload_parts(key, file_path, upload_id, archive_id, file_size)
            .await;

        match upload_result {
            Ok(completed_parts) => {
                // 3. Complete multipart upload
                let completed_upload = aws_sdk_s3::types::CompletedMultipartUpload::builder()
                    .set_parts(Some(completed_parts))
                    .build();

                self.client
                    .complete_multipart_upload()
                    .bucket(&self.bucket)
                    .key(key)
                    .upload_id(upload_id)
                    .multipart_upload(completed_upload)
                    .send()
                    .await
                    .context("Failed to complete multipart upload")?;

                info!(archive_id, key = %key, "Multipart upload completed successfully");
                Ok(())
            }
            Err(e) => {
                // Abort multipart upload on error
                debug!(archive_id, upload_id = %upload_id, error = %e, "Aborting multipart upload due to error");
                let _ = self
                    .client
                    .abort_multipart_upload()
                    .bucket(&self.bucket)
                    .key(key)
                    .upload_id(upload_id)
                    .send()
                    .await;

                Err(e)
            }
        }
    }

    /// Upload individual parts for multipart upload.
    async fn upload_parts(
        &self,
        key: &str,
        file_path: &Path,
        upload_id: &str,
        archive_id: Option<i64>,
        file_size: u64,
    ) -> Result<Vec<aws_sdk_s3::types::CompletedPart>> {
        use tokio::io::AsyncReadExt;

        let mut file = tokio::fs::File::open(file_path)
            .await
            .context("Failed to open file for multipart upload")?;

        let mut completed_parts = Vec::new();
        let mut part_number = 1;
        let mut bytes_uploaded = 0u64;

        loop {
            // Read chunk into buffer
            let mut buffer = vec![0u8; CHUNK_SIZE as usize];
            let bytes_read = file
                .read(&mut buffer)
                .await
                .context("Failed to read file chunk")?;

            if bytes_read == 0 {
                break; // EOF
            }

            // Truncate buffer to actual bytes read
            buffer.truncate(bytes_read);
            bytes_uploaded += bytes_read as u64;

            // Upload part
            let upload_part_output = self
                .client
                .upload_part()
                .bucket(&self.bucket)
                .key(key)
                .upload_id(upload_id)
                .part_number(part_number)
                .body(ByteStream::from(buffer))
                .send()
                .await
                .with_context(|| format!("Failed to upload part {part_number}"))?;

            let etag = upload_part_output
                .e_tag()
                .context("No ETag in upload part response")?
                .to_string();

            completed_parts.push(
                aws_sdk_s3::types::CompletedPart::builder()
                    .part_number(part_number)
                    .e_tag(etag)
                    .build(),
            );

            debug!(
                archive_id,
                part_number,
                progress_pct = (bytes_uploaded * 100 / file_size),
                "Uploaded part {part_number}/{}",
                (file_size + CHUNK_SIZE - 1) / CHUNK_SIZE
            );

            part_number += 1;
        }

        Ok(completed_parts)
    }

    /// Copy an object within S3 using server-side copy.
    ///
    /// This is much faster than download+re-upload and uses no bandwidth.
    ///
    /// # Errors
    ///
    /// Returns an error if the copy fails.
    pub async fn copy_object(&self, source_key: &str, dest_key: &str) -> Result<()> {
        let copy_source = format!("{}/{}", self.bucket, source_key);

        debug!(source = %source_key, dest = %dest_key, "Copying S3 object (server-side)");

        self.client
            .copy_object()
            .bucket(&self.bucket)
            .copy_source(copy_source)
            .key(dest_key)
            .send()
            .await
            .context("Failed to copy S3 object")?;

        debug!(source = %source_key, dest = %dest_key, "Successfully copied S3 object");
        Ok(())
    }

    /// Upload bytes to S3.
    ///
    /// # Errors
    ///
    /// Returns an error if the upload fails.
    pub async fn upload_bytes(&self, data: &[u8], s3_key: &str, content_type: &str) -> Result<()> {
        debug!(key = %s3_key, content_type = %content_type, size = data.len(), "Uploading bytes to S3");

        let body = ByteStream::from(data.to_vec());

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
}

impl std::fmt::Debug for StreamingUploader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamingUploader")
            .field("bucket", &self.bucket)
            .finish()
    }
}
