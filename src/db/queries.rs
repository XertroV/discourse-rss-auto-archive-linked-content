use anyhow::{Context, Result};
use sqlx::{Row, SqlitePool};
use std::collections::{HashMap, HashSet};

use super::models::{
    Archive, ArchiveArtifact, ArchiveDisplay, ArchiveJob, ArchiveJobType, AuditEvent, Link,
    LinkOccurrence, NewLink, NewLinkOccurrence, NewPost, NewSubmission, Post, Session, Submission,
    SubtitleLanguage, ThreadArchiveJob, ThreadDisplay, User, VideoFile,
};

// ========== Source Filter Helpers ==========

/// Represents a domain filter condition with its SQL clause and bind values.
struct DomainFilter {
    /// SQL WHERE clause fragment (e.g., "(l.domain = ? OR l.domain LIKE ?)")
    sql: String,
    /// Values to bind in order
    values: Vec<String>,
}

/// Get domain filter SQL and bind values for a source name.
/// Returns a proper domain match that handles exact domain and subdomains.
fn get_domain_filter(source: &str) -> DomainFilter {
    // Map source names to their domains
    // For most sources, match exact domain or subdomains (e.g., "reddit.com" or "*.reddit.com")
    // For twitter, match both x.com and twitter.com for backwards compatibility
    match source {
        "twitter" => DomainFilter {
            sql: "(l.domain = ? OR l.domain LIKE ? OR l.domain = ? OR l.domain LIKE ?)".to_string(),
            values: vec![
                "x.com".to_string(),
                "%.x.com".to_string(),
                "twitter.com".to_string(),
                "%.twitter.com".to_string(),
            ],
        },
        "reddit" => DomainFilter {
            sql: "(l.domain = ? OR l.domain LIKE ?)".to_string(),
            values: vec!["reddit.com".to_string(), "%.reddit.com".to_string()],
        },
        "youtube" => DomainFilter {
            sql: "(l.domain = ? OR l.domain LIKE ?)".to_string(),
            values: vec!["youtube.com".to_string(), "%.youtube.com".to_string()],
        },
        "tiktok" => DomainFilter {
            sql: "(l.domain = ? OR l.domain LIKE ?)".to_string(),
            values: vec!["tiktok.com".to_string(), "%.tiktok.com".to_string()],
        },
        // For unknown sources, try to match as a domain directly
        _ => DomainFilter {
            sql: "(l.domain = ? OR l.domain LIKE ?)".to_string(),
            values: vec![source.to_string(), format!("%.{}", source)],
        },
    }
}

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

