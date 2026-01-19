use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use reqwest::header::COOKIE;
use scraper::{Html, Selector};
use tracing::{debug, info, warn};

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::{ytdlp, CookieOptions};
use crate::constants::ARCHIVAL_USER_AGENT;

static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        Regex::new(r"^https?://(www\.)?reddit\.com/").unwrap(),
        Regex::new(r"^https?://old\.reddit\.com/").unwrap(),
        Regex::new(r"^https?://m\.reddit\.com/").unwrap(),
        Regex::new(r"^https?://new\.reddit\.com/").unwrap(),
        Regex::new(r"^https?://redd\.it/").unwrap(),
        Regex::new(r"^https?://i\.redd\.it/").unwrap(),
        Regex::new(r"^https?://v\.redd\.it/").unwrap(),
        Regex::new(r"^https?://preview\.redd\.it/").unwrap(),
    ]
});

static SHORTLINK_PATTERN: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^https?://redd\.it/[a-zA-Z0-9]+$").unwrap());

/// Pattern to extract subreddit name from URL.
static SUBREDDIT_PATTERN: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"/r/([a-zA-Z0-9_]+)").unwrap());

/// Known NSFW subreddit name patterns (case-insensitive prefixes/patterns).
const NSFW_SUBREDDIT_PATTERNS: &[&str] = &[
    "nsfw",
    "gonewild",
    "porn",
    "xxx",
    "nude",
    "sex",
    "adult",
    "18_plus",
    "onlyfans",
    "lewd",
    "hentai",
    "rule34",
    "celebnsfw",
    "realgirls",
    "boobs",
    "ass",
    "tits",
];

/// Extracted media from Reddit page.
#[derive(Debug, Default)]
struct RedditMedia {
    video_url: Option<String>,
    image_urls: Vec<String>,
    thumbnail_url: Option<String>,
}

pub struct RedditHandler;

impl RedditHandler {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for RedditHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SiteHandler for RedditHandler {
    fn site_id(&self) -> &'static str {
        "reddit"
    }

    fn url_patterns(&self) -> &[Regex] {
        &PATTERNS
    }

    fn priority(&self) -> i32 {
        100
    }

    fn normalize_url(&self, url: &str) -> String {
        let mut normalized = url.to_string();

        // Convert various reddit domains to old.reddit.com
        normalized = normalized.replace("://www.reddit.com/", "://old.reddit.com/");
        normalized = normalized.replace("://m.reddit.com/", "://old.reddit.com/");
        normalized = normalized.replace("://new.reddit.com/", "://old.reddit.com/");
        normalized = normalized.replace("://reddit.com/", "://old.reddit.com/");

        // Apply base normalization
        super::normalize::normalize_url(&normalized)
    }

