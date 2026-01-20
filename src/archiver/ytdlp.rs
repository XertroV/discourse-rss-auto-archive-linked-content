use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono;
use serde::Deserialize;
use sqlx::SqlitePool;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{debug, info, warn};

use super::CookieOptions;
use crate::config::Config;
use crate::handlers::ArchiveResult;

/// Video metadata from yt-dlp --dump-json
#[derive(Debug, Deserialize)]
struct VideoMetadata {
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    filesize: Option<u64>,
    #[serde(default)]
    filesize_approx: Option<u64>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
}

/// Download progress information parsed from yt-dlp output.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadProgress {
    /// Progress percentage (0.0-100.0)
    pub percent: f64,
    /// Download speed (e.g., "2.34MiB/s")
    pub speed: Option<String>,
    /// Estimated time remaining (e.g., "00:15")
    pub eta: Option<String>,
    /// Downloaded size (e.g., "45.5MiB")
    pub downloaded: Option<String>,
    /// Total size (e.g., "~100MiB")
    pub total_size: Option<String>,
}

/// Parse download progress from yt-dlp output line.
///
/// Example yt-dlp progress lines:
/// - `[download]   15.2% of ~45.50MiB at 2.34MiB/s ETA 00:15`
/// - `[download] 100% of 45.50MiB in 00:20`
/// - `[download] Destination: filename.mp4`
///
/// Returns `Some(DownloadProgress)` if the line contains parseable progress info.
fn parse_ytdlp_progress(line: &str) -> Option<DownloadProgress> {
    // Only process [download] lines
    if !line.contains("[download]") {
        return None;
    }

    // Skip non-progress lines (destination, resuming, etc.)
    if line.contains("Destination:")
        || line.contains("Resuming")
        || line.contains("has already been downloaded")
    {
        return None;
    }

    // Try to extract percentage
    let percent = if let Some(pct_pos) = line.find('%') {
        // Look backwards from % to find the number
        let before_pct = &line[..pct_pos];
        let parts: Vec<&str> = before_pct.split_whitespace().collect();
        if let Some(last_part) = parts.last() {
            last_part.parse::<f64>().ok()?
        } else {
            return None;
        }
    } else {
        return None;
    };

    // Extract speed (e.g., "at 2.34MiB/s")
    let speed = if let Some(at_pos) = line.find(" at ") {
        let after_at = &line[at_pos + 4..];
        after_at.split_whitespace().next().map(|s| s.to_string())
    } else {
        None
    };

    // Extract ETA (e.g., "ETA 00:15")
    let eta = if let Some(eta_pos) = line.find("ETA ") {
        let after_eta = &line[eta_pos + 4..];
        after_eta.split_whitespace().next().map(|s| s.to_string())
    } else {
        None
    };

    // Extract size info (e.g., "of ~45.50MiB" or "of 45.50MiB")
    let (downloaded, total_size) = if let Some(of_pos) = line.find(" of ") {
        let after_of = &line[of_pos + 4..];
        let size_part = after_of.split_whitespace().next().map(|s| s.to_string());
        (None, size_part)
    } else {
        (None, None)
    };

    Some(DownloadProgress {
        percent,
        speed,
        eta,
        downloaded,
        total_size,
    })
}

/// Determine the appropriate quality/format string based on video characteristics.
fn select_format_string(metadata: Option<&VideoMetadata>) -> String {
    // Default format if no metadata available
    const DEFAULT_FORMAT: &str = "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best";

    let Some(meta) = metadata else {
        debug!("No metadata available, using default 1080p format");
        return DEFAULT_FORMAT.to_string();
    };

    let duration_secs = meta.duration.unwrap_or(0.0);
    let is_short_video = duration_secs < 600.0; // Less than 10 minutes

    // Estimate file size and bitrate
    let estimated_size = meta.filesize.or(meta.filesize_approx).unwrap_or(0);

    // Calculate approximate bitrate (bytes per second) to detect highly compressed videos
    // Highly compressed videos (e.g., static screen recordings, slideshows) have low bitrate
    let bitrate_bps = if duration_secs > 0.0 {
        (estimated_size as f64) / duration_secs
    } else {
        0.0
    };

    // Heuristic: videos with bitrate < 500 KB/s are considered "highly compressed"
    // These can be downloaded at higher resolution even if long
    let is_highly_compressed = estimated_size > 0 && bitrate_bps < 500_000.0;

    let width = meta.width.unwrap_or(0);
    let height = meta.height.unwrap_or(0);
    let total_pixels = width * height;
    let is_native_acceptable = total_pixels <= 1920 * 1080;

    // Decision logic:
    // 1. Short videos (<10 min):
    //    - If native resolution ≤ 1920x1080: use native
    //    - Otherwise: cap at 1080p
    // 2. Long videos BUT highly compressed (low bitrate):
    //    - Use 1080p (well-compressed, so file size won't be huge)
    // 3. Long videos with normal/high bitrate:
    //    - Cap at 720p for size efficiency

    if is_short_video {
        if is_native_acceptable && width > 0 && height > 0 {
            debug!(
                duration_secs,
                width, height, "Short video with acceptable native resolution, using best quality"
            );
            "bestvideo+bestaudio/best".to_string()
        } else {
            debug!(
                duration_secs,
                width, height, "Short video, capping at 1080p"
            );
            DEFAULT_FORMAT.to_string()
        }
    } else if is_highly_compressed {
        debug!(
            duration_secs,
            estimated_size,
            bitrate_mbps = bitrate_bps / 1_000_000.0,
            "Long but highly compressed video (low bitrate), using 1080p"
        );
        DEFAULT_FORMAT.to_string()
    } else {
        debug!(
            duration_secs,
            estimated_size,
            bitrate_mbps = bitrate_bps / 1_000_000.0,
            "Long video with normal/high bitrate, capping at 720p for size efficiency"
        );
        "bestvideo[height<=720]+bestaudio/best[height<=720]/best".to_string()
    }
}

