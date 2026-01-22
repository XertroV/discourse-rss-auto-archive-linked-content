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
    pub const fn as_str(&self) -> &'static str {
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
    Playlist,
    Pdf,
}

impl ContentType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Image => "image",
            Self::Text => "text",
            Self::Gallery => "gallery",
            Self::Thread => "thread",
            Self::Playlist => "playlist",
            Self::Pdf => "pdf",
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
    pub archive_today_url: Option<String>,
    pub ipfs_cid: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub created_at: String,
    /// Whether the content is NSFW (Not Safe For Work).
    #[serde(default)]
    pub is_nsfw: bool,
    /// Source of the NSFW flag: 'api', 'metadata', 'subreddit', 'manual'.
    pub nsfw_source: Option<String>,
    /// When the archive is eligible for retry (for exponential backoff).
    pub next_retry_at: Option<String>,
    /// When the last archive attempt was made.
    pub last_attempt_at: Option<String>,
    /// HTTP status code from the original page fetch (200, 404, 401, etc.).
    pub http_status_code: Option<i32>,
    /// Publication date of the Discourse post containing this link.
    /// Used for sorting archives by when the post was made, not when archived.
    pub post_date: Option<String>,
    /// Reference to the archive of a quoted tweet (Twitter/X only).
    pub quoted_archive_id: Option<i64>,
    /// Reference to the archive of the tweet this replies to (Twitter/X only).
    pub reply_to_archive_id: Option<i64>,
    /// User who submitted this for archiving (NULL for RSS-sourced archives).
    pub submitted_by_user_id: Option<i64>,
    /// Download progress percentage (0.0-100.0) for active downloads.
    pub progress_percent: Option<f64>,
    /// JSON string with download progress details (speed, ETA, size).
    pub progress_details: Option<String>,
    /// Timestamp of the last progress update.
    pub last_progress_update: Option<String>,
    /// Extracted Open Graph title from the archived page.
    pub og_title: Option<String>,
    /// Extracted Open Graph description from the archived page.
    pub og_description: Option<String>,
    /// Extracted Open Graph image URL from the archived page.
    pub og_image: Option<String>,
    /// Extracted Open Graph type from the archived page (article, website, etc.).
    pub og_type: Option<String>,
    /// Timestamp when OG metadata was extracted.
    pub og_extracted_at: Option<String>,
    /// Whether OG extraction was attempted (0 = no, 1 = yes).
    /// Prevents repeated failed extraction attempts.
    #[serde(default)]
    pub og_extraction_attempted: bool,
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
    ViewHtml,
    CompleteHtml,
    Mhtml,
    Screenshot,
    Pdf,
    Video,
    Thumb,
    Metadata,
    Image,
    Subtitles,
    Transcript,
    Comments,
}

impl ArtifactKind {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::RawHtml => "raw_html",
            Self::ViewHtml => "view_html",
            Self::CompleteHtml => "complete_html",
            Self::Mhtml => "mhtml",
            Self::Screenshot => "screenshot",
            Self::Pdf => "pdf",
            Self::Video => "video",
            Self::Thumb => "thumb",
            Self::Metadata => "metadata",
            Self::Image => "image",
            Self::Subtitles => "subtitles",
            Self::Transcript => "transcript",
            Self::Comments => "comments",
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
    /// Perceptual hash for image/video content (for deduplication)
    pub perceptual_hash: Option<String>,
    /// If this artifact is a duplicate, points to the original artifact
    pub duplicate_of_artifact_id: Option<i64>,
    /// Reference to the canonical video file (for video aliasing)
    pub video_file_id: Option<i64>,
    /// Structured metadata (JSON) for artifact-specific data (e.g., subtitle language, transcript source)
    pub metadata: Option<String>,
}

