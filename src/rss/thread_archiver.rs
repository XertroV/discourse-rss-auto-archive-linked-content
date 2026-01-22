//! Thread-specific JSON API archiving for bulk archive requests.
//!
//! This module handles fetching all posts from a Discourse thread via JSON API
//! and archiving all links found within the thread's posts.

use std::collections::HashSet;
use std::time::Duration;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use tracing::{debug, info, trace, warn};

use crate::config::Config;
use crate::constants::ARCHIVER_HONEST_USER_AGENT;
use crate::db::{
    self, create_pending_archive, get_archive_by_link_id, get_link_by_normalized_url,
    get_post_by_guid, insert_link, insert_link_occurrence, insert_post, is_domain_excluded,
    link_occurrence_exists, update_post, update_thread_archive_job_progress, Database,
    DiscoursePost, DiscoursePostsResponse, NewLink, NewLinkOccurrence, NewPost, ThreadArchiveJob,
};
use crate::handlers::normalize_url;
use crate::rss::link_extractor::extract_links;

/// Progress tracking for thread archive job.
#[derive(Debug, Clone, Default)]
pub struct ArchiveProgress {
    pub processed_posts: i64,
    pub new_links_found: i64,
    pub archives_created: i64,
    pub skipped_links: i64,
}

/// Archive all links from a Discourse thread via JSON API.
///
/// Paginates through all posts in the thread, processes each post, extracts links,
/// and creates pending archives for new links.
///
/// # Errors
///
/// Returns an error if the JSON API cannot be fetched or parsed.
pub async fn archive_thread_links(
    config: &Config,
    db: &Database,
    job: &ThreadArchiveJob,
) -> Result<ArchiveProgress> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("Failed to build HTTP client")?;

    // Extract topic ID from thread URL
    let topic_id = extract_topic_id(&job.thread_url)?;
    let base_url = config
        .discourse_base_url()
        .context("Failed to extract Discourse base URL")?;
    let forum_domain = extract_domain(&config.rss_url);

    info!(
        job_id = job.id,
        topic_id,
        base_url = %base_url,
        "Starting thread archive via JSON API"
    );

    let mut progress = ArchiveProgress::default();
    let mut processed_guids = HashSet::new();
    let mut page_num = 0;
    const PAGE_SIZE: i64 = 20; // Each request returns ~20 posts

    loop {
        // Fetch page - use /t/{topic_id}/{post_number}.json endpoint
        // Pattern: 1, 21, 41, 61, 81... (increments of 20)
        // This endpoint returns posts centered around the specified post_number
        // When post_number is beyond the thread end, it clamps to the last ~20 posts
        let post_number = (page_num * PAGE_SIZE) + 1;
        let url = format!("{}/t/{}/{}.json", base_url, topic_id, post_number);

        debug!(
            job_id = job.id,
            page_num,
            url = %url,
            "Fetching posts page"
        );

        let response = client
            .get(&url)
            .header("User-Agent", ARCHIVER_HONEST_USER_AGENT)
            .send()
            .await
            .context("Failed to fetch posts JSON")?;

        if !response.status().is_success() {
            anyhow::bail!("Posts JSON fetch failed with status {}", response.status());
        }

        let batch: DiscoursePostsResponse = response
            .json()
            .await
            .context("Failed to parse posts JSON")?;

        // Stop if no posts
        if batch.post_stream.posts.is_empty() {
            debug!(
                job_id = job.id,
                page_num, "No more posts, pagination complete"
            );
            break;
        }

        // Update total posts count from first post (if available) on first request
        if page_num == 0 {
            if let Some(first_post) = batch.post_stream.posts.first() {
                if let Some(total) = first_post.posts_count {
                    debug!(
                        job_id = job.id,
                        total_posts = total,
                        "Found total posts count"
                    );
                    db::set_thread_archive_job_processing(db.pool(), job.id, Some(total)).await?;
                }
            }
        }

        // Track how many new posts we found in this batch
        let mut batch_new_count = 0;

        // Process each post in this batch
        for post in &batch.post_stream.posts {
            let guid = format!("topic-{}-post-{}", post.topic_id, post.id);

            // Skip if already processed in this run (handles potential duplicates from pagination)
            if processed_guids.contains(&guid) {
                trace!(guid = %guid, "Skipping already processed post");
                continue;
            }
            processed_guids.insert(guid.clone());
            batch_new_count += 1;

            // Check if exists / changed
            let content_hash = compute_hash(&post.cooked);
            let existing = get_post_by_guid(db.pool(), &guid).await?;

            let post_id = if let Some(existing_post) = existing {
                if existing_post.content_hash.as_deref() == Some(&content_hash) {
                    // Unchanged - skip link processing
                    trace!(guid = %guid, "Post unchanged, skipping");
                    continue;
                }
                // Changed - update and reprocess
                debug!(guid = %guid, "Post content changed, updating");
                let new_post = build_new_post(post, &content_hash, forum_domain.as_deref());
                update_post(db.pool(), existing_post.id, &new_post).await?;
                existing_post.id
            } else {
                // New post - insert
                debug!(guid = %guid, "New post found from thread archive");
                let new_post = build_new_post(post, &content_hash, forum_domain.as_deref());
                insert_post(db.pool(), &new_post).await?
            };

            // Process links
            let post_progress = process_post_links(
                db,
                post_id,
                &post.cooked,
                forum_domain.as_deref(),
                Some(&post.created_at),
            )
            .await?;

            progress.processed_posts += 1;
            progress.new_links_found += post_progress.new_links_found;
            progress.archives_created += post_progress.archives_created;
            progress.skipped_links += post_progress.skipped_links;

            update_thread_archive_job_progress(
                db.pool(),
                job.id,
                progress.processed_posts,
                progress.new_links_found,
                progress.archives_created,
                progress.skipped_links,
            )
            .await?;
        }

        debug!(
            job_id = job.id,
            page_num,
            batch_new_count,
            batch_total = batch.post_stream.posts.len(),
            "Processed batch"
        );

        // Next page
        page_num += 1;

        // Stop if we've gone far enough without finding new posts
        // This handles threads where pagination wraps around to the end
        if page_num > 50 && batch_new_count == 0 {
            debug!(
                job_id = job.id,
                page_num, "Gone far enough with no new posts in batch, stopping pagination"
            );
            break;
        }

        // Rate limiting between requests
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Final check: fetch the latest posts to ensure we didn't miss any
    // Use a high post_number (9999) which will clamp to the last ~20 posts
    debug!(
        job_id = job.id,
        "Fetching latest posts to ensure complete coverage"
    );

    let final_url = format!("{}/t/{}/9999.json", base_url, topic_id);
    let final_response = client
        .get(&final_url)
        .header("User-Agent", ARCHIVER_HONEST_USER_AGENT)
        .send()
        .await
        .context("Failed to fetch final posts batch")?;

    if final_response.status().is_success() {
        if let Ok(final_batch) = final_response.json::<DiscoursePostsResponse>().await {
            let mut final_new_count = 0;
            for post in &final_batch.post_stream.posts {
                let guid = format!("topic-{}-post-{}", post.topic_id, post.id);

                if processed_guids.contains(&guid) {
                    continue;
                }
                processed_guids.insert(guid.clone());
                final_new_count += 1;

                let content_hash = compute_hash(&post.cooked);
                let existing = get_post_by_guid(db.pool(), &guid).await?;

                let post_id = if let Some(existing_post) = existing {
                    if existing_post.content_hash.as_deref() == Some(&content_hash) {
                        continue;
                    }
                    let new_post = build_new_post(post, &content_hash, forum_domain.as_deref());
                    update_post(db.pool(), existing_post.id, &new_post).await?;
                    existing_post.id
                } else {
                    let new_post = build_new_post(post, &content_hash, forum_domain.as_deref());
                    insert_post(db.pool(), &new_post).await?
                };

                let post_progress = process_post_links(
                    db,
                    post_id,
                    &post.cooked,
                    forum_domain.as_deref(),
                    Some(&post.created_at),
                )
                .await?;

                progress.processed_posts += 1;
                progress.new_links_found += post_progress.new_links_found;
                progress.archives_created += post_progress.archives_created;
                progress.skipped_links += post_progress.skipped_links;

                update_thread_archive_job_progress(
                    db.pool(),
                    job.id,
                    progress.processed_posts,
                    progress.new_links_found,
                    progress.archives_created,
                    progress.skipped_links,
                )
                .await?;
            }

            if final_new_count > 0 {
                debug!(
                    job_id = job.id,
                    final_new_count, "Found additional posts in final check"
                );
            }
        }
    }

    info!(
        job_id = job.id,
        processed_posts = progress.processed_posts,
        new_links = progress.new_links_found,
        archives_created = progress.archives_created,
        skipped = progress.skipped_links,
        "Thread archive processing complete"
    );

    Ok(progress)
}

