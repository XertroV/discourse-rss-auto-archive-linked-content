# Stage 2: Implement Streaming Upload for Large Files

## Overview

Replace the current memory-intensive S3 upload process with multipart streaming uploads to eliminate memory constraints when archiving large YouTube videos.

## Current Architecture Issues

**Critical Memory Risk:**
- Entire files loaded into RAM before S3 upload (`s3/mod.rs:68`)
- A 4GB video requires 4GB+ RAM → OOM crash risk on 4GB system
- No multipart/streaming upload support with current `rust-s3` implementation

## Objectives

1. **Eliminate memory constraints** - Stream files to S3 without loading into memory
2. **Enable unlimited video lengths** - Support archiving videos of any size
3. **Proper server-side copy** - Implement S3 copy operation without download+re-upload
4. **Better error handling** - Retry individual chunks on failure
5. **Progress tracking** - Monitor upload progress for large files

## Implementation Plan

### 1. Add aws-sdk-s3 Dependency

**File:** `Cargo.toml`

```toml
[dependencies]
# Add alongside existing rust-s3 (keep for backwards compatibility initially)
aws-sdk-s3 = "1.68"
aws-config = "1.5"
```

**Weight concern:** aws-sdk-s3 has ~150+ dependencies vs rust-s3's ~50. Acceptable trade-off for:
- Official AWS SDK with full feature support
- Works with MinIO and Cloudflare R2 (S3-compatible)
- Multipart upload support
- Server-side copy support

### 2. Implement Streaming Upload Module

**New file:** `src/s3/multipart.rs`

Key features:
- **5MB chunks** - Standard S3 multipart chunk size
- **Fast path** - Files <5MB use simple `put_object`
- **Concurrent upload** - Upload chunks concurrently (configurable)
- **Retry logic** - Retry failed chunks individually
- **Progress tracking** - Log upload progress

```rust
pub struct StreamingUploader {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl StreamingUploader {
    pub async fn upload_file(
        &self,
        key: &str,
        file_path: &Path,
        content_type: &str,
    ) -> Result<()> {
        let file_size = tokio::fs::metadata(file_path).await?.len();

        // Small files: use simple put_object
        if file_size < CHUNK_SIZE {
            return self.upload_small_file(key, file_path, content_type).await;
        }

        // Large files: use multipart upload
        // ... (implementation details in planning document)
    }
}
```

### 3. Update S3Client

**File:** `src/s3/mod.rs`

Add streaming uploader alongside existing rust-s3 client:

```rust
pub struct S3Client {
    // Keep existing rust-s3 bucket for backwards compat
    bucket: Box<Bucket>,
    // Add new AWS SDK client
    streaming_uploader: StreamingUploader,
}

impl S3Client {
    pub async fn upload_file(&self, ...) -> Result<()> {
        // Replace memory-loading with streaming upload
        self.streaming_uploader
            .upload_file(s3_key, local_path, &content_type)
            .await
    }

    pub async fn copy_object(&self, source_key: &str, dest_key: &str) -> Result<()> {
        // Implement proper S3 server-side copy
        self.streaming_uploader.copy_object(source_key, dest_key).await
    }
}
```

### 4. Server-Side Copy Implementation

Use aws-sdk-s3's `CopyObject` operation:

```rust
pub async fn copy_object(&self, source_key: &str, dest_key: &str) -> Result<()> {
    let copy_source = format!("{}/{}", self.bucket, source_key);

    self.client
        .copy_object()
        .bucket(&self.bucket)
        .copy_source(copy_source)
        .key(dest_key)
        .send()
        .await?;

    Ok(())
}
```

Benefits:
- No bandwidth usage (server-side operation)
- No memory usage (no download)
- Fast (S3 internal copy)

### 5. Configuration Options

**Optional config additions:**

```toml
[s3]
multipart_chunk_size_mb = 5  # Default: 5MB
multipart_concurrency = 4     # Concurrent chunk uploads
```

## Testing Strategy

### Unit Tests

```rust
#[tokio::test]
async fn test_multipart_upload_large_file() {
    // Create 20MB test file
    // Verify chunks uploaded correctly
    // Verify file matches after download
}

#[tokio::test]
async fn test_small_file_fast_path() {
    // Create 1MB test file
    // Verify single put_object used
}

#[tokio::test]
async fn test_server_side_copy() {
    // Upload file once
    // Copy to new key
    // Verify both exist and match
}
```

### Integration Tests

- Test with MinIO (local S3-compatible)
- Test with Cloudflare R2 (if available)
- Test with AWS S3
- Verify deduplication still works

## Migration Path

1. **Phase 1:** Add aws-sdk-s3, implement streaming upload
2. **Phase 2:** Test thoroughly with small and large files
3. **Phase 3:** Deploy to production
4. **Phase 4:** Remove rust-s3 dependency (optional, can keep for simple operations)

## Compatibility

### S3-Compatible Services

**Tested/Supported:**
- ✅ AWS S3
- ✅ MinIO (local development)
- ✅ Cloudflare R2 (requires custom endpoint)

Both multipart upload and server-side copy are part of the S3 API spec.

## Performance Impact

| Operation | Current | With Streaming |
|-----------|---------|----------------|
| **100MB video** | 100MB RAM | ~10MB RAM (2x5MB buffers) |
| **4GB video** | 4GB RAM (OOM) | ~10MB RAM |
| **Deduplication copy** | Download + Re-upload | Server-side copy (instant) |
| **Upload speed** | Same | Same or faster (concurrent chunks) |

## Risks & Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| Dependency size increase | Build time | Acceptable for safety gain |
| Code complexity | Maintenance | Well-documented, tested |
| S3 compatibility | Service-specific bugs | Test with MinIO, R2, AWS |
| Multipart overhead | Small files slower | Fast path for <5MB files |

## Success Criteria

- [ ] Upload 10GB video without OOM
- [ ] Memory usage stays under 100MB during upload
- [ ] Server-side copy works for deduplication
- [ ] Works with MinIO and R2
- [ ] All existing tests pass
- [ ] No performance regression for small files

## Timeline Estimate

- **Implementation:** 2-3 days
- **Testing:** 1-2 days
- **Documentation:** 0.5 day
- **Total:** 4-6 days

## Related

- **Prerequisite:** Stage 1 (duration limits, timeout) ✅ Complete
- **Depends on:** None
- **Blocks:** Unlimited video length archiving

## References

- [AWS SDK for Rust - S3 Examples](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/rust_s3_code_examples.html)
- [aws-sdk-s3 Client Documentation](https://docs.rs/aws-sdk-s3/latest/aws_sdk_s3/client/struct.Client.html)
- [S3 Multipart Upload API](https://docs.aws.amazon.com/AmazonS3/latest/API/API_CreateMultipartUpload.html)
