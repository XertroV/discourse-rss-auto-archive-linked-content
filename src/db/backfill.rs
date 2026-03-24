//! Background workers to backfill content from S3.
//!
//! This module provides background tasks for:
//! - Populating `transcript_text` for existing archives with transcripts on S3
//! - Downloading TikTok subtitles from meta.json for archives without subtitles
//! - Regenerating YouTube transcripts to fix VTT rolling subtitle triplication

use std::time::Duration;

use anyhow::Result;
use tracing::{debug, info, warn};

use crate::db::{
    create_archive_job, get_archives_needing_metrics_backfill,
    get_archives_needing_transcript_backfill, get_tiktok_archives_needing_subtitle_backfill,
    get_youtube_archives_with_vtt_subtitles, insert_artifact, set_archive_engagement_metrics,
    set_archive_transcript_text, set_job_completed, set_job_failed, set_job_running,
    set_job_skipped, ArchiveJobType, Database, EngagementMetrics,
};
use crate::s3::S3Client;

/// Create a `SupplementaryArtifacts` job and mark it running. Returns the job ID.
async fn start_backfill_job(db: &Database, archive_id: i64) -> Option<i64> {
    match create_archive_job(
        db.pool(),
        archive_id,
        ArchiveJobType::SupplementaryArtifacts,
    )
    .await
    {
        Ok(id) => {
            if let Err(e) = set_job_running(db.pool(), id).await {
                warn!(archive_id, error = %e, "Failed to set backfill job running");
                None
            } else {
                Some(id)
            }
        }
        Err(e) => {
            warn!(archive_id, error = %e, "Failed to create backfill job");
            None
        }
    }
}

async fn complete_backfill_job(db: &Database, job_id: Option<i64>, metadata: Option<&str>) {
    if let Some(id) = job_id {
        if let Err(e) = set_job_completed(db.pool(), id, metadata).await {
            warn!(job_id = id, error = %e, "Failed to mark backfill job completed");
        }
    }
}

async fn fail_backfill_job(db: &Database, job_id: Option<i64>, error: &str) {
    if let Some(id) = job_id {
        if let Err(e) = set_job_failed(db.pool(), id, error).await {
            warn!(job_id = id, error = %e, "Failed to mark backfill job failed");
        }
    }
}

async fn skip_backfill_job(db: &Database, job_id: Option<i64>, reason: &str) {
    if let Some(id) = job_id {
        if let Err(e) = set_job_skipped(db.pool(), id, Some(reason)).await {
            warn!(job_id = id, error = %e, "Failed to mark backfill job skipped");
        }
    }
}

/// Bump this version to force re-scanning metrics for all archives.
/// You can also pass a domain filter to `run_metrics_backfill` to re-scan a specific source.
pub const METRICS_BACKFILL_VERSION: i64 = 1;

/// Bump this version to force re-scanning TikTok subtitle backfill for all archives.
///
/// The version is stored in the `s3_key` field of the `subtitle_backfill_attempted` marker
/// artifact. Old markers (pre-versioning) have `s3_key = "none"` which casts to 0, so
/// setting this to 2 causes all previously-attempted archives to be retried.
///
/// History:
/// - v1 (implicit, "none"): Initial backfill; bugs caused all downloads to fail (no Referer
///   header → 403, language code mismatch `en` vs `eng-US`).
/// - v2: Referer/Origin headers added, language wildcard `en.*` to match `eng-US`. Reset
///   all prior markers so affected archives are retried.
pub const SUBTITLE_BACKFILL_VERSION: i64 = 3;

/// Configuration for the backfill worker.
pub struct BackfillConfig {
    /// Number of archives to process per batch.
    pub batch_size: i64,
    /// Delay between processing each archive (to pace S3 requests).
    pub item_delay: Duration,
    /// Delay between batches.
    pub batch_delay: Duration,
}

impl Default for BackfillConfig {
    fn default() -> Self {
        Self {
            batch_size: 50,
            item_delay: Duration::from_millis(100),
            batch_delay: Duration::from_secs(1),
        }
    }
}