/// Extract topic ID from a Discourse thread URL.
///
/// Supports URLs like:
/// - `https://discuss.example.com/t/topic-slug/123`
/// - `https://discuss.example.com/t/topic-slug/123/45`
///
/// # Errors
///
/// Returns an error if the URL is invalid or doesn't match the expected pattern.
fn extract_topic_id(url: &str) -> Result<i64> {
    let parsed = url::Url::parse(url).context("Invalid URL")?;
    let segments: Vec<_> = parsed
        .path_segments()
        .context("URL has no path segments")?
        .collect();

    if segments.len() >= 3 && segments[0] == "t" {
        segments[2]
            .parse::<i64>()
            .context("Invalid topic ID (not a number)")
    } else {
        anyhow::bail!("Not a valid Discourse topic URL: {}", url)
    }
}

/// Build a NewPost from a DiscoursePost JSON response.
fn build_new_post(post: &DiscoursePost, content_hash: &str, forum_domain: Option<&str>) -> NewPost {
    let guid = format!("topic-{}-post-{}", post.topic_id, post.id);
    let discourse_url = format!(
        "https://{}/t/{}/{}/{}",
        forum_domain.unwrap_or(""),
        post.topic_slug,
        post.topic_id,
        post.post_number
    );

    NewPost {
        guid,
        discourse_url,
        author: Some(post.username.clone()),
        title: None, // We don't have thread title in individual posts
        body_html: Some(post.cooked.clone()),
        content_hash: Some(content_hash.to_string()),
        published_at: Some(post.created_at.clone()),
    }
}

