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

    if current_version < 12 {
        debug!("Running migration v12");
        run_migration_v12(pool).await?;
        set_schema_version(pool, 12).await?;
    }

    if current_version < 13 {
        debug!("Running migration v13");
        run_migration_v13(pool).await?;
        set_schema_version(pool, 13).await?;
    }

    if current_version < 14 {
        debug!("Running migration v14");
        run_migration_v14(pool).await?;
        set_schema_version(pool, 14).await?;
    }

    if current_version < 15 {
        debug!("Running migration v15");
        run_migration_v15(pool).await?;
        set_schema_version(pool, 15).await?;
    }

    if current_version < 16 {
        debug!("Running migration v16");
        run_migration_v16(pool).await?;
        set_schema_version(pool, 16).await?;
    }

    if current_version < 17 {
        debug!("Running migration v17");
        run_migration_v17(pool).await?;
        set_schema_version(pool, 17).await?;
    }

    if current_version < 18 {
        debug!("Running migration v18");
        run_migration_v18(pool).await?;
        set_schema_version(pool, 18).await?;
    }

    if current_version < 19 {
        debug!("Running migration v19");
        run_migration_v19(pool).await?;
        set_schema_version(pool, 19).await?;
    }

    if current_version < 20 {
        debug!("Running migration v20");
        run_migration_v20(pool).await?;
        set_schema_version(pool, 20).await?;
    }

    if current_version < 21 {
        debug!("Running migration v21");
        run_migration_v21(pool).await?;
        set_schema_version(pool, 21).await?;
    }

    if current_version < 22 {
        debug!("Running migration v22");
        run_migration_v22(pool).await?;
        set_schema_version(pool, 22).await?;
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

async fn run_migration_v12(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v12: adding user accounts and auth tables");

    // Users table
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            email TEXT,
            display_name TEXT,
            is_approved INTEGER NOT NULL DEFAULT 0,
            is_admin INTEGER NOT NULL DEFAULT 0,
            is_active INTEGER NOT NULL DEFAULT 1,
            failed_login_attempts INTEGER NOT NULL DEFAULT 0,
            locked_until TEXT,
            password_updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create users table")?;

    // Sessions table
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            token TEXT NOT NULL UNIQUE,
            csrf_token TEXT NOT NULL,
            ip_address TEXT NOT NULL,
            user_agent TEXT,
            expires_at TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            last_used_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create sessions table")?;

    // Audit events table
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS audit_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
            event_type TEXT NOT NULL,
            target_type TEXT,
            target_id INTEGER,
            metadata TEXT,
            ip_address TEXT,
            user_agent TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create audit_events table")?;

    // Create indexes
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_users_username ON users(username)")
        .execute(pool)
        .await
        .context("Failed to create users username index")?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_users_is_approved ON users(is_approved)")
        .execute(pool)
        .await
        .context("Failed to create users is_approved index")?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_users_is_admin ON users(is_admin)")
        .execute(pool)
        .await
        .context("Failed to create users is_admin index")?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_token ON sessions(token)")
        .execute(pool)
        .await
        .context("Failed to create sessions token index")?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id)")
        .execute(pool)
        .await
        .context("Failed to create sessions user_id index")?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at)")
        .execute(pool)
        .await
        .context("Failed to create sessions expires_at index")?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_events_user_id ON audit_events(user_id)")
        .execute(pool)
        .await
        .context("Failed to create audit_events user_id index")?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_audit_events_event_type ON audit_events(event_type)",
    )
    .execute(pool)
    .await
    .context("Failed to create audit_events event_type index")?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_audit_events_created_at ON audit_events(created_at)",
    )
    .execute(pool)
    .await
    .context("Failed to create audit_events created_at index")?;

    Ok(())
}

async fn run_migration_v13(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v13: adding forwarded_for to audit_events, user_agents table, and display_name uniqueness");

    // Add forwarded_for column to audit_events for X-Forwarded-For header storage
    sqlx::query("ALTER TABLE audit_events ADD COLUMN forwarded_for TEXT")
        .execute(pool)
        .await
        .context("Failed to add forwarded_for column to audit_events")?;

    // Create user_agents table to deduplicate user agent strings
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS user_agents (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            hash TEXT NOT NULL UNIQUE,
            user_agent TEXT NOT NULL,
            first_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            last_seen_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create user_agents table")?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_user_agents_hash ON user_agents(hash)")
        .execute(pool)
        .await
        .context("Failed to create user_agents hash index")?;

    // Add user_agent_id column to audit_events (nullable, for new entries)
    sqlx::query(
        "ALTER TABLE audit_events ADD COLUMN user_agent_id INTEGER REFERENCES user_agents(id)",
    )
    .execute(pool)
    .await
    .context("Failed to add user_agent_id column to audit_events")?;

    // Add unique index on display_name (partial index, only for non-null values)
    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_users_display_name ON users(display_name) WHERE display_name IS NOT NULL",
    )
    .execute(pool)
    .await
    .context("Failed to create users display_name unique index")?;

    Ok(())
}

