//! TikTok comment extraction using TikTok's public API.
//!
//! This module extracts comments from TikTok videos via direct API calls to TikTok's comment endpoint.
//! It fetches comments in batches of 50 (API limit) and paginates until reaching the configured limit.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// TikTok API response for comment listing
#[derive(Debug, Deserialize)]
struct TikTokCommentResponse {
    #[serde(default)]
    comments: Vec<TikTokComment>,
    #[serde(default)]
    has_more: bool,
    #[serde(default)]
    cursor: u64,
    #[serde(default)]
    status_code: i32,
    #[serde(default)]
    status_msg: String,
}

/// Individual comment from TikTok API
#[derive(Debug, Deserialize)]
struct TikTokComment {
    cid: String,
    text: String,
    #[serde(default)]
    create_time: i64,
    #[serde(default)]
    digg_count: u64,
    user: TikTokUser,
    #[serde(default)]
    #[allow(dead_code)]
    reply_comment_total: u64,
}

#[derive(Debug, Deserialize)]
struct TikTokUser {
    #[serde(default)]
    unique_id: String,
    #[serde(default)]
    uid: String,
}

/// Extract video ID from TikTok URL.
///
/// Supports formats:
/// - https://www.tiktok.com/@username/video/7123456789
/// - https://m.tiktok.com/v/7123456789
/// - https://vm.tiktok.com/ABCD123/ (redirects to full URL)
pub fn extract_video_id(url: &str) -> Option<String> {
    // Only process TikTok URLs
    if !url.contains("tiktok.com") {
        return None;
    }

    // Standard format: /video/ID
    if let Some(pos) = url.find("/video/") {
        let id_start = pos + 7; // "/video/".len()
        let id_end = url[id_start..]
            .find(|c: char| !c.is_ascii_digit())
            .map(|i| id_start + i)
            .unwrap_or(url.len());
        let id = &url[id_start..id_end];
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
            return Some(id.to_string());
        }
    }

    // Mobile short format: /v/ID
    if let Some(pos) = url.find("/v/") {
        let id_start = pos + 3;
        let id_end = url[id_start..]
            .find(|c: char| !c.is_ascii_digit())
            .map(|i| id_start + i)
            .unwrap_or(url.len());
        let id = &url[id_start..id_end];
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
            return Some(id.to_string());
        }
    }

    None
}

/// Fetch a single batch of comments from TikTok API.
async fn fetch_comments_batch(
    client: &Client,
    video_id: &str,
    cursor: u64,
) -> Result<TikTokCommentResponse> {
    let url = format!(
        "https://www.tiktok.com/api/comment/list/?aid=1988&aweme_id={}&count=50&cursor={}",
        video_id, cursor
    );

    let response = client
        .get(&url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        )
        .header("Referer", format!("https://www.tiktok.com/@i/video/{}", video_id))
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("Failed to send request to TikTok API")?;

    if !response.status().is_success() {
        anyhow::bail!("TikTok API returned status {}", response.status());
    }

    let data = response
        .json::<TikTokCommentResponse>()
        .await
        .context("Failed to parse TikTok API response")?;

    // Check for API errors
    if data.status_code != 0 {
        anyhow::bail!("TikTok API error: {}", data.status_msg);
    }

    Ok(data)
}

/// Deduplicate comments by ID.
///
/// TikTok API may return duplicate comments at page boundaries.
fn deduplicate_comments(comments: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut seen = HashSet::new();
    comments
        .into_iter()
        .filter(|c| {
            if let Some(id) = c.get("id").and_then(|v| v.as_str()) {
                seen.insert(id.to_string())
            } else {
                true // Keep comments without IDs (shouldn't happen)
            }
        })
        .collect()
}

