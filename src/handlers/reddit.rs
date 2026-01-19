use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use tracing::debug;

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::ytdlp;

static PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
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

static SHORTLINK_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^https?://redd\.it/[a-zA-Z0-9]+$").unwrap());

pub struct RedditHandler;

impl RedditHandler {
    #[must_use]
    pub fn new() -> Self {
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

        // Use yt-dlp for video content
        let ytdlp_result = ytdlp::download(&normalized_url, work_dir, cookies_file).await;

        match ytdlp_result {
            Ok(result) => Ok(result),
            Err(e) => {
                // If yt-dlp fails, try HTTP fetch for metadata
                debug!("yt-dlp failed for Reddit URL, falling back to HTTP: {e}");

                // For now, return a minimal result
                // In a full implementation, we'd fetch the JSON API
                Ok(ArchiveResult {
                    content_type: "thread".to_string(),
                    ..Default::default()
                })
            }
        }
    }
}

/// Check if a URL is a redd.it shortlink.
fn is_shortlink(url: &str) -> bool {
    SHORTLINK_PATTERN.is_match(url)
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
}
