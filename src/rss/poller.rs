use std::time::Duration;

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use sha2::{Digest, Sha256};
use tracing::{debug, error, info, trace, warn};

use crate::config::Config;
use crate::constants::ARCHIVER_HONEST_USER_AGENT;
use crate::db::{
    self, create_audit_event, create_forum_account_link, create_pending_archive,
    display_name_exists, get_archive_by_link_id, get_forum_link_by_forum_username,
    get_forum_link_by_user_id, get_link_by_normalized_url, get_post_by_guid, get_user_by_username,
    insert_link, insert_link_occurrence, insert_post, link_occurrence_exists, update_post,
    update_user_approval, update_user_profile, Database, LatestPostsResponse, NewLink,
    NewLinkOccurrence, NewPost,
};
use crate::handlers::normalize_url;
use crate::rss::link_extractor::{extract_links, ExtractedLink};

/// Regex to match the link_archive_account command at the start of text content.
static LINK_ACCOUNT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*link_archive_account:(\S+)").expect("Invalid regex"));

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
    let mut total_new_count = 0;
    let mut before_post_id: Option<i64> = None;

    // Fetch multiple pages if configured
    // Discourse /posts.rss uses cursor-based pagination with 'before' parameter
    for page_num in 0..config.rss_max_pages {
        let (page_new_count, min_post_id) =
            fetch_and_process_page(client, config, db, before_post_id).await?;
        total_new_count += page_new_count;

        // If we got no posts, we've reached the end
        if min_post_id.is_none() {
            debug!(
                page = page_num,
                "No posts in this batch, stopping pagination"
            );
            break;
        }

        // Use the minimum post ID from this batch as the 'before' cursor for the next request
        before_post_id = min_post_id;

        // Optimization: If the first page has no new posts, all posts are already in DB
        // No need to fetch older pages - stop immediately to save resources
        if page_new_count == 0 {
            if page_num == 0 {
                trace!("First page has no new posts, all content is up to date");
            } else {
                debug!(
                    page = page_num,
                    "No new posts on this page, stopping pagination"
                );
            }
            break;
        }

        // Add a small delay between pages to be polite
        if page_num + 1 < config.rss_max_pages {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    Ok(total_new_count)
}

/// Fetch and process a single page of the RSS feed.
///
/// Returns (new_count, min_post_id) where min_post_id is used for cursor pagination.
///
/// # Errors
///
/// Returns an error if the RSS feed cannot be fetched or parsed.
async fn fetch_and_process_page(
    client: &reqwest::Client,
    config: &Config,
    db: &Database,
    before_post_id: Option<i64>,
) -> Result<(usize, Option<i64>)> {
    // Convert RSS URL to JSON URL and build with 'before' parameter for cursor-based pagination
    let json_url = config.rss_url.replace("/posts.rss", "/posts.json");

    let url = if let Some(before_id) = before_post_id {
        let separator = if json_url.contains('?') { "&" } else { "?" };
        format!("{json_url}{separator}before={before_id}")
    } else {
        json_url
    };

    trace!(url = %url, before_post_id, "Fetching posts JSON feed page");

    let response = client
        .get(&url)
        .header("User-Agent", ARCHIVER_HONEST_USER_AGENT)
        .send()
        .await
        .context("Failed to fetch RSS feed")?;

    if !response.status().is_success() {
        anyhow::bail!("Posts JSON fetch failed with status {}", response.status());
    }

    // Parse JSON response directly (much simpler than XML parsing!)
    let json_response: LatestPostsResponse = response
        .json()
        .await
        .context("Failed to parse JSON response")?;

    let mut new_count = 0;
    let mut min_post_id: Option<i64> = None;

    // Sort posts by created_at timestamp (oldest first) to ensure chronological processing.
    // This is critical for security-sensitive operations like link_archive_account,
    // where processing order determines who successfully claims an account.
    let mut posts = json_response.latest_posts;
    posts.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    // Get base URL and domain for GUID/URL construction
    let base_url = config
        .discourse_base_url()
        .context("Failed to get discourse base URL")?;
    let domain = extract_domain(&config.rss_url).unwrap_or_else(|| "unknown".to_string());

    for post in posts {
        // Construct GUID in EXACT same format as RSS: {domain}-post-{id}
        // Example: "discuss.criticalfallibilism.com-post-20218"
        let guid = format!("{}-post-{}", domain, post.id);

        // Track minimum post ID for pagination
        min_post_id = Some(match min_post_id {
            Some(current_min) => current_min.min(post.id),
            None => post.id,
        });

        // Check if we've seen this post before
        let existing = get_post_by_guid(db.pool(), &guid).await?;
        let content_html = post.cooked.clone();
        let content_hash = compute_hash(&content_html);

        let discourse_url = format!("{}{}", base_url, post.post_url);

        // Standardize on simple @username format (ignore display name)
        let author = Some(format!("@{}", post.username));

        // Use topic title as post title
        let title = post.topic_title.clone();

        // Use created_at timestamp (already in correct format)
        let published_at = Some(post.created_at.clone());

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

        // Check for account linking command at start of post
        if let Some(target_username) = extract_link_account_command(&content_html) {
            process_account_link_command(
                db,
                &target_username,
                new_post.author.as_deref(),
                &new_post.guid,
                &new_post.discourse_url,
                new_post.title.as_deref(),
                new_post.published_at.as_deref(),
            )
            .await;
        }
    }

    Ok((new_count, min_post_id))
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
            // Fetch the post to get its publication date
            let post = db::get_post(db.pool(), post_id).await?;
            let post_date = post.and_then(|p| p.published_at);

            debug!(url = %link.url, post_date = ?post_date, "Creating pending archive");
            create_pending_archive(db.pool(), link_id, post_date.as_deref()).await?;
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

/// Extract Discourse post ID from GUID.
///
/// Discourse RSS GUIDs are typically URLs like "https://discuss.example.com/p/12345"
/// where 12345 is the post ID.
///
/// Note: Not currently used in JSON API code path (we have post.id directly),
/// but kept as a utility for processing historical RSS-based GUIDs.
#[allow(dead_code)]
fn extract_post_id_from_guid(guid: &str) -> Option<i64> {
    // Try parsing as URL first
    if let Ok(url) = url::Url::parse(guid) {
        // Get the last path segment
        let path = url.path();
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        // Look for numeric segment (post ID)
        for segment in segments.iter().rev() {
            if let Ok(id) = segment.parse::<i64>() {
                return Some(id);
            }
        }
    }

    // Fallback: try to find any number in the GUID string
    // This handles cases where GUID might not be a valid URL
    guid.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<i64>().ok())
        .next_back()
}

/// Strip HTML tags from content to get plain text.
fn strip_html_tags(html: &str) -> String {
    // Use scraper to parse and extract text
    let fragment = scraper::Html::parse_fragment(html);
    let mut text = String::new();
    for node in fragment.root_element().descendants() {
        if let Some(txt) = node.value().as_text() {
            text.push_str(txt);
        }
    }
    text
}

/// Extract the link_archive_account command from HTML content.
///
/// Returns Some(username) if the content starts with `link_archive_account:<username>`.
fn extract_link_account_command(html: &str) -> Option<String> {
    let text = strip_html_tags(html);
    LINK_ACCOUNT_RE
        .captures(text.trim())
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

/// Extract the forum username from an RSS author name.
///
/// RSS author names are in the format `@Username Firstname Lastname`.
/// This extracts just the `@Username` part (the @ followed by adjacent non-whitespace characters).
fn extract_forum_display_name(author: &str) -> Option<String> {
    let trimmed = author.trim();
    if !trimmed.starts_with('@') {
        return None;
    }
    // Find the first whitespace to get just @Username
    let username = trimmed.split_whitespace().next().filter(|s| s.len() > 1)?; // Must have at least @ and one character
    Some(username.to_string())
}

/// Process a link_archive_account command from a forum post.
///
/// This links a forum account to an existing archive account, sets the display name,
/// and auto-approves the user.
async fn process_account_link_command(
    db: &Database,
    target_username: &str,
    forum_author: Option<&str>,
    post_guid: &str,
    post_url: &str,
    post_title: Option<&str>,
    published_at: Option<&str>,
) {
    let forum_username = match forum_author {
        Some(name) if !name.is_empty() => name,
        _ => {
            warn!(
                post_guid = %post_guid,
                target_username = %target_username,
                "Cannot link account: post has no author"
            );
            return;
        }
    };

    // Extract display name from forum author (e.g., "@Max Max" -> "@Max")
    let forum_display_name = match extract_forum_display_name(forum_username) {
        Some(name) => name,
        None => {
            warn!(
                post_guid = %post_guid,
                forum_username = %forum_username,
                "Cannot extract display name from forum author (must start with @)"
            );
            return;
        }
    };

    // Check if this forum account is already linked
    match get_forum_link_by_forum_username(db.pool(), forum_username).await {
        Ok(Some(_)) => {
            trace!(
                forum_username = %forum_username,
                "Forum account already linked to an archive account, skipping"
            );
            return;
        }
        Ok(None) => {}
        Err(e) => {
            error!(
                forum_username = %forum_username,
                "Failed to check forum link status: {e:#}"
            );
            return;
        }
    }

    // Find the target archive account by username
    let target_user = match get_user_by_username(db.pool(), target_username).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            warn!(
                target_username = %target_username,
                forum_username = %forum_username,
                post_guid = %post_guid,
                "Archive account not found for link command"
            );
            return;
        }
        Err(e) => {
            error!(
                target_username = %target_username,
                "Failed to look up archive account: {e:#}"
            );
            return;
        }
    };

    // Check if this archive account is already linked
    match get_forum_link_by_user_id(db.pool(), target_user.id).await {
        Ok(Some(_)) => {
            trace!(
                target_username = %target_username,
                "Archive account already linked to a forum account, skipping"
            );
            return;
        }
        Ok(None) => {}
        Err(e) => {
            error!(
                target_username = %target_username,
                "Failed to check archive account link status: {e:#}"
            );
            return;
        }
    }

    // Check if the forum display name is already taken by another user
    match display_name_exists(db.pool(), &forum_display_name, Some(target_user.id)).await {
        Ok(true) => {
            warn!(
                forum_display_name = %forum_display_name,
                target_username = %target_username,
                "Forum display name is already taken, cannot link"
            );
            return;
        }
        Ok(false) => {}
        Err(e) => {
            error!(
                forum_display_name = %forum_display_name,
                "Failed to check display name availability: {e:#}"
            );
            return;
        }
    }

    // Create the forum account link
    if let Err(e) = create_forum_account_link(
        db.pool(),
        target_user.id,
        forum_username,
        post_guid,
        post_url,
        Some(&forum_display_name),
        post_title,
        published_at,
    )
    .await
    {
        error!(
            target_username = %target_username,
            forum_username = %forum_username,
            "Failed to create forum account link: {e:#}"
        );
        return;
    }

    // Update user's display_name to extracted forum username (e.g., "@Max")
    if let Err(e) = update_user_profile(
        db.pool(),
        target_user.id,
        target_user.email.as_deref(),
        Some(&forum_display_name),
    )
    .await
    {
        error!(
            target_username = %target_username,
            "Failed to update display name: {e:#}"
        );
        // Continue anyway - the link is created, just display name failed
    }

    // Approve the user
    if let Err(e) = update_user_approval(db.pool(), target_user.id, true).await {
        error!(
            target_username = %target_username,
            "Failed to approve user: {e:#}"
        );
        // Continue anyway - user can be manually approved
    }

    // Create audit event
    let metadata = serde_json::json!({
        "forum_username": forum_username,
        "post_guid": post_guid,
        "post_url": post_url,
    });
    if let Err(e) = create_audit_event(
        db.pool(),
        Some(target_user.id),
        "forum_account_linked",
        Some("user"),
        Some(target_user.id),
        Some(&metadata.to_string()),
        None, // No IP - triggered by RSS
        None,
        None,
    )
    .await
    {
        error!(
            target_username = %target_username,
            "Failed to create audit event: {e:#}"
        );
    }

    info!(
        target_username = %target_username,
        forum_username = %forum_username,
        post_guid = %post_guid,
        "Successfully linked forum account to archive account and approved user"
    );
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

    #[test]
    fn test_extract_post_id_from_guid() {
        // Standard Discourse GUID format
        assert_eq!(
            extract_post_id_from_guid("https://discuss.example.com/p/12345"),
            Some(12345)
        );

        // With additional path segments
        assert_eq!(
            extract_post_id_from_guid("https://discuss.example.com/t/topic-name/123/456"),
            Some(456)
        );

        // Simple numeric GUID
        assert_eq!(extract_post_id_from_guid("98765"), Some(98765));

        // No numeric ID
        assert_eq!(extract_post_id_from_guid("https://example.com/about"), None);

        // Empty string
        assert_eq!(extract_post_id_from_guid(""), None);
    }

    #[test]
    fn test_extract_link_account_command() {
        // Simple plain text at start
        assert_eq!(
            extract_link_account_command("link_archive_account:TestUser123"),
            Some("TestUser123".to_string())
        );

        // With leading whitespace
        assert_eq!(
            extract_link_account_command("  link_archive_account:TestUser123"),
            Some("TestUser123".to_string())
        );

        // Wrapped in HTML paragraph
        assert_eq!(
            extract_link_account_command("<p>link_archive_account:TestUser123</p>"),
            Some("TestUser123".to_string())
        );

        // With HTML and whitespace
        assert_eq!(
            extract_link_account_command("<p>  link_archive_account:MyUser  </p>"),
            Some("MyUser".to_string())
        );

        // Not at start - should return None
        assert_eq!(
            extract_link_account_command("Hello link_archive_account:TestUser123"),
            None
        );

        // Not at start with HTML
        assert_eq!(
            extract_link_account_command("<p>Hello</p><p>link_archive_account:TestUser123</p>"),
            None
        );

        // Different text at start
        assert_eq!(extract_link_account_command("Some other text here"), None);

        // Empty string
        assert_eq!(extract_link_account_command(""), None);

        // Command with additional text after username
        assert_eq!(
            extract_link_account_command("link_archive_account:User123 and more text"),
            Some("User123".to_string())
        );
    }

    #[test]
    fn test_strip_html_tags() {
        assert_eq!(strip_html_tags("<p>Hello World</p>"), "Hello World");
        assert_eq!(strip_html_tags("<div><p>Nested</p></div>"), "Nested");
        assert_eq!(strip_html_tags("Plain text"), "Plain text");
        assert_eq!(
            strip_html_tags("<p>Multiple</p><p>Paragraphs</p>"),
            "MultipleParagraphs"
        );
    }

    #[test]
    fn test_extract_forum_display_name() {
        // Standard case: @Username followed by real name
        assert_eq!(
            extract_forum_display_name("@Max Max"),
            Some("@Max".to_string())
        );

        // Multiple name parts
        assert_eq!(
            extract_forum_display_name("@JohnDoe John Doe"),
            Some("@JohnDoe".to_string())
        );

        // Just username, no real name
        assert_eq!(
            extract_forum_display_name("@Username"),
            Some("@Username".to_string())
        );

        // With leading/trailing whitespace
        assert_eq!(
            extract_forum_display_name("  @User  Firstname Lastname  "),
            Some("@User".to_string())
        );

        // No @ prefix - should return None
        assert_eq!(extract_forum_display_name("Username"), None);
        assert_eq!(extract_forum_display_name("User Name"), None);

        // Empty string
        assert_eq!(extract_forum_display_name(""), None);

        // Just @ with no username - should return None
        assert_eq!(extract_forum_display_name("@"), None);
        assert_eq!(extract_forum_display_name("@ something"), None);
    }
}