/// Run the backfill worker.
///
/// This function runs in a loop, fetching batches of archives that need
/// transcript backfill and populating their `transcript_text` column from S3.
///
/// The worker exits when all archives have been backfilled.
pub async fn run_backfill_worker(db: Database, s3: S3Client, config: BackfillConfig) {
    info!("Starting search content backfill worker");

    let mut total_backfilled: u64 = 0;

    loop {
        // Fetch batch of archives needing backfill
        let batch = match get_archives_needing_transcript_backfill(db.pool(), config.batch_size)
            .await
        {
            Ok(batch) => batch,
            Err(e) => {
                warn!(error = %e, "Failed to get archives needing backfill, retrying in 1 minute");
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }
        };

        if batch.is_empty() {
            info!(
                total = total_backfilled,
                "Search content backfill complete - no more archives to process"
            );
            break;
        }

        let batch_size = batch.len();
        let mut batch_success = 0u64;

        for (archive_id, s3_key) in batch {
            match backfill_archive_transcript(&db, &s3, archive_id, &s3_key).await {
                Ok(true) => {
                    batch_success += 1;
                    total_backfilled += 1;
                    debug!(archive_id, "Backfilled transcript text");
                }
                Ok(false) => {
                    debug!(archive_id, s3_key, "Transcript not found or empty on S3");
                }
                Err(e) => {
                    warn!(
                        archive_id,
                        s3_key,
                        error = %e,
                        "Failed to backfill transcript"
                    );
                }
            }

            // Pace ourselves to avoid overwhelming S3
            tokio::time::sleep(config.item_delay).await;
        }

        info!(
            batch_size,
            batch_success,
            total = total_backfilled,
            "Processed backfill batch"
        );

        // Short delay between batches
        tokio::time::sleep(config.batch_delay).await;
    }
}

/// Backfill transcript text for a single archive.
///
/// Returns `Ok(true)` if transcript was backfilled, `Ok(false)` if not found or empty.
async fn backfill_archive_transcript(
    db: &Database,
    s3: &S3Client,
    archive_id: i64,
    s3_key: &str,
) -> Result<bool> {
    let job_id = start_backfill_job(db, archive_id).await;

    let result: Result<bool> = async {
        // Fetch transcript from S3
        let response = s3.get_object(s3_key).await?;

        let Some((bytes, _content_type)) = response else {
            return Ok(false);
        };

        // Convert to UTF-8 text
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(e) => {
                warn!(
                    archive_id,
                    s3_key,
                    error = %e,
                    "Transcript is not valid UTF-8"
                );
                return Ok(false);
            }
        };

        // Skip empty transcripts
        if text.trim().is_empty() {
            return Ok(false);
        }

        // Store in database
        set_archive_transcript_text(db.pool(), archive_id, &text).await?;

        Ok(true)
    }
    .await;

    match &result {
        Ok(true) => complete_backfill_job(db, job_id, None).await,
        Ok(false) => skip_backfill_job(db, job_id, "Transcript not found or empty on S3").await,
        Err(e) => fail_backfill_job(db, job_id, &e.to_string()).await,
    }

    result
}

// ========== TikTok Subtitle Backfill ==========

/// Configuration for the TikTok subtitle backfill worker.
pub struct TikTokSubtitleBackfillConfig {
    /// Number of archives to process per batch.
    pub batch_size: i64,
    /// Delay between processing each archive (to pace TikTok CDN requests).
    pub item_delay: Duration,
    /// Delay between batches.
    pub batch_delay: Duration,
}

impl Default for TikTokSubtitleBackfillConfig {
    fn default() -> Self {
        Self {
            batch_size: 20,
            item_delay: Duration::from_millis(500), // Be gentle on TikTok CDN
            batch_delay: Duration::from_secs(5),
        }
    }
}