/// Get video metadata without downloading (pre-flight check).
///
/// # Errors
///
/// Returns an error if yt-dlp fails to retrieve metadata.
async fn get_video_metadata(url: &str, cookies: &CookieOptions<'_>) -> Result<VideoMetadata> {
    let mut args = vec![
        "--dump-json".to_string(),
        "--no-playlist".to_string(),
        "--no-warnings".to_string(),
        "--quiet".to_string(),
    ];

    // Add cookie options
    if let Some(spec) = cookies.browser_profile {
        let spec = maybe_adjust_chromium_user_data_dir_spec(spec);
        args.push("--cookies-from-browser".to_string());
        args.push(spec);
    } else if let Some(cookies_path) = cookies.cookies_file {
        if cookies_path.exists() && !cookies_path.is_dir() {
            args.push("--cookies".to_string());
            args.push(cookies_path.to_string_lossy().to_string());
        }
    }

    args.push(url.to_string());

    debug!(url = %url, "Fetching video metadata");

    let output = Command::new("yt-dlp")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn yt-dlp for metadata")?
        .wait_with_output()
        .await
        .context("Failed to wait for yt-dlp metadata")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("yt-dlp metadata fetch failed: {stderr}");
    }

    let metadata: VideoMetadata =
        serde_json::from_slice(&output.stdout).context("Failed to parse yt-dlp metadata JSON")?;

    Ok(metadata)
}

