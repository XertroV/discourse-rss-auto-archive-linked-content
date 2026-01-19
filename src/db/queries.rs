use anyhow::{Context, Result};
use sqlx::SqlitePool;

use super::models::{
    Archive, ArchiveArtifact, ArchiveDisplay, ArchiveJob, ArchiveJobType, Link, LinkOccurrence,
    NewLink, NewLinkOccurrence, NewPost, NewSubmission, Post, Submission,
};

// ========== Posts ==========

/// Get a post by its GUID.
pub async fn get_post_by_guid(pool: &SqlitePool, guid: &str) -> Result<Option<Post>> {
    sqlx::query_as("SELECT * FROM posts WHERE guid = ?")
        .bind(guid)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch post by guid")
}

/// Insert a new post, returning its ID.
pub async fn insert_post(pool: &SqlitePool, post: &NewPost) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO posts (guid, discourse_url, author, title, body_html, content_hash, published_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ",
    )
    .bind(&post.guid)
    .bind(&post.discourse_url)
    .bind(&post.author)
    .bind(&post.title)
    .bind(&post.body_html)
    .bind(&post.content_hash)
    .bind(&post.published_at)
    .execute(pool)
    .await
    .context("Failed to insert post")?;

    Ok(result.last_insert_rowid())
}

/// Update an existing post's content.
pub async fn update_post(pool: &SqlitePool, id: i64, post: &NewPost) -> Result<()> {
    sqlx::query(
        r"
        UPDATE posts
        SET discourse_url = ?, author = ?, title = ?, body_html = ?,
            content_hash = ?, published_at = ?, processed_at = datetime('now')
        WHERE id = ?
        ",
    )
    .bind(&post.discourse_url)
    .bind(&post.author)
    .bind(&post.title)
    .bind(&post.body_html)
    .bind(&post.content_hash)
    .bind(&post.published_at)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to update post")?;

    Ok(())
}

// ========== Links ==========

/// Get a link by its normalized URL.
pub async fn get_link_by_normalized_url(pool: &SqlitePool, url: &str) -> Result<Option<Link>> {
    sqlx::query_as("SELECT * FROM links WHERE normalized_url = ?")
        .bind(url)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch link by normalized URL")
}

/// Insert a new link, returning its ID.
pub async fn insert_link(pool: &SqlitePool, link: &NewLink) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO links (original_url, normalized_url, canonical_url, domain)
        VALUES (?, ?, ?, ?)
        ",
    )
    .bind(&link.original_url)
    .bind(&link.normalized_url)
    .bind(&link.canonical_url)
    .bind(&link.domain)
    .execute(pool)
    .await
    .context("Failed to insert link")?;

    Ok(result.last_insert_rowid())
}

/// Update the final URL after redirect resolution.
pub async fn update_link_final_url(pool: &SqlitePool, id: i64, final_url: &str) -> Result<()> {
    sqlx::query("UPDATE links SET final_url = ? WHERE id = ?")
        .bind(final_url)
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to update link final URL")?;

    Ok(())
}

/// Update the last archived timestamp.
pub async fn update_link_last_archived(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query("UPDATE links SET last_archived_at = datetime('now') WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to update link last archived")?;

    Ok(())
}

// ========== Link Occurrences ==========

/// Check if a link occurrence exists for a post.
pub async fn link_occurrence_exists(pool: &SqlitePool, link_id: i64, post_id: i64) -> Result<bool> {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM link_occurrences WHERE link_id = ? AND post_id = ?")
            .bind(link_id)
            .bind(post_id)
            .fetch_one(pool)
            .await?;

    Ok(row.0 > 0)
}

/// Insert a new link occurrence.
pub async fn insert_link_occurrence(pool: &SqlitePool, occ: &NewLinkOccurrence) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO link_occurrences (link_id, post_id, in_quote, context_snippet)
        VALUES (?, ?, ?, ?)
        ",
    )
    .bind(occ.link_id)
    .bind(occ.post_id)
    .bind(occ.in_quote)
    .bind(&occ.context_snippet)
    .execute(pool)
    .await
    .context("Failed to insert link occurrence")?;

    Ok(result.last_insert_rowid())
}

/// Check if a link has any non-quote occurrences.
pub async fn link_has_non_quote_occurrence(pool: &SqlitePool, link_id: i64) -> Result<bool> {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM link_occurrences WHERE link_id = ? AND in_quote = 0")
            .bind(link_id)
            .fetch_one(pool)
            .await?;

    Ok(row.0 > 0)
}

// ========== Archives ==========

/// Get an archive by ID.
pub async fn get_archive(pool: &SqlitePool, id: i64) -> Result<Option<Archive>> {
    sqlx::query_as("SELECT * FROM archives WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch archive")
}

/// Get the archive for a link.
pub async fn get_archive_by_link_id(pool: &SqlitePool, link_id: i64) -> Result<Option<Archive>> {
    sqlx::query_as("SELECT * FROM archives WHERE link_id = ?")
        .bind(link_id)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch archive by link")
}

/// Create a pending archive for a link.
pub async fn create_pending_archive(pool: &SqlitePool, link_id: i64) -> Result<i64> {
    let result = sqlx::query("INSERT INTO archives (link_id, status) VALUES (?, 'pending')")
        .bind(link_id)
        .execute(pool)
        .await
        .context("Failed to create pending archive")?;

    Ok(result.last_insert_rowid())
}

/// Update archive status to processing.
pub async fn set_archive_processing(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query("UPDATE archives SET status = 'processing' WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to set archive processing")?;

    Ok(())
}