/// Run the TikTok subtitle backfill worker.
///
/// This function runs once at startup, finding TikTok video archives without subtitles
/// and attempting to download them from the meta.json stored on S3.
///
/// The worker exits when all eligible archives have been processed.
pub async fn run_tiktok_subtitle_backfill_worker(
    db: Database,
    s3: S3Client,
    config: TikTokSubtitleBackfillConfig,
) {
    info!("Starting TikTok subtitle backfill worker");

    let mut total_processed: u64 = 0;
    let mut total_success: u64 = 0;

    loop {
        // Fetch batch of TikTok archives needing subtitle backfill
        let batch = match get_tiktok_archives_needing_subtitle_backfill(
            db.pool(),
            config.batch_size,
            SUBTITLE_BACKFILL_VERSION,
        )
        .await
        {
            Ok(batch) => batch,
            Err(e) => {
                warn!(error = %e, "Failed to get TikTok archives needing subtitle backfill, retrying in 1 minute");
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }
        };

        if batch.is_empty() {
            info!(
                total_processed,
                total_success, "TikTok subtitle backfill complete - no more archives to process"
            );
            break;
        }

        let batch_size = batch.len();
        let mut batch_success = 0u64;

        for (archive_id, meta_s3_key, original_url) in batch {
            let (count, should_mark) =
                match backfill_tiktok_subtitles(&db, &s3, archive_id, &meta_s3_key, &original_url)
                    .await
                {
                    Ok((count, should_mark)) => {
                        if count > 0 {
                            batch_success += 1;
                            total_success += 1;
                            info!(
                                archive_id,
                                subtitle_count = count,
                                "Backfilled TikTok subtitles"
                            );
                        }
                        (count, should_mark)
                    }
                    Err(e) => {
                        warn!(
                            archive_id,
                            meta_s3_key,
                            error = %e,
                            "Failed to backfill TikTok subtitles"
                        );
                        (0, true) // Mark as attempted on error too
                    }
                };

            // Insert marker artifact to prevent re-processing this archive.
            // The version is stored in s3_key so the query can filter by it;
            // bumping SUBTITLE_BACKFILL_VERSION forces a re-scan of old markers.
            //
            // Delete any pre-existing markers first to avoid accumulating duplicate
            // rows across version bumps (old markers have s3_key='none', new ones
            // have the version number).
            if should_mark && count == 0 {
                if let Err(e) = sqlx::query(
                    "DELETE FROM archive_artifacts WHERE archive_id = ? AND kind = 'subtitle_backfill_attempted'",
                )
                .bind(archive_id)
                .execute(db.pool())
                .await
                {
                    warn!(
                        archive_id,
                        error = %e,
                        "Failed to delete old subtitle backfill markers"
                    );
                }

                let version_key = SUBTITLE_BACKFILL_VERSION.to_string();
                if let Err(e) = insert_artifact(
                    db.pool(),
                    archive_id,
                    "subtitle_backfill_attempted",
                    &version_key, // Version stored here for filtering
                    None,
                    None,
                    None,
                )
                .await
                {
                    warn!(
                        archive_id,
                        error = %e,
                        "Failed to insert subtitle backfill marker"
                    );
                }
            }

            total_processed += 1;

            // Pace ourselves to avoid overwhelming TikTok CDN
            tokio::time::sleep(config.item_delay).await;
        }

        info!(
            batch_size,
            batch_success,
            total_processed,
            total_success,
            "Processed TikTok subtitle backfill batch"
        );

        // Short delay between batches
        tokio::time::sleep(config.batch_delay).await;
    }
}

