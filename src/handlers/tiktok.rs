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
                ytdlp::download(&resolved_url, work_dir, cookies, config, None, None, false)
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
                        ytdlp::download(&resolved_url, work_dir, cookies, config, None, None, false)
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

            // Download TikTok subtitles if present (best-effort, non-fatal)
            // Only download for videos, not photo slideshows
            if meta.format == ytdlp::TikTokContentFormat::Video {
                let subtitles = extract_subtitle_info(&meta.json);
                if !subtitles.is_empty() {
                    debug!(
                        count = subtitles.len(),
                        "Found TikTok subtitles in metadata"
                    );
                    match download_tiktok_subtitles(&subtitles, work_dir, true).await {
                        Ok(filenames) => {
                            for filename in filenames {
                                debug!(filename = %filename, "Downloaded TikTok subtitle");
                                result.extra_files.push(filename);
                            }
                        }
                        Err(e) => {
                            // Non-fatal: subtitles are supplementary content
                            tracing::warn!(
                                error = %e,
                                "Failed to download TikTok subtitles (non-fatal)"
                            );
                        }
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

/// Subtitle info extracted from TikTok metadata.
#[derive(Debug, Clone)]
pub struct TikTokSubtitleInfo {
    /// Language code (e.g., "eng-US", "ind-ID")
    pub language_code: String,
    /// URL to the VTT subtitle file
    pub url: String,
    /// File extension (typically "vtt")
    pub ext: String,
}

/// Extract subtitles from the "subtitles" object format.
/// Format: { "subtitles": { "eng-US": [{"url": "...", "ext": "vtt"}] } }
fn extract_from_subtitles_object(json: &serde_json::Value) -> Option<Vec<TikTokSubtitleInfo>> {
    let subtitles = json.get("subtitles")?.as_object()?;

    let results: Vec<_> = subtitles
        .iter()
        .filter_map(|(lang_code, entries)| {
            let entries_arr = entries.as_array()?;

            // Prefer VTT format over JSON
            let entry = entries_arr
                .iter()
                .find(|e| e.get("ext").and_then(|v| v.as_str()) == Some("vtt"))
                .or_else(|| entries_arr.first())?;

            Some(TikTokSubtitleInfo {
                language_code: lang_code.clone(),
                url: entry.get("url")?.as_str()?.to_string(),
                ext: entry
                    .get("ext")
                    .and_then(|e| e.as_str())
                    .unwrap_or("vtt")
                    .to_string(),
            })
        })
        .collect();

    if results.is_empty() {
        None
    } else {
        Some(results)
    }
}

/// Extract subtitles from the "subtitleInfos" array format (newer TikTok API).
/// Format: { "video": { "subtitleInfos": [{"LanguageCodeName": "eng-US", "Url": "...", "Format": "webvtt"}] } }
/// Or: { "subtitleInfos": [{"LanguageCodeName": "eng-US", "Url": "...", "Format": "webvtt"}] }
fn extract_from_subtitle_infos_array(json: &serde_json::Value) -> Option<Vec<TikTokSubtitleInfo>> {
    // Check both root level and nested in .video object
    let subtitle_infos = json
        .get("subtitleInfos")
        .or_else(|| json.get("video").and_then(|v| v.get("subtitleInfos")))?
        .as_array()?;

    let results: Vec<_> = subtitle_infos
        .iter()
        .filter_map(|info| {
            let format = info.get("Format")?.as_str()?;

            // Only accept webvtt format - other formats like "creator_caption" may be JSON
            let ext = match format {
                "webvtt" => "vtt",
                _ => return None, // Skip non-VTT formats
            };

            Some(TikTokSubtitleInfo {
                language_code: info.get("LanguageCodeName")?.as_str()?.to_string(),
                url: info.get("Url")?.as_str()?.to_string(),
                ext: ext.to_string(),
            })
        })
        .collect();

    if results.is_empty() {
        None
    } else {
        Some(results)
    }
}

/// Compute priority for language code sorting.
/// eng-US (0) > eng-GB (1) > other eng-* (2) > all others (3)
fn language_priority(code: &str) -> u8 {
    match code {
        "eng-US" => 0,
        "eng-GB" => 1,
        _ if code.starts_with("eng-") => 2,
        _ => 3,
    }
}

/// Extract subtitle URLs from TikTok JSON metadata.
///
/// TikTok metadata contains a "subtitles" object with language codes as keys
/// (e.g., "eng-US", "ind-ID") and arrays of subtitle objects with "url" and "ext" fields.
///
/// Returns a list of subtitle info, prioritized with English subtitles first.
/// Priority: eng-US > eng-GB > eng-* > all others (alphabetically)
pub fn extract_subtitle_info(metadata_json: &str) -> Vec<TikTokSubtitleInfo> {
    let json: serde_json::Value = match serde_json::from_str(metadata_json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    // Try both formats and use whichever succeeds
    let mut results = extract_from_subtitles_object(&json)
        .or_else(|| extract_from_subtitle_infos_array(&json))
        .unwrap_or_default();

    if results.is_empty() {
        return Vec::new();
    }

    // Sort by priority, then alphabetically within same priority
    results.sort_by(|a, b| {
        language_priority(&a.language_code)
            .cmp(&language_priority(&b.language_code))
            .then_with(|| a.language_code.cmp(&b.language_code))
    });

    results
}

/// Download TikTok subtitles from extracted URLs.
///
/// Downloads VTT files from TikTok CDN and saves them to work_dir.
/// Returns list of downloaded filenames.
///
/// Naming convention: `tiktok.{language_code}.{ext}` (yt-dlp compatible format)
/// e.g., `tiktok.eng-US.vtt`
///
/// # Arguments
///
/// * `subtitles` - List of subtitle info extracted from metadata
/// * `work_dir` - Directory to save downloaded files
/// * `english_only` - If true, only download English subtitles (eng-*)
///
/// # Errors
///
/// Returns an error only if work_dir is invalid. Individual subtitle download
/// failures are logged as warnings but don't fail the operation.
pub async fn download_tiktok_subtitles(
    subtitles: &[TikTokSubtitleInfo],
    work_dir: &Path,
    english_only: bool,
) -> Result<Vec<String>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("Failed to build HTTP client")?;

    let mut downloaded = Vec::new();

    for sub in subtitles {
        // Skip non-English if english_only is set
        if english_only && !sub.language_code.starts_with("eng-") {
            continue;
        }

        // Use yt-dlp compatible format: tiktok.{lang}.{ext}
        let filename = format!("tiktok.{}.{}", sub.language_code, sub.ext);
        let file_path = work_dir.join(&filename);

        match download_single_subtitle(&client, &sub.url, &file_path).await {
            Ok(size) => {
                debug!(
                    language = %sub.language_code,
                    filename = %filename,
                    size_bytes = size,
                    "Downloaded TikTok subtitle"
                );
                downloaded.push(filename);
            }
            Err(e) => {
                // Non-fatal: log warning and continue with other subtitles
                tracing::warn!(
                    language = %sub.language_code,
                    url = %sub.url,
                    error = %e,
                    "Failed to download TikTok subtitle (non-fatal)"
                );
            }
        }
    }

    Ok(downloaded)
}

/// Download a single subtitle file.
async fn download_single_subtitle(
    client: &reqwest::Client,
    url: &str,
    file_path: &Path,
) -> Result<usize> {
    let response = client
        .get(url)
        .header("User-Agent", ARCHIVAL_USER_AGENT)
        .send()
        .await
        .context("Failed to fetch subtitle")?;

    if !response.status().is_success() {
        anyhow::bail!("Subtitle download failed with status {}", response.status());
    }

    let bytes = response
        .bytes()
        .await
        .context("Failed to read subtitle data")?;

    tokio::fs::write(file_path, &bytes)
        .await
        .context("Failed to write subtitle file")?;

    Ok(bytes.len())
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

    #[test]
    fn test_extract_subtitle_info_basic() {
        let json = r#"{
            "subtitles": {
                "eng-US": [{"url": "https://example.com/eng-us.vtt", "ext": "vtt"}],
                "ind-ID": [{"url": "https://example.com/ind-id.vtt", "ext": "vtt"}]
            }
        }"#;

        let subs = extract_subtitle_info(json);
        assert_eq!(subs.len(), 2);
        // English should be first
        assert_eq!(subs[0].language_code, "eng-US");
        assert_eq!(subs[0].url, "https://example.com/eng-us.vtt");
        assert_eq!(subs[0].ext, "vtt");
    }

    #[test]
    fn test_extract_subtitle_info_priority() {
        let json = r#"{
            "subtitles": {
                "ind-ID": [{"url": "https://example.com/ind.vtt", "ext": "vtt"}],
                "eng-GB": [{"url": "https://example.com/gb.vtt", "ext": "vtt"}],
                "eng-US": [{"url": "https://example.com/us.vtt", "ext": "vtt"}],
                "eng-AU": [{"url": "https://example.com/au.vtt", "ext": "vtt"}]
            }
        }"#;

        let subs = extract_subtitle_info(json);
        assert_eq!(subs.len(), 4);
        // Priority: eng-US > eng-GB > eng-AU > ind-ID
        assert_eq!(subs[0].language_code, "eng-US");
        assert_eq!(subs[1].language_code, "eng-GB");
        assert_eq!(subs[2].language_code, "eng-AU");
        assert_eq!(subs[3].language_code, "ind-ID");
    }

    #[test]
    fn test_extract_subtitle_info_no_subtitles() {
        let json = r#"{"title": "Test Video"}"#;
        let subs = extract_subtitle_info(json);
        assert!(subs.is_empty());
    }

    #[test]
    fn test_extract_subtitle_info_invalid_json() {
        let json = "not valid json";
        let subs = extract_subtitle_info(json);
        assert!(subs.is_empty());
    }

    #[test]
    fn test_extract_subtitle_info_empty_subtitles() {
        let json = r#"{"subtitles": {}}"#;
        let subs = extract_subtitle_info(json);
        assert!(subs.is_empty());
    }

    #[test]
    fn test_extract_subtitle_info_prefers_vtt() {
        // TikTok provides both JSON and VTT formats - we should prefer VTT
        let json = r#"{
            "subtitles": {
                "eng-US": [
                    {"url": "https://example.com/subs.json", "ext": "json"},
                    {"url": "https://example.com/subs.vtt", "ext": "vtt"}
                ]
            }
        }"#;

        let subs = extract_subtitle_info(json);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].ext, "vtt");
        assert_eq!(subs[0].url, "https://example.com/subs.vtt");
    }

    #[test]
    fn test_extract_subtitle_info_falls_back_to_first() {
        // If no VTT available, fall back to first entry
        let json = r#"{
            "subtitles": {
                "eng-US": [
                    {"url": "https://example.com/subs.json", "ext": "json"}
                ]
            }
        }"#;

        let subs = extract_subtitle_info(json);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].ext, "json");
    }

    #[test]
    fn test_extract_subtitle_info_real_file() {
        let json_content = std::fs::read_to_string("api-examples/tiktok_meta_video2.json")
            .expect("Failed to read test file");

        let subs = extract_subtitle_info(&json_content);

        // The file should have subtitles
        assert!(!subs.is_empty(), "Expected subtitles, got none");

        // Should have eng-US
        let has_eng_us = subs.iter().any(|s| s.language_code == "eng-US");
        assert!(has_eng_us, "Expected eng-US subtitle");

        // First should be eng-US (priority order)
        assert_eq!(subs[0].language_code, "eng-US");

        println!(
            "Found {} subtitles, first is: {}",
            subs.len(),
            subs[0].language_code
        );
    }

    #[test]
    fn test_extract_subtitle_info_subtitleinfos_format() {
        // Test the newer TikTok subtitleInfos array format
        // Note: only "webvtt" format is extracted, "creator_caption" is skipped
        let json = r#"{
            "subtitleInfos": [
                {
                    "LanguageCodeName": "eng-US",
                    "Url": "https://example.com/subs1.vtt",
                    "Format": "webvtt"
                },
                {
                    "LanguageCodeName": "eng-US",
                    "Url": "https://example.com/subs2.vtt",
                    "Format": "creator_caption"
                },
                {
                    "LanguageCodeName": "ind-ID",
                    "Url": "https://example.com/subs3.vtt",
                    "Format": "webvtt"
                }
            ]
        }"#;

        let subs = extract_subtitle_info(json);
        // Should only extract webvtt formats (2 out of 3)
        assert_eq!(subs.len(), 2);

        // All should be VTT format
        assert!(subs.iter().all(|s| s.ext == "vtt"));

        // Should prioritize eng-US
        assert_eq!(subs[0].language_code, "eng-US");

        // Second should be ind-ID (creator_caption was skipped)
        assert_eq!(subs[1].language_code, "ind-ID");
    }

    #[test]
    fn test_extract_subtitle_info_nested_video_subtitleinfos() {
        // Test subtitleInfos nested in video object (actual TikTok API format)
        let json = r#"{
            "video": {
                "id": "12345",
                "subtitleInfos": [
                    {
                        "LanguageCodeName": "eng-US",
                        "Url": "https://example.com/nested.vtt",
                        "Format": "webvtt"
                    }
                ]
            }
        }"#;

        let subs = extract_subtitle_info(json);
        assert!(!subs.is_empty(), "Should find nested subtitleInfos");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].language_code, "eng-US");
        assert_eq!(subs[0].ext, "vtt");
    }

    #[test]
    fn test_extract_subtitle_info_real_file_255() {
        // Test with actual archive 255 file that has nested video.subtitleInfos
        let json_content = std::fs::read_to_string("api-examples/tiktok_meta_video-255.json")
            .expect("Failed to read test file");

        let subs = extract_subtitle_info(&json_content);

        // Should find subtitles in the nested video object
        assert!(
            !subs.is_empty(),
            "Expected subtitles in video.subtitleInfos, got none"
        );

        // Should have eng-US
        let has_eng_us = subs.iter().any(|s| s.language_code == "eng-US");
        assert!(has_eng_us, "Expected eng-US subtitle");

        println!(
            "Found {} subtitles from nested format, first is: {}",
            subs.len(),
            subs[0].language_code
        );
    }
}