/// Download content using yt-dlp.
///
/// If both browser_profile and cookies_file are provided, browser_profile is preferred
/// as it typically provides fresher cookies.
///
/// If `archive_id` and `pool` are provided, progress updates will be written to the database.
///
/// # Errors
///
/// Returns an error if yt-dlp fails or times out.
pub async fn download(
    url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
    config: &Config,
    archive_id: Option<i64>,
    pool: Option<&SqlitePool>,
) -> Result<ArchiveResult> {
    // Pre-flight check: get video metadata to check duration limits and select quality
    let mut format_string = select_format_string(None); // Default

    match get_video_metadata(url, cookies).await {
        Ok(metadata) => {
            // Check duration limit
            if let Some(max_duration) = config.youtube_max_duration_seconds {
                if let Some(duration) = metadata.duration {
                    let duration_secs = duration as u32;
                    if duration_secs > max_duration {
                        anyhow::bail!(
                            "Video duration ({duration_secs}s) exceeds maximum allowed duration ({max_duration}s). \
                            Configure YOUTUBE_MAX_DURATION_SECONDS to increase or remove the limit."
                        );
                    }
                    debug!(duration_secs, max_duration, "Video duration check passed");
                }
            }

            // Select format based on metadata
            format_string = select_format_string(Some(&metadata));
        }
        Err(e) => {
            // Log warning but continue - metadata fetch can fail for some videos
            warn!("Failed to fetch video metadata for pre-flight checks: {e}");
        }
    }

    let output_template = work_dir.join("%(title)s.%(ext)s");

    let mut args = vec![
        "-4".to_string(),
        "--no-playlist".to_string(),
        "--write-info-json".to_string(),
        "--write-thumbnail".to_string(),
        // Request both manual and auto-generated subtitles
        "--write-subs".to_string(),
        "--write-auto-subs".to_string(),
        // Request subtitles in multiple languages (English + original if different)
        "--sub-langs".to_string(),
        "en.*,en-orig,en".to_string(),
        // Request subtitles in multiple formats (both VTT and SRT)
        "--sub-format".to_string(),
        "vtt,srt".to_string(),
        "--output".to_string(),
        output_template.to_string_lossy().to_string(),
        // Use --newline for parseable progress output (each update on a new line)
        "--newline".to_string(),
        // Format selection: adaptive based on video characteristics
        "--format".to_string(),
        format_string,
        // Enable JavaScript challenge solving for YouTube bot detection
        // TODO: Re-enable --remote-components ejs:github once compatibility issue is resolved
        // For now, yt-dlp will use default extractors (may have limited format availability)
        // "--remote-components".to_string(),
        // "ejs:github".to_string(),
    ];

    // Skip comment extraction during main download - comments will be extracted
    // by the dedicated comment worker to avoid blocking other archives
    // Comment extraction is now done as a separate background job

    // Prefer browser profile over cookies file (fresher cookies)
    // Only use one method to avoid potential conflicts
    let mut cookie_method_used = false;

    if let Some(spec) = cookies.browser_profile {
        let spec = maybe_adjust_chromium_user_data_dir_spec(spec);
        debug!(spec = %spec, "Using cookies from browser profile");
        args.push("--cookies-from-browser".to_string());
        args.push(spec);
        cookie_method_used = true;
    }

    // Only use cookies file if browser profile not set
    if !cookie_method_used {
        if let Some(cookies_path) = cookies.cookies_file {
            if !cookies_path.exists() {
                warn!(path = %cookies_path.display(), "Cookies file specified but does not exist, continuing without cookies");
            } else if cookies_path.is_dir() {
                warn!(path = %cookies_path.display(), "Cookies path is a directory, continuing without cookies");
            } else {
                debug!(path = %cookies_path.display(), "Using cookies file for authenticated download");
                args.push("--cookies".to_string());
                args.push(cookies_path.to_string_lossy().to_string());
            }
        }
    }

    // URL goes last
    args.push(url.to_string());

    debug!(url = %url, "Running yt-dlp");

    // Spawn yt-dlp and capture stdout/stderr for progress tracking
    let mut child = Command::new("yt-dlp")
        .args(&args)
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn yt-dlp")?;

    let stdout = child
        .stdout
        .take()
        .context("Failed to capture yt-dlp stdout")?;
    let stderr = child
        .stderr
        .take()
        .context("Failed to capture yt-dlp stderr")?;

    // Stream stdout line by line for progress tracking
    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_lines = Vec::new();
    let mut stderr_reader = BufReader::new(stderr).lines();

    // Track last progress update time to avoid excessive DB writes
    let mut last_update = std::time::Instant::now();
    let update_interval = Duration::from_secs(2); // Update DB every 2 seconds max

    // Wrap the streaming in a timeout
    let timeout_duration = Duration::from_secs(config.youtube_download_timeout_seconds);
    let streaming_future = async {
        loop {
            tokio::select! {
                line_result = stdout_reader.next_line() => {
                    match line_result {
                        Ok(Some(line)) => {
                            debug!("yt-dlp: {}", line);

                            // Try to parse progress
                            if let Some(progress) = parse_ytdlp_progress(&line) {
                                // Only update DB if enough time has passed and we have pool + archive_id
                                if let (Some(id), Some(db_pool)) = (archive_id, pool) {
                                    if last_update.elapsed() >= update_interval {
                                        let progress_json = serde_json::to_string(&progress)
                                            .unwrap_or_else(|_| "{}".to_string());

                                        if let Err(e) = crate::db::update_archive_progress(
                                            db_pool,
                                            id,
                                            progress.percent,
                                            &progress_json,
                                        )
                                        .await
                                        {
                                            warn!("Failed to update progress: {}", e);
                                        } else {
                                            debug!(
                                                percent = progress.percent,
                                                speed = ?progress.speed,
                                                eta = ?progress.eta,
                                                "Updated download progress"
                                            );
                                        }

                                        last_update = std::time::Instant::now();
                                    }
                                }
                            }
                        }
                        Ok(None) => break, // EOF
                        Err(e) => {
                            warn!("Error reading yt-dlp stdout: {}", e);
                            break;
                        }
                    }
                }
                line_result = stderr_reader.next_line() => {
                    match line_result {
                        Ok(Some(line)) => {
                            stderr_lines.push(line);
                        }
                        Ok(None) => {} // stderr EOF
                        Err(e) => {
                            warn!("Error reading yt-dlp stderr: {}", e);
                        }
                    }
                }
            }
        }

        // Wait for process to complete
        let status = child.wait().await.context("Failed to wait for yt-dlp")?;

        Ok::<_, anyhow::Error>((status, stderr_lines))
    };

    let (status, stderr_lines) = tokio::time::timeout(timeout_duration, streaming_future)
        .await
        .context(format!(
            "yt-dlp download timed out after {} seconds",
            config.youtube_download_timeout_seconds
        ))??;

    // Clear progress from database when done
    if let (Some(id), Some(db_pool)) = (archive_id, pool) {
        if let Err(e) = crate::db::clear_archive_progress(db_pool, id).await {
            warn!("Failed to clear progress: {}", e);
        }
    }

    if !status.success() {
        let stderr = stderr_lines.join("\n");
        if stderr.contains("could not find chromium cookies database") {
            anyhow::bail!(
                "yt-dlp failed: {stderr}\n\nHint: yt-dlp couldn't locate Chromium's Cookies database under the path from YT_DLP_COOKIES_FROM_BROWSER.\n- If you're using a persisted --user-data-dir, the DB is commonly under .../Default (or .../Default/Network/Cookies).\n- Run ./dc-cookies-browser.sh once and let Chromium fully start, then log in and retry."
            );
        }
        anyhow::bail!("yt-dlp failed: {stderr}");
    }

    // Find the info.json file to get metadata
    let metadata = find_and_parse_metadata(work_dir, config).await?;

    Ok(metadata)
}

