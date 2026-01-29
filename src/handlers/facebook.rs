use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use tracing::debug;

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::{ytdlp, CookieOptions};

static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        // Facebook reel URLs
        Regex::new(r"^https?://(www\.)?facebook\.com/reel/").unwrap(),
        Regex::new(r"^https?://m\.facebook\.com/reel/").unwrap(),
    ]
});

pub struct FacebookHandler;

impl FacebookHandler {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for FacebookHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SiteHandler for FacebookHandler {
    fn site_id(&self) -> &'static str {
        "facebook"
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
        let mut result = ytdlp::download(url, work_dir, cookies, config, None, None, false).await?;

        // Store reel_id in metadata for predictable S3 path
        if let Some(reel_id) = extract_reel_id(url) {
            debug!(reel_id = %reel_id, "Extracted Facebook reel ID");
            result.video_id = Some(reel_id);
        }

        // Facebook reels don't have proper titles - yt-dlp generates a title from
        // view/reaction counts (e.g., "3.9M views · 135K reactions | Author on Reels").
        // If there's a description (the creator's caption), use that as the title instead.
        if let Some(ref text) = result.text {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                debug!(
                    original_title = ?result.title,
                    description = %trimmed,
                    "Using description as title for Facebook reel"
                );
                result.title = Some(trimmed.to_string());
            }
        }

        Ok(result)
    }
}

/// Extract reel ID from Facebook reel URL.
///
/// Supports URL formats:
/// - `facebook.com/reel/REEL_ID`
/// - `www.facebook.com/reel/REEL_ID`
/// - `m.facebook.com/reel/REEL_ID`
pub fn extract_reel_id(url: &str) -> Option<String> {
    // Find the /reel/ segment and extract the ID after it
    if let Some(reel_pos) = url.find("/reel/") {
        let after_reel = &url[reel_pos + 6..]; // Skip "/reel/"
                                               // ID ends at query string, fragment, or end of string
        let reel_id = after_reel
            .split('?')
            .next()
            .unwrap_or(after_reel)
            .split('#')
            .next()
            .unwrap_or(after_reel)
            .split('/')
            .next()
            .unwrap_or(after_reel);

        if !reel_id.is_empty() {
            return Some(reel_id.to_string());
        }
    }
    None
}

