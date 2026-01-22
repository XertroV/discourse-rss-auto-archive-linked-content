//! Background worker to backfill search content from S3.
//!
//! This module provides a background task that populates the `transcript_text`
//! column for existing archives that have transcript artifacts stored on S3
//! but don't have the text indexed in the database yet.

use std::time::Duration;

use anyhow::Result;
use tracing::{debug, info, warn};

use crate::db::{get_archives_needing_transcript_backfill, set_archive_transcript_text, Database};
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
