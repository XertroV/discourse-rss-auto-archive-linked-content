use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::ytdlp;

static PATTERNS: Lazy<Vec<Regex>> =
    Lazy::new(|| vec![Regex::new(r"^https?://(www\.)?streamable\.com/[a-zA-Z0-9]+").unwrap()]);

pub struct StreamableHandler;

impl StreamableHandler {
    #[must_use]
    pub fn new() -> Self {
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
}
