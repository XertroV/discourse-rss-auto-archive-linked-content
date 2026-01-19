use std::path::Path;

use anyhow::{Context, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;

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
        // Normalize URL first
        let normalized_url = self.normalize_url(url);

        // Use yt-dlp for video content
        let ytdlp_result = ytdlp::download(&normalized_url, work_dir, cookies_file).await;

        match ytdlp_result {
            Ok(result) => Ok(result),
            Err(e) => {
                // If yt-dlp fails, try HTTP fetch for metadata
                tracing::debug!("yt-dlp failed for Reddit URL, falling back to HTTP: {e}");

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

/// Resolve a redd.it short URL to full Reddit URL.
#[allow(dead_code)]
pub async fn resolve_short_url(short_url: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
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
}