/// Update archive as complete with results.
#[allow(clippy::too_many_arguments)]
pub async fn set_archive_complete(
    pool: &SqlitePool,
    id: i64,
    content_title: Option<&str>,
    content_author: Option<&str>,
    content_text: Option<&str>,
    content_type: Option<&str>,
    s3_key_primary: Option<&str>,
    s3_key_thumb: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r"
        UPDATE archives
        SET status = 'complete',
            archived_at = datetime('now'),
            content_title = ?,
            content_author = ?,
            content_text = ?,
            content_type = ?,
            s3_key_primary = ?,
            s3_key_thumb = ?
        WHERE id = ?
        ",
    )
    .bind(content_title)
    .bind(content_author)
    .bind(content_text)
    .bind(content_type)
    .bind(s3_key_primary)
    .bind(s3_key_thumb)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to set archive complete")?;

    Ok(())
}

/// Set the NSFW status for an archive.
pub async fn set_archive_nsfw(
    pool: &SqlitePool,
    id: i64,
    is_nsfw: bool,
    nsfw_source: Option<&str>,
) -> Result<()> {
    sqlx::query("UPDATE archives SET is_nsfw = ?, nsfw_source = ? WHERE id = ?")
        .bind(is_nsfw)
        .bind(nsfw_source)
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to set archive NSFW status")?;

    Ok(())
}

/// Update archive as failed with exponential backoff for retry.
///
/// The `next_retry_at` is calculated as: now + (base_delay * 2^retry_count)
/// With base_delay = 5 minutes:
/// - retry 0: 5 minutes
/// - retry 1: 10 minutes
/// - retry 2: 20 minutes
/// - retry 3: 40 minutes (but this is the last retry)
pub async fn set_archive_failed(pool: &SqlitePool, id: i64, error: &str) -> Result<()> {
    // Calculate exponential backoff delay in minutes: 5 * 2^retry_count
    // We use the current retry_count before incrementing, so:
    // retry_count=0 -> 5*2^0 = 5 min, retry_count=1 -> 5*2^1 = 10 min, etc.
    sqlx::query(
        r"
        UPDATE archives
        SET status = 'failed',
            error_message = ?,
            last_attempt_at = datetime('now'),
            next_retry_at = datetime('now', '+' || (5 * (1 << retry_count)) || ' minutes'),
            retry_count = retry_count + 1
        WHERE id = ?
        ",
    )
    .bind(error)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to set archive failed")?;

    Ok(())
}

/// Reset a failed archive to pending for retry.
pub async fn reset_archive_for_retry(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query("UPDATE archives SET status = 'pending' WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to reset archive for retry")?;

    Ok(())
}

/// Mark archive as permanently skipped.
pub async fn set_archive_skipped(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query("UPDATE archives SET status = 'skipped' WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to set archive skipped")?;

    Ok(())
}

/// Set the Wayback URL for an archive.
pub async fn set_archive_wayback_url(pool: &SqlitePool, id: i64, wayback_url: &str) -> Result<()> {
    sqlx::query("UPDATE archives SET wayback_url = ? WHERE id = ?")
        .bind(wayback_url)
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to set wayback URL")?;

    Ok(())
}

/// Get pending archives for processing.
pub async fn get_pending_archives(pool: &SqlitePool, limit: i64) -> Result<Vec<Archive>> {
    sqlx::query_as(
        r"
        SELECT * FROM archives
        WHERE status = 'pending'
        ORDER BY created_at ASC
        LIMIT ?
        ",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch pending archives")
}

/// Get failed archives eligible for retry.
///
/// Returns archives where:
/// - status = 'failed'
/// - retry_count < max_retries
/// - next_retry_at <= now (or is null for legacy data)
pub async fn get_failed_archives_for_retry(
    pool: &SqlitePool,
    limit: i64,
    max_retries: i32,
) -> Result<Vec<Archive>> {
    sqlx::query_as(
        r"
        SELECT * FROM archives
        WHERE status = 'failed'
          AND retry_count < ?
          AND (next_retry_at IS NULL OR next_retry_at <= datetime('now'))
        ORDER BY next_retry_at ASC NULLS FIRST, created_at ASC
        LIMIT ?
        ",
    )
    .bind(max_retries)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch failed archives")
}

/// Get recent archives for the home page.
pub async fn get_recent_archives(pool: &SqlitePool, limit: i64) -> Result<Vec<Archive>> {
    sqlx::query_as(
        r"
        SELECT * FROM archives
        WHERE status = 'complete'
        ORDER BY archived_at DESC
        LIMIT ?
        ",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch recent archives")
}

/// Get recent archives with NSFW filter.
pub async fn get_recent_archives_filtered(
    pool: &SqlitePool,
    limit: i64,
    nsfw_filter: Option<bool>,
) -> Result<Vec<Archive>> {
    match nsfw_filter {
        Some(true) => {
            // Only NSFW
            sqlx::query_as(
                r"
                SELECT * FROM archives
                WHERE status = 'complete' AND is_nsfw = 1
                ORDER BY archived_at DESC
                LIMIT ?
                ",
            )
            .bind(limit)
            .fetch_all(pool)
            .await
            .context("Failed to fetch NSFW archives")
        }
        Some(false) => {
            // Hide NSFW
            sqlx::query_as(
                r"
                SELECT * FROM archives
                WHERE status = 'complete' AND (is_nsfw = 0 OR is_nsfw IS NULL)
                ORDER BY archived_at DESC
                LIMIT ?
                ",
            )
            .bind(limit)
            .fetch_all(pool)
            .await
            .context("Failed to fetch SFW archives")
        }
        None => {
            // Show all
            get_recent_archives(pool, limit).await
        }
    }
}

