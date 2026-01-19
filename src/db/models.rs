use serde::{Deserialize, Serialize};

/// A Discourse forum post from the RSS feed.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Post {
    pub id: i64,
    pub guid: String,
    pub discourse_url: String,
    pub author: Option<String>,
    pub title: Option<String>,
    pub body_html: Option<String>,
    pub content_hash: Option<String>,
    pub published_at: Option<String>,
    pub processed_at: String,
}

/// A URL found in a post.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Link {
    pub id: i64,
    pub original_url: String,
    pub normalized_url: String,
    pub canonical_url: Option<String>,
    pub final_url: Option<String>,
    pub domain: String,
    pub first_seen_at: String,
    pub last_archived_at: Option<String>,
}

/// A specific occurrence of a link in a post.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LinkOccurrence {
    pub id: i64,
    pub link_id: i64,
    pub post_id: i64,
    pub in_quote: bool,
    pub context_snippet: Option<String>,
    pub seen_at: String,
}

/// Archive status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveStatus {
    Pending,
    Processing,
    Complete,
    Failed,
    Skipped,
}

impl ArchiveStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Complete => "complete",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "processing" => Some(Self::Processing),
            "complete" => Some(Self::Complete),
            "failed" => Some(Self::Failed),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

/// Content type of an archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentType {
    Video,
    Image,
    Text,
    Gallery,
    Thread,
}

impl ContentType {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Image => "image",
            Self::Text => "text",
            Self::Gallery => "gallery",
            Self::Thread => "thread",
        }
    }
}

/// An archived piece of content.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Archive {
    pub id: i64,
    pub link_id: i64,
    pub status: String,
    pub archived_at: Option<String>,
    pub content_title: Option<String>,
    pub content_author: Option<String>,
    pub content_text: Option<String>,
    pub content_type: Option<String>,
    pub s3_key_primary: Option<String>,
    pub s3_key_thumb: Option<String>,
    pub s3_keys_extra: Option<String>,
    pub wayback_url: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub created_at: String,
}

impl Archive {
    #[must_use]
    pub fn status_enum(&self) -> Option<ArchiveStatus> {
        ArchiveStatus::from_str(&self.status)
    }
}

/// Kind of archive artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    RawHtml,
    Screenshot,
    Pdf,
    Video,
    Thumb,
    Metadata,
    Image,
    Subtitles,
}

impl ArtifactKind {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RawHtml => "raw_html",
            Self::Screenshot => "screenshot",
            Self::Pdf => "pdf",
            Self::Video => "video",
            Self::Thumb => "thumb",
            Self::Metadata => "metadata",
            Self::Image => "image",
            Self::Subtitles => "subtitles",
        }
    }
}

/// An individual file stored for an archive.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ArchiveArtifact {
    pub id: i64,
    pub archive_id: i64,
    pub kind: String,
    pub s3_key: String,
    pub content_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub sha256: Option<String>,
    pub created_at: String,
}

/// Data for inserting a new post.
#[derive(Debug, Clone)]
pub struct NewPost {
    pub guid: String,
    pub discourse_url: String,
    pub author: Option<String>,
    pub title: Option<String>,
    pub body_html: Option<String>,
    pub content_hash: Option<String>,
    pub published_at: Option<String>,
}

/// Data for inserting a new link.
#[derive(Debug, Clone)]
pub struct NewLink {
    pub original_url: String,
    pub normalized_url: String,
    pub canonical_url: Option<String>,
    pub domain: String,
}

/// Data for inserting a new link occurrence.
#[derive(Debug, Clone)]
pub struct NewLinkOccurrence {
    pub link_id: i64,
    pub post_id: i64,
    pub in_quote: bool,
    pub context_snippet: Option<String>,
}

/// Archive with associated link for display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveWithLink {
    pub archive: Archive,
    pub link: Link,
}
