mod migrations;
mod models;
mod queries;

pub use models::*;
pub use queries::*;

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use tracing::info;

#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create a new database connection, running migrations if needed.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection fails or migrations fail.
    pub async fn new(path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .foreign_keys(true)
            // Without a busy timeout, concurrent writers can cause immediate SQLITE_BUSY
            // errors (e.g. when resetting/deleting archives from the web UI while the
            // worker is writing). WAL helps, but writes are still serialized.
            .busy_timeout(Duration::from_secs(10));

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .context("Failed to connect to SQLite database")?;

        let db = Self { pool };
        db.run_migrations().await?;

        Ok(db)
    }

    /// Run all pending migrations.
    async fn run_migrations(&self) -> Result<()> {
        migrations::run(&self.pool).await?;
        info!("Database migrations complete");
        Ok(())
    }

    /// Get a reference to the connection pool.
    #[must_use]
    pub const fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
