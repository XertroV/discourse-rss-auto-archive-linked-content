use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use serde::Deserialize;
use tracing::debug;

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::ytdlp;

static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        Regex::new(r"^https?://(www\.)?reddit\.com/").unwrap(),
        Regex::new(r"^https?://old\.reddit\.com/").unwrap(),
        Regex::new(r"^https?://m\.reddit\.com/").unwrap(),
        Regex::new(r"^https?://new\.reddit\.com/").unwrap(),
        Regex::new(r"^https?://redd\.it/").unwrap(),
        Regex::new(r"^https?://i\.redd\.it/").unwrap(),
        Regex::new(r"^https?://v\.redd\.it/").unwrap(),
        Regex::new(r"^https?://preview\.redd\.it/").unwrap(),
    ]
});

static SHORTLINK_PATTERN: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^https?://redd\.it/[a-zA-Z0-9]+$").unwrap());

/// Pattern to extract subreddit name from URL.
static SUBREDDIT_PATTERN: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"/r/([a-zA-Z0-9_]+)").unwrap());

/// Known NSFW subreddit name patterns (case-insensitive prefixes/patterns).
const NSFW_SUBREDDIT_PATTERNS: &[&str] = &[
    "nsfw",
    "gonewild",
    "porn",
    "xxx",
    "nude",
    "sex",
    "adult",
    "18_plus",
    "onlyfans",
    "lewd",
    "hentai",
    "rule34",
    "celebnsfw",
    "realgirls",
    "boobs",
    "ass",
    "tits",
];

/// Reddit JSON API response structures
#[derive(Debug, Deserialize)]
struct RedditListing {
    data: ListingData,
}

#[derive(Debug, Deserialize)]
struct ListingData {
    children: Vec<RedditChild>,
}

#[derive(Debug, Deserialize)]
struct RedditChild {
    data: PostData,
}

#[derive(Debug, Deserialize)]
struct PostData {
    title: Option<String>,
    author: Option<String>,
    selftext: Option<String>,
    selftext_html: Option<String>,
    score: Option<i64>,
    num_comments: Option<i64>,
    created_utc: Option<f64>,
    subreddit: Option<String>,
    permalink: Option<String>,
    url: Option<String>,
    is_video: Option<bool>,
    is_self: Option<bool>,
    thumbnail: Option<String>,
    over_18: Option<bool>,
}

pub struct RedditHandler;

impl RedditHandler {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for RedditHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SiteHandler for RedditHandler {
    fn site_id(&self) -> &'static str {
        "reddit"
    }

    fn url_patterns(&self) -> &[Regex] {
        &PATTERNS
    }

    fn priority(&self) -> i32 {
        100
    }

    fn normalize_url(&self, url: &str) -> String {
        let mut normalized = url.to_string();

        // Convert various reddit domains to old.reddit.com
        normalized = normalized.replace("://www.reddit.com/", "://old.reddit.com/");
        normalized = normalized.replace("://m.reddit.com/", "://old.reddit.com/");
        normalized = normalized.replace("://new.reddit.com/", "://old.reddit.com/");
        normalized = normalized.replace("://reddit.com/", "://old.reddit.com/");

        // Apply base normalization
        super::normalize::normalize_url(&normalized)
    }