async fn run_migration_v14(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v14: adding metadata column to archive_artifacts for subtitle/transcript metadata");

    // Add metadata column to archive_artifacts for storing structured data (JSON)
    // This is useful for subtitles (language, is_auto, format) and transcripts (source info)
    sqlx::query("ALTER TABLE archive_artifacts ADD COLUMN metadata TEXT")
        .execute(pool)
        .await
        .context("Failed to add metadata column to archive_artifacts")?;

    Ok(())
}

async fn run_migration_v15(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v15: adding comments system");

    // Comments table - stores user comments on archives
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS comments (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            archive_id INTEGER NOT NULL REFERENCES archives(id) ON DELETE CASCADE,
            user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
            parent_comment_id INTEGER REFERENCES comments(id) ON DELETE CASCADE,
            content TEXT NOT NULL,
            is_deleted INTEGER NOT NULL DEFAULT 0,
            deleted_by_admin INTEGER NOT NULL DEFAULT 0,
            is_pinned INTEGER NOT NULL DEFAULT 0,
            pinned_by_user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            deleted_at TEXT
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create comments table")?;

    // Comment edits table - tracks edit history
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS comment_edits (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            comment_id INTEGER NOT NULL REFERENCES comments(id) ON DELETE CASCADE,
            previous_content TEXT NOT NULL,
            edited_by_user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            edited_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create comment_edits table")?;

    // Comment reactions table - tracks helpful votes
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS comment_reactions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            comment_id INTEGER NOT NULL REFERENCES comments(id) ON DELETE CASCADE,
            user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            reaction_type TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(comment_id, user_id, reaction_type)
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create comment_reactions table")?;

    // Create indexes for efficient querying
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_comments_archive_id ON comments(archive_id) WHERE is_deleted = 0",
    )
    .execute(pool)
    .await
    .context("Failed to create comments archive_id index")?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_comments_user_id ON comments(user_id)")
        .execute(pool)
        .await
        .context("Failed to create comments user_id index")?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_comments_parent_id ON comments(parent_comment_id)")
        .execute(pool)
        .await
        .context("Failed to create comments parent_comment_id index")?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_comments_pinned_created ON comments(is_pinned DESC, created_at DESC) WHERE is_deleted = 0",
    )
    .execute(pool)
    .await
    .context("Failed to create comments pinned+created index")?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_comment_edits_comment_id ON comment_edits(comment_id)",
    )
    .execute(pool)
    .await
    .context("Failed to create comment_edits comment_id index")?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_comment_reactions_comment_id ON comment_reactions(comment_id, reaction_type)",
    )
    .execute(pool)
    .await
    .context("Failed to create comment_reactions index")?;

    Ok(())
}

async fn run_migration_v16(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS excluded_domains (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            domain TEXT NOT NULL UNIQUE,
            reason TEXT NOT NULL DEFAULT 'Self-archive exclusion',
            is_active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            created_by_user_id INTEGER,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create excluded_domains table")?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_excluded_domains_domain ON excluded_domains(domain, is_active)",
    )
    .execute(pool)
    .await
    .context("Failed to create excluded_domains domain index")?;

    Ok(())
}

async fn run_migration_v17(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v17: adding Twitter quote/reply relationship tracking");

    // Add quoted_archive_id column to archives table
    // This links a tweet archive to the archive of the tweet it quotes
    sqlx::query(
        "ALTER TABLE archives ADD COLUMN quoted_archive_id INTEGER REFERENCES archives(id)",
    )
    .execute(pool)
    .await
    .context("Failed to add quoted_archive_id column")?;

    // Add reply_to_archive_id column to archives table
    // This links a tweet archive to the archive of the tweet it replies to
    sqlx::query(
        "ALTER TABLE archives ADD COLUMN reply_to_archive_id INTEGER REFERENCES archives(id)",
    )
    .execute(pool)
    .await
    .context("Failed to add reply_to_archive_id column")?;

    // Create indexes for efficient lookup of quote/reply chains
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_archives_quoted_archive_id ON archives(quoted_archive_id)",
    )
    .execute(pool)
    .await
    .context("Failed to create quoted_archive_id index")?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_archives_reply_to_archive_id ON archives(reply_to_archive_id)",
    )
    .execute(pool)
    .await
    .context("Failed to create reply_to_archive_id index")?;

    Ok(())
}