    async fn archive(
        &self,
        url: &str,
        work_dir: &Path,
        cookies: &CookieOptions<'_>,
    ) -> Result<ArchiveResult> {
        // Resolve redd.it shortlinks first
        let resolved_url = if is_shortlink(url) {
            match resolve_short_url(url).await {
                Ok(resolved) => {
                    debug!(original = %url, resolved = %resolved, "Resolved redd.it shortlink");
                    resolved
                }
                Err(e) => {
                    debug!("Failed to resolve shortlink, using original URL: {e}");
                    url.to_string()
                }
            }
        } else {
            url.to_string()
        };

        // Normalize URL
        let normalized_url = self.normalize_url(&resolved_url);

        // Follow redirects to get final canonical URL
        let final_url = match follow_redirects(&normalized_url).await {
            Ok(final_url) if final_url != normalized_url => {
                debug!(
                    normalized = %normalized_url,
                    final_url = %final_url,
                    "Followed redirect to final URL"
                );
                Some(final_url.clone())
            }
            Ok(_) => {
                debug!("No redirect, using normalized URL");
                None
            }
            Err(e) => {
                debug!("Failed to follow redirects: {e}, using normalized URL");
                None
            }
        };

        // Use final URL for archiving if available, otherwise use normalized URL
        let archive_url = final_url.as_ref().unwrap_or(&normalized_url);

        // Check if the subreddit name suggests NSFW content
        let subreddit_nsfw = is_nsfw_subreddit(archive_url);

        // ALWAYS fetch HTML - this is required for Reddit archives
        // Use cookies if available for logged-in access
        let html_result = fetch_reddit_html(archive_url, work_dir, cookies).await;

        // Extract media URLs from HTML if successful
        let (html_content, media) = match &html_result {
            Ok((html, media)) => (Some(html.clone()), Some(media.clone())),
            Err(e) => {
                warn!("Failed to fetch Reddit HTML: {e}");
                (None, None)
            }
        };

        // Try yt-dlp for video/media content (only if we detected video)
        let has_video = media
            .as_ref()
            .map(|m| m.video_url.is_some())
            .unwrap_or(false);
        let ytdlp_result = if has_video {
            match ytdlp::download(archive_url, work_dir, cookies).await {
                Ok(result) => Some(result),
                Err(e) => {
                    debug!("yt-dlp failed for Reddit video: {e}");
                    None
                }
            }
        } else {
            // Try yt-dlp anyway in case there's embedded media we didn't detect
            match ytdlp::download(archive_url, work_dir, cookies).await {
                Ok(result) => Some(result),
                Err(e) => {
                    debug!("yt-dlp found no media: {e}");
                    None
                }
            }
        };

        // We MUST have HTML content - fail if we don't
        if html_content.is_none() {
            anyhow::bail!(
                "Failed to archive Reddit page: could not fetch HTML content. {}",
                html_result.err().map(|e| e.to_string()).unwrap_or_default()
            );
        }

        // Build the result
        let mut result = if let Some(ytdlp_data) = ytdlp_result {
            // Got video/media via yt-dlp
            ytdlp_data
        } else {
            // No media, just HTML
            ArchiveResult {
                content_type: "thread".to_string(),
                primary_file: Some("raw.html".to_string()),
                ..Default::default()
            }
        };

        // Extract metadata from HTML
        if let Some(ref html) = html_content {
            let doc = Html::parse_document(html);
            if result.title.is_none() {
                result.title = extract_title_from_html(&doc);
            }
            if result.author.is_none() {
                result.author = extract_author_from_html(&doc);
            }
        }

        // Add detected media URLs to extra_files for downloading
        if let Some(ref media) = media {
            // Store image URLs in metadata for later download
            if !media.image_urls.is_empty() {
                let metadata = serde_json::json!({
                    "detected_images": media.image_urls,
                    "detected_video": media.video_url,
                    "detected_thumbnail": media.thumbnail_url,
                });
                result.metadata_json = Some(metadata.to_string());
            }
        }

        // Set NSFW status from subreddit detection or HTML content
        if result.is_nsfw.is_none() && subreddit_nsfw {
            result.is_nsfw = Some(true);
            result.nsfw_source = Some("subreddit".to_string());
        }

        // Check HTML for NSFW indicators
        if result.is_nsfw.is_none() {
            if let Some(ref html) = html_content {
                if html.contains("over18") || html.contains("nsfw") || html.contains("NSFW") {
                    result.is_nsfw = Some(true);
                    result.nsfw_source = Some("html".to_string());
                }
            }
        }

        // Set final URL if it's different from the normalized URL
        result.final_url = final_url.clone();

        Ok(result)
    }
}

