//! Background cleanup worker for expired sessions and old audit events.

use sqlx::SqlitePool;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Cleanup configuration.
pub struct CleanupConfig {
    /// Interval between cleanup runs.
    pub interval: Duration,
    /// Number of days to keep audit events.
    pub audit_retention_days: i64,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(3600), // 1 hour
            audit_retention_days: 90,
        }
    }
}

/// Run a single cleanup cycle.
async fn cleanup_once(pool: &SqlitePool, audit_retention_days: i64) {
    // Delete expired sessions
    match crate::db::delete_expired_sessions(pool).await {
        Ok(count) => {
            if count > 0 {
                tracing::info!(expired_sessions = count, "Cleaned up expired sessions");
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete expired sessions: {e}");
        }
    }

    // Delete old audit events
    match crate::db::delete_old_audit_events(pool, audit_retention_days).await {
        Ok(count) => {
            if count > 0 {
                tracing::info!(
                    old_audit_events = count,
                    retention_days = audit_retention_days,
                    "Cleaned up old audit events"
                );
            }
        }
        Err(e) => {
            tracing::error!("Failed to delete old audit events: {e}");
        }
    }
}

/// Run the cleanup worker.
/// This task runs cleanup immediately on start, then at the configured interval.
/// It respects the cancellation token for graceful shutdown.
pub async fn run_cleanup_worker(
    pool: SqlitePool,
    config: CleanupConfig,
    shutdown: CancellationToken,
) {
    tracing::info!(
        interval_secs = config.interval.as_secs(),
        audit_retention_days = config.audit_retention_days,
        "Starting cleanup worker"
    );

    // Run immediately on startup
    cleanup_once(&pool, config.audit_retention_days).await;

    let mut interval = tokio::time::interval(config.interval);
    interval.tick().await; // Skip the first immediate tick (we already ran cleanup)

    loop {
        tokio::select! {
            _ = interval.tick() => {
                cleanup_once(&pool, config.audit_retention_days).await;
            }
            _ = shutdown.cancelled() => {
                tracing::info!("Cleanup worker shutting down");
                break;
            }
        }
    }
}