/// Get recent archives with domain and content_type filters.
/// Used for RSS/Atom feeds with optional filtering.
pub async fn get_recent_archives_with_filters(
    pool: &SqlitePool,
    limit: i64,
    domain: Option<&str>,
    content_type: Option<&str>,
) -> Result<Vec<Archive>> {
    match (domain, content_type) {
        (Some(d), Some(ct)) => {
            // Filter by both domain and content_type
            sqlx::query_as(
                r"
                SELECT a.* FROM archives a
                JOIN links l ON a.link_id = l.id
                WHERE a.status = 'complete' AND l.domain = ? AND a.content_type = ?
                ORDER BY a.archived_at DESC
                LIMIT ?
                ",
            )
            .bind(d)
            .bind(ct)
            .bind(limit)
            .fetch_all(pool)
            .await
            .context("Failed to fetch archives with domain and content_type filter")
        }
        (Some(d), None) => {
            // Filter by domain only
            sqlx::query_as(
                r"
                SELECT a.* FROM archives a
                JOIN links l ON a.link_id = l.id
                WHERE a.status = 'complete' AND l.domain = ?
                ORDER BY a.archived_at DESC
                LIMIT ?
                ",
            )
            .bind(d)
            .bind(limit)
            .fetch_all(pool)
            .await
            .context("Failed to fetch archives with domain filter")
        }
        (None, Some(ct)) => {
            // Filter by content_type only
            sqlx::query_as(
                r"
                SELECT * FROM archives
                WHERE status = 'complete' AND content_type = ?
                ORDER BY archived_at DESC
                LIMIT ?
                ",
            )
            .bind(ct)
            .bind(limit)
            .fetch_all(pool)
            .await
            .context("Failed to fetch archives with content_type filter")
        }
        (None, None) => {
            // No filters, use existing function
            get_recent_archives(pool, limit).await
        }
    }
}