/// Download supplementary artifacts (subtitles, comments) without re-downloading the video.
///
/// This is used during re-archiving when the video file already exists but
/// subtitles or comments are missing. Uses `--skip-download` to avoid re-downloading the video.
///
/// # Errors
///
/// Returns an error if yt-dlp fails or times out.
pub async fn download_supplementary_artifacts(
    url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
    config: &Config,
    download_subtitles: bool,
    download_comments: bool,
) -> Result<ArchiveResult> {
    if !download_subtitles && !download_comments {
        debug!("No supplementary artifacts requested");
        return Ok(ArchiveResult::default());
    }

    let output_template = work_dir.join("%(title)s.%(ext)s");

    let mut args = vec![
        "-4".to_string(),
        "--no-playlist".to_string(),
        "--skip-download".to_string(), // Skip video download - we already have it
        "--write-info-json".to_string(),
        "--output".to_string(),
        output_template.to_string_lossy().to_string(),
        "--no-progress".to_string(),
        "--quiet".to_string(),
    ];

    // Add subtitle options if requested
    if download_subtitles {
        args.push("--write-subs".to_string());
        args.push("--write-auto-subs".to_string());
        args.push("--sub-langs".to_string());
        args.push("en.*,en-orig,en".to_string());
        args.push("--sub-format".to_string());
        args.push("vtt,srt".to_string());
        debug!("Subtitle download enabled for supplementary artifacts");
    }

    // Add comment extraction if requested
    if download_comments && config.comments_enabled {
        args.push("--write-comments".to_string());
        debug!("Comment extraction enabled for supplementary artifacts");
    }

    // Cookie handling
    let mut cookie_method_used = false;

    if let Some(spec) = cookies.browser_profile {
        let spec = maybe_adjust_chromium_user_data_dir_spec(spec);
        debug!(spec = %spec, "Using cookies from browser profile");
        args.push("--cookies-from-browser".to_string());
        args.push(spec);
        cookie_method_used = true;
    }

    if !cookie_method_used {
        if let Some(cookies_path) = cookies.cookies_file {
            if cookies_path.exists() && !cookies_path.is_dir() {
                debug!(path = %cookies_path.display(), "Using cookies file");
                args.push("--cookies".to_string());
                args.push(cookies_path.to_string_lossy().to_string());
            }
        }
    }

    args.push(url.to_string());

    debug!(url = %url, download_subtitles, download_comments, "Running yt-dlp for supplementary artifacts");

    // Wrap with timeout (shorter than full download since we're not downloading video)
    let timeout_duration = Duration::from_secs(120); // 2 minutes should be plenty for metadata
    let download_future = async {
        Command::new("yt-dlp")
            .args(&args)
            .current_dir(work_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn yt-dlp")?
            .wait_with_output()
            .await
            .context("Failed to wait for yt-dlp")
    };

    let output = tokio::time::timeout(timeout_duration, download_future)
        .await
        .context("yt-dlp supplementary artifact download timed out after 120 seconds")??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("yt-dlp supplementary download failed: {stderr}");
    }

    // Find and parse metadata to get the extra files
    let metadata = find_and_parse_metadata(work_dir, config).await?;

    info!(
        subtitle_count = metadata
            .extra_files
            .iter()
            .filter(|f| is_subtitle_file(f))
            .count(),
        has_comments = metadata.extra_files.iter().any(|f| f == "comments.json"),
        "Downloaded supplementary artifacts"
    );

    Ok(metadata)
}

fn maybe_adjust_chromium_user_data_dir_spec(spec: &str) -> String {
    let Some((browser, rest)) = spec.split_once(':') else {
        return spec.to_string();
    };

    // Only attempt to adjust Chromium/Chrome specs. Other browsers (e.g. firefox) have
    // different layouts.
    let browser_lower = browser.to_ascii_lowercase();
    if !browser_lower.starts_with("chromium") && !browser_lower.starts_with("chrome") {
        return spec.to_string();
    }

    let (profile_raw, container_suffix) = match rest.split_once("::") {
        Some((profile, container)) => (profile, Some(container)),
        None => (rest, None),
    };

    let profile_raw = profile_raw.trim();
    if profile_raw.is_empty() {
        return spec.to_string();
    }

    let profile_path = Path::new(profile_raw);
    if !profile_path.is_absolute() {
        return spec.to_string();
    }

    // Chromium stores cookies DB either directly in the profile dir as `Cookies`,
    // or in `Network/Cookies` for newer versions.
    let cookies_db_present =
        |dir: &Path| dir.join("Cookies").is_file() || dir.join("Network").join("Cookies").is_file();

    if cookies_db_present(profile_path) {
        return spec.to_string();
    }

    // Common pitfall: passing a Chromium *user-data-dir* (which contains a `Default/`
    // profile directory), while yt-dlp expects the actual profile directory.
    let default_profile = profile_path.join("Default");
    if cookies_db_present(&default_profile) {
        let mut new_spec = format!("{}:{}", browser, default_profile.to_string_lossy());
        if let Some(container) = container_suffix {
            new_spec.push_str("::");
            new_spec.push_str(container);
        }
        info!(
            provided = %profile_path.display(),
            using = %default_profile.display(),
            "Chromium cookies DB not found in provided profile path; treating it as user-data-dir and using the Default profile"
        );
        return new_spec;
    }

    warn!(
        provided = %profile_path.display(),
        "Chromium cookies DB not found under provided profile path (expected Cookies or Network/Cookies). If this is a user-data-dir, try pointing YT_DLP_COOKIES_FROM_BROWSER at .../Default."
    );
    spec.to_string()
}

