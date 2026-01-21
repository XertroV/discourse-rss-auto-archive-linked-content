use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use reqwest::header;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::{gallerydl, ytdlp, CookieOptions};
use crate::chromium_profile::fetch_html_with_chromium;
use crate::constants::ARCHIVAL_USER_AGENT;

static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        Regex::new(r"^https?://(www\.)?twitter\.com/").unwrap(),
        Regex::new(r"^https?://(www\.)?x\.com/").unwrap(),
        Regex::new(r"^https?://mobile\.twitter\.com/").unwrap(),
        Regex::new(r"^https?://mobile\.x\.com/").unwrap(),
    ]
});

/// Pattern to extract tweet ID from URL.
static TWEET_ID_PATTERN: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"/status/(\d+)").unwrap());

/// Metadata extracted from Twitter/X content.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TwitterMetadata {
    pub tweet_id: Option<String>,
    pub author: Option<String>,
    pub author_id: Option<String>,
    pub text: Option<String>,
    pub created_at: Option<String>,
    pub quoted_tweet_url: Option<String>,
    pub reply_to_tweet_url: Option<String>,
    pub media_count: usize,
    pub is_retweet: bool,
    pub source_url: String,
    /// Whether Twitter marked this as possibly_sensitive.
    pub possibly_sensitive: Option<bool>,
    /// Whether Twitter marked this as sensitive (alternate field name).
    pub sensitive: Option<bool>,
}

/// Result of attempting to archive from a source.
#[derive(Debug)]
#[allow(dead_code)] // Variants reserved for future use
enum ArchiveAttempt {
    Success(ArchiveResult),
    RateLimited,
    Blocked,
    NotFound,
    Error(anyhow::Error),
}

pub struct TwitterHandler;

impl TwitterHandler {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for TwitterHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SiteHandler for TwitterHandler {
    fn site_id(&self) -> &'static str {
        "twitter"
    }

    fn url_patterns(&self) -> &[Regex] {
        &PATTERNS
    }

    fn priority(&self) -> i32 {
        100
    }

    fn normalize_url(&self, url: &str) -> String {
        // Normalize all variants to x.com
        let normalized = url
            .replace("://twitter.com/", "://x.com/")
            .replace("://www.twitter.com/", "://x.com/")
            .replace("://mobile.twitter.com/", "://x.com/")
            .replace("://www.x.com/", "://x.com/")
            .replace("://mobile.x.com/", "://x.com/");

        super::normalize::normalize_url(&normalized)
    }

    async fn archive(
        &self,
        url: &str,
        work_dir: &Path,
        cookies: &CookieOptions<'_>,
        config: &crate::config::Config,
    ) -> Result<ArchiveResult> {
        let normalized_url = self.normalize_url(url);
        let tweet_id = extract_tweet_id(&normalized_url);

        debug!(url = %normalized_url, tweet_id = ?tweet_id, "Archiving Twitter/X content");

        // Start with an empty result - we'll populate it based on what we find
        let mut result = ArchiveResult::default();

        // Set video_id for deduplication
        if let Some(ref tid) = tweet_id {
            result.video_id = Some(format!("twitter_{tid}"));
        }

        // Store final URL
        result.final_url = Some(normalized_url.clone());

        // Step 1: Fetch HTML snapshot first (this is our primary source of truth)
        // This allows us to detect if there's media before calling yt-dlp/gallery-dl
        let mut html_content: Option<String> = None;
        if config.twitter_html_snapshot {
            match fetch_html_snapshot_cdp(&normalized_url, work_dir, cookies).await {
                Ok(()) => {
                    debug!(url = %normalized_url, "Successfully saved HTML snapshot");
                    let html_path = work_dir.join("raw.html");
                    if let Ok(html) = tokio::fs::read_to_string(&html_path).await {
                        html_content = Some(html);
                    }
                    result.primary_file = Some("raw.html".to_string());
                    result.content_type = "thread".to_string();
                }
                Err(e) => {
                    warn!(url = %normalized_url, error = %e, "Failed to fetch HTML snapshot for Twitter");
                }
            }
        }

        // Step 2: Detect if there's media in the HTML
        let has_media = html_content
            .as_ref()
            .map(|html| detect_media_in_html(html))
            .unwrap_or(false);

        // Step 3: Only call yt-dlp/gallery-dl if we detected media
        if has_media {
            debug!(url = %normalized_url, "Media detected in tweet, attempting download");

            match archive_twitter_direct(&normalized_url, work_dir, cookies, config).await {
                Ok(media_result) => {
                    // Merge media result with our result
                    if media_result.primary_file.is_some() {
                        result.primary_file = media_result.primary_file;
                    }
                    if !media_result.content_type.is_empty() {
                        result.content_type = media_result.content_type;
                    }
                    result.title = media_result.title.or(result.title);
                    result.author = media_result.author.or(result.author);
                    result.text = media_result.text.or(result.text);
                    result.metadata_json = media_result.metadata_json.or(result.metadata_json);
                    result.is_nsfw = media_result.is_nsfw.or(result.is_nsfw);
                    result.nsfw_source = media_result.nsfw_source.or(result.nsfw_source);
                }
                Err(e) => {
                    // Media download failed, but we still have the HTML snapshot
                    warn!(url = %normalized_url, error = %e, "Failed to download Twitter media, using HTML snapshot only");
                }
            }
        } else {
            debug!(url = %normalized_url, "No media detected in tweet, skipping yt-dlp/gallery-dl");
        }

        // Step 4: Detect NSFW from HTML if not already detected
        if result.is_nsfw.is_none() || !result.is_nsfw.unwrap_or(false) {
            if let Some(ref html) = html_content {
                if detect_nsfw_from_html(html) {
                    result.is_nsfw = Some(true);
                    result.nsfw_source = Some("twitter_html".to_string());
                    debug!(url = %normalized_url, "Detected NSFW from Twitter HTML snapshot");
                }
            }
        }

        // Ensure we have a primary file (either HTML or media)
        if result.primary_file.is_none() {
            // No HTML and no media - this is a failure
            anyhow::bail!("Failed to archive Twitter content: no HTML snapshot or media");
        }

        Ok(result)
    }
}

