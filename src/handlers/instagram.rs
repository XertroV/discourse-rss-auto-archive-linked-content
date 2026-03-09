use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use tracing::debug;

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::{ytdlp, CookieOptions};

static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        // Standard Instagram URLs
        Regex::new(r"^https?://(www\.)?instagram\.com/p/[A-Za-z0-9_-]+").unwrap(),
        Regex::new(r"^https?://(www\.)?instagram\.com/reel/[A-Za-z0-9_-]+").unwrap(),
        Regex::new(r"^https?://(www\.)?instagram\.com/reels/[A-Za-z0-9_-]+").unwrap(),
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
            .replace("://www.instagram.com/", "://instagram.com/")
            // Normalize /reels/ (plural share URL) to /reel/ (canonical)
            .replace("/reels/", "/reel/");

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
        config: &crate::config::Config,
    ) -> Result<ArchiveResult> {
        let mut result = ytdlp::download(url, work_dir, cookies, config, None, None, false).await?;

        if let Some(shortcode) = extract_shortcode(url) {
            debug!(shortcode = %shortcode, "Extracted Instagram shortcode");
            result.video_id = Some(shortcode);
        }

        Ok(result)
    }
}

/// Extract shortcode from Instagram URL.
///
/// Supports URL formats:
/// - `instagram.com/p/SHORTCODE`
/// - `instagram.com/reel/SHORTCODE`
/// - `instagram.com/reels/SHORTCODE`
/// - `instagram.com/tv/SHORTCODE`
fn extract_shortcode(url: &str) -> Option<String> {
    for segment in &["/p/", "/reel/", "/reels/", "/tv/"] {
        if let Some(pos) = url.find(segment) {
            let after = &url[pos + segment.len()..];
            let shortcode = after
                .split('?')
                .next()
                .unwrap_or(after)
                .split('#')
                .next()
                .unwrap_or(after)
                .split('/')
                .next()
                .unwrap_or(after);

            if !shortcode.is_empty() {
                return Some(shortcode.to_string());
            }
        }
    }
    None
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

        // Reel URLs (singular)
        assert!(handler.can_handle("https://instagram.com/reel/ABC123"));
        assert!(handler.can_handle("https://www.instagram.com/reel/ABC123"));

        // Reels URLs (plural - share URLs)
        assert!(handler.can_handle("https://instagram.com/reels/ABC123"));
        assert!(handler.can_handle("https://www.instagram.com/reels/DRzuPzVCcmB"));

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

        // Normalize /reels/ to /reel/
        assert_eq!(
            handler.normalize_url("https://www.instagram.com/reels/DRzuPzVCcmB/"),
            "https://instagram.com/reel/DRzuPzVCcmB"
        );

        assert_eq!(
            handler.normalize_url("https://instagram.com/reels/ABC123?igsh=foo"),
            "https://instagram.com/reel/ABC123"
        );
    }

    #[test]
    fn test_extract_shortcode() {
        // Post URLs
        assert_eq!(
            extract_shortcode("https://instagram.com/p/ABC123"),
            Some("ABC123".to_string())
        );

        // Reel URLs (singular)
        assert_eq!(
            extract_shortcode("https://instagram.com/reel/XYZ789"),
            Some("XYZ789".to_string())
        );

        // Reels URLs (plural)
        assert_eq!(
            extract_shortcode("https://instagram.com/reels/DRzuPzVCcmB"),
            Some("DRzuPzVCcmB".to_string())
        );

        // TV URLs
        assert_eq!(
            extract_shortcode("https://instagram.com/tv/DEF456"),
            Some("DEF456".to_string())
        );

        // With query parameters
        assert_eq!(
            extract_shortcode("https://instagram.com/p/ABC123?utm_source=ig"),
            Some("ABC123".to_string())
        );

        // With trailing slash
        assert_eq!(
            extract_shortcode("https://instagram.com/reel/ABC123/"),
            Some("ABC123".to_string())
        );

        // Profile URL (no shortcode segment)
        assert_eq!(extract_shortcode("https://instagram.com/username"), None);

        // Story URL (no shortcode segment)
        assert_eq!(
            extract_shortcode("https://instagram.com/stories/user/123"),
            None
        );
    }
}
