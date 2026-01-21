use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use scraper::Html;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};
use url::Url;

use super::monolith::create_complete_html;
use super::rate_limiter::DomainRateLimiter;
use super::screenshot::ScreenshotService;
use crate::config::Config;
use crate::db::{
    create_archive_job, find_artifact_by_perceptual_hash, find_video_file, get_archive,
    get_failed_archives_for_retry, get_link, get_or_create_video_file, get_pending_archives,
    has_artifact_kind, insert_artifact, insert_artifact_with_hash, insert_artifact_with_metadata,
    insert_artifact_with_video_file, is_domain_excluded, mark_og_extraction_attempted,
    reset_archive_for_retry, reset_stuck_processing_archives, reset_todays_failed_archives,
    set_archive_complete, set_archive_failed, set_archive_ipfs_cid, set_archive_nsfw,
    set_archive_processing, set_archive_skipped, set_job_completed, set_job_failed,
    set_job_running, set_job_skipped, update_archive_og_metadata, update_link_final_url,
    update_link_last_archived, update_video_file_metadata_key, ArchiveJobType, ArtifactKind,
    Database, VideoFile,
};
use crate::dedup;
use crate::handlers::youtube::extract_video_id;
use crate::handlers::HANDLERS;
use crate::ipfs::IpfsClient;
use crate::og_extractor;
use crate::s3::S3Client;

const MAX_RETRIES: i32 = 3;

/// Check if domain is in comments-supported platforms
pub fn is_comments_supported_platform(domain: &str, config: &Config) -> bool {
    // Platform domain mapping
    let platform_domains = [
        ("youtube", vec!["youtube.com", "youtu.be"]),
        (
            "tiktok",
            vec!["tiktok.com", "vm.tiktok.com", "m.tiktok.com"],
        ),
        ("twitter", vec!["x.com", "twitter.com"]),
        ("instagram", vec!["instagram.com", "instagr.am"]),
    ];

    // Check if domain matches any supported platform
    for (platform, domains) in &platform_domains {
        if config.comments_platforms.contains(&platform.to_string()) {
            for supported_domain in domains {
                if domain.contains(supported_domain) {
                    return true;
                }
            }
        }
    }
    false
}