/// Archive Twitter content directly (via yt-dlp, with gallery-dl fallback for images).
///
/// We use yt-dlp first because:
/// - It handles videos reliably
/// - It provides good metadata for text-only tweets
/// - gallery-dl can cause issues and is only needed for image galleries
async fn archive_twitter_direct(
    url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
    config: &crate::config::Config,
) -> Result<ArchiveResult> {
    // Try yt-dlp first - handles videos and provides metadata
    debug!(url = %url, "Trying yt-dlp for Twitter content");
    match ytdlp::download(url, work_dir, cookies, config, None, None).await {
        Ok(mut result) => {
            debug!("yt-dlp succeeded for Twitter");

            // Extract tweet ID for deduplication
            if let Some(tweet_id) = extract_tweet_id(url) {
                debug!(tweet_id = %tweet_id, "Extracted Twitter tweet ID");
                result.video_id = Some(format!("twitter_{tweet_id}"));
            }

            // Default to "thread" for text-only tweets (not "image")
            if result.content_type.is_empty() && result.primary_file.is_none() {
                result.content_type = "thread".to_string();
            }

            // Detect NSFW from yt-dlp metadata
            if result.is_nsfw.is_none() {
                let (is_nsfw, nsfw_source) =
                    detect_nsfw_from_ytdlp_metadata(result.metadata_json.as_deref());
                if is_nsfw.unwrap_or(false) {
                    result.is_nsfw = is_nsfw;
                    result.nsfw_source = nsfw_source;
                }
            }

            return Ok(result);
        }
        Err(e) => {
            let err_str = e.to_string();
            if is_rate_limit_error(&err_str) {
                debug!("yt-dlp rate-limited for Twitter: {e}");
                return Err(e);
            }
            // yt-dlp failed - might be an image-only tweet, try gallery-dl
            debug!("yt-dlp failed for Twitter, trying gallery-dl for images: {e}");
        }
    }

    // Fall back to gallery-dl for image-only tweets
    debug!(url = %url, "Trying gallery-dl for Twitter images");
    let mut result = gallerydl::download(url, work_dir, cookies).await?;

    // Extract Twitter-specific metadata from gallery-dl JSON
    if let Some(ref json_str) = result.metadata_json {
        if let Ok(metadata) = extract_twitter_metadata_from_json(json_str) {
            // Update result with extracted metadata
            if result.title.is_none() && metadata.text.is_some() {
                // Use first 100 chars of tweet text as title
                result.title = metadata.text.as_ref().map(|t| {
                    if t.len() > 100 {
                        format!("{}...", &t[..97])
                    } else {
                        t.clone()
                    }
                });
            }
            if result.author.is_none() {
                result.author = metadata.author.clone();
            }
            if result.text.is_none() {
                result.text = metadata.text.clone();
            }

            // Set content_type based on actual media count
            if metadata.media_count == 0 {
                result.content_type = "thread".to_string();
            } else if metadata.media_count == 1 {
                result.content_type = "image".to_string();
            } else if metadata.media_count > 1 {
                result.content_type = "gallery".to_string();
            }

            // Store enhanced metadata
            let enhanced_metadata = serde_json::json!({
                "twitter": metadata,
                "original_metadata": serde_json::from_str::<serde_json::Value>(json_str).ok(),
            });
            result.metadata_json = Some(enhanced_metadata.to_string());
        }
    }

    // Default to "thread" if content_type wasn't set (not "image")
    if result.content_type.is_empty() {
        result.content_type = "thread".to_string();
    }

    // Extract tweet ID for deduplication
    if let Some(tweet_id) = extract_tweet_id(url) {
        result.video_id = Some(format!("twitter_{tweet_id}"));
    }

    // Detect NSFW from gallery-dl metadata
    if result.is_nsfw.is_none() {
        if let Some(ref json_str) = result.metadata_json {
            let (is_nsfw, nsfw_source) = detect_nsfw_from_gallerydl_json(json_str);
            if is_nsfw.unwrap_or(false) {
                result.is_nsfw = is_nsfw;
                result.nsfw_source = nsfw_source;
            }
        }
    }

    Ok(result)
}

