//! Database backup module.
//!
//! Provides functionality to backup the `SQLite` database, compress it with zstd,
//! and upload to S3 with retention policies.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use tokio::fs;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::s3::S3Client;

/// Backup manager handles database backups to S3.
#[derive(Clone)]
pub struct BackupManager {
    db_path: PathBuf,
    work_dir: PathBuf,
    s3_client: S3Client,
    s3_prefix: String,
    retention_count: usize,
}

impl BackupManager {
    /// Create a new backup manager.
    #[must_use]
    pub fn new(config: &Config, s3_client: S3Client) -> Self {
        Self {
            db_path: config.database_path.clone(),
            work_dir: config.work_dir.clone(),
            s3_client,
            s3_prefix: format!("{}backups/", config.s3_prefix),
            retention_count: config.backup_retention_count,
        }
    }

    /// Run the backup loop at the configured interval.
    pub async fn run_loop(&self, interval: Duration) {
        info!(
            interval_hours = interval.as_secs() / 3600,
            "Starting backup scheduler"
        );

        loop {
            // Wait for the interval
            tokio::time::sleep(interval).await;

            // Run backup
            match self.run_backup().await {
                Ok(key) => info!(s3_key = %key, "Database backup completed successfully"),
                Err(e) => error!("Database backup failed: {e:#}"),
            }
        }
    }

    /// Perform a database backup: VACUUM INTO, compress, upload, cleanup.
    ///
    /// # Returns
    ///
    /// Returns the S3 key of the uploaded backup on success.
    ///
    /// # Errors
    ///
    /// Returns an error if any step fails.
    pub async fn run_backup(&self) -> Result<String> {
        let timestamp = Utc::now();
        let backup_name = format!(
            "archive-backup-{}.sqlite.zst",
            timestamp.format("%Y%m%d-%H%M%S")
        );

        // Create temp directory for this backup
        let backup_dir = self.work_dir.join("backup");
        fs::create_dir_all(&backup_dir)
            .await
            .context("Failed to create backup directory")?;

        let raw_backup_path = backup_dir.join(format!("backup-{}.sqlite", timestamp.timestamp()));
        let compressed_path = backup_dir.join(&backup_name);

        info!(db_path = ?self.db_path, "Starting database backup");

        // Step 1: VACUUM INTO to create a consistent backup
        self.vacuum_into(&raw_backup_path)
            .await
            .context("VACUUM INTO failed")?;

        // Step 2: Compress with zstd
        self.compress_zstd(&raw_backup_path, &compressed_path)
            .await
            .context("Compression failed")?;

        // Clean up raw backup
        if let Err(e) = fs::remove_file(&raw_backup_path).await {
            warn!(path = ?raw_backup_path, "Failed to remove raw backup file: {e}");
        }

        // Step 3: Upload to S3
        let s3_key = format!("{}{}", self.s3_prefix, backup_name);
        self.s3_client
            .upload_file(&compressed_path, &s3_key, None)
            .await
            .context("S3 upload failed")?;

        // Clean up compressed file
        if let Err(e) = fs::remove_file(&compressed_path).await {
            warn!(path = ?compressed_path, "Failed to remove compressed backup file: {e}");
        }

        // Step 4: Apply retention policy
        if let Err(e) = self.apply_retention().await {
            warn!("Failed to apply backup retention: {e}");
        }

        Ok(s3_key)
    }

    /// Use VACUUM INTO to create a consistent backup of the database.
    async fn vacuum_into(&self, output_path: &Path) -> Result<()> {
        let db_path_str = self.db_path.to_string_lossy().to_string();
        let output_path_str = output_path.to_string_lossy().to_string();

        debug!(db = %db_path_str, output = %output_path_str, "Running VACUUM INTO");

        // We need to run VACUUM INTO via a separate SQLite connection
        // to avoid locking the main database for too long
        let conn = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{db_path_str}?mode=ro"))
            .await
            .context("Failed to open database for backup")?;

        // VACUUM INTO creates a complete copy of the database
        let query = format!("VACUUM INTO '{}'", output_path_str.replace('\'', "''"));
        sqlx::query(&query)
            .execute(&conn)
            .await
            .context("VACUUM INTO query failed")?;

        conn.close().await;

        let metadata = fs::metadata(output_path).await?;
        #[allow(clippy::cast_precision_loss)]
        let size_mb = metadata.len() as f64 / 1_048_576.0;
        info!(size_mb, "VACUUM INTO completed");

        Ok(())
    }

