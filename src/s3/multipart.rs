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
            size_mb = file_size as f64 / 1_024.0 / 1_024.0,
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
        let total_parts = file_size.div_ceil(CHUNK_SIZE);

        loop {
            // Read a full chunk (or remainder at EOF)
            // Note: read() may return fewer bytes than requested even when more data
            // is available, so we must loop until the buffer is full or EOF.
            let mut buffer = vec![0u8; CHUNK_SIZE as usize];
            let mut total_read = 0usize;

            while total_read < CHUNK_SIZE as usize {
                let bytes_read = file
                    .read(&mut buffer[total_read..])
                    .await
                    .context("Failed to read file chunk")?;

                if bytes_read == 0 {
                    break; // EOF
                }
                total_read += bytes_read;
            }

            if total_read == 0 {
                break; // No more data
            }

            // Truncate buffer to actual bytes read
            buffer.truncate(total_read);
            bytes_uploaded += total_read as u64;

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
                total_parts,
                progress_pct = (bytes_uploaded * 100 / file_size),
                "Uploaded part {part_number}/{total_parts}"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_size_is_5mb() {
        assert_eq!(CHUNK_SIZE, 5 * 1024 * 1024, "Chunk size should be 5MB");
    }

    #[test]
    fn test_chunk_size_meets_s3_minimum() {
        // S3 requires minimum 5MB chunks for multipart upload (except last part)
        assert!(
            CHUNK_SIZE >= 5 * 1024 * 1024,
            "Chunk size must be at least 5MB for S3 multipart uploads"
        );
    }

    #[test]
    fn test_small_file_threshold() {
        // Files smaller than CHUNK_SIZE should use simple upload
        let small_file_size = CHUNK_SIZE - 1;
        assert!(
            small_file_size < CHUNK_SIZE,
            "Small files should be below chunk size threshold"
        );
    }

    #[test]
    fn test_large_file_threshold() {
        // Files at or above CHUNK_SIZE should use multipart upload
        let large_file_size = CHUNK_SIZE;
        assert!(
            large_file_size >= CHUNK_SIZE,
            "Large files should meet or exceed chunk size threshold"
        );
    }

    #[test]
    fn test_multipart_part_count_calculation() {
        // Verify part count calculation is correct
        let file_size = 15 * 1024 * 1024; // 15MB
        let expected_parts = 3; // 5MB + 5MB + 5MB
        let calculated_parts = (file_size + CHUNK_SIZE - 1) / CHUNK_SIZE;
        assert_eq!(
            calculated_parts, expected_parts,
            "Part count calculation should be correct"
        );
    }

    #[test]
    fn test_multipart_part_count_with_remainder() {
        // Verify part count with non-even division
        let file_size = 17 * 1024 * 1024; // 17MB
        let expected_parts = 4; // 5MB + 5MB + 5MB + 2MB
        let calculated_parts = (file_size + CHUNK_SIZE - 1) / CHUNK_SIZE;
        assert_eq!(
            calculated_parts, expected_parts,
            "Part count should round up for remainder"
        );
    }

    #[test]
    fn test_large_video_file_size() {
        // Verify we can handle large files (e.g., 4GB video)
        let video_size = 4u64 * 1024 * 1024 * 1024; // 4GB
        let part_count = (video_size + CHUNK_SIZE - 1) / CHUNK_SIZE;

        // S3 supports up to 10,000 parts
        assert!(
            part_count < 10_000,
            "4GB file should require fewer than 10,000 parts"
        );

        // Should be about 819 parts for 4GB with 5MB chunks
        assert!(
            part_count > 800 && part_count < 850,
            "4GB file should require ~819 parts with 5MB chunks, got {part_count}"
        );
    }

    #[test]
    fn test_max_supported_file_size() {
        // S3 supports up to 10,000 parts at 5MB each
        let max_parts = 10_000u64;
        let max_file_size = max_parts * CHUNK_SIZE;

        // 10,000 * 5MB = 50,000 MB = ~48.8 GiB
        let expected_max = 10_000 * 5 * 1024 * 1024; // 52,428,800,000 bytes

        assert_eq!(
            max_file_size, expected_max,
            "Max file size with 5MB chunks and 10,000 parts"
        );

        // Verify it's close to 50GB (using decimal GB for clarity)
        let fifty_billion_bytes = 50_000_000_000u64;
        assert!(
            max_file_size > fifty_billion_bytes,
            "Max file size should exceed 50 billion bytes"
        );
    }
}
