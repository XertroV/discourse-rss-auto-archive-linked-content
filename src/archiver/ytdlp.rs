use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::{debug, warn};

use super::CookieOptions;
use crate::handlers::ArchiveResult;

/// Download content using yt-dlp.
///
/// If both browser_profile and cookies_file are provided, browser_profile is preferred
/// as it typically provides fresher cookies.
///
/// # Errors
///
/// Returns an error if yt-dlp fails or times out.
pub async fn download(
    url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
) -> Result<ArchiveResult> {
    let output_template = work_dir.join("%(title)s.%(ext)s");

    let mut args = vec![
        "-4".to_string(),
        "--no-playlist".to_string(),
        "--write-info-json".to_string(),
        "--write-thumbnail".to_string(),
        "--write-subs".to_string(),
        "--sub-langs".to_string(),
        "en".to_string(),
        "--output".to_string(),
        output_template.to_string_lossy().to_string(),
        "--no-progress".to_string(),
        "--quiet".to_string(),
        // Format selection: prefer reasonable quality
        "--format".to_string(),
        "bestvideo[height<=1080]+bestaudio/best[height<=1080]/best".to_string(),
    ];

    // Prefer browser profile over cookies file (fresher cookies)
    // Only use one method to avoid potential conflicts
    let mut cookie_method_used = false;

    if let Some(ref spec) = cookies.browser_profile {
        debug!(spec = %spec, "Using cookies from browser profile");
        args.push("--cookies-from-browser".to_string());
        args.push(spec.to_string());
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

    let output = Command::new("yt-dlp")
        .args(&args)
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn yt-dlp")?
        .wait_with_output()
        .await
        .context("Failed to wait for yt-dlp")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("yt-dlp failed: {stderr}");
    }

    // Find the info.json file to get metadata
    let metadata = find_and_parse_metadata(work_dir).await?;

    Ok(metadata)
}

/// Find and parse the info.json metadata file.
async fn find_and_parse_metadata(work_dir: &Path) -> Result<ArchiveResult> {
    let mut entries = tokio::fs::read_dir(work_dir)
        .await
        .context("Failed to read work directory")?;

    let mut info_file = None;
    let mut video_file = None;
    let mut thumb_file = None;
    let mut extra_files = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();

        if name.ends_with(".info.json") {
            info_file = Some(path);
        } else if is_video_file(&name) {
            video_file = Some(name.to_string());
        } else if is_thumbnail(&name) {
            thumb_file = Some(name.to_string());
        } else if name.ends_with(".vtt") || name.ends_with(".srt") {
            extra_files.push(name.to_string());
        }
    }

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

            result.metadata_json = Some(content);
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