/// Backfill subtitles for a single TikTok archive.
///
/// Returns the number of subtitle files downloaded, or 0 if none found/available.
/// Returns -1 (cast to usize wraps) when we've attempted but found nothing (for marking).
async fn backfill_tiktok_subtitles(
    db: &Database,
    s3: &S3Client,
    archive_id: i64,
    meta_s3_key: &str,
    original_url: &str,
) -> Result<(usize, bool)> {
    // Returns (count, should_mark_attempted)
    use crate::archiver::process_subtitle_files;
    use crate::handlers::tiktok::{download_tiktok_subtitles, extract_subtitle_info};

    let job_id = start_backfill_job(db, archive_id).await;

    let result = async {
        // Fetch meta.json from S3
        let response = s3.get_object(meta_s3_key).await?;

        let Some((bytes, _content_type)) = response else {
            debug!(archive_id, meta_s3_key, "meta.json not found in S3");
            return Ok((0usize, true, "meta.json not found in S3")); // Mark as attempted - no meta.json
        };

        // Parse JSON
        let meta_json = match String::from_utf8(bytes) {
            Ok(json) => json,
            Err(e) => {
                warn!(
                    archive_id,
                    meta_s3_key,
                    error = %e,
                    "TikTok meta.json is not valid UTF-8"
                );
                return Ok((0, true, "meta.json is not valid UTF-8")); // Mark as attempted - invalid JSON
            }
        };

        // Extract subtitle info
        let subtitles = extract_subtitle_info(&meta_json);
        if subtitles.is_empty() {
            debug!(
                archive_id,
                "No subtitles field in meta.json or subtitles object is empty"
            );
            return Ok((0, true, "no subtitles in meta.json")); // Mark as attempted - no subtitles in JSON
        }

        // Log what languages we found
        let languages: Vec<&str> = subtitles.iter().map(|s| s.language_code.as_str()).collect();
        let has_english = subtitles
            .iter()
            .any(|s| s.language_code.starts_with("eng-"));
        debug!(
            archive_id,
            languages = ?languages,
            has_english,
            "Found subtitles in TikTok meta.json"
        );

        // Create a temporary work directory
        let work_dir =
            std::env::temp_dir().join(format!("tiktok-subtitle-backfill-{}", archive_id));
        std::fs::create_dir_all(&work_dir)?;

        // Download subtitles (English only for now)
        let filenames = match download_tiktok_subtitles(&subtitles, &work_dir, true).await {
            Ok(files) => files,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&work_dir);
                return Err(e);
            }
        };

        // If CDN URLs from saved meta.json have expired (common for TikTok time-limited URLs),
        // fall back to re-fetching fresh metadata via yt-dlp for new CDN URLs.
        if filenames.is_empty() && has_english {
            warn!(
                archive_id,
                original_url,
                "TikTok subtitle CDN URLs appear expired; re-fetching fresh metadata via yt-dlp"
            );
            use crate::archiver::{ytdlp, CookieOptions};
            let empty_cookies = CookieOptions::default();
            match ytdlp::get_tiktok_metadata(original_url, &empty_cookies).await {
                Ok(fresh_meta) => {
                    let fresh_subtitles = extract_subtitle_info(&fresh_meta.json);
                    let fresh_has_english = fresh_subtitles
                        .iter()
                        .any(|s| s.language_code.starts_with("eng-"));
                    debug!(
                        archive_id,
                        subtitle_count = fresh_subtitles.len(),
                        fresh_has_english,
                        "Re-fetched TikTok metadata for subtitle retry"
                    );
                    if fresh_has_english {
                        match download_tiktok_subtitles(&fresh_subtitles, &work_dir, true).await {
                            Ok(fresh_files) if !fresh_files.is_empty() => {
                                let count = fresh_files.len();
                                let s3_prefix =
                                    meta_s3_key.trim_end_matches("meta.json").to_string();
                                process_subtitle_files(
                                    db,
                                    s3,
                                    archive_id,
                                    &fresh_files,
                                    &work_dir,
                                    &s3_prefix,
                                )
                                .await;
                                let _ = std::fs::remove_dir_all(&work_dir);
                                return Ok((count, true, ""));
                            }
                            Ok(_) => {
                                warn!(
                                    archive_id,
                                    "Fresh TikTok subtitle download also failed (CDN blocked or subtitles removed)"
                                );
                            }
                            Err(e) => {
                                warn!(archive_id, error = %e, "Error downloading fresh TikTok subtitles");
                            }
                        }
                    } else {
                        debug!(archive_id, "Fresh TikTok metadata has no English subtitles");
                    }
                }
                Err(e) => {
                    warn!(
                        archive_id,
                        error = %e,
                        "Failed to re-fetch TikTok metadata for subtitle retry"
                    );
                }
            }
            let _ = std::fs::remove_dir_all(&work_dir);
            return Ok((0, true, "subtitle CDN URLs expired and re-fetch failed"));
        }

        if filenames.is_empty() {
            debug!(
                archive_id,
                has_english, "No English subtitles downloaded (subtitles exist in other languages)"
            );
            let _ = std::fs::remove_dir_all(&work_dir);
            return Ok((0, true, "no English subtitles available")); // Mark as attempted - no English subs
        }

        let count = filenames.len();

        // Process and upload subtitle files
        // Derive s3_prefix from meta_s3_key (strip "meta.json" suffix)
        let s3_prefix = meta_s3_key.trim_end_matches("meta.json").to_string();
        process_subtitle_files(db, s3, archive_id, &filenames, &work_dir, &s3_prefix).await;

        // Clean up
        let _ = std::fs::remove_dir_all(&work_dir);

        Ok((count, true, "")) // Mark as attempted and we got subtitles!
    }
    .await;

    match &result {
        Ok((count, _, _)) if *count > 0 => {
            let meta = format!("{{\"subtitle_count\":{}}}", count);
            complete_backfill_job(db, job_id, Some(&meta)).await;
        }
        Ok((_, _, reason)) => {
            skip_backfill_job(db, job_id, reason).await;
        }
        Err(e) => fail_backfill_job(db, job_id, &e.to_string()).await,
    }

    result.map(|(count, should_mark, _)| (count, should_mark))
}

