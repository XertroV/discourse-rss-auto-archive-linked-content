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
use crate::og_extractor::extract_og_metadata;

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
    pub quoted_tweet_author: Option<String>,
    pub quoted_tweet_text: Option<String>,
    pub quoted_tweet_date: Option<String>,
    pub quoted_tweet_deleted: bool,
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
        // Only save raw.html from CDP (the best method), skip other method-specific HTML files
        let mut html_content: Option<String> = None;
        if config.twitter_html_snapshot {
            match fetch_html_snapshot_cdp_only(&normalized_url, work_dir, cookies).await {
                Ok(()) => {
                    debug!(url = %normalized_url, "Successfully saved raw.html from CDP");
                    // Read raw.html
                    let html_path = work_dir.join("raw.html");
                    if let Ok(html) = tokio::fs::read_to_string(&html_path).await {
                        html_content = Some(html.clone());

                        // Extract og:description as plaintext content (contains tweet text)
                        if let Ok(og_metadata) = extract_og_metadata(&html) {
                            if let Some(description) = og_metadata.description {
                                result.text = Some(description);
                                debug!(url = %normalized_url, "Extracted tweet text from og:description");
                            }
                            // Also use og:title if we don't have a title yet
                            if result.title.is_none() {
                                result.title = og_metadata.title;
                            }
                        }
                    }
                    result.primary_file = Some("raw.html".to_string());
                    result.content_type = "thread".to_string();
                }
                Err(e) => {
                    warn!(url = %normalized_url, error = %e, "Failed to fetch HTML snapshot for Twitter");
                }
            }
        }

        // Step 2: Detect what type of media is in the HTML
        let media_type = html_content
            .as_ref()
            .map(|html| detect_media_type_in_html(html))
            .unwrap_or(TweetMediaType::None);

        // Step 3: Call the appropriate tool based on media type
        // - Videos and GIFs → yt-dlp
        // - Images only → gallery-dl (skip yt-dlp to avoid unnecessary failure)
        if media_type != TweetMediaType::None {
            debug!(url = %normalized_url, media_type = ?media_type, "Media detected in tweet, attempting download");

            match archive_twitter_media(&normalized_url, work_dir, cookies, config, media_type)
                .await
            {
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
                    // Merge extra_files (important for galleries!)
                    if !media_result.extra_files.is_empty() {
                        result.extra_files = media_result.extra_files;
                    }
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

/// Archive Twitter media based on detected media type.
///
/// - Videos and GIFs → yt-dlp (handles video content)
/// - Images only → gallery-dl directly (faster, avoids yt-dlp "No video" error)
/// - Cards → try yt-dlp first, fall back to gallery-dl
async fn archive_twitter_media(
    url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
    config: &crate::config::Config,
    media_type: TweetMediaType,
) -> Result<ArchiveResult> {
    match media_type {
        TweetMediaType::Video | TweetMediaType::Gif => {
            // Use yt-dlp for video and GIF content
            debug!(url = %url, media_type = ?media_type, "Using yt-dlp for video/GIF content");
            archive_with_ytdlp(url, work_dir, cookies, config).await
        }
        TweetMediaType::Images => {
            // Use gallery-dl directly for image-only tweets (skip yt-dlp)
            debug!(url = %url, "Using gallery-dl for image-only tweet");
            archive_with_gallerydl(url, work_dir, cookies).await
        }
        TweetMediaType::Mixed => {
            // Mixed media: both video/GIF AND images - use both tools
            debug!(url = %url, "Mixed media detected, using both yt-dlp and gallery-dl");
            archive_mixed_media(url, work_dir, cookies, config).await
        }
        TweetMediaType::Card => {
            // Cards might be videos or images, try yt-dlp first
            debug!(url = %url, "Card detected, trying yt-dlp first");
            match archive_with_ytdlp(url, work_dir, cookies, config).await {
                Ok(result) => Ok(result),
                Err(e) => {
                    let err_str = e.to_string();
                    if is_rate_limit_error(&err_str) {
                        return Err(e);
                    }
                    debug!("yt-dlp failed for card, trying gallery-dl: {e}");
                    archive_with_gallerydl(url, work_dir, cookies).await
                }
            }
        }
        TweetMediaType::None => {
            // No media - this shouldn't be called but handle gracefully
            anyhow::bail!("No media detected in tweet")
        }
    }
}

/// Archive Twitter content using yt-dlp (for videos and GIFs).
async fn archive_with_ytdlp(
    url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
    config: &crate::config::Config,
) -> Result<ArchiveResult> {
    let mut result = ytdlp::download(url, work_dir, cookies, config, None, None).await?;

    debug!("yt-dlp succeeded for Twitter");

    // Extract tweet ID for deduplication
    if let Some(tweet_id) = extract_tweet_id(url) {
        debug!(tweet_id = %tweet_id, "Extracted Twitter tweet ID");
        result.video_id = Some(format!("twitter_{tweet_id}"));
    }

    // Default to "video" for yt-dlp results (not "thread")
    if result.content_type.is_empty() {
        result.content_type = "video".to_string();
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

    Ok(result)
}

/// Archive Twitter content using gallery-dl (for images).
async fn archive_with_gallerydl(
    url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
) -> Result<ArchiveResult> {
    let mut result = gallerydl::download(url, work_dir, cookies).await?;

    debug!(
        url = %url,
        primary_file = ?result.primary_file,
        extra_files_count = result.extra_files.len(),
        content_type = %result.content_type,
        "gallery-dl succeeded for Twitter"
    );

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

            // Format text with quoted tweet if present
            if result.text.is_none() {
                if metadata.quoted_tweet_url.is_some() {
                    // This is a quote tweet - format with both outer and quoted content
                    result.text =
                        format_quote_tweet_text(&metadata).or_else(|| metadata.text.clone());
                } else {
                    result.text = metadata.text.clone();
                }
            }

            // Store enhanced metadata (but DON'T override content_type from gallerydl)
            // The gallerydl result already has correct content_type based on actual files found
            let enhanced_metadata = serde_json::json!({
                "twitter": metadata,
                "original_metadata": serde_json::from_str::<serde_json::Value>(json_str).ok(),
            });
            result.metadata_json = Some(enhanced_metadata.to_string());
        }
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

/// Archive mixed media tweet (video + images).
///
/// Runs yt-dlp for video first, then gallery-dl for images.
/// Merges results with video as primary_file and images in extra_files.
async fn archive_mixed_media(
    url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
    config: &crate::config::Config,
) -> Result<ArchiveResult> {
    debug!(url = %url, "Archiving mixed media tweet (video + images)");

    // Step 1: Download video with yt-dlp
    let video_result = archive_with_ytdlp(url, work_dir, cookies, config).await?;

    // Step 2: Download images with gallery-dl
    let image_result = match archive_with_gallerydl(url, work_dir, cookies).await {
        Ok(result) => result,
        Err(e) => {
            warn!(url = %url, error = %e, "gallery-dl failed for mixed media, continuing with video only");
            return Ok(ArchiveResult {
                content_type: "video".to_string(),
                ..video_result
            });
        }
    };

    // Step 3: Merge results - video as primary, images in extra_files
    let mut merged = video_result;

    // Add images to extra_files (primary image + any extra images)
    if let Some(ref primary_image) = image_result.primary_file {
        merged.extra_files.push(primary_image.clone());
    }
    merged.extra_files.extend(image_result.extra_files);

    // Set content type to mixed
    merged.content_type = "mixed".to_string();

    // Merge metadata - keep both video and image metadata
    if merged.metadata_json.is_some() && image_result.metadata_json.is_some() {
        if let (Some(video_meta), Some(image_meta)) =
            (&merged.metadata_json, &image_result.metadata_json)
        {
            if let (Ok(v), Ok(i)) = (
                serde_json::from_str::<serde_json::Value>(video_meta),
                serde_json::from_str::<serde_json::Value>(image_meta),
            ) {
                let combined = serde_json::json!({
                    "video_metadata": v,
                    "image_metadata": i,
                });
                merged.metadata_json = Some(combined.to_string());
            }
        }
    }

    debug!(
        url = %url,
        primary_file = ?merged.primary_file,
        extra_files_count = merged.extra_files.len(),
        "Successfully archived mixed media tweet"
    );

    Ok(merged)
}

/// Try to archive via nitter instances.
#[allow(dead_code)]
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
        // Check if the quoted tweet is deleted (tombstone/unavailable)
        let is_deleted = quoted
            .get("deleted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            || quoted
                .get("tombstone")
                .map(|v| !v.is_null())
                .unwrap_or(false)
            || quoted
                .get("unavailable")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

        metadata.quoted_tweet_deleted = is_deleted;

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
                metadata.quoted_tweet_author = Some(quoted_user.to_string());
            }
        }

        // Extract quoted tweet text
        metadata.quoted_tweet_text = quoted
            .get("content")
            .or_else(|| quoted.get("text"))
            .or_else(|| quoted.get("full_text"))
            .and_then(|v| v.as_str())
            .map(String::from);

        // Extract quoted tweet date
        metadata.quoted_tweet_date = quoted
            .get("date")
            .or_else(|| quoted.get("created_at"))
            .and_then(|v| v.as_str())
            .map(String::from);
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

/// Detect if a quoted tweet has been deleted from HTML snapshot.
///
/// Looks for indicators that a quoted tweet is unavailable or deleted.
#[allow(dead_code)] // Reserved for future HTML-based detection
fn detect_deleted_quoted_tweet_from_html(html: &str) -> bool {
    let html_lower = html.to_ascii_lowercase();

    // Look for deleted/unavailable tweet indicators
    html_lower.contains("this post is unavailable")
        || html_lower.contains("this tweet is unavailable")
        || html_lower.contains("this post was deleted")
        || html_lower.contains("this tweet was deleted")
        || html_lower.contains("tweet unavailable")
        || html_lower.contains("post unavailable")
        || html_lower.contains("tweet-tombstone")
        || html_lower.contains("tombstone")
}

/// Format combined tweet text for quote tweets.
///
/// Creates a markdown-formatted string showing both the outer tweet and quoted tweet:
/// ```text
/// [outer tweet content]
///
/// @person - date
/// > [quoted tweet content]
/// ```
fn format_quote_tweet_text(metadata: &TwitterMetadata) -> Option<String> {
    // Need both outer tweet text and quoted tweet info
    let outer_text = metadata.text.as_ref()?;
    let quoted_author = metadata.quoted_tweet_author.as_ref()?;

    let mut result = outer_text.clone();
    result.push_str("\n\n");
    result.push('@');
    result.push_str(quoted_author);

    // Add date if available
    if let Some(ref date) = metadata.quoted_tweet_date {
        result.push_str(" - ");
        result.push_str(date);
    }

    result.push('\n');

    // Add quoted tweet content or deletion notice
    if metadata.quoted_tweet_deleted {
        result.push_str("> [This tweet has been deleted]");
    } else if let Some(ref quoted_text) = metadata.quoted_tweet_text {
        // Format as markdown blockquote - add > to each line
        for line in quoted_text.lines() {
            result.push_str("> ");
            result.push_str(line);
            result.push('\n');
        }
        // Remove trailing newline
        result.pop();
    } else {
        // Have URL but no text - tweet might be unavailable
        result.push_str("> [Quoted tweet content unavailable]");
    }

    Some(result)
}

/// Type of media detected in tweet HTML.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TweetMediaType {
    /// No media detected (text-only tweet)
    None,
    /// Video content (use yt-dlp)
    Video,
    /// GIF content (use yt-dlp - Twitter GIFs are actually videos)
    Gif,
    /// Image(s) only, no video (use gallery-dl)
    Images,
    /// Card with media preview
    Card,
    /// Mixed media: both video/GIF AND images (use both tools)
    Mixed,
}

/// Detect the type of media in Twitter HTML.
///
/// This is used to decide whether to invoke yt-dlp or gallery-dl for downloading.
/// - Videos and GIFs → yt-dlp
/// - Images only → gallery-dl (faster, no unnecessary yt-dlp failure)
/// - Text-only tweets → skip both tools
fn detect_media_type_in_html(html: &str) -> TweetMediaType {
    let html_lower = html.to_ascii_lowercase();

    // Video indicators
    let has_video = html_lower.contains("data-testid=\"videoplayer\"")
        || html_lower.contains("data-testid=\"videocomponent\"")
        || html_lower.contains("<video")
        || html_lower.contains("player.m3u8")
        || html_lower.contains("ext_tw_video")
        || html_lower.contains("amplify_video");

    // GIF indicators - Twitter GIFs are actually MP4 videos
    // pbs.twimg.com/tweet_video/ is used for GIF thumbnails
    let has_gif = html_lower.contains("data-testid=\"tweetgif\"")
        || html_lower.contains("tweet_video_thumb")
        || html_lower.contains("pbs.twimg.com/tweet_video");

    // Image indicators (excluding profile pictures and icons)
    // Twitter uses data-testid="tweetPhoto" for tweet images
    let has_images = html_lower.contains("data-testid=\"tweetphoto\"")
        || html_lower.contains("pbs.twimg.com/media/");

    // Check for mixed media first (video/GIF AND images)
    if (has_video || has_gif) && has_images {
        return TweetMediaType::Mixed;
    }

    if has_video {
        return TweetMediaType::Video;
    }

    if has_gif {
        return TweetMediaType::Gif;
    }

    if has_images {
        return TweetMediaType::Images;
    }

    // Card with media (preview cards with images/videos)
    let has_media_card = html_lower.contains("data-testid=\"card.wrapper\"")
        && (html_lower.contains("pbs.twimg.com/card_img/")
            || html_lower.contains("data-testid=\"card.layoutlarge.media\""));

    if has_media_card {
        return TweetMediaType::Card;
    }

    TweetMediaType::None
}

/// Detect if the Twitter HTML contains media (video, images, or GIFs).
///
/// This is used to decide whether to invoke yt-dlp/gallery-dl for downloading.
/// For text-only tweets, we skip these tools to avoid unnecessary API calls.
#[cfg(test)]
fn detect_media_in_html(html: &str) -> bool {
    detect_media_type_in_html(html) != TweetMediaType::None
}

/// Remove `<noscript>` tags and their contents from HTML.
///
/// Twitter/X pages include a `<noscript>` section that shows a "JavaScript is required"
/// message. Since we're capturing the rendered page (which has JS enabled), this
/// noscript content is misleading and should be removed.
fn strip_noscript_tags(html: &str) -> String {
    // Use regex to remove <noscript>...</noscript> tags and their contents
    // This handles both single-line and multi-line noscript blocks
    static NOSCRIPT_PATTERN: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"(?is)<noscript[^>]*>.*?</noscript>").unwrap());
    NOSCRIPT_PATTERN.replace_all(html, "").into_owned()
}

