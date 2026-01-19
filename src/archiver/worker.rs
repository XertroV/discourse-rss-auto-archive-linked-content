use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

use super::rate_limiter::DomainRateLimiter;
use super::screenshot::ScreenshotService;
use crate::config::Config;
use crate::db::{
    find_artifact_by_perceptual_hash, get_archive, get_failed_archives_for_retry, get_link,
    get_pending_archives, insert_artifact, insert_artifact_with_hash, reset_archive_for_retry,
    reset_stuck_processing_archives, reset_todays_failed_archives, set_archive_complete,
    set_archive_failed, set_archive_ipfs_cid, set_archive_nsfw, set_archive_processing,
    set_archive_skipped, update_link_final_url, update_link_last_archived, ArtifactKind, Database,
};
use crate::dedup;
use crate::handlers::HANDLERS;
use crate::ipfs::IpfsClient;
use crate::s3::S3Client;

const MAX_RETRIES: i32 = 3;

/// Archive worker pool.
pub struct ArchiveWorker {
    config: Config,
    db: Database,
    s3: S3Client,
    ipfs: IpfsClient,
    screenshot: Arc<ScreenshotService>,
    semaphore: Arc<Semaphore>,
    domain_limiter: Arc<DomainRateLimiter>,
}

impl ArchiveWorker {
    /// Create a new archive worker.
    pub fn new(config: Config, db: Database, s3: S3Client, ipfs: IpfsClient) -> Self {
        let semaphore = Arc::new(Semaphore::new(config.worker_concurrency));
        let domain_limiter = Arc::new(DomainRateLimiter::new(config.per_domain_concurrency));
        let screenshot_config = config.screenshot_config();
        let pdf_config = config.pdf_config();
        let screenshot = Arc::new(ScreenshotService::with_pdf_config(
            screenshot_config,
            pdf_config,
        ));

        if screenshot.is_enabled() {
            info!("Screenshot capture enabled");
        }
        if screenshot.is_pdf_enabled() {
            info!("PDF generation enabled");
        }

        Self {
            config,
            db,
            s3,
            ipfs,
            screenshot,
            semaphore,
            domain_limiter,
        }
    }

    /// Recover from a previous unclean shutdown.
    ///
    /// This resets archives that were stuck in "processing" state (interrupted
    /// mid-processing) and gives today's failed archives another chance.
    pub async fn recover_on_startup(&self) -> Result<()> {
        // Reset archives that were mid-processing when we shut down
        let stuck = reset_stuck_processing_archives(self.db.pool()).await?;
        if stuck > 0 {
            info!(count = stuck, "Reset stuck processing archives to pending");
        }

        // Give today's failed archives another chance
        let failed = reset_todays_failed_archives(self.db.pool(), MAX_RETRIES).await?;
        if failed > 0 {
            info!(
                count = failed,
                "Reset today's failed archives for retry on startup"
            );
        }

        Ok(())
    }

