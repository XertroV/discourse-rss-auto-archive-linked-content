use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::{gallerydl, CookieOptions};

static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        // Direct image links
        Regex::new(r"^https?://i\.imgur\.com/[A-Za-z0-9]+\.[a-z]+").unwrap(),
        // Album URLs
        Regex::new(r"^https?://(www\.)?imgur\.com/a/[A-Za-z0-9]+").unwrap(),
        // Gallery URLs
        Regex::new(r"^https?://(www\.)?imgur\.com/gallery/[A-Za-z0-9]+").unwrap(),
        // Single image page URLs
        Regex::new(r"^https?://(www\.)?imgur\.com/[A-Za-z0-9]+$").unwrap(),
        // Video URLs
        Regex::new(r"^https?://i\.imgur\.com/[A-Za-z0-9]+\.gifv").unwrap(),
        Regex::new(r"^https?://i\.imgur\.com/[A-Za-z0-9]+\.mp4").unwrap(),
    ]
});

pub struct ImgurHandler;

impl ImgurHandler {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for ImgurHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SiteHandler for ImgurHandler {
    fn site_id(&self) -> &'static str {
        "imgur"
    }

    fn url_patterns(&self) -> &[Regex] {
        &PATTERNS
    }

    fn priority(&self) -> i32 {
        100
    }

    fn normalize_url(&self, url: &str) -> String {
        let normalized = url.replace("://www.imgur.com/", "://imgur.com/");

        // Convert .gifv to page URL for better archiving
        let normalized = if normalized.contains("i.imgur.com") && normalized.ends_with(".gifv") {
            let id = normalized
                .trim_start_matches("https://i.imgur.com/")
                .trim_start_matches("http://i.imgur.com/")
                .trim_end_matches(".gifv");
            format!("https://imgur.com/{id}")
        } else {
            normalized
        };

        // Remove query parameters
        if let Some(pos) = normalized.find('?') {
            normalized[..pos].to_string()
        } else {
            normalized
        }
    }

    async fn archive(
        &self,
        url: &str,
        work_dir: &Path,
        cookies: &CookieOptions<'_>,
        config: &crate::config::Config,
    ) -> Result<ArchiveResult> {
        gallerydl::download(url, work_dir, cookies, config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle() {
        let handler = ImgurHandler::new();

        // Direct image links
        assert!(handler.can_handle("https://i.imgur.com/ABC123.jpg"));
        assert!(handler.can_handle("https://i.imgur.com/ABC123.png"));
        assert!(handler.can_handle("https://i.imgur.com/ABC123.gif"));

        // Album URLs
        assert!(handler.can_handle("https://imgur.com/a/ABC123"));
        assert!(handler.can_handle("https://www.imgur.com/a/ABC123"));

        // Gallery URLs
        assert!(handler.can_handle("https://imgur.com/gallery/ABC123"));
        assert!(handler.can_handle("https://www.imgur.com/gallery/ABC123"));

        // Single image page URLs
        assert!(handler.can_handle("https://imgur.com/ABC123"));
        assert!(handler.can_handle("https://www.imgur.com/ABC123"));

        // Video URLs
        assert!(handler.can_handle("https://i.imgur.com/ABC123.gifv"));
        assert!(handler.can_handle("https://i.imgur.com/ABC123.mp4"));

        // Non-matching URLs
        assert!(!handler.can_handle("https://example.com/"));
        assert!(!handler.can_handle("https://instagram.com/p/ABC123"));
    }

    #[test]
    fn test_normalize_url() {
        let handler = ImgurHandler::new();

        // Remove www
        assert_eq!(
            handler.normalize_url("https://www.imgur.com/a/ABC123"),
            "https://imgur.com/a/ABC123"
        );

        // Convert gifv to page URL
        assert_eq!(
            handler.normalize_url("https://i.imgur.com/ABC123.gifv"),
            "https://imgur.com/ABC123"
        );

        // Remove query parameters
        assert_eq!(
            handler.normalize_url("https://imgur.com/a/ABC123?ref=test"),
            "https://imgur.com/a/ABC123"
        );
    }
}
