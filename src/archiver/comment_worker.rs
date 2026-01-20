//! Background worker for extracting comments from archived videos and social media.
//!
//! This module runs a loop that polls for pending comment extraction jobs
//! and processes them one at a time, avoiding rate limits by serializing comment downloads.

use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{debug, error, info, warn};

use crate::archiver::{ytdlp, CookieOptions};
use crate::config::Config;
use crate::db::{
    get_archive, get_pending_comment_extraction_jobs, set_job_completed, set_job_failed,
    set_job_running, Database,
};
use crate::s3::S3Client;

/// Run the comment extraction worker loop.
///
/// This function runs forever, polling for pending comment extraction jobs
/// and processing them one at a time. It should be spawned as a background
/// task.
///
/// Only one comment extraction runs at a time to avoid rate limiting from platforms like YouTube.
pub async fn run(config: Config, db: Database, s3: S3Client) {
    info!("Comment extraction worker started");

    loop {
        // Check for pending jobs
        match get_pending_comment_extraction_jobs(db.pool(), 1).await {
            Ok(jobs) if !jobs.is_empty() => {
                let job = &jobs[0];
                info!(
                    job_id = job.id,
                    archive_id = job.archive_id,
                    "Processing comment extraction job"
                );

                // Get the archive details
                let archive = match get_archive(db.pool(), job.archive_id).await {
                    Ok(Some(archive)) => archive,
                    Ok(None) => {
                        error!(job_id = job.id, "Archive not found, skipping job");
                        if let Err(e) = set_job_failed(db.pool(), job.id, "Archive not found").await
                        {
                            error!(job_id = job.id, "Failed to mark job failed: {e}");
                        }
                        continue;
                    }
                    Err(e) => {
                        error!(job_id = job.id, error = %e, "Failed to fetch archive");
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        continue;
                    }
                };

                // Mark job as running
                if let Err(e) = set_job_running(db.pool(), job.id).await {
                    error!(job_id = job.id, "Failed to mark job running: {e}");
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    continue;
                }

                // Extract comments
                match extract_comments_for_archive(&config, &db, &s3, &archive).await {
                    Ok(comment_count) => {
                        info!(
                            job_id = job.id,
                            archive_id = archive.id,
                            comments = comment_count,
                            "Comment extraction completed successfully"
                        );
                        let metadata = serde_json::json!({
                            "comment_count": comment_count,
                            "platform": archive.content_type.as_deref().unwrap_or("unknown"),
                        });
                        if let Err(e) =
                            set_job_completed(db.pool(), job.id, Some(&metadata.to_string())).await
                        {
                            error!(job_id = job.id, "Failed to mark job complete: {e}");
                        }
                    }
                    Err(e) => {
                        error!(job_id = job.id, error = %e, "Comment extraction failed");
                        let error_msg = format!("{e:#}");
                        if let Err(e) = set_job_failed(db.pool(), job.id, &error_msg).await {
                            error!(job_id = job.id, "Failed to mark job failed: {e}");
                        }
                    }
                }
            }
            Ok(_) => {
                // No pending jobs, just wait
                debug!("No pending comment extraction jobs");
            }
            Err(e) => {
                error!("Failed to fetch pending comment extraction jobs: {e}");
            }
        }

        // Wait before checking again
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

/// Extract comments for a single archive.
async fn extract_comments_for_archive(
    config: &Config,
    db: &Database,
    s3: &S3Client,
    archive: &crate::db::Archive,
) -> Result<usize> {
    // Get the link for this archive
    let link = crate::db::get_link(db.pool(), archive.link_id)
        .await
        .context("Failed to fetch link")?
        .ok_or_else(|| anyhow::anyhow!("Link not found for archive"))?;

    let url = link.final_url.as_deref().unwrap_or(&link.normalized_url);

    info!(archive_id = archive.id, url = %url, "Extracting comments");

    // Create a temporary work directory for this extraction
    let work_dir = config.work_dir.join(format!("comments_{}", archive.id));
    tokio::fs::create_dir_all(&work_dir)
        .await
        .with_context(|| format!("Failed to create work directory: {}", work_dir.display()))?;

    // Prepare cookies
    let cookies = CookieOptions {
        browser_profile: config.yt_dlp_cookies_from_browser.as_deref(),
        cookies_file: config.cookies_file_path.as_deref(),
    };

    // Extract comments using yt-dlp
    let comment_count = ytdlp::extract_comments_only(
        url,
        &work_dir,
        &cookies,
        config,
        Some(archive.id),
        Some(db.pool()),
    )
    .await
    .context("Failed to extract comments with yt-dlp")?;

    // Upload comments.json to S3 if it exists
    let comments_json_path = work_dir.join("comments.json");
    if comments_json_path.exists() {
        let s3_key = format!("{}comments.json", archive.id);
        s3.upload_file(&comments_json_path, &s3_key, Some(archive.id))
            .await
            .context("Failed to upload comments.json to S3")?;

        // Get file size for artifact record
        let file_size = tokio::fs::metadata(&comments_json_path)
            .await
            .ok()
            .map(|m| m.len() as i64);

        // Insert artifact record
        crate::db::insert_artifact(
            db.pool(),
            archive.id,
            "comments",
            &s3_key,
            Some("application/json"),
            file_size,
            None,
        )
        .await
        .context("Failed to insert comments artifact")?;

        info!(archive_id = archive.id, s3_key = %s3_key, "Comments uploaded to S3");
    }

    // Clean up work directory
    if let Err(e) = tokio::fs::remove_dir_all(&work_dir).await {
        warn!(work_dir = %work_dir.display(), error = %e, "Failed to clean up work directory");
    }

    Ok(comment_count)
}
