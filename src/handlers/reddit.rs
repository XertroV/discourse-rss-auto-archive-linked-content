use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use reqwest::header::COOKIE;
use scraper::{Html, Selector};
use tokio::process::Command;
use tracing::{debug, info, warn};

use crate::chromium_profile::chromium_user_data_and_profile_from_spec;
use crate::fs_utils::copy_dir_best_effort;

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

/// Default subreddits exempt from the basic name-pattern NSFW heuristic.
///
/// This only affects `is_nsfw_subreddit()` (subreddit-name keyword matching). Explicit Reddit
/// `over18`/NSFW signals can still mark content NSFW.
const DEFAULT_NSFW_SUBREDDIT_EXEMPTIONS: &[&str] = &["twoxchromosomes"];

/// Additional exemptions can be provided via `REDDIT_NSFW_SUBREDDIT_EXEMPTIONS`.
///
/// Format: comma-separated subreddit names (case-insensitive), e.g.
/// `TwoXChromosomes, some_subreddit, r/AnotherOne`
static NSFW_SUBREDDIT_EXEMPTIONS: std::sync::LazyLock<Vec<String>> =
    std::sync::LazyLock::new(|| {
        let mut v: Vec<String> = DEFAULT_NSFW_SUBREDDIT_EXEMPTIONS
            .iter()
            .map(|s| s.to_string())
            .collect();

        if let Ok(extra) = std::env::var("REDDIT_NSFW_SUBREDDIT_EXEMPTIONS") {
            for item in extra.split(',') {
                let item = item.trim();
                if item.is_empty() {
                    continue;
                }
                let item = item.strip_prefix("r/").unwrap_or(item);
                let lowered = item.to_lowercase();
                if !lowered.is_empty() {
                    v.push(lowered);
                }
            }
        }

        v.sort();
        v.dedup();
        v
    });

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
        config: &crate::config::Config,
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

        // Fetch HTML first (authenticated when possible). We'll derive canonical/final URL
        // from the HTML itself, which works even when redirect behavior depends on cookies.
        let html_result = fetch_reddit_html(&normalized_url, work_dir, cookies).await;

        // Extract media URLs and final URL from HTML if successful
        let (html_content, media, final_url) = match &html_result {
            Ok((html, media, final_url)) => {
                (Some(html.clone()), Some(media.clone()), final_url.clone())
            }
            Err(e) => {
                warn!("Failed to fetch Reddit HTML: {e}");
                (None, None, None)
            }
        };

        // Use final URL for downstream tooling when available.
        let archive_url = final_url.as_deref().unwrap_or(&normalized_url);

        // Check if the subreddit name suggests NSFW content (heuristic)
        let subreddit_nsfw = is_nsfw_subreddit(archive_url);

        // Try yt-dlp for video/media content (only if we detected video)
        let has_video = media
            .as_ref()
            .map(|m| m.video_url.is_some())
            .unwrap_or(false);
        let ytdlp_result = if has_video {
            match ytdlp::download(archive_url, work_dir, cookies, config).await {
                Ok(result) => Some(result),
                Err(e) => {
                    debug!("yt-dlp failed for Reddit video: {e}");
                    None
                }
            }
        } else {
            // Try yt-dlp anyway in case there's embedded media we didn't detect
            match ytdlp::download(archive_url, work_dir, cookies, config).await {
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

        // If Reddit explicitly marks the post as NSFW/over18, always persist NSFW=true.
        // This overrides any earlier value (including Some(false)).
        let definitely_nsfw = html_content
            .as_deref()
            .map(is_reddit_definitely_nsfw)
            .unwrap_or(false);

        if definitely_nsfw {
            result.is_nsfw = Some(true);
            result.nsfw_source = Some("reddit_over18".to_string());
        } else if result.is_nsfw.is_none() && subreddit_nsfw {
            result.is_nsfw = Some(true);
            result.nsfw_source = Some("subreddit".to_string());
        }

        // Set final URL if discovered
        result.final_url = final_url;

        Ok(result)
    }
}