/// Remove `<script>` tags and their contents from HTML.
///
/// Scripts don't execute in sandboxed iframes used for viewing archives,
/// but they add unnecessary bloat. Removing them keeps the archived HTML
/// clean and reduces file size.
fn strip_script_tags(html: &str) -> String {
    static SCRIPT_PATTERN: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap());
    SCRIPT_PATTERN.replace_all(html, "").into_owned()
}

/// Clean HTML for archiving by stripping noscript and script tags.
fn clean_html_for_archive(html: &str) -> String {
    let html = strip_noscript_tags(html);
    strip_script_tags(&html)
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

/// Fetch HTML snapshot using CDP (Chrome DevTools Protocol) via ScreenshotService.
///
/// This is the preferred method as it properly waits for JavaScript rendering.
/// Strips noscript tags from the output.
#[allow(dead_code)]
async fn fetch_html_snapshot_cdp(
    twitter_url: &str,
    output_path: &Path,
    cookies: &CookieOptions<'_>,
) -> Result<()> {
    let screenshot_service = cookies
        .screenshot_service
        .context("ScreenshotService not available for CDP HTML capture")?;

    let html = screenshot_service
        .capture_html(twitter_url)
        .await
        .context("CDP HTML capture failed")?;

    if html.trim().is_empty() {
        anyhow::bail!("CDP returned empty HTML for Twitter");
    }

    // Clean HTML for archiving (strip scripts and noscript tags)
    let html = clean_html_for_archive(&html);

    tokio::fs::write(output_path, &html)
        .await
        .context("Failed to write HTML file")?;

    debug!(path = %output_path.display(), size = html.len(), "Saved HTML snapshot via CDP");
    Ok(())
}

/// Fetch HTML snapshot using Chromium's --dump-dom CLI flag.
///
/// Uses a cloned Chromium profile for cookies. May not wait long enough for
/// full JS rendering on complex pages.
/// Strips noscript tags from the output.
#[allow(dead_code)]
async fn fetch_html_snapshot_dump_dom(
    twitter_url: &str,
    work_dir: &Path,
    output_path: &Path,
    cookies: &CookieOptions<'_>,
) -> Result<()> {
    let spec = cookies
        .browser_profile
        .context("Browser profile not available for dump-dom HTML capture")?;

    let html = fetch_html_with_chromium(twitter_url, work_dir, spec, 60, "twitter html")
        .await
        .context("Chromium dump-dom failed")?;

    if html.trim().is_empty() {
        anyhow::bail!("Chromium dump-dom returned empty HTML for Twitter");
    }

    // Clean HTML for archiving (strip scripts and noscript tags)
    let html = clean_html_for_archive(&html);

    tokio::fs::write(output_path, &html)
        .await
        .context("Failed to write HTML file")?;

    debug!(path = %output_path.display(), size = html.len(), "Saved HTML snapshot via dump-dom");
    Ok(())
}

/// Fetch HTML snapshot using plain HTTP GET (no JavaScript rendering).
///
/// This is the simplest method but won't include dynamically rendered content.
/// Strips noscript tags from the output.
#[allow(dead_code)]
async fn fetch_html_snapshot_http(twitter_url: &str, output_path: &Path) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .context("Failed to build HTTP client")?;

    let html = fetch_html_from_url(&client, twitter_url)
        .await
        .context("HTTP fetch failed")?;

    if html.trim().is_empty() {
        anyhow::bail!("HTTP fetch returned empty HTML for Twitter");
    }

    // Clean HTML for archiving (strip scripts and noscript tags)
    let html = clean_html_for_archive(&html);

    tokio::fs::write(output_path, &html)
        .await
        .context("Failed to write HTML file")?;

    debug!(path = %output_path.display(), size = html.len(), "Saved HTML snapshot via HTTP");
    Ok(())
}

