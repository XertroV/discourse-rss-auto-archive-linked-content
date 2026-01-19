use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::{debug, warn};

use super::CookieOptions;
use crate::handlers::ArchiveResult;

/// Download content using gallery-dl.
///
/// Note: gallery-dl only supports cookies.txt files, not browser profiles.
/// If only browser_profile is set in CookieOptions, no cookies will be used.
///
/// # Errors
///
/// Returns an error if gallery-dl fails or times out.
pub async fn download(
    url: &str,
    work_dir: &Path,
    cookies: &CookieOptions<'_>,
) -> Result<ArchiveResult> {
    let mut args = vec![
        url.to_string(),
        "--directory".to_string(),
        work_dir.to_string_lossy().to_string(),
        "--write-metadata".to_string(),
        "--write-info-json".to_string(),
        // Use flat directory structure
        "--filename".to_string(),
        "{category}_{filename}.{extension}".to_string(),
        "--no-mtime".to_string(),
    ];

    // gallery-dl only supports cookies files, not browser profiles
    if let Some(cookies_path) = cookies.cookies_file {
        if !cookies_path.exists() {
            warn!(path = %cookies_path.display(), "Cookies file specified but does not exist, continuing without cookies");
        } else if cookies_path.is_dir() {
            warn!(path = %cookies_path.display(), "Cookies path is a directory, continuing without cookies");
        } else {
            debug!(path = %cookies_path.display(), "Using cookies file for gallery-dl download");
            args.push("--cookies".to_string());
            args.push(cookies_path.to_string_lossy().to_string());
        }
    } else if cookies.browser_profile.is_some() {
        // Log that browser profile isn't supported by gallery-dl
        debug!("Browser profile configured but gallery-dl only supports cookies files; proceeding without cookies");
    }

    debug!(url = %url, "Running gallery-dl");

    let output = Command::new("gallery-dl")
        .args(&args)
        .current_dir(work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn gallery-dl")?
        .wait_with_output()
        .await
        .context("Failed to wait for gallery-dl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gallery-dl failed: {stderr}");
    }

    // Find downloaded files and metadata
    let result = find_and_parse_files(work_dir).await?;

    Ok(result)
}

/// Find downloaded files and parse metadata.
async fn find_and_parse_files(work_dir: &Path) -> Result<ArchiveResult> {
    let mut entries = tokio::fs::read_dir(work_dir)
        .await
        .context("Failed to read work directory")?;

    let mut primary_file = None;
    let mut extra_files = Vec::new();
    let mut metadata_json = None;
    let mut title = None;
    let mut author = None;
    let mut description = None;
    let mut content_type = "image".to_string();

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();

        if name.ends_with(".json") {
            // Try to parse metadata from JSON files
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    // Extract metadata from gallery-dl JSON
                    if title.is_none() {
                        title = json
                            .get("title")
                            .or_else(|| json.get("description"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.chars().take(200).collect());
                    }
                    if author.is_none() {
                        author = json
                            .get("uploader")
                            .or_else(|| json.get("username"))
                            .or_else(|| json.get("owner"))
                            .or_else(|| json.get("user"))
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                    if description.is_none() {
                        description = json
                            .get("description")
                            .or_else(|| json.get("content"))
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                    if metadata_json.is_none() {
                        metadata_json = Some(content);
                    }
                }
            }
        } else if is_image_file(&name) {
            if primary_file.is_none() {
                primary_file = Some(name.to_string());
            } else {
                extra_files.push(name.to_string());
            }
        } else if is_video_file(&name) {
            // Videos take priority over images
            if primary_file.is_none() || !is_video_file(primary_file.as_deref().unwrap_or("")) {
                if let Some(prev) = primary_file.take() {
                    extra_files.push(prev);
                }
                primary_file = Some(name.to_string());
                content_type = "video".to_string();
            } else {
                extra_files.push(name.to_string());
            }
        }
    }

    // Sanitize and rename primary file if found
    if let Some(ref orig_name) = primary_file {
        let sanitized = crate::archiver::sanitize_filename(orig_name);
        if sanitized != *orig_name {
            let orig_path = work_dir.join(orig_name);
            let new_path = work_dir.join(&sanitized);
            if let Err(e) = tokio::fs::rename(&orig_path, &new_path).await {
                warn!(
                    original = %orig_name,
                    sanitized = %sanitized,
                    error = %e,
                    "Failed to rename primary file to sanitized name, keeping original"
                );
            } else {
                debug!(
                    original = %orig_name,
                    sanitized = %sanitized,
                    "Renamed primary file to sanitized name"
                );
                primary_file = Some(sanitized);
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

    // Determine content type based on file count
    if extra_files.len() > 1 {
        content_type = "gallery".to_string();
    }

    Ok(ArchiveResult {
        title,
        author,
        text: description,
        content_type,
        primary_file,
        thumbnail: None, // gallery-dl doesn't generate separate thumbnails
        extra_files,
        metadata_json,
        is_nsfw: None,
        nsfw_source: None,
        final_url: None,
        video_id: None,
        http_status_code: None,
    })
}

fn is_image_file(name: &str) -> bool {
    let image_exts = [
        ".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".tiff", ".svg",
    ];
    let lower = name.to_lowercase();
    image_exts.iter().any(|ext| lower.ends_with(ext))
}

fn is_video_file(name: &str) -> bool {
    let video_exts = [".mp4", ".webm", ".mkv", ".avi", ".mov", ".flv", ".m4v"];
    let lower = name.to_lowercase();
    video_exts.iter().any(|ext| lower.ends_with(ext))
}

/// Check if gallery-dl is available.
pub async fn is_available() -> bool {
    Command::new("gallery-dl")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_image_file() {
        assert!(is_image_file("test.jpg"));
        assert!(is_image_file("test.PNG"));
        assert!(is_image_file("test.webp"));
        assert!(!is_image_file("test.mp4"));
        assert!(!is_image_file("test.json"));
    }

    #[test]
    fn test_is_video_file() {
        assert!(is_video_file("test.mp4"));
        assert!(is_video_file("test.WEBM"));
        assert!(!is_video_file("test.jpg"));
        assert!(!is_video_file("test.json"));
    }
}
