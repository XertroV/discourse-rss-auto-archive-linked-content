use anyhow::{Context, Result};
use sqlx::SqlitePool;

use super::models::*;

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
pub async fn link_occurrence_exists(
    pool: &SqlitePool,
    link_id: i64,
    post_id: i64,
) -> Result<bool> {
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

/// Update archive as failed.
pub async fn set_archive_failed(pool: &SqlitePool, id: i64, error: &str) -> Result<()> {
    sqlx::query(
        r"
        UPDATE archives
        SET status = 'failed',
            error_message = ?,
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

/// Get failed archives eligible for retry (retry_count < 3).
pub async fn get_failed_archives_for_retry(pool: &SqlitePool, limit: i64) -> Result<Vec<Archive>> {
    sqlx::query_as(
        r"
        SELECT * FROM archives
        WHERE status = 'failed' AND retry_count < 3
        ORDER BY created_at ASC
        LIMIT ?
        ",
    )
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