/// Process links extracted from a single post.
async fn process_post_links(
    db: &Database,
    post_id: i64,
    html: &str,
    forum_domain: Option<&str>,
    post_date: Option<&str>,
) -> Result<ArchiveProgress> {
    let extracted = extract_links(html);
    let mut progress = ArchiveProgress::default();

    for link in extracted {
        match process_single_link(db, post_id, &link, forum_domain, post_date).await {
            Ok(result) => {
                progress.new_links_found += i64::from(result.is_new_link);
                progress.archives_created += i64::from(result.archive_created);
                progress.skipped_links += i64::from(result.skipped);
            }
            Err(e) => {
                warn!(url = %link.url, "Failed to process link: {e:#}");
                progress.skipped_links += 1;
            }
        }
    }

    Ok(progress)
}

/// Result of processing a single link.
struct LinkProcessResult {
    is_new_link: bool,
    archive_created: bool,
    skipped: bool,
}

/// Process a single extracted link.
async fn process_single_link(
    db: &Database,
    post_id: i64,
    link: &crate::rss::link_extractor::ExtractedLink,
    forum_domain: Option<&str>,
    post_date: Option<&str>,
) -> Result<LinkProcessResult> {
    // Normalize the URL
    let normalized = normalize_url(&link.url);
    let domain = extract_domain(&normalized).unwrap_or_default();

    // Skip internal links and non-http URLs
    if !normalized.starts_with("http") {
        return Ok(LinkProcessResult {
            is_new_link: false,
            archive_created: false,
            skipped: true,
        });
    }

    // Skip links to the same domain as the forum (self-links)
    if let Some(forum) = forum_domain {
        if domains_match(&domain, forum) {
            return Ok(LinkProcessResult {
                is_new_link: false,
                archive_created: false,
                skipped: true,
            });
        }
    }

    // Skip specific domains that shouldn't be archived
    const SKIP_DOMAINS: &[&str] = &[
        "curi.us",
        // Archive sites - no need to archive the archivers
        "web.archive.org",
        "archive.org",
        "archive.today",
        "archive.is",
        "archive.ph",
        "archive.fo",
        "archive.li",
        "archive.vn",
        "archive.md",
    ];
    let domain_lower = domain.to_lowercase();
    for skip_domain in SKIP_DOMAINS {
        if domain_lower == *skip_domain || domain_lower.ends_with(&format!(".{skip_domain}")) {
            trace!(url = %link.url, domain = %domain, "Skipping domain from archiving");
            return Ok(LinkProcessResult {
                is_new_link: false,
                archive_created: false,
                skipped: true,
            });
        }
    }

    // Check if domain is excluded via admin settings
    if is_domain_excluded(db.pool(), &domain_lower).await? {
        trace!(url = %link.url, domain = %domain, "Domain is excluded from archiving");
        return Ok(LinkProcessResult {
            is_new_link: false,
            archive_created: false,
            skipped: true,
        });
    }

    // Check if we already have this link
    let existing_link = get_link_by_normalized_url(db.pool(), &normalized).await?;
    let is_new_link = existing_link.is_none();

    let link_id = if let Some(existing) = existing_link {
        existing.id
    } else {
        let new_link = NewLink {
            original_url: link.url.clone(),
            normalized_url: normalized.clone(),
            canonical_url: None,
            domain: domain.clone(),
        };
        insert_link(db.pool(), &new_link).await?
    };

    // Check if this occurrence already exists
    if link_occurrence_exists(db.pool(), link_id, post_id).await? {
        return Ok(LinkProcessResult {
            is_new_link,
            archive_created: false,
            skipped: false,
        });
    }

    // Insert the occurrence
    let occurrence = NewLinkOccurrence {
        link_id,
        post_id,
        in_quote: link.in_quote,
        context_snippet: link.context.clone(),
    };
    insert_link_occurrence(db.pool(), &occurrence).await?;

    // Decide if we should create an archive
    let should_archive = should_archive_link(db, link_id, link.in_quote).await?;
    let mut archive_created = false;

    if should_archive {
        // Check if archive already exists
        if get_archive_by_link_id(db.pool(), link_id).await?.is_none() {
            debug!(url = %link.url, post_date = ?post_date, "Creating pending archive from thread");
            create_pending_archive(db.pool(), link_id, post_date).await?;
            archive_created = true;
        }
    }

    Ok(LinkProcessResult {
        is_new_link,
        archive_created,
        skipped: false,
    })
}