/// Get all threads (posts) with aggregated statistics.
///
/// Returns threads sorted by the specified order with pagination.
/// Sort options: "name" (title), "created" (published_at), "updated" (last_archived_at)
pub async fn get_all_threads(
    pool: &SqlitePool,
    sort_by: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<ThreadDisplay>> {
    // Fetch per-post thread stats, then aggregate by thread/topic so we only
    // show one card per thread regardless of post count.
    let rows: Vec<ThreadDisplay> = sqlx::query_as(
        r"
        SELECT
            p.guid,
            p.title,
            p.author,
            p.discourse_url,
            p.published_at,
            COUNT(DISTINCT lo.link_id) as link_count,
            COUNT(DISTINCT a.id) as archive_count,
            MAX(a.archived_at) as last_archived_at
        FROM posts p
        LEFT JOIN link_occurrences lo ON p.id = lo.post_id
        LEFT JOIN links l ON lo.link_id = l.id
        LEFT JOIN archives a ON l.id = a.link_id
        GROUP BY p.id
        ORDER BY p.published_at DESC
        ",
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch thread rows")?;

    // Aggregate rows by thread key.
    let mut threads: HashMap<String, ThreadDisplay> = HashMap::new();

    for row in rows {
        let key = thread_key_from_url(&row.discourse_url);

        threads
            .entry(key)
            .and_modify(|agg| {
                agg.link_count += row.link_count;
                agg.archive_count += row.archive_count;
                agg.last_archived_at =
                    max_opt_string(agg.last_archived_at.take(), row.last_archived_at.clone());
                agg.published_at =
                    min_opt_string(agg.published_at.take(), row.published_at.clone());
            })
            .or_insert(row);
    }

    let mut threads: Vec<ThreadDisplay> = threads.into_values().collect();

    // Sort according to requested order, nulls last for dates.
    match sort_by {
        "name" => threads.sort_by(|a, b| {
            a.title
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .cmp(&b.title.as_deref().unwrap_or("").to_lowercase())
        }),
        "updated" => threads.sort_by(|a, b| cmp_opt_desc(&a.last_archived_at, &b.last_archived_at)),
        _ => threads.sort_by(|a, b| cmp_opt_desc(&a.published_at, &b.published_at)),
    }

    // Apply pagination after aggregation.
    let start = offset.max(0) as usize;
    let end = (start + limit.max(0) as usize).min(threads.len());

    Ok(if start >= threads.len() {
        Vec::new()
    } else {
        threads[start..end].to_vec()
    })
}

/// Count total number of unique threads.
/// This performs the same aggregation as get_all_threads() to count unique thread keys.
pub async fn count_all_threads(pool: &SqlitePool) -> Result<i64> {
    // Fetch all post discourse URLs to aggregate by thread key
    let rows: Vec<(String,)> = sqlx::query_as(
        r"
        SELECT DISTINCT p.discourse_url
        FROM posts p
        ",
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch post URLs for thread count")?;

    // Count unique thread keys (same aggregation as get_all_threads)
    let mut thread_keys: HashSet<String> = HashSet::new();
    for (url,) in rows {
        let key = thread_key_from_url(&url);
        thread_keys.insert(key);
    }

    Ok(thread_keys.len() as i64)
}

/// Fetch all posts that belong to the given thread key (host + topic id/path).
pub async fn get_posts_by_thread_key(pool: &SqlitePool, thread_key: &str) -> Result<Vec<Post>> {
    let Some((host, rest)) = thread_key.split_once(':') else {
        return Ok(Vec::new());
    };

    // Primary Discourse pattern: /t/<slug>/<topic_id>/<post_no?>
    if rest.chars().all(|c| c.is_ascii_digit()) {
        let pattern_base = format!("%://{host}/t/%/{rest}");
        let pattern_with_post = format!("%://{host}/t/%/{rest}/%");

        return sqlx::query_as(
            r#"
            SELECT * FROM posts
            WHERE discourse_url LIKE ? OR discourse_url LIKE ?
            ORDER BY published_at IS NULL, published_at, processed_at
            "#,
        )
        .bind(pattern_base)
        .bind(pattern_with_post)
        .fetch_all(pool)
        .await
        .context("Failed to fetch posts for thread key");
    }

    // Fallback: match on host + path prefix.
    let pattern = format!("%://{host}{rest}%");
    sqlx::query_as(
        r#"
        SELECT * FROM posts
        WHERE discourse_url LIKE ?
        ORDER BY published_at IS NULL, published_at, processed_at
        "#,
    )
    .bind(pattern)
    .fetch_all(pool)
    .await
    .context("Failed to fetch posts for thread key")
}

pub fn thread_key_from_url(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        let host = parsed.host_str().unwrap_or("");
        let segments: Vec<_> = parsed
            .path_segments()
            .map_or_else(Vec::new, std::iter::Iterator::collect);

        if segments.len() >= 3 && segments[0] == "t" {
            // /t/<slug>/<topic_id>/<post_no?>
            let topic_id = segments[2];
            return format!("{host}:{topic_id}");
        }

        return format!("{host}:{}", parsed.path());
    }

    // Fallback to the raw URL if parsing fails
    url.to_string()
}

/// Extract numeric topic ID from a thread key.
///
/// # Examples
///
/// ```
/// use discourse_rss_auto_archive_linked_content::db::queries::extract_topic_id_from_thread_key;
///
/// assert_eq!(
///     extract_topic_id_from_thread_key("discuss.example.com:2147"),
///     Some(2147)
/// );
/// assert_eq!(
///     extract_topic_id_from_thread_key("example.com:/path"),
///     None
/// );
/// ```
pub fn extract_topic_id_from_thread_key(thread_key: &str) -> Option<i64> {
    let (_host, rest) = thread_key.split_once(':')?;
    rest.parse::<i64>().ok()
}

/// Fetch all posts matching a numeric topic ID across all domains.
///
/// This queries posts by matching the discourse_url against patterns for
/// standard Discourse topic URLs like `/t/<slug>/<topic_id>` and
/// `/t/<slug>/<topic_id>/<post_number>`.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn get_posts_by_topic_id(pool: &SqlitePool, topic_id: i64) -> Result<Vec<Post>> {
    let pattern1 = format!("%/t/%/{}", topic_id);
    let pattern2 = format!("%/t/%/{}/%", topic_id);

    sqlx::query_as(
        r#"
        SELECT * FROM posts
        WHERE discourse_url LIKE ? OR discourse_url LIKE ?
        ORDER BY published_at IS NULL, published_at, processed_at
        "#,
    )
    .bind(pattern1)
    .bind(pattern2)
    .fetch_all(pool)
    .await
    .context("Failed to fetch posts for topic_id")
}

fn cmp_opt_desc(a: &Option<String>, b: &Option<String>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.cmp(a),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn max_opt_string(a: Option<String>, b: Option<String>) -> Option<String> {
    match (a, b) {
        (Some(a), Some(b)) => Some(std::cmp::max(a, b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn min_opt_string(a: Option<String>, b: Option<String>) -> Option<String> {
    match (a, b) {
        (Some(a), Some(b)) => Some(std::cmp::min(a, b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
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
/// Create a pending archive for a link.
///
/// Uses INSERT OR IGNORE to handle race conditions where multiple threads
/// try to create archives for the same link simultaneously. Returns the ID
/// of either the newly created or existing archive.
///
/// This is safe due to the UNIQUE constraint on archives(link_id) added in
/// migration v23, following the same pattern as video_files deduplication.
pub async fn create_pending_archive(
    pool: &SqlitePool,
    link_id: i64,
    post_date: Option<&str>,
) -> Result<i64> {
    // Try to insert (will be ignored if archive already exists for this link_id)
    sqlx::query(
        "INSERT OR IGNORE INTO archives (link_id, status, post_date) VALUES (?, 'pending', ?)",
    )
    .bind(link_id)
    .bind(post_date)
    .execute(pool)
    .await
    .context("Failed to insert pending archive")?;

    // Fetch the archive (either newly created or existing)
    // This is guaranteed to exist due to the INSERT OR IGNORE above
    let archive = get_archive_by_link_id(pool, link_id)
        .await
        .context("Failed to fetch archive after insert")?
        .expect("Archive should exist after INSERT OR IGNORE");

    Ok(archive.id)
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

/// Mark archive as requiring authentication (can be retried when cookies are configured).
pub async fn set_archive_auth_required(pool: &SqlitePool, id: i64, error: &str) -> Result<()> {
    sqlx::query(
        r"
        UPDATE archives
        SET status = 'auth_required',
            error_message = ?,
            last_attempt_at = datetime('now')
        WHERE id = ?
        ",
    )
    .bind(error)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to set archive auth_required")?;

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

/// Get archives requiring authentication.
///
/// Returns archives where status = 'auth_required'.
/// These archives failed due to authentication requirements and can be retried
/// when cookies are configured.
pub async fn get_archives_needing_auth(pool: &SqlitePool, limit: i64) -> Result<Vec<Archive>> {
    sqlx::query_as(
        r"
        SELECT * FROM archives
        WHERE status = 'auth_required'
        ORDER BY last_attempt_at DESC NULLS LAST, created_at ASC
        LIMIT ?
        ",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch auth-required archives")
}

/// Get recent archives for the home page.
pub async fn get_recent_archives(pool: &SqlitePool, limit: i64) -> Result<Vec<Archive>> {
    sqlx::query_as(
        r"
        SELECT * FROM archives
        WHERE status = 'complete'
        ORDER BY COALESCE(post_date, archived_at, created_at) DESC
        LIMIT ?
        ",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch recent archives")
}

/// Get recent archives with NSFW filter and pagination.
pub async fn get_recent_archives_filtered(
    pool: &SqlitePool,
    limit: i64,
    offset: i64,
    nsfw_filter: Option<bool>,
) -> Result<Vec<Archive>> {
    get_recent_archives_filtered_full(pool, limit, offset, nsfw_filter, None).await
}

/// Get recent archives with NSFW and content_type filters and pagination.
pub async fn get_recent_archives_filtered_full(
    pool: &SqlitePool,
    limit: i64,
    offset: i64,
    nsfw_filter: Option<bool>,
    content_type: Option<&str>,
) -> Result<Vec<Archive>> {
    // Build WHERE clause dynamically based on filters
    let mut where_clauses = vec!["status = 'complete'".to_string()];

    match nsfw_filter {
        Some(true) => where_clauses.push("is_nsfw = 1".to_string()),
        Some(false) => where_clauses.push("(is_nsfw = 0 OR is_nsfw IS NULL)".to_string()),
        None => {}
    }

    if content_type.is_some() {
        where_clauses.push("content_type = ?".to_string());
    }

    let where_clause = where_clauses.join(" AND ");
    let sql = format!(
        "SELECT * FROM archives WHERE {} ORDER BY COALESCE(post_date, archived_at, created_at) DESC LIMIT ? OFFSET ?",
        where_clause
    );

    let mut query = sqlx::query_as(&sql);

    // Bind content_type if present (bind in order of ? placeholders)
    if let Some(ct) = content_type {
        query = query.bind(ct);
    }

    query
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .context("Failed to fetch filtered archives")
}

/// Get total count of archives with NSFW filter.
pub async fn get_archives_count(pool: &SqlitePool, nsfw_filter: Option<bool>) -> Result<i64> {
    let count: (i64,) = match nsfw_filter {
        Some(true) => {
            sqlx::query_as("SELECT COUNT(*) FROM archives WHERE status = 'complete' AND is_nsfw = 1")
                .fetch_one(pool)
                .await
                .context("Failed to count NSFW archives")?
        }
        Some(false) => {
            sqlx::query_as("SELECT COUNT(*) FROM archives WHERE status = 'complete' AND (is_nsfw = 0 OR is_nsfw IS NULL)")
                .fetch_one(pool)
                .await
                .context("Failed to count SFW archives")?
        }
        None => {
            sqlx::query_as("SELECT COUNT(*) FROM archives WHERE status = 'complete'")
                .fetch_one(pool)
                .await
                .context("Failed to count archives")?
        }
    };
    Ok(count.0)
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
                ORDER BY COALESCE(a.post_date, a.archived_at, a.created_at) DESC
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
                ORDER BY COALESCE(a.post_date, a.archived_at, a.created_at) DESC
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
                ORDER BY COALESCE(post_date, archived_at, created_at) DESC
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
    let sanitized = crate::db::sanitize_fts_query(query);
    if sanitized.is_empty() && query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let url_pattern = format!("%{}%", query.trim());

    // If FTS sanitization resulted in an empty string, only search URLs
    if sanitized.is_empty() {
        return sqlx::query_as(
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
            WHERE l.original_url LIKE ?
            GROUP BY a.id, a.link_id, a.status, a.archived_at,
                     a.content_title, a.content_author, a.content_type,
                     a.is_nsfw, a.error_message, a.retry_count,
                     l.original_url, l.domain
            ORDER BY a.archived_at DESC
            LIMIT ?
            ",
        )
        .bind(url_pattern)
        .bind(limit)
        .fetch_all(pool)
        .await
        .context("Failed to search archives by URL");
    }

    // Search both FTS and URLs using UNION to combine results
    sqlx::query_as(
        r"
        WITH combined_results AS (
            SELECT
                a.id, a.link_id, a.status, a.archived_at,
                a.content_title, a.content_author, a.content_type,
                a.is_nsfw, a.error_message, a.retry_count,
                l.original_url, l.domain,
                COALESCE(SUM(aa.size_bytes), 0) as total_size_bytes,
                COALESCE(rank, 0) as search_rank
            FROM archives a
            JOIN links l ON a.link_id = l.id
            JOIN archives_fts ON a.id = archives_fts.rowid
            LEFT JOIN archive_artifacts aa ON a.id = aa.archive_id
            WHERE archives_fts MATCH ?
            GROUP BY a.id, a.link_id, a.status, a.archived_at,
                     a.content_title, a.content_author, a.content_type,
                     a.is_nsfw, a.error_message, a.retry_count,
                     l.original_url, l.domain
            UNION
            SELECT
                a.id, a.link_id, a.status, a.archived_at,
                a.content_title, a.content_author, a.content_type,
                a.is_nsfw, a.error_message, a.retry_count,
                l.original_url, l.domain,
                COALESCE(SUM(aa.size_bytes), 0) as total_size_bytes,
                0 as search_rank
            FROM archives a
            JOIN links l ON a.link_id = l.id
            LEFT JOIN archive_artifacts aa ON a.id = aa.archive_id
            WHERE l.original_url LIKE ?
            AND NOT EXISTS (
                SELECT 1 FROM archives_fts WHERE archives_fts.rowid = a.id AND archives_fts MATCH ?
            )
            GROUP BY a.id, a.link_id, a.status, a.archived_at,
                     a.content_title, a.content_author, a.content_type,
                     a.is_nsfw, a.error_message, a.retry_count,
                     l.original_url, l.domain
        )
        SELECT
            id, link_id, status, archived_at,
            content_title, content_author, content_type,
            is_nsfw, error_message, retry_count,
            original_url, domain, total_size_bytes
        FROM combined_results
        ORDER BY search_rank DESC
        LIMIT ?
        ",
    )
    .bind(&sanitized)
    .bind(&url_pattern)
    .bind(&sanitized)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to search archives with links")
}

/// Get recent archives with link info for display, with optional content_type filter.
pub async fn get_recent_archives_display_filtered(
    pool: &SqlitePool,
    limit: i64,
    content_type: Option<&str>,
    source: Option<&str>,
) -> Result<Vec<ArchiveDisplay>> {
    // Build WHERE clause dynamically based on filters
    let mut where_clauses: Vec<String> = Vec::new();
    let domain_filter = source.map(get_domain_filter);

    if content_type.is_some() {
        where_clauses.push("a.content_type = ?".to_string());
    }

    if let Some(ref df) = domain_filter {
        where_clauses.push(df.sql.clone());
    }

    if where_clauses.is_empty() {
        return get_recent_archives_display(pool, limit).await;
    }

    let where_clause = where_clauses.join(" AND ");
    let sql = format!(
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
        WHERE {}
        GROUP BY a.id, a.link_id, a.status, a.archived_at,
                 a.content_title, a.content_author, a.content_type,
                 a.is_nsfw, a.error_message, a.retry_count,
                 l.original_url, l.domain
        ORDER BY COALESCE(a.archived_at, a.last_attempt_at, a.created_at) DESC
        LIMIT ?
        ",
        where_clause
    );

    let mut query = sqlx::query_as(&sql);

    // Bind parameters in order
    if let Some(ct) = content_type {
        query = query.bind(ct);
    }

    if let Some(ref df) = domain_filter {
        for value in &df.values {
            query = query.bind(value);
        }
    }

    query
        .bind(limit)
        .fetch_all(pool)
        .await
        .context("Failed to fetch recent archives with filters")
}

/// Get all archives for table view with pagination support.
/// Orders by archive ID descending (newest first).
pub async fn get_all_archives_table_view(
    pool: &SqlitePool,
    limit: i64,
    offset: i64,
    content_type: Option<&str>,
    source: Option<&str>,
) -> Result<Vec<ArchiveDisplay>> {
    // Build WHERE clause dynamically based on filters
    let mut where_clauses: Vec<String> = Vec::new();
    let domain_filter = source.map(get_domain_filter);

    if content_type.is_some() {
        where_clauses.push("a.content_type = ?".to_string());
    }

    if let Some(ref df) = domain_filter {
        where_clauses.push(df.sql.clone());
    }

    let where_clause = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let sql = format!(
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
        {}
        GROUP BY a.id, a.link_id, a.status, a.archived_at,
                 a.content_title, a.content_author, a.content_type,
                 a.is_nsfw, a.error_message, a.retry_count,
                 l.original_url, l.domain
        ORDER BY a.id DESC
        LIMIT ? OFFSET ?
        ",
        where_clause
    );

    let mut query = sqlx::query_as(&sql);

    // Bind parameters in order
    if let Some(ct) = content_type {
        query = query.bind(ct);
    }

    if let Some(ref df) = domain_filter {
        for value in &df.values {
            query = query.bind(value);
        }
    }

    query
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .context("Failed to fetch archives for table view")
}

/// Count all archives with optional filters for pagination calculation.
pub async fn count_all_archives_filtered(
    pool: &SqlitePool,
    content_type: Option<&str>,
    source: Option<&str>,
) -> Result<i64> {
    // Build WHERE clause dynamically based on filters
    let mut where_clauses: Vec<String> = Vec::new();
    let domain_filter = source.map(get_domain_filter);

    if content_type.is_some() {
        where_clauses.push("a.content_type = ?".to_string());
    }

    if let Some(ref df) = domain_filter {
        where_clauses.push(df.sql.clone());
    }

    let where_clause = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let sql = format!(
        r"
        SELECT COUNT(DISTINCT a.id)
        FROM archives a
        JOIN links l ON a.link_id = l.id
        {}
        ",
        where_clause
    );

    let mut query = sqlx::query_scalar(&sql);

    // Bind parameters in order
    if let Some(ct) = content_type {
        query = query.bind(ct);
    }

    if let Some(ref df) = domain_filter {
        for value in &df.values {
            query = query.bind(value);
        }
    }

    query
        .fetch_one(pool)
        .await
        .context("Failed to count archives")
}

/// Search archives with link info for display, with optional content_type and source filters.
pub async fn search_archives_display_filtered(
    pool: &SqlitePool,
    query: &str,
    limit: i64,
    content_type: Option<&str>,
    source: Option<&str>,
) -> Result<Vec<ArchiveDisplay>> {
    let sanitized = crate::db::sanitize_fts_query(query);
    if sanitized.is_empty() && query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let url_pattern = format!("%{}%", query.trim());

    // Build additional filter clauses
    let domain_filter = source.map(get_domain_filter);
    let mut additional_filters = Vec::new();

    if content_type.is_some() {
        additional_filters.push("a.content_type = ?".to_string());
    }

    if let Some(ref df) = domain_filter {
        additional_filters.push(df.sql.clone());
    }

    let additional_where = if additional_filters.is_empty() {
        String::new()
    } else {
        format!("AND {}", additional_filters.join(" AND "))
    };

    // Use UNION for FTS + URL search, or simple query for URL-only
    let sql = if sanitized.is_empty() {
        // URL-only search with filters
        format!(
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
            WHERE l.original_url LIKE ? {}
            GROUP BY a.id, a.link_id, a.status, a.archived_at,
                     a.content_title, a.content_author, a.content_type,
                     a.is_nsfw, a.error_message, a.retry_count,
                     l.original_url, l.domain
            ORDER BY a.archived_at DESC
            LIMIT ?
            ",
            additional_where
        )
    } else {
        // FTS + URL search with filters using UNION
        format!(
            r"
            WITH combined_results AS (
                SELECT
                    a.id, a.link_id, a.status, a.archived_at,
                    a.content_title, a.content_author, a.content_type,
                    a.is_nsfw, a.error_message, a.retry_count,
                    l.original_url, l.domain,
                    COALESCE(SUM(aa.size_bytes), 0) as total_size_bytes,
                    COALESCE(rank, 0) as search_rank
                FROM archives a
                JOIN links l ON a.link_id = l.id
                JOIN archives_fts ON a.id = archives_fts.rowid
                LEFT JOIN archive_artifacts aa ON a.id = aa.archive_id
                WHERE archives_fts MATCH ? {}
                GROUP BY a.id, a.link_id, a.status, a.archived_at,
                         a.content_title, a.content_author, a.content_type,
                         a.is_nsfw, a.error_message, a.retry_count,
                         l.original_url, l.domain
                UNION
                SELECT
                    a.id, a.link_id, a.status, a.archived_at,
                    a.content_title, a.content_author, a.content_type,
                    a.is_nsfw, a.error_message, a.retry_count,
                    l.original_url, l.domain,
                    COALESCE(SUM(aa.size_bytes), 0) as total_size_bytes,
                    0 as search_rank
                FROM archives a
                JOIN links l ON a.link_id = l.id
                LEFT JOIN archive_artifacts aa ON a.id = aa.archive_id
                WHERE l.original_url LIKE ? {}
                AND NOT EXISTS (
                    SELECT 1 FROM archives_fts WHERE archives_fts.rowid = a.id AND archives_fts MATCH ?
                )
                GROUP BY a.id, a.link_id, a.status, a.archived_at,
                         a.content_title, a.content_author, a.content_type,
                         a.is_nsfw, a.error_message, a.retry_count,
                         l.original_url, l.domain
            )
            SELECT
                id, link_id, status, archived_at,
                content_title, content_author, content_type,
                is_nsfw, error_message, retry_count,
                original_url, domain, total_size_bytes
            FROM combined_results
            ORDER BY search_rank DESC
            LIMIT ?
            ",
            additional_where, additional_where
        )
    };

    let mut sql_query = sqlx::query_as(&sql);

    // Bind parameters in order
    if !sanitized.is_empty() {
        // For UNION query: sanitized, url_pattern, content_type, domain_filter, sanitized (for NOT EXISTS), limit
        sql_query = sql_query.bind(&sanitized);
        sql_query = sql_query.bind(&url_pattern);

        // Bind filter parameters for first part of UNION
        if let Some(ct) = content_type {
            sql_query = sql_query.bind(ct);
        }
        if let Some(ref df) = domain_filter {
            for value in &df.values {
                sql_query = sql_query.bind(value);
            }
        }

        // Bind filter parameters for second part of UNION
        if let Some(ct) = content_type {
            sql_query = sql_query.bind(ct);
        }
        if let Some(ref df) = domain_filter {
            for value in &df.values {
                sql_query = sql_query.bind(value);
            }
        }

        // Bind sanitized again for NOT EXISTS clause
        sql_query = sql_query.bind(&sanitized);
    } else {
        // For URL-only query: url_pattern, content_type, domain_filter, limit
        sql_query = sql_query.bind(&url_pattern);

        if let Some(ct) = content_type {
            sql_query = sql_query.bind(ct);
        }
        if let Some(ref df) = domain_filter {
            for value in &df.values {
                sql_query = sql_query.bind(value);
            }
        }
    }

    sql_query
        .bind(limit)
        .fetch_all(pool)
        .await
        .context("Failed to search archives with filters")
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
    get_archives_for_posts_display(pool, &[post_id]).await
}

/// Get archives for multiple posts with link info for display.
pub async fn get_archives_for_posts_display(
    pool: &SqlitePool,
    post_ids: &[i64],
) -> Result<Vec<ArchiveDisplay>> {
    if post_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = std::iter::repeat_n("?", post_ids.len())
        .collect::<Vec<_>>()
        .join(",");

    let query = format!(
        r#"
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
        WHERE lo.post_id IN ({placeholders})
        GROUP BY a.id, a.link_id, a.status, a.archived_at,
                 a.content_title, a.content_author, a.content_type,
                 a.is_nsfw, a.error_message, a.retry_count,
                 l.original_url, l.domain
        ORDER BY COALESCE(a.post_date, a.archived_at, a.created_at) DESC
        "#
    );

    let mut query = sqlx::query_as(&query);
    for id in post_ids {
        query = query.bind(id);
    }

    query
        .fetch_all(pool)
        .await
        .context("Failed to fetch archives for posts with links")
}

/// Get all archives created for a thread archive job.
///
/// This queries archives through the relationship:
/// ThreadArchiveJob.thread_url → Posts → LinkOccurrences → Links → Archives
pub async fn get_archives_for_thread_job(
    pool: &SqlitePool,
    job: &ThreadArchiveJob,
) -> Result<Vec<ArchiveDisplay>> {
    // Extract thread key from job.thread_url
    let thread_key = thread_key_from_url(&job.thread_url);

    // Get all posts for this thread
    let posts = get_posts_by_thread_key(pool, &thread_key).await?;

    if posts.is_empty() {
        return Ok(Vec::new());
    }

    // Get post IDs
    let post_ids: Vec<i64> = posts.iter().map(|p| p.id).collect();

    // Reuse existing helper to get archives
    get_archives_for_posts_display(pool, &post_ids).await
}

/// Count archives by status for a specific thread.
pub async fn count_archives_by_status_for_thread(
    pool: &SqlitePool,
    thread_url: &str,
) -> Result<HashMap<String, i64>> {
    let thread_key = thread_key_from_url(thread_url);
    let pattern = format!("%{}%", thread_key);

    let results: Vec<(String, i64)> = sqlx::query_as(
        r#"
        SELECT a.status, COUNT(DISTINCT a.id) as count
        FROM posts p
        JOIN link_occurrences lo ON p.id = lo.post_id
        JOIN links l ON lo.link_id = l.id
        JOIN archives a ON l.id = a.link_id
        WHERE p.discourse_url LIKE ?
        GROUP BY a.status
        ORDER BY a.status
        "#,
    )
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    Ok(results.into_iter().collect())
}

/// Search archives using FTS.
pub async fn search_archives(pool: &SqlitePool, query: &str, limit: i64) -> Result<Vec<Archive>> {
    let sanitized = crate::db::sanitize_fts_query(query);
    if sanitized.is_empty() && query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let url_pattern = format!("%{}%", query.trim());

    // If FTS sanitization resulted in an empty string, only search URLs
    if sanitized.is_empty() {
        return sqlx::query_as(
            r"
            SELECT archives.* FROM archives
            JOIN links l ON archives.link_id = l.id
            WHERE l.original_url LIKE ?
            ORDER BY archives.archived_at DESC
            LIMIT ?
            ",
        )
        .bind(url_pattern)
        .bind(limit)
        .fetch_all(pool)
        .await
        .context("Failed to search archives by URL");
    }

    // Search both FTS and URLs using UNION
    sqlx::query_as(
        r"
        SELECT * FROM (
            SELECT archives.* FROM archives
            JOIN links l ON archives.link_id = l.id
            JOIN archives_fts ON archives.id = archives_fts.rowid
            WHERE archives_fts MATCH ?
            UNION
            SELECT archives.* FROM archives
            JOIN links l ON archives.link_id = l.id
            WHERE l.original_url LIKE ?
            AND NOT EXISTS (
                SELECT 1 FROM archives_fts WHERE archives_fts.rowid = archives.id AND archives_fts MATCH ?
            )
        )
        LIMIT ?
        ",
    )
    .bind(&sanitized)
    .bind(&url_pattern)
    .bind(&sanitized)
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
    search_archives_filtered_full(pool, query, limit, nsfw_filter, None).await
}

/// Search archives with NSFW and content_type filters.
pub async fn search_archives_filtered_full(
    pool: &SqlitePool,
    query: &str,
    limit: i64,
    nsfw_filter: Option<bool>,
    content_type: Option<&str>,
) -> Result<Vec<Archive>> {
    let sanitized = crate::db::sanitize_fts_query(query);
    if sanitized.is_empty() && query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let url_pattern = format!("%{}%", query.trim());

    // Build additional filter clauses
    let mut additional_filters = Vec::new();

    match nsfw_filter {
        Some(true) => additional_filters.push("archives.is_nsfw = 1".to_string()),
        Some(false) => additional_filters
            .push("(archives.is_nsfw = 0 OR archives.is_nsfw IS NULL)".to_string()),
        None => {}
    }

    if content_type.is_some() {
        additional_filters.push("archives.content_type = ?".to_string());
    }

    let additional_where = if additional_filters.is_empty() {
        String::new()
    } else {
        format!("AND {}", additional_filters.join(" AND "))
    };

    // Use UNION for FTS + URL search, or simple query for URL-only
    let sql = if sanitized.is_empty() {
        // URL-only search with filters
        format!(
            "SELECT archives.* FROM archives JOIN links l ON archives.link_id = l.id WHERE l.original_url LIKE ? {} ORDER BY archives.archived_at DESC LIMIT ?",
            additional_where
        )
    } else {
        // FTS + URL search with filters using UNION
        format!(
            "SELECT * FROM (SELECT archives.* FROM archives JOIN links l ON archives.link_id = l.id JOIN archives_fts ON archives.id = archives_fts.rowid WHERE archives_fts MATCH ? {} UNION SELECT archives.* FROM archives JOIN links l ON archives.link_id = l.id WHERE l.original_url LIKE ? {} AND NOT EXISTS (SELECT 1 FROM archives_fts WHERE archives_fts.rowid = archives.id AND archives_fts MATCH ?)) LIMIT ?",
            additional_where, additional_where
        )
    };

    let mut q = sqlx::query_as(&sql);

    // Bind parameters in order
    if !sanitized.is_empty() {
        q = q.bind(&sanitized);
        if let Some(ct) = content_type {
            q = q.bind(ct);
        }
        q = q.bind(&url_pattern);
        if let Some(ct) = content_type {
            q = q.bind(ct);
        }
        q = q.bind(&sanitized);
    } else {
        q = q.bind(&url_pattern);
        if let Some(ct) = content_type {
            q = q.bind(ct);
        }
    }

    q.bind(limit)
        .fetch_all(pool)
        .await
        .context("Failed to search filtered archives")
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

/// Insert a new archive artifact with metadata.
///
/// # Errors
///
/// Returns an error if the database insert fails.
pub async fn insert_artifact_with_metadata(
    pool: &SqlitePool,
    archive_id: i64,
    kind: &str,
    s3_key: &str,
    content_type: Option<&str>,
    size_bytes: Option<i64>,
    sha256: Option<&str>,
    metadata: Option<&str>,
) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO archive_artifacts (archive_id, kind, s3_key, content_type, size_bytes, sha256, metadata)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ",
    )
    .bind(archive_id)
    .bind(kind)
    .bind(s3_key)
    .bind(content_type)
    .bind(size_bytes)
    .bind(sha256)
    .bind(metadata)
    .execute(pool)
    .await
    .context("Failed to insert artifact with metadata")?;

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

/// Get distinct artifact kinds that exist for an archive.
pub async fn get_existing_artifact_kinds(
    pool: &SqlitePool,
    archive_id: i64,
) -> Result<std::collections::HashSet<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT DISTINCT kind FROM archive_artifacts WHERE archive_id = ?")
            .bind(archive_id)
            .fetch_all(pool)
            .await
            .context("Failed to get existing artifact kinds")?;

    Ok(rows.into_iter().map(|(k,)| k).collect())
}

/// Check if a specific artifact kind exists for an archive.
pub async fn has_artifact_kind(pool: &SqlitePool, archive_id: i64, kind: &str) -> Result<bool> {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM archive_artifacts WHERE archive_id = ? AND kind = ?")
            .bind(archive_id)
            .bind(kind)
            .fetch_one(pool)
            .await?;

    Ok(row.0 > 0)
}

/// Check if an archive has missing artifacts based on its content type.
/// For video content (e.g., YouTube), checks for subtitles and transcripts.
/// Returns true if any expected artifacts are missing.
pub async fn has_missing_artifacts(pool: &SqlitePool, archive_id: i64) -> Result<bool> {
    use crate::db::models::ArtifactKind;

    // Get the archive to check its content type and URL
    let archive = get_archive(pool, archive_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Archive {} not found", archive_id))?;

    // Get the link to check the URL for platform detection
    let link = get_link(pool, archive.link_id).await?;
    let url = link
        .as_ref()
        .and_then(|l| l.final_url.as_deref().or(Some(&l.normalized_url)))
        .unwrap_or("");

    // Only check video content for now (YouTube, TikTok videos)
    if archive.content_type.as_deref() != Some("video") {
        return Ok(false);
    }

    // Video content should have subtitles, transcript, and comments (platform-dependent)
    let has_subtitles =
        has_artifact_kind(pool, archive_id, ArtifactKind::Subtitles.as_str()).await?;
    let has_transcript =
        has_artifact_kind(pool, archive_id, ArtifactKind::Transcript.as_str()).await?;
    let has_comments = has_artifact_kind(pool, archive_id, ArtifactKind::Comments.as_str()).await?;

    // Check if comments are missing on supported platforms (YouTube, TikTok)
    let is_comments_platform =
        url.contains("youtube.com") || url.contains("youtu.be") || url.contains("tiktok.com");

    // Missing if any of these are missing:
    // - Subtitles or transcript (for all videos)
    // - Comments (for YouTube/TikTok videos)
    Ok(!has_subtitles || !has_transcript || (is_comments_platform && !has_comments))
}

/// Find an artifact by its S3 key.
pub async fn find_artifact_by_s3_key(
    pool: &SqlitePool,
    s3_key: &str,
) -> Result<Option<ArchiveArtifact>> {
    sqlx::query_as("SELECT * FROM archive_artifacts WHERE s3_key = ?")
        .bind(s3_key)
        .fetch_optional(pool)
        .await
        .context("Failed to find artifact by S3 key")
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

/// Get total count of completed archives by content type.
pub async fn count_archives_by_content_type(pool: &SqlitePool) -> Result<Vec<(String, i64)>> {
    sqlx::query_as(
        "SELECT COALESCE(content_type, 'unknown') as content_type, COUNT(*)
         FROM archives
         WHERE status = 'complete'
         GROUP BY content_type
         ORDER BY COUNT(*) DESC",
    )
    .fetch_all(pool)
    .await
    .context("Failed to count archives by content type")
}

// TODO: Add content type trends - track which content types are most successful
// at archiving over time (success rate by content type, average processing time, etc.)

/// Get top domains by completed archive count.
pub async fn get_top_domains(pool: &SqlitePool, limit: i64) -> Result<Vec<(String, i64)>> {
    sqlx::query_as(
        "SELECT l.domain, COUNT(*) as count
         FROM archives a
         JOIN links l ON a.link_id = l.id
         WHERE a.status = 'complete'
         GROUP BY l.domain
         ORDER BY count DESC
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to get top domains")
}

/// Get recent activity counts (24h, 7d, 30d).
pub async fn get_recent_activity_counts(pool: &SqlitePool) -> Result<(i64, i64, i64)> {
    let last_24h: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM archives
         WHERE status = 'complete'
         AND created_at >= datetime('now', '-1 day')",
    )
    .fetch_one(pool)
    .await?;

    let last_7d: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM archives
         WHERE status = 'complete'
         AND created_at >= datetime('now', '-7 days')",
    )
    .fetch_one(pool)
    .await?;

    let last_30d: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM archives
         WHERE status = 'complete'
         AND created_at >= datetime('now', '-30 days')",
    )
    .fetch_one(pool)
    .await?;

    Ok((last_24h.0, last_7d.0, last_30d.0))
}

/// Get storage statistics (total size in bytes, average size, largest archive).
pub async fn get_storage_stats(pool: &SqlitePool) -> Result<(i64, f64, i64)> {
    let stats: Option<(Option<i64>, Option<f64>, Option<i64>)> = sqlx::query_as(
        "SELECT
            COALESCE(SUM(size_bytes), 0) as total_size,
            COALESCE(AVG(size_bytes), 0.0) as avg_size,
            COALESCE(MAX(size_bytes), 0) as max_size
         FROM artifacts",
    )
    .fetch_optional(pool)
    .await?;

    match stats {
        Some((Some(total), Some(avg), Some(max))) => Ok((total, avg, max)),
        _ => Ok((0, 0.0, 0)),
    }
}

/// Get timeline data for archives (by month for the last 12 months).
pub async fn get_archive_timeline(pool: &SqlitePool) -> Result<Vec<(String, i64)>> {
    sqlx::query_as(
        "SELECT
            strftime('%Y-%m', created_at) as month,
            COUNT(*) as count
         FROM archives
         WHERE status = 'complete'
         AND created_at >= datetime('now', '-12 months')
         GROUP BY month
         ORDER BY month ASC",
    )
    .fetch_all(pool)
    .await
    .context("Failed to get archive timeline")
}

/// Get quality metrics (archives with video, complete.html, screenshots).
pub async fn get_quality_metrics(pool: &SqlitePool) -> Result<(i64, i64, i64)> {
    let with_video: (i64,) = sqlx::query_as(
        "SELECT COUNT(DISTINCT archive_id) FROM artifacts
         WHERE file_type = 'video'",
    )
    .fetch_one(pool)
    .await?;

    let with_complete_html: (i64,) = sqlx::query_as(
        "SELECT COUNT(DISTINCT archive_id) FROM artifacts
         WHERE s3_key LIKE '%/complete.html'",
    )
    .fetch_one(pool)
    .await?;

    let with_screenshot: (i64,) = sqlx::query_as(
        "SELECT COUNT(DISTINCT archive_id) FROM artifacts
         WHERE file_type = 'image' AND s3_key LIKE '%screenshot%'",
    )
    .fetch_one(pool)
    .await?;

    Ok((with_video.0, with_complete_html.0, with_screenshot.0))
}

/// Get NSFW archive count.
pub async fn get_nsfw_count(pool: &SqlitePool) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM archives
         WHERE status = 'complete' AND is_nsfw = 1",
    )
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
        INSERT INTO submissions (url, normalized_url, submitted_by_ip, status, submitted_by_user_id)
        VALUES (?, ?, ?, 'pending', ?)
        ",
    )
    .bind(&submission.url)
    .bind(&submission.normalized_url)
    .bind(&submission.submitted_by_ip)
    .bind(submission.submitted_by_user_id)
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

/// Get user submission statistics.
pub async fn get_user_submission_stats(
    pool: &SqlitePool,
    user_id: i64,
) -> Result<(i64, i64, i64, i64)> {
    let total: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM submissions WHERE submitted_by_user_id = ?")
            .bind(user_id)
            .fetch_one(pool)
            .await?;

    let complete: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM submissions WHERE submitted_by_user_id = ? AND status = 'complete'",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    let pending: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM submissions WHERE submitted_by_user_id = ? AND status IN ('pending', 'processing')",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    let failed: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM submissions WHERE submitted_by_user_id = ? AND status IN ('failed', 'rejected')",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    Ok((total.0, complete.0, pending.0, failed.0))
}

/// Get user's recent submissions with details.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserSubmissionDetail {
    pub id: i64,
    pub url: String,
    pub status: String,
    pub created_at: String,
    pub processed_at: Option<String>,
    pub error_message: Option<String>,
    pub link_id: Option<i64>,
    pub archive_id: Option<i64>,
    /// The archive status (if an archive exists for this submission).
    pub archive_status: Option<String>,
    /// The archive error message (if the archive failed).
    pub archive_error: Option<String>,
}

pub async fn get_user_submissions(
    pool: &SqlitePool,
    user_id: i64,
    limit: i64,
) -> Result<Vec<UserSubmissionDetail>> {
    sqlx::query_as(
        r"
        SELECT s.id, s.url, s.status, s.created_at, s.processed_at,
               s.error_message, s.link_id, a.id as archive_id,
               a.status as archive_status, a.error_message as archive_error
        FROM submissions s
        LEFT JOIN archives a ON s.link_id = a.link_id
        WHERE s.submitted_by_user_id = ?
        ORDER BY s.created_at DESC
        LIMIT ?
        ",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch user submissions")
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
    pub post_discourse_url: String,
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
            p.discourse_url as post_discourse_url,
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

/// Reset an archive for re-archiving while preserving existing metadata.
///
/// This resets the archive to pending state but keeps:
/// - archived_at, content_title, content_author, content_text, content_type
/// - is_nsfw, nsfw_source
/// - post_date
///
/// Artifacts and jobs are deleted to be regenerated during processing.
/// Use this when you want to refresh artifacts without losing metadata.
pub async fn reset_archive_for_rearchive_preserve_metadata(
    pool: &SqlitePool,
    id: i64,
) -> Result<()> {
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

    // Reset archive to pending state but PRESERVE metadata
    // Only reset: status, s3 keys, external URLs, error state, retry count
    sqlx::query(
        r"
        UPDATE archives
        SET status = 'pending',
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

/// Update download progress for an archive.
///
/// This is called periodically during yt-dlp downloads to track progress.
/// The progress_details should be a JSON string with speed, ETA, etc.
pub async fn update_archive_progress(
    pool: &SqlitePool,
    id: i64,
    progress_percent: f64,
    progress_details: &str,
) -> Result<()> {
    sqlx::query(
        r"
        UPDATE archives
        SET progress_percent = ?,
            progress_details = ?,
            last_progress_update = datetime('now')
        WHERE id = ?
        ",
    )
    .bind(progress_percent)
    .bind(progress_details)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to update archive progress")?;

    Ok(())
}

/// Clear progress tracking when download completes or fails.
pub async fn clear_archive_progress(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query(
        r"
        UPDATE archives
        SET progress_percent = NULL,
            progress_details = NULL,
            last_progress_update = NULL
        WHERE id = ?
        ",
    )
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to clear archive progress")?;

    Ok(())
}

/// Set the quoted tweet archive link for a Twitter/X archive.
pub async fn set_archive_quoted_link(
    pool: &SqlitePool,
    archive_id: i64,
    quoted_archive_id: i64,
) -> Result<()> {
    sqlx::query("UPDATE archives SET quoted_archive_id = ? WHERE id = ?")
        .bind(quoted_archive_id)
        .bind(archive_id)
        .execute(pool)
        .await
        .context("Failed to set quoted archive link")?;

    Ok(())
}

/// Set the reply-to tweet archive link for a Twitter/X archive.
pub async fn set_archive_reply_link(
    pool: &SqlitePool,
    archive_id: i64,
    reply_to_archive_id: i64,
) -> Result<()> {
    sqlx::query("UPDATE archives SET reply_to_archive_id = ? WHERE id = ?")
        .bind(reply_to_archive_id)
        .bind(archive_id)
        .execute(pool)
        .await
        .context("Failed to set reply-to archive link")?;

    Ok(())
}

/// Get the quote/reply chain for a Twitter/X archive.
/// Returns archives in order from the given archive up to the root.
pub async fn get_quote_reply_chain(pool: &SqlitePool, archive_id: i64) -> Result<Vec<Archive>> {
    // Use a recursive CTE to traverse the chain
    let archives: Vec<Archive> = sqlx::query_as(
        r"
        WITH RECURSIVE chain AS (
            SELECT * FROM archives WHERE id = ?
            UNION ALL
            SELECT a.* FROM archives a
            JOIN chain c ON (a.id = c.quoted_archive_id OR a.id = c.reply_to_archive_id)
            WHERE a.id != c.id
        )
        SELECT * FROM chain
        LIMIT 10
        ",
    )
    .bind(archive_id)
    .fetch_all(pool)
    .await
    .context("Failed to get quote/reply chain")?;

    Ok(archives)
}

/// Find an existing archive for a URL (by normalized URL).
/// Used to link quote/reply tweets to existing archives.
pub async fn find_archive_by_url(pool: &SqlitePool, normalized_url: &str) -> Result<Option<i64>> {
    let result: Option<(i64,)> = sqlx::query_as(
        r"
        SELECT a.id FROM archives a
        JOIN links l ON a.link_id = l.id
        WHERE l.normalized_url = ?
        AND a.status = 'complete'
        LIMIT 1
        ",
    )
    .bind(normalized_url)
    .fetch_optional(pool)
    .await
    .context("Failed to find archive by URL")?;

    Ok(result.map(|(id,)| id))
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
        r"
        UPDATE archive_jobs
        SET status = 'completed',
            completed_at = datetime('now'),
            metadata = ?,
            duration_seconds = (julianday(datetime('now')) - julianday(started_at)) * 86400
        WHERE id = ?
        ",
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
        r"
        UPDATE archive_jobs
        SET status = 'failed',
            completed_at = datetime('now'),
            error_message = ?,
            duration_seconds = (julianday(datetime('now')) - julianday(started_at)) * 86400
        WHERE id = ?
        ",
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
        r"
        UPDATE archive_jobs
        SET status = 'skipped',
            completed_at = datetime('now'),
            error_message = ?,
            duration_seconds = (julianday(datetime('now')) - julianday(started_at)) * 86400
        WHERE id = ?
        ",
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

/// Get pending comment extraction jobs.
pub async fn get_pending_comment_extraction_jobs(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<ArchiveJob>> {
    sqlx::query_as(
        r"
        SELECT * FROM archive_jobs
        WHERE job_type = 'comment_extraction'
          AND status = 'pending'
        ORDER BY created_at ASC
        LIMIT ?
        ",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch pending comment extraction jobs")
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

// ========== Video Files ==========

/// Find a video file by platform and video ID.
pub async fn find_video_file(
    pool: &SqlitePool,
    platform: &str,
    video_id: &str,
) -> Result<Option<VideoFile>> {
    sqlx::query_as(
        r"
        SELECT * FROM video_files
        WHERE platform = ? AND video_id = ?
        ",
    )
    .bind(platform)
    .bind(video_id)
    .fetch_optional(pool)
    .await
    .context("Failed to find video file")
}

/// Get a video file by ID.
pub async fn get_video_file(pool: &SqlitePool, id: i64) -> Result<Option<VideoFile>> {
    sqlx::query_as("SELECT * FROM video_files WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("Failed to get video file")
}

/// Get a video file by S3 key.
pub async fn find_video_file_by_s3_key(
    pool: &SqlitePool,
    s3_key: &str,
) -> Result<Option<VideoFile>> {
    sqlx::query_as("SELECT * FROM video_files WHERE s3_key = ?")
        .bind(s3_key)
        .fetch_optional(pool)
        .await
        .context("Failed to find video file by S3 key")
}

/// Insert a new video file record.
///
/// Uses INSERT OR IGNORE to handle race conditions where multiple archives
/// try to insert the same video simultaneously. Returns the ID of either
/// the newly inserted or existing record.
pub async fn insert_video_file(
    pool: &SqlitePool,
    video_id: &str,
    platform: &str,
    s3_key: &str,
    metadata_s3_key: Option<&str>,
    size_bytes: Option<i64>,
    content_type: Option<&str>,
    duration_seconds: Option<i64>,
) -> Result<i64> {
    // First, try to insert (will be ignored if already exists due to UNIQUE constraint)
    sqlx::query(
        r"
        INSERT OR IGNORE INTO video_files
            (video_id, platform, s3_key, metadata_s3_key, size_bytes, content_type, duration_seconds)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ",
    )
    .bind(video_id)
    .bind(platform)
    .bind(s3_key)
    .bind(metadata_s3_key)
    .bind(size_bytes)
    .bind(content_type)
    .bind(duration_seconds)
    .execute(pool)
    .await
    .context("Failed to insert video file")?;

    // Now fetch the record (either newly inserted or existing)
    let video_file: VideoFile =
        sqlx::query_as("SELECT * FROM video_files WHERE platform = ? AND video_id = ?")
            .bind(platform)
            .bind(video_id)
            .fetch_one(pool)
            .await
            .context("Failed to fetch video file after insert")?;

    Ok(video_file.id)
}

/// Get or create a video file record (atomic upsert).
///
/// This is the preferred method for video deduplication as it handles
/// race conditions safely using SQLite's INSERT OR IGNORE.
pub async fn get_or_create_video_file(
    pool: &SqlitePool,
    video_id: &str,
    platform: &str,
    s3_key: &str,
    metadata_s3_key: Option<&str>,
    size_bytes: Option<i64>,
    content_type: Option<&str>,
    duration_seconds: Option<i64>,
) -> Result<VideoFile> {
    // Insert or ignore (handles race conditions)
    sqlx::query(
        r"
        INSERT OR IGNORE INTO video_files
            (video_id, platform, s3_key, metadata_s3_key, size_bytes, content_type, duration_seconds)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ",
    )
    .bind(video_id)
    .bind(platform)
    .bind(s3_key)
    .bind(metadata_s3_key)
    .bind(size_bytes)
    .bind(content_type)
    .bind(duration_seconds)
    .execute(pool)
    .await
    .context("Failed to insert video file")?;

    // Fetch the record (either newly inserted or existing)
    sqlx::query_as("SELECT * FROM video_files WHERE platform = ? AND video_id = ?")
        .bind(platform)
        .bind(video_id)
        .fetch_one(pool)
        .await
        .context("Failed to fetch video file")
}

/// Update video file metadata (size, content_type, duration).
pub async fn update_video_file_metadata(
    pool: &SqlitePool,
    id: i64,
    size_bytes: Option<i64>,
    content_type: Option<&str>,
    duration_seconds: Option<i64>,
) -> Result<()> {
    sqlx::query(
        r"
        UPDATE video_files
        SET size_bytes = COALESCE(?, size_bytes),
            content_type = COALESCE(?, content_type),
            duration_seconds = COALESCE(?, duration_seconds)
        WHERE id = ?
        ",
    )
    .bind(size_bytes)
    .bind(content_type)
    .bind(duration_seconds)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to update video file metadata")?;

    Ok(())
}

/// Update the metadata S3 key for a video file.
pub async fn update_video_file_metadata_key(
    pool: &SqlitePool,
    id: i64,
    metadata_s3_key: &str,
) -> Result<()> {
    sqlx::query("UPDATE video_files SET metadata_s3_key = ? WHERE id = ?")
        .bind(metadata_s3_key)
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to update video file metadata key")?;

    Ok(())
}

/// Get all archives that reference a specific video file.
pub async fn get_archives_for_video_file(
    pool: &SqlitePool,
    video_file_id: i64,
) -> Result<Vec<Archive>> {
    sqlx::query_as(
        r"
        SELECT DISTINCT a.* FROM archives a
        INNER JOIN archive_artifacts aa ON a.id = aa.archive_id
        WHERE aa.video_file_id = ?
        ORDER BY a.archived_at DESC
        ",
    )
    .bind(video_file_id)
    .fetch_all(pool)
    .await
    .context("Failed to get archives for video file")
}

/// Insert an artifact with a video file reference.
pub async fn insert_artifact_with_video_file(
    pool: &SqlitePool,
    archive_id: i64,
    kind: &str,
    s3_key: &str,
    content_type: Option<&str>,
    size_bytes: Option<i64>,
    sha256: Option<&str>,
    video_file_id: i64,
) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO archive_artifacts (archive_id, kind, s3_key, content_type, size_bytes, sha256, video_file_id)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ",
    )
    .bind(archive_id)
    .bind(kind)
    .bind(s3_key)
    .bind(content_type)
    .bind(size_bytes)
    .bind(sha256)
    .bind(video_file_id)
    .execute(pool)
    .await
    .context("Failed to insert artifact with video file")?;

    Ok(result.last_insert_rowid())
}

/// Count how many archives reference a video file.
pub async fn count_archives_for_video_file(pool: &SqlitePool, video_file_id: i64) -> Result<i64> {
    let row: (i64,) = sqlx::query_as(
        r"
        SELECT COUNT(DISTINCT aa.archive_id) FROM archive_artifacts aa
        WHERE aa.video_file_id = ?
        ",
    )
    .bind(video_file_id)
    .fetch_one(pool)
    .await
    .context("Failed to count archives for video file")?;

    Ok(row.0)
}

/// Get all video files for a specific platform.
pub async fn get_video_files_by_platform(
    pool: &SqlitePool,
    platform: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<VideoFile>> {
    sqlx::query_as(
        r"
        SELECT * FROM video_files
        WHERE platform = ?
        ORDER BY created_at DESC
        LIMIT ? OFFSET ?
        ",
    )
    .bind(platform)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .context("Failed to get video files by platform")
}

// ========== Users ==========

/// Get a user by ID.
pub async fn get_user_by_id(pool: &SqlitePool, id: i64) -> Result<Option<User>> {
    sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch user by id")
}

/// Get a user by username.
pub async fn get_user_by_username(pool: &SqlitePool, username: &str) -> Result<Option<User>> {
    sqlx::query_as("SELECT * FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch user by username")
}

/// Get a user by username or display_name.
/// Used for login - users can sign in with either.
pub async fn get_user_by_username_or_display_name(
    pool: &SqlitePool,
    identifier: &str,
) -> Result<Option<User>> {
    sqlx::query_as("SELECT * FROM users WHERE username = ? OR display_name = ? LIMIT 1")
        .bind(identifier)
        .bind(identifier)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch user by username or display_name")
}

/// Check if a username already exists.
pub async fn username_exists(pool: &SqlitePool, username: &str) -> Result<bool> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE username = ?")
        .bind(username)
        .fetch_one(pool)
        .await
        .context("Failed to check username existence")?;
    Ok(row.0 > 0)
}

/// Check if a display_name already exists (excluding a specific user).
pub async fn display_name_exists(
    pool: &SqlitePool,
    display_name: &str,
    exclude_user_id: Option<i64>,
) -> Result<bool> {
    let row: (i64,) = if let Some(user_id) = exclude_user_id {
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE display_name = ? AND id != ?")
            .bind(display_name)
            .bind(user_id)
            .fetch_one(pool)
            .await
            .context("Failed to check display_name existence")?
    } else {
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE display_name = ?")
            .bind(display_name)
            .fetch_one(pool)
            .await
            .context("Failed to check display_name existence")?
    };
    Ok(row.0 > 0)
}

/// Create a new user.
pub async fn create_user(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
    is_admin: bool,
) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO users (username, password_hash, is_admin, is_approved)
        VALUES (?, ?, ?, ?)
        ",
    )
    .bind(username)
    .bind(password_hash)
    .bind(is_admin)
    .bind(is_admin) // First user auto-approved, others need approval
    .execute(pool)
    .await
    .context("Failed to create user")?;

    Ok(result.last_insert_rowid())
}

/// Count total users.
pub async fn count_users(pool: &SqlitePool) -> Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .context("Failed to count users")?;
    Ok(row.0)
}

/// Get all users with pagination.
pub async fn get_all_users(pool: &SqlitePool, limit: i64, offset: i64) -> Result<Vec<User>> {
    sqlx::query_as(
        r"
        SELECT * FROM users
        ORDER BY created_at DESC
        LIMIT ? OFFSET ?
        ",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .context("Failed to get all users")
}

/// Update user approval status.
pub async fn update_user_approval(
    pool: &SqlitePool,
    user_id: i64,
    is_approved: bool,
) -> Result<()> {
    sqlx::query("UPDATE users SET is_approved = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(is_approved)
        .bind(user_id)
        .execute(pool)
        .await
        .context("Failed to update user approval")?;
    Ok(())
}

/// Update user admin status.
pub async fn update_user_admin(pool: &SqlitePool, user_id: i64, is_admin: bool) -> Result<()> {
    sqlx::query("UPDATE users SET is_admin = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(is_admin)
        .bind(user_id)
        .execute(pool)
        .await
        .context("Failed to update user admin status")?;
    Ok(())
}

/// Update user active status (soft delete).
pub async fn update_user_active(pool: &SqlitePool, user_id: i64, is_active: bool) -> Result<()> {
    sqlx::query("UPDATE users SET is_active = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(is_active)
        .bind(user_id)
        .execute(pool)
        .await
        .context("Failed to update user active status")?;
    Ok(())
}

/// Update user password.
pub async fn update_user_password(
    pool: &SqlitePool,
    user_id: i64,
    password_hash: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE users SET password_hash = ?, password_updated_at = datetime('now'), updated_at = datetime('now') WHERE id = ?"
    )
    .bind(password_hash)
    .bind(user_id)
    .execute(pool)
    .await
    .context("Failed to update user password")?;
    Ok(())
}

/// Update user profile (email, display_name).
pub async fn update_user_profile(
    pool: &SqlitePool,
    user_id: i64,
    email: Option<&str>,
    display_name: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "UPDATE users SET email = ?, display_name = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(email)
    .bind(display_name)
    .bind(user_id)
    .execute(pool)
    .await
    .context("Failed to update user profile")?;
    Ok(())
}

/// Increment failed login attempts.
pub async fn increment_failed_login_attempts(pool: &SqlitePool, user_id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE users SET failed_login_attempts = failed_login_attempts + 1, updated_at = datetime('now') WHERE id = ?"
    )
    .bind(user_id)
    .execute(pool)
    .await
    .context("Failed to increment failed login attempts")?;
    Ok(())
}

/// Reset failed login attempts.
pub async fn reset_failed_login_attempts(pool: &SqlitePool, user_id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE users SET failed_login_attempts = 0, locked_until = NULL, updated_at = datetime('now') WHERE id = ?"
    )
    .bind(user_id)
    .execute(pool)
    .await
    .context("Failed to reset failed login attempts")?;
    Ok(())
}

/// Lock user account until specified time.
pub async fn lock_user_until(pool: &SqlitePool, user_id: i64, locked_until: &str) -> Result<()> {
    sqlx::query("UPDATE users SET locked_until = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(locked_until)
        .bind(user_id)
        .execute(pool)
        .await
        .context("Failed to lock user account")?;
    Ok(())
}

// ========== Sessions ==========

/// Create a new session.
pub async fn create_session(
    pool: &SqlitePool,
    user_id: i64,
    token: &str,
    csrf_token: &str,
    ip_address: &str,
    user_agent: Option<&str>,
    expires_at: &str,
) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO sessions (user_id, token, csrf_token, ip_address, user_agent, expires_at)
        VALUES (?, ?, ?, ?, ?, ?)
        ",
    )
    .bind(user_id)
    .bind(token)
    .bind(csrf_token)
    .bind(ip_address)
    .bind(user_agent)
    .bind(expires_at)
    .execute(pool)
    .await
    .context("Failed to create session")?;

    Ok(result.last_insert_rowid())
}

/// Get a session by token.
pub async fn get_session_by_token(pool: &SqlitePool, token: &str) -> Result<Option<Session>> {
    sqlx::query_as("SELECT * FROM sessions WHERE token = ?")
        .bind(token)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch session by token")
}

/// Update session last_used_at.
pub async fn update_session_last_used(pool: &SqlitePool, session_id: i64) -> Result<()> {
    sqlx::query("UPDATE sessions SET last_used_at = datetime('now') WHERE id = ?")
        .bind(session_id)
        .execute(pool)
        .await
        .context("Failed to update session last_used")?;
    Ok(())
}

/// Delete a session.
pub async fn delete_session(pool: &SqlitePool, token: &str) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE token = ?")
        .bind(token)
        .execute(pool)
        .await
        .context("Failed to delete session")?;
    Ok(())
}

/// Delete all sessions for a user.
pub async fn delete_user_sessions(pool: &SqlitePool, user_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await
        .context("Failed to delete user sessions")?;
    Ok(())
}

/// Delete all sessions for a user except the current one.
/// Used when changing password to invalidate other sessions.
pub async fn delete_other_user_sessions(
    pool: &SqlitePool,
    user_id: i64,
    current_token: &str,
) -> Result<u64> {
    let result = sqlx::query("DELETE FROM sessions WHERE user_id = ? AND token != ?")
        .bind(user_id)
        .bind(current_token)
        .execute(pool)
        .await
        .context("Failed to delete other user sessions")?;
    Ok(result.rows_affected())
}

/// Count active sessions for a user.
pub async fn count_user_sessions(pool: &SqlitePool, user_id: i64) -> Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await
        .context("Failed to count user sessions")?;
    Ok(row.0)
}

/// Delete oldest sessions for a user, keeping only the most recent `keep_count`.
/// Used to enforce max session limits.
pub async fn delete_oldest_user_sessions(
    pool: &SqlitePool,
    user_id: i64,
    keep_count: i64,
) -> Result<u64> {
    let result = sqlx::query(
        r"
        DELETE FROM sessions
        WHERE user_id = ? AND id NOT IN (
            SELECT id FROM sessions
            WHERE user_id = ?
            ORDER BY COALESCE(last_used_at, created_at) DESC
            LIMIT ?
        )
        ",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(keep_count)
    .execute(pool)
    .await
    .context("Failed to delete oldest user sessions")?;
    Ok(result.rows_affected())
}

/// Delete expired sessions.
pub async fn delete_expired_sessions(pool: &SqlitePool) -> Result<u64> {
    let result = sqlx::query("DELETE FROM sessions WHERE expires_at < datetime('now')")
        .execute(pool)
        .await
        .context("Failed to delete expired sessions")?;
    Ok(result.rows_affected())
}

// ========== Audit Events ==========

/// Create an audit event.
#[allow(clippy::too_many_arguments)]
pub async fn create_audit_event(
    pool: &SqlitePool,
    user_id: Option<i64>,
    event_type: &str,
    target_type: Option<&str>,
    target_id: Option<i64>,
    metadata: Option<&str>,
    ip_address: Option<&str>,
    forwarded_for: Option<&str>,
    user_agent: Option<&str>,
) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO audit_events (user_id, event_type, target_type, target_id, metadata, ip_address, forwarded_for, user_agent)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ",
    )
    .bind(user_id)
    .bind(event_type)
    .bind(target_type)
    .bind(target_id)
    .bind(metadata)
    .bind(ip_address)
    .bind(forwarded_for)
    .bind(user_agent)
    .execute(pool)
    .await
    .context("Failed to create audit event")?;

    Ok(result.last_insert_rowid())
}

/// Get audit events with pagination.
pub async fn get_audit_events(
    pool: &SqlitePool,
    limit: i64,
    offset: i64,
) -> Result<Vec<AuditEvent>> {
    sqlx::query_as(
        r"
        SELECT * FROM audit_events
        ORDER BY created_at DESC
        LIMIT ? OFFSET ?
        ",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .context("Failed to get audit events")
}

/// Get audit events for a specific user.
pub async fn get_audit_events_for_user(
    pool: &SqlitePool,
    user_id: i64,
    limit: i64,
    offset: i64,
) -> Result<Vec<AuditEvent>> {
    sqlx::query_as(
        r"
        SELECT * FROM audit_events
        WHERE user_id = ?
        ORDER BY created_at DESC
        LIMIT ? OFFSET ?
        ",
    )
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .context("Failed to get audit events for user")
}

/// Count total audit events.
pub async fn count_audit_events(pool: &SqlitePool) -> Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_events")
        .fetch_one(pool)
        .await
        .context("Failed to count audit events")?;
    Ok(row.0)
}

// ========== User Agents ==========

/// Get or create a user agent entry (for deduplication).
/// Returns the user_agent id.
pub async fn get_or_create_user_agent(pool: &SqlitePool, user_agent: &str) -> Result<i64> {
    use sha2::{Digest, Sha256};

    // Compute hash of user agent
    let mut hasher = Sha256::new();
    hasher.update(user_agent.as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    // Try to get existing entry
    let existing: Option<(i64,)> = sqlx::query_as("SELECT id FROM user_agents WHERE hash = ?")
        .bind(&hash)
        .fetch_optional(pool)
        .await
        .context("Failed to check user_agent existence")?;

    if let Some((id,)) = existing {
        // Update last_seen_at
        sqlx::query("UPDATE user_agents SET last_seen_at = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await
            .context("Failed to update user_agent last_seen")?;
        return Ok(id);
    }

    // Insert new entry
    let result = sqlx::query(
        r"
        INSERT INTO user_agents (hash, user_agent)
        VALUES (?, ?)
        ",
    )
    .bind(&hash)
    .bind(user_agent)
    .execute(pool)
    .await
    .context("Failed to insert user_agent")?;

    Ok(result.last_insert_rowid())
}

/// Delete old audit events (for cleanup).
pub async fn delete_old_audit_events(pool: &SqlitePool, days: i64) -> Result<u64> {
    let result = sqlx::query(&format!(
        "DELETE FROM audit_events WHERE created_at < datetime('now', '-{} days')",
        days
    ))
    .execute(pool)
    .await
    .context("Failed to delete old audit events")?;
    Ok(result.rows_affected())
}

// ==================== Comment Queries ====================

/// Create a new top-level comment on an archive.
pub async fn create_comment(
    pool: &SqlitePool,
    archive_id: i64,
    user_id: i64,
    content: &str,
) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO comments (archive_id, user_id, content)
        VALUES (?, ?, ?)
        ",
    )
    .bind(archive_id)
    .bind(user_id)
    .bind(content)
    .execute(pool)
    .await
    .context("Failed to create comment")?;

    Ok(result.last_insert_rowid())
}

/// Create a reply to an existing comment.
pub async fn create_comment_reply(
    pool: &SqlitePool,
    archive_id: i64,
    user_id: i64,
    parent_comment_id: i64,
    content: &str,
) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO comments (archive_id, user_id, parent_comment_id, content)
        VALUES (?, ?, ?, ?)
        ",
    )
    .bind(archive_id)
    .bind(user_id)
    .bind(parent_comment_id)
    .bind(content)
    .execute(pool)
    .await
    .context("Failed to create comment reply")?;

    Ok(result.last_insert_rowid())
}

/// Get all non-deleted comments for an archive with author info.
pub async fn get_comments_for_archive(
    pool: &SqlitePool,
    archive_id: i64,
) -> Result<Vec<crate::db::models::CommentWithAuthor>> {
    sqlx::query_as(
        r"
        SELECT
            c.id, c.archive_id, c.user_id, c.parent_comment_id, c.content,
            c.is_deleted, c.deleted_by_admin, c.is_pinned, c.pinned_by_user_id,
            c.created_at, c.updated_at, c.deleted_at,
            COALESCE(u.display_name, u.username) as author_display_name,
            u.username as author_username,
            (SELECT COUNT(*) FROM comment_edits WHERE comment_id = c.id) as edit_count,
            (SELECT COUNT(*) FROM comment_reactions WHERE comment_id = c.id AND reaction_type = 'helpful') as helpful_count
        FROM comments c
        LEFT JOIN users u ON c.user_id = u.id
        WHERE c.archive_id = ?
        ORDER BY c.is_pinned DESC, c.created_at ASC
        ",
    )
    .bind(archive_id)
    .fetch_all(pool)
    .await
    .context("Failed to get comments for archive")
}

/// Get a specific comment with author info.
pub async fn get_comment_with_author(
    pool: &SqlitePool,
    comment_id: i64,
) -> Result<Option<crate::db::models::CommentWithAuthor>> {
    sqlx::query_as(
        r"
        SELECT
            c.id, c.archive_id, c.user_id, c.parent_comment_id, c.content,
            c.is_deleted, c.deleted_by_admin, c.is_pinned, c.pinned_by_user_id,
            c.created_at, c.updated_at, c.deleted_at,
            COALESCE(u.display_name, u.username) as author_display_name,
            u.username as author_username,
            (SELECT COUNT(*) FROM comment_edits WHERE comment_id = c.id) as edit_count,
            (SELECT COUNT(*) FROM comment_reactions WHERE comment_id = c.id AND reaction_type = 'helpful') as helpful_count
        FROM comments c
        LEFT JOIN users u ON c.user_id = u.id
        WHERE c.id = ?
        ",
    )
    .bind(comment_id)
    .fetch_optional(pool)
    .await
    .context("Failed to get comment")
}

/// Update a comment's content and record edit history.
pub async fn update_comment(
    pool: &SqlitePool,
    comment_id: i64,
    new_content: &str,
    edited_by_user_id: i64,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    // Get the old content for history
    let old_content: (String,) = sqlx::query_as("SELECT content FROM comments WHERE id = ?")
        .bind(comment_id)
        .fetch_one(&mut *tx)
        .await
        .context("Failed to fetch comment for edit")?;

    // Record edit history
    sqlx::query(
        r"
        INSERT INTO comment_edits (comment_id, previous_content, edited_by_user_id)
        VALUES (?, ?, ?)
        ",
    )
    .bind(comment_id)
    .bind(&old_content.0)
    .bind(edited_by_user_id)
    .execute(&mut *tx)
    .await
    .context("Failed to record comment edit history")?;

    // Update the comment
    sqlx::query("UPDATE comments SET content = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(new_content)
        .bind(comment_id)
        .execute(&mut *tx)
        .await
        .context("Failed to update comment")?;

    tx.commit().await?;
    Ok(())
}

/// Soft-delete a comment (mark as deleted, keep content for history).
pub async fn soft_delete_comment(pool: &SqlitePool, comment_id: i64, by_admin: bool) -> Result<()> {
    sqlx::query(
        "UPDATE comments SET is_deleted = 1, deleted_by_admin = ?, deleted_at = datetime('now') WHERE id = ?",
    )
    .bind(by_admin as i32)
    .bind(comment_id)
    .execute(pool)
    .await
    .context("Failed to delete comment")?;

    Ok(())
}

/// Check if a user can edit a comment (owner within 1 hour, or admin).
pub async fn can_user_edit_comment(
    pool: &SqlitePool,
    comment_id: i64,
    user_id: i64,
    is_admin: bool,
) -> Result<bool> {
    if is_admin {
        return Ok(true);
    }

    let result: Option<(String,)> =
        sqlx::query_as("SELECT created_at FROM comments WHERE id = ? AND user_id = ?")
            .bind(comment_id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .context("Failed to check edit permission")?;

    if let Some((created_at,)) = result {
        // Parse the created_at timestamp and check if within 1 hour
        // Using SQLite's datetime calculations
        let within_hour: (i32,) =
            sqlx::query_as("SELECT (julianday('now') - julianday(?)) * 24 < 1")
                .bind(created_at)
                .fetch_one(pool)
                .await
                .context("Failed to check time window")?;

        Ok(within_hour.0 != 0)
    } else {
        Ok(false)
    }
}

/// Get edit history for a comment.
pub async fn get_comment_edit_history(
    pool: &SqlitePool,
    comment_id: i64,
) -> Result<Vec<crate::db::models::CommentEdit>> {
    sqlx::query_as("SELECT id, comment_id, previous_content, edited_by_user_id, edited_at FROM comment_edits WHERE comment_id = ? ORDER BY edited_at ASC")
        .bind(comment_id)
        .fetch_all(pool)
        .await
        .context("Failed to get comment edit history")
}

/// Pin a comment (admin only).
pub async fn pin_comment(pool: &SqlitePool, comment_id: i64, admin_user_id: i64) -> Result<()> {
    sqlx::query("UPDATE comments SET is_pinned = 1, pinned_by_user_id = ? WHERE id = ?")
        .bind(admin_user_id)
        .bind(comment_id)
        .execute(pool)
        .await
        .context("Failed to pin comment")?;

    Ok(())
}

/// Unpin a comment (admin only).
pub async fn unpin_comment(pool: &SqlitePool, comment_id: i64) -> Result<()> {
    sqlx::query("UPDATE comments SET is_pinned = 0, pinned_by_user_id = NULL WHERE id = ?")
        .bind(comment_id)
        .execute(pool)
        .await
        .context("Failed to unpin comment")?;

    Ok(())
}

/// Add a helpful reaction to a comment.
pub async fn add_comment_reaction(pool: &SqlitePool, comment_id: i64, user_id: i64) -> Result<()> {
    sqlx::query(
        r"
        INSERT OR IGNORE INTO comment_reactions (comment_id, user_id, reaction_type)
        VALUES (?, ?, 'helpful')
        ",
    )
    .bind(comment_id)
    .bind(user_id)
    .execute(pool)
    .await
    .context("Failed to add comment reaction")?;

    Ok(())
}

/// Remove a helpful reaction from a comment.
pub async fn remove_comment_reaction(
    pool: &SqlitePool,
    comment_id: i64,
    user_id: i64,
) -> Result<()> {
    sqlx::query(
        "DELETE FROM comment_reactions WHERE comment_id = ? AND user_id = ? AND reaction_type = 'helpful'",
    )
    .bind(comment_id)
    .bind(user_id)
    .execute(pool)
    .await
    .context("Failed to remove comment reaction")?;

    Ok(())
}

/// Get count of helpful reactions for a comment.
pub async fn get_comment_reaction_count(pool: &SqlitePool, comment_id: i64) -> Result<i64> {
    let result: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM comment_reactions WHERE comment_id = ? AND reaction_type = 'helpful'",
    )
    .bind(comment_id)
    .fetch_one(pool)
    .await
    .context("Failed to get reaction count")?;

    Ok(result.0)
}

/// Check if a user has reacted to a comment.
pub async fn has_user_reacted(pool: &SqlitePool, comment_id: i64, user_id: i64) -> Result<bool> {
    let result: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM comment_reactions WHERE comment_id = ? AND user_id = ? AND reaction_type = 'helpful'",
    )
    .bind(comment_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .context("Failed to check user reaction")?;

    Ok(result.is_some())
}

// ============================================================================
// Excluded Domains queries
// ============================================================================

/// Add an excluded domain.
pub async fn add_excluded_domain(
    pool: &SqlitePool,
    domain: &str,
    reason: &str,
    created_by_user_id: Option<i64>,
) -> Result<i64> {
    let result = sqlx::query(
        "INSERT INTO excluded_domains (domain, reason, created_by_user_id) VALUES (?, ?, ?)",
    )
    .bind(domain)
    .bind(reason)
    .bind(created_by_user_id)
    .execute(pool)
    .await
    .context("Failed to add excluded domain")?;

    Ok(result.last_insert_rowid())
}

/// Check if a domain is excluded from archiving.
pub async fn is_domain_excluded(pool: &SqlitePool, domain: &str) -> Result<bool> {
    let result: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM excluded_domains WHERE domain = ? AND is_active = 1 LIMIT 1",
    )
    .bind(domain)
    .fetch_optional(pool)
    .await
    .context("Failed to check excluded domain")?;

    Ok(result.is_some())
}

/// Get all active excluded domains.
pub async fn get_active_excluded_domains(
    pool: &SqlitePool,
) -> Result<Vec<crate::db::ExcludedDomain>> {
    let domains = sqlx::query_as::<_, crate::db::ExcludedDomain>(
        "SELECT id, domain, reason, is_active, created_at, created_by_user_id, updated_at FROM excluded_domains WHERE is_active = 1 ORDER BY domain",
    )
    .fetch_all(pool)
    .await
    .context("Failed to get active excluded domains")?;

    Ok(domains)
}

/// Get all excluded domains (including inactive).
pub async fn get_all_excluded_domains(pool: &SqlitePool) -> Result<Vec<crate::db::ExcludedDomain>> {
    let domains = sqlx::query_as::<_, crate::db::ExcludedDomain>(
        "SELECT id, domain, reason, is_active, created_at, created_by_user_id, updated_at FROM excluded_domains ORDER BY is_active DESC, domain",
    )
    .fetch_all(pool)
    .await
    .context("Failed to get all excluded domains")?;

    Ok(domains)
}

/// Update an excluded domain's active status.
pub async fn update_excluded_domain_status(
    pool: &SqlitePool,
    domain: &str,
    is_active: bool,
) -> Result<()> {
    sqlx::query(
        "UPDATE excluded_domains SET is_active = ?, updated_at = CURRENT_TIMESTAMP WHERE domain = ?",
    )
    .bind(is_active as i32)
    .bind(domain)
    .execute(pool)
    .await
    .context("Failed to update excluded domain status")?;

    Ok(())
}

/// Delete an excluded domain.
pub async fn delete_excluded_domain(pool: &SqlitePool, domain: &str) -> Result<()> {
    sqlx::query("DELETE FROM excluded_domains WHERE domain = ?")
        .bind(domain)
        .execute(pool)
        .await
        .context("Failed to delete excluded domain")?;

    Ok(())
}

// ============================================================================
// Thread Archive Jobs queries
// ============================================================================

use super::models::NewThreadArchiveJob;

/// Insert a new thread archive job.
pub async fn insert_thread_archive_job(
    pool: &SqlitePool,
    job: &NewThreadArchiveJob,
) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO thread_archive_jobs (thread_url, rss_url, user_id)
        VALUES (?, ?, ?)
        ",
    )
    .bind(&job.thread_url)
    .bind(&job.rss_url)
    .bind(job.user_id)
    .execute(pool)
    .await
    .context("Failed to insert thread archive job")?;

    Ok(result.last_insert_rowid())
}

/// Get a thread archive job by ID.
pub async fn get_thread_archive_job(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<ThreadArchiveJob>> {
    sqlx::query_as("SELECT * FROM thread_archive_jobs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("Failed to fetch thread archive job")
}

/// Get pending thread archive jobs for processing.
pub async fn get_pending_thread_archive_jobs(
    pool: &SqlitePool,
    limit: i64,
) -> Result<Vec<ThreadArchiveJob>> {
    sqlx::query_as(
        r"
        SELECT * FROM thread_archive_jobs
        WHERE status = 'pending'
        ORDER BY created_at ASC
        LIMIT ?
        ",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch pending thread archive jobs")
}

/// Set thread archive job to processing status.
pub async fn set_thread_archive_job_processing(
    pool: &SqlitePool,
    id: i64,
    total_posts: Option<i64>,
) -> Result<()> {
    sqlx::query(
        r"
        UPDATE thread_archive_jobs
        SET status = 'processing', started_at = datetime('now'), total_posts = ?
        WHERE id = ?
        ",
    )
    .bind(total_posts)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to set thread archive job processing")?;

    Ok(())
}

/// Update thread archive job progress.
pub async fn update_thread_archive_job_progress(
    pool: &SqlitePool,
    id: i64,
    processed_posts: i64,
    new_links_found: i64,
    archives_created: i64,
    skipped_links: i64,
) -> Result<()> {
    sqlx::query(
        r"
        UPDATE thread_archive_jobs
        SET processed_posts = ?, new_links_found = ?, archives_created = ?, skipped_links = ?
        WHERE id = ?
        ",
    )
    .bind(processed_posts)
    .bind(new_links_found)
    .bind(archives_created)
    .bind(skipped_links)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to update thread archive job progress")?;

    Ok(())
}

/// Set thread archive job as complete.
pub async fn set_thread_archive_job_complete(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query(
        r"
        UPDATE thread_archive_jobs
        SET status = 'complete', completed_at = datetime('now')
        WHERE id = ?
        ",
    )
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to set thread archive job complete")?;

    Ok(())
}

/// Set thread archive job as failed.
pub async fn set_thread_archive_job_failed(pool: &SqlitePool, id: i64, error: &str) -> Result<()> {
    sqlx::query(
        r"
        UPDATE thread_archive_jobs
        SET status = 'failed', error_message = ?, completed_at = datetime('now')
        WHERE id = ?
        ",
    )
    .bind(error)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to set thread archive job failed")?;

    Ok(())
}

/// Check if a thread archive job exists for this URL recently (within last hour).
pub async fn thread_archive_job_exists_recent(pool: &SqlitePool, thread_url: &str) -> Result<bool> {
    let result: Option<(i64,)> = sqlx::query_as(
        r"
        SELECT id FROM thread_archive_jobs
        WHERE thread_url = ?
        AND created_at > datetime('now', '-1 hour')
        LIMIT 1
        ",
    )
    .bind(thread_url)
    .fetch_optional(pool)
    .await
    .context("Failed to check recent thread archive job")?;

    Ok(result.is_some())
}

/// Count thread archive jobs from a user in the last hour (for rate limiting).
pub async fn count_user_thread_archive_jobs_last_hour(
    pool: &SqlitePool,
    user_id: i64,
) -> Result<i64> {
    let result: (i64,) = sqlx::query_as(
        r"
        SELECT COUNT(*) FROM thread_archive_jobs
        WHERE user_id = ?
        AND created_at > datetime('now', '-1 hour')
        ",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .context("Failed to count user thread archive jobs")?;

    Ok(result.0)
}

/// Get recent thread archive jobs for a user.
pub async fn get_user_thread_archive_jobs(
    pool: &SqlitePool,
    user_id: i64,
    limit: i64,
) -> Result<Vec<ThreadArchiveJob>> {
    sqlx::query_as(
        r"
        SELECT * FROM thread_archive_jobs
        WHERE user_id = ?
        ORDER BY created_at DESC
        LIMIT ?
        ",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to fetch user thread archive jobs")
}

// ============================================================================
// Forum Account Link queries
// ============================================================================

use super::models::ForumAccountLink;

/// Get a forum account link by archive user ID.
pub async fn get_forum_link_by_user_id(
    pool: &SqlitePool,
    user_id: i64,
) -> Result<Option<ForumAccountLink>> {
    sqlx::query_as("SELECT * FROM forum_account_links WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .context("Failed to get forum link by user ID")
}

/// Get a forum account link by forum username.
pub async fn get_forum_link_by_forum_username(
    pool: &SqlitePool,
    forum_username: &str,
) -> Result<Option<ForumAccountLink>> {
    sqlx::query_as("SELECT * FROM forum_account_links WHERE forum_username = ?")
        .bind(forum_username)
        .fetch_optional(pool)
        .await
        .context("Failed to get forum link by forum username")
}

/// Create a new forum account link.
pub async fn create_forum_account_link(
    pool: &SqlitePool,
    user_id: i64,
    forum_username: &str,
    linked_via_post_guid: &str,
    linked_via_post_url: &str,
    forum_author_raw: Option<&str>,
    post_title: Option<&str>,
    post_published_at: Option<&str>,
) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO forum_account_links
            (user_id, forum_username, linked_via_post_guid, linked_via_post_url, forum_author_raw, post_title, post_published_at)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ",
    )
    .bind(user_id)
    .bind(forum_username)
    .bind(linked_via_post_guid)
    .bind(linked_via_post_url)
    .bind(forum_author_raw)
    .bind(post_title)
    .bind(post_published_at)
    .execute(pool)
    .await
    .context("Failed to create forum account link")?;

    Ok(result.last_insert_rowid())
}

/// Check if a user has a forum account link.
pub async fn user_has_forum_link(pool: &SqlitePool, user_id: i64) -> Result<bool> {
    let result: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM forum_account_links WHERE user_id = ? LIMIT 1")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .context("Failed to check forum link status")?;

    Ok(result.is_some())
}

/// Get all forum account links with associated usernames.
pub async fn get_all_forum_links(pool: &SqlitePool) -> Result<Vec<ForumAccountLink>> {
    sqlx::query_as(
        r"
        SELECT f.*
        FROM forum_account_links f
        ORDER BY f.created_at DESC
        ",
    )
    .fetch_all(pool)
    .await
    .context("Failed to get all forum links")
}

/// Delete a forum account link and reset the user's display name.
///
/// This removes the link and clears the display_name from the user,
/// effectively "unlocking" their display name for manual editing.
pub async fn delete_forum_link(pool: &SqlitePool, link_id: i64) -> Result<Option<i64>> {
    // First get the user_id from the link
    let link: Option<ForumAccountLink> =
        sqlx::query_as("SELECT * FROM forum_account_links WHERE id = ?")
            .bind(link_id)
            .fetch_optional(pool)
            .await
            .context("Failed to get forum link")?;

    let Some(link) = link else {
        return Ok(None);
    };

    let user_id = link.user_id;

    // Delete the link
    sqlx::query("DELETE FROM forum_account_links WHERE id = ?")
        .bind(link_id)
        .execute(pool)
        .await
        .context("Failed to delete forum link")?;

    // Reset the user's display name to NULL
    sqlx::query("UPDATE users SET display_name = NULL, updated_at = datetime('now') WHERE id = ?")
        .bind(user_id)
        .execute(pool)
        .await
        .context("Failed to reset user display name")?;

    Ok(Some(user_id))
}

/// Get a forum account link by ID.
pub async fn get_forum_link_by_id(
    pool: &SqlitePool,
    link_id: i64,
) -> Result<Option<ForumAccountLink>> {
    sqlx::query_as("SELECT * FROM forum_account_links WHERE id = ?")
        .bind(link_id)
        .fetch_optional(pool)
        .await
        .context("Failed to get forum link by ID")
}

// ========== Open Graph Metadata ==========

/// Update Open Graph metadata for an archive.
///
/// This stores extracted OG metadata and marks the extraction as attempted.
/// Pass None for fields that weren't found in the HTML.
pub async fn update_archive_og_metadata(
    pool: &SqlitePool,
    archive_id: i64,
    og_title: Option<&str>,
    og_description: Option<&str>,
    og_image: Option<&str>,
    og_type: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r"
        UPDATE archives
        SET og_title = ?,
            og_description = ?,
            og_image = ?,
            og_type = ?,
            og_extracted_at = datetime('now'),
            og_extraction_attempted = 1
        WHERE id = ?
        ",
    )
    .bind(og_title)
    .bind(og_description)
    .bind(og_image)
    .bind(og_type)
    .bind(archive_id)
    .execute(pool)
    .await
    .context("Failed to update archive OG metadata")?;

    Ok(())
}

/// Mark OG extraction as attempted for an archive without storing metadata.
///
/// This is used when extraction fails or no OG metadata is found,
/// to prevent repeated extraction attempts.
pub async fn mark_og_extraction_attempted(pool: &SqlitePool, archive_id: i64) -> Result<()> {
    sqlx::query(
        r"
        UPDATE archives
        SET og_extraction_attempted = 1,
            og_extracted_at = datetime('now')
        WHERE id = ?
        ",
    )
    .bind(archive_id)
    .execute(pool)
    .await
    .context("Failed to mark OG extraction as attempted")?;

    Ok(())
}

// ========== Subtitle Languages ==========

/// Insert or update a subtitle language entry.
///
/// Uses ON CONFLICT to upsert - if an entry already exists for the artifact,
/// it will be updated instead of inserted.
pub async fn upsert_subtitle_language(
    pool: &SqlitePool,
    artifact_id: i64,
    language: &str,
    detected_from: &str,
    is_auto: bool,
) -> Result<i64> {
    let result = sqlx::query(
        r"
        INSERT INTO subtitle_languages (artifact_id, language, detected_from, is_auto)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(artifact_id) DO UPDATE SET
            language = excluded.language,
            detected_from = excluded.detected_from,
            is_auto = excluded.is_auto,
            updated_at = datetime('now')
        RETURNING id
        ",
    )
    .bind(artifact_id)
    .bind(language)
    .bind(detected_from)
    .bind(is_auto)
    .fetch_one(pool)
    .await
    .context("Failed to upsert subtitle language")?;

    Ok(result.get::<i64, _>("id"))
}

/// Get subtitle language info for an artifact.
pub async fn get_subtitle_language_for_artifact(
    pool: &SqlitePool,
    artifact_id: i64,
) -> Result<Option<SubtitleLanguage>> {
    sqlx::query_as("SELECT * FROM subtitle_languages WHERE artifact_id = ?")
        .bind(artifact_id)
        .fetch_optional(pool)
        .await
        .context("Failed to get subtitle language for artifact")
}

/// Get all subtitle language entries for admin listing.
///
/// Returns entries with additional archive info for context.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct SubtitleLanguageWithContext {
    pub id: i64,
    pub artifact_id: i64,
    pub language: String,
    pub detected_from: String,
    pub is_auto: bool,
    pub created_at: String,
    pub updated_at: String,
    // Context from joins
    pub archive_id: i64,
    pub s3_key: String,
    pub normalized_url: Option<String>,
}

/// Get all subtitle languages with context for admin panel.
pub async fn get_all_subtitle_languages(
    pool: &SqlitePool,
) -> Result<Vec<SubtitleLanguageWithContext>> {
    sqlx::query_as(
        r"
        SELECT
            sl.id,
            sl.artifact_id,
            sl.language,
            sl.detected_from,
            sl.is_auto,
            sl.created_at,
            sl.updated_at,
            aa.archive_id,
            aa.s3_key,
            l.normalized_url
        FROM subtitle_languages sl
        JOIN archive_artifacts aa ON sl.artifact_id = aa.id
        JOIN archives a ON aa.archive_id = a.id
        LEFT JOIN links l ON a.link_id = l.id
        ORDER BY sl.created_at DESC
        ",
    )
    .fetch_all(pool)
    .await
    .context("Failed to get all subtitle languages")
}

/// Delete a subtitle language entry by ID.
pub async fn delete_subtitle_language(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result = sqlx::query("DELETE FROM subtitle_languages WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to delete subtitle language")?;

    Ok(result.rows_affected() > 0)
}

/// Get subtitle languages for all artifacts of an archive.
///
/// Returns a map of artifact_id -> SubtitleLanguage for efficient lookup.
pub async fn get_subtitle_languages_for_archive(
    pool: &SqlitePool,
    archive_id: i64,
) -> Result<std::collections::HashMap<i64, SubtitleLanguage>> {
    let rows: Vec<SubtitleLanguage> = sqlx::query_as(
        r"
        SELECT sl.*
        FROM subtitle_languages sl
        JOIN archive_artifacts aa ON sl.artifact_id = aa.id
        WHERE aa.archive_id = ?
        ",
    )
    .bind(archive_id)
    .fetch_all(pool)
    .await
    .context("Failed to get subtitle languages for archive")?;

    Ok(rows.into_iter().map(|sl| (sl.artifact_id, sl)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_topic_id_from_thread_key() {
        // Valid thread keys with numeric topic IDs
        assert_eq!(
            extract_topic_id_from_thread_key("discuss.example.com:2147"),
            Some(2147)
        );
        assert_eq!(
            extract_topic_id_from_thread_key("forum.test.org:123"),
            Some(123)
        );
        assert_eq!(extract_topic_id_from_thread_key("example.com:1"), Some(1));

        // Invalid thread keys (non-numeric after colon)
        assert_eq!(
            extract_topic_id_from_thread_key("example.com:/path/to/thing"),
            None
        );
        assert_eq!(extract_topic_id_from_thread_key("example.com:abc"), None);

        // Edge cases
        assert_eq!(extract_topic_id_from_thread_key("no-colon"), None);
        assert_eq!(extract_topic_id_from_thread_key(""), None);
        assert_eq!(extract_topic_id_from_thread_key(":123"), Some(123));
    }

    #[test]
    fn test_thread_key_from_url() {
        // Standard Discourse URLs
        assert_eq!(
            thread_key_from_url("https://discuss.example.com/t/topic-slug/2147"),
            "discuss.example.com:2147"
        );
        assert_eq!(
            thread_key_from_url("https://discuss.example.com/t/topic-slug/2147/5"),
            "discuss.example.com:2147"
        );

        // Non-standard URLs
        assert_eq!(
            thread_key_from_url("https://example.com/other/path"),
            "example.com:/other/path"
        );

        // Invalid URLs
        assert_eq!(thread_key_from_url("not-a-url"), "not-a-url");
    }
}