/// Extract and process comments from yt-dlp info.json.
///
/// Transforms yt-dlp comment format into our standardized JSON schema.
/// Applies comment count limits from config.
///
/// # Errors
///
/// Returns an error if file I/O fails or JSON is malformed.
async fn extract_comments_from_info_json(
    info_json_path: &Path,
    platform: &str,
    config: &Config,
) -> Result<Option<std::path::PathBuf>> {
    // Read and parse info.json
    let json_content = tokio::fs::read_to_string(info_json_path)
        .await
        .context("Failed to read info.json for comment extraction")?;

    let metadata: serde_json::Value =
        serde_json::from_str(&json_content).context("Failed to parse info.json for comments")?;

    // Check if comments are present
    let comments_array = match metadata.get("comments") {
        Some(serde_json::Value::Array(arr)) if !arr.is_empty() => arr,
        _ => {
            debug!("No comments found in metadata");
            return Ok(None);
        }
    };

    let total_comments = comments_array.len();
    info!("Found {} comments in yt-dlp metadata", total_comments);

    // Transform comments to our schema, applying limit
    let mut processed_comments = Vec::new();
    for (idx, comment) in comments_array.iter().enumerate() {
        if idx >= config.comments_max_count {
            debug!(
                "Reached comment limit ({}/{}), truncating",
                config.comments_max_count, total_comments
            );
            break;
        }

        // Extract comment fields with fallbacks
        let comment_obj = serde_json::json!({
            "id": comment.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            "author": comment.get("author").and_then(|v| v.as_str()).unwrap_or("unknown"),
            "author_id": comment.get("author_id").and_then(|v| v.as_str()),
            "text": comment.get("text").and_then(|v| v.as_str()).unwrap_or(""),
            "timestamp": comment.get("timestamp").and_then(|v| v.as_i64()),
            "likes": comment.get("like_count").and_then(|v| v.as_i64()).unwrap_or(0),
            "is_pinned": comment.get("is_pinned").and_then(|v| v.as_bool()).unwrap_or(false),
            "is_creator": comment.get("author_is_uploader").and_then(|v| v.as_bool()).unwrap_or(false),
            "parent_id": comment.get("parent").and_then(|v| v.as_str()).unwrap_or("root"),
            "replies": [],  // yt-dlp typically doesn't nest replies in the JSON
        });

        processed_comments.push(comment_obj);
    }

    let extracted_count = processed_comments.len();
    let limited = total_comments > config.comments_max_count;

    // Calculate basic stats
    let top_level_count = processed_comments
        .iter()
        .filter(|c| c.get("parent_id").and_then(|v| v.as_str()) == Some("root"))
        .count();

    // Build output JSON in our standard schema
    let output = serde_json::json!({
        "platform": platform,
        "extraction_method": "ytdlp",
        "extracted_at": chrono::Utc::now().to_rfc3339(),
        "content_url": metadata.get("webpage_url").and_then(|v| v.as_str()).unwrap_or(""),
        "content_id": metadata.get("id").and_then(|v| v.as_str()).unwrap_or(""),
        "limited": limited,
        "limit_applied": config.comments_max_count,
        "stats": {
            "total_comments": total_comments,
            "extracted_comments": extracted_count,
            "top_level_comments": top_level_count,
            "max_depth": 1,  // yt-dlp doesn't provide nesting depth info
        },
        "comments": processed_comments,
    });

    // Write comments.json to work directory
    let comments_path = info_json_path.with_file_name("comments.json");
    let json_str =
        serde_json::to_string_pretty(&output).context("Failed to serialize comments JSON")?;

    tokio::fs::write(&comments_path, json_str)
        .await
        .context("Failed to write comments.json")?;

    info!(
        path = %comments_path.display(),
        count = extracted_count,
        limited = limited,
        "Extracted comments to JSON file"
    );

    Ok(Some(comments_path))
}