/// Fetch TikTok comments for a video.
///
/// # Arguments
/// * `url` - TikTok video URL
/// * `limit` - Maximum number of comments to fetch
/// * `archive_id` - Optional archive ID for progress tracking
/// * `pool` - Optional database pool for progress updates
///
/// # Returns
/// Returns a JSON value containing comments in the standard schema, or an error if extraction fails.
pub async fn fetch_tiktok_comments(
    url: &str,
    limit: usize,
    archive_id: Option<i64>,
    pool: Option<&SqlitePool>,
) -> Result<serde_json::Value> {
    // Extract video ID from URL
    let video_id = extract_video_id(url)
        .ok_or_else(|| anyhow::anyhow!("Could not extract video ID from URL: {}", url))?;

    info!(
        video_id = %video_id,
        limit = limit,
        "Extracting TikTok comments"
    );

    let client = Client::new();
    let mut all_comments = Vec::new();
    let mut cursor = 0u64;
    let mut request_count = 0;
    let mut last_progress_update = Instant::now();
    let progress_update_interval = Duration::from_secs(10);

    loop {
        // Check if we've reached the limit
        if all_comments.len() >= limit {
            info!("Reached comment limit of {}", limit);
            break;
        }

        // Fetch a batch of comments
        request_count += 1;
        debug!(
            request = request_count,
            cursor = cursor,
            fetched = all_comments.len(),
            "Fetching comment batch"
        );

        let response = match fetch_comments_batch(&client, &video_id, cursor).await {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "Failed to fetch comment batch, stopping extraction");
                break;
            }
        };

        // Process comments from this batch
        let batch_size = response.comments.len();
        if batch_size == 0 {
            debug!("No comments in batch, reached end");
            break;
        }

        for comment in response.comments {
            if all_comments.len() >= limit {
                break;
            }

            // Transform to standard schema
            let author_id = if comment.user.uid.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(comment.user.uid.clone())
            };

            let comment_obj = serde_json::json!({
                "id": comment.cid,
                "author": if comment.user.unique_id.is_empty() {
                    &comment.user.uid
                } else {
                    &comment.user.unique_id
                },
                "author_id": author_id,
                "text": comment.text,
                "timestamp": comment.create_time,
                "likes": comment.digg_count,
                "is_pinned": false,  // TikTok API doesn't provide this
                "is_creator": false, // TikTok API doesn't provide this easily
                "parent_id": "root", // TikTok comments are flat in this API
                "replies": [],       // Not fetching nested replies
            });

            all_comments.push(comment_obj);
        }

        info!(
            batch = request_count,
            batch_size = batch_size,
            total = all_comments.len(),
            "Fetched comment batch"
        );

        // Update progress if needed
        if let (Some(aid), Some(p)) = (archive_id, pool) {
            if all_comments.len() % 250 == 0
                || last_progress_update.elapsed() >= progress_update_interval
            {
                let progress_percent =
                    (all_comments.len() as f64 / limit as f64 * 100.0).min(100.0);
                let progress_json = serde_json::json!({
                    "comments_downloaded": all_comments.len(),
                    "estimated_total": limit,
                    "stage": "fetching_comments"
                });

                if let Err(e) = crate::db::update_archive_progress(
                    p,
                    aid,
                    progress_percent,
                    &progress_json.to_string(),
                )
                .await
                {
                    warn!(error = %e, "Failed to update progress");
                }

                last_progress_update = Instant::now();
            }
        }

        // Check if there are more comments
        if !response.has_more {
            debug!("API reports no more comments available");
            break;
        }

        // Update cursor for next batch
        cursor = response.cursor;

        // Rate limiting: wait 500ms between requests
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let total_fetched = all_comments.len();

    // Deduplicate comments
    let deduplicated = deduplicate_comments(all_comments);
    let dedup_count = total_fetched - deduplicated.len();
    if dedup_count > 0 {
        info!(
            duplicates_removed = dedup_count,
            "Removed duplicate comments"
        );
    }

    let extracted_count = deduplicated.len();
    let limited = extracted_count >= limit;

    // Build output JSON in standard schema
    let output = serde_json::json!({
        "platform": "tiktok",
        "extraction_method": "api",
        "extracted_at": chrono::Utc::now().to_rfc3339(),
        "content_url": url,
        "content_id": video_id,
        "limited": limited,
        "limit_applied": limit,
        "stats": {
            "total_comments": extracted_count,
            "extracted_comments": extracted_count,
            "top_level_comments": extracted_count,
            "max_depth": 0,  // TikTok comments are flat
        },
        "comments": deduplicated,
    });

    info!(
        video_id = %video_id,
        comments = extracted_count,
        requests = request_count,
        "TikTok comment extraction completed"
    );

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_video_id_standard() {
        assert_eq!(
            extract_video_id("https://www.tiktok.com/@username/video/7123456789"),
            Some("7123456789".to_string())
        );
        assert_eq!(
            extract_video_id("https://www.tiktok.com/@user/video/7579937914797215031?foo=bar"),
            Some("7579937914797215031".to_string())
        );
    }

    #[test]
    fn test_extract_video_id_mobile() {
        assert_eq!(
            extract_video_id("https://m.tiktok.com/v/7123456789"),
            Some("7123456789".to_string())
        );
    }

    #[test]
    fn test_extract_video_id_invalid() {
        assert_eq!(extract_video_id("https://www.tiktok.com/@user"), None);
        assert_eq!(extract_video_id("https://example.com/video/123"), None);
    }

    #[test]
    fn test_deduplicate_comments() {
        let comments = vec![
            serde_json::json!({"id": "1", "text": "first"}),
            serde_json::json!({"id": "2", "text": "second"}),
            serde_json::json!({"id": "1", "text": "duplicate"}),
            serde_json::json!({"id": "3", "text": "third"}),
        ];

        let deduped = deduplicate_comments(comments);
        assert_eq!(deduped.len(), 3);

        // Check that we kept the first occurrence
        assert_eq!(deduped[0]["id"], "1");
        assert_eq!(deduped[0]["text"], "first");
        assert_eq!(deduped[1]["id"], "2");
        assert_eq!(deduped[2]["id"], "3");
    }

    /// Integration test: Download comments from a real TikTok video.
    ///
    /// This test is ignored by default and only runs when explicitly requested with:
    /// `cargo test --lib -- --ignored test_fetch_real_tiktok_comments`
    ///
    /// It's useful for verifying that the TikTok API hasn't changed and that
    /// deserialization works correctly with real API responses.
    #[tokio::test]
    #[ignore]
    async fn test_fetch_real_tiktok_comments() {
        let url = "https://www.tiktok.com/@sgj88764q/video/7579937914797215031";
        let limit = 100; // Fetch 100 comments for testing

        let result = fetch_tiktok_comments(url, limit, None, None).await;

        match result {
            Ok(json) => {
                println!("Successfully fetched comments!");
                println!("Platform: {}", json["platform"]);
                println!("Video ID: {}", json["content_id"]);
                println!("Comments fetched: {}", json["stats"]["extracted_comments"]);

                // Verify schema structure
                assert_eq!(json["platform"], "tiktok");
                assert_eq!(json["extraction_method"], "api");
                assert_eq!(json["content_id"], "7579937914797215031");
                assert!(json["stats"]["extracted_comments"].as_u64().unwrap() > 0);
                assert!(json["comments"].is_array());

                // Check first comment structure
                if let Some(first_comment) = json["comments"].as_array().and_then(|c| c.first()) {
                    assert!(first_comment["id"].is_string());
                    assert!(first_comment["author"].is_string());
                    assert!(first_comment["text"].is_string());
                    assert!(first_comment["timestamp"].is_number());
                    assert!(first_comment["likes"].is_number());
                    println!("First comment: {}", first_comment["text"]);
                }

                println!("\n✓ Schema validation passed");
                println!("✓ API deserialization works correctly");
            }
            Err(e) => {
                eprintln!("Failed to fetch comments: {}", e);
                panic!("Integration test failed - TikTok API may have changed");
            }
        }
    }
}