/// Try to archive via nitter instances.
async fn try_nitter_archive(
    twitter_url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
    config: &crate::config::Config,
    nitter_instances: &[String],
) -> ArchiveAttempt {
    for instance in nitter_instances {
        let nitter_url = get_nitter_url(twitter_url, instance);
        debug!(nitter_url = %nitter_url, instance = %instance, "Trying nitter instance");

        match archive_nitter(&nitter_url, work_dir, cookies, config).await {
            Ok(result) => {
                info!(instance = %instance, "Successfully archived via nitter");
                return ArchiveAttempt::Success(result);
            }
            Err(e) => {
                let err_str = e.to_string();
                let err_lower = err_str.to_lowercase();
                if err_lower.contains("404") || err_lower.contains("not found") {
                    debug!(instance = %instance, "Nitter instance returned 404");
                    continue;
                }
                if is_rate_limit_error(&err_str) {
                    debug!(instance = %instance, "Nitter instance rate-limited");
                    continue;
                }
                debug!(instance = %instance, error = %e, "Nitter instance failed");
            }
        }
    }

    ArchiveAttempt::Error(anyhow::anyhow!(
        "All nitter instances failed for {}",
        twitter_url
    ))
}

/// Archive content from a nitter instance.
async fn archive_nitter(
    nitter_url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
    config: &crate::config::Config,
) -> Result<ArchiveResult> {
    // Try yt-dlp first (consistent with direct Twitter archiving)
    match ytdlp::download(nitter_url, work_dir, cookies, config, None, None).await {
        Ok(result) => {
            debug!("yt-dlp succeeded for nitter");
            return Ok(result);
        }
        Err(e) => {
            debug!("yt-dlp failed for nitter: {e}");
        }
    }

    // Fall back to gallery-dl for images
    gallerydl::download(nitter_url, work_dir, cookies).await
}

/// Convert Twitter/X URL to nitter URL.
pub fn get_nitter_url(twitter_url: &str, nitter_instance: &str) -> String {
    // Ensure instance doesn't have trailing slash
    let instance = nitter_instance.trim_end_matches('/');

    // Handle both x.com and twitter.com URLs
    let url = twitter_url
        .replace("https://x.com/", &format!("https://{instance}/"))
        .replace("https://twitter.com/", &format!("https://{instance}/"))
        .replace("https://www.x.com/", &format!("https://{instance}/"))
        .replace("https://www.twitter.com/", &format!("https://{instance}/"))
        .replace("https://mobile.x.com/", &format!("https://{instance}/"))
        .replace(
            "https://mobile.twitter.com/",
            &format!("https://{instance}/"),
        );

    url
}

/// Check if an error indicates rate limiting.
fn is_rate_limit_error(err_str: &str) -> bool {
    let lower = err_str.to_lowercase();
    lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("temporarily unavailable")
        || lower.contains("try again later")
}

/// Extract tweet ID from Twitter/X URL.
///
/// Twitter URLs have format: `https://x.com/{user}/status/{tweet_id}`
pub fn extract_tweet_id(url: &str) -> Option<String> {
    TWEET_ID_PATTERN
        .captures(url)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}