/// Find and parse the info.json metadata file.
async fn find_and_parse_metadata(work_dir: &Path, config: &Config) -> Result<ArchiveResult> {
    let mut entries = tokio::fs::read_dir(work_dir)
        .await
        .context("Failed to read work directory")?;

    let mut info_file = None;
    let mut video_file = None;
    let mut thumb_file = None;
    let mut extra_files = Vec::new();
    let mut subtitle_files = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();

        if name.ends_with(".info.json") {
            info_file = Some(path);
        } else if is_video_file(&name) {
            video_file = Some(name.to_string());
        } else if is_thumbnail(&name) {
            thumb_file = Some(name.to_string());
        } else if is_subtitle_file(&name) {
            subtitle_files.push(name.to_string());
            // Keep subtitles in extra_files for backward compatibility
            extra_files.push(name.to_string());
        }
    }

    // Sanitize and rename video file if found
    if let Some(ref orig_name) = video_file {
        let sanitized = crate::archiver::sanitize_filename(orig_name);
        if sanitized != *orig_name {
            let orig_path = work_dir.join(orig_name);
            let new_path = work_dir.join(&sanitized);
            if let Err(e) = tokio::fs::rename(&orig_path, &new_path).await {
                warn!(
                    original = %orig_name,
                    sanitized = %sanitized,
                    error = %e,
                    "Failed to rename video file to sanitized name, keeping original"
                );
            } else {
                debug!(
                    original = %orig_name,
                    sanitized = %sanitized,
                    "Renamed video file to sanitized name"
                );
                video_file = Some(sanitized);
            }
        }
    }

    // Sanitize and rename thumbnail if found
    if let Some(ref orig_name) = thumb_file {
        let sanitized = crate::archiver::sanitize_filename(orig_name);
        if sanitized != *orig_name {
            let orig_path = work_dir.join(orig_name);
            let new_path = work_dir.join(&sanitized);
            if let Err(e) = tokio::fs::rename(&orig_path, &new_path).await {
                warn!(
                    original = %orig_name,
                    sanitized = %sanitized,
                    error = %e,
                    "Failed to rename thumbnail to sanitized name, keeping original"
                );
            } else {
                debug!(
                    original = %orig_name,
                    sanitized = %sanitized,
                    "Renamed thumbnail to sanitized name"
                );
                thumb_file = Some(sanitized);
            }
        }
    }

    // Sanitize and rename extra files
    let mut sanitized_extra_files = Vec::new();
    for orig_name in extra_files {
        let sanitized = crate::archiver::sanitize_filename(&orig_name);
        if sanitized != orig_name {
            let orig_path = work_dir.join(&orig_name);
            let new_path = work_dir.join(&sanitized);
            if let Err(e) = tokio::fs::rename(&orig_path, &new_path).await {
                warn!(
                    original = %orig_name,
                    sanitized = %sanitized,
                    error = %e,
                    "Failed to rename extra file to sanitized name, keeping original"
                );
                sanitized_extra_files.push(orig_name);
            } else {
                debug!(
                    original = %orig_name,
                    sanitized = %sanitized,
                    "Renamed extra file to sanitized name"
                );
                sanitized_extra_files.push(sanitized);
            }
        } else {
            sanitized_extra_files.push(orig_name);
        }
    }
    let extra_files = sanitized_extra_files;

    let mut result = ArchiveResult {
        content_type: "video".to_string(),
        primary_file: video_file,
        thumbnail: thumb_file,
        extra_files,
        ..Default::default()
    };

    // Parse info.json if found
    if let Some(info_path) = info_file {
        let content = tokio::fs::read_to_string(&info_path)
            .await
            .context("Failed to read info.json")?;

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            result.title = json.get("title").and_then(|v| v.as_str()).map(String::from);
            result.author = json
                .get("uploader")
                .or_else(|| json.get("channel"))
                .and_then(|v| v.as_str())
                .map(String::from);
            result.text = json
                .get("description")
                .and_then(|v| v.as_str())
                .map(String::from);

            // Extract NSFW status from age_limit field
            // age_limit >= 18 indicates adult/NSFW content
            if let Some(age_limit) = json.get("age_limit").and_then(serde_json::Value::as_i64) {
                if age_limit >= 18 {
                    result.is_nsfw = Some(true);
                    result.nsfw_source = Some("metadata".to_string());
                }
            }

            // Also check Reddit's over_18 field (for Reddit-specific NSFW)
            if result.is_nsfw.is_none() || !result.is_nsfw.unwrap_or(false) {
                if let Some(over_18) = json.get("over_18").and_then(serde_json::Value::as_bool) {
                    if over_18 {
                        result.is_nsfw = Some(true);
                        result.nsfw_source = Some("metadata".to_string());
                    }
                }
            }

            result.metadata_json = Some(content);

            // Comment extraction is now handled by the dedicated comment worker
            // after the main archive completes, to avoid blocking other archives
        }
    }

    Ok(result)
}

fn is_video_file(name: &str) -> bool {
    let video_exts = [".mp4", ".webm", ".mkv", ".avi", ".mov", ".flv"];
    video_exts.iter().any(|ext| name.ends_with(ext))
}

fn is_thumbnail(name: &str) -> bool {
    let thumb_exts = [".jpg", ".jpeg", ".png", ".webp"];
    thumb_exts.iter().any(|ext| name.ends_with(ext))
        && (name.contains("thumb") || name.contains("thumbnail"))
}