    /// Compress a file using zstd.
    async fn compress_zstd(&self, input_path: &Path, output_path: &Path) -> Result<()> {
        let input_path_owned = input_path.to_path_buf();
        let output_path_owned = output_path.to_path_buf();

        // Run compression in a blocking task since zstd is CPU-bound
        tokio::task::spawn_blocking(move || {
            use std::fs::File;
            use std::io::{BufReader, BufWriter, Write};

            let input_file = File::open(&input_path_owned)
                .context("Failed to open input file for compression")?;
            let input_reader = BufReader::new(input_file);

            let output_file = File::create(&output_path_owned)
                .context("Failed to create compressed output file")?;
            let output_writer = BufWriter::new(output_file);

            // Use compression level 3 for good balance of speed and ratio
            let mut encoder =
                zstd::stream::Encoder::new(output_writer, 3).context("Failed to create encoder")?;

            std::io::copy(&mut BufReader::new(input_reader.get_ref()), &mut encoder)
                .context("Failed to compress data")?;

            encoder.finish()?.flush()?;

            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("Compression task panicked")??;

        let input_size = fs::metadata(input_path).await?.len();
        let output_size = fs::metadata(output_path).await?.len();

        #[allow(clippy::cast_precision_loss)]
        let (input_mb, output_mb, ratio_pct) = {
            let input_mb = input_size as f64 / 1_048_576.0;
            let output_mb = output_size as f64 / 1_048_576.0;
            let ratio = if input_size > 0 {
                (output_size as f64 / input_size as f64) * 100.0
            } else {
                100.0
            };
            (input_mb, output_mb, ratio)
        };

        info!(input_mb, output_mb, ratio_pct, "Compression completed");

        Ok(())
    }

    /// Delete old backups beyond the retention count.
    async fn apply_retention(&self) -> Result<()> {
        if self.retention_count == 0 {
            debug!("Backup retention disabled (count=0)");
            return Ok(());
        }

        info!(
            retention_count = self.retention_count,
            prefix = %self.s3_prefix,
            "Applying backup retention policy"
        );

        // List all backup files in S3
        let backups = self.list_backups().await?;

        if backups.len() <= self.retention_count {
            debug!(backup_count = backups.len(), "No backups to delete");
            return Ok(());
        }

        // Sort by timestamp (newest first) and delete older ones
        let mut sorted_backups = backups;
        sorted_backups.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        let to_delete = &sorted_backups[self.retention_count..];
        info!(count = to_delete.len(), "Deleting old backups");

        for backup in to_delete {
            debug!(key = %backup.key, "Deleting old backup");
            if let Err(e) = self.delete_backup(&backup.key).await {
                warn!(key = %backup.key, "Failed to delete backup: {e}");
            }
        }

        Ok(())
    }

    /// List all backups in S3.
    async fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        let response = self
            .s3_client
            .list_objects(&self.s3_prefix)
            .await
            .context("Failed to list backups")?;

        let mut backups = Vec::new();

        for key in response {
            if let Some(info) = parse_backup_key(&key) {
                backups.push(info);
            }
        }

        Ok(backups)
    }

    /// Delete a backup from S3.
    async fn delete_backup(&self, key: &str) -> Result<()> {
        self.s3_client
            .delete_object(key)
            .await
            .context("Failed to delete backup")
    }
}

/// Information about a backup file.
#[derive(Debug)]
struct BackupInfo {
    key: String,
    timestamp: DateTime<Utc>,
}

/// Parse backup key to extract timestamp.
fn parse_backup_key(key: &str) -> Option<BackupInfo> {
    // Expected format: prefix/archive-backup-YYYYMMDD-HHMMSS.sqlite.zst
    let filename = key.rsplit('/').next()?;
    if !filename.starts_with("archive-backup-") || !filename.ends_with(".sqlite.zst") {
        return None;
    }

    let timestamp_str = filename
        .strip_prefix("archive-backup-")?
        .strip_suffix(".sqlite.zst")?;

    // Parse YYYYMMDD-HHMMSS
    let timestamp = chrono::NaiveDateTime::parse_from_str(timestamp_str, "%Y%m%d-%H%M%S").ok()?;
    let timestamp = timestamp.and_utc();

    Some(BackupInfo {
        key: key.to_string(),
        timestamp,
    })
}

impl std::fmt::Debug for BackupManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackupManager")
            .field("db_path", &self.db_path)
            .field("s3_prefix", &self.s3_prefix)
            .field("retention_count", &self.retention_count)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn test_parse_backup_key() {
        let key = "archives/backups/archive-backup-20240115-143022.sqlite.zst";
        let info = parse_backup_key(key).unwrap();
        assert_eq!(info.key, key);
        assert_eq!(info.timestamp.year(), 2024);
        assert_eq!(info.timestamp.month(), 1);
        assert_eq!(info.timestamp.day(), 15);
        assert_eq!(info.timestamp.hour(), 14);
        assert_eq!(info.timestamp.minute(), 30);
        assert_eq!(info.timestamp.second(), 22);
    }

    #[test]
    fn test_parse_backup_key_invalid() {
        assert!(parse_backup_key("random-file.txt").is_none());
        assert!(parse_backup_key("archive-backup-invalid.sqlite.zst").is_none());
        assert!(parse_backup_key("backup-20240115.sqlite.zst").is_none());
    }
}
