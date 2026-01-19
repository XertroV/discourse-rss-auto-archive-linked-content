use std::path::Path;

use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;

use crate::archiver::CookieOptions;

/// Result of archiving a URL.
#[derive(Debug, Clone)]
pub struct ArchiveResult {
    /// Title of the content.
    pub title: Option<String>,
    /// Author of the content.
    pub author: Option<String>,
    /// Extracted text content.
    pub text: Option<String>,
    /// Type of content (video, image, text, etc.).
    pub content_type: String,
    /// Path to primary downloaded file (relative to work dir).
    pub primary_file: Option<String>,
    /// Path to thumbnail (relative to work dir).
    pub thumbnail: Option<String>,
    /// Paths to additional files.
    pub extra_files: Vec<String>,
    /// Raw metadata JSON.
    pub metadata_json: Option<String>,
    /// Whether the content is NSFW (Not Safe For Work).
    pub is_nsfw: Option<bool>,
    /// Source of the NSFW determination (api, metadata, subreddit).
    pub nsfw_source: Option<String>,
    /// Final URL after following redirects (if different from original).
    pub final_url: Option<String>,
    /// Video ID for content like YouTube (for predictable S3 paths).
    pub video_id: Option<String>,
}

impl Default for ArchiveResult {
    fn default() -> Self {
        Self {
            title: None,
            author: None,
            text: None,
            content_type: "text".to_string(),
            primary_file: None,
            thumbnail: None,
            extra_files: Vec::new(),
            metadata_json: None,
            is_nsfw: None,
            nsfw_source: None,
            final_url: None,
            video_id: None,
        }
    }
}

/// Trait for site-specific URL handlers.
#[async_trait]
pub trait SiteHandler: Send + Sync {
    /// Unique identifier for this handler.
    fn site_id(&self) -> &'static str;

    /// URL patterns this handler matches.
    fn url_patterns(&self) -> &[Regex];

    /// Check if this handler can handle the given URL.
    fn can_handle(&self, url: &str) -> bool {
        self.url_patterns().iter().any(|p| p.is_match(url))
    }

    /// Normalize a URL for this site.
    fn normalize_url(&self, url: &str) -> String {
        url.to_string()
    }

    /// Priority for handler selection (higher = preferred).
    fn priority(&self) -> i32 {
        0
    }

    /// Archive content from the URL.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL to archive
    /// * `work_dir` - Temporary directory for downloads
    /// * `cookies` - Cookie options for authenticated downloads
    /// * `config` - Application configuration
    ///
    /// # Errors
    ///
    /// Returns an error if archiving fails.
    async fn archive(
        &self,
        url: &str,
        work_dir: &Path,
        cookies: &CookieOptions<'_>,
        config: &crate::config::Config,
    ) -> Result<ArchiveResult>;
}