/// Extract Twitter metadata from gallery-dl JSON output.
fn extract_twitter_metadata_from_json(json_str: &str) -> Result<TwitterMetadata> {
    let json: serde_json::Value =
        serde_json::from_str(json_str).context("Failed to parse gallery-dl JSON")?;

    let mut metadata = TwitterMetadata::default();

    // gallery-dl Twitter extractor fields
    metadata.tweet_id = json
        .get("tweet_id")
        .or_else(|| json.get("id"))
        .and_then(|v| v.as_u64())
        .map(|n| n.to_string())
        .or_else(|| {
            json.get("tweet_id")
                .or_else(|| json.get("id"))
                .and_then(|v| v.as_str())
                .map(String::from)
        });

    metadata.author = json
        .get("author")
        .or_else(|| json.get("user"))
        .and_then(|v| {
            if let Some(obj) = v.as_object() {
                obj.get("name")
                    .or_else(|| obj.get("screen_name"))
                    .and_then(|n| n.as_str())
                    .map(String::from)
            } else {
                v.as_str().map(String::from)
            }
        });

    metadata.author_id = json
        .get("author")
        .or_else(|| json.get("user"))
        .and_then(|v| {
            if let Some(obj) = v.as_object() {
                obj.get("id")
                    .and_then(|n| n.as_u64())
                    .map(|n| n.to_string())
            } else {
                None
            }
        });

    metadata.text = json
        .get("content")
        .or_else(|| json.get("text"))
        .or_else(|| json.get("full_text"))
        .and_then(|v| v.as_str())
        .map(String::from);

    metadata.created_at = json
        .get("date")
        .or_else(|| json.get("created_at"))
        .and_then(|v| v.as_str())
        .map(String::from);

    // Check for quoted tweet
    if let Some(quoted) = json
        .get("quoted_tweet")
        .or_else(|| json.get("quoted_status"))
    {
        if let Some(quoted_id) = quoted.get("id").and_then(|v| v.as_u64()) {
            if let Some(quoted_user) = quoted
                .get("user")
                .or_else(|| quoted.get("author"))
                .and_then(|u| {
                    u.get("screen_name")
                        .or_else(|| u.get("name"))
                        .and_then(|n| n.as_str())
                })
            {
                metadata.quoted_tweet_url =
                    Some(format!("https://x.com/{quoted_user}/status/{quoted_id}"));
            }
        }
    }

    // Check for reply
    if let Some(reply_to_id) = json
        .get("reply_to_tweet_id")
        .or_else(|| json.get("in_reply_to_status_id"))
        .and_then(|v| v.as_u64())
    {
        if let Some(reply_to_user) = json
            .get("reply_to_user")
            .or_else(|| json.get("in_reply_to_screen_name"))
            .and_then(|v| v.as_str())
        {
            metadata.reply_to_tweet_url = Some(format!(
                "https://x.com/{reply_to_user}/status/{reply_to_id}"
            ));
        }
    }

    // Count media
    if let Some(media) = json.get("media") {
        if let Some(arr) = media.as_array() {
            metadata.media_count = arr.len();
        }
    }

    // Check if retweet
    metadata.is_retweet = json.get("retweet").is_some()
        || json
            .get("retweeted_status")
            .map(|v| !v.is_null())
            .unwrap_or(false);

    // Extract NSFW indicators
    metadata.possibly_sensitive = json.get("possibly_sensitive").and_then(|v| v.as_bool());

    metadata.sensitive = json.get("sensitive").and_then(|v| v.as_bool());

    Ok(metadata)
}

/// Extract NSFW status from gallery-dl Twitter JSON metadata.
///
/// Checks for `possibly_sensitive` and `sensitive` fields in JSON output.
/// Returns (is_nsfw, source) tuple.
fn detect_nsfw_from_gallerydl_json(json_str: &str) -> (Option<bool>, Option<String>) {
    let json: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(j) => j,
        Err(_) => return (None, None),
    };

    // Check possibly_sensitive field (primary indicator)
    if let Some(sensitive) = json
        .get("possibly_sensitive")
        .or_else(|| json.get("sensitive"))
        .and_then(|v| v.as_bool())
    {
        if sensitive {
            return (Some(true), Some("twitter_metadata".to_string()));
        }
    }

    (None, None)
}

/// Extract NSFW status from yt-dlp Twitter/X JSON metadata.
///
/// Checks for age_limit and possibly_sensitive fields.
fn detect_nsfw_from_ytdlp_metadata(metadata_json: Option<&str>) -> (Option<bool>, Option<String>) {
    let Some(json_str) = metadata_json else {
        return (None, None);
    };

    let json: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(j) => j,
        Err(_) => return (None, None),
    };

    // Check age_limit (used by some platforms)
    if let Some(age_limit) = json.get("age_limit").and_then(|v| v.as_i64()) {
        if age_limit >= 18 {
            return (Some(true), Some("metadata".to_string()));
        }
    }

    // Check possibly_sensitive
    if let Some(sensitive) = json
        .get("possibly_sensitive")
        .or_else(|| json.get("sensitive"))
        .and_then(|v| v.as_bool())
    {
        if sensitive {
            return (Some(true), Some("twitter_metadata".to_string()));
        }
    }

    (None, None)
}

/// Detect NSFW status from Twitter HTML snapshot.
///
/// Looks for sensitive content warnings and CSS classes.
fn detect_nsfw_from_html(html: &str) -> bool {
    let html_lower = html.to_ascii_lowercase();

    // Look for explicit sensitive content indicators
    html_lower.contains("potentially sensitive content")
        || html_lower.contains("sensitive media")
        || html_lower.contains("this media may contain sensitive")
        // CSS/data attributes that Twitter uses for blurred content
        || html_lower.contains("data-sensitive=\"true\"")
        || html_lower.contains("is-sensitive-media")
        || html_lower.contains("sensitivemediawarning")
}

/// Detect if the Twitter HTML contains media (video, images, or GIFs).
///
/// This is used to decide whether to invoke yt-dlp/gallery-dl for downloading.
/// For text-only tweets, we skip these tools to avoid unnecessary API calls.
fn detect_media_in_html(html: &str) -> bool {
    let html_lower = html.to_ascii_lowercase();

    // Video indicators
    let has_video = html_lower.contains("data-testid=\"videoplayer\"")
        || html_lower.contains("data-testid=\"videocomponent\"")
        || html_lower.contains("\"video\"")
        || html_lower.contains("<video")
        || html_lower.contains("player.m3u8")
        || html_lower.contains("ext_tw_video")
        || html_lower.contains("amplify_video");

    // Image indicators (excluding profile pictures and icons)
    // Twitter uses data-testid="tweetPhoto" for tweet images
    let has_images = html_lower.contains("data-testid=\"tweetphoto\"")
        || html_lower.contains("pbs.twimg.com/media/")
        || html_lower.contains("pbs.twimg.com/tweet_video")
        || html_lower.contains("pbs.twimg.com/ext_tw_video");

    // GIF indicators
    let has_gif =
        html_lower.contains("data-testid=\"tweetgif\"") || html_lower.contains("tweet_video_thumb");

    // Card with media (preview cards with images/videos)
    let has_media_card = html_lower.contains("data-testid=\"card.wrapper\"")
        && (html_lower.contains("pbs.twimg.com/card_img/")
            || html_lower.contains("data-testid=\"card.layoutlarge.media\""));

    has_video || has_images || has_gif || has_media_card
}

