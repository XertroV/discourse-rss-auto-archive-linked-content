use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::{debug, warn};

use super::CookieOptions;
use crate::config::Config;
use crate::handlers::ArchiveResult;

/// Information about a single video in a playlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistVideoInfo {
    /// Video ID (e.g., YouTube video ID)
    pub id: String,
    /// Video title
    pub title: String,
    /// Video URL
    pub url: String,
    /// Uploader/channel name
    pub uploader: Option<String>,
    /// Upload/publish date (ISO format)
    pub upload_date: Option<String>,
    /// Video duration in seconds
    pub duration: Option<i32>,
}

/// Information about a YouTube playlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistInfo {
    /// Playlist ID
    pub id: String,
    /// Playlist title
    pub title: String,
    /// Playlist URL
    pub url: String,
    /// Playlist uploader/channel
    pub uploader: Option<String>,
    /// Total number of videos
    pub video_count: i32,
    /// List of videos in the playlist
    pub videos: Vec<PlaylistVideoInfo>,
}

/// Extract playlist metadata using yt-dlp.
///
/// # Errors
///
/// Returns an error if yt-dlp fails or the response cannot be parsed.
async fn get_playlist_metadata(url: &str, cookies: &CookieOptions<'_>) -> Result<PlaylistInfo> {
    let mut args = vec![
        "--dump-json".to_string(),
        "--flat-playlist".to_string(),
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

    debug!(url = %url, "Fetching YouTube playlist metadata");

    let output = Command::new("yt-dlp")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn yt-dlp for playlist metadata")?
        .wait_with_output()
        .await
        .context("Failed to wait for yt-dlp playlist metadata")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("yt-dlp playlist metadata fetch failed: {stderr}");
    }

    // Parse the JSON output from yt-dlp
    let playlist_json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("Failed to parse yt-dlp playlist JSON")?;

    // Extract playlist information
    let playlist_id = playlist_json
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let playlist_title = playlist_json
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled Playlist")
        .to_string();

    let playlist_url = playlist_json
        .get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let uploader = playlist_json
        .get("uploader")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Extract videos from the entries
    let mut videos = Vec::new();
    if let Some(entries) = playlist_json.get("entries").and_then(|v| v.as_array()) {
        for entry in entries {
            let video_id = entry
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            let title = entry
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled Video")
                .to_string();

            let video_url = entry
                .get("url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={}", video_id));

            let uploader_name = entry
                .get("uploader")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let upload_date = entry.get("upload_date").and_then(|v| v.as_str()).map(|s| {
                // Convert YYYYMMDD format to ISO format YYYY-MM-DD
                if s.len() == 8 {
                    format!("{}-{}-{}", &s[0..4], &s[4..6], &s[6..8])
                } else {
                    s.to_string()
                }
            });

            let duration = entry
                .get("duration")
                .and_then(|v| v.as_i64())
                .map(|d| d as i32);

            videos.push(PlaylistVideoInfo {
                id: video_id,
                title,
                url: video_url,
                uploader: uploader_name,
                upload_date,
                duration,
            });
        }
    }

    let video_count = videos.len() as i32;

    Ok(PlaylistInfo {
        id: playlist_id,
        title: playlist_title,
        url: playlist_url.unwrap_or_else(|| url.to_string()),
        uploader,
        video_count,
        videos,
    })
}

/// Adjust chromium user data directory spec for yt-dlp.
/// See: https://github.com/yt-dlp/yt-dlp/wiki/Configuration#authentication-using-cookies
fn maybe_adjust_chromium_user_data_dir_spec(spec: &str) -> String {
    // Specs like "chromium+basictext:/path/to/profile" need to stay as-is
    // Single path arguments might need the prefix
    if spec.contains('+') || spec.contains(':') {
        spec.to_string()
    } else {
        // If just a path is provided, add chromium+basictext prefix
        format!("chromium+basictext:{spec}")
    }
}

/// Archive a YouTube playlist.
///
/// Extracts playlist metadata without downloading any videos and stores
/// the information as JSON in the archive result.
///
/// # Errors
///
/// Returns an error if yt-dlp fails or metadata extraction fails.
pub async fn archive_playlist(
    url: &str,
    _work_dir: &Path,
    cookies: &CookieOptions<'_>,
    _config: &Config,
    playlist_id: &str,
) -> Result<ArchiveResult> {
    debug!(url = %url, playlist_id = %playlist_id, "Archiving YouTube playlist");

    // Fetch playlist metadata
    let playlist_info = match get_playlist_metadata(url, cookies).await {
        Ok(info) => info,
        Err(e) => {
            warn!("Failed to fetch playlist metadata: {e}");
            anyhow::bail!("Failed to fetch YouTube playlist metadata: {e}");
        }
    };

    // Serialize playlist info as JSON
    let metadata_json = serde_json::to_string_pretty(&playlist_info)
        .context("Failed to serialize playlist metadata")?;

    debug!(
        playlist_id = %playlist_id,
        video_count = playlist_info.video_count,
        "Successfully extracted playlist metadata"
    );

    Ok(ArchiveResult {
        title: Some(playlist_info.title.clone()),
        author: playlist_info.uploader.clone(),
        text: Some(metadata_json),
        content_type: "playlist".to_string(),
        primary_file: None,
        thumbnail: None,
        extra_files: Vec::new(),
        metadata_json: Some(serde_json::to_string(&playlist_info)?),
        is_nsfw: Some(false),
        nsfw_source: None,
        final_url: None,
        video_id: Some(playlist_id.to_string()),
        http_status_code: Some(200),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chromium_user_data_dir_spec() {
        assert_eq!(
            maybe_adjust_chromium_user_data_dir_spec("/path/to/profile"),
            "chromium+basictext:/path/to/profile"
        );
        assert_eq!(
            maybe_adjust_chromium_user_data_dir_spec("chromium+basictext:/path/to/profile"),
            "chromium+basictext:/path/to/profile"
        );
    }
}