/// Get recent archives with link info for display (all statuses).
pub async fn get_recent_archives_display(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<ArchiveDisplay>> {
    sqlx::query_as(
        r"
        SELECT
            a.id, a.link_id, a.status, a.archived_at,
            a.content_title, a.content_author, a.content_type,
            a.is_nsfw, a.error_message, a.retry_count,
            l.original_url, l.domain,
            COALESCE(SUM(aa.size_bytes), 0) as total_size_bytes
        FROM archives a
        JOIN links l ON a.link_id = l.id
        LEFT JOIN archive_artifacts aa ON a.id = aa.archive_id
        GROUP BY a.id, a.link_id, a.status, a.archived_at,
                 a.content_title, a.content_author, a.content_type,
                 a.is_nsfw, a.error_message, a.retry_count,
                 l.original_url, l.domain
        ORDER BY COALESCE(a.archived_at, a.last_attempt_at, a.created_at) DESC
        LIMIT ?
        ",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch recent archives with links")
}

/// Search archives with link info for display.
pub async fn search_archives_display(
    pool: &SqlitePool,
    query: &str,
    limit: i64,
) -> Result<Vec<ArchiveDisplay>> {
    sqlx::query_as(
        r"
        SELECT
            a.id, a.link_id, a.status, a.archived_at,
            a.content_title, a.content_author, a.content_type,
            a.is_nsfw, a.error_message, a.retry_count,
            l.original_url, l.domain,
            COALESCE(SUM(aa.size_bytes), 0) as total_size_bytes
        FROM archives a
        JOIN links l ON a.link_id = l.id
        JOIN archives_fts ON a.id = archives_fts.rowid
        LEFT JOIN archive_artifacts aa ON a.id = aa.archive_id
        WHERE archives_fts MATCH ?
        GROUP BY a.id, a.link_id, a.status, a.archived_at,
                 a.content_title, a.content_author, a.content_type,
                 a.is_nsfw, a.error_message, a.retry_count,
                 l.original_url, l.domain
        ORDER BY rank
        LIMIT ?
        ",
    )
    .bind(query)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to search archives with links")
}

/// Get archives by domain with link info for display (all statuses).
pub async fn get_archives_by_domain_display(
    pool: &SqlitePool,
    domain: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<ArchiveDisplay>> {
    sqlx::query_as(
        r"
        SELECT
            a.id, a.link_id, a.status, a.archived_at,
            a.content_title, a.content_author, a.content_type,
            a.is_nsfw, a.error_message, a.retry_count,
            l.original_url, l.domain,
            COALESCE(SUM(aa.size_bytes), 0) as total_size_bytes
        FROM archives a
        JOIN links l ON a.link_id = l.id
        LEFT JOIN archive_artifacts aa ON a.id = aa.archive_id
        WHERE l.domain = ?
        GROUP BY a.id, a.link_id, a.status, a.archived_at,
                 a.content_title, a.content_author, a.content_type,
                 a.is_nsfw, a.error_message, a.retry_count,
                 l.original_url, l.domain
        ORDER BY COALESCE(a.archived_at, a.last_attempt_at, a.created_at) DESC
        LIMIT ? OFFSET ?
        ",
    )
    .bind(domain)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .context("Failed to fetch archives by domain with links")
}

/// Get archives for a post with link info for display.
pub async fn get_archives_for_post_display(
    pool: &SqlitePool,
    post_id: i64,
) -> Result<Vec<ArchiveDisplay>> {
    sqlx::query_as(
        r"
        SELECT DISTINCT
            a.id, a.link_id, a.status, a.archived_at,
            a.content_title, a.content_author, a.content_type,
            a.is_nsfw, a.error_message, a.retry_count,
            l.original_url, l.domain,
            COALESCE(SUM(aa.size_bytes), 0) as total_size_bytes
        FROM archives a
        JOIN links l ON a.link_id = l.id
        JOIN link_occurrences lo ON l.id = lo.link_id
        LEFT JOIN archive_artifacts aa ON a.id = aa.archive_id
        WHERE lo.post_id = ?
        GROUP BY a.id, a.link_id, a.status, a.archived_at,
                 a.content_title, a.content_author, a.content_type,
                 a.is_nsfw, a.error_message, a.retry_count,
                 l.original_url, l.domain
        ORDER BY a.archived_at DESC
        ",
    )
    .bind(post_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch archives for post with links")
}

/// Search archives using FTS.
pub async fn search_archives(pool: &SqlitePool, query: &str, limit: i64) -> Result<Vec<Archive>> {
    sqlx::query_as(
        r"
        SELECT archives.* FROM archives
        JOIN archives_fts ON archives.id = archives_fts.rowid
        WHERE archives_fts MATCH ?
        ORDER BY rank
        LIMIT ?
        ",
    )
    .bind(query)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to search archives")
}

/// Search archives with NSFW filter.
pub async fn search_archives_filtered(
    pool: &SqlitePool,
    query: &str,
    limit: i64,
    nsfw_filter: Option<bool>,
) -> Result<Vec<Archive>> {
    match nsfw_filter {
        Some(true) => {
            // Only NSFW
            sqlx::query_as(
                r"
                SELECT archives.* FROM archives
                JOIN archives_fts ON archives.id = archives_fts.rowid
                WHERE archives_fts MATCH ? AND archives.is_nsfw = 1
                ORDER BY rank
                LIMIT ?
                ",
            )
            .bind(query)
            .bind(limit)
            .fetch_all(pool)
            .await
            .context("Failed to search NSFW archives")
        }
        Some(false) => {
            // Hide NSFW
            sqlx::query_as(
                r"
                SELECT archives.* FROM archives
                JOIN archives_fts ON archives.id = archives_fts.rowid
                WHERE archives_fts MATCH ? AND (archives.is_nsfw = 0 OR archives.is_nsfw IS NULL)
                ORDER BY rank
                LIMIT ?
                ",
            )
            .bind(query)
            .bind(limit)
            .fetch_all(pool)
            .await
            .context("Failed to search SFW archives")
        }
        None => {
            // Show all
            search_archives(pool, query, limit).await
        }
    }
}

/// Get archives by domain.
pub async fn get_archives_by_domain(
    pool: &SqlitePool,
    domain: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Archive>> {
    sqlx::query_as(
        r"
        SELECT archives.* FROM archives
        JOIN links ON archives.link_id = links.id
        WHERE links.domain = ? AND archives.status = 'complete'
        ORDER BY archives.archived_at DESC
        LIMIT ? OFFSET ?
        ",
    )
    .bind(domain)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .context("Failed to fetch archives by domain")
}

/// Get link by ID.
pub async fn get_link(pool: &SqlitePool, id: i64) -> Result<Option<Link>> {
    sqlx::query_as("SELECT * FROM links WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch link")
}

// ========== Archive Artifacts ==========

/// Insert an archive artifact.
pub async fn insert_artifact(
    pool: &SqlitePool,
    archive_id: i64,
    kind: &str,
    s3_key: &str,
    content_type: Option<&str>,
    size_bytes: Option<i64>,
    sha256: Option<&str>,
) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO archive_artifacts (archive_id, kind, s3_key, content_type, size_bytes, sha256)
        VALUES (?, ?, ?, ?, ?, ?)
        ",
    )
    .bind(archive_id)
    .bind(kind)
    .bind(s3_key)
    .bind(content_type)
    .bind(size_bytes)
    .bind(sha256)
    .execute(pool)
    .await
    .context("Failed to insert artifact")?;

    Ok(result.last_insert_rowid())
}

/// Get artifacts for an archive.
pub async fn get_artifacts_for_archive(
    pool: &SqlitePool,
    archive_id: i64,
) -> Result<Vec<ArchiveArtifact>> {
    sqlx::query_as("SELECT * FROM archive_artifacts WHERE archive_id = ? ORDER BY created_at")
        .bind(archive_id)
        .fetch_all(pool)
        .await
        .context("Failed to fetch artifacts")
}

// ========== Statistics ==========

/// Get total count of archives by status.
pub async fn count_archives_by_status(pool: &SqlitePool) -> Result<Vec<(String, i64)>> {
    sqlx::query_as("SELECT status, COUNT(*) FROM archives GROUP BY status")
        .fetch_all(pool)
        .await
        .context("Failed to count archives")
}

/// Get total count of links.
pub async fn count_links(pool: &SqlitePool) -> Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM links")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// Get total count of posts.
pub async fn count_posts(pool: &SqlitePool) -> Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM posts")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// Get a post by ID.
pub async fn get_post(pool: &SqlitePool, id: i64) -> Result<Option<Post>> {
    sqlx::query_as("SELECT * FROM posts WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch post")
}

/// Get archives for a specific post by joining through link_occurrences.
pub async fn get_archives_for_post(pool: &SqlitePool, post_id: i64) -> Result<Vec<Archive>> {
    sqlx::query_as(
        r"
        SELECT DISTINCT archives.* FROM archives
        JOIN links ON archives.link_id = links.id
        JOIN link_occurrences ON links.id = link_occurrences.link_id
        WHERE link_occurrences.post_id = ?
        ORDER BY archives.created_at DESC
        ",
    )
    .bind(post_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch archives for post")
}

/// Get link occurrences for a post.
pub async fn get_occurrences_for_post(
    pool: &SqlitePool,
    post_id: i64,
) -> Result<Vec<LinkOccurrence>> {
    sqlx::query_as(
        r"
        SELECT * FROM link_occurrences
        WHERE post_id = ?
        ORDER BY seen_at ASC
        ",
    )
    .bind(post_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch occurrences for post")
}

// ========== IPFS ==========

/// Set the IPFS CID for an archive.
pub async fn set_archive_ipfs_cid(pool: &SqlitePool, id: i64, ipfs_cid: &str) -> Result<()> {
    sqlx::query("UPDATE archives SET ipfs_cid = ? WHERE id = ?")
        .bind(ipfs_cid)
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to set IPFS CID")?;

    Ok(())
}

// ========== Submissions ==========

/// Insert a new submission, returning its ID.
pub async fn insert_submission(pool: &SqlitePool, submission: &NewSubmission) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO submissions (url, normalized_url, submitted_by_ip, status)
        VALUES (?, ?, ?, 'pending')
        ",
    )
    .bind(&submission.url)
    .bind(&submission.normalized_url)
    .bind(&submission.submitted_by_ip)
    .execute(pool)
    .await
    .context("Failed to insert submission")?;

    Ok(result.last_insert_rowid())
}

/// Get a submission by ID.
pub async fn get_submission(pool: &SqlitePool, id: i64) -> Result<Option<Submission>> {
    sqlx::query_as("SELECT * FROM submissions WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch submission")
}

/// Get pending submissions for processing.
pub async fn get_pending_submissions(pool: &SqlitePool, limit: i64) -> Result<Vec<Submission>> {
    sqlx::query_as(
        r"
        SELECT * FROM submissions
        WHERE status = 'pending'
        ORDER BY created_at ASC
        LIMIT ?
        ",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch pending submissions")
}

/// Count submissions from an IP in the last hour.
pub async fn count_submissions_from_ip_last_hour(pool: &SqlitePool, ip: &str) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        r"
        SELECT COUNT(*) FROM submissions
        WHERE submitted_by_ip = ?
        AND created_at > datetime('now', '-1 hour')
        ",
    )
    .bind(ip)
    .fetch_one(pool)
    .await?;

    Ok(row.0)
}