fn is_subtitle_file(name: &str) -> bool {
    name.ends_with(".vtt") || name.ends_with(".srt")
}

/// Parse subtitle file information from filename.
///
/// yt-dlp names subtitle files with patterns like:
/// - `{title}.{lang}.vtt` (manual subtitles)
/// - `{title}.{lang}.{format}.vtt` (auto-generated, format like "auto")
///
/// Returns (language, is_auto, format).
pub fn parse_subtitle_info(filename: &str) -> (String, bool, String) {
    // Remove file extension
    let stem = filename
        .strip_suffix(".vtt")
        .or_else(|| filename.strip_suffix(".srt"))
        .unwrap_or(filename);

    // Split by dots to get components
    let parts: Vec<&str> = stem.split('.').collect();

    if parts.len() < 2 {
        return ("unknown".to_string(), false, "vtt".to_string());
    }

    let format = if filename.ends_with(".vtt") {
        "vtt"
    } else {
        "srt"
    };

    // Last part is usually the language code
    let lang = parts[parts.len() - 1];

    // If there's a middle part, it might indicate auto-generated
    // yt-dlp typically uses patterns like "en.auto" or "en-US"
    let is_auto =
        parts.len() > 2 && (parts[parts.len() - 2].contains("auto") || stem.contains("-auto"));

    (lang.to_string(), is_auto, format.to_string())
}