// ========== YouTube VTT Dedup Backfill ==========

/// Configuration for the YouTube VTT dedup backfill worker.
pub struct YouTubeVttDedupConfig {
    pub batch_size: i64,
    pub item_delay: Duration,
    pub batch_delay: Duration,
}

impl Default for YouTubeVttDedupConfig {
    fn default() -> Self {
        Self {
            batch_size: 50,
            item_delay: Duration::from_millis(100),
            batch_delay: Duration::from_secs(1),
        }
    }
}

/// Run the YouTube VTT dedup backfill worker.
///
/// Re-parses YouTube VTT files with the deduplication logic to fix triplicated
/// transcripts caused by rolling subtitles. Runs once at startup then exits.
pub async fn run_youtube_vtt_dedup_backfill(
    db: Database,
    s3: S3Client,
    config: YouTubeVttDedupConfig,
) {
    info!("Starting YouTube VTT dedup backfill worker");

    let mut total_processed: u64 = 0;
    let mut total_updated: u64 = 0;

    loop {
        let batch = match get_youtube_archives_with_vtt_subtitles(db.pool(), config.batch_size)
            .await
        {
            Ok(batch) => batch,
            Err(e) => {
                warn!(error = %e, "Failed to get YouTube archives for VTT dedup, retrying in 1 minute");
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }
        };

        if batch.is_empty() {
            info!(
                total_processed,
                total_updated, "YouTube VTT dedup backfill complete - no more archives to process"
            );
            break;
        }

        let batch_len = batch.len();
        let mut batch_updated = 0u64;

        for (archive_id, vtt_s3_key, transcript_s3_key) in &batch {
            match backfill_youtube_vtt_dedup(&db, &s3, *archive_id, vtt_s3_key, transcript_s3_key)
                .await
            {
                Ok(true) => {
                    batch_updated += 1;
                    total_updated += 1;
                }
                Ok(false) => {
                    debug!(archive_id, "VTT dedup: no significant change, marking done");
                }
                Err(e) => {
                    warn!(archive_id, vtt_s3_key, error = %e, "Failed to dedup YouTube VTT");
                }
            }

            // Mark as processed regardless of outcome
            if let Err(e) = insert_artifact(
                db.pool(),
                *archive_id,
                "vtt_dedup_done",
                "none",
                None,
                None,
                None,
            )
            .await
            {
                warn!(archive_id, error = %e, "Failed to insert vtt_dedup_done marker");
            }

            total_processed += 1;
            tokio::time::sleep(config.item_delay).await;
        }

        info!(
            batch_size = batch_len,
            batch_updated, total_processed, total_updated, "Processed YouTube VTT dedup batch"
        );

        tokio::time::sleep(config.batch_delay).await;
    }
}

// ========== Engagement Metrics Backfill ==========

/// Configuration for the metrics backfill worker.
pub struct MetricsBackfillConfig {
    /// Delay between processing each archive.
    pub item_delay: Duration,
    /// Optional domain filter (e.g., "tiktok.com" to only backfill TikTok).
    pub domain_filter: Option<String>,
}

impl Default for MetricsBackfillConfig {
    fn default() -> Self {
        Self {
            item_delay: Duration::from_millis(50),
            domain_filter: None,
        }
    }
}

/// Run the engagement metrics backfill worker.
///
/// Fetches meta.json from S3 for archives missing engagement metrics,
/// extracts view/like/repost/comment/save counts, and stores them in the DB.
///
/// Uses `METRICS_BACKFILL_VERSION` to track which version of the backfill
/// has been applied. Bump the version constant to force a re-scan.
pub async fn run_metrics_backfill(db: Database, s3: S3Client, config: MetricsBackfillConfig) {
    info!(
        version = METRICS_BACKFILL_VERSION,
        domain_filter = ?config.domain_filter,
        "Starting engagement metrics backfill"
    );

    let archives = match get_archives_needing_metrics_backfill(
        db.pool(),
        METRICS_BACKFILL_VERSION,
        config.domain_filter.as_deref(),
    )
    .await
    {
        Ok(a) => a,
        Err(e) => {
            warn!(error = %e, "Failed to query archives needing metrics backfill");
            return;
        }
    };

    if archives.is_empty() {
        info!("Engagement metrics backfill: nothing to do");
        return;
    }

    let total = archives.len();
    let mut success = 0u64;
    let mut with_metrics = 0u64;

    for (archive_id, meta_s3_key) in &archives {
        match backfill_single_metrics(&db, &s3, *archive_id, meta_s3_key).await {
            Ok(true) => {
                success += 1;
                with_metrics += 1;
            }
            Ok(false) => {
                success += 1;
            }
            Err(e) => {
                warn!(archive_id, error = %e, "Failed to backfill metrics");
            }
        }

        tokio::time::sleep(config.item_delay).await;
    }

    info!(
        total,
        success,
        with_metrics,
        version = METRICS_BACKFILL_VERSION,
        "Engagement metrics backfill complete"
    );
}

