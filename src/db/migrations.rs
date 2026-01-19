use anyhow::{Context, Result};
use sqlx::SqlitePool;
use tracing::debug;

/// Schema version for tracking migrations.
const SCHEMA_VERSION: i32 = 1;

/// Run all pending migrations.
pub async fn run(pool: &SqlitePool) -> Result<()> {
    create_migration_table(pool).await?;
    let current_version = get_schema_version(pool).await?;

    if current_version < SCHEMA_VERSION {
        debug!(
            current = current_version,
            target = SCHEMA_VERSION,
            "Running migrations"
        );
        run_migration_v1(pool).await?;
        set_schema_version(pool, SCHEMA_VERSION).await?;
    }

    Ok(())
}

async fn create_migration_table(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS _schema_version (
            version INTEGER PRIMARY KEY
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create schema version table")?;

    Ok(())
}

async fn get_schema_version(pool: &SqlitePool) -> Result<i32> {
    let row: Option<(i32,)> = sqlx::query_as("SELECT version FROM _schema_version LIMIT 1")
        .fetch_optional(pool)
        .await
        .context("Failed to get schema version")?;

    Ok(row.map_or(0, |(v,)| v))
}

async fn set_schema_version(pool: &SqlitePool, version: i32) -> Result<()> {
    sqlx::query("DELETE FROM _schema_version")
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO _schema_version (version) VALUES (?)")
        .bind(version)
        .execute(pool)
        .await?;
    Ok(())
}

async fn run_migration_v1(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v1: creating initial schema");

    // Posts table
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS posts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            guid TEXT UNIQUE NOT NULL,
            discourse_url TEXT NOT NULL,
            author TEXT,
            title TEXT,
            body_html TEXT,
            content_hash TEXT,
            published_at TEXT,
            processed_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create posts table")?;

    // Links table
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS links (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            original_url TEXT NOT NULL,
            normalized_url TEXT NOT NULL,
            canonical_url TEXT,
            final_url TEXT,
            domain TEXT NOT NULL,
            first_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_archived_at TEXT
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create links table")?;

    // Link occurrences table
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS link_occurrences (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            link_id INTEGER NOT NULL REFERENCES links(id) ON DELETE CASCADE,
            post_id INTEGER NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
            in_quote INTEGER NOT NULL DEFAULT 0,
            context_snippet TEXT,
            seen_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create link_occurrences table")?;

    // Archives table
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS archives (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            link_id INTEGER NOT NULL REFERENCES links(id) ON DELETE CASCADE,
            status TEXT NOT NULL DEFAULT 'pending',
            archived_at TEXT,
            content_title TEXT,
            content_author TEXT,
            content_text TEXT,
            content_type TEXT,
            s3_key_primary TEXT,
            s3_key_thumb TEXT,
            s3_keys_extra TEXT,
            wayback_url TEXT,
            error_message TEXT,
            retry_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create archives table")?;

    // Archive artifacts table
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS archive_artifacts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            archive_id INTEGER NOT NULL REFERENCES archives(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            s3_key TEXT NOT NULL,
            content_type TEXT,
            size_bytes INTEGER,
            sha256 TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create archive_artifacts table")?;

    // Create FTS5 virtual table for full-text search
    sqlx::query(
        r"
        CREATE VIRTUAL TABLE IF NOT EXISTS archives_fts USING fts5(
            content_title,
            content_author,
            content_text,
            content='archives',
            content_rowid='id'
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create FTS5 table")?;

    // Triggers to keep FTS in sync with archives table
    sqlx::query(
        r"
        CREATE TRIGGER IF NOT EXISTS archives_fts_insert AFTER INSERT ON archives BEGIN
            INSERT INTO archives_fts(rowid, content_title, content_author, content_text)
            VALUES (new.id, new.content_title, new.content_author, new.content_text);
        END
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create FTS insert trigger")?;

    sqlx::query(
        r"
        CREATE TRIGGER IF NOT EXISTS archives_fts_delete AFTER DELETE ON archives BEGIN
            INSERT INTO archives_fts(archives_fts, rowid, content_title, content_author, content_text)
            VALUES ('delete', old.id, old.content_title, old.content_author, old.content_text);
        END
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create FTS delete trigger")?;

    sqlx::query(
        r"
        CREATE TRIGGER IF NOT EXISTS archives_fts_update AFTER UPDATE ON archives BEGIN
            INSERT INTO archives_fts(archives_fts, rowid, content_title, content_author, content_text)
            VALUES ('delete', old.id, old.content_title, old.content_author, old.content_text);
            INSERT INTO archives_fts(rowid, content_title, content_author, content_text)
            VALUES (new.id, new.content_title, new.content_author, new.content_text);
        END
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create FTS update trigger")?;

    // Indexes for common queries
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_posts_guid ON posts(guid)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_links_normalized_url ON links(normalized_url)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_links_domain ON links(domain)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_archives_status ON archives(status)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_archives_link_id ON archives(link_id)")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_link_occurrences_link_id ON link_occurrences(link_id)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_link_occurrences_post_id ON link_occurrences(post_id)",
    )
    .execute(pool)
    .await?;

    Ok(())
}