/// Fetch HTML snapshot using HTTP with cookies from cookies.txt file.
///
/// Uses the Netscape cookie format file to add authentication cookies.
/// May help with rate limiting or accessing protected content.
/// Strips noscript tags from the output.
#[allow(dead_code)]
async fn fetch_html_snapshot_http_with_cookies(
    twitter_url: &str,
    output_path: &Path,
    cookies: &CookieOptions<'_>,
) -> Result<()> {
    let html = fetch_tweet_html(twitter_url, cookies)
        .await
        .context("HTTP with cookies fetch failed")?;

    if html.trim().is_empty() {
        anyhow::bail!("HTTP with cookies returned empty HTML for Twitter");
    }

    // Clean HTML for archiving (strip scripts and noscript tags)
    let html = clean_html_for_archive(&html);

    tokio::fs::write(output_path, &html)
        .await
        .context("Failed to write HTML file")?;

    debug!(path = %output_path.display(), size = html.len(), "Saved HTML snapshot via HTTP with cookies");
    Ok(())
}

/// Fetch HTML snapshot using CDP only and save directly to raw.html.
///
/// This is the preferred method for Twitter as it properly waits for JavaScript rendering.
/// Only saves raw.html - no method-specific files.
async fn fetch_html_snapshot_cdp_only(
    twitter_url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
) -> Result<()> {
    let raw_html_path = work_dir.join("raw.html");

    // Try CDP method (best for JavaScript-heavy sites like Twitter)
    if cookies.screenshot_service.is_some() {
        match fetch_html_snapshot_cdp(twitter_url, &raw_html_path, cookies).await {
            Ok(()) => {
                info!(url = %twitter_url, "CDP HTML snapshot saved to raw.html");
                return Ok(());
            }
            Err(e) => {
                warn!(url = %twitter_url, error = %e, "CDP HTML snapshot failed, trying fallback");
            }
        }
    } else {
        debug!(url = %twitter_url, "CDP method not available - screenshot service not configured");
    }

    // Fallback to dump-dom if CDP is not available
    if cookies.browser_profile.is_some() {
        match fetch_html_snapshot_dump_dom(twitter_url, work_dir, &raw_html_path, cookies).await {
            Ok(()) => {
                info!(url = %twitter_url, "Dump-dom HTML snapshot saved to raw.html (fallback)");
                return Ok(());
            }
            Err(e) => {
                warn!(url = %twitter_url, error = %e, "Dump-dom HTML snapshot also failed");
            }
        }
    }

    // Last resort: HTTP with cookies (won't have JS-rendered content but better than nothing)
    if cookies.cookies_file.is_some() {
        match fetch_html_snapshot_http_with_cookies(twitter_url, &raw_html_path, cookies).await {
            Ok(()) => {
                info!(url = %twitter_url, "HTTP with cookies HTML snapshot saved to raw.html (fallback)");
                return Ok(());
            }
            Err(e) => {
                warn!(url = %twitter_url, error = %e, "HTTP with cookies HTML snapshot also failed");
            }
        }
    }

    anyhow::bail!("All HTML snapshot methods failed for Twitter URL: {twitter_url}");
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

    #[test]
    fn test_detect_media_type_mixed_video_and_images() {
        // Tweet with both video player and images
        let html = r#"
            <article>
                <div data-testid="videoPlayer">Video content</div>
                <div data-testid="tweetPhoto"><img src="photo.jpg"></div>
            </article>
        "#;
        assert_eq!(detect_media_type_in_html(html), TweetMediaType::Mixed);
    }

    #[test]
    fn test_detect_media_type_mixed_video_and_twimg_media() {
        // Tweet with video and pbs.twimg.com/media/ images
        let html = r#"
            <article>
                <div data-testid="videoPlayer">Video content</div>
                <img src="https://pbs.twimg.com/media/ABC123.jpg">
            </article>
        "#;
        assert_eq!(detect_media_type_in_html(html), TweetMediaType::Mixed);
    }

    #[test]
    fn test_detect_media_type_video_only_no_images() {
        let html = r#"<div data-testid="videoPlayer">Video only</div>"#;
        assert_eq!(detect_media_type_in_html(html), TweetMediaType::Video);
    }

    #[test]
    fn test_detect_media_type_images_only_no_video() {
        let html = r#"<div data-testid="tweetPhoto"><img src="photo.jpg"></div>"#;
        assert_eq!(detect_media_type_in_html(html), TweetMediaType::Images);
    }

    #[test]
    fn test_detect_media_type_gif_and_images_is_mixed() {
        // GIF (tweet_video) plus images should be Mixed
        let html = r#"
            <article>
                <img src="https://pbs.twimg.com/tweet_video/ABC123.jpg">
                <div data-testid="tweetPhoto"><img src="photo.jpg"></div>
            </article>
        "#;
        assert_eq!(detect_media_type_in_html(html), TweetMediaType::Mixed);
    }

    #[test]
    fn test_strip_noscript_tags_simple() {
        let html = r#"<html><body><noscript>JavaScript required</noscript><div>Content</div></body></html>"#;
        let result = strip_noscript_tags(html);
        assert!(!result.contains("<noscript"));
        assert!(!result.contains("JavaScript required"));
        assert!(result.contains("<div>Content</div>"));
    }

    #[test]
    fn test_strip_noscript_tags_multiline() {
        let html = r#"<html>
<body>
<noscript>
    <div>JavaScript is required to view this page</div>
    <p>Please enable JavaScript</p>
</noscript>
<div>Actual content</div>
</body>
</html>"#;
        let result = strip_noscript_tags(html);
        assert!(!result.contains("<noscript"));
        assert!(!result.contains("JavaScript is required"));
        assert!(result.contains("Actual content"));
    }

    #[test]
    fn test_strip_noscript_tags_multiple() {
        let html = r#"<html><noscript>First</noscript><div>Middle</div><noscript>Second</noscript></html>"#;
        let result = strip_noscript_tags(html);
        assert!(!result.contains("First"));
        assert!(!result.contains("Second"));
        assert!(result.contains("Middle"));
    }

    #[test]
    fn test_strip_noscript_tags_with_attributes() {
        let html = r#"<noscript class="js-warning" id="noscript-msg">Enable JS</noscript><div>Content</div>"#;
        let result = strip_noscript_tags(html);
        assert!(!result.contains("<noscript"));
        assert!(!result.contains("Enable JS"));
        assert!(result.contains("<div>Content</div>"));
    }

    #[test]
    fn test_strip_noscript_tags_none_present() {
        let html = r#"<html><body><div>No noscript here</div></body></html>"#;
        let result = strip_noscript_tags(html);
        assert_eq!(result, html);
    }

    #[test]
    fn test_strip_noscript_tags_case_insensitive() {
        let html =
            r#"<NOSCRIPT>Upper case</NOSCRIPT><NoScript>Mixed case</NoScript><div>Content</div>"#;
        let result = strip_noscript_tags(html);
        assert!(!result.contains("Upper case"));
        assert!(!result.contains("Mixed case"));
        assert!(result.contains("<div>Content</div>"));
    }

    #[test]
    fn test_strip_script_tags_simple() {
        let html = r#"<html><head><script>alert('hi')</script></head><body>Content</body></html>"#;
        let result = strip_script_tags(html);
        assert!(!result.contains("<script"));
        assert!(!result.contains("alert"));
        assert!(result.contains("Content"));
    }

    #[test]
    fn test_strip_script_tags_with_src() {
        let html = r#"<html><script src="app.js"></script><div>Content</div></html>"#;
        let result = strip_script_tags(html);
        assert!(!result.contains("<script"));
        assert!(!result.contains("app.js"));
        assert!(result.contains("<div>Content</div>"));
    }

    #[test]
    fn test_strip_script_tags_multiline() {
        let html = r#"<html>
<script type="text/javascript">
    function foo() {
        console.log('test');
    }
</script>
<div>Content</div>
</html>"#;
        let result = strip_script_tags(html);
        assert!(!result.contains("<script"));
        assert!(!result.contains("function foo"));
        assert!(result.contains("Content"));
    }

    #[test]
    fn test_strip_script_tags_multiple() {
        let html = r#"<script>first()</script><div>Middle</div><script>second()</script>"#;
        let result = strip_script_tags(html);
        assert!(!result.contains("first"));
        assert!(!result.contains("second"));
        assert!(result.contains("Middle"));
    }

    #[test]
    fn test_strip_script_tags_case_insensitive() {
        let html = r#"<SCRIPT>upper()</SCRIPT><Script>mixed()</Script><div>Content</div>"#;
        let result = strip_script_tags(html);
        assert!(!result.contains("upper"));
        assert!(!result.contains("mixed"));
        assert!(result.contains("<div>Content</div>"));
    }

    #[test]
    fn test_strip_script_tags_none_present() {
        let html = r#"<html><body><div>No scripts here</div></body></html>"#;
        let result = strip_script_tags(html);
        assert_eq!(result, html);
    }

    #[test]
    fn test_clean_html_for_archive_strips_both() {
        let html = r#"<html>
<noscript>JS required</noscript>
<script>alert('hi')</script>
<div>Real content</div>
</html>"#;
        let result = clean_html_for_archive(html);
        assert!(!result.contains("<noscript"));
        assert!(!result.contains("JS required"));
        assert!(!result.contains("<script"));
        assert!(!result.contains("alert"));
        assert!(result.contains("Real content"));
    }

    #[test]
    fn test_detect_deleted_quoted_tweet_from_html_unavailable() {
        let html = r#"<div>This tweet is unavailable</div>"#;
        assert!(detect_deleted_quoted_tweet_from_html(html));
    }

    #[test]
    fn test_detect_deleted_quoted_tweet_from_html_deleted() {
        let html = r#"<div>This post was deleted by the author</div>"#;
        assert!(detect_deleted_quoted_tweet_from_html(html));
    }

    #[test]
    fn test_detect_deleted_quoted_tweet_from_html_tombstone() {
        let html = r#"<div class="tweet-tombstone">Content removed</div>"#;
        assert!(detect_deleted_quoted_tweet_from_html(html));
    }

    #[test]
    fn test_detect_deleted_quoted_tweet_from_html_normal() {
        let html = r#"<div>Normal tweet content</div>"#;
        assert!(!detect_deleted_quoted_tweet_from_html(html));
    }

    #[test]
    fn test_format_quote_tweet_text_basic() {
        let metadata = TwitterMetadata {
            text: Some("Check out this tweet!".to_string()),
            quoted_tweet_author: Some("testuser".to_string()),
            quoted_tweet_text: Some("This is the quoted tweet".to_string()),
            quoted_tweet_date: Some("2024-01-15T12:00:00".to_string()),
            quoted_tweet_url: Some("https://x.com/testuser/status/123".to_string()),
            quoted_tweet_deleted: false,
            ..Default::default()
        };

        let result = format_quote_tweet_text(&metadata).unwrap();
        assert!(result.contains("Check out this tweet!"));
        assert!(result.contains("@testuser - 2024-01-15T12:00:00"));
        assert!(result.contains("> This is the quoted tweet"));
    }

    #[test]
    fn test_format_quote_tweet_text_deleted() {
        let metadata = TwitterMetadata {
            text: Some("Quoting a deleted tweet".to_string()),
            quoted_tweet_author: Some("deleteduser".to_string()),
            quoted_tweet_date: Some("2024-01-10T10:00:00".to_string()),
            quoted_tweet_url: Some("https://x.com/deleteduser/status/456".to_string()),
            quoted_tweet_deleted: true,
            ..Default::default()
        };

        let result = format_quote_tweet_text(&metadata).unwrap();
        assert!(result.contains("Quoting a deleted tweet"));
        assert!(result.contains("@deleteduser - 2024-01-10T10:00:00"));
        assert!(result.contains("> [This tweet has been deleted]"));
    }

    #[test]
    fn test_format_quote_tweet_text_multiline() {
        let metadata = TwitterMetadata {
            text: Some("Check this out".to_string()),
            quoted_tweet_author: Some("user".to_string()),
            quoted_tweet_text: Some("Line 1\nLine 2\nLine 3".to_string()),
            quoted_tweet_url: Some("https://x.com/user/status/789".to_string()),
            quoted_tweet_deleted: false,
            ..Default::default()
        };

        let result = format_quote_tweet_text(&metadata).unwrap();
        assert!(result.contains("> Line 1\n> Line 2\n> Line 3"));
    }

    #[test]
    fn test_format_quote_tweet_text_no_date() {
        let metadata = TwitterMetadata {
            text: Some("Outer tweet".to_string()),
            quoted_tweet_author: Some("user".to_string()),
            quoted_tweet_text: Some("Quoted content".to_string()),
            quoted_tweet_url: Some("https://x.com/user/status/111".to_string()),
            quoted_tweet_deleted: false,
            ..Default::default()
        };

        let result = format_quote_tweet_text(&metadata).unwrap();
        assert!(result.contains("@user\n>"));
        assert!(!result.contains(" - "));
    }

    #[test]
    fn test_format_quote_tweet_text_unavailable_content() {
        let metadata = TwitterMetadata {
            text: Some("Outer tweet".to_string()),
            quoted_tweet_author: Some("user".to_string()),
            quoted_tweet_url: Some("https://x.com/user/status/222".to_string()),
            quoted_tweet_deleted: false,
            // No quoted_tweet_text
            ..Default::default()
        };

        let result = format_quote_tweet_text(&metadata).unwrap();
        assert!(result.contains("> [Quoted tweet content unavailable]"));
    }

    #[test]
    fn test_format_quote_tweet_text_missing_required_fields() {
        // Missing outer text
        let metadata1 = TwitterMetadata {
            quoted_tweet_author: Some("user".to_string()),
            quoted_tweet_text: Some("Quoted".to_string()),
            ..Default::default()
        };
        assert_eq!(format_quote_tweet_text(&metadata1), None);

        // Missing quoted author
        let metadata2 = TwitterMetadata {
            text: Some("Outer".to_string()),
            quoted_tweet_text: Some("Quoted".to_string()),
            ..Default::default()
        };
        assert_eq!(format_quote_tweet_text(&metadata2), None);
    }

    #[test]
    fn test_extract_twitter_metadata_with_deleted_quoted_tweet() {
        let json = r#"{
            "tweet_id": 1234567890,
            "author": {"name": "Test User", "screen_name": "testuser"},
            "content": "This tweet quotes a deleted one",
            "quoted_tweet": {
                "id": 9876543210,
                "user": {"screen_name": "deleteduser"},
                "deleted": true
            }
        }"#;

        let metadata = extract_twitter_metadata_from_json(json).unwrap();
        assert_eq!(metadata.quoted_tweet_deleted, true);
        assert_eq!(
            metadata.quoted_tweet_url,
            Some("https://x.com/deleteduser/status/9876543210".to_string())
        );
        assert_eq!(
            metadata.quoted_tweet_author,
            Some("deleteduser".to_string())
        );
    }

    #[test]
    fn test_extract_twitter_metadata_with_tombstone_quoted_tweet() {
        let json = r#"{
            "tweet_id": 1234567890,
            "author": {"name": "Test User", "screen_name": "testuser"},
            "content": "This tweet quotes an unavailable one",
            "quoted_tweet": {
                "id": 9876543210,
                "user": {"screen_name": "unavailableuser"},
                "tombstone": {"text": "This Tweet is unavailable"}
            }
        }"#;

        let metadata = extract_twitter_metadata_from_json(json).unwrap();
        assert_eq!(metadata.quoted_tweet_deleted, true);
    }

    #[test]
    fn test_extract_twitter_metadata_with_available_quoted_tweet() {
        let json = r#"{
            "tweet_id": 1234567890,
            "author": {"name": "Test User", "screen_name": "testuser"},
            "content": "Check this out!",
            "quoted_tweet": {
                "id": 9876543210,
                "user": {"screen_name": "quoteduser"},
                "content": "Original quoted content",
                "date": "2024-01-15T12:00:00"
            }
        }"#;

        let metadata = extract_twitter_metadata_from_json(json).unwrap();
        assert_eq!(metadata.quoted_tweet_deleted, false);
        assert_eq!(
            metadata.quoted_tweet_text,
            Some("Original quoted content".to_string())
        );
        assert_eq!(
            metadata.quoted_tweet_date,
            Some("2024-01-15T12:00:00".to_string())
        );
        assert_eq!(metadata.quoted_tweet_author, Some("quoteduser".to_string()));
    }
}