    /// Run the worker loop.
    pub async fn run(&self) {
        loop {
            // Process failed archives first (for retry)
            if let Err(e) = self.process_failed().await {
                error!("Error processing failed archives: {e:#}");
            }

            // Process pending archives
            match self.process_pending().await {
                Ok(count) => {
                    if count > 0 {
                        info!(count, "Processed pending archives");
                    }
                }
                Err(e) => {
                    error!("Error processing pending archives: {e:#}");
                }
            }

            // Wait before next iteration
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    }

    async fn process_pending(&self) -> Result<usize> {
        let pending = get_pending_archives(
            self.db.pool(),
            i64::try_from(self.config.worker_concurrency).unwrap_or(4),
        )
        .await?;

        let mut handles = Vec::new();

        for archive in pending {
            // Get the link to determine domain for rate limiting
            let link = if let Some(l) = get_link(self.db.pool(), archive.link_id).await? {
                l
            } else {
                warn!(archive_id = archive.id, "Link not found, skipping");
                continue;
            };

            let domain = link.domain.clone();
            let permit = self.semaphore.clone().acquire_owned().await?;
            let db = self.db.clone();
            let s3 = self.s3.clone();
            let ipfs = self.ipfs.clone();
            let screenshot = Arc::clone(&self.screenshot);
            let config = self.config.clone();
            let domain_limiter = Arc::clone(&self.domain_limiter);

            let handle = tokio::spawn(async move {
                let _global_permit = permit;
                // Acquire domain-specific permit
                let _domain_permit = domain_limiter.acquire(&domain).await;
                debug!(archive_id = archive.id, domain = %domain, "Acquired domain permit");
                process_archive(
                    &db,
                    &s3,
                    &ipfs,
                    &screenshot,
                    &config,
                    archive.id,
                    archive.link_id,
                )
                .await;
            });

            handles.push(handle);
        }

        let count = handles.len();

        // Wait for all to complete
        for handle in handles {
            if let Err(e) = handle.await {
                error!("Worker task panicked: {e}");
            }
        }

        Ok(count)
    }

    async fn process_failed(&self) -> Result<()> {
        // The query already filters by retry_count and next_retry_at,
        // so archives returned here are ready for retry
        let failed = get_failed_archives_for_retry(self.db.pool(), 10, MAX_RETRIES).await?;

        for archive in failed {
            if archive.retry_count >= MAX_RETRIES {
                // Mark as permanently skipped (shouldn't happen due to query filter, but be safe)
                set_archive_skipped(self.db.pool(), archive.id).await?;
                warn!(
                    archive_id = archive.id,
                    "Archive marked as skipped after max retries"
                );
            } else {
                // Reset to pending for retry
                reset_archive_for_retry(self.db.pool(), archive.id).await?;
                debug!(
                    archive_id = archive.id,
                    retry_count = archive.retry_count,
                    "Archive reset for retry (attempt {})",
                    archive.retry_count + 1
                );
            }
        }

        Ok(())
    }
}

async fn process_archive(
    db: &Database,
    s3: &S3Client,
    ipfs: &IpfsClient,
    screenshot: &ScreenshotService,
    config: &Config,
    archive_id: i64,
    link_id: i64,
) {
    // Fetch link to get domain for logging
    let domain = match get_link(db.pool(), link_id).await {
        Ok(Some(link)) => link.domain,
        Ok(None) => "unknown".to_string(),
        Err(_) => "unknown".to_string(),
    };

    if let Err(e) =
        process_archive_inner(db, s3, ipfs, screenshot, config, archive_id, link_id).await
    {
        error!(archive_id, domain = %domain, "Archive failed: {e:#}");
        if let Err(e2) = set_archive_failed(db.pool(), archive_id, &format!("{e:#}")).await {
            error!(archive_id, domain = %domain, "Failed to mark archive as failed: {e2:#}");
        }
    }
}

/// Create view.html with archive banner injected.
/// Returns the file size of the created view.html.
async fn create_view_html(
    db: &Database,
    archive_id: i64,
    link_id: i64,
    raw_html_path: &Path,
    work_dir: &Path,
    s3: &S3Client,
    s3_prefix: &str,
) -> Result<Option<i64>> {
    // Get archive and link info
    let archive = get_archive(db.pool(), archive_id)
        .await?
        .context("Archive not found")?;
    let link = get_link(db.pool(), link_id)
        .await?
        .context("Link not found")?;

    // Read raw HTML
    let raw_html = tokio::fs::read_to_string(raw_html_path)
        .await
        .context("Failed to read raw.html")?;

    // Inject archive banner
    let banner_html = crate::web::templates::render_archive_banner(&archive, &link);
    let view_html = inject_archive_banner(&raw_html, &banner_html);

    // Save view.html locally
    let view_html_path = work_dir.join("view.html");
    tokio::fs::write(&view_html_path, &view_html)
        .await
        .context("Failed to write view.html")?;

    // Get file size
    let size_bytes = Some(view_html.len() as i64);

    // Upload view.html to S3
    let view_key = format!("{s3_prefix}media/view.html");
    s3.upload_file(&view_html_path, &view_key)
        .await
        .context("Failed to upload view.html")?;

    Ok(size_bytes)
}

/// Inject archive banner into HTML content.
fn inject_archive_banner(html: &str, banner: &str) -> String {
    // Try to find <body> tag
    if let Some(body_pos) = html.find("<body") {
        // Find the end of the opening body tag
        let body_end = if let Some(close_pos) = html[body_pos..].find('>') {
            body_pos + close_pos + 1
        } else {
            body_pos
        };

        // Insert banner after opening body tag
        format!("{}{}{}", &html[..body_end], banner, &html[body_end..])
    } else {
        // No body tag, inject at start of document
        // Try to find </head> or <html> to inject after
        if let Some(head_end_pos) = html.find("</head>") {
            format!(
                "{}{}{}",
                &html[..head_end_pos + 7],
                banner,
                &html[head_end_pos + 7..]
            )
        } else if let Some(html_pos) = html.find("<html") {
            // Find end of opening html tag
            let html_end = if let Some(close_pos) = html[html_pos..].find('>') {
                html_pos + close_pos + 1
            } else {
                html_pos
            };
            format!("{}{}{}", &html[..html_end], banner, &html[html_end..])
        } else {
            // Just prepend to the whole document
            format!("{banner}{html}")
        }
    }
}

async fn process_archive_inner(
    db: &Database,
    s3: &S3Client,
    ipfs: &IpfsClient,
    screenshot: &ScreenshotService,
    config: &Config,
    archive_id: i64,
    link_id: i64,
) -> Result<()> {
    // Mark as processing
    set_archive_processing(db.pool(), archive_id).await?;

    // Get the link
    let link = get_link(db.pool(), link_id)
        .await?
        .context("Link not found")?;

    debug!(archive_id, url = %link.normalized_url, "Processing archive");

    // Find handler
    let handler = HANDLERS
        .find_handler(&link.normalized_url)
        .context("No handler found for URL")?;

    // Create work directory
    let work_dir = config.work_dir.join(format!("archive_{archive_id}"));
    tokio::fs::create_dir_all(&work_dir)
        .await
        .context("Failed to create work directory")?;

    // Run the archive
    // Only use cookies if the file actually exists; this keeps the service working
    // even if COOKIES_FILE_PATH is set but cookies haven't been exported yet.
    let cookies_file = match config.cookies_file_path.as_deref() {
        Some(path) if path.exists() => Some(path),
        Some(path) => {
            debug!(cookies_path = %path.display(), "Cookies file configured but not found; proceeding without cookies");
            None
        }
        None => None,
    };
    let cookies = super::CookieOptions {
        cookies_file,
        browser_profile: config.yt_dlp_cookies_from_browser.as_deref(),
    };
    let result = handler
        .archive(&link.normalized_url, &work_dir, &cookies)
        .await
        .context("Handler archive failed")?;

    // Upload artifacts to S3
    let s3_prefix = format!("{}{}/", config.s3_prefix, link_id);
    let mut primary_key = None;
    let mut thumb_key = None;
    let mut primary_local_path = None;

    if let Some(ref primary) = result.primary_file {
        let local_path = work_dir.join(primary);
        if local_path.exists() {
            let key = format!("{s3_prefix}media/{primary}");
            let metadata = tokio::fs::metadata(&local_path).await.ok();
            let size_bytes = metadata.map(|m| m.len() as i64);
            let content_type = mime_guess::from_path(&local_path)
                .first_or_octet_stream()
                .to_string();

            // Determine artifact kind based on content type
            let kind = if result.content_type == "video" {
                ArtifactKind::Video
            } else if result.content_type == "image" || result.content_type == "gallery" {
                ArtifactKind::Image
            } else if primary == "raw.html" {
                ArtifactKind::RawHtml
            } else {
                ArtifactKind::Metadata
            };

            // Check for duplicates if dedup is enabled and this is an image/video
            let is_media = matches!(kind, ArtifactKind::Image | ArtifactKind::Video);
            let (perceptual_hash, duplicate_of) = if config.dedup_enabled && is_media {
                match check_for_duplicate(db, &local_path, config.dedup_similarity_threshold).await
                {
                    Ok(Some(duplicate_artifact)) => {
                        debug!(
                            archive_id,
                            duplicate_of = duplicate_artifact.id,
                            "Found duplicate artifact, skipping upload"
                        );
                        // Use the existing artifact's S3 key
                        primary_key = Some(duplicate_artifact.s3_key.clone());
                        (
                            duplicate_artifact.perceptual_hash.clone(),
                            Some(duplicate_artifact.id),
                        )
                    }
                    Ok(None) => {
                        // Not a duplicate, compute hash for storage
                        match compute_perceptual_hash(&local_path).await {
                            Ok(hash) => (Some(hash), None),
                            Err(e) => {
                                debug!(archive_id, error = %e, "Failed to compute perceptual hash");
                                (None, None)
                            }
                        }
                    }
                    Err(e) => {
                        debug!(archive_id, error = %e, "Error checking for duplicates");
                        (None, None)
                    }
                }
            } else {
                (None, None)
            };

            // Upload to S3 only if not a duplicate
            if duplicate_of.is_none() {
                s3.upload_file(&local_path, &key).await?;
                primary_key = Some(key.clone());
                primary_local_path = Some(local_path.clone());
            }

            // Insert artifact record with hash info
            if let Err(e) = insert_artifact_with_hash(
                db.pool(),
                archive_id,
                kind.as_str(),
                primary_key.as_deref().unwrap_or(&key),
                Some(&content_type),
                size_bytes,
                None,
                perceptual_hash.as_deref(),
                duplicate_of,
            )
            .await
            {
                warn!(archive_id, error = %e, "Failed to insert primary artifact record");
            }

            // Store primary_local_path if we uploaded (not a duplicate)
            if duplicate_of.is_none() {
                primary_local_path = Some(local_path.clone());
            }

            // If this is an HTML file (raw.html), create view.html with archive banner
            if primary == "raw.html" {
                match create_view_html(
                    db,
                    archive_id,
                    link_id,
                    &local_path,
                    &work_dir,
                    s3,
                    &s3_prefix,
                )
                .await
                {
                    Ok(view_size) => {
                        debug!(archive_id, "Created view.html with archive banner");
                        // Insert view.html artifact record
                        let view_key = format!("{s3_prefix}media/view.html");
                        if let Err(e) = insert_artifact(
                            db.pool(),
                            archive_id,
                            ArtifactKind::RawHtml.as_str(),
                            &view_key,
                            Some("text/html"),
                            view_size,
                            None,
                        )
                        .await
                        {
                            warn!(archive_id, error = %e, "Failed to insert view.html artifact record");
                        }
                    }
                    Err(e) => {
                        warn!(archive_id, error = %e, "Failed to create view.html, continuing without banner");
                    }
                }
            }
        }
    }

    if let Some(ref thumb) = result.thumbnail {
        let local_path = work_dir.join(thumb);
        if local_path.exists() {
            let key = format!("{s3_prefix}thumb/{thumb}");
            let metadata = tokio::fs::metadata(&local_path).await.ok();
            let size_bytes = metadata.map(|m| m.len() as i64);
            let content_type = mime_guess::from_path(&local_path)
                .first_or_octet_stream()
                .to_string();

            // Check for duplicates if dedup is enabled
            let (perceptual_hash, duplicate_of) = if config.dedup_enabled {
                match check_for_duplicate(db, &local_path, config.dedup_similarity_threshold).await
                {
                    Ok(Some(duplicate_artifact)) => {
                        debug!(
                            archive_id,
                            duplicate_of = duplicate_artifact.id,
                            "Found duplicate thumbnail, skipping upload"
                        );
                        // Use existing thumbnail's key
                        thumb_key = Some(duplicate_artifact.s3_key.clone());
                        (
                            duplicate_artifact.perceptual_hash.clone(),
                            Some(duplicate_artifact.id),
                        )
                    }
                    Ok(None) => match compute_perceptual_hash(&local_path).await {
                        Ok(hash) => (Some(hash), None),
                        Err(e) => {
                            debug!(archive_id, error = %e, "Failed to compute thumbnail hash");
                            (None, None)
                        }
                    },
                    Err(e) => {
                        debug!(archive_id, error = %e, "Error checking thumbnail for duplicates");
                        (None, None)
                    }
                }
            } else {
                (None, None)
            };

            // Upload only if not a duplicate
            if duplicate_of.is_none() {
                s3.upload_file(&local_path, &key).await?;
                thumb_key = Some(key.clone());
            }

            // Insert thumbnail artifact record with hash info
            if let Err(e) = insert_artifact_with_hash(
                db.pool(),
                archive_id,
                ArtifactKind::Thumb.as_str(),
                thumb_key.as_deref().unwrap_or(&key),
                Some(&content_type),
                size_bytes,
                None,
                perceptual_hash.as_deref(),
                duplicate_of,
            )
            .await
            {
                warn!(archive_id, error = %e, "Failed to insert thumbnail artifact record");
            }
        }
    }

    // Upload metadata JSON if present
    if let Some(ref metadata) = result.metadata_json {
        let key = format!("{s3_prefix}meta.json");
        let size_bytes = Some(metadata.len() as i64);
        s3.upload_bytes(metadata.as_bytes(), &key, "application/json")
            .await?;

        // Insert metadata artifact record
        if let Err(e) = insert_artifact(
            db.pool(),
            archive_id,
            ArtifactKind::Metadata.as_str(),
            &key,
            Some("application/json"),
            size_bytes,
            None,
        )
        .await
        {
            warn!(archive_id, error = %e, "Failed to insert metadata artifact record");
        }
    }

    // Upload extra files (images, etc.) from handlers
    for extra_file in &result.extra_files {
        let local_path = work_dir.join(extra_file);
        if local_path.exists() {
            let key = format!("{s3_prefix}media/{extra_file}");
            let metadata = tokio::fs::metadata(&local_path).await.ok();
            let size_bytes = metadata.map(|m| m.len() as i64);
            let content_type = mime_guess::from_path(&local_path)
                .first_or_octet_stream()
                .to_string();

            if let Err(e) = s3.upload_file(&local_path, &key).await {
                warn!(archive_id, file = %extra_file, error = %e, "Failed to upload extra file");
                continue;
            }

            debug!(archive_id, file = %extra_file, "Uploaded extra file");

            // Determine artifact kind based on content type
            let kind = if content_type.starts_with("image/") {
                ArtifactKind::Image
            } else if content_type.starts_with("video/") {
                ArtifactKind::Video
            } else if content_type.contains("subtitle")
                || extra_file.ends_with(".vtt")
                || extra_file.ends_with(".srt")
            {
                ArtifactKind::Subtitles
            } else {
                ArtifactKind::Metadata
            };

            // Insert extra file artifact record
            if let Err(e) = insert_artifact(
                db.pool(),
                archive_id,
                kind.as_str(),
                &key,
                Some(&content_type),
                size_bytes,
                None,
            )
            .await
            {
                warn!(archive_id, file = %extra_file, error = %e, "Failed to insert extra file artifact record");
            }
        } else {
            warn!(archive_id, file = %extra_file, "Extra file not found in work directory");
        }
    }

    // Capture screenshot if enabled (non-fatal if it fails)
    if screenshot.is_enabled() {
        match screenshot.capture(&link.normalized_url).await {
            Ok(png_data) => {
                let screenshot_key = format!("{s3_prefix}render/screenshot.png");
                let size_bytes = Some(png_data.len() as i64);
                if let Err(e) = s3
                    .upload_bytes(&png_data, &screenshot_key, "image/png")
                    .await
                {
                    warn!(archive_id, error = %e, "Failed to upload screenshot");
                } else {
                    debug!(archive_id, key = %screenshot_key, "Screenshot uploaded");
                    // Insert screenshot artifact record
                    if let Err(e) = insert_artifact(
                        db.pool(),
                        archive_id,
                        ArtifactKind::Screenshot.as_str(),
                        &screenshot_key,
                        Some("image/png"),
                        size_bytes,
                        None,
                    )
                    .await
                    {
                        warn!(archive_id, error = %e, "Failed to insert screenshot artifact record");
                    }
                }
            }
            Err(e) => {
                warn!(archive_id, error = %e, "Failed to capture screenshot");
            }
        }
    }

    // Generate PDF if enabled (non-fatal if it fails)
    if screenshot.is_pdf_enabled() {
        match screenshot.capture_pdf(&link.normalized_url).await {
            Ok(pdf_data) => {
                let pdf_key = format!("{s3_prefix}render/page.pdf");
                let size_bytes = Some(pdf_data.len() as i64);
                if let Err(e) = s3
                    .upload_bytes(&pdf_data, &pdf_key, "application/pdf")
                    .await
                {
                    warn!(archive_id, error = %e, "Failed to upload PDF");
                } else {
                    debug!(archive_id, key = %pdf_key, "PDF uploaded");
                    // Insert PDF artifact record
                    if let Err(e) = insert_artifact(
                        db.pool(),
                        archive_id,
                        ArtifactKind::Pdf.as_str(),
                        &pdf_key,
                        Some("application/pdf"),
                        size_bytes,
                        None,
                    )
                    .await
                    {
                        warn!(archive_id, error = %e, "Failed to insert PDF artifact record");
                    }
                }
            }
            Err(e) => {
                warn!(archive_id, error = %e, "Failed to generate PDF");
            }
        }
    }

    // Pin to IPFS if enabled
    let ipfs_cid = if ipfs.is_enabled() {
        // Try to pin the primary file to IPFS
        if let Some(ref local_path) = primary_local_path {
            match ipfs.pin_file(local_path).await {
                Ok(cid) => {
                    info!(archive_id, cid = %cid, "Pinned to IPFS");
                    Some(cid)
                }
                Err(e) => {
                    warn!(archive_id, error = %e, "Failed to pin to IPFS, continuing without IPFS");
                    None
                }
            }
        } else {
            // Try to pin the work directory if no primary file
            match ipfs.pin_directory(&work_dir).await {
                Ok(cid) => {
                    info!(archive_id, cid = %cid, "Pinned directory to IPFS");
                    Some(cid)
                }
                Err(e) => {
                    warn!(archive_id, error = %e, "Failed to pin directory to IPFS");
                    None
                }
            }
        }
    } else {
        None
    };

    // Mark as complete
    set_archive_complete(
        db.pool(),
        archive_id,
        result.title.as_deref(),
        result.author.as_deref(),
        result.text.as_deref(),
        Some(&result.content_type),
        primary_key.as_deref(),
        thumb_key.as_deref(),
    )
    .await?;

    // Store NSFW status if detected
    if let Some(is_nsfw) = result.is_nsfw {
        set_archive_nsfw(
            db.pool(),
            archive_id,
            is_nsfw,
            result.nsfw_source.as_deref(),
        )
        .await?;
        if is_nsfw {
            info!(archive_id, nsfw_source = ?result.nsfw_source, "Archive marked as NSFW");
        }
    }

    // Store IPFS CID if we have one
    if let Some(ref cid) = ipfs_cid {
        set_archive_ipfs_cid(db.pool(), archive_id, cid).await?;
    }

    // Update link final URL if different from normalized URL
    if let Some(ref final_url) = result.final_url {
        update_link_final_url(db.pool(), link_id, final_url).await?;
        debug!(
            link_id,
            normalized_url = %link.normalized_url,
            final_url = %final_url,
            "Updated link with final URL after redirect"
        );
    }

    // Update link last archived timestamp
    update_link_last_archived(db.pool(), link_id).await?;

    // Clean up work directory
    if let Err(e) = tokio::fs::remove_dir_all(&work_dir).await {
        warn!(archive_id, "Failed to clean up work directory: {e}");
    }

    info!(archive_id, url = %link.normalized_url, ipfs_cid = ?ipfs_cid, "Archive complete");

    Ok(())
}

/// Create a work directory for an archive job.
#[allow(dead_code)]
fn create_work_dir(base: &Path, archive_id: i64) -> PathBuf {
    base.join(format!("archive_{archive_id}"))
}

/// Compute perceptual hash for an image or video file.
async fn compute_perceptual_hash(path: &Path) -> Result<String> {
    let data = tokio::fs::read(path)
        .await
        .context("Failed to read file for hashing")?;
    dedup::compute_image_hash(&data).context("Failed to compute perceptual hash")
}

/// Check if a file is a duplicate of an existing artifact.
///
/// Returns the original artifact if a duplicate is found, or None if unique.
async fn check_for_duplicate(
    db: &Database,
    path: &Path,
    _threshold: u32,
) -> Result<Option<crate::db::ArchiveArtifact>> {
    // Compute hash for this file
    let hash = compute_perceptual_hash(path).await?;

    // Query database for all artifacts with perceptual hashes
    // We check similarity against all existing hashes
    let pool = db.pool();

    // Try exact match first (fast path)
    if let Some(artifact) = find_artifact_by_perceptual_hash(pool, &hash).await? {
        return Ok(Some(artifact));
    }

    // For now, we only do exact hash matching
    // Future enhancement: query all hashes and compare similarity
    // This would require loading all hashes which could be slow for large databases
    // A better approach might be to use a more sophisticated indexing scheme

    Ok(None)
}