/// Check if yt-dlp is available.
pub async fn is_available() -> bool {
    Command::new("yt-dlp")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Fetch metadata only without downloading the video.
///
/// This is useful when we already have the video file and just need the title/author info.
/// Uses `yt-dlp --dump-single-json` to get metadata.
pub async fn fetch_metadata_only(url: &str, cookies: &CookieOptions<'_>) -> Result<ArchiveResult> {
    let mut args = vec![
        "-4".to_string(),
        "--no-playlist".to_string(),
        "--dump-single-json".to_string(),
        "--no-download".to_string(),
        "--quiet".to_string(),
    ];

    // Prefer browser profile over cookies file (fresher cookies)
    let mut cookie_method_used = false;

    if let Some(spec) = cookies.browser_profile {
        let spec = maybe_adjust_chromium_user_data_dir_spec(spec);
        debug!(spec = %spec, "Using cookies from browser profile for metadata fetch");
        args.push("--cookies-from-browser".to_string());
        args.push(spec);
        cookie_method_used = true;
    }

    if !cookie_method_used {
        if let Some(cookies_path) = cookies.cookies_file {
            if cookies_path.exists() && !cookies_path.is_dir() {
                debug!(path = %cookies_path.display(), "Using cookies file for metadata fetch");
                args.push("--cookies".to_string());
                args.push(cookies_path.to_string_lossy().to_string());
            }
        }
    }

    args.push(url.to_string());

    debug!(url = %url, "Fetching metadata with yt-dlp");

    let output = Command::new("yt-dlp")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn yt-dlp for metadata")?
        .wait_with_output()
        .await
        .context("Failed to wait for yt-dlp metadata fetch")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("yt-dlp metadata fetch failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse the JSON output
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
        let title = json.get("title").and_then(|v| v.as_str()).map(String::from);
        let author = json
            .get("uploader")
            .or_else(|| json.get("channel"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let text = json
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Extract NSFW status from age_limit or over_18 field
        let (is_nsfw, nsfw_source) = {
            // Check age_limit first (common for video platforms)
            if let Some(age_limit) = json.get("age_limit").and_then(serde_json::Value::as_i64) {
                if age_limit >= 18 {
                    (Some(true), Some("metadata".to_string()))
                } else {
                    // Check Reddit's over_18 field
                    if let Some(over_18) = json.get("over_18").and_then(serde_json::Value::as_bool)
                    {
                        if over_18 {
                            (Some(true), Some("metadata".to_string()))
                        } else {
                            (None, None)
                        }
                    } else {
                        (None, None)
                    }
                }
            } else if let Some(over_18) = json.get("over_18").and_then(serde_json::Value::as_bool) {
                // Check Reddit's over_18 field if age_limit not present
                if over_18 {
                    (Some(true), Some("metadata".to_string()))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            }
        };

        Ok(ArchiveResult {
            title,
            author,
            text,
            content_type: "video".to_string(),
            is_nsfw,
            nsfw_source,
            metadata_json: Some(stdout.to_string()),
            ..Default::default()
        })
    } else {
        // If JSON parsing fails, return a minimal result
        warn!(url = %url, "Failed to parse yt-dlp metadata JSON");
        Ok(ArchiveResult {
            content_type: "video".to_string(),
            ..Default::default()
        })
    }
}

/// Extract comments only without downloading video.
///
/// This function downloads only the comments for a video/post using yt-dlp's
/// --skip-download flag. Useful for background comment extraction jobs.
///
/// Returns the number of comments extracted.
///
/// # Errors
///
/// Returns an error if yt-dlp fails or comments cannot be extracted.
pub async fn extract_comments_only(
    url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
    config: &Config,
    archive_id: Option<i64>,
    pool: Option<&SqlitePool>,
) -> Result<usize> {
    let mut args = vec![
        "-4".to_string(),
        "--no-playlist".to_string(),
        "--skip-download".to_string(), // Don't download the video
        "--write-info-json".to_string(),
        "--write-comments".to_string(),
        "--output".to_string(),
        work_dir
            .join("%(title)s.%(ext)s")
            .to_string_lossy()
            .to_string(),
        // Use --newline for parseable progress output
        "--newline".to_string(),
    ];

    // Add extractor args for comment limits
    args.push("--extractor-args".to_string());
    args.push(format!(
        "youtube:max_comments={};comment_sort=top",
        config.comments_max_count
    ));

    // Add delay between comment API requests to avoid rate limiting
    let delay_secs = config.comments_request_delay_ms as f64 / 1000.0;
    args.push("--sleep-requests".to_string());
    args.push(format!("{:.1}", delay_secs));

    // Add cookie options
    if let Some(spec) = cookies.browser_profile {
        let spec = maybe_adjust_chromium_user_data_dir_spec(spec);
        args.push("--cookies-from-browser".to_string());
        args.push(spec);
    } else if let Some(cookies_path) = cookies.cookies_file {
        if cookies_path.exists() && !cookies_path.is_dir() {
            args.push("--cookies".to_string());
            args.push(cookies_path.to_string_lossy().to_string());
        }
    }

    args.push(url.to_string());

    debug!(url = %url, "Extracting comments with yt-dlp");

    // Spawn yt-dlp and capture stdout/stderr for progress tracking
    let mut child = Command::new("yt-dlp")
        .args(&args)
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn yt-dlp for comment extraction")?;

    let stdout = child.stdout.take().expect("Failed to open stdout");
    let stderr = child.stderr.take().expect("Failed to open stderr");

    let stderr_reader = BufReader::new(stderr);
    let stdout_reader = BufReader::new(stdout);

    // Stream stderr for log output
    let stderr_handle = tokio::spawn(async move {
        let mut lines = stderr_reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            debug!("yt-dlp: {line}");
        }
    });

    // Stream stdout for progress tracking
    let archive_id_opt = archive_id;
    let pool_opt = pool.cloned();
    let max_comments = config.comments_max_count;
    let stdout_handle = tokio::spawn(async move {
        let mut lines = stdout_reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            debug!("yt-dlp: {line}");

            // Update progress if we have pool and archive_id
            if let (Some(id), Some(p)) = (archive_id_opt, &pool_opt) {
                // Parse yt-dlp progress output for comment downloads
                // yt-dlp outputs: "[youtube] Downloading comment API JSON page N (X/~Y)"
                if line.contains("Downloading comment") {
                    if let Some(count_part) = line.split('(').nth(1) {
                        if let Some(current_str) = count_part.split('/').next() {
                            if let Ok(current) = current_str.parse::<u64>() {
                                // Estimate progress based on comment count vs max
                                let estimated_total = max_comments.max(current as usize) as u64;
                                let progress_percent =
                                    (current as f64 / estimated_total as f64 * 100.0).min(100.0);

                                let details = serde_json::json!({
                                    "comments_downloaded": current,
                                    "estimated_total": estimated_total,
                                    "stage": "downloading_comments"
                                });

                                if let Err(e) = crate::db::update_archive_progress(
                                    &p,
                                    id,
                                    progress_percent,
                                    &details.to_string(),
                                )
                                .await
                                {
                                    warn!("Failed to update comment extraction progress: {e}");
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    // Wait for yt-dlp to finish with timeout
    let timeout = Duration::from_secs(config.youtube_download_timeout_seconds);
    let status = tokio::time::timeout(timeout, child.wait())
        .await
        .context("Comment extraction timed out")??;

    // Wait for stream handlers to finish
    let _ = tokio::join!(stderr_handle, stdout_handle);

    if !status.success() {
        anyhow::bail!("yt-dlp comment extraction failed with status: {status}");
    }

    // Find and parse the comments.json file
    let mut comment_count = 0;
    for entry in std::fs::read_dir(work_dir).context("Failed to read work directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json")
            && path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.ends_with(".info.json"))
                .unwrap_or(false)
        {
            // Extract comments from info.json
            if let Ok(Some(_)) = extract_comments_from_info_json(&path, "youtube", config).await {
                // Read the generated comments.json to get the count
                let comments_json_path = path.with_file_name("comments.json");
                if comments_json_path.exists() {
                    if let Ok(content) = tokio::fs::read_to_string(&comments_json_path).await {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(count) = json
                                .get("stats")
                                .and_then(|s| s.get("extracted_comments"))
                                .and_then(|c| c.as_u64())
                            {
                                comment_count = count as usize;
                                info!("Extracted {comment_count} comments from {}", path.display());
                            }
                        }
                    }
                }
                break;
            }
        }
    }

    if comment_count == 0 {
        warn!("No comments were extracted from {url}");
    }

    Ok(comment_count)
}
