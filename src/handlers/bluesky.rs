use std::path::Path;

use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::traits::{ArchiveResult, SiteHandler};
use crate::archiver::CookieOptions;
use crate::constants::ARCHIVAL_USER_AGENT;

static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        Regex::new(r"^https?://bsky\.app/profile/[^/]+/post/[a-zA-Z0-9]+").unwrap(),
        Regex::new(r"^https?://bsky\.social/profile/[^/]+/post/[a-zA-Z0-9]+").unwrap(),
    ]
});

// Regex to extract handle and post ID from URL
static URL_PARSER: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^https?://bsky\.(app|social)/profile/([^/]+)/post/([a-zA-Z0-9]+)").unwrap()
});

const BSKY_API_BASE: &str = "https://public.api.bsky.app/xrpc";

pub struct BlueskyHandler {
    client: reqwest::Client,
}

impl BlueskyHandler {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent(ARCHIVAL_USER_AGENT)
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }
}

impl Default for BlueskyHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from resolveHandle API
#[derive(Debug, Deserialize)]
struct ResolveHandleResponse {
    did: String,
}

/// Response from getPostThread API
#[derive(Debug, Deserialize)]
struct PostThreadResponse {
    thread: ThreadPost,
}

#[derive(Debug, Deserialize)]
struct ThreadPost {
    post: Post,
}

#[derive(Debug, Deserialize)]
struct Post {
    uri: String,
    cid: String,
    author: Author,
    record: PostRecord,
    #[serde(default)]
    embed: Option<Embed>,
}

#[derive(Debug, Deserialize)]
struct Author {
    did: String,
    handle: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PostRecord {
    text: String,
    #[serde(rename = "createdAt")]
    created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "$type")]
enum Embed {
    #[serde(rename = "app.bsky.embed.images#view")]
    Images { images: Vec<EmbedImage> },
    #[serde(rename = "app.bsky.embed.external#view")]
    External { external: ExternalEmbed },
    #[serde(rename = "app.bsky.embed.record#view")]
    Record {
        #[allow(dead_code)]
        record: serde_json::Value,
    },
    #[serde(rename = "app.bsky.embed.recordWithMedia#view")]
    RecordWithMedia {
        #[allow(dead_code)]
        media: serde_json::Value,
    },
    #[serde(rename = "app.bsky.embed.video#view")]
    Video(VideoEmbed),
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct EmbedImage {
    thumb: String,
    fullsize: String,
    #[allow(dead_code)]
    alt: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExternalEmbed {
    uri: String,
    title: String,
    description: String,
}

#[derive(Debug, Deserialize)]
struct VideoEmbed {
    thumbnail: Option<String>,
    #[allow(dead_code)]
    playlist: Option<String>,
}

/// Metadata saved to JSON file
#[derive(Debug, Serialize)]
struct BlueskyMetadata {
    uri: String,
    cid: String,
    author_did: String,
    author_handle: String,
    author_display_name: Option<String>,
    text: String,
    created_at: String,
    embed_type: Option<String>,
    image_urls: Vec<String>,
    external_url: Option<String>,
}

impl BlueskyHandler {
    /// Parse handle and post ID from a Bluesky URL
    fn parse_url(url: &str) -> Option<(String, String)> {
        URL_PARSER.captures(url).map(|caps| {
            let handle = caps.get(2).unwrap().as_str().to_string();
            let post_id = caps.get(3).unwrap().as_str().to_string();
            (handle, post_id)
        })
    }

    /// Resolve a handle to a DID
    async fn resolve_handle(&self, handle: &str) -> Result<String> {
        let url = format!("{BSKY_API_BASE}/com.atproto.identity.resolveHandle?handle={handle}");

        let response: ResolveHandleResponse = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to resolve Bluesky handle")?
            .error_for_status()
            .context("Handle resolution returned error")?
            .json()
            .await
            .context("Failed to parse handle resolution response")?;

        Ok(response.did)
    }

    /// Fetch a post thread by AT URI
    async fn get_post(&self, did: &str, post_id: &str) -> Result<Post> {
        let at_uri = format!("at://{did}/app.bsky.feed.post/{post_id}");
        let url = format!(
            "{}/app.bsky.feed.getPostThread?uri={}",
            BSKY_API_BASE,
            urlencoding::encode(&at_uri)
        );

        let response: PostThreadResponse = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch Bluesky post")?
            .error_for_status()
            .context("Post fetch returned error")?
            .json()
            .await
            .context("Failed to parse post response")?;

        Ok(response.thread.post)
    }

    /// Download an image from Bluesky CDN
    async fn download_image(&self, url: &str, work_dir: &Path, index: usize) -> Result<String> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("Failed to download image")?
            .error_for_status()
            .context("Image download returned error")?;

        // Determine extension from content-type or URL
        let ext = response
            .headers()
            .get("content-type")
            .and_then(|ct| ct.to_str().ok())
            .map_or("jpg", |ct| match ct {
                "image/jpeg" => "jpg",
                "image/png" => "png",
                "image/gif" => "gif",
                "image/webp" => "webp",
                _ => "jpg",
            });

        let filename = format!("image_{index}.{ext}");
        let file_path = work_dir.join(&filename);

        let bytes = response
            .bytes()
            .await
            .context("Failed to read image bytes")?;
        tokio::fs::write(&file_path, &bytes)
            .await
            .context("Failed to write image file")?;

        Ok(filename)
    }
}

#[async_trait]
impl SiteHandler for BlueskyHandler {
    fn site_id(&self) -> &'static str {
        "bluesky"
    }

