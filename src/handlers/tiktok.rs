use std::path::Path;

use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use tracing::{debug, info};

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::{gallerydl, ytdlp, CookieOptions};
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

        // Log cookie configuration status (helps diagnose auth-required errors)
        let cookies_configured =
            cookies.cookies_file.is_some() || cookies.browser_profile.is_some();
        info!(
            url = %resolved_url,
            cookies_configured = %cookies_configured,
            "Starting TikTok archive"
        );

        // Pre-flight: detect content type (video vs photo slideshow)
        let metadata = match ytdlp::get_tiktok_metadata(&resolved_url, cookies).await {
            Ok(meta) => {
                debug!(
                    url = %resolved_url,
                    format = ?meta.format,
                    "Detected TikTok content type from metadata"
                );
                Some(meta)
            }
            Err(e) => {
                debug!(
                    url = %resolved_url,
                    error = %e,
                    "Failed to get TikTok metadata, will try gallery-dl as fallback"
                );
                None
            }
        };

        // Route to appropriate tool based on detected format
        let mut result = match metadata {
            Some(ref meta) if meta.format == ytdlp::TikTokContentFormat::PhotoSlideshow => {
                debug!(url = %resolved_url, "Routing to gallery-dl for TikTok photo slideshow");
                gallerydl::download(&resolved_url, work_dir, cookies, config)
                    .await
                    .with_context(|| {
                        format!(
                            "TikTok photo slideshow download failed (cookies configured: {})",
                            cookies_configured
                        )
                    })?
            }
            Some(ref meta) if meta.format == ytdlp::TikTokContentFormat::Video => {
                debug!(url = %resolved_url, "Routing to yt-dlp for TikTok video");
                ytdlp::download(&resolved_url, work_dir, cookies, config, None, None)
                    .await
                    .with_context(|| {
                        format!(
                            "TikTok video download failed (cookies configured: {})",
                            cookies_configured
                        )
                    })?
            }
            _ => {
                // Unknown or failed detection - try gallery-dl as safe fallback for TikTok
                debug!(url = %resolved_url, "Content type unknown, trying gallery-dl as fallback");
                match gallerydl::download(&resolved_url, work_dir, cookies, config).await {
                    Ok(result) => result,
                    Err(gallery_err) => {
                        debug!(error = %gallery_err, "gallery-dl failed, trying yt-dlp");
                        ytdlp::download(&resolved_url, work_dir, cookies, config, None, None)
                            .await
                            .with_context(|| {
                                format!(
                                    "TikTok download failed (cookies configured: {})",
                                    cookies_configured
                                )
                            })?
                    }
                }
            }
        };

        // Save pre-flight metadata if we have it
        if let Some(meta) = metadata {
            let metadata_filename = ytdlp::save_metadata_to_file(work_dir, &meta.json).await?;

            // If gallery-dl was used, it might not have TikTok metadata,
            // so include the yt-dlp metadata we fetched
            if result.metadata_json.is_none() {
                result.metadata_json = Some(meta.json);
            }

            // Add metadata file to extra_files if not already tracked
            if !result.extra_files.contains(&metadata_filename) {
                result.extra_files.push(metadata_filename);
            }
        }

        // Extract video_id for deduplication
        if let Some(video_id) = extract_video_id(&resolved_url) {
            debug!(video_id = %video_id, "Extracted TikTok video ID");
            result.video_id = Some(video_id);
        }

        Ok(result)
    }
}

/// Extract video ID from TikTok URL.
///
/// TikTok video URLs have formats like:
/// - `https://www.tiktok.com/@user/video/1234567890123456789`
/// - `https://tiktok.com/@user/video/1234567890123456789`
pub fn extract_video_id(url: &str) -> Option<String> {
    // Look for /video/{id} pattern
    if let Some(idx) = url.find("/video/") {
        let rest = &url[idx + 7..]; // Skip "/video/"
                                    // Take digits until non-digit or end
        let video_id: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if !video_id.is_empty() {
            return Some(video_id);
        }
    }
    None
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

    #[test]
    fn test_extract_video_id() {
        assert_eq!(
            extract_video_id("https://www.tiktok.com/@user/video/1234567890123456789"),
            Some("1234567890123456789".to_string())
        );
        assert_eq!(
            extract_video_id("https://tiktok.com/@someuser/video/9876543210"),
            Some("9876543210".to_string())
        );
        assert_eq!(
            extract_video_id("https://www.tiktok.com/@user/video/123?is_copy_url=1"),
            Some("123".to_string())
        );
        // No video ID
        assert_eq!(extract_video_id("https://vm.tiktok.com/abc123"), None);
        assert_eq!(extract_video_id("https://tiktok.com/@user"), None);
    }
}