// NOTE: We intentionally avoid separate redirect probing for Reddit.
// Redirect/canonical behavior may depend on authenticated cookies, and we already
// fetch HTML with cookies when possible and can derive the canonical URL from it.

/// Fetch Reddit HTML page and extract media URLs.
///
/// Returns the HTML content and detected media information.
/// Uses cookies if available for authenticated access.
async fn fetch_reddit_html(
    url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
) -> Result<(String, RedditMedia, Option<String>)> {
    // Reddit frequently blocks datacenter IPs unless authenticated.
    // Prefer using cookies-from-browser (persisted Chromium profile) since that's the most
    // reliable way to mimic a real logged-in browser.
    if cookies.browser_profile.is_some() {
        match fetch_html_with_chromium_profile(url, work_dir, cookies).await {
            Ok(html) => {
                if is_reddit_block_page(&html) {
                    warn!("Reddit HTML looks like a block page on old.reddit.com; retrying via www.reddit.com");
                } else {
                    debug!("Fetched Reddit HTML via Chromium profile");
                    return finalize_reddit_html(url, work_dir, html).await;
                }
            }
            Err(e) => {
                warn!("Chromium profile fetch failed; will try cookies.txt fallback: {e}");
            }
        }

        // Retry via www.reddit.com if old.reddit.com appears blocked.
        if url.contains("old.reddit.com") {
            let alt_url = url.replace("old.reddit.com", "www.reddit.com");
            match fetch_html_with_chromium_profile(&alt_url, work_dir, cookies).await {
                Ok(html) => {
                    if !is_reddit_block_page(&html) {
                        debug!(original = %url, alt = %alt_url, "Fetched Reddit HTML via Chromium profile (www fallback)");
                        return finalize_reddit_html(&alt_url, work_dir, html).await;
                    }
                    warn!("www.reddit.com HTML also looks blocked; falling back to cookies.txt");
                }
                Err(e) => {
                    warn!("Chromium www.reddit.com fallback failed; will try cookies.txt fallback: {e}");
                }
            }
        }
    }

    // Fallback: cookies.txt via reqwest (fast, but less reliable for Reddit)
    let cookie_header = build_cookie_header(cookies, "reddit.com");
    if cookie_header.is_none() {
        anyhow::bail!(
            "Reddit HTML fetch requires authenticated cookies (datacenter IPs are often blocked). \
Configure either YT_DLP_COOKIES_FROM_BROWSER (recommended) or COOKIES_FILE_PATH for reddit.com."
        );
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .context("Failed to build HTTP client")?;

    info!("Using cookies.txt for Reddit request");
    let response = client
        .get(url)
        .header("User-Agent", ARCHIVAL_USER_AGENT)
        .header(COOKIE, cookie_header.unwrap())
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

    finalize_reddit_html(url, work_dir, html).await
}

async fn finalize_reddit_html(
    url: &str,
    work_dir: &Path,
    html: String,
) -> Result<(String, RedditMedia, Option<String>)> {
    // Save raw HTML
    let html_path = work_dir.join("raw.html");
    tokio::fs::write(&html_path, &html)
        .await
        .context("Failed to write HTML file")?;

    // Parse HTML and extract media URLs
    let doc = Html::parse_document(&html);
    let media = extract_media_from_html(&doc);
    let final_url = extract_canonical_url_from_html(&doc);

    debug!(
        requested = %url,
        final_url = %final_url.as_deref().unwrap_or(""),
        video = ?media.video_url,
        images = media.image_urls.len(),
        "Extracted media from Reddit HTML"
    );

    Ok((html, media, final_url))
}

fn extract_canonical_url_from_html(doc: &Html) -> Option<String> {
    // Most reliable for Reddit: <link rel="canonical" href="...">
    let selector = Selector::parse("link[rel='canonical']").ok()?;
    let el = doc.select(&selector).next()?;
    let href = el.value().attr("href")?.trim();
    if href.is_empty() {
        None
    } else if href.starts_with("//") {
        Some(format!("https:{href}"))
    } else if href.starts_with("http://") || href.starts_with("https://") {
        Some(href.to_string())
    } else {
        None
    }
}

async fn fetch_html_with_chromium_profile(
    url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
) -> Result<String> {
    let spec = cookies
        .browser_profile
        .context("No browser_profile configured")?;

    let (source_user_data_dir, profile_dir) = chromium_user_data_and_profile_from_spec(spec);

    // Chromium will refuse to start if the same user-data-dir is in use (e.g. the cookie-browser
    // container is running and has the profile open). To avoid lock contention, copy the
    // user-data-dir to a per-archive temp directory and run Chromium against the copy.
    let user_data_dir =
        clone_chromium_user_data_dir(work_dir, &source_user_data_dir, profile_dir.as_deref())
            .await
            .context("Failed to clone Chromium user-data-dir for Reddit HTML fetch")?;

    let chrome_path =
        std::env::var("SCREENSHOT_CHROME_PATH").unwrap_or_else(|_| "chromium".to_string());

    let mut cmd = Command::new(chrome_path);
    cmd.arg("--headless=new")
        .arg("--no-sandbox")
        .arg("--disable-gpu")
        .arg("--disable-dev-shm-usage")
        .arg("--window-size=1280,2000")
        .arg("--lang=en-US,en")
        .arg("--disable-blink-features=AutomationControlled")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg(format!("--user-agent={ARCHIVAL_USER_AGENT}"))
        .arg(format!("--user-data-dir={}", user_data_dir.display()));

    if let Some(profile_dir) = profile_dir {
        cmd.arg(format!("--profile-directory={profile_dir}"));
    }

    // Dump final DOM after JS execution/navigation.
    cmd.arg("--dump-dom").arg(url);

    let output = tokio::time::timeout(Duration::from_secs(45), cmd.output())
        .await
        .context("Chromium dump-dom timed out")?
        .context("Failed to execute Chromium")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "Chromium dump-dom failed (exit {:?}): {}",
            output.status.code(),
            stderr.trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let html = stdout.trim();
    if html.is_empty() {
        anyhow::bail!("Chromium dump-dom returned empty output");
    }
    Ok(html.to_string())
}

fn is_reddit_block_page(html: &str) -> bool {
    let s = html.to_ascii_lowercase();
    s.contains("you've been blocked by network security")
        || s.contains("you have been blocked by network security")
        || s.contains("whoa there")
        || s.contains("woah there")
}

async fn clone_chromium_user_data_dir(
    work_dir: &Path,
    source: &std::path::Path,
    profile_dir: Option<&str>,
) -> Result<std::path::PathBuf> {
    let dest = work_dir.join("chromium-user-data");

    // Clean up any previous attempt (best-effort). Work dirs are per-archive, but retries can
    // happen when re-archiving.
    let _ = tokio::fs::remove_dir_all(&dest).await;
    tokio::fs::create_dir_all(&dest)
        .await
        .context("Failed to create chromium-user-data dir")?;

    // Prefer `cp -a` to preserve Chromium's expected layout.
    // However, the shared cookie volume may contain files that are not readable by the
    // archiver user (e.g. root-owned 0600). In that case `cp` fails early; we fall back to a
    // best-effort copy that skips unreadable files.
    let cp_output = Command::new("cp")
        .arg("-a")
        .arg(format!("{}/.", source.display()))
        .arg(&dest)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await;

    match cp_output {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                status = %output.status,
                source = %source.display(),
                stderr = %stderr.trim(),
                "cp -a failed while cloning Chromium profile; falling back to best-effort copy"
            );
            copy_dir_best_effort(source, &dest, "chromium profile clone (reddit html)").await?;
        }
        Err(e) => {
            warn!(
                error = %e,
                source = %source.display(),
                "Failed to spawn cp -a for Chromium profile copy; falling back to best-effort copy"
            );
            copy_dir_best_effort(source, &dest, "chromium profile clone (reddit html)").await?;
        }
    }

    // Remove singleton lock/socket artifacts so Chromium doesn't think the copied profile is
    // already in-use.
    for name in ["SingletonLock", "SingletonCookie", "SingletonSocket"] {
        let _ = tokio::fs::remove_file(dest.join(name)).await;
    }

    // Validate that the clone contains the critical cookie materials.
    // `Local State` (in user-data-dir root) is commonly required to decrypt cookies.
    let local_state = dest.join("Local State");
    if !local_state.is_file() {
        anyhow::bail!(
            "Cloned Chromium profile is missing 'Local State'. This usually means the shared cookies volume has restrictive permissions (root-owned 0600 files).\n\nFix (recommended):\n  docker compose --profile manual exec cookie-browser bash -lc 'chmod -R a+rX /cookies/chromium-profile'\n\nOr run cookie-browser as a non-root user that matches the archiver container UID/GID."
        );
    }

    // Cookies DB must also be present and readable.
    let profile_name = profile_dir.unwrap_or("Default");
    let cookie_db_candidates = [
        dest.join(profile_name).join("Cookies"),
        dest.join(profile_name).join("Network").join("Cookies"),
        dest.join("Default").join("Cookies"),
        dest.join("Default").join("Network").join("Cookies"),
    ];
    let has_cookie_db = cookie_db_candidates.iter().any(|p| p.is_file());
    if !has_cookie_db {
        anyhow::bail!(
            "Cloned Chromium profile does not contain a readable Cookies database. This usually means the shared cookies volume has restrictive permissions.\n\nFix (recommended):\n  docker compose --profile manual exec cookie-browser bash -lc 'chmod -R a+rX /cookies/chromium-profile'\n\nAlternative: use COOKIES_FILE_PATH with an exported Netscape cookies.txt."
        );
    }

    Ok(dest)
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

            if NSFW_SUBREDDIT_EXEMPTIONS
                .iter()
                .any(|s| s == &subreddit_lower)
            {
                return false;
            }

            return NSFW_SUBREDDIT_PATTERNS
                .iter()
                .any(|pattern| subreddit_lower.contains(pattern));
        }
    }
    false
}