/// Backfill metrics for a single archive. Returns true if metrics were found.
async fn backfill_single_metrics(
    db: &Database,
    s3: &S3Client,
    archive_id: i64,
    meta_s3_key: &str,
) -> Result<bool> {
    let response = s3.get_object(meta_s3_key).await?;
    let Some((bytes, _)) = response else {
        // No meta.json, mark as backfilled with empty metrics
        set_archive_engagement_metrics(
            db.pool(),
            archive_id,
            &EngagementMetrics::default(),
            Some(METRICS_BACKFILL_VERSION),
        )
        .await?;
        return Ok(false);
    };

    let json_str = String::from_utf8(bytes).unwrap_or_default();
    let metrics = EngagementMetrics::from_metadata_json(&json_str);
    let has_metrics = metrics.has_any();

    set_archive_engagement_metrics(
        db.pool(),
        archive_id,
        &metrics,
        Some(METRICS_BACKFILL_VERSION),
    )
    .await?;

    if has_metrics {
        debug!(
            archive_id,
            view_count = ?metrics.view_count,
            like_count = ?metrics.like_count,
            "Backfilled engagement metrics"
        );
    }

    Ok(has_metrics)
}

/// Dedup a single YouTube archive's VTT transcript.
///
/// Returns `Ok(true)` if the transcript was updated (was triplicated),
/// `Ok(false)` if no significant change.
async fn backfill_youtube_vtt_dedup(
    db: &Database,
    s3: &S3Client,
    archive_id: i64,
    vtt_s3_key: &str,
    transcript_s3_key: &str,
) -> Result<bool> {
    use crate::archiver::transcript::{generate_transcript, parse_vtt_content};

    let job_id = start_backfill_job(db, archive_id).await;

    let result: Result<bool> = async {
        // Fetch VTT from S3
        let response = s3.get_object(vtt_s3_key).await?;
        let Some((bytes, _)) = response else {
            debug!(archive_id, vtt_s3_key, "VTT file not found in S3");
            return Ok(false);
        };

        let vtt_content = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(e) => {
                warn!(archive_id, vtt_s3_key, error = %e, "VTT is not valid UTF-8");
                return Ok(false);
            }
        };

        // Parse with dedup logic
        let cues = parse_vtt_content(&vtt_content);
        let new_transcript = generate_transcript(&cues);

        if new_transcript.trim().is_empty() {
            return Ok(false);
        }

        // Get existing transcript length from DB for comparison
        let existing_len = sqlx::query_scalar::<_, i64>(
            "SELECT LENGTH(transcript_text) FROM archives WHERE id = ?",
        )
        .bind(archive_id)
        .fetch_optional(db.pool())
        .await?
        .unwrap_or(0);

        let new_len = new_transcript.len() as i64;

        // Only update if new transcript is significantly shorter (< 60% of old = was triplicated)
        if existing_len > 0 && new_len < (existing_len * 60 / 100) {
            // Upload new transcript to S3
            s3.upload_bytes(new_transcript.as_bytes(), transcript_s3_key, "text/plain")
                .await?;

            // Update DB
            set_archive_transcript_text(db.pool(), archive_id, &new_transcript).await?;

            info!(
                archive_id,
                old_len = existing_len,
                new_len,
                "Deduped YouTube transcript"
            );
            return Ok(true);
        }

        Ok(false)
    }
    .await;

    match &result {
        Ok(true) => complete_backfill_job(db, job_id, None).await,
        Ok(false) => skip_backfill_job(db, job_id, "No significant change after dedup").await,
        Err(e) => fail_backfill_job(db, job_id, &e.to_string()).await,
    }

    result
}
