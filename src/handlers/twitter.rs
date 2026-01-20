use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use tracing::debug;

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::{ytdlp, CookieOptions};

static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        Regex::new(r"^https?://(www\.)?twitter\.com/").unwrap(),
        Regex::new(r"^https?://(www\.)?x\.com/").unwrap(),
        Regex::new(r"^https?://mobile\.twitter\.com/").unwrap(),
        Regex::new(r"^https?://mobile\.x\.com/").unwrap(),
    ]
});

pub struct TwitterHandler;

impl TwitterHandler {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for TwitterHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SiteHandler for TwitterHandler {
    fn site_id(&self) -> &'static str {
        "twitter"
    }

    fn url_patterns(&self) -> &[Regex] {
        &PATTERNS
    }

    fn priority(&self) -> i32 {
        100
    }

    fn normalize_url(&self, url: &str) -> String {
        // Normalize x.com to twitter.com for consistency
        let normalized = url
            .replace("://x.com/", "://twitter.com/")
            .replace("://www.x.com/", "://twitter.com/")
            .replace("://mobile.x.com/", "://twitter.com/")
            .replace("://mobile.twitter.com/", "://twitter.com/")
            .replace("://www.twitter.com/", "://twitter.com/");

        super::normalize::normalize_url(&normalized)
    }

    async fn archive(
        &self,
        url: &str,
        work_dir: &Path,
        cookies: &CookieOptions<'_>,
        config: &crate::config::Config,
    ) -> Result<ArchiveResult> {
        let mut result = ytdlp::download(url, work_dir, cookies, config).await?;

        // Extract video_id (tweet ID) for deduplication
        if result.content_type == "video" {
            if let Some(tweet_id) = extract_tweet_id(url) {
                debug!(tweet_id = %tweet_id, "Extracted Twitter tweet ID for video");
                result.video_id = Some(tweet_id);
            }
        }

        Ok(result)
    }
}

/// Extract tweet ID from Twitter/X URL.
///
/// Twitter URLs have format: `https://twitter.com/{user}/status/{tweet_id}`
pub fn extract_tweet_id(url: &str) -> Option<String> {
    // Look for /status/{id} pattern
    if let Some(idx) = url.find("/status/") {
        let rest = &url[idx + 8..]; // Skip "/status/"
                                    // Take digits until non-digit or end
        let tweet_id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !tweet_id.is_empty() {
            return Some(tweet_id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle() {
        let handler = TwitterHandler::new();

        assert!(handler.can_handle("https://twitter.com/user/status/123"));
        assert!(handler.can_handle("https://www.twitter.com/user/status/123"));
        assert!(handler.can_handle("https://x.com/user/status/123"));
        assert!(handler.can_handle("https://www.x.com/user/status/123"));
        assert!(handler.can_handle("https://mobile.twitter.com/user/status/123"));

        assert!(!handler.can_handle("https://example.com/"));
        assert!(!handler.can_handle("https://youtube.com/"));
    }

    #[test]
    fn test_normalize_url() {
        let handler = TwitterHandler::new();

        assert!(handler
            .normalize_url("https://x.com/user/status/123")
            .contains("twitter.com"));
        assert!(handler
            .normalize_url("https://www.x.com/user/status/123")
            .contains("twitter.com"));
    }

    #[test]
    fn test_extract_tweet_id() {
        assert_eq!(
            extract_tweet_id("https://twitter.com/user/status/1234567890"),
            Some("1234567890".to_string())
        );
        assert_eq!(
            extract_tweet_id("https://x.com/user/status/9876543210"),
            Some("9876543210".to_string())
        );
        assert_eq!(
            extract_tweet_id("https://twitter.com/user/status/123?s=20"),
            Some("123".to_string())
        );
        // No tweet ID
        assert_eq!(extract_tweet_id("https://twitter.com/user"), None);
        assert_eq!(extract_tweet_id("https://twitter.com/"), None);
    }
}
