use std::path::Path;

use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::{ytdlp, CookieOptions};
use crate::constants::ARCHIVAL_USER_AGENT;

static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        Regex::new(r"^https?://(www\.)?tiktok\.com/").unwrap(),
        Regex::new(r"^https?://vm\.tiktok\.com/").unwrap(),
        Regex::new(r"^https?://m\.tiktok\.com/").unwrap(),
    ]
});

pub struct TikTokHandler;

impl TikTokHandler {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for TikTokHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SiteHandler for TikTokHandler {
    fn site_id(&self) -> &'static str {
        "tiktok"
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
        // Resolve short URLs first
        let resolved_url = if url.contains("vm.tiktok.com") {
            resolve_short_url(url)
                .await
                .unwrap_or_else(|_| url.to_string())
        } else {
            url.to_string()
        };

        ytdlp::download(&resolved_url, work_dir, cookies, config).await
    }
}

/// Resolve a vm.tiktok.com short URL to full URL.
async fn resolve_short_url(short_url: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(short_url)
        .header("User-Agent", ARCHIVAL_USER_AGENT)
        .send()
        .await
        .context("Failed to resolve short URL")?;

    Ok(response.url().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle() {
        let handler = TikTokHandler::new();

        assert!(handler.can_handle("https://www.tiktok.com/@user/video/123"));
        assert!(handler.can_handle("https://tiktok.com/@user/video/123"));
        assert!(handler.can_handle("https://vm.tiktok.com/abc123"));
        assert!(handler.can_handle("https://m.tiktok.com/@user/video/123"));

        assert!(!handler.can_handle("https://example.com/"));
        assert!(!handler.can_handle("https://youtube.com/"));
    }
}