/// Determine if a link should be archived.
async fn should_archive_link(db: &Database, link_id: i64, in_quote: bool) -> Result<bool> {
    // If this is a quote link, only archive if it's the first occurrence ever
    if in_quote {
        // Check if there's already an archive for this link
        if get_archive_by_link_id(db.pool(), link_id).await?.is_some() {
            return Ok(false);
        }
        // Check if there's any non-quote occurrence
        if db::link_has_non_quote_occurrence(db.pool(), link_id).await? {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Compute SHA256 hash of content.
fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Extract domain from URL.
fn extract_domain(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(ToString::to_string))
}

/// Check if two domains match (ignoring www prefix and case).
fn domains_match(domain1: &str, domain2: &str) -> bool {
    let d1 = domain1.to_ascii_lowercase();
    let d2 = domain2.to_ascii_lowercase();
    let d1 = d1.strip_prefix("www.").unwrap_or(&d1);
    let d2 = d2.strip_prefix("www.").unwrap_or(&d2);
    d1 == d2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            extract_domain("https://www.example.com/path"),
            Some("www.example.com".to_string())
        );
        assert_eq!(
            extract_domain("https://discuss.criticalfallibilism.com/t/topic/1714.rss"),
            Some("discuss.criticalfallibilism.com".to_string())
        );
    }

    #[test]
    fn test_domains_match() {
        assert!(domains_match("example.com", "example.com"));
        assert!(domains_match("www.example.com", "example.com"));
        assert!(domains_match("EXAMPLE.COM", "example.com"));
        assert!(!domains_match("other.com", "example.com"));
    }
}
