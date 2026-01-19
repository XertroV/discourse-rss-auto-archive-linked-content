use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::{ytdlp, CookieOptions};

static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        Regex::new(r"^https?://(www\.)?youtube\.com/watch").unwrap(),
        Regex::new(r"^https?://(www\.)?youtube\.com/shorts/").unwrap(),
        Regex::new(r"^https?://(www\.)?youtube\.com/live/").unwrap(),
        Regex::new(r"^https?://(www\.)?youtube\.com/embed/").unwrap(),
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
    ) -> Result<ArchiveResult> {
        ytdlp::download(url, work_dir, cookies).await
    }
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
}