/// Detected language information for a subtitle artifact.
///
/// Tracks the language of subtitle files, how it was detected, and whether it's auto-generated.
/// This enables:
/// - Displaying language info in the Archived Files table
/// - Admin management of subtitle language entries
/// - Re-detection when an entry is deleted
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SubtitleLanguage {
    pub id: i64,
    /// The artifact ID this language info belongs to
    pub artifact_id: i64,
    /// Language code (e.g., "en", "ja", "unknown")
    pub language: String,
    /// How the language was detected: "filename", "vtt_header", "metadata", "manual"
    pub detected_from: String,
    /// Whether this is an auto-generated subtitle
    pub is_auto: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// A canonical video file stored on S3.
///
/// Videos are stored once at a predictable path (videos/{video_id}.{ext})
/// and referenced by multiple archives. This enables deduplication where
/// the same video referenced from different posts only stores one copy.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct VideoFile {
    pub id: i64,
    /// Platform-specific video identifier (e.g., "dQw4w9WgXcQ" for YouTube)
    pub video_id: String,
    /// Platform name (e.g., "youtube", "tiktok", "reddit")
    pub platform: String,
    /// S3 key where the video is stored (e.g., "videos/dQw4w9WgXcQ.mp4")
    pub s3_key: String,
    /// S3 key for metadata JSON (e.g., "videos/dQw4w9WgXcQ.json")
    pub metadata_s3_key: Option<String>,
    /// File size in bytes
    pub size_bytes: Option<i64>,
    /// MIME content type (e.g., "video/mp4")
    pub content_type: Option<String>,
    /// Video duration in seconds (from metadata)
    pub duration_seconds: Option<i64>,
    /// When this video file record was created
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

/// Flattened archive data for list display (includes link info).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ArchiveDisplay {
    // Archive fields
    pub id: i64,
    pub link_id: i64,
    pub status: String,
    pub archived_at: Option<String>,
    pub content_title: Option<String>,
    pub content_author: Option<String>,
    pub content_type: Option<String>,
    pub is_nsfw: bool,
    pub error_message: Option<String>,
    pub retry_count: i32,
    // Link fields
    pub original_url: String,
    pub domain: String,
    // Total size of all artifacts (in bytes)
    pub total_size_bytes: Option<i64>,
}

/// Thread (post) with aggregated stats for list display.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadDisplay {
    pub guid: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub discourse_url: String,
    pub published_at: Option<String>,
    pub link_count: i64,
    pub archive_count: i64,
    pub last_archived_at: Option<String>,
}

/// Submission status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubmissionStatus {
    Pending,
    Processing,
    Complete,
    Failed,
    Rejected,
}

impl SubmissionStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Complete => "complete",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
        }
    }

    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "processing" => Some(Self::Processing),
            "complete" => Some(Self::Complete),
            "failed" => Some(Self::Failed),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }
}

/// A manually submitted URL for archiving.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Submission {
    pub id: i64,
    pub url: String,
    pub normalized_url: String,
    pub submitted_by_ip: String,
    pub status: String,
    pub link_id: Option<i64>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub processed_at: Option<String>,
    /// User who submitted this URL (NULL for anonymous submissions).
    pub submitted_by_user_id: Option<i64>,
}

/// Data for creating a new submission.
#[derive(Debug, Clone)]
pub struct NewSubmission {
    pub url: String,
    pub normalized_url: String,
    pub submitted_by_ip: String,
    pub submitted_by_user_id: Option<i64>,
}

/// Job type for archive jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveJobType {
    /// Fetch HTML content from the URL
    FetchHtml,
    /// Download video using yt-dlp
    YtDlp,
    /// Download images using gallery-dl
    GalleryDl,
    /// Capture screenshot
    Screenshot,
    /// Generate PDF
    Pdf,
    /// Generate MHTML
    Mhtml,
    /// Generate monolith HTML
    Monolith,
    /// Upload to S3
    S3Upload,
    /// Submit to Wayback Machine
    Wayback,
    /// Submit to Archive.today
    ArchiveToday,
    /// Pin to IPFS
    Ipfs,
    /// Fetch supplementary artifacts (subtitles, transcripts) for existing archive
    SupplementaryArtifacts,
    /// Extract comments from video/social media platform
    CommentExtraction,
}