/// Fetch HTML from Twitter/X or nitter for HTML snapshot.
#[allow(dead_code)] // Will be used in Phase 5 (HTML snapshot)
pub async fn fetch_tweet_html(url: &str, cookies: &CookieOptions<'_>) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .context("Failed to build HTTP client")?;

    let mut request = client.get(url).header("User-Agent", ARCHIVAL_USER_AGENT);

    // Add cookies if available
    if let Some(cookies_path) = cookies.cookies_file {
        if cookies_path.exists() && !cookies_path.is_dir() {
            if let Ok(cookie_str) = build_cookie_header_for_domain(cookies_path, "x.com") {
                request = request.header("Cookie", cookie_str);
            }
        }
    }

    let response = request.send().await.context("Failed to fetch tweet HTML")?;

    let status = response.status();
    if status.as_u16() == 429 {
        anyhow::bail!("Rate limited (429)");
    }
    if !status.is_success() {
        anyhow::bail!("HTTP error: {status}");
    }

    response
        .text()
        .await
        .context("Failed to read response body")
}

/// Build cookie header string from cookies.txt file for a specific domain.
#[allow(dead_code)] // Used by fetch_tweet_html
fn build_cookie_header_for_domain(cookies_path: &Path, domain: &str) -> Result<String> {
    let content = std::fs::read_to_string(cookies_path).context("Failed to read cookies file")?;

    let mut cookies = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 7 {
            let cookie_domain = parts[0].trim_start_matches('.');
            if cookie_domain.ends_with(domain) || domain.ends_with(cookie_domain) {
                let name = parts[5];
                let value = parts[6];
                cookies.push(format!("{name}={value}"));
            }
        }
    }

    if cookies.is_empty() {
        anyhow::bail!("No cookies found for domain {domain}");
    }

    Ok(cookies.join("; "))
}

/// Fetch HTML snapshot using browser to get rendered HTML after JS execution.
///
/// Twitter requires JavaScript to render content, so we use a headless browser to
/// get the fully rendered DOM. This function tries multiple methods in order:
/// 1. ScreenshotService CDP (best - uses persistent browser with proper waiting)
/// 2. Chromium --dump-dom CLI (fallback - may not wait long enough for JS)
/// 3. HTTP fetch (last resort - no JS rendering)
///
/// Note: keep this function around even if we don't use it so that we can swap to it quickly if need be.
async fn fetch_html_snapshot_cdp(
    twitter_url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
) -> Result<()> {
    let html_path = work_dir.join("raw.html");

    // Try ScreenshotService CDP first (preferred - proper waiting for JS rendering)
    if let Some(screenshot_service) = cookies.screenshot_service {
        match screenshot_service.capture_html(twitter_url).await {
            Ok(html) => {
                if html.trim().is_empty() {
                    warn!("ScreenshotService returned empty HTML for Twitter");
                } else {
                    tokio::fs::write(&html_path, &html)
                        .await
                        .context("Failed to write raw.html")?;
                    debug!(path = %html_path.display(), size = html.len(), "Saved rendered HTML snapshot via CDP");
                    return Ok(());
                }
            }
            Err(e) => {
                warn!(url = %twitter_url, error = %e, "CDP HTML capture failed for Twitter, falling back to dump-dom");
            }
        }
    }

    // Try chromium --dump-dom (may not wait long enough for full JS rendering)
    if let Some(spec) = cookies.browser_profile {
        match fetch_html_with_chromium(twitter_url, work_dir, spec, 60, "twitter html").await {
            Ok(html) => {
                if html.trim().is_empty() {
                    warn!("Chromium dump-dom returned empty HTML for Twitter");
                } else {
                    tokio::fs::write(&html_path, &html)
                        .await
                        .context("Failed to write raw.html")?;
                    debug!(path = %html_path.display(), size = html.len(), "Saved rendered HTML snapshot via chromium dump-dom");
                    return Ok(());
                }
            }
            Err(e) => {
                warn!(url = %twitter_url, error = %e, "Chromium dump-dom failed for Twitter, falling back to HTTP");
            }
        }
    }

    // Fallback to HTTP fetch (won't have JS-rendered content, but better than nothing)
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .context("Failed to build HTTP client")?;

    debug!(url = %twitter_url, "Trying HTTP fetch for Twitter HTML (JS content will be missing)");
    match fetch_html_from_url(&client, twitter_url).await {
        Ok(content) if !content.trim().is_empty() => {
            tokio::fs::write(&html_path, &content)
                .await
                .context("Failed to write raw.html")?;
            debug!(path = %html_path.display(), size = content.len(), "Saved HTML snapshot via HTTP (no JS rendering)");
            Ok(())
        }
        Ok(_) => {
            anyhow::bail!("Twitter returned empty HTML");
        }
        Err(e) => Err(e.context("Failed to fetch HTML from Twitter")),
    }
}

