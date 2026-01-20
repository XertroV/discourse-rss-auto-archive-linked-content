use anyhow::{Context, Result};
use sqlx::SqlitePool;
use tracing::debug;

/// Run all pending migrations.
pub async fn run(pool: &SqlitePool) -> Result<()> {
    create_migration_table(pool).await?;
    let current_version = get_schema_version(pool).await?;

    if current_version < 1 {
        debug!("Running migration v1");
        run_migration_v1(pool).await?;
        set_schema_version(pool, 1).await?;
    }

    if current_version < 2 {
        debug!("Running migration v2");
        run_migration_v2(pool).await?;
        set_schema_version(pool, 2).await?;
    }

    if current_version < 3 {
        debug!("Running migration v3");
        run_migration_v3(pool).await?;
        set_schema_version(pool, 3).await?;
    }

    if current_version < 4 {
        debug!("Running migration v4");
        run_migration_v4(pool).await?;
        set_schema_version(pool, 4).await?;
    }

    if current_version < 5 {
        debug!("Running migration v5");
        run_migration_v5(pool).await?;
        set_schema_version(pool, 5).await?;
    }

    if current_version < 6 {
        debug!("Running migration v6");
        run_migration_v6(pool).await?;
        set_schema_version(pool, 6).await?;
    }

    if current_version < 7 {
        debug!("Running migration v7");
        run_migration_v7(pool).await?;
        set_schema_version(pool, 7).await?;
    }

    if current_version < 8 {
        debug!("Running migration v8");
        run_migration_v8(pool).await?;
        set_schema_version(pool, 8).await?;
    }

    if current_version < 9 {
        debug!("Running migration v9");
        run_migration_v9(pool).await?;
        set_schema_version(pool, 9).await?;
    }

    if current_version < 10 {
        debug!("Running migration v10");
        run_migration_v10(pool).await?;
        set_schema_version(pool, 10).await?;
    }

    if current_version < 11 {
        debug!("Running migration v11");
        run_migration_v11(pool).await?;
        set_schema_version(pool, 11).await?;
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

async fn run_migration_v2(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v2: adding IPFS and submissions support");

    // Add ipfs_cid column to archives table
    sqlx::query("ALTER TABLE archives ADD COLUMN ipfs_cid TEXT")
        .execute(pool)
        .await
        .context("Failed to add ipfs_cid column")?;

    // Create submissions table for manual URL submissions
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS submissions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            url TEXT NOT NULL,
            normalized_url TEXT NOT NULL,
            submitted_by_ip TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            link_id INTEGER REFERENCES links(id),
            error_message TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            processed_at TEXT
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create submissions table")?;

    // Index for rate limiting by IP
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_submissions_ip_created ON submissions(submitted_by_ip, created_at)")
        .execute(pool)
        .await?;

    // Index for finding pending submissions
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_submissions_status ON submissions(status)")
        .execute(pool)
        .await?;

    Ok(())
}

async fn run_migration_v3(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v3: adding Archive.today URL column");

    // Add archive_today_url column to archives table
    sqlx::query("ALTER TABLE archives ADD COLUMN archive_today_url TEXT")
        .execute(pool)
        .await
        .context("Failed to add archive_today_url column")?;

    Ok(())
}

async fn run_migration_v4(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v4: adding NSFW content filtering columns");

    // Add is_nsfw column to archives table (default false/0)
    sqlx::query("ALTER TABLE archives ADD COLUMN is_nsfw INTEGER NOT NULL DEFAULT 0")
        .execute(pool)
        .await
        .context("Failed to add is_nsfw column")?;

    // Add nsfw_source column to track where the NSFW flag came from
    // Values: 'api', 'metadata', 'subreddit', 'manual', null
    sqlx::query("ALTER TABLE archives ADD COLUMN nsfw_source TEXT")
        .execute(pool)
        .await
        .context("Failed to add nsfw_source column")?;

    // Create index for filtering NSFW content
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_archives_is_nsfw ON archives(is_nsfw)")
        .execute(pool)
        .await
        .context("Failed to create is_nsfw index")?;

    Ok(())
}

async fn run_migration_v5(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v5: adding retry backoff columns");

    // Add next_retry_at column to archives table for exponential backoff
    // This stores when the archive is eligible to be retried
    sqlx::query("ALTER TABLE archives ADD COLUMN next_retry_at TEXT")
        .execute(pool)
        .await
        .context("Failed to add next_retry_at column")?;

    // Add last_attempt_at column to track when we last tried archiving
    sqlx::query("ALTER TABLE archives ADD COLUMN last_attempt_at TEXT")
        .execute(pool)
        .await
        .context("Failed to add last_attempt_at column")?;

    // Create index for efficient querying of retryable archives
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_archives_next_retry_at ON archives(next_retry_at)")
        .execute(pool)
        .await
        .context("Failed to create next_retry_at index")?;

    Ok(())
}

async fn run_migration_v6(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v6: adding perceptual hash for content deduplication");

    // Add perceptual_hash column to archive_artifacts table
    // This stores a perceptual hash (pHash) for image/video content
    sqlx::query("ALTER TABLE archive_artifacts ADD COLUMN perceptual_hash TEXT")
        .execute(pool)
        .await
        .context("Failed to add perceptual_hash column")?;

    // Add duplicate_of_artifact_id column to track duplicates
    // If set, this artifact is a duplicate of another artifact
    sqlx::query("ALTER TABLE archive_artifacts ADD COLUMN duplicate_of_artifact_id INTEGER REFERENCES archive_artifacts(id)")
        .execute(pool)
        .await
        .context("Failed to add duplicate_of_artifact_id column")?;

    // Create index for efficient duplicate lookup by hash
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_archive_artifacts_perceptual_hash ON archive_artifacts(perceptual_hash)",
    )
    .execute(pool)
    .await
    .context("Failed to create perceptual_hash index")?;

    Ok(())
}

async fn run_migration_v7(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v7: adding HTTP status code and archive jobs tracking");

    // Add http_status_code column to archives table
    // Stores the HTTP response status code (200, 404, 401, etc.)
    sqlx::query("ALTER TABLE archives ADD COLUMN http_status_code INTEGER")
        .execute(pool)
        .await
        .context("Failed to add http_status_code column")?;

    // Create archive_jobs table for tracking individual archiving steps
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS archive_jobs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            archive_id INTEGER NOT NULL REFERENCES archives(id) ON DELETE CASCADE,
            job_type TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            started_at TEXT,
            completed_at TEXT,
            error_message TEXT,
            metadata TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create archive_jobs table")?;

    // Index for querying jobs by archive
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_archive_jobs_archive_id ON archive_jobs(archive_id)",
    )
    .execute(pool)
    .await
    .context("Failed to create archive_jobs archive_id index")?;

    // Index for querying jobs by status
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_archive_jobs_status ON archive_jobs(status)")
        .execute(pool)
        .await
        .context("Failed to create archive_jobs status index")?;

    Ok(())
}

