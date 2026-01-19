use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::{gallerydl, CookieOptions};

static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        // Standard Instagram URLs
        Regex::new(r"^https?://(www\.)?instagram\.com/p/[A-Za-z0-9_-]+").unwrap(),
        Regex::new(r"^https?://(www\.)?instagram\.com/reel/[A-Za-z0-9_-]+").unwrap(),
        Regex::new(r"^https?://(www\.)?instagram\.com/stories/[^/]+/[0-9]+").unwrap(),
        Regex::new(r"^https?://(www\.)?instagram\.com/[A-Za-z0-9_.]+/?$").unwrap(),
        // Instagram TV
        Regex::new(r"^https?://(www\.)?instagram\.com/tv/[A-Za-z0-9_-]+").unwrap(),
        // Instagram share URLs
        Regex::new(r"^https?://instagr\.am/p/[A-Za-z0-9_-]+").unwrap(),
    ]
});

pub struct InstagramHandler;

impl InstagramHandler {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for InstagramHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SiteHandler for InstagramHandler {
    fn site_id(&self) -> &'static str {
        "instagram"
    }

    fn url_patterns(&self) -> &[Regex] {
        &PATTERNS
    }

    fn priority(&self) -> i32 {
        100
    }

    fn normalize_url(&self, url: &str) -> String {
        // Normalize instagr.am to instagram.com
        let normalized = url
            .replace("://instagr.am/", "://instagram.com/")
            .replace("://www.instagram.com/", "://instagram.com/");

        // Remove query parameters and trailing slashes
        let without_query = if let Some(pos) = normalized.find('?') {
            &normalized[..pos]
        } else {
            &normalized
        };

        without_query.trim_end_matches('/').to_string()
    }

    async fn archive(
        &self,
        url: &str,
        work_dir: &Path,
        cookies: &CookieOptions<'_>,
        _config: &crate::config::Config,
    ) -> Result<ArchiveResult> {
        gallerydl::download(url, work_dir, cookies).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle() {
        let handler = InstagramHandler::new();

        // Post URLs
        assert!(handler.can_handle("https://instagram.com/p/ABC123"));
        assert!(handler.can_handle("https://www.instagram.com/p/ABC123"));
        assert!(handler.can_handle("https://instagram.com/p/ABC123_-xyz"));

        // Reel URLs
        assert!(handler.can_handle("https://instagram.com/reel/ABC123"));
        assert!(handler.can_handle("https://www.instagram.com/reel/ABC123"));

        // Story URLs
        assert!(handler.can_handle("https://instagram.com/stories/username/1234567890"));

        // Profile URLs
        assert!(handler.can_handle("https://instagram.com/username"));
        assert!(handler.can_handle("https://instagram.com/user_name.123"));

        // IGTV URLs
        assert!(handler.can_handle("https://instagram.com/tv/ABC123"));

        // Short URLs
        assert!(handler.can_handle("https://instagr.am/p/ABC123"));

        // Non-matching URLs
        assert!(!handler.can_handle("https://example.com/"));
        assert!(!handler.can_handle("https://twitter.com/user"));
        assert!(!handler.can_handle("https://facebook.com/user"));
    }

    #[test]
    fn test_normalize_url() {
        let handler = InstagramHandler::new();

        // instagr.am to instagram.com
        assert_eq!(
            handler.normalize_url("https://instagr.am/p/ABC123"),
            "https://instagram.com/p/ABC123"
        );

        // Remove www
        assert_eq!(
            handler.normalize_url("https://www.instagram.com/p/ABC123"),
            "https://instagram.com/p/ABC123"
        );

        // Remove trailing slash
        assert_eq!(
            handler.normalize_url("https://instagram.com/username/"),
            "https://instagram.com/username"
        );

        // Remove query parameters
        assert_eq!(
            handler.normalize_url("https://instagram.com/p/ABC123?utm_source=ig_web"),
            "https://instagram.com/p/ABC123"
        );
    }
}