    async fn archive(
        &self,
        url: &str,
        work_dir: &Path,
        cookies_file: Option<&Path>,
    ) -> Result<ArchiveResult> {
        // Resolve redd.it shortlinks first
        let resolved_url = if is_shortlink(url) {
            match resolve_short_url(url).await {
                Ok(resolved) => {
                    debug!(original = %url, resolved = %resolved, "Resolved redd.it shortlink");
                    resolved
                }
                Err(e) => {
                    debug!("Failed to resolve shortlink, using original URL: {e}");
                    url.to_string()
                }
            }
        } else {
            url.to_string()
        };

        // Normalize URL
        let normalized_url = self.normalize_url(&resolved_url);

        // Check if the subreddit name suggests NSFW content
        let subreddit_nsfw = is_nsfw_subreddit(&normalized_url);

        // Always try to fetch Reddit JSON API data for metadata (this is additive)
        let json_result = fetch_reddit_json(&normalized_url, work_dir).await;
        let json_metadata = json_result.as_ref().ok();

        // Check if JSON API indicates NSFW
        let api_nsfw = json_metadata.and_then(|r| r.is_nsfw).unwrap_or(false);

        // Use yt-dlp for video/media content
        let ytdlp_result = ytdlp::download(&normalized_url, work_dir, cookies_file).await;

        match ytdlp_result {
            Ok(mut result) => {
                // Merge JSON metadata with yt-dlp result
                if let Some(json_data) = json_metadata {
                    // Keep existing yt-dlp metadata but supplement with JSON data
                    if result.title.is_none() {
                        result.title = json_data.title.clone();
                    }
                    if result.author.is_none() {
                        result.author = json_data.author.clone();
                    }
                    if result.text.is_none() {
                        result.text = json_data.text.clone();
                    }
                    // Always include the JSON metadata alongside
                    if result.metadata_json.is_none() {
                        result.metadata_json = json_data.metadata_json.clone();
                    }
                }

                // Set NSFW status - check multiple sources
                if result.is_nsfw.is_none() {
                    if api_nsfw {
                        result.is_nsfw = Some(true);
                        result.nsfw_source = Some("api".to_string());
                    } else if subreddit_nsfw {
                        result.is_nsfw = Some(true);
                        result.nsfw_source = Some("subreddit".to_string());
                    }
                }

                // Also check the metadata JSON for Reddit's over_18 field (from yt-dlp)
                if result.is_nsfw.is_none() {
                    if let Some(ref json_str) = result.metadata_json {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                            if json
                                .get("over_18")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false)
                            {
                                result.is_nsfw = Some(true);
                                result.nsfw_source = Some("metadata".to_string());
                            }
                        }
                    }
                }

                Ok(result)
            }
            Err(e) => {
                // If yt-dlp fails, use JSON API result if available
                debug!("yt-dlp failed for Reddit URL: {e}");
                match json_result {
                    Ok(mut result) => {
                        // Set NSFW status from API or subreddit detection
                        if result.is_nsfw.is_none() {
                            if api_nsfw {
                                result.is_nsfw = Some(true);
                                result.nsfw_source = Some("api".to_string());
                            } else if subreddit_nsfw {
                                result.is_nsfw = Some(true);
                                result.nsfw_source = Some("subreddit".to_string());
                            }
                        }
                        Ok(result)
                    }
                    Err(json_err) => {
                        debug!("JSON API also failed: {json_err}");
                        // Return a minimal result indicating the archive attempt
                        let mut result = ArchiveResult {
                            content_type: "thread".to_string(),
                            ..Default::default()
                        };
                        // Still check subreddit name for NSFW
                        if subreddit_nsfw {
                            result.is_nsfw = Some(true);
                            result.nsfw_source = Some("subreddit".to_string());
                        }
                        Ok(result)
                    }
                }
            }
        }
    }
}

/// Fetch Reddit post data from the JSON API and create an archive result.
async fn fetch_reddit_json(url: &str, work_dir: &Path) -> Result<ArchiveResult> {
    // Convert URL to JSON endpoint
    let json_url = make_json_url(url);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(&json_url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (compatible; discourse-link-archiver/0.1)",
        )
        .send()
        .await
        .context("Failed to fetch Reddit JSON")?;

    if !response.status().is_success() {
        anyhow::bail!("Reddit JSON API returned status {}", response.status());
    }

    let body = response
        .text()
        .await
        .context("Failed to read response body")?;

    // Save raw JSON
    let json_path = work_dir.join("reddit_data.json");
    tokio::fs::write(&json_path, &body)
        .await
        .context("Failed to write JSON file")?;

    // Parse the response - Reddit returns an array for post pages
    let post_data = parse_reddit_json(&body)?;

    // Build text content from post data
    let text = build_post_text(&post_data);

    // Determine content type
    let content_type = if post_data.is_video.unwrap_or(false) {
        "video"
    } else if post_data.is_self.unwrap_or(false) {
        "thread"
    } else {
        "text"
    };

    // Build metadata JSON
    let metadata = serde_json::json!({
        "title": post_data.title,
        "author": post_data.author,
        "subreddit": post_data.subreddit,
        "score": post_data.score,
        "num_comments": post_data.num_comments,
        "created_utc": post_data.created_utc,
        "permalink": post_data.permalink,
        "url": post_data.url,
        "is_video": post_data.is_video,
        "is_self": post_data.is_self,
        "over_18": post_data.over_18,
    });

    // Determine NSFW status from API
    let is_nsfw = post_data.over_18;
    let nsfw_source = if is_nsfw == Some(true) {
        Some("api".to_string())
    } else {
        None
    };

    Ok(ArchiveResult {
        title: post_data.title,
        author: post_data.author,
        text: Some(text),
        content_type: content_type.to_string(),
        metadata_json: Some(metadata.to_string()),
        primary_file: Some("reddit_data.json".to_string()),
        is_nsfw,
        nsfw_source,
        ..Default::default()
    })
}

/// Convert a Reddit URL to its JSON API equivalent.
fn make_json_url(url: &str) -> String {
    // Remove query parameters and trailing slashes for clean JSON URL
    let base_url = url.split('?').next().unwrap_or(url).trim_end_matches('/');

    format!("{base_url}.json")
}

