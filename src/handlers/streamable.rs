use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use tracing::debug;

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::{ytdlp, CookieOptions};

static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![Regex::new(r"^https?://(www\.)?streamable\.com/[a-zA-Z0-9]+").unwrap()]
});

pub struct StreamableHandler;

impl StreamableHandler {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for StreamableHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SiteHandler for StreamableHandler {
    fn site_id(&self) -> &'static str {
        "streamable"
    }

    fn url_patterns(&self) -> &[Regex] {
        &PATTERNS
    }

    fn priority(&self) -> i32 {
        100
    }

    async fn archive(
        &self,
        url: &str,
        work_dir: &Path,
        cookies: &CookieOptions<'_>,
        config: &crate::config::Config,
    ) -> Result<ArchiveResult> {
        let mut result = ytdlp::download(url, work_dir, cookies, config, None, None).await?;

        // Extract video_id for deduplication
        if let Some(video_id) = extract_video_id(url) {
            debug!(video_id = %video_id, "Extracted Streamable video ID");
            result.video_id = Some(video_id);
        }

        Ok(result)
    }
}

/// Extract video ID from Streamable URL.
///
/// Streamable URLs have format: `https://streamable.com/{video_id}`
pub fn extract_video_id(url: &str) -> Option<String> {
    // Parse the URL and extract the path
    url::Url::parse(url).ok().and_then(|parsed| {
        let path = parsed.path().trim_start_matches('/');
        // Video ID is typically alphanumeric, 5-8 chars
        if !path.is_empty() && path.chars().all(char::is_alphanumeric) {
            Some(path.to_string())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle() {
        let handler = StreamableHandler::new();

        assert!(handler.can_handle("https://streamable.com/abc123"));
        assert!(handler.can_handle("https://www.streamable.com/abc123"));
        assert!(handler.can_handle("http://streamable.com/xyz789"));

        assert!(!handler.can_handle("https://example.com/"));
        assert!(!handler.can_handle("https://youtube.com/watch?v=abc"));
        assert!(!handler.can_handle("https://streamable.com/")); // No video ID
    }

    #[test]
    fn test_site_id() {
        let handler = StreamableHandler::new();
        assert_eq!(handler.site_id(), "streamable");
    }

    #[test]
    fn test_priority() {
        let handler = StreamableHandler::new();
        assert_eq!(handler.priority(), 100);
    }

    #[test]
    fn test_extract_video_id() {
        assert_eq!(
            extract_video_id("https://streamable.com/abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(
            extract_video_id("https://www.streamable.com/xyz789"),
            Some("xyz789".to_string())
        );
        assert_eq!(
            extract_video_id("http://streamable.com/A1B2c3"),
            Some("A1B2c3".to_string())
        );
        // Invalid URLs
        assert_eq!(extract_video_id("https://streamable.com/"), None);
        assert_eq!(extract_video_id("invalid-url"), None);
    }
}