/// Fetch Reddit HTML page and extract media URLs.
///
/// Returns the HTML content and detected media information.
/// Uses cookies if available for authenticated access.
async fn fetch_reddit_html(
    url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
) -> Result<(String, RedditMedia)> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("Failed to build HTTP client")?;

    // Build request with optional cookies
    let mut request = client.get(url).header("User-Agent", ARCHIVAL_USER_AGENT);

    // Add cookies if available
    if let Some(cookie_header) = build_cookie_header(cookies, "reddit.com") {
        info!("Using cookies for Reddit request");
        request = request.header(COOKIE, cookie_header);
    }

    let response = request
        .send()
        .await
        .context("Failed to fetch Reddit HTML")?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("Reddit returned HTTP status {}", status);
    }

    let html = response
        .text()
        .await
        .context("Failed to read response body")?;

    // Save raw HTML
    let html_path = work_dir.join("raw.html");
    tokio::fs::write(&html_path, &html)
        .await
        .context("Failed to write HTML file")?;

    // Parse HTML and extract media URLs
    let doc = Html::parse_document(&html);
    let media = extract_media_from_html(&doc);

    debug!(
        video = ?media.video_url,
        images = media.image_urls.len(),
        "Extracted media from Reddit HTML"
    );

    Ok((html, media))
}

/// Build a cookie header string from CookieOptions for a specific domain.
///
/// Reads cookies from the Netscape format cookies file if available.
fn build_cookie_header(cookies: &CookieOptions<'_>, domain: &str) -> Option<String> {
    // Try to read cookies from file
    let cookies_path = cookies.cookies_file?;

    if !cookies_path.exists() {
        return None;
    }

    // Read and parse Netscape format cookies file
    let content = std::fs::read_to_string(cookies_path).ok()?;
    let mut cookie_pairs = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Netscape format: domain, include_subdomains, path, secure, expires, name, value
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 7 {
            continue;
        }

        let cookie_domain = fields[0].trim_start_matches('.');
        let name = fields[5];
        let value = fields[6];

        // Check if cookie applies to this domain
        if cookie_domain == domain
            || cookie_domain.ends_with(&format!(".{domain}"))
            || domain.ends_with(&format!(".{cookie_domain}"))
            || domain.ends_with(cookie_domain)
        {
            cookie_pairs.push(format!("{name}={value}"));
        }
    }

    if cookie_pairs.is_empty() {
        None
    } else {
        debug!(
            count = cookie_pairs.len(),
            domain = %domain,
            "Loaded cookies for domain"
        );
        Some(cookie_pairs.join("; "))
    }
}

/// Extract media URLs from Reddit HTML page.
fn extract_media_from_html(doc: &Html) -> RedditMedia {
    let mut media = RedditMedia::default();

    // Look for video sources
    // Reddit videos are often in <source> tags or data attributes
    if let Ok(video_selector) = Selector::parse("video source, shreddit-player") {
        for element in doc.select(&video_selector) {
            if let Some(src) = element.value().attr("src") {
                if src.contains("v.redd.it") || src.contains("reddit") {
                    media.video_url = Some(src.to_string());
                    break;
                }
            }
        }
    }

    // Look for post images
    // Reddit images are in i.redd.it or preview.redd.it
    if let Ok(img_selector) = Selector::parse("img[src*='redd.it'], img[src*='redditmedia']") {
        for element in doc.select(&img_selector) {
            if let Some(src) = element.value().attr("src") {
                // Skip tiny thumbnails and icons
                if !src.contains("icon") && !src.contains("avatar") {
                    media.image_urls.push(src.to_string());
                }
            }
        }
    }

    // Look for gallery images (Reddit galleries)
    if let Ok(gallery_selector) = Selector::parse("[data-gallery-item] img, .gallery-tile img") {
        for element in doc.select(&gallery_selector) {
            if let Some(src) = element.value().attr("src") {
                media.image_urls.push(src.to_string());
            }
        }
    }

    // Look for linked media in the post
    if let Ok(link_selector) = Selector::parse("a[href*='i.redd.it'], a[href*='imgur']") {
        for element in doc.select(&link_selector) {
            if let Some(href) = element.value().attr("href") {
                if href.ends_with(".jpg")
                    || href.ends_with(".png")
                    || href.ends_with(".gif")
                    || href.ends_with(".webp")
                {
                    if !media.image_urls.contains(&href.to_string()) {
                        media.image_urls.push(href.to_string());
                    }
                }
            }
        }
    }

    // Look for thumbnail
    if let Ok(thumb_selector) = Selector::parse("[data-thumbnail], .thumbnail img") {
        if let Some(element) = doc.select(&thumb_selector).next() {
            if let Some(src) = element
                .value()
                .attr("src")
                .or(element.value().attr("data-src"))
            {
                media.thumbnail_url = Some(src.to_string());
            }
        }
    }

    // Deduplicate image URLs
    media.image_urls.sort();
    media.image_urls.dedup();

    media
}