/// Parse Reddit JSON response to extract post data.
fn parse_reddit_json(body: &str) -> Result<PostData> {
    // Reddit post pages return an array of listings
    // First element is the post, second is comments
    let listings: Vec<RedditListing> = match serde_json::from_str(body) {
        Ok(l) => l,
        Err(_) => {
            // Try parsing as a single listing (for some subreddit pages)
            let listing: RedditListing =
                serde_json::from_str(body).context("Failed to parse Reddit JSON")?;
            vec![listing]
        }
    };

    let post = listings
        .first()
        .and_then(|l| l.data.children.first())
        .map(|c| c.data.clone())
        .context("No post data found in Reddit response")?;

    Ok(post)
}

/// Build readable text content from Reddit post data.
fn build_post_text(post: &PostData) -> String {
    let mut parts = Vec::new();

    if let Some(title) = &post.title {
        parts.push(format!("Title: {title}"));
    }

    if let Some(author) = &post.author {
        parts.push(format!("Author: u/{author}"));
    }

    if let Some(subreddit) = &post.subreddit {
        parts.push(format!("Subreddit: r/{subreddit}"));
    }

    if let Some(score) = post.score {
        parts.push(format!("Score: {score}"));
    }

    if let Some(comments) = post.num_comments {
        parts.push(format!("Comments: {comments}"));
    }

    parts.push(String::new()); // Empty line

    // Add selftext if present
    if let Some(text) = &post.selftext {
        if !text.is_empty() {
            parts.push(text.clone());
        }
    }

    // Add URL if it's a link post
    if let Some(url) = &post.url {
        if post.is_self != Some(true) && !url.contains("reddit.com") {
            parts.push(format!("\nLinked URL: {url}"));
        }
    }

    parts.join("\n")
}

impl Clone for PostData {
    fn clone(&self) -> Self {
        Self {
            title: self.title.clone(),
            author: self.author.clone(),
            selftext: self.selftext.clone(),
            selftext_html: self.selftext_html.clone(),
            score: self.score,
            num_comments: self.num_comments,
            created_utc: self.created_utc,
            subreddit: self.subreddit.clone(),
            permalink: self.permalink.clone(),
            url: self.url.clone(),
            is_video: self.is_video,
            is_self: self.is_self,
            thumbnail: self.thumbnail.clone(),
            over_18: self.over_18,
        }
    }
}

/// Check if a URL is a redd.it shortlink.
fn is_shortlink(url: &str) -> bool {
    SHORTLINK_PATTERN.is_match(url)
}

/// Check if a Reddit URL points to a known NSFW subreddit based on name patterns.
fn is_nsfw_subreddit(url: &str) -> bool {
    if let Some(caps) = SUBREDDIT_PATTERN.captures(url) {
        if let Some(subreddit) = caps.get(1) {
            let subreddit_lower = subreddit.as_str().to_lowercase();
            return NSFW_SUBREDDIT_PATTERNS
                .iter()
                .any(|pattern| subreddit_lower.contains(pattern));
        }
    }
    false
}

