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

        // Detect photo slideshow URLs early - yt-dlp doesn't support /photo/ URLs
        let is_photo_url = resolved_url.contains("/photo/");

        // Pre-flight: detect content type (video vs photo slideshow)
        // Skip yt-dlp for /photo/ URLs since yt-dlp doesn't support them
        let metadata = if is_photo_url {
            debug!(url = %resolved_url, "Detected /photo/ URL, skipping yt-dlp (not supported)");
            None
        } else {
            match ytdlp::get_tiktok_metadata(&resolved_url, cookies).await {
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
        if let Some(ref meta) = metadata {
            let metadata_filename = ytdlp::save_metadata_to_file(work_dir, &meta.json).await?;

            // If gallery-dl was used, it might not have TikTok metadata,
            // so include the yt-dlp metadata we fetched
            if result.metadata_json.is_none() {
                result.metadata_json = Some(meta.json.clone());
            }

            // Add metadata file to extra_files if not already tracked
            if !result.extra_files.contains(&metadata_filename) {
                result.extra_files.push(metadata_filename);
            }

            // Download TikTok music if present (best-effort, non-fatal)
            if let Some((music_url, music_title)) = extract_music_info(&meta.json) {
                match download_tiktok_music(&music_url, music_title.as_deref(), work_dir).await {
                    Ok(filename) => {
                        debug!(music_url = %music_url, filename = %filename, "Downloaded TikTok music");
                        result.extra_files.push(filename);
                    }
                    Err(e) => {
                        // Non-fatal: music is supplementary content
                        tracing::warn!(
                            error = %e,
                            music_url = %music_url,
                            "Failed to download TikTok music (non-fatal)"
                        );
                    }
                }
            }

            // For photo slideshows, extract and store image order
            if meta.format == ytdlp::TikTokContentFormat::PhotoSlideshow {
                if let Some(image_order) = extract_image_order(&meta.json) {
                    // Append image order to text field (or prepend if text exists)
                    let current_text = result.text.unwrap_or_default();
                    let combined = if current_text.is_empty() {
                        image_order
                    } else {
                        format!("{}\n\n{}", image_order, current_text)
                    };
                    result.text = Some(combined);
                    debug!("Stored TikTok image order for slideshow");
                }
            }
        } else if is_photo_url && result.metadata_json.is_some() {
            // For /photo/ URLs where we skipped yt-dlp, extract from gallery-dl metadata
            let gallerydl_metadata = result.metadata_json.as_ref().unwrap();

            // Download TikTok music if present (best-effort, non-fatal)
            if let Some((music_url, music_title)) = extract_music_info(gallerydl_metadata) {
                match download_tiktok_music(&music_url, music_title.as_deref(), work_dir).await {
                    Ok(filename) => {
                        debug!(music_url = %music_url, filename = %filename, "Downloaded TikTok music from gallery-dl metadata");
                        result.extra_files.push(filename);
                    }
                    Err(e) => {
                        // Non-fatal: music is supplementary content
                        tracing::warn!(
                            error = %e,
                            music_url = %music_url,
                            "Failed to download TikTok music (non-fatal)"
                        );
                    }
                }
            }

            // Extract and store image order for photo slideshows
            if let Some(image_order) = extract_image_order(gallerydl_metadata) {
                // Append image order to text field (or prepend if text exists)
                let current_text = result.text.unwrap_or_default();
                let combined = if current_text.is_empty() {
                    image_order
                } else {
                    format!("{}\n\n{}", image_order, current_text)
                };
                result.text = Some(combined);
                debug!("Stored TikTok image order for slideshow from gallery-dl metadata");
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

/// Extract music metadata from TikTok JSON metadata.
///
/// Supports both yt-dlp format (music.playUrl) and gallery-dl format (music.play_url).
///
/// Returns (music_url, music_title) if music field exists.
fn extract_music_info(metadata_json: &str) -> Option<(String, Option<String>)> {
    let json: serde_json::Value = serde_json::from_str(metadata_json).ok()?;

    let music_obj = json.get("music")?;

    // Try yt-dlp format first (camelCase)
    let play_url = music_obj
        .get("playUrl")
        .or_else(|| music_obj.get("play_url")) // gallery-dl format (snake_case)
        .and_then(|v| v.as_str())?
        .to_string();

    let title = music_obj
        .get("title")
        .and_then(|t| t.as_str())
        .map(String::from);

    Some((play_url, title))
}

/// Extract ordered image hashes from TikTok slideshow metadata.
///
/// Returns a JSON array of image hashes in the correct slide order.
/// These hashes can be used to match downloaded files and preserve ordering.
fn extract_image_order(metadata_json: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(metadata_json).ok()?;

    let images = json.get("imagePost")?.get("images")?.as_array()?;

    let hashes: Vec<String> = images
        .iter()
        .filter_map(|img| {
            // Extract URL from imageURL.urlList[0]
            let url = img.get("imageURL")?.get("urlList")?.get(0)?.as_str()?;

            // Extract hash from URL (pattern: /HASH~)
            // Example: /303abb876a544755912cc1486b9be949~tplv-photomode-image.jpeg
            let hash = url.split('/').last()?.split('~').next()?.to_string();

            Some(hash)
        })
        .collect();

    if hashes.is_empty() {
        return None;
    }

    // Return as JSON array
    serde_json::to_string(&serde_json::json!({
        "tiktok_image_order": hashes
    }))
    .ok()
}

/// Download TikTok background music.
///
/// # Errors
///
/// Returns an error if HTTP request or file write fails.
async fn download_tiktok_music(
    music_url: &str,
    title: Option<&str>,
    work_dir: &Path,
) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(music_url)
        .header("User-Agent", ARCHIVAL_USER_AGENT)
        .send()
        .await
        .context("Failed to fetch TikTok music")?;

    if !response.status().is_success() {
        anyhow::bail!("Music download failed with status {}", response.status());
    }

    // Infer extension from Content-Type or URL
    let extension = response
        .headers()
        .get("content-type")
        .and_then(|ct| ct.to_str().ok())
        .and_then(|ct| match ct {
            ct if ct.contains("audio/mpeg") => Some("mp3"),
            ct if ct.contains("audio/mp4") => Some("m4a"),
            ct if ct.contains("audio/aac") => Some("aac"),
            _ => None,
        })
        .or_else(|| {
            music_url
                .split('.')
                .last()
                .filter(|ext| matches!(*ext, "mp3" | "m4a" | "aac" | "wav"))
        })
        .unwrap_or("mp3");

    let filename = format!("tiktok_music.{extension}");
    let file_path = work_dir.join(&filename);

    let bytes = response
        .bytes()
        .await
        .context("Failed to read music data")?;
    tokio::fs::write(&file_path, &bytes)
        .await
        .context("Failed to write music file")?;

    debug!(
        path = %file_path.display(),
        size_bytes = bytes.len(),
        title = ?title,
        "Saved TikTok music file"
    );

    Ok(filename)
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