    fn url_patterns(&self) -> &[Regex] {
        &PATTERNS
    }

    fn priority(&self) -> i32 {
        100
    }

    fn normalize_url(&self, url: &str) -> String {
        // Normalize to bsky.app format
        url.replace("bsky.social", "bsky.app")
    }

    async fn archive(
        &self,
        url: &str,
        work_dir: &Path,
        _cookies: &CookieOptions<'_>,
    ) -> Result<ArchiveResult> {
        // Parse URL to get handle and post ID
        let (handle, post_id) = Self::parse_url(url).context("Invalid Bluesky URL format")?;

        // Resolve handle to DID
        let did = self.resolve_handle(&handle).await?;

        // Fetch the post
        let post = self.get_post(&did, &post_id).await?;

        // Prepare result
        let mut result = ArchiveResult {
            title: Some(format!("Post by @{}", post.author.handle)),
            author: post
                .author
                .display_name
                .clone()
                .or(Some(post.author.handle.clone())),
            text: Some(post.record.text.clone()),
            content_type: "text".to_string(),
            ..Default::default()
        };

        let mut image_urls = Vec::new();
        let mut embed_type = None;
        let mut external_url = None;

        // Process embed if present
        if let Some(embed) = &post.embed {
            match embed {
                Embed::Images { images } => {
                    result.content_type = "image".to_string();
                    embed_type = Some("images".to_string());

                    // Download images
                    for (i, img) in images.iter().enumerate() {
                        image_urls.push(img.fullsize.clone());
                        match self.download_image(&img.fullsize, work_dir, i).await {
                            Ok(filename) => {
                                if i == 0 {
                                    result.primary_file = Some(filename);
                                } else {
                                    result.extra_files.push(filename);
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to download image {}: {}", i, e);
                            }
                        }

                        // Also download thumbnail as potential thumb
                        if i == 0 {
                            if let Ok(thumb_filename) =
                                self.download_image(&img.thumb, work_dir, 999).await
                            {
                                result.thumbnail = Some(thumb_filename);
                            }
                        }
                    }
                }
                Embed::Video(video) => {
                    result.content_type = "video".to_string();
                    embed_type = Some("video".to_string());

                    // Download thumbnail if available
                    if let Some(thumb_url) = &video.thumbnail {
                        if let Ok(thumb_filename) =
                            self.download_image(thumb_url, work_dir, 0).await
                        {
                            result.thumbnail = Some(thumb_filename);
                        }
                    }
                }
                Embed::External { external } => {
                    embed_type = Some("external".to_string());
                    external_url = Some(external.uri.clone());
                    // Append external link info to text
                    let ext_text = format!(
                        "\n\n[External Link]\nTitle: {}\nDescription: {}\nURL: {}",
                        external.title, external.description, external.uri
                    );
                    if let Some(ref mut text) = result.text {
                        text.push_str(&ext_text);
                    }
                }
                Embed::Record { .. } | Embed::RecordWithMedia { .. } => {
                    embed_type = Some("quote".to_string());
                }
                Embed::Unknown => {}
            }
        }

        // Create metadata JSON
        let metadata = BlueskyMetadata {
            uri: post.uri,
            cid: post.cid,
            author_did: post.author.did,
            author_handle: post.author.handle,
            author_display_name: post.author.display_name,
            text: post.record.text,
            created_at: post.record.created_at,
            embed_type,
            image_urls,
            external_url,
        };

        let metadata_json =
            serde_json::to_string_pretty(&metadata).context("Failed to serialize metadata")?;

        // Write metadata file
        let metadata_path = work_dir.join("metadata.json");
        tokio::fs::write(&metadata_path, &metadata_json)
            .await
            .context("Failed to write metadata file")?;

        result.metadata_json = Some(metadata_json);
        result.extra_files.push("metadata.json".to_string());

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle() {
        let handler = BlueskyHandler::new();

        assert!(handler.can_handle("https://bsky.app/profile/alice.bsky.social/post/3abc123"));
        assert!(handler.can_handle("https://bsky.social/profile/bob.dev/post/xyz789"));
        assert!(handler.can_handle("https://bsky.app/profile/example.com/post/12345"));

        assert!(!handler.can_handle("https://twitter.com/user/status/123"));
        assert!(!handler.can_handle("https://bsky.app/profile/alice"));
        assert!(!handler.can_handle("https://bsky.app/"));
    }

    #[test]
    fn test_parse_url() {
        let (handle, post_id) =
            BlueskyHandler::parse_url("https://bsky.app/profile/alice.bsky.social/post/abc123")
                .unwrap();
        assert_eq!(handle, "alice.bsky.social");
        assert_eq!(post_id, "abc123");

        let (handle, post_id) =
            BlueskyHandler::parse_url("https://bsky.social/profile/bob.dev/post/xyz").unwrap();
        assert_eq!(handle, "bob.dev");
        assert_eq!(post_id, "xyz");
    }

    #[test]
    fn test_normalize_url() {
        let handler = BlueskyHandler::new();

        assert_eq!(
            handler.normalize_url("https://bsky.social/profile/alice/post/123"),
            "https://bsky.app/profile/alice/post/123"
        );

        assert_eq!(
            handler.normalize_url("https://bsky.app/profile/alice/post/123"),
            "https://bsky.app/profile/alice/post/123"
        );
    }

    #[test]
    fn test_site_id() {
        let handler = BlueskyHandler::new();
        assert_eq!(handler.site_id(), "bluesky");
    }

    #[test]
    fn test_priority() {
        let handler = BlueskyHandler::new();
        assert_eq!(handler.priority(), 100);
    }
}