/// Check if a URL has already been submitted recently (within last 24 hours).
pub async fn submission_exists_for_url(pool: &SqlitePool, normalized_url: &str) -> Result<bool> {
    let row: (i64,) = sqlx::query_as(
        r"
        SELECT COUNT(*) FROM submissions
        WHERE normalized_url = ?
        AND created_at > datetime('now', '-24 hours')
        ",
    )
    .bind(normalized_url)
    .fetch_one(pool)
    .await?;

    Ok(row.0 > 0)
}

/// Set submission status to processing.
pub async fn set_submission_processing(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query("UPDATE submissions SET status = 'processing' WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to set submission processing")?;

    Ok(())
}

/// Set submission as complete with link_id.
pub async fn set_submission_complete(pool: &SqlitePool, id: i64, link_id: i64) -> Result<()> {
    sqlx::query(
        r"
        UPDATE submissions
        SET status = 'complete',
            link_id = ?,
            processed_at = datetime('now')
        WHERE id = ?
        ",
    )
    .bind(link_id)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to set submission complete")?;

    Ok(())
}

/// Set submission as failed.
pub async fn set_submission_failed(pool: &SqlitePool, id: i64, error: &str) -> Result<()> {
    sqlx::query(
        r"
        UPDATE submissions
        SET status = 'failed',
            error_message = ?,
            processed_at = datetime('now')
        WHERE id = ?
        ",
    )
    .bind(error)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to set submission failed")?;

    Ok(())
}

/// Set submission as rejected (e.g., rate limited, invalid URL).
pub async fn set_submission_rejected(pool: &SqlitePool, id: i64, reason: &str) -> Result<()> {
    sqlx::query(
        r"
        UPDATE submissions
        SET status = 'rejected',
            error_message = ?,
            processed_at = datetime('now')
        WHERE id = ?
        ",
    )
    .bind(reason)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to set submission rejected")?;

    Ok(())
}

// ========== Startup Recovery ==========

/// Reset archives stuck in "processing" status back to "pending".
///
/// This should be called on startup to recover from interrupted processing
/// (e.g., container restart, crash). These archives were mid-processing when
/// the application stopped.
pub async fn reset_stuck_processing_archives(pool: &SqlitePool) -> Result<u64> {
    let result = sqlx::query(
        r"
        UPDATE archives
        SET status = 'pending'
        WHERE status = 'processing'
        ",
    )
    .execute(pool)
    .await
    .context("Failed to reset stuck processing archives")?;

    Ok(result.rows_affected())
}