async fn run_migration_v18(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v18: adding thread_archive_jobs table for bulk thread archiving");

    // Create thread_archive_jobs table for tracking bulk thread archive requests
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS thread_archive_jobs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            thread_url TEXT NOT NULL,
            rss_url TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            user_id INTEGER NOT NULL REFERENCES users(id),
            total_posts INTEGER,
            processed_posts INTEGER NOT NULL DEFAULT 0,
            new_links_found INTEGER NOT NULL DEFAULT 0,
            archives_created INTEGER NOT NULL DEFAULT 0,
            skipped_links INTEGER NOT NULL DEFAULT 0,
            error_message TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            started_at TEXT,
            completed_at TEXT
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create thread_archive_jobs table")?;

    // Index for querying pending jobs
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_thread_archive_jobs_status ON thread_archive_jobs(status)",
    )
    .execute(pool)
    .await
    .context("Failed to create thread_archive_jobs status index")?;

    // Index for user's job history
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_thread_archive_jobs_user_id ON thread_archive_jobs(user_id)",
    )
    .execute(pool)
    .await
    .context("Failed to create thread_archive_jobs user_id index")?;

    // Index for rate limiting and deduplication
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_thread_archive_jobs_created_at ON thread_archive_jobs(created_at DESC)",
    )
    .execute(pool)
    .await
    .context("Failed to create thread_archive_jobs created_at index")?;

    Ok(())
}

async fn run_migration_v19(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v19: adding forum_account_links table for forum account linking");

    // Create forum_account_links table for tracking links between forum accounts and archive accounts
    sqlx::query(
        r"
        CREATE TABLE IF NOT EXISTS forum_account_links (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
            forum_username TEXT NOT NULL UNIQUE,
            linked_via_post_guid TEXT NOT NULL,
            linked_via_post_url TEXT NOT NULL,
            forum_author_raw TEXT,
            post_title TEXT,
            post_published_at TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        ",
    )
    .execute(pool)
    .await
    .context("Failed to create forum_account_links table")?;

    // Index for looking up links by post GUID
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_forum_links_post_guid ON forum_account_links(linked_via_post_guid)",
    )
    .execute(pool)
    .await
    .context("Failed to create forum_account_links post_guid index")?;

    Ok(())
}

async fn run_migration_v20(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v20: adding submitted_by_user_id to submissions and archives");

    // Add submitted_by_user_id column to submissions table
    sqlx::query(
        "ALTER TABLE submissions ADD COLUMN submitted_by_user_id INTEGER REFERENCES users(id)",
    )
    .execute(pool)
    .await
    .context("Failed to add submitted_by_user_id column to submissions")?;

    // Add submitted_by_user_id column to archives table
    sqlx::query(
        "ALTER TABLE archives ADD COLUMN submitted_by_user_id INTEGER REFERENCES users(id)",
    )
    .execute(pool)
    .await
    .context("Failed to add submitted_by_user_id column to archives")?;

    // Create indexes for efficient lookup
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_submissions_user_id ON submissions(submitted_by_user_id)",
    )
    .execute(pool)
    .await
    .context("Failed to create submissions user_id index")?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_archives_submitted_by_user_id ON archives(submitted_by_user_id)",
    )
    .execute(pool)
    .await
    .context("Failed to create archives submitted_by_user_id index")?;

    Ok(())
}

async fn run_migration_v21(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v21: adding download progress tracking for yt-dlp");

    // Add progress_percent column to track download percentage (0.0-100.0)
    sqlx::query("ALTER TABLE archives ADD COLUMN progress_percent REAL")
        .execute(pool)
        .await
        .context("Failed to add progress_percent column")?;

    // Add progress_details column to store JSON with speed, ETA, size, etc.
    sqlx::query("ALTER TABLE archives ADD COLUMN progress_details TEXT")
        .execute(pool)
        .await
        .context("Failed to add progress_details column")?;

    // Add last_progress_update to track when progress was last updated
    sqlx::query("ALTER TABLE archives ADD COLUMN last_progress_update TEXT")
        .execute(pool)
        .await
        .context("Failed to add last_progress_update column")?;

    // Create index for efficient querying of active downloads
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_archives_last_progress_update ON archives(last_progress_update) WHERE status = 'processing'",
    )
    .execute(pool)
    .await
    .context("Failed to create last_progress_update index")?;

    Ok(())
}

async fn run_migration_v22(pool: &SqlitePool) -> Result<()> {
    debug!("Running migration v22: adding duration tracking for archive jobs");

    // Add duration_seconds column to track job execution time with subsecond accuracy
    // This is calculated as the difference between started_at and completed_at
    sqlx::query("ALTER TABLE archive_jobs ADD COLUMN duration_seconds REAL")
        .execute(pool)
        .await
        .context("Failed to add duration_seconds column")?;

    Ok(())
}