/// Normalize Facebook URL by removing www prefix and query parameters.
#[allow(dead_code)]
pub fn normalize_url(url: &str) -> Option<String> {
    let reel_id = extract_reel_id(url)?;
    Some(format!("https://facebook.com/reel/{}", reel_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle() {
        let handler = FacebookHandler::new();

        // Reel URLs
        assert!(handler.can_handle("https://www.facebook.com/reel/1953912198789892"));
        assert!(handler.can_handle("https://facebook.com/reel/1953912198789892"));
        assert!(handler.can_handle("https://m.facebook.com/reel/1953912198789892"));

        // With query parameters
        assert!(handler.can_handle("https://www.facebook.com/reel/123456?s=something"));

        // Non-Facebook URLs
        assert!(!handler.can_handle("https://example.com/"));
        assert!(!handler.can_handle("https://youtube.com/watch?v=abc"));

        // Other Facebook URLs (not yet supported)
        assert!(!handler.can_handle("https://www.facebook.com/videos/123456"));
        assert!(!handler.can_handle("https://www.facebook.com/watch?v=123456"));
        assert!(!handler.can_handle("https://www.facebook.com/someuser/posts/123"));
    }

    #[test]
    fn test_extract_reel_id() {
        // Standard reel URLs
        assert_eq!(
            extract_reel_id("https://www.facebook.com/reel/1953912198789892"),
            Some("1953912198789892".to_string())
        );
        assert_eq!(
            extract_reel_id("https://facebook.com/reel/123456"),
            Some("123456".to_string())
        );
        assert_eq!(
            extract_reel_id("https://m.facebook.com/reel/789012"),
            Some("789012".to_string())
        );

        // With query parameters
        assert_eq!(
            extract_reel_id("https://www.facebook.com/reel/123456?s=yWAINhsB4hEg2isc"),
            Some("123456".to_string())
        );

        // With fragment
        assert_eq!(
            extract_reel_id("https://www.facebook.com/reel/123456#section"),
            Some("123456".to_string())
        );

        // With trailing slash
        assert_eq!(
            extract_reel_id("https://www.facebook.com/reel/123456/"),
            Some("123456".to_string())
        );

        // Invalid URLs (no /reel/ path)
        assert_eq!(extract_reel_id("https://facebook.com/"), None);
        assert_eq!(extract_reel_id("https://facebook.com/videos/123"), None);
        assert_eq!(extract_reel_id("https://facebook.com/watch?v=123"), None);
    }

    #[test]
    fn test_normalize_url() {
        // Removes www prefix
        assert_eq!(
            normalize_url("https://www.facebook.com/reel/123456"),
            Some("https://facebook.com/reel/123456".to_string())
        );

        // Removes query parameters
        assert_eq!(
            normalize_url("https://facebook.com/reel/123456?s=abc"),
            Some("https://facebook.com/reel/123456".to_string())
        );

        // Normalizes mobile URLs
        assert_eq!(
            normalize_url("https://m.facebook.com/reel/123456"),
            Some("https://facebook.com/reel/123456".to_string())
        );

        // Invalid URLs return None
        assert_eq!(normalize_url("https://example.com/"), None);
    }

    #[test]
    fn test_site_id() {
        let handler = FacebookHandler::new();
        assert_eq!(handler.site_id(), "facebook");
    }

    #[test]
    fn test_priority() {
        let handler = FacebookHandler::new();
        assert_eq!(handler.priority(), 100);
    }

    #[test]
    fn test_title_from_description() {
        use super::super::traits::ArchiveResult;

        // Simulate a Facebook reel result where title is auto-generated
        // and description contains the creator's caption
        let mut result = ArchiveResult {
            title: Some("3.9M views · 135K reactions | Author Jason K Pargin on Reels".to_string()),
            text: Some("This is the actual caption the creator wrote for the reel".to_string()),
            ..Default::default()
        };

        // Apply the same logic as in archive()
        if let Some(ref text) = result.text {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                result.title = Some(trimmed.to_string());
            }
        }

        assert_eq!(
            result.title,
            Some("This is the actual caption the creator wrote for the reel".to_string())
        );
    }

    #[test]
    fn test_title_preserved_when_no_description() {
        use super::super::traits::ArchiveResult;

        // Simulate a Facebook reel with no description (empty caption)
        let mut result = ArchiveResult {
            title: Some("3.9M views · 135K reactions | Author Jason K Pargin on Reels".to_string()),
            text: Some("".to_string()),
            ..Default::default()
        };

        // Apply the same logic as in archive()
        if let Some(ref text) = result.text {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                result.title = Some(trimmed.to_string());
            }
        }

        // Title should remain unchanged when description is empty
        assert_eq!(
            result.title,
            Some("3.9M views · 135K reactions | Author Jason K Pargin on Reels".to_string())
        );
    }

    #[test]
    fn test_title_preserved_when_description_is_whitespace() {
        use super::super::traits::ArchiveResult;

        // Simulate a Facebook reel where description is just whitespace
        let mut result = ArchiveResult {
            title: Some("3.9M views · 135K reactions | Author Jason K Pargin on Reels".to_string()),
            text: Some("   \n\t  ".to_string()),
            ..Default::default()
        };

        // Apply the same logic as in archive()
        if let Some(ref text) = result.text {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                result.title = Some(trimmed.to_string());
            }
        }

        // Title should remain unchanged when description is only whitespace
        assert_eq!(
            result.title,
            Some("3.9M views · 135K reactions | Author Jason K Pargin on Reels".to_string())
        );
    }
}
