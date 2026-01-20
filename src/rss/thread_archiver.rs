//! Thread-specific RSS archiving for bulk archive requests.
//!
//! This module handles fetching a Discourse thread's RSS feed and archiving
//! all links found within the thread's posts.

use std::time::Duration;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use tracing::{debug, info, trace, warn};

use crate::config::Config;
use crate::constants::ARCHIVER_HONEST_USER_AGENT;
use crate::db::{
    self, create_pending_archive, get_archive_by_link_id, get_link_by_normalized_url,
    get_post_by_guid, insert_link, insert_link_occurrence, insert_post, is_domain_excluded,
    link_occurrence_exists, update_post, update_thread_archive_job_progress, Database, NewLink,
    NewLinkOccurrence, NewPost, ThreadArchiveJob,
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

/// Archive all links from a thread RSS feed.
///
/// Fetches the thread's RSS, processes each post, extracts links,
/// and creates pending archives for new links.
///
/// # Errors
///
/// Returns an error if the RSS feed cannot be fetched or parsed.
pub async fn archive_thread_links(
    config: &Config,
    db: &Database,
    job: &ThreadArchiveJob,
) -> Result<ArchiveProgress> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("Failed to build HTTP client")?;

    info!(job_id = job.id, rss_url = %job.rss_url, "Fetching thread RSS feed");

    let response = client
        .get(&job.rss_url)
        .header("User-Agent", ARCHIVER_HONEST_USER_AGENT)
        .send()
        .await
        .context("Failed to fetch thread RSS feed")?;

    if !response.status().is_success() {
        anyhow::bail!("RSS fetch failed with status {}", response.status());
    }

    let body = response.bytes().await.context("Failed to read RSS body")?;
    let feed = feed_rs::parser::parse(&body[..]).context("Failed to parse RSS feed")?;

    let total_posts = feed.entries.len();
    info!(
        job_id = job.id,
        total_posts, "Parsed thread RSS, processing posts"
    );

    // Update job with total posts count
    db::set_thread_archive_job_processing(db.pool(), job.id, Some(total_posts as i64)).await?;

    // Extract forum domain from RSS URL to skip self-links
    let forum_domain = extract_domain(&config.rss_url);

    let mut progress = ArchiveProgress::default();

    for entry in feed.entries {
        let guid = entry.id.clone();

        // Extract post content
        let content_html = entry
            .content
            .as_ref()
            .and_then(|c| c.body.clone())
            .or_else(|| entry.summary.as_ref().map(|s| s.content.clone()))
            .unwrap_or_default();
        let content_hash = compute_hash(&content_html);

        // Get post URL
        let discourse_url = entry
            .links
            .first()
            .map(|l| l.href.clone())
            .unwrap_or_default();

        // Get author
        let author = entry.authors.first().map(|a| a.name.clone());

        // Get title
        let title = entry.title.map(|t| t.content);

        // Get published date
        let published_at = entry.published.map(|d| d.to_rfc3339());

        let new_post = NewPost {
            guid: guid.clone(),
            discourse_url,
            author,
            title,
            body_html: Some(content_html.clone()),
            content_hash: Some(content_hash.clone()),
            published_at: published_at.clone(),
        };

        // Check if we've seen this post before
        let existing = get_post_by_guid(db.pool(), &guid).await?;

        let post_id = if let Some(post) = existing {
            // Check if content changed
            if post.content_hash.as_deref() != Some(&content_hash) {
                debug!(guid = %guid, "Post content changed, updating");
                update_post(db.pool(), post.id, &new_post).await?;
            }
            post.id
        } else {
            debug!(guid = %guid, "New post found from thread archive");
            insert_post(db.pool(), &new_post).await?
        };

        // Extract and process links from this post
        let post_progress = process_post_links(
            db,
            post_id,
            &content_html,
            forum_domain.as_deref(),
            published_at.as_deref(),
        )
        .await?;

        progress.new_links_found += post_progress.new_links_found;
        progress.archives_created += post_progress.archives_created;
        progress.skipped_links += post_progress.skipped_links;
        progress.processed_posts += 1;

        // Update job progress in DB periodically (every post)
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