/// Fetch HTML from a URL.
async fn fetch_html_from_url(client: &reqwest::Client, url: &str) -> Result<String> {
    let response = client
        .get(url)
        .header(header::USER_AGENT, ARCHIVAL_USER_AGENT)
        .header(header::ACCEPT, "text/html,application/xhtml+xml")
        .send()
        .await
        .context("Failed to send request")?;

    let status = response.status();
    if status.as_u16() == 429 {
        anyhow::bail!("Rate limited (429)");
    }
    if status.as_u16() == 404 {
        anyhow::bail!("Not found (404)");
    }
    if !status.is_success() {
        anyhow::bail!("HTTP error: {status}");
    }

    response
        .text()
        .await
        .context("Failed to read response body")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle() {
        let handler = TwitterHandler::new();

        assert!(handler.can_handle("https://twitter.com/user/status/123"));
        assert!(handler.can_handle("https://www.twitter.com/user/status/123"));
        assert!(handler.can_handle("https://x.com/user/status/123"));
        assert!(handler.can_handle("https://www.x.com/user/status/123"));
        assert!(handler.can_handle("https://mobile.twitter.com/user/status/123"));
        assert!(handler.can_handle("https://mobile.x.com/user/status/123"));

        assert!(!handler.can_handle("https://example.com/"));
        assert!(!handler.can_handle("https://youtube.com/"));
    }

    #[test]
    fn test_normalize_url() {
        let handler = TwitterHandler::new();

        // All variants should normalize to x.com
        assert!(handler
            .normalize_url("https://twitter.com/user/status/123")
            .contains("x.com"));
        assert!(handler
            .normalize_url("https://www.twitter.com/user/status/123")
            .contains("x.com"));
        assert!(handler
            .normalize_url("https://mobile.twitter.com/user/status/123")
            .contains("x.com"));
        assert!(handler
            .normalize_url("https://www.x.com/user/status/123")
            .contains("x.com"));
        assert!(!handler
            .normalize_url("https://www.x.com/user/status/123")
            .contains("www."));
    }

    #[test]
    fn test_extract_tweet_id() {
        assert_eq!(
            extract_tweet_id("https://x.com/user/status/1234567890"),
            Some("1234567890".to_string())
        );
        assert_eq!(
            extract_tweet_id("https://twitter.com/user/status/9876543210"),
            Some("9876543210".to_string())
        );
        assert_eq!(
            extract_tweet_id("https://x.com/user/status/123?s=20"),
            Some("123".to_string())
        );
        // No tweet ID
        assert_eq!(extract_tweet_id("https://x.com/user"), None);
        assert_eq!(extract_tweet_id("https://x.com/"), None);
    }

    #[test]
    fn test_get_nitter_url() {
        assert_eq!(
            get_nitter_url("https://x.com/user/status/123", "nitter.net"),
            "https://nitter.net/user/status/123"
        );
        assert_eq!(
            get_nitter_url("https://twitter.com/user/status/123", "nitter.net"),
            "https://nitter.net/user/status/123"
        );
        assert_eq!(
            get_nitter_url("https://x.com/user/status/123", "nitter.net/"),
            "https://nitter.net/user/status/123"
        );
    }

    #[test]
    fn test_is_rate_limit_error() {
        // Function handles case-insensitive matching internally
        assert!(is_rate_limit_error("HTTP 429 Too Many Requests"));
        assert!(is_rate_limit_error("Rate Limit Exceeded"));
        assert!(is_rate_limit_error("Too many requests, try again later"));
        assert!(is_rate_limit_error("Service Temporarily Unavailable"));
        assert!(!is_rate_limit_error("404 Not Found"));
        assert!(!is_rate_limit_error("Connection refused"));
    }

    #[test]
    fn test_extract_twitter_metadata_from_json() {
        let json = r#"{
            "tweet_id": 1234567890,
            "author": {"name": "Test User", "screen_name": "testuser", "id": 999},
            "content": "Hello world! This is a test tweet.",
            "date": "2024-01-15T12:00:00",
            "media": [{"type": "photo"}, {"type": "photo"}]
        }"#;

        let metadata = extract_twitter_metadata_from_json(json).unwrap();
        assert_eq!(metadata.tweet_id, Some("1234567890".to_string()));
        assert_eq!(metadata.author, Some("Test User".to_string()));
        assert_eq!(
            metadata.text,
            Some("Hello world! This is a test tweet.".to_string())
        );
        assert_eq!(metadata.media_count, 2);
    }

    #[test]
    fn test_extract_twitter_metadata_with_quoted_tweet() {
        let json = r#"{
            "tweet_id": 1234567890,
            "author": {"name": "Test User", "screen_name": "testuser"},
            "content": "Check this out!",
            "quoted_tweet": {
                "id": 9876543210,
                "user": {"screen_name": "quoteduser"}
            }
        }"#;

        let metadata = extract_twitter_metadata_from_json(json).unwrap();
        assert_eq!(
            metadata.quoted_tweet_url,
            Some("https://x.com/quoteduser/status/9876543210".to_string())
        );
    }

    #[test]
    fn test_detect_nsfw_from_gallerydl_json_possibly_sensitive_true() {
        let json = r#"{"possibly_sensitive": true, "tweet_id": 123}"#;
        let (is_nsfw, source) = detect_nsfw_from_gallerydl_json(json);
        assert_eq!(is_nsfw, Some(true));
        assert_eq!(source, Some("twitter_metadata".to_string()));
    }

    #[test]
    fn test_detect_nsfw_from_gallerydl_json_sensitive_true() {
        let json = r#"{"sensitive": true, "tweet_id": 123}"#;
        let (is_nsfw, source) = detect_nsfw_from_gallerydl_json(json);
        assert_eq!(is_nsfw, Some(true));
        assert_eq!(source, Some("twitter_metadata".to_string()));
    }

    #[test]
    fn test_detect_nsfw_from_gallerydl_json_false() {
        let json = r#"{"possibly_sensitive": false, "tweet_id": 123}"#;
        let (is_nsfw, _) = detect_nsfw_from_gallerydl_json(json);
        assert_eq!(is_nsfw, None);
    }

    #[test]
    fn test_detect_nsfw_from_gallerydl_json_missing_field() {
        let json = r#"{"tweet_id": 123, "author": "test"}"#;
        let (is_nsfw, _) = detect_nsfw_from_gallerydl_json(json);
        assert_eq!(is_nsfw, None);
    }

    #[test]
    fn test_detect_nsfw_from_gallerydl_json_invalid_json() {
        let json = "invalid json {";
        let (is_nsfw, source) = detect_nsfw_from_gallerydl_json(json);
        assert_eq!(is_nsfw, None);
        assert_eq!(source, None);
    }

    #[test]
    fn test_detect_nsfw_from_ytdlp_metadata_age_limit() {
        let json = r#"{"age_limit": 18, "id": "123"}"#;
        let (is_nsfw, source) = detect_nsfw_from_ytdlp_metadata(Some(json));
        assert_eq!(is_nsfw, Some(true));
        assert_eq!(source, Some("metadata".to_string()));
    }

    #[test]
    fn test_detect_nsfw_from_ytdlp_metadata_age_limit_over_18() {
        let json = r#"{"age_limit": 21, "id": "123"}"#;
        let (is_nsfw, source) = detect_nsfw_from_ytdlp_metadata(Some(json));
        assert_eq!(is_nsfw, Some(true));
        assert_eq!(source, Some("metadata".to_string()));
    }

    #[test]
    fn test_detect_nsfw_from_ytdlp_metadata_possibly_sensitive() {
        let json = r#"{"possibly_sensitive": true, "id": "123"}"#;
        let (is_nsfw, source) = detect_nsfw_from_ytdlp_metadata(Some(json));
        assert_eq!(is_nsfw, Some(true));
        assert_eq!(source, Some("twitter_metadata".to_string()));
    }

    #[test]
    fn test_detect_nsfw_from_ytdlp_metadata_none() {
        let (is_nsfw, source) = detect_nsfw_from_ytdlp_metadata(None);
        assert_eq!(is_nsfw, None);
        assert_eq!(source, None);
    }

    #[test]
    fn test_detect_nsfw_from_ytdlp_metadata_age_limit_under_18() {
        let json = r#"{"age_limit": 13, "id": "123"}"#;
        let (is_nsfw, _) = detect_nsfw_from_ytdlp_metadata(Some(json));
        assert_eq!(is_nsfw, None);
    }

    #[test]
    fn test_detect_nsfw_from_html_potentially_sensitive() {
        let html = r#"<div>The following media includes potentially sensitive content</div>"#;
        assert!(detect_nsfw_from_html(html));
    }

    #[test]
    fn test_detect_nsfw_from_html_sensitive_media() {
        let html = r#"<div>This tweet contains sensitive media warning</div>"#;
        assert!(detect_nsfw_from_html(html));
    }

    #[test]
    fn test_detect_nsfw_from_html_data_attribute() {
        let html = r#"<div data-sensitive="true">Content</div>"#;
        assert!(detect_nsfw_from_html(html));
    }

    #[test]
    fn test_detect_nsfw_from_html_css_class() {
        let html = r#"<div class="is-sensitive-media">Content</div>"#;
        assert!(detect_nsfw_from_html(html));
    }

    #[test]
    fn test_detect_nsfw_from_html_sensitivemediawarning() {
        let html = r#"<div class="sensitivemediawarning">Warning</div>"#;
        assert!(detect_nsfw_from_html(html));
    }

    #[test]
    fn test_detect_nsfw_from_html_case_insensitive() {
        let html = r#"<div>POTENTIALLY SENSITIVE CONTENT here</div>"#;
        assert!(detect_nsfw_from_html(html));
    }

    #[test]
    fn test_detect_nsfw_from_html_safe_content() {
        let html = r#"<div>Normal tweet content with no warnings</div>"#;
        assert!(!detect_nsfw_from_html(html));
    }

    #[test]
    fn test_extract_twitter_metadata_with_nsfw_fields() {
        let json = r#"{
            "tweet_id": 1234567890,
            "author": {"name": "Test User"},
            "content": "Test tweet",
            "possibly_sensitive": true
        }"#;

        let metadata = extract_twitter_metadata_from_json(json).unwrap();
        assert_eq!(metadata.possibly_sensitive, Some(true));
        assert_eq!(metadata.sensitive, None);
    }

    #[test]
    fn test_extract_twitter_metadata_with_sensitive_field() {
        let json = r#"{
            "tweet_id": 1234567890,
            "author": {"name": "Test User"},
            "content": "Test tweet",
            "sensitive": true
        }"#;

        let metadata = extract_twitter_metadata_from_json(json).unwrap();
        assert_eq!(metadata.possibly_sensitive, None);
        assert_eq!(metadata.sensitive, Some(true));
    }

    #[test]
    fn test_extract_twitter_metadata_without_nsfw_fields() {
        let json = r#"{
            "tweet_id": 1234567890,
            "author": {"name": "Test User"},
            "content": "Test tweet"
        }"#;

        let metadata = extract_twitter_metadata_from_json(json).unwrap();
        assert_eq!(metadata.possibly_sensitive, None);
        assert_eq!(metadata.sensitive, None);
    }

    #[test]
    fn test_detect_media_in_html_video_player() {
        let html = r#"<div data-testid="videoPlayer">Video content</div>"#;
        assert!(detect_media_in_html(html));
    }

    #[test]
    fn test_detect_media_in_html_video_component() {
        let html = r#"<div data-testid="videoComponent"><video src="test.mp4"></video></div>"#;
        assert!(detect_media_in_html(html));
    }

    #[test]
    fn test_detect_media_in_html_video_tag() {
        let html = r#"<video src="https://video.twimg.com/test.mp4"></video>"#;
        assert!(detect_media_in_html(html));
    }

    #[test]
    fn test_detect_media_in_html_m3u8_stream() {
        let html = r#"<script>var videoUrl = "https://video.twimg.com/player.m3u8";</script>"#;
        assert!(detect_media_in_html(html));
    }

    #[test]
    fn test_detect_media_in_html_tweet_photo() {
        let html = r#"<div data-testid="tweetPhoto"><img src="photo.jpg"></div>"#;
        assert!(detect_media_in_html(html));
    }

    #[test]
    fn test_detect_media_in_html_twimg_media() {
        let html = r#"<img src="https://pbs.twimg.com/media/ABC123.jpg" alt="Tweet image">"#;
        assert!(detect_media_in_html(html));
    }

    #[test]
    fn test_detect_media_in_html_tweet_video() {
        let html = r#"<img src="https://pbs.twimg.com/tweet_video/ABC123.jpg">"#;
        assert!(detect_media_in_html(html));
    }

    #[test]
    fn test_detect_media_in_html_gif() {
        let html = r#"<div data-testid="tweetGif"><img src="animated.gif"></div>"#;
        assert!(detect_media_in_html(html));
    }

    #[test]
    fn test_detect_media_in_html_gif_thumb() {
        let html = r#"<img class="tweet_video_thumb" src="thumb.jpg">"#;
        assert!(detect_media_in_html(html));
    }

    #[test]
    fn test_detect_media_in_html_card_with_media() {
        let html = r#"
            <div data-testid="card.wrapper">
                <div data-testid="card.layoutLarge.media">
                    <img src="https://pbs.twimg.com/card_img/123.jpg">
                </div>
            </div>
        "#;
        assert!(detect_media_in_html(html));
    }

    #[test]
    fn test_detect_media_in_html_case_insensitive() {
        let html = r#"<DIV DATA-TESTID="VIDEOPLAYER">Video</DIV>"#;
        assert!(detect_media_in_html(html));
    }

    #[test]
    fn test_detect_media_in_html_text_only_tweet() {
        let html = r#"
            <article>
                <div>Just a text tweet with no media</div>
                <div>Some profile image: https://pbs.twimg.com/profile_images/123.jpg</div>
            </article>
        "#;
        // Should be false - only has profile image, not tweet media
        assert!(!detect_media_in_html(html));
    }

    #[test]
    fn test_detect_media_in_html_empty() {
        let html = "";
        assert!(!detect_media_in_html(html));
    }

    #[test]
    fn test_detect_media_in_html_no_media_indicators() {
        let html = r#"
            <html>
                <body>
                    <div>Normal tweet content without media</div>
                    <p>Just text and links</p>
                </body>
            </html>
        "#;
        assert!(!detect_media_in_html(html));
    }
}
