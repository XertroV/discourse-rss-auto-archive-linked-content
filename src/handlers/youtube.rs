use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use tracing::debug;

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::{playlist as playlist_archiver, ytdlp, CookieOptions};

static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        Regex::new(r"^https?://(www\.)?youtube\.com/watch").unwrap(),
        Regex::new(r"^https?://(www\.)?youtube\.com/shorts/").unwrap(),
        Regex::new(r"^https?://(www\.)?youtube\.com/live/").unwrap(),
        Regex::new(r"^https?://(www\.)?youtube\.com/embed/").unwrap(),
        Regex::new(r"^https?://(www\.)?youtube\.com/playlist").unwrap(),
        Regex::new(r"^https?://youtu\.be/").unwrap(),
        Regex::new(r"^https?://m\.youtube\.com/").unwrap(),
    ]
});

pub struct YouTubeHandler;

impl YouTubeHandler {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for YouTubeHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SiteHandler for YouTubeHandler {
    fn site_id(&self) -> &'static str {
        "youtube"
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
        // Check if this is a playlist URL
        if is_playlist_url(url) {
            if let Some(playlist_id) = extract_playlist_id(url) {
                debug!(playlist_id = %playlist_id, "Extracted YouTube playlist ID, delegating to playlist archiver");
                return playlist_archiver::archive_playlist(
                    url,
                    work_dir,
                    cookies,
                    config,
                    &playlist_id,
                )
                .await;
            }
        }

        // Regular video handling
        let mut result = ytdlp::download(url, work_dir, cookies, config).await?;

        // Store video_id in metadata for predictable S3 path
        if let Some(video_id) = extract_video_id(url) {
            debug!(video_id = %video_id, "Extracted YouTube video ID");
            result.video_id = Some(video_id);
        }

        Ok(result)
    }
}

/// Check if a URL is a YouTube playlist URL.
pub fn is_playlist_url(url: &str) -> bool {
    url.contains("youtube.com/playlist")
}

/// Extract playlist ID from YouTube URL.
///
/// Playlist URLs have format: `youtube.com/playlist?list=PLAYLIST_ID`
pub fn extract_playlist_id(url: &str) -> Option<String> {
    if let Some(query_string) = url.split('?').nth(1) {
        for param in query_string.split('&') {
            if let Some(playlist_id) = param.strip_prefix("list=") {
                let playlist_id = playlist_id.split('&').next().unwrap_or(playlist_id);
                if !playlist_id.is_empty() {
                    return Some(playlist_id.to_string());
                }
            }
        }
    }
    None
}

/// Extract video ID from YouTube URL.
///
/// Supports various YouTube URL formats:
/// - `youtube.com/watch?v=VIDEO_ID`
/// - `youtu.be/VIDEO_ID`
/// - `youtube.com/shorts/VIDEO_ID`
/// - `youtube.com/live/VIDEO_ID`
/// - `youtube.com/embed/VIDEO_ID`
pub fn extract_video_id(url: &str) -> Option<String> {
    // Pattern for standard watch URLs: youtube.com/watch?v=VIDEO_ID
    if url.contains("watch?") || url.contains("watch/?") {
        if let Some(v_param) = url.split('?').nth(1) {
            for param in v_param.split('&') {
                if let Some(video_id) = param.strip_prefix("v=") {
                    let video_id = video_id.split('&').next().unwrap_or(video_id);
                    return Some(video_id.to_string());
                }
            }
        }
        return None;
    }

    // Pattern for youtu.be/VIDEO_ID
    if url.contains("youtu.be/") {
        if let Some(path) = url.split("youtu.be/").nth(1) {
            let video_id = path.split('?').next().unwrap_or(path);
            let video_id = video_id.split('/').next().unwrap_or(video_id);
            if !video_id.is_empty() {
                return Some(video_id.to_string());
            }
        }
        return None;
    }

    // Pattern for shorts/VIDEO_ID, live/VIDEO_ID, embed/VIDEO_ID
    for prefix in ["shorts/", "live/", "embed/"] {
        if url.contains(prefix) {
            if let Some(path) = url.split(prefix).nth(1) {
                let video_id = path.split('?').next().unwrap_or(path);
                let video_id = video_id.split('/').next().unwrap_or(video_id);
                if !video_id.is_empty() {
                    return Some(video_id.to_string());
                }
            }
            return None;
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle() {
        let handler = YouTubeHandler::new();

        assert!(handler.can_handle("https://www.youtube.com/watch?v=abc123"));
        assert!(handler.can_handle("https://youtube.com/watch?v=abc123"));
        assert!(handler.can_handle("https://youtu.be/abc123"));
        assert!(handler.can_handle("https://www.youtube.com/shorts/abc123"));
        assert!(handler.can_handle("https://m.youtube.com/watch?v=abc123"));

        assert!(!handler.can_handle("https://example.com/"));
        assert!(!handler.can_handle("https://reddit.com/"));
    }

    #[test]
    fn test_extract_video_id_watch() {
        // Standard watch URLs
        assert_eq!(
            extract_video_id("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(
            extract_video_id("https://youtube.com/watch?v=abc123"),
            Some("abc123".to_string())
        );
        // With additional parameters
        assert_eq!(
            extract_video_id("https://www.youtube.com/watch?v=xyz789&t=30s"),
            Some("xyz789".to_string())
        );
        assert_eq!(
            extract_video_id("https://www.youtube.com/watch?list=PLxyz&v=abc123"),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn test_extract_video_id_youtu_be() {
        assert_eq!(
            extract_video_id("https://youtu.be/dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(
            extract_video_id("https://youtu.be/abc123?t=30"),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn test_extract_video_id_shorts() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/shorts/abc123"),
            Some("abc123".to_string())
        );
        assert_eq!(
            extract_video_id("https://youtube.com/shorts/xyz789?feature=share"),
            Some("xyz789".to_string())
        );
    }

    #[test]
    fn test_extract_video_id_live() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/live/abc123"),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn test_extract_video_id_embed() {
        assert_eq!(
            extract_video_id("https://www.youtube.com/embed/abc123"),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn test_extract_video_id_invalid() {
        assert_eq!(extract_video_id("https://example.com/video"), None);
        assert_eq!(extract_video_id("https://youtube.com/"), None);
        assert_eq!(
            extract_video_id("https://www.youtube.com/channel/xyz"),
            None
        );
    }

    #[test]
    fn test_is_playlist_url() {
        assert!(is_playlist_url(
            "https://www.youtube.com/playlist?list=PLxyz"
        ));
        assert!(is_playlist_url(
            "https://youtube.com/playlist?list=PLabc123"
        ));
        assert!(!is_playlist_url("https://www.youtube.com/watch?v=abc123"));
        assert!(!is_playlist_url("https://youtu.be/abc123"));
    }

    #[test]
    fn test_extract_playlist_id() {
        assert_eq!(
            extract_playlist_id("https://www.youtube.com/playlist?list=PLxyz"),
            Some("PLxyz".to_string())
        );
        assert_eq!(
            extract_playlist_id("https://youtube.com/playlist?list=PLabc123&index=1"),
            Some("PLabc123".to_string())
        );
        assert_eq!(
            extract_playlist_id("https://www.youtube.com/playlist?index=1&list=PLtest"),
            Some("PLtest".to_string())
        );
        assert_eq!(extract_playlist_id("https://youtube.com/playlist"), None);
        assert_eq!(
            extract_playlist_id("https://www.youtube.com/watch?v=abc123"),
            None
        );
    }
}