/// Extract title from Reddit HTML page.
fn extract_title_from_html(doc: &Html) -> Option<String> {
    // Try various selectors for the title
    let selectors = [
        "h1[slot='title']",
        "h1.title",
        "[data-test-id='post-title']",
        "title",
    ];

    for selector_str in selectors {
        if let Ok(selector) = Selector::parse(selector_str) {
            if let Some(element) = doc.select(&selector).next() {
                let text: String = element.text().collect();
                let text = text.trim();
                if !text.is_empty() && !text.starts_with("reddit") {
                    return Some(text.to_string());
                }
            }
        }
    }

    None
}

/// Extract author from Reddit HTML page.
fn extract_author_from_html(doc: &Html) -> Option<String> {
    let selectors = [
        "a[href*='/user/']",
        "[data-author]",
        ".author",
        "span[slot='authorName']",
    ];

    for selector_str in selectors {
        if let Ok(selector) = Selector::parse(selector_str) {
            if let Some(element) = doc.select(&selector).next() {
                // Try data-author attribute first
                if let Some(author) = element.value().attr("data-author") {
                    return Some(author.to_string());
                }
                // Try href extraction
                if let Some(href) = element.value().attr("href") {
                    if let Some(user) = href.strip_prefix("/user/") {
                        let user = user.split('/').next().unwrap_or(user);
                        return Some(user.to_string());
                    }
                }
                // Try text content
                let text: String = element.text().collect();
                let text = text.trim();
                if !text.is_empty() && text.starts_with("u/") {
                    return Some(text.trim_start_matches("u/").to_string());
                }
            }
        }
    }

    None
}

impl Clone for RedditMedia {
    fn clone(&self) -> Self {
        Self {
            video_url: self.video_url.clone(),
            image_urls: self.image_urls.clone(),
            thumbnail_url: self.thumbnail_url.clone(),
        }
    }
}

/// Check if a URL is a redd.it shortlink.
fn is_shortlink(url: &str) -> bool {
    SHORTLINK_PATTERN.is_match(url)
}

/// Check if a Reddit URL points to a known NSFW subreddit based on name patterns.
fn is_nsfw_subreddit(url: &str) -> bool {
    if let Some(caps) = SUBREDDIT_PATTERN.captures(url) {
        if let Some(subreddit) = caps.get(1) {
            let subreddit_lower = subreddit.as_str().to_lowercase();
            return NSFW_SUBREDDIT_PATTERNS
                .iter()
                .any(|pattern| subreddit_lower.contains(pattern));
        }
    }
    false
}

/// Resolve a redd.it short URL to full Reddit URL.
///
/// Sends a HEAD request and follows the redirect location header.
pub async fn resolve_short_url(short_url: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .head(short_url)
        .header("User-Agent", ARCHIVAL_USER_AGENT)
        .send()
        .await
        .context("Failed to resolve short URL")?;

    if let Some(location) = response.headers().get("location") {
        Ok(location.to_str().unwrap_or(short_url).to_string())
    } else {
        Ok(short_url.to_string())
    }
}