/// Resolve a redd.it short URL to full Reddit URL.
///
/// Sends a HEAD request and follows the redirect location header.
pub async fn resolve_short_url(short_url: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .head(short_url)
        .send()
        .await
        .context("Failed to resolve short URL")?;

    if let Some(location) = response.headers().get("location") {
        Ok(location.to_str().unwrap_or(short_url).to_string())
    } else {
        Ok(short_url.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle() {
        let handler = RedditHandler::new();

        assert!(handler.can_handle("https://www.reddit.com/r/rust/"));
        assert!(handler.can_handle("https://old.reddit.com/r/test"));
        assert!(handler.can_handle("https://redd.it/abc123"));
        assert!(handler.can_handle("https://i.redd.it/image.jpg"));
        assert!(handler.can_handle("https://v.redd.it/video123"));

        assert!(!handler.can_handle("https://example.com/"));
        assert!(!handler.can_handle("https://youtube.com/"));
    }

    #[test]
    fn test_normalize_url() {
        let handler = RedditHandler::new();

        assert!(handler
            .normalize_url("https://www.reddit.com/r/test")
            .contains("old.reddit.com"));
        assert!(handler
            .normalize_url("https://m.reddit.com/r/test")
            .contains("old.reddit.com"));
        assert!(handler
            .normalize_url("https://new.reddit.com/r/test")
            .contains("old.reddit.com"));
    }

    #[test]
    fn test_is_shortlink() {
        // Valid shortlinks
        assert!(is_shortlink("https://redd.it/abc123"));
        assert!(is_shortlink("http://redd.it/xyz789"));

        // Not shortlinks (media subdomains)
        assert!(!is_shortlink("https://i.redd.it/image.jpg"));
        assert!(!is_shortlink("https://v.redd.it/video123"));
        assert!(!is_shortlink("https://preview.redd.it/something"));

        // Not shortlinks (full Reddit URLs)
        assert!(!is_shortlink("https://www.reddit.com/r/rust"));
        assert!(!is_shortlink("https://old.reddit.com/r/test"));
    }

    #[test]
    fn test_is_nsfw_subreddit() {
        // NSFW subreddits
        assert!(is_nsfw_subreddit(
            "https://old.reddit.com/r/nsfw/comments/abc123"
        ));
        assert!(is_nsfw_subreddit("https://reddit.com/r/gonewild/"));
        assert!(is_nsfw_subreddit("https://www.reddit.com/r/NSFW_GIF/"));
        assert!(is_nsfw_subreddit("https://old.reddit.com/r/porn/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/hentai/"));
        assert!(is_nsfw_subreddit(
            "https://reddit.com/r/rule34/comments/xyz"
        ));

        // SFW subreddits
        assert!(!is_nsfw_subreddit("https://reddit.com/r/rust/"));
        assert!(!is_nsfw_subreddit("https://old.reddit.com/r/programming/"));
        assert!(!is_nsfw_subreddit("https://www.reddit.com/r/funny/"));
        assert!(!is_nsfw_subreddit("https://reddit.com/r/pics/"));

        // Edge cases
        assert!(!is_nsfw_subreddit("https://reddit.com/user/someone"));
        assert!(!is_nsfw_subreddit("https://i.redd.it/image.jpg"));
    }

    #[test]
    fn test_make_json_url() {
        assert_eq!(
            make_json_url("https://old.reddit.com/r/rust/comments/abc123/title"),
            "https://old.reddit.com/r/rust/comments/abc123/title.json"
        );
        assert_eq!(
            make_json_url("https://old.reddit.com/r/rust/comments/abc123/title/"),
            "https://old.reddit.com/r/rust/comments/abc123/title.json"
        );
        assert_eq!(
            make_json_url("https://old.reddit.com/r/rust/comments/abc123/title?sort=top"),
            "https://old.reddit.com/r/rust/comments/abc123/title.json"
        );
    }

    #[test]
    fn test_parse_reddit_json() {
        let json = r#"[
            {
                "data": {
                    "children": [
                        {
                            "data": {
                                "title": "Test Post Title",
                                "author": "testuser",
                                "selftext": "This is the post content",
                                "score": 100,
                                "num_comments": 50,
                                "subreddit": "rust",
                                "is_self": true,
                                "is_video": false,
                                "over_18": false
                            }
                        }
                    ]
                }
            }
        ]"#;

        let result = parse_reddit_json(json).expect("Should parse successfully");
        assert_eq!(result.title.as_deref(), Some("Test Post Title"));
        assert_eq!(result.author.as_deref(), Some("testuser"));
        assert_eq!(result.selftext.as_deref(), Some("This is the post content"));
        assert_eq!(result.score, Some(100));
        assert_eq!(result.num_comments, Some(50));
        assert_eq!(result.subreddit.as_deref(), Some("rust"));
        assert_eq!(result.is_self, Some(true));
        assert_eq!(result.is_video, Some(false));
        assert_eq!(result.over_18, Some(false));
    }

    #[test]
    fn test_build_post_text() {
        let post = PostData {
            title: Some("My Test Post".to_string()),
            author: Some("testuser".to_string()),
            selftext: Some("Hello, world!".to_string()),
            selftext_html: None,
            score: Some(42),
            num_comments: Some(10),
            created_utc: None,
            subreddit: Some("testing".to_string()),
            permalink: None,
            url: None,
            is_video: Some(false),
            is_self: Some(true),
            thumbnail: None,
            over_18: None,
        };

        let text = build_post_text(&post);
        assert!(text.contains("Title: My Test Post"));
        assert!(text.contains("Author: u/testuser"));
        assert!(text.contains("Subreddit: r/testing"));
        assert!(text.contains("Score: 42"));
        assert!(text.contains("Comments: 10"));
        assert!(text.contains("Hello, world!"));
    }

    #[test]
    fn test_build_post_text_with_link() {
        let post = PostData {
            title: Some("Link Post".to_string()),
            author: Some("linkposter".to_string()),
            selftext: None,
            selftext_html: None,
            score: Some(100),
            num_comments: Some(25),
            created_utc: None,
            subreddit: Some("news".to_string()),
            permalink: None,
            url: Some("https://example.com/article".to_string()),
            is_video: Some(false),
            is_self: Some(false),
            thumbnail: None,
            over_18: None,
        };

        let text = build_post_text(&post);
        assert!(text.contains("Title: Link Post"));
        assert!(text.contains("Linked URL: https://example.com/article"));
    }
}