/// Reset failed archives from today for retry on startup.
///
/// This allows archives that failed today to be retried when the container
/// restarts, giving them a fresh chance (e.g., if the failure was due to
/// a temporary network issue or service outage).
///
/// Only resets archives that haven't exceeded the max retry count and were
/// created or last attempted today.
pub async fn reset_todays_failed_archives(pool: &SqlitePool, max_retries: i32) -> Result<u64> {
    let result = sqlx::query(
        r"
        UPDATE archives
        SET status = 'pending',
            next_retry_at = NULL
        WHERE status = 'failed'
          AND retry_count < ?
          AND (
              date(created_at) = date('now')
              OR date(last_attempt_at) = date('now')
          )
        ",
    )
    .bind(max_retries)
    .execute(pool)
    .await
    .context("Failed to reset today's failed archives")?;

    Ok(result.rows_affected())
}

// ========== Content Deduplication ==========

/// Find an artifact with a matching perceptual hash.
///
/// Returns the first artifact that has the same perceptual hash,
/// indicating a potential duplicate.
pub async fn find_artifact_by_perceptual_hash(
    pool: &SqlitePool,
    perceptual_hash: &str,
) -> Result<Option<ArchiveArtifact>> {
    sqlx::query_as(
        r"
        SELECT * FROM archive_artifacts
        WHERE perceptual_hash = ?
          AND duplicate_of_artifact_id IS NULL
        LIMIT 1
        ",
    )
    .bind(perceptual_hash)
    .fetch_optional(pool)
    .await
    .context("Failed to find artifact by perceptual hash")
}

/// Insert an artifact with perceptual hash for deduplication.
pub async fn insert_artifact_with_hash(
    pool: &SqlitePool,
    archive_id: i64,
    kind: &str,
    s3_key: &str,
    content_type: Option<&str>,
    size_bytes: Option<i64>,
    sha256: Option<&str>,
    perceptual_hash: Option<&str>,
    duplicate_of_artifact_id: Option<i64>,
) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO archive_artifacts (archive_id, kind, s3_key, content_type, size_bytes, sha256, perceptual_hash, duplicate_of_artifact_id)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ",
    )
    .bind(archive_id)
    .bind(kind)
    .bind(s3_key)
    .bind(content_type)
    .bind(size_bytes)
    .bind(sha256)
    .bind(perceptual_hash)
    .bind(duplicate_of_artifact_id)
    .execute(pool)
    .await
    .context("Failed to insert artifact with hash")?;

    Ok(result.last_insert_rowid())
}

/// Update an existing artifact's perceptual hash.
pub async fn update_artifact_perceptual_hash(
    pool: &SqlitePool,
    artifact_id: i64,
    perceptual_hash: &str,
) -> Result<()> {
    sqlx::query("UPDATE archive_artifacts SET perceptual_hash = ? WHERE id = ?")
        .bind(perceptual_hash)
        .bind(artifact_id)
        .execute(pool)
        .await
        .context("Failed to update artifact perceptual hash")?;

    Ok(())
}

/// Get an artifact by ID.
pub async fn get_artifact(pool: &SqlitePool, id: i64) -> Result<Option<ArchiveArtifact>> {
    sqlx::query_as("SELECT * FROM archive_artifacts WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch artifact")
}

// ========== Debug / Queue Stats ==========

/// Queue statistics for debug page.
#[derive(Debug, Clone)]
pub struct QueueStats {
    pub pending_count: i64,
    pub processing_count: i64,
    pub failed_awaiting_retry: i64,
    pub failed_max_retries: i64,
    pub skipped_count: i64,
    pub complete_count: i64,
    pub next_retry_at: Option<String>,
    pub oldest_pending_at: Option<String>,
}

/// Get queue statistics for debug page.
pub async fn get_queue_stats(pool: &SqlitePool, max_retries: i32) -> Result<QueueStats> {
    // Get counts by status
    let pending: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM archives WHERE status = 'pending'")
        .fetch_one(pool)
        .await?;

    let processing: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM archives WHERE status = 'processing'")
            .fetch_one(pool)
            .await?;

    let failed_awaiting: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM archives WHERE status = 'failed' AND retry_count < ?")
            .bind(max_retries)
            .fetch_one(pool)
            .await?;

    let failed_max: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM archives WHERE status = 'failed' AND retry_count >= ?",
    )
    .bind(max_retries)
    .fetch_one(pool)
    .await?;

    let skipped: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM archives WHERE status = 'skipped'")
        .fetch_one(pool)
        .await?;

    let complete: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM archives WHERE status = 'complete'")
            .fetch_one(pool)
            .await?;

    // Get next retry time
    let next_retry: Option<(String,)> = sqlx::query_as(
        "SELECT next_retry_at FROM archives WHERE status = 'failed' AND retry_count < ? AND next_retry_at IS NOT NULL ORDER BY next_retry_at ASC LIMIT 1",
    )
    .bind(max_retries)
    .fetch_optional(pool)
    .await?;

    // Get oldest pending
    let oldest_pending: Option<(String,)> = sqlx::query_as(
        "SELECT created_at FROM archives WHERE status = 'pending' ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    Ok(QueueStats {
        pending_count: pending.0,
        processing_count: processing.0,
        failed_awaiting_retry: failed_awaiting.0,
        failed_max_retries: failed_max.0,
        skipped_count: skipped.0,
        complete_count: complete.0,
        next_retry_at: next_retry.map(|r| r.0),
        oldest_pending_at: oldest_pending.map(|r| r.0),
    })
}

/// Get recent failed archives with error details.
pub async fn get_recent_failed_archives(pool: &SqlitePool, limit: i64) -> Result<Vec<Archive>> {
    sqlx::query_as(
        r"
        SELECT * FROM archives
        WHERE status = 'failed' OR status = 'skipped'
        ORDER BY last_attempt_at DESC NULLS LAST, created_at DESC
        LIMIT ?
        ",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch recent failed archives")
}