impl ArchiveJobType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::FetchHtml => "fetch_html",
            Self::YtDlp => "yt_dlp",
            Self::GalleryDl => "gallery_dl",
            Self::Screenshot => "screenshot",
            Self::Pdf => "pdf",
            Self::Mhtml => "mhtml",
            Self::Monolith => "monolith",
            Self::S3Upload => "s3_upload",
            Self::Wayback => "wayback",
            Self::ArchiveToday => "archive_today",
            Self::Ipfs => "ipfs",
            Self::SupplementaryArtifacts => "supplementary_artifacts",
            Self::CommentExtraction => "comment_extraction",
        }
    }

    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "fetch_html" => Some(Self::FetchHtml),
            "yt_dlp" => Some(Self::YtDlp),
            "gallery_dl" => Some(Self::GalleryDl),
            "screenshot" => Some(Self::Screenshot),
            "pdf" => Some(Self::Pdf),
            "mhtml" => Some(Self::Mhtml),
            "monolith" => Some(Self::Monolith),
            "s3_upload" => Some(Self::S3Upload),
            "wayback" => Some(Self::Wayback),
            "archive_today" => Some(Self::ArchiveToday),
            "ipfs" => Some(Self::Ipfs),
            "supplementary_artifacts" => Some(Self::SupplementaryArtifacts),
            "comment_extraction" => Some(Self::CommentExtraction),
            _ => None,
        }
    }

    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::FetchHtml => "Fetch HTML",
            Self::YtDlp => "Video Download (yt-dlp)",
            Self::GalleryDl => "Gallery Download",
            Self::Screenshot => "Screenshot",
            Self::Pdf => "PDF Generation",
            Self::Mhtml => "MHTML Archive",
            Self::Monolith => "Monolith HTML",
            Self::S3Upload => "S3 Upload",
            Self::Wayback => "Wayback Machine",
            Self::ArchiveToday => "Archive.today",
            Self::Ipfs => "IPFS",
            Self::SupplementaryArtifacts => "Supplementary Artifacts",
            Self::CommentExtraction => "Comment Extraction",
        }
    }
}

/// Job status for archive jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArchiveJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

impl ArchiveJobStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

/// A single job/step in the archiving process.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ArchiveJob {
    pub id: i64,
    pub archive_id: i64,
    pub job_type: String,
    pub status: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_message: Option<String>,
    pub metadata: Option<String>,
    pub created_at: String,
    /// Duration of job execution in seconds with subsecond accuracy (calculated from started_at to completed_at).
    pub duration_seconds: Option<f64>,
}

impl ArchiveJob {
    #[must_use]
    pub fn job_type_enum(&self) -> Option<ArchiveJobType> {
        ArchiveJobType::from_str(&self.job_type)
    }

    #[must_use]
    pub fn status_enum(&self) -> Option<ArchiveJobStatus> {
        ArchiveJobStatus::from_str(&self.status)
    }
}

/// User account.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub is_approved: bool,
    pub is_admin: bool,
    pub is_active: bool,
    pub failed_login_attempts: i32,
    pub locked_until: Option<String>,
    pub password_updated_at: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A link between a Discourse forum account and an archive user account.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ForumAccountLink {
    pub id: i64,
    pub user_id: i64,
    pub forum_username: String,
    pub linked_via_post_guid: String,
    pub linked_via_post_url: String,
    pub forum_author_raw: Option<String>,
    pub post_title: Option<String>,
    pub post_published_at: Option<String>,
    pub created_at: String,
}

/// User session.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Session {
    pub id: i64,
    pub user_id: i64,
    pub token: String,
    pub csrf_token: String,
    pub ip_address: String,
    pub user_agent: Option<String>,
    pub expires_at: String,
    pub created_at: String,
    pub last_used_at: String,
}

/// Audit event for tracking user actions.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditEvent {
    pub id: i64,
    pub user_id: Option<i64>,
    pub event_type: String,
    pub target_type: Option<String>,
    pub target_id: Option<i64>,
    pub metadata: Option<String>,
    pub ip_address: Option<String>,
    pub forwarded_for: Option<String>,
    pub user_agent: Option<String>,
    pub user_agent_id: Option<i64>,
    pub created_at: String,
}

/// User agent for deduplication.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserAgent {
    pub id: i64,
    pub hash: String,
    pub user_agent: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
}