fn is_reddit_definitely_nsfw(html: &str) -> bool {
    // Be conservative: only treat explicit Reddit flags as definitive.
    // (Avoid keyword scanning which is prone to false positives.)
    let s = html.to_ascii_lowercase();

    // Common JSON flags in Reddit pages.
    s.contains("\"over18\":true")
        || s.contains("\"over18\": true")
        || s.contains("\"isnsfw\":true")
        || s.contains("\"isnsfw\": true")
        || s.contains("\"is_nsfw\":true")
        || s.contains("\"is_nsfw\": true")
        || s.contains("\"nsfw\":true")
        || s.contains("\"nsfw\": true")
        // Some pages may include explicit HTML attributes.
        || s.contains("data-over18=\"true\"")
        || s.contains("data-nsfw=\"true\"")
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

        // Exemptions
        assert!(!is_nsfw_subreddit(
            "https://old.reddit.com/r/TwoXChromosomes/comments/1q98p8v/debunking_lesbian_domestic_violence_data/nyvkjv5/"
        ));

        // Edge cases - non-subreddit URLs
        assert!(!is_nsfw_subreddit("https://reddit.com/user/someone"));
        assert!(!is_nsfw_subreddit("https://i.redd.it/image.jpg"));
        assert!(!is_nsfw_subreddit("https://reddit.com/"));
        assert!(!is_nsfw_subreddit("https://www.reddit.com"));
    }

    #[test]
    fn test_is_reddit_definitely_nsfw() {
        assert!(is_reddit_definitely_nsfw("{\"over18\":true}"));
        assert!(is_reddit_definitely_nsfw("data-over18=\"true\""));
        assert!(is_reddit_definitely_nsfw("{\"isNsfw\": true}"));
        assert!(!is_reddit_definitely_nsfw("this is not nsfw, just text"));
    }
}