/// Extract platform name from domain for metadata
pub fn extract_platform_name(domain: &str) -> &'static str {
    if domain.contains("youtube.com") || domain.contains("youtu.be") {
        "youtube"
    } else if domain.contains("tiktok.com") {
        "tiktok"
    } else if domain.contains("x.com") || domain.contains("twitter.com") {
        "twitter"
    } else if domain.contains("instagram.com") {
        "instagram"
    } else {
        "unknown"
    }
}

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
        let mhtml_config = config.mhtml_config();
        let monolith_config = config.monolith_config();
        let screenshot = Arc::new(ScreenshotService::with_all_configs(
            screenshot_config,
            pdf_config,
            mhtml_config,
        ));

        if screenshot.is_enabled() {
            info!("Screenshot capture enabled");
        }
        if screenshot.is_pdf_enabled() {
            info!("PDF generation enabled");
        }
        if screenshot.is_mhtml_enabled() {
            info!("MHTML archive enabled");
        }
        if monolith_config.enabled {
            info!("Monolith self-contained HTML enabled");
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

    /// Fetch missing artifacts for a completed archive without re-archiving.
    /// This is useful for getting supplementary artifacts like subtitles and transcripts
    /// for video content that was archived without them.
    pub async fn fetch_missing_artifacts(&self, archive_id: i64) -> Result<()> {
        info!(archive_id, "Fetching missing artifacts for archive");

        // Get the archive
        let archive = get_archive(self.db.pool(), archive_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Archive {} not found", archive_id))?;

        // Only process complete archives
        if archive.status != "complete" {
            anyhow::bail!(
                "Archive {} is not complete (status: {})",
                archive_id,
                archive.status
            );
        }

        // Only process video content for now
        if archive.content_type.as_deref() != Some("video") {
            anyhow::bail!("Archive {} is not video content", archive_id);
        }

        // Get the link
        let link = get_link(self.db.pool(), archive.link_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Link {} not found", archive.link_id))?;

        // Check what artifacts are missing
        let needs_subtitles =
            !has_artifact_kind(self.db.pool(), archive_id, ArtifactKind::Subtitles.as_str())
                .await
                .unwrap_or(true);

        let needs_transcript = !has_artifact_kind(
            self.db.pool(),
            archive_id,
            ArtifactKind::Transcript.as_str(),
        )
        .await
        .unwrap_or(true);

        let needs_comments =
            !has_artifact_kind(self.db.pool(), archive_id, ArtifactKind::Comments.as_str())
                .await
                .unwrap_or(true);

        if !needs_subtitles && !needs_transcript && !needs_comments {
            info!(archive_id, "No missing artifacts to fetch");
            return Ok(());
        }

        info!(
            archive_id,
            needs_subtitles,
            needs_transcript,
            needs_comments,
            "Downloading missing supplementary artifacts"
        );

        // Create a temporary work directory
        let work_dir = std::env::temp_dir().join(format!("archive-{}", archive_id));
        std::fs::create_dir_all(&work_dir).context("Failed to create temp directory")?;

        // Set up cookies
        let cookies_file = match self.config.cookies_file_path.as_deref() {
            Some(path) if path.exists() => Some(path),
            _ => None,
        };
        let cookies = super::CookieOptions {
            cookies_file,
            browser_profile: self.config.yt_dlp_cookies_from_browser.as_deref(),
            screenshot_service: Some(&*self.screenshot),
        };

        // Download supplementary artifacts (subtitles, transcripts, comments)
        let should_download_comments = self.config.comments_enabled
            && is_comments_supported_platform(&link.domain, &self.config)
            && needs_comments;

        let result = super::ytdlp::download_supplementary_artifacts(
            &link.normalized_url,
            &work_dir,
            &cookies,
            &self.config,
            needs_subtitles,
            should_download_comments,
        )
        .await
        .context("Failed to download supplementary artifacts")?;

        // Process subtitle files if any were downloaded
        if !result.extra_files.is_empty() {
            let subtitle_files: Vec<String> = result
                .extra_files
                .iter()
                .filter(|f| f.ends_with(".vtt") || f.ends_with(".srt"))
                .cloned()
                .collect();

            if !subtitle_files.is_empty() {
                // Determine S3 prefix for this archive
                let s3_prefix = format!("archives/{}/", archive_id);

                // Process and upload subtitle files
                process_subtitle_files(
                    &self.db,
                    &self.s3,
                    archive_id,
                    &subtitle_files,
                    &work_dir,
                    &s3_prefix,
                )
                .await;

                info!(
                    archive_id,
                    count = subtitle_files.len(),
                    "Successfully fetched subtitle artifacts"
                );
            }

            // Process comments.json if present
            let comments_file = result
                .extra_files
                .iter()
                .find(|f| f.ends_with("comments.json"));

            if let Some(comments_filename) = comments_file {
                let comments_path = work_dir.join(comments_filename);
                if comments_path.exists() {
                    let s3_prefix = format!("archives/{}/", archive_id);
                    let s3_key = format!("{}comments.json", s3_prefix);

                    // Read comment stats from JSON for metadata
                    let comment_stats = tokio::fs::read_to_string(&comments_path)
                        .await
                        .ok()
                        .and_then(|content| {
                            serde_json::from_str::<serde_json::Value>(&content).ok()
                        })
                        .and_then(|json| json.get("stats").cloned());

                    // Upload to S3
                    match self
                        .s3
                        .upload_file(&comments_path, &s3_key, Some(archive_id))
                        .await
                    {
                        Ok(_) => {
                            let size = comments_path.metadata().ok().map(|m| m.len() as i64);

                            // Prepare metadata JSON
                            let metadata_json = comment_stats.map(|stats| {
                                serde_json::json!({
                                    "stats": stats,
                                    "platform": extract_platform_name(&link.domain)
                                })
                                .to_string()
                            });

                            // Insert artifact with Comments kind
                            if let Err(e) = crate::db::insert_artifact_with_metadata(
                                self.db.pool(),
                                archive_id,
                                ArtifactKind::Comments.as_str(),
                                &s3_key,
                                Some("application/json"),
                                size,
                                None, // sha256
                                metadata_json.as_deref(),
                            )
                            .await
                            {
                                error!(
                                    archive_id,
                                    error = %e,
                                    "Failed to insert comments artifact"
                                );
                            } else {
                                info!(archive_id, "Successfully uploaded comments artifact");
                            }
                        }
                        Err(e) => {
                            error!(
                                archive_id,
                                error = %e,
                                "Failed to upload comments.json to S3"
                            );
                        }
                    }
                }
            }
        } else {
            warn!(
                archive_id,
                "No supplementary artifacts were downloaded (might not be available)"
            );
        }

        // Clean up temp directory
        if let Err(e) = std::fs::remove_dir_all(&work_dir) {
            warn!(
                archive_id,
                error = %e,
                "Failed to clean up temp directory"
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
        let error_msg = format!("{e:#}");
        error!(archive_id, domain = %domain, "Archive failed: {error_msg}");

        // Check if this is a permanent failure (401, 403, 404) that shouldn't be retried
        if is_permanent_failure(&error_msg) {
            warn!(
                archive_id,
                domain = %domain,
                "Permanent failure detected, marking as skipped (no retry)"
            );
            if let Err(e2) = set_archive_skipped(db.pool(), archive_id).await {
                error!(archive_id, domain = %domain, "Failed to mark archive as skipped: {e2:#}");
            }
            // Also store the error message
            if let Err(e2) = set_archive_failed(db.pool(), archive_id, &error_msg).await {
                error!(archive_id, domain = %domain, "Failed to store error message: {e2:#}");
            }
        } else if let Err(e2) = set_archive_failed(db.pool(), archive_id, &error_msg).await {
            error!(archive_id, domain = %domain, "Failed to mark archive as failed: {e2:#}");
        }
    }
}

/// Check if an error indicates a permanent failure that shouldn't be retried.
///
/// Returns true for HTTP 401 (Unauthorized), 403 (Forbidden), and 404 (Not Found)
/// errors, which are unlikely to succeed on retry.
fn is_permanent_failure(error_msg: &str) -> bool {
    let error_lower = error_msg.to_lowercase();

    // Check for specific HTTP status codes
    error_lower.contains("401")
        || error_lower.contains("403")
        || error_lower.contains("404")
        || error_lower.contains("unauthorized")
        || error_lower.contains("forbidden")
        || error_lower.contains("not found")
        // Also check for common permanent error patterns
        || error_lower.contains("private")
        || error_lower.contains("deleted")
        || error_lower.contains("removed")
}

/// Check if a URL should be skipped from archiving due to archive prevention signals.
///
/// Checks:
/// 1. If domain is in excluded_domains list
/// 2. If the URL has X-No-Archive HTTP header or x-no-archive meta tag
/// 3. If allowArchive=1 query parameter is present (bypass signal)
///
/// Returns true if the URL should be skipped (i.e., it has archive prevention signals).
async fn should_skip_due_to_archive_prevention(db: &Database, url: &str) -> Result<bool> {
    // Parse URL to extract domain and query params
    let parsed = Url::parse(url).context("Failed to parse URL")?;
    let domain = parsed
        .domain()
        .map(|d| d.to_lowercase())
        .unwrap_or_default();

    // Check if allowArchive=1 is present (bypass signal)
    let has_allow_archive = parsed
        .query_pairs()
        .any(|(k, v)| k == "allowArchive" && v == "1");

    if has_allow_archive {
        debug!(url = %url, "URL has allowArchive=1, will archive despite any signals");
        return Ok(false);
    }

    // Check if domain is in excluded list
    if is_domain_excluded(db.pool(), &domain).await? {
        warn!(url = %url, domain = %domain, "Domain is in excluded list, skipping archive");
        return Ok(true);
    }

    // Fetch the URL and check for archive prevention headers/meta tags
    // Use a short timeout for this check to avoid blocking the worker
    match fetch_url_for_signals(url).await {
        Ok(signals) => {
            if signals.has_no_archive_header || signals.has_no_archive_meta {
                warn!(
                    url = %url,
                    has_header = signals.has_no_archive_header,
                    has_meta = signals.has_no_archive_meta,
                    "URL has archive prevention signals, skipping archive"
                );
                return Ok(true);
            }
        }
        Err(e) => {
            // Log but don't fail - if we can't fetch to check signals, proceed with archiving
            // (it might be a timeout or network issue, not necessarily a signal)
            debug!(url = %url, error = %e, "Could not fetch URL to check archive prevention signals, proceeding anyway");
        }
    }

    Ok(false)
}

/// Archive prevention signals found on a page.
#[derive(Debug, Default)]
struct ArchivePreventionSignals {
    has_no_archive_header: bool,
    has_no_archive_meta: bool,
}

/// Fetch a URL and check for archive prevention signals.
///
/// Looks for:
/// - X-No-Archive HTTP header
/// - x-no-archive or robots: noarchive meta tags
async fn fetch_url_for_signals(url: &str) -> Result<ArchivePreventionSignals> {
    let client = reqwest::Client::new();

    // Use a short timeout for this probe
    let response = client
        .head(url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .context("Failed to fetch URL for signals check")?;

    let mut signals = ArchivePreventionSignals::default();

    // Check for X-No-Archive header
    if response.headers().get("X-No-Archive").is_some() {
        signals.has_no_archive_header = true;
    }

    // For meta tag check, we need to GET the page (HEAD won't include body)
    // Only do this if we didn't find a header
    if !signals.has_no_archive_header {
        if let Ok(full_response) = client
            .get(url)
            .timeout(Duration::from_secs(10))
            .send()
            .await
        {
            if let Ok(html_text) = full_response.text().await {
                // Parse HTML and check only the <head> section
                let document = Html::parse_document(&html_text);
                if let Ok(selector) = scraper::Selector::parse("head") {
                    if let Some(head) = document.select(&selector).next() {
                        let head_html = head.inner_html().to_lowercase();
                        if head_html.contains("x-no-archive") {
                            signals.has_no_archive_meta = true;
                        }
                    }
                }
            }
        }
    }

    Ok(signals)
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
    let banner_html = crate::web::pages::render_archive_banner(&archive, &link);
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
    s3.upload_file(&view_html_path, &view_key, Some(archive_id))
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

    // Check for archive prevention signals (excluded domains, X-No-Archive header, meta tags)
    if should_skip_due_to_archive_prevention(db, &link.normalized_url).await? {
        info!(archive_id, url = %link.normalized_url, "Skipping archive due to prevention signals");
        set_archive_skipped(db.pool(), archive_id).await?;
        return Ok(());
    }

    // Find handler
    let handler = HANDLERS
        .find_handler(&link.normalized_url)
        .context("No handler found for URL")?;

    // Check for existing video before downloading (supports all platforms)
    let platform = handler.site_id();
    let is_youtube = platform == "youtube";
    let is_twitter = platform == "twitter";
    let video_id = if is_youtube {
        extract_video_id(&link.normalized_url)
    } else {
        None
    };

    let primary_job_type = if is_youtube {
        ArchiveJobType::YtDlp
    } else {
        ArchiveJobType::FetchHtml
    };
    let main_job = start_job(db.pool(), archive_id, primary_job_type).await;

    // Check if video already exists in database (database-backed deduplication)
    let mut existing_video_file: Option<VideoFile> = None;
    if let Some(ref vid) = video_id {
        match find_video_file(db.pool(), platform, vid).await {
            Ok(Some(vf)) => {
                info!(
                    archive_id,
                    video_id = %vid,
                    platform = %platform,
                    s3_key = %vf.s3_key,
                    "Video already exists in database, skipping download"
                );
                existing_video_file = Some(vf);
            }
            Ok(None) => {
                // Not in database, check S3 as fallback for migration from old system
                match check_existing_youtube_video(s3, vid).await {
                    Ok(Some((existing_key, _ext))) => {
                        // Found on S3 but not in database - register it
                        match s3.get_object_metadata(&existing_key).await {
                            Ok((size_bytes, content_type)) if size_bytes > 0 => {
                                info!(
                                    archive_id,
                                    video_id = %vid,
                                    s3_key = %existing_key,
                                    "Found existing video on S3, registering in database"
                                );
                                // Register the existing video in the database
                                match get_or_create_video_file(
                                    db.pool(),
                                    vid,
                                    platform,
                                    &existing_key,
                                    Some(&format!("videos/{vid}.json")),
                                    Some(size_bytes),
                                    Some(&content_type),
                                    None, // duration unknown
                                )
                                .await
                                {
                                    Ok(vf) => existing_video_file = Some(vf),
                                    Err(e) => {
                                        warn!(
                                            archive_id,
                                            video_id = %vid,
                                            error = %e,
                                            "Failed to register existing video in database"
                                        );
                                    }
                                }
                            }
                            Ok((size_bytes, _)) => {
                                warn!(
                                    archive_id,
                                    video_id = ?video_id,
                                    s3_key = %existing_key,
                                    size = size_bytes,
                                    "Existing video on S3 is empty; will re-download"
                                );
                            }
                            Err(e) => {
                                warn!(
                                    archive_id,
                                    video_id = ?video_id,
                                    s3_key = %existing_key,
                                    error = %e,
                                    "Failed to get metadata for existing video; will re-download"
                                );
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        warn!(archive_id, error = %e, "Failed to check S3 for existing video");
                    }
                }
            }
            Err(e) => {
                warn!(archive_id, error = %e, "Failed to check database for existing video, proceeding with download");
            }
        }
    }

    // Create work directory
    let work_dir = config.work_dir.join(format!("archive_{archive_id}"));
    tokio::fs::create_dir_all(&work_dir)
        .await
        .context("Failed to create work directory")?;

    let mut primary_key: Option<String> = None;
    let mut thumb_key: Option<String> = None;
    let mut primary_local_path: Option<PathBuf> = None;

    // If video already exists, fetch metadata without re-downloading
    let (result, _existing_video_file_used) = if let Some(ref vf) = existing_video_file {
        // Fetch metadata using yt-dlp without downloading the video
        let cookies_file = match config.cookies_file_path.as_deref() {
            Some(path) if path.exists() => Some(path),
            _ => None,
        };
        let cookies = super::CookieOptions {
            cookies_file,
            browser_profile: config.yt_dlp_cookies_from_browser.as_deref(),
            screenshot_service: Some(screenshot),
        };

        let mut result =
            match super::ytdlp::fetch_metadata_only(&link.normalized_url, &cookies).await {
                Ok(meta) => {
                    debug!(
                        url = %link.normalized_url,
                        title = ?meta.title,
                        "Fetched metadata for existing video"
                    );
                    meta
                }
                Err(e) => {
                    // If metadata fetch fails, continue with minimal info
                    warn!(
                        url = %link.normalized_url,
                        error = %e,
                        "Failed to fetch metadata for existing video, using minimal info"
                    );
                    use crate::handlers::ArchiveResult;
                    ArchiveResult {
                        content_type: "video".to_string(),
                        ..Default::default()
                    }
                }
            };

        // Set the primary file to the existing video filename
        let filename = vf
            .s3_key
            .rsplit('/')
            .next()
            .unwrap_or(&vf.s3_key)
            .to_string();
        result.primary_file = Some(filename);
        result.video_id = video_id.clone();

        // Reuse the existing S3 object as the primary artifact for this archive
        primary_key = Some(vf.s3_key.clone());

        // Create artifact record with video_file_id reference
        if let Err(e) = insert_artifact_with_video_file(
            db.pool(),
            archive_id,
            ArtifactKind::Video.as_str(),
            &vf.s3_key,
            vf.content_type.as_deref(),
            vf.size_bytes,
            None, // sha256
            vf.id,
        )
        .await
        {
            warn!(archive_id, error = %e, "Failed to insert existing video artifact record");
        } else {
            debug!(
                archive_id,
                video_id = ?video_id,
                video_file_id = vf.id,
                s3_key = %vf.s3_key,
                size = ?vf.size_bytes,
                "Created artifact record referencing existing video file"
            );
        }

        // Save metadata JSON alongside video at videos/<video_id>.json (if we have new metadata)
        if let (Some(ref vid), Some(ref meta_json)) = (&video_id, &result.metadata_json) {
            let metadata_key = format!("videos/{vid}.json");
            match s3
                .upload_bytes(meta_json.as_bytes(), &metadata_key, "application/json")
                .await
            {
                Ok(()) => {
                    debug!(
                        archive_id,
                        video_id = %vid,
                        s3_key = %metadata_key,
                        "Saved yt-dlp metadata JSON alongside video"
                    );
                    // Update the video file's metadata key if not set
                    if vf.metadata_s3_key.is_none() {
                        if let Err(e) =
                            update_video_file_metadata_key(db.pool(), vf.id, &metadata_key).await
                        {
                            warn!(archive_id, error = %e, "Failed to update video file metadata key");
                        }
                    }
                    // Also create artifact record for metadata
                    if let Err(e) = insert_artifact(
                        db.pool(),
                        archive_id,
                        ArtifactKind::Metadata.as_str(),
                        &metadata_key,
                        Some("application/json"),
                        Some(meta_json.len() as i64),
                        None,
                    )
                    .await
                    {
                        warn!(archive_id, error = %e, "Failed to insert metadata artifact record");
                    }
                }
                Err(e) => {
                    warn!(
                        archive_id,
                        video_id = %vid,
                        error = %e,
                        "Failed to save yt-dlp metadata JSON"
                    );
                }
            }
        }

        // Check for missing supplementary artifacts (subtitles, comments) and download them
        let needs_subtitles =
            !has_artifact_kind(db.pool(), archive_id, ArtifactKind::Subtitles.as_str())
                .await
                .unwrap_or(true);
        let needs_transcript =
            !has_artifact_kind(db.pool(), archive_id, ArtifactKind::Transcript.as_str())
                .await
                .unwrap_or(true);
        // Check if we have comments artifact (stored as metadata with kind "comments" or in extra_files)
        let has_comments_artifact = result.extra_files.iter().any(|f| f == "comments.json");

        if needs_subtitles || (needs_transcript && needs_subtitles) {
            info!(
                archive_id,
                needs_subtitles,
                needs_transcript,
                "Downloading missing supplementary artifacts for existing video"
            );

            match super::ytdlp::download_supplementary_artifacts(
                &link.normalized_url,
                &work_dir,
                &cookies,
                config,
                needs_subtitles,
                config.comments_enabled && !has_comments_artifact,
            )
            .await
            {
                Ok(supplementary_result) => {
                    // Update result's extra_files with any new files downloaded
                    for file in supplementary_result.extra_files {
                        if !result.extra_files.contains(&file) {
                            result.extra_files.push(file);
                        }
                    }
                    debug!(
                        archive_id,
                        extra_files = ?result.extra_files,
                        "Merged supplementary artifacts into result"
                    );
                }
                Err(e) => {
                    warn!(
                        archive_id,
                        error = %e,
                        "Failed to download supplementary artifacts, continuing without them"
                    );
                }
            }
        }

        complete_job(db.pool(), main_job, Some("Reused existing video")).await;

        (result, true)
    } else {
        // Run the archive normally
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
            screenshot_service: Some(screenshot),
        };
        let result = match handler
            .archive(&link.normalized_url, &work_dir, &cookies, config)
            .await
        {
            Ok(res) => {
                complete_job(db.pool(), main_job, None).await;
                res
            }
            Err(e) => {
                fail_job(db.pool(), main_job, &format!("{e:#}")).await;
                return Err(e.context("Handler archive failed"));
            }
        };

        (result, false)
    };

    // Upload artifacts to S3
    let s3_prefix = format!("{}{}/", config.s3_prefix, link_id);

    if let Some(ref primary) = result.primary_file {
        let local_path = work_dir.join(primary);
        if local_path.exists() {
            let key = format!("{s3_prefix}media/{primary}");
            let metadata = tokio::fs::metadata(&local_path).await.ok();
            let size_bytes = metadata.map(|m| m.len() as i64);

            // Check for empty files (can cause S3 upload issues)
            if let Some(0) = size_bytes {
                return Err(anyhow::anyhow!(
                    "Primary file {primary} is empty (0 bytes), cannot upload to S3"
                ));
            }

            let content_type = mime_guess::from_path(&local_path)
                .first_or_octet_stream()
                .to_string();

            // Determine artifact kind based on content type
            let kind = if result.content_type == "video" {
                ArtifactKind::Video
            } else if result.content_type == "image" || result.content_type == "gallery" {
                ArtifactKind::Image
            } else if result.content_type == "pdf" {
                ArtifactKind::Pdf
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
                s3.upload_file(&local_path, &key, Some(archive_id)).await?;
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

            // Register video in database and copy to predictable path for deduplication
            if let Some(ref vid) = result.video_id {
                if result.content_type == "video" && duplicate_of.is_none() {
                    if let Some(ref uploaded_key) = primary_key {
                        // Copy video to predictable path
                        let predictable_key =
                            match copy_video_to_predictable_path(s3, uploaded_key, vid).await {
                                Ok(key) => {
                                    debug!(
                                        archive_id,
                                        video_id = %vid,
                                        predictable_key = %key,
                                        "Copied video to predictable S3 path"
                                    );
                                    Some(key)
                                }
                                Err(e) => {
                                    warn!(
                                        archive_id,
                                        video_id = %vid,
                                        error = %e,
                                        "Failed to copy video to predictable path"
                                    );
                                    None
                                }
                            };

                        // Save metadata JSON alongside video at videos/<video_id>.json
                        let metadata_key = if let Some(ref meta_json) = result.metadata_json {
                            let meta_key = format!("videos/{vid}.json");
                            match s3
                                .upload_bytes(meta_json.as_bytes(), &meta_key, "application/json")
                                .await
                            {
                                Ok(()) => {
                                    debug!(
                                        archive_id,
                                        video_id = %vid,
                                        s3_key = %meta_key,
                                        "Saved yt-dlp metadata JSON alongside video"
                                    );
                                    Some(meta_key)
                                }
                                Err(e) => {
                                    warn!(
                                        archive_id,
                                        video_id = %vid,
                                        error = %e,
                                        "Failed to save yt-dlp metadata JSON alongside video"
                                    );
                                    None
                                }
                            }
                        } else {
                            None
                        };

                        // Register video in database (using predictable path as canonical)
                        if let Some(ref canonical_key) = predictable_key {
                            match get_or_create_video_file(
                                db.pool(),
                                vid,
                                platform,
                                canonical_key,
                                metadata_key.as_deref(),
                                size_bytes,
                                Some(&content_type),
                                None, // duration could be extracted from metadata if needed
                            )
                            .await
                            {
                                Ok(vf) => {
                                    debug!(
                                        archive_id,
                                        video_id = %vid,
                                        video_file_id = vf.id,
                                        s3_key = %canonical_key,
                                        "Registered video in database"
                                    );
                                }
                                Err(e) => {
                                    warn!(
                                        archive_id,
                                        video_id = %vid,
                                        error = %e,
                                        "Failed to register video in database"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Ensure raw HTML is always archived and derived artifacts are produced, even when the primary
    // artifact is a video or image (e.g., Reddit posts with embedded media).
    let raw_html_path = work_dir.join("raw.html");
    let raw_is_primary = matches!(result.primary_file.as_deref(), Some("raw.html"));
    if raw_html_path.exists() {
        // Check if file is empty before uploading (empty files can cause S3 signature errors)
        let metadata = tokio::fs::metadata(&raw_html_path).await.ok();
        let size_bytes = metadata.as_ref().map(|m| m.len() as i64);

        if let Some(0) = size_bytes {
            warn!(archive_id, "Skipping upload of empty raw.html file");
        } else if !raw_is_primary {
            let raw_key = format!("{s3_prefix}media/raw.html");

            match s3
                .upload_file(&raw_html_path, &raw_key, Some(archive_id))
                .await
            {
                Ok(()) => {
                    if let Err(e) = insert_artifact(
                        db.pool(),
                        archive_id,
                        ArtifactKind::RawHtml.as_str(),
                        &raw_key,
                        Some("text/html"),
                        size_bytes,
                        None,
                    )
                    .await
                    {
                        warn!(archive_id, error = %e, "Failed to insert raw.html artifact record");
                    }
                }
                Err(e) => {
                    warn!(archive_id, error = %e, "Failed to upload raw.html");
                }
            }
        }

        // Upload method-specific HTML files (raw_cdp.html, raw_dump_dom.html, etc.)
        // These are created by Twitter archiving to allow comparison between methods
        // Skip for Twitter - we only save raw.html, screenshot shows the content
        if !is_twitter {
            for method_file in &[
                "raw_cdp.html",
                "raw_dump_dom.html",
                "raw_http.html",
                "raw_http_cookies.html",
            ] {
                let method_path = work_dir.join(method_file);
                if method_path.exists() {
                    let metadata = tokio::fs::metadata(&method_path).await.ok();
                    let size_bytes = metadata.as_ref().map(|m| m.len() as i64);

                    if let Some(0) = size_bytes {
                        debug!(archive_id, file = %method_file, "Skipping upload of empty method-specific HTML file");
                        continue;
                    }

                    let method_key = format!("{s3_prefix}media/{method_file}");
                    // Derive artifact kind from filename (e.g., "raw_cdp.html" -> "raw_html_cdp")
                    let artifact_kind = method_file
                        .strip_suffix(".html")
                        .unwrap_or(method_file)
                        .replace("raw_", "raw_html_");

                    match s3
                        .upload_file(&method_path, &method_key, Some(archive_id))
                        .await
                    {
                        Ok(()) => {
                            debug!(archive_id, file = %method_file, key = %method_key, "Uploaded method-specific HTML");
                            if let Err(e) = insert_artifact(
                                db.pool(),
                                archive_id,
                                &artifact_kind,
                                &method_key,
                                Some("text/html"),
                                size_bytes,
                                None,
                            )
                            .await
                            {
                                warn!(archive_id, file = %method_file, error = %e, "Failed to insert artifact record");
                            }
                        }
                        Err(e) => {
                            warn!(archive_id, file = %method_file, error = %e, "Failed to upload method-specific HTML");
                        }
                    }
                }
            }
        }

        // Create view.html and complete.html for non-Twitter archives
        // Twitter archives use screenshot as the default view, so these aren't needed
        if is_twitter {
            debug!(archive_id, "Skipping view.html and complete.html for Twitter - using screenshot as default view");
        } else {
            match create_view_html(
                db,
                archive_id,
                link_id,
                &raw_html_path,
                &work_dir,
                s3,
                &s3_prefix,
            )
            .await
            {
                Ok(view_size) => {
                    debug!(archive_id, "Created view.html with archive banner");
                    let view_key = format!("{s3_prefix}media/view.html");
                    if let Err(e) = insert_artifact(
                        db.pool(),
                        archive_id,
                        ArtifactKind::ViewHtml.as_str(),
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

            // Create complete.html with monolith if enabled (non-fatal if fails)
            let monolith_config = config.monolith_config();
            if monolith_config.enabled {
                let complete_path = work_dir.join("complete.html");
                let cookies_file = config.cookies_file_path.as_deref();

                let monolith_input = Url::from_file_path(&raw_html_path)
                    .ok()
                    .map_or_else(|| raw_html_path.display().to_string(), |u| u.to_string());

                let monolith_job = start_job(db.pool(), archive_id, ArchiveJobType::Monolith).await;

                match create_complete_html(
                    &monolith_input,
                    &complete_path,
                    cookies_file,
                    &monolith_config,
                )
                .await
                {
                    Ok(()) => {
                        let complete_key = format!("{s3_prefix}media/complete.html");
                        let metadata = tokio::fs::metadata(&complete_path).await.ok();
                        let size_bytes = metadata.map(|m| m.len() as i64);
                        if let Err(e) = s3
                            .upload_file(&complete_path, &complete_key, Some(archive_id))
                            .await
                        {
                            warn!(archive_id, error = %e, "Failed to upload complete.html");
                        } else {
                            debug!(archive_id, key = %complete_key, "Uploaded complete.html");
                            if let Err(e) = insert_artifact(
                                db.pool(),
                                archive_id,
                                ArtifactKind::CompleteHtml.as_str(),
                                &complete_key,
                                Some("text/html"),
                                size_bytes,
                                None,
                            )
                            .await
                            {
                                warn!(archive_id, error = %e, "Failed to insert complete.html artifact record");
                            } else {
                                let size_meta = size_bytes.map(|s| format!("{s} bytes"));
                                complete_job(db.pool(), monolith_job, size_meta.as_deref()).await;
                            }
                        }
                    }
                    Err(e) => {
                        let err_str = e.to_string();
                        if err_str.contains("exit code Some(101)") || err_str.contains("panicked") {
                            warn!(
                                archive_id,
                                error = %e,
                                "Monolith crashed processing this page - likely a tool limitation with this content type"
                            );
                            fail_job(db.pool(), monolith_job, &err_str).await;
                        } else {
                            warn!(archive_id, error = %e, "Failed to create complete.html with monolith");
                            fail_job(db.pool(), monolith_job, &err_str).await;
                        }
                    }
                }
            }
        } // end of else block for !is_twitter
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
                s3.upload_file(&local_path, &key, Some(archive_id)).await?;
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

    // Separate subtitle files and comments from other extra files for special processing
    let mut subtitle_files = Vec::new();
    let mut comments_file = None;
    let mut other_extra_files = Vec::new();

    for extra_file in &result.extra_files {
        if extra_file.ends_with(".vtt") || extra_file.ends_with(".srt") {
            subtitle_files.push(extra_file.clone());
        } else if extra_file == "comments.json" {
            comments_file = Some(extra_file.clone());
        } else {
            other_extra_files.push(extra_file.clone());
        }
    }

    // Upload comments.json if present
    if let Some(comments_filename) = comments_file {
        let local_path = work_dir.join(&comments_filename);
        if local_path.exists() {
            let key = format!("{s3_prefix}comments.json");

            // Read comment stats from JSON for metadata
            match tokio::fs::read_to_string(&local_path).await {
                Ok(json_str) => {
                    if let Ok(comments_json) = serde_json::from_str::<serde_json::Value>(&json_str)
                    {
                        // Extract comment statistics
                        let comment_count = comments_json
                            .get("stats")
                            .and_then(|s| s.get("extracted_comments"))
                            .and_then(|c| c.as_i64())
                            .unwrap_or(0);

                        let extraction_method = comments_json
                            .get("extraction_method")
                            .and_then(|m| m.as_str())
                            .unwrap_or("unknown");

                        let limited = comments_json
                            .get("limited")
                            .and_then(|l| l.as_bool())
                            .unwrap_or(false);

                        // Upload to S3
                        if let Err(e) = s3.upload_file(&local_path, &key, Some(archive_id)).await {
                            warn!(archive_id, error = %e, "Failed to upload comments.json");
                        } else {
                            let size_bytes = tokio::fs::metadata(&local_path)
                                .await
                                .ok()
                                .map(|m| m.len() as i64);

                            info!(
                                archive_id,
                                key = %key,
                                comment_count,
                                limited,
                                "Uploaded comments artifact"
                            );

                            // Insert artifact record with metadata
                            let artifact_metadata = serde_json::json!({
                                "comment_count": comment_count,
                                "extraction_method": extraction_method,
                                "limited": limited,
                            });

                            if let Err(e) = insert_artifact_with_metadata(
                                db.pool(),
                                archive_id,
                                "comments",
                                &key,
                                Some("application/json"),
                                size_bytes,
                                None,
                                Some(&artifact_metadata.to_string()),
                            )
                            .await
                            {
                                warn!(archive_id, error = %e, "Failed to insert comments artifact record");
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(archive_id, error = %e, "Failed to read comments.json");
                }
            }
        }
    }

    // Upload non-subtitle extra files (images, etc.) from handlers
    for extra_file in &other_extra_files {
        let local_path = work_dir.join(extra_file);
        if local_path.exists() {
            let key = format!("{s3_prefix}media/{extra_file}");
            let metadata = tokio::fs::metadata(&local_path).await.ok();
            let size_bytes = metadata.map(|m| m.len() as i64);
            let content_type = mime_guess::from_path(&local_path)
                .first_or_octet_stream()
                .to_string();

            if let Err(e) = s3.upload_file(&local_path, &key, Some(archive_id)).await {
                warn!(archive_id, file = %extra_file, error = %e, "Failed to upload extra file");
                continue;
            }

            debug!(archive_id, file = %extra_file, "Uploaded extra file");

            // Determine artifact kind based on content type
            let kind = if content_type.starts_with("image/") {
                ArtifactKind::Image
            } else if content_type.starts_with("video/") {
                ArtifactKind::Video
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

    // Process subtitle files with metadata tracking
    if !subtitle_files.is_empty() {
        process_subtitle_files(db, s3, archive_id, &subtitle_files, &work_dir, &s3_prefix).await;
    }

    // Capture screenshot if enabled (non-fatal if it fails)
    // Skip screenshots for direct PDF files - they're already archived
    let is_pdf = result.content_type == "pdf";
    let skip_browser_captures = is_pdf || is_youtube;

    if screenshot.is_enabled() && !skip_browser_captures {
        let screenshot_job = start_job(db.pool(), archive_id, ArchiveJobType::Screenshot).await;
        match screenshot.capture(&link.normalized_url).await {
            Ok(webp_data) => {
                let screenshot_key = format!("{s3_prefix}render/screenshot.webp");
                let size_bytes = Some(webp_data.len() as i64);
                if let Err(e) = s3
                    .upload_bytes(&webp_data, &screenshot_key, "image/webp")
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
                        Some("image/webp"),
                        size_bytes,
                        None,
                    )
                    .await
                    {
                        warn!(archive_id, error = %e, "Failed to insert screenshot artifact record");
                    }
                    let size_meta = size_bytes.map(|s| format!("{s} bytes"));
                    complete_job(db.pool(), screenshot_job, size_meta.as_deref()).await;
                }
            }
            Err(e) => {
                warn!(archive_id, error = %e, "Failed to capture screenshot");
                fail_job(db.pool(), screenshot_job, &format!("{e:#}")).await;
            }
        }
    } else if is_youtube && screenshot.is_enabled() {
        skip_job(
            db.pool(),
            archive_id,
            ArchiveJobType::Screenshot,
            "Disabled for YouTube content",
        )
        .await;
    }

    // Generate PDF if enabled (non-fatal if it fails)
    // Skip PDF generation for direct PDF files - they're already archived
    if screenshot.is_pdf_enabled() && !skip_browser_captures {
        let pdf_job = start_job(db.pool(), archive_id, ArchiveJobType::Pdf).await;
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
                    let size_meta = size_bytes.map(|s| format!("{s} bytes"));
                    complete_job(db.pool(), pdf_job, size_meta.as_deref()).await;
                }
            }
            Err(e) => {
                warn!(archive_id, error = %e, "Failed to generate PDF");
                fail_job(db.pool(), pdf_job, &format!("{e:#}")).await;
            }
        }
    } else if is_youtube && screenshot.is_pdf_enabled() {
        skip_job(
            db.pool(),
            archive_id,
            ArchiveJobType::Pdf,
            "Disabled for YouTube content",
        )
        .await;
    }

    // Generate MHTML archive if enabled (non-fatal if it fails)
    // Skip MHTML for direct PDF files - they're already archived
    if screenshot.is_mhtml_enabled() && !skip_browser_captures {
        let mhtml_job = start_job(db.pool(), archive_id, ArchiveJobType::Mhtml).await;
        match screenshot.capture_mhtml(&link.normalized_url).await {
            Ok(mhtml_data) => {
                let mhtml_key = format!("{s3_prefix}render/complete.mhtml");
                let size_bytes = Some(mhtml_data.len() as i64);
                if let Err(e) = s3
                    .upload_bytes(&mhtml_data, &mhtml_key, "message/rfc822")
                    .await
                {
                    warn!(archive_id, error = %e, "Failed to upload MHTML");
                } else {
                    debug!(archive_id, key = %mhtml_key, "MHTML archive uploaded");
                    // Insert MHTML artifact record
                    if let Err(e) = insert_artifact(
                        db.pool(),
                        archive_id,
                        ArtifactKind::Mhtml.as_str(),
                        &mhtml_key,
                        Some("message/rfc822"),
                        size_bytes,
                        None,
                    )
                    .await
                    {
                        warn!(archive_id, error = %e, "Failed to insert MHTML artifact record");
                    }
                    let size_meta = size_bytes.map(|s| format!("{s} bytes"));
                    complete_job(db.pool(), mhtml_job, size_meta.as_deref()).await;
                }
            }
            Err(e) => {
                warn!(archive_id, error = %e, "Failed to generate MHTML archive");
                fail_job(db.pool(), mhtml_job, &format!("{e:#}")).await;
            }
        }
    } else if is_youtube && screenshot.is_mhtml_enabled() {
        skip_job(
            db.pool(),
            archive_id,
            ArchiveJobType::Mhtml,
            "Disabled for YouTube content",
        )
        .await;
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

    // Queue comment extraction job if this is a comments-supported platform
    // Skip comment extraction for playlists (they have no comments)
    let is_playlist = result.content_type == "playlist";
    if config.comments_enabled
        && !is_playlist
        && is_comments_supported_platform(&link.domain, config)
    {
        match create_archive_job(db.pool(), archive_id, ArchiveJobType::CommentExtraction).await {
            Ok(job_id) => {
                debug!(
                    archive_id,
                    job_id,
                    platform = extract_platform_name(&link.domain),
                    "Queued comment extraction job"
                );
            }
            Err(e) => {
                warn!(
                    archive_id,
                    error = %e,
                    "Failed to queue comment extraction job"
                );
            }
        }
    } else if is_playlist {
        debug!(
            archive_id,
            "Skipping comment extraction for playlist (playlists have no comments)"
        );
    }

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

    // Extract Open Graph metadata from raw.html if available
    let raw_html_path = work_dir.join("raw.html");
    if raw_html_path.exists() {
        match tokio::fs::read_to_string(&raw_html_path).await {
            Ok(html_content) => {
                match og_extractor::extract_og_metadata(&html_content) {
                    Ok(og_metadata) => {
                        if og_metadata.has_content() {
                            // Save extracted metadata to database
                            if let Err(e) = update_archive_og_metadata(
                                db.pool(),
                                archive_id,
                                og_metadata.title.as_deref(),
                                og_metadata.description.as_deref(),
                                og_metadata.image.as_deref(),
                                og_metadata.og_type.as_deref(),
                            )
                            .await
                            {
                                warn!(archive_id, error = %e, "Failed to save OG metadata");
                            } else {
                                debug!(
                                    archive_id,
                                    og_title = ?og_metadata.title,
                                    "Extracted and saved OG metadata"
                                );
                            }
                        } else {
                            // No OG metadata found, mark as attempted to avoid future retries
                            if let Err(e) =
                                mark_og_extraction_attempted(db.pool(), archive_id).await
                            {
                                warn!(
                                    archive_id,
                                    error = %e,
                                    "Failed to mark OG extraction as attempted"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(archive_id, error = %e, "Failed to extract OG metadata");
                        // Mark as attempted even on failure to avoid repeated attempts
                        let _ = mark_og_extraction_attempted(db.pool(), archive_id).await;
                    }
                }
            }
            Err(e) => {
                debug!(archive_id, error = %e, "Could not read raw.html for OG extraction");
            }
        }
    }

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

/// Check if a YouTube video already exists on S3.
///
/// Returns the existing S3 key if found, along with file extension.
async fn check_existing_youtube_video(
    s3: &S3Client,
    video_id: &str,
) -> Result<Option<(String, String)>> {
    // Check for common video extensions at predictable path
    let video_extensions = ["webm", "mp4", "mkv", "mov", "avi"];
    let video_prefix = format!("videos/{video_id}");

    for ext in video_extensions {
        let key = format!("{video_prefix}.{ext}");
        if s3.object_exists(&key).await? {
            debug!(video_id = %video_id, s3_key = %key, "Found existing YouTube video on S3");
            return Ok(Some((key, ext.to_string())));
        }
    }

    Ok(None)
}

/// Copy a video file to the predictable video path on S3.
async fn copy_video_to_predictable_path(
    s3: &S3Client,
    source_key: &str,
    video_id: &str,
) -> Result<String> {
    // Extract extension from source key
    let ext = source_key.rsplit('.').next().unwrap_or("mp4");

    let target_key = format!("videos/{video_id}.{ext}");

    // Check if target already exists (might be copied from a different archive)
    if s3.object_exists(&target_key).await? {
        debug!(video_id = %video_id, target_key = %target_key, "Video already exists at predictable path");
        return Ok(target_key);
    }

    // Use S3 server-side copy (more efficient than download + re-upload)
    s3.copy_object(source_key, &target_key).await?;

    info!(
        video_id = %video_id,
        source_key = %source_key,
        target_key = %target_key,
        "Copied video to predictable S3 path"
    );

    Ok(target_key)
}

/// Create a job record and mark it running. Returns the job ID when successful.
async fn start_job(
    pool: &sqlx::SqlitePool,
    archive_id: i64,
    job_type: ArchiveJobType,
) -> Option<i64> {
    match create_archive_job(pool, archive_id, job_type).await {
        Ok(id) => {
            if let Err(e) = set_job_running(pool, id).await {
                warn!(archive_id, job_type = ?job_type, error = %e, "Failed to set job running");
                None
            } else {
                Some(id)
            }
        }
        Err(e) => {
            warn!(archive_id, job_type = ?job_type, error = %e, "Failed to create archive job");
            None
        }
    }
}

async fn complete_job(pool: &sqlx::SqlitePool, job_id: Option<i64>, metadata: Option<&str>) {
    if let Some(id) = job_id {
        if let Err(e) = set_job_completed(pool, id, metadata).await {
            warn!(job_id = id, error = %e, "Failed to mark job completed");
        }
    }
}

async fn fail_job(pool: &sqlx::SqlitePool, job_id: Option<i64>, error: &str) {
    if let Some(id) = job_id {
        if let Err(e) = set_job_failed(pool, id, error).await {
            warn!(job_id = id, error = %e, "Failed to mark job failed");
        }
    }
}

async fn skip_job(
    pool: &sqlx::SqlitePool,
    archive_id: i64,
    job_type: ArchiveJobType,
    reason: &str,
) {
    match create_archive_job(pool, archive_id, job_type).await {
        Ok(id) => {
            if let Err(e) = set_job_skipped(pool, id, Some(reason)).await {
                warn!(archive_id, job_id = id, error = %e, "Failed to mark job skipped");
            }
        }
        Err(e) => {
            warn!(archive_id, job_type = ?job_type, error = %e, "Failed to record skipped job");
        }
    }
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
/// Process subtitle files: upload them with metadata and generate a transcript.
async fn process_subtitle_files(
    db: &Database,
    s3: &S3Client,
    archive_id: i64,
    subtitle_files: &[String],
    work_dir: &Path,
    s3_prefix: &str,
) {
    use super::transcript::build_transcript_from_file;
    use super::ytdlp::{parse_subtitle_info, parse_vtt_language_from_file};

    // Categorize subtitles by language and type
    // Tuple: (path, language, is_english, is_auto)
    let mut best_subtitle_for_transcript: Option<(PathBuf, String, bool, bool)> = None;

    // Upload each subtitle file with metadata
    for subtitle_file in subtitle_files {
        let local_path = work_dir.join(subtitle_file);
        if !local_path.exists() {
            warn!(archive_id, file = %subtitle_file, "Subtitle file not found");
            continue;
        }

        let (mut language, is_auto, format) = parse_subtitle_info(subtitle_file);
        let mut detected_from = "filename";

        // If language is unknown or looks like a placeholder (NA, etc.),
        // try to read it from the VTT file header
        if (language == "unknown" || language == "NA" || language.len() > 10) && format == "vtt" {
            if let Some(header_lang) = parse_vtt_language_from_file(&local_path).await {
                debug!(
                    archive_id,
                    file = %subtitle_file,
                    filename_lang = %language,
                    header_lang = %header_lang,
                    "Detected language from VTT header"
                );
                language = header_lang;
                detected_from = "vtt_header";
            }
        }
        let key = format!("{s3_prefix}subtitles/{subtitle_file}");
        let metadata_result = tokio::fs::metadata(&local_path).await.ok();
        let size_bytes = metadata_result.map(|m| m.len() as i64);

        let content_type = if format == "vtt" {
            "text/vtt"
        } else {
            "application/x-subrip"
        };

        // Upload subtitle file
        if let Err(e) = s3.upload_file(&local_path, &key, Some(archive_id)).await {
            warn!(archive_id, file = %subtitle_file, error = %e, "Failed to upload subtitle file");
            continue;
        }

        debug!(
            archive_id,
            file = %subtitle_file,
            language = %language,
            is_auto = is_auto,
            "Uploaded subtitle file"
        );

        // Store metadata about the subtitle in JSON format
        let subtitle_metadata = serde_json::json!({
            "language": language,
            "is_auto": is_auto,
            "format": format,
        });

        // Insert subtitle artifact with metadata
        match crate::db::insert_artifact_with_metadata(
            db.pool(),
            archive_id,
            ArtifactKind::Subtitles.as_str(),
            &key,
            Some(content_type),
            size_bytes,
            None, // sha256
            Some(&subtitle_metadata.to_string()),
        )
        .await
        {
            Ok(artifact_id) => {
                // Insert language info into subtitle_languages table
                if let Err(e) = crate::db::upsert_subtitle_language(
                    db.pool(),
                    artifact_id,
                    &language,
                    detected_from,
                    is_auto,
                )
                .await
                {
                    warn!(
                        archive_id,
                        artifact_id,
                        error = %e,
                        "Failed to insert subtitle language"
                    );
                }
            }
            Err(e) => {
                warn!(archive_id, file = %subtitle_file, error = %e, "Failed to insert subtitle artifact");
            }
        }

        // Select best subtitle for transcript generation
        // Prefer: English manual > English auto > any manual > any auto
        let is_english = language.starts_with("en");

        match &best_subtitle_for_transcript {
            None => {
                // No subtitle selected yet, use this one
                best_subtitle_for_transcript =
                    Some((local_path.clone(), language.clone(), is_english, is_auto));
            }
            Some((_, _, best_is_english, best_is_auto)) => {
                // Determine if current subtitle is better
                let current_is_better = match (is_english, *best_is_english) {
                    // English always beats non-English
                    (true, false) => true,
                    // Non-English never beats English
                    (false, true) => false,
                    // Same language group: manual beats auto
                    _ => !is_auto && *best_is_auto,
                };
                if current_is_better {
                    best_subtitle_for_transcript =
                        Some((local_path.clone(), language.clone(), is_english, is_auto));
                }
            }
        }
    }

    // Generate transcript from the best subtitle file
    if let Some((best_subtitle_path, language, is_english, is_auto)) = best_subtitle_for_transcript
    {
        match build_transcript_from_file(&best_subtitle_path).await {
            Ok(transcript) if !transcript.is_empty() => {
                let transcript_key = format!("{s3_prefix}subtitles/transcript.txt");
                let size_bytes = transcript.len() as i64;

                // Upload transcript
                match s3
                    .upload_bytes(transcript.as_bytes(), &transcript_key, "text/plain")
                    .await
                {
                    Ok(()) => {
                        debug!(
                            archive_id,
                            key = %transcript_key,
                            language = %language,
                            is_english,
                            is_auto,
                            size = size_bytes,
                            "Generated and uploaded transcript"
                        );

                        // Store metadata about transcript source
                        let transcript_metadata = serde_json::json!({
                            "source": if is_auto { "auto_subtitles" } else { "manual_subtitles" },
                            "language": language,
                            "source_file": best_subtitle_path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown"),
                        });

                        // Insert transcript artifact
                        if let Err(e) = crate::db::insert_artifact_with_metadata(
                            db.pool(),
                            archive_id,
                            "transcript", // Custom artifact type for transcripts
                            &transcript_key,
                            Some("text/plain"),
                            Some(size_bytes),
                            None,
                            Some(&transcript_metadata.to_string()),
                        )
                        .await
                        {
                            warn!(archive_id, error = %e, "Failed to insert transcript artifact");
                        }
                    }
                    Err(e) => {
                        warn!(archive_id, error = %e, "Failed to upload transcript");
                    }
                }
            }
            Ok(_) => {
                debug!(archive_id, "Transcript was empty, skipping upload");
            }
            Err(e) => {
                warn!(archive_id, error = %e, "Failed to generate transcript");
            }
        }
    }
}

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