async fn run_migration_v8(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v8: adding exports table for bulk export tracking");

    // Create exports table for tracking bulk export downloads and rate limiting
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS exports (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            site_domain TEXT NOT NULL,
            exported_by_ip TEXT NOT NULL,
            archive_count INTEGER NOT NULL DEFAULT 0,
            total_size_bytes INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create exports table")?;

    // Index for rate limiting by IP
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_exports_ip_created ON exports(exported_by_ip, created_at)",
    )
    .execute(pool)
    .await
    .context("Failed to create exports IP index")?;

    // Index for site statistics
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_exports_site ON exports(site_domain)")
        .execute(pool)
        .await
        .context("Failed to create exports site index")?;

    Ok(())
}

async fn run_migration_v9(pool: &SqlitePool) -> Result<()> {
    debug!(
        "Running migration v9: adding post_date to archives for sorting by post publication date"
    );

    // Add post_date column to archives table
    // This stores when the Discourse post (containing the link) was published
    // Enables sorting archives by the date of the post, not the archive date
    sqlx::query("ALTER TABLE archives ADD COLUMN post_date TEXT")
        .execute(pool)
        .await
        .context("Failed to add post_date column")?;

    // Populate post_date from the first link occurrence's post published_at
    // This backfills existing archives with the post date
    sqlx::query(
        r"
        UPDATE archives
        SET post_date = (
            SELECT p.published_at
            FROM link_occurrences lo
            JOIN posts p ON lo.post_id = p.id
            WHERE lo.link_id = archives.link_id
            ORDER BY lo.seen_at ASC
            LIMIT 1
        )
        WHERE post_date IS NULL
        ",
    )
    .execute(pool)
    .await
    .context("Failed to backfill post_date")?;

    // Create index for efficient sorting by post date
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_archives_post_date ON archives(post_date DESC)")
        .execute(pool)
        .await
        .context("Failed to create post_date index")?;

    Ok(())
}

async fn run_migration_v10(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v10: adding video_files table for video path aliasing");

    // Create video_files table for canonical video storage
    // This enables storing each unique video once and referencing it from multiple archives
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS video_files (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            video_id TEXT NOT NULL,
            platform TEXT NOT NULL,
            s3_key TEXT NOT NULL,
            metadata_s3_key TEXT,
            size_bytes INTEGER,
            content_type TEXT,
            duration_seconds INTEGER,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(platform, video_id)
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create video_files table")?;

    // Create indexes for efficient lookups
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_video_files_platform_video_id ON video_files(platform, video_id)",
    )
    .execute(pool)
    .await
    .context("Failed to create video_files platform+video_id index")?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_video_files_s3_key ON video_files(s3_key)")
        .execute(pool)
        .await
        .context("Failed to create video_files s3_key index")?;

    // Add video_file_id column to archive_artifacts table
    // This links artifacts to their canonical video file
    sqlx::query(
        "ALTER TABLE archive_artifacts ADD COLUMN video_file_id INTEGER REFERENCES video_files(id)",
    )
    .execute(pool)
    .await
    .context("Failed to add video_file_id column to archive_artifacts")?;

    // Create index for efficient artifact->video_file lookups
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_archive_artifacts_video_file_id ON archive_artifacts(video_file_id)",
    )
    .execute(pool)
    .await
    .context("Failed to create archive_artifacts video_file_id index")?;

    Ok(())
}

async fn run_migration_v11(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v11: adding content_type index for efficient filtering");

    // Create index for filtering by content type (video, image, gallery, text, thread)
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_archives_content_type ON archives(content_type)")
        .execute(pool)
        .await
        .context("Failed to create content_type index")?;

    Ok(())
}