/// Reset all skipped archives back to pending for retry.
pub async fn reset_skipped_archives(pool: &SqlitePool) -> Result<u64> {
    let result = sqlx::query(
        r"
        UPDATE archives
        SET status = 'pending',
            retry_count = 0,
            next_retry_at = NULL,
            error_message = NULL
        WHERE status = 'skipped'
        ",
    )
    .execute(pool)
    .await
    .context("Failed to reset skipped archives")?;

    Ok(result.rows_affected())
}

/// Reset a single skipped archive back to pending for retry.
pub async fn reset_single_skipped_archive(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result = sqlx::query(
        r"
        UPDATE archives
        SET status = 'pending',
            retry_count = 0,
            next_retry_at = NULL,
            error_message = NULL
        WHERE id = ? AND status = 'skipped'
        ",
    )
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to reset skipped archive")?;

    Ok(result.rows_affected() > 0)
}

/// Get link occurrences with post info for an archive's link.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LinkOccurrenceWithPost {
    pub occurrence_id: i64,
    pub post_id: i64,
    pub in_quote: bool,
    pub context_snippet: Option<String>,
    pub seen_at: String,
    pub post_guid: String,
    pub post_title: Option<String>,
    pub post_author: Option<String>,
}

/// Get all occurrences of a link with post information.
pub async fn get_link_occurrences_with_posts(
    pool: &SqlitePool,
    link_id: i64,
) -> Result<Vec<LinkOccurrenceWithPost>> {
    sqlx::query_as(
        r"
        SELECT
            lo.id as occurrence_id,
            lo.post_id,
            lo.in_quote,
            lo.context_snippet,
            lo.seen_at,
            p.guid as post_guid,
            p.title as post_title,
            p.author as post_author
        FROM link_occurrences lo
        JOIN posts p ON lo.post_id = p.id
        WHERE lo.link_id = ?
        ORDER BY lo.seen_at DESC
        ",
    )
    .bind(link_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch link occurrences with posts")
}

/// Toggle NSFW status for an archive.
pub async fn toggle_archive_nsfw(pool: &SqlitePool, id: i64) -> Result<bool> {
    // Get current status
    let archive: Option<Archive> = sqlx::query_as("SELECT * FROM archives WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;

    let archive = archive.context("Archive not found")?;
    let new_status = !archive.is_nsfw;

    sqlx::query("UPDATE archives SET is_nsfw = ?, nsfw_source = 'manual' WHERE id = ?")
        .bind(new_status)
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to toggle NSFW status")?;

    Ok(new_status)
}

/// Delete an archive and all its artifacts.
pub async fn delete_archive(pool: &SqlitePool, id: i64) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin delete transaction")?;

    // If other artifacts point at this archive's artifacts as duplicates,
    // SQLite will block deletion due to the self-referential FK.
    sqlx::query(
        r"
        UPDATE archive_artifacts
        SET duplicate_of_artifact_id = NULL
        WHERE duplicate_of_artifact_id IN (
            SELECT id FROM archive_artifacts WHERE archive_id = ?
        )
        ",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .context("Failed to clear duplicate references")?;

    // Delete artifacts first (foreign key constraint)
    sqlx::query("DELETE FROM archive_artifacts WHERE archive_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("Failed to delete artifacts")?;

    // Delete the archive
    sqlx::query("DELETE FROM archives WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("Failed to delete archive")?;

    tx.commit().await.context("Failed to commit delete")?;

    Ok(())
}

// ========== Re-archiving ==========

/// Reset an archive for full re-archiving.
///
/// This resets the archive to pending state, clears all results from previous
/// attempts, and deletes associated artifacts. The archive will be processed
/// fresh through the full pipeline (including redirect handling).
pub async fn reset_archive_for_rearchive(pool: &SqlitePool, id: i64) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin reset transaction")?;

    // If other artifacts point at this archive's artifacts as duplicates,
    // SQLite will block deletion due to the self-referential FK.
    sqlx::query(
        r"
        UPDATE archive_artifacts
        SET duplicate_of_artifact_id = NULL
        WHERE duplicate_of_artifact_id IN (
            SELECT id FROM archive_artifacts WHERE archive_id = ?
        )
        ",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .context("Failed to clear duplicate references")?;

    // Delete existing artifacts for this archive
    sqlx::query("DELETE FROM archive_artifacts WHERE archive_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("Failed to delete artifacts")?;

    // Delete existing jobs for this archive
    sqlx::query("DELETE FROM archive_jobs WHERE archive_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("Failed to delete jobs")?;

    // Reset archive to pending state with cleared results
    sqlx::query(
        r"
        UPDATE archives
        SET status = 'pending',
            archived_at = NULL,
            content_title = NULL,
            content_author = NULL,
            content_text = NULL,
            content_type = NULL,
            s3_key_primary = NULL,
            s3_key_thumb = NULL,
            s3_keys_extra = NULL,
            wayback_url = NULL,
            archive_today_url = NULL,
            ipfs_cid = NULL,
            error_message = NULL,
            retry_count = 0,
            next_retry_at = NULL,
            last_attempt_at = NULL,
            is_nsfw = 0,
            nsfw_source = NULL,
            http_status_code = NULL
        WHERE id = ?
        ",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .context("Failed to reset archive")?;

    tx.commit().await.context("Failed to commit reset")?;

    Ok(())
}

// ========== HTTP Status Code ==========

/// Set the HTTP status code for an archive.
pub async fn set_archive_http_status_code(
    pool: &SqlitePool,
    id: i64,
    status_code: u16,
) -> Result<()> {
    sqlx::query("UPDATE archives SET http_status_code = ? WHERE id = ?")
        .bind(i32::from(status_code))
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to set HTTP status code")?;

    Ok(())
}

// ========== Archive Jobs ==========

/// Create a new archive job.
pub async fn create_archive_job(
    pool: &SqlitePool,
    archive_id: i64,
    job_type: ArchiveJobType,
) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO archive_jobs (archive_id, job_type, status)
        VALUES (?, ?, 'pending')
        ",
    )
    .bind(archive_id)
    .bind(job_type.as_str())
    .execute(pool)
    .await
    .context("Failed to create archive job")?;

    Ok(result.last_insert_rowid())
}

/// Set job status to running.
pub async fn set_job_running(pool: &SqlitePool, job_id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE archive_jobs SET status = 'running', started_at = datetime('now') WHERE id = ?",
    )
    .bind(job_id)
    .execute(pool)
    .await
    .context("Failed to set job running")?;

    Ok(())
}

/// Set job status to completed.
pub async fn set_job_completed(
    pool: &SqlitePool,
    job_id: i64,
    metadata: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE archive_jobs SET status = 'completed', completed_at = datetime('now'), metadata = ? WHERE id = ?",
    )
    .bind(metadata)
    .bind(job_id)
    .execute(pool)
    .await
    .context("Failed to set job completed")?;

    Ok(())
}

/// Set job status to failed with error message.
pub async fn set_job_failed(pool: &SqlitePool, job_id: i64, error: &str) -> Result<()> {
    sqlx::query(
        "UPDATE archive_jobs SET status = 'failed', completed_at = datetime('now'), error_message = ? WHERE id = ?",
    )
    .bind(error)
    .bind(job_id)
    .execute(pool)
    .await
    .context("Failed to set job failed")?;

    Ok(())
}

/// Set job status to skipped with optional reason.
pub async fn set_job_skipped(pool: &SqlitePool, job_id: i64, reason: Option<&str>) -> Result<()> {
    sqlx::query(
        "UPDATE archive_jobs SET status = 'skipped', completed_at = datetime('now'), error_message = ? WHERE id = ?",
    )
    .bind(reason)
    .bind(job_id)
    .execute(pool)
    .await
    .context("Failed to set job skipped")?;

    Ok(())
}

/// Get all jobs for an archive.
pub async fn get_jobs_for_archive(pool: &SqlitePool, archive_id: i64) -> Result<Vec<ArchiveJob>> {
    sqlx::query_as(
        r"
        SELECT * FROM archive_jobs
        WHERE archive_id = ?
        ORDER BY created_at ASC
        ",
    )
    .bind(archive_id)
    .fetch_all(pool)
    .await
    .context("Failed to fetch jobs for archive")
}

/// Check if all jobs for an archive succeeded (completed or skipped).
pub async fn all_jobs_succeeded(pool: &SqlitePool, archive_id: i64) -> Result<bool> {
    let row: (i64,) = sqlx::query_as(
        r"
        SELECT COUNT(*) FROM archive_jobs
        WHERE archive_id = ?
          AND status NOT IN ('completed', 'skipped')
        ",
    )
    .bind(archive_id)
    .fetch_one(pool)
    .await?;

    Ok(row.0 == 0)
}

/// Delete all jobs for an archive.
pub async fn delete_jobs_for_archive(pool: &SqlitePool, archive_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM archive_jobs WHERE archive_id = ?")
        .bind(archive_id)
        .execute(pool)
        .await
        .context("Failed to delete jobs for archive")?;

    Ok(())
}

// ========== Exports ==========

/// Count exports from an IP in the last hour (for rate limiting).
pub async fn count_exports_from_ip_last_hour(pool: &SqlitePool, ip: &str) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        r"
        SELECT COUNT(*) FROM exports
        WHERE exported_by_ip = ?
        AND created_at > datetime('now', '-1 hour')
        ",
    )
    .bind(ip)
    .fetch_one(pool)
    .await
    .context("Failed to count exports from IP")?;

    Ok(row.0)
}

/// Record a bulk export download.
pub async fn insert_export(
    pool: &SqlitePool,
    site_domain: &str,
    exported_by_ip: &str,
    archive_count: i64,
    total_size_bytes: i64,
) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO exports (site_domain, exported_by_ip, archive_count, total_size_bytes)
        VALUES (?, ?, ?, ?)
        ",
    )
    .bind(site_domain)
    .bind(exported_by_ip)
    .bind(archive_count)
    .bind(total_size_bytes)
    .execute(pool)
    .await
    .context("Failed to insert export record")?;

    Ok(result.last_insert_rowid())
}

/// Get all complete archives with their artifacts for a specific domain.
/// This is used for bulk export functionality.
pub async fn get_archives_with_artifacts_for_domain(
    pool: &SqlitePool,
    domain: &str,
) -> Result<Vec<(Archive, Link, Vec<ArchiveArtifact>)>> {
    // First, get all complete archives for the domain
    let archives = get_archives_by_domain(pool, domain, 10000, 0).await?;

    let mut result = Vec::new();
    for archive in archives {
        // Get the link
        let link = match get_link(pool, archive.link_id).await? {
            Some(l) => l,
            None => continue, // Skip if link not found
        };

        // Get artifacts
        let artifacts = get_artifacts_for_archive(pool, archive.id).await?;

        result.push((archive, link, artifacts));
    }

    Ok(result)
}
