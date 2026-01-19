use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::ytdlp;

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
        cookies_file: Option<&Path>,
    ) -> Result<ArchiveResult> {
        ytdlp::download(url, work_dir, cookies_file).await
    }
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
}