/// A comment on an archive.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Comment {
    pub id: i64,
    pub archive_id: i64,
    pub user_id: Option<i64>,
    pub parent_comment_id: Option<i64>,
    pub content: String,
    pub is_deleted: bool,
    pub deleted_by_admin: bool,
    pub is_pinned: bool,
    pub pinned_by_user_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

/// Edit history entry for a comment.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CommentEdit {
    pub id: i64,
    pub comment_id: i64,
    pub previous_content: String,
    pub edited_by_user_id: i64,
    pub edited_at: String,
}

/// A helpful reaction (upvote) on a comment.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CommentReaction {
    pub id: i64,
    pub comment_id: i64,
    pub user_id: i64,
    pub reaction_type: String,
    pub created_at: String,
}

/// A comment with author details for display.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CommentWithAuthor {
    pub id: i64,
    pub archive_id: i64,
    pub user_id: Option<i64>,
    pub parent_comment_id: Option<i64>,
    pub content: String,
    pub is_deleted: bool,
    pub deleted_by_admin: bool,
    pub is_pinned: bool,
    pub pinned_by_user_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub author_username: Option<String>,
    pub author_display_name: Option<String>,
    pub edit_count: i64,
    pub helpful_count: i64,
}

/// An excluded domain that should not be archived.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExcludedDomain {
    pub id: i64,
    pub domain: String,
    pub reason: String,
    pub is_active: bool,
    pub created_at: String,
    pub created_by_user_id: Option<i64>,
    pub updated_at: String,
}

/// Status of a thread archive job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThreadArchiveJobStatus {
    Pending,
    Processing,
    Complete,
    Failed,
}

impl ThreadArchiveJobStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }

    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "processing" => Some(Self::Processing),
            "complete" => Some(Self::Complete),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// A request to archive all links in a Discourse thread.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ThreadArchiveJob {
    pub id: i64,
    pub thread_url: String,
    pub rss_url: String,
    pub status: String,
    pub user_id: i64,
    pub total_posts: Option<i64>,
    pub processed_posts: i64,
    pub new_links_found: i64,
    pub archives_created: i64,
    pub skipped_links: i64,
    pub error_message: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

impl ThreadArchiveJob {
    #[must_use]
    pub fn status_enum(&self) -> Option<ThreadArchiveJobStatus> {
        ThreadArchiveJobStatus::from_str(&self.status)
    }
}

/// Data for creating a new thread archive job.
#[derive(Debug, Clone)]
pub struct NewThreadArchiveJob {
    pub thread_url: String,
    pub rss_url: String,
    pub user_id: i64,
}

// ========================================
// Discourse JSON API Response Models
// ========================================

/// Response from Discourse /t/:topic_id/posts.json endpoint.
#[derive(Debug, Deserialize)]
pub struct DiscoursePostsResponse {
    pub post_stream: DiscoursePostStream,
}

/// Post stream container in Discourse JSON responses.
#[derive(Debug, Deserialize)]
pub struct DiscoursePostStream {
    pub posts: Vec<DiscoursePost>,
}

/// A single post from Discourse JSON API (thread-specific).
#[derive(Debug, Deserialize)]
pub struct DiscoursePost {
    pub id: i64,
    pub post_number: i64,
    pub username: String,
    pub topic_id: i64,
    pub topic_slug: String,
    pub created_at: String,
    pub updated_at: String,
    pub cooked: String, // HTML content
    #[serde(default)]
    pub posts_count: Option<i64>, // Total posts in thread (only in first post)
}

/// Response from /posts.json endpoint (latest posts across all topics).
#[derive(Debug, Deserialize)]
pub struct LatestPostsResponse {
    pub latest_posts: Vec<LatestPost>,
}

/// A single post from /posts.json (global latest posts feed).
#[derive(Debug, Deserialize)]
pub struct LatestPost {
    pub id: i64,
    pub post_number: i64,
    pub username: String,
    pub topic_id: i64,
    pub topic_slug: String,
    pub topic_title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub cooked: String,
    pub post_url: String,
}
