use std::time::Duration;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::db::{
    self, create_pending_archive, get_archive_by_link_id, get_link_by_normalized_url,
    get_post_by_guid, insert_link, insert_link_occurrence, insert_post, link_occurrence_exists,
    update_post, Database, NewLink, NewLinkOccurrence, NewPost,
};
use crate::handlers::normalize_url;
use crate::rss::link_extractor::{extract_links, ExtractedLink};

/// Run the RSS polling loop forever.
pub async fn poll_loop(config: Config, db: Database) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("Failed to build HTTP client");

    let mut consecutive_empty = 0u32;
    let base_interval = config.poll_interval;
    let max_interval = Duration::from_secs(300); // 5 minutes max

    loop {
        match poll_once(&client, &config, &db).await {
            Ok(new_count) => {
                if new_count > 0 {
                    info!(new_posts = new_count, "Processed new posts");
                    consecutive_empty = 0;
                } else {
                    consecutive_empty = consecutive_empty.saturating_add(1);
                    debug!(consecutive_empty, "No new posts");
                }
            }
            Err(e) => {
                error!("Poll error: {e:#}");
                consecutive_empty = consecutive_empty.saturating_add(1);
            }
        }

        // Adaptive polling: increase interval when no new content
        let interval = if consecutive_empty > 10 {
            max_interval.min(base_interval * 2)
        } else if consecutive_empty > 5 {
            base_interval.mul_f32(1.5)
        } else {
            base_interval
        };

        tokio::time::sleep(interval).await;
    }
}

/// Poll the RSS feed once and process any new posts.
///
/// # Errors
///
/// Returns an error if the RSS feed cannot be fetched or parsed.
pub async fn poll_once(client: &reqwest::Client, config: &Config, db: &Database) -> Result<usize> {
    let response = client
        .get(&config.rss_url)
        .header("User-Agent", "discourse-link-archiver/0.1")
        .send()
        .await
        .context("Failed to fetch RSS feed")?;

    if !response.status().is_success() {
        anyhow::bail!("RSS fetch failed with status {}", response.status());
    }

    let body = response.bytes().await.context("Failed to read RSS body")?;
    let feed = feed_rs::parser::parse(&body[..]).context("Failed to parse RSS feed")?;

    let mut new_count = 0;

    for entry in feed.entries {
        let guid = entry.id.clone();

        // Check if we've seen this post before
        let existing = get_post_by_guid(db.pool(), &guid).await?;
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
            published_at,
        };

        let post_id = if let Some(post) = existing {
            // Check if content changed
            if post.content_hash.as_deref() != Some(&content_hash) {
                debug!(guid = %guid, "Post content changed, updating");
                update_post(db.pool(), post.id, &new_post).await?;
            }
            post.id
        } else {
            debug!(guid = %guid, "New post found");
            let id = insert_post(db.pool(), &new_post).await?;
            new_count += 1;
            id
        };

        // Extract and process links
        process_links(db, post_id, &content_html, config).await?;
    }

    Ok(new_count)
}

/// Process links extracted from a post.
async fn process_links(db: &Database, post_id: i64, html: &str, config: &Config) -> Result<()> {
    let extracted = extract_links(html);

    // Extract forum domain from RSS URL to skip self-links
    let forum_domain = extract_domain(&config.rss_url);

    for link in extracted {
        if let Err(e) = process_single_link(db, post_id, &link, forum_domain.as_deref()).await {
            warn!(url = %link.url, "Failed to process link: {e:#}");
        }
    }

    Ok(())
}

/// Process a single extracted link.
async fn process_single_link(
    db: &Database,
    post_id: i64,
    link: &ExtractedLink,
    forum_domain: Option<&str>,
) -> Result<()> {
    // Normalize the URL
    let normalized = normalize_url(&link.url);
    let domain = extract_domain(&normalized).unwrap_or_default();

    // Skip internal links and non-http URLs
    if !normalized.starts_with("http") {
        return Ok(());
    }

    // Skip links to the same domain as the forum (self-links)
    if let Some(forum) = forum_domain {
        if domains_match(&domain, forum) {
            return Ok(());
        }
    }

    // Skip specific domains that shouldn't be archived
    const SKIP_DOMAINS: &[&str] = &["curi.us"];
    let domain_lower = domain.to_lowercase();
    for skip_domain in SKIP_DOMAINS {
        if domain_lower == *skip_domain || domain_lower.ends_with(&format!(".{skip_domain}")) {
            debug!(url = %link.url, domain = %domain, "Skipping domain from archiving");
            return Ok(());
        }
    }

    // Check if we already have this link
    let existing_link = get_link_by_normalized_url(db.pool(), &normalized).await?;

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
        return Ok(());
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

    if should_archive {
        // Check if archive already exists
        if get_archive_by_link_id(db.pool(), link_id).await?.is_none() {
            debug!(url = %link.url, "Creating pending archive");
            create_pending_archive(db.pool(), link_id).await?;
        }
    }

    Ok(())
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
    fn test_compute_hash() {
        let hash1 = compute_hash("hello");
        let hash2 = compute_hash("hello");
        let hash3 = compute_hash("world");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 64); // SHA256 hex is 64 chars
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            extract_domain("https://www.example.com/path"),
            Some("www.example.com".to_string())
        );
        assert_eq!(
            extract_domain("https://reddit.com/r/test"),
            Some("reddit.com".to_string())
        );
        assert_eq!(extract_domain("not a url"), None);
    }

    #[test]
    fn test_domains_match() {
        // Same domain
        assert!(domains_match("example.com", "example.com"));

        // www prefix handling
        assert!(domains_match("www.example.com", "example.com"));
        assert!(domains_match("example.com", "www.example.com"));
        assert!(domains_match("www.example.com", "www.example.com"));

        // Case insensitive
        assert!(domains_match("Example.COM", "example.com"));
        assert!(domains_match("WWW.Example.COM", "example.com"));

        // Different domains
        assert!(!domains_match("example.com", "other.com"));
        assert!(!domains_match("www.example.com", "www.other.com"));

        // Subdomains are different
        assert!(!domains_match("sub.example.com", "example.com"));
        assert!(!domains_match("discuss.example.com", "www.example.com"));
    }
}