/// Follow redirects for a Reddit URL to get the final canonical URL.
///
/// Some Reddit URLs (e.g., /comment/xyz) redirect to the full URL with post title.
/// This function follows up to 5 redirects and returns the final URL.
pub async fn follow_redirects(url: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .timeout(Duration::from_secs(10))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .head(url)
        .header("User-Agent", ARCHIVAL_USER_AGENT)
        .send()
        .await
        .context("Failed to follow redirects")?;

    Ok(response.url().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle() {
        let handler = RedditHandler::new();

        assert!(handler.can_handle("https://www.reddit.com/r/rust/"));
        assert!(handler.can_handle("https://old.reddit.com/r/test"));
        assert!(handler.can_handle("https://redd.it/abc123"));
        assert!(handler.can_handle("https://i.redd.it/image.jpg"));
        assert!(handler.can_handle("https://v.redd.it/video123"));

        assert!(!handler.can_handle("https://example.com/"));
        assert!(!handler.can_handle("https://youtube.com/"));
    }

    #[test]
    fn test_normalize_url() {
        let handler = RedditHandler::new();

        assert!(handler
            .normalize_url("https://www.reddit.com/r/test")
            .contains("old.reddit.com"));
        assert!(handler
            .normalize_url("https://m.reddit.com/r/test")
            .contains("old.reddit.com"));
        assert!(handler
            .normalize_url("https://new.reddit.com/r/test")
            .contains("old.reddit.com"));
    }

    #[test]
    fn test_is_shortlink() {
        // Valid shortlinks
        assert!(is_shortlink("https://redd.it/abc123"));
        assert!(is_shortlink("http://redd.it/xyz789"));

        // Not shortlinks (media subdomains)
        assert!(!is_shortlink("https://i.redd.it/image.jpg"));
        assert!(!is_shortlink("https://v.redd.it/video123"));
        assert!(!is_shortlink("https://preview.redd.it/something"));

        // Not shortlinks (full Reddit URLs)
        assert!(!is_shortlink("https://www.reddit.com/r/rust"));
        assert!(!is_shortlink("https://old.reddit.com/r/test"));
    }

    #[test]
    fn test_is_nsfw_subreddit() {
        // Test exact pattern matches
        assert!(is_nsfw_subreddit(
            "https://old.reddit.com/r/nsfw/comments/abc123"
        ));
        assert!(is_nsfw_subreddit("https://reddit.com/r/gonewild/"));
        assert!(is_nsfw_subreddit("https://old.reddit.com/r/porn/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/hentai/"));
        assert!(is_nsfw_subreddit(
            "https://reddit.com/r/rule34/comments/xyz"
        ));

        // Test additional patterns from NSFW_SUBREDDIT_PATTERNS
        assert!(is_nsfw_subreddit("https://reddit.com/r/xxx/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/nude/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/sex/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/adult/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/18_plus/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/onlyfans/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/lewd/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/celebnsfw/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/realgirls/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/boobs/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/ass/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/tits/"));

        // Test case insensitivity
        assert!(is_nsfw_subreddit("https://www.reddit.com/r/NSFW_GIF/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/GoNeWiLd/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/PORN/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/HentAI/"));

        // Test pattern matching within subreddit names
        assert!(is_nsfw_subreddit("https://reddit.com/r/asiansgoNewild/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/amateur_porn/"));
        assert!(is_nsfw_subreddit("https://reddit.com/r/nsfw_gifs/"));

        // SFW subreddits that might contain misleading words
        assert!(!is_nsfw_subreddit("https://reddit.com/r/rust/"));
        assert!(!is_nsfw_subreddit("https://old.reddit.com/r/programming/"));
        assert!(!is_nsfw_subreddit("https://www.reddit.com/r/funny/"));
        assert!(!is_nsfw_subreddit("https://reddit.com/r/pics/"));
        assert!(!is_nsfw_subreddit("https://reddit.com/r/technology/"));
        assert!(!is_nsfw_subreddit("https://reddit.com/r/worldnews/"));

        // Edge cases - non-subreddit URLs
        assert!(!is_nsfw_subreddit("https://reddit.com/user/someone"));
        assert!(!is_nsfw_subreddit("https://i.redd.it/image.jpg"));
        assert!(!is_nsfw_subreddit("https://reddit.com/"));
        assert!(!is_nsfw_subreddit("https://www.reddit.com"));
    }
}
