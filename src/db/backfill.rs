//! Background workers to backfill content from S3.
//!
//! This module provides background tasks for:
//! - Populating `transcript_text` for existing archives with transcripts on S3
//! - Downloading TikTok subtitles from meta.json for archives without subtitles

use std::time::Duration;

use anyhow::Result;
use tracing::{debug, info, warn};

use crate::db::{
    get_archives_needing_transcript_backfill, get_tiktok_archives_needing_subtitle_backfill,
    insert_artifact, set_archive_transcript_text, Database,
};
use crate::s3::S3Client;

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

        for (archive_id, meta_s3_key) in batch {
            let (count, should_mark) =
                match backfill_tiktok_subtitles(&db, &s3, archive_id, &meta_s3_key).await {
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

            // Insert marker artifact to prevent re-processing this archive
            if should_mark && count == 0 {
                if let Err(e) = insert_artifact(
                    db.pool(),
                    archive_id,
                    "subtitle_backfill_attempted",
                    "none", // No actual S3 key
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
) -> Result<(usize, bool)> {
    // Returns (count, should_mark_attempted)
    use crate::archiver::process_subtitle_files;
    use crate::handlers::tiktok::{download_tiktok_subtitles, extract_subtitle_info};

    // Fetch meta.json from S3
    let response = s3.get_object(meta_s3_key).await?;

    let Some((bytes, _content_type)) = response else {
        debug!(archive_id, meta_s3_key, "meta.json not found in S3");
        return Ok((0, true)); // Mark as attempted - no meta.json
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
            return Ok((0, true)); // Mark as attempted - invalid JSON
        }
    };

    // Extract subtitle info
    let subtitles = extract_subtitle_info(&meta_json);
    if subtitles.is_empty() {
        debug!(
            archive_id,
            "No subtitles field in meta.json or subtitles object is empty"
        );
        return Ok((0, true)); // Mark as attempted - no subtitles in JSON
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
    let work_dir = std::env::temp_dir().join(format!("tiktok-subtitle-backfill-{}", archive_id));
    std::fs::create_dir_all(&work_dir)?;

    // Download subtitles (English only for now)
    let filenames = match download_tiktok_subtitles(&subtitles, &work_dir, true).await {
        Ok(files) => files,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&work_dir);
            return Err(e);
        }
    };

    if filenames.is_empty() {
        debug!(
            archive_id,
            has_english, "No English subtitles downloaded (subtitles exist in other languages)"
        );
        let _ = std::fs::remove_dir_all(&work_dir);
        return Ok((0, true)); // Mark as attempted - no English subtitles
    }

    let count = filenames.len();

    // Process and upload subtitle files
    // Derive s3_prefix from meta_s3_key (strip "meta.json" suffix)
    let s3_prefix = meta_s3_key.trim_end_matches("meta.json").to_string();
    process_subtitle_files(db, s3, archive_id, &filenames, &work_dir, &s3_prefix).await;

    // Clean up
    let _ = std::fs::remove_dir_all(&work_dir);

    Ok((count, true)) // Mark as attempted and we got subtitles!
}
