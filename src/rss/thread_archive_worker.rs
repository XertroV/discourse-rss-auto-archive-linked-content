//! Background worker for processing thread archive jobs.
//!
//! This module runs a loop that polls for pending thread archive jobs
//! and processes them one at a time.

use std::time::Duration;

use tracing::{trace, error, info};

use crate::config::Config;
use crate::db::{
    get_pending_thread_archive_jobs, set_thread_archive_job_complete,
    set_thread_archive_job_failed, Database,
};

use super::thread_archiver::archive_thread_links;

/// Run the thread archive worker loop.
///
/// This function runs forever, polling for pending thread archive jobs
/// and processing them one at a time. It should be spawned as a background
/// task.
pub async fn run(config: Config, db: Database) {
    info!("Thread archive worker started");

    loop {
        // Check for pending jobs
        match get_pending_thread_archive_jobs(db.pool(), 1).await {
            Ok(jobs) if !jobs.is_empty() => {
                let job = &jobs[0];
                info!(
                    job_id = job.id,
                    thread_url = %job.thread_url,
                    user_id = job.user_id,
                    "Processing thread archive job"
                );

                // Process the thread
                match archive_thread_links(&config, &db, job).await {
                    Ok(progress) => {
                        info!(
                            job_id = job.id,
                            posts = progress.processed_posts,
                            links = progress.new_links_found,
                            archives = progress.archives_created,
                            skipped = progress.skipped_links,
                            "Thread archive job completed successfully"
                        );
                        if let Err(e) = set_thread_archive_job_complete(db.pool(), job.id).await {
                            error!(job_id = job.id, "Failed to mark job complete: {e}");
                        }
                    }
                    Err(e) => {
                        error!(job_id = job.id, error = %e, "Thread archive job failed");
                        let error_msg = format!("{e:#}");
                        if let Err(e) =
                            set_thread_archive_job_failed(db.pool(), job.id, &error_msg).await
                        {
                            error!(job_id = job.id, "Failed to mark job failed: {e}");
                        }
                    }
                }
            }
            Ok(_) => {
                // No pending jobs, just wait
                trace!("No pending thread archive jobs");
            }
            Err(e) => {
                error!("Failed to fetch pending thread archive jobs: {e}");
            }
        }

        // Wait before checking again
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}
