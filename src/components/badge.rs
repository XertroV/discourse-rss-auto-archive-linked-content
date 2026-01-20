//! Badge components for displaying status, domain, media type, and NSFW indicators.
//!
//! This module provides maud components for rendering various badge types used
//! throughout the archive interface.

use maud::{html, Markup, Render};

/// Status badge variants based on archive status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusVariant {
    Complete,
    Failed,
    Pending,
    Processing,
    Skipped,
}

impl StatusVariant {
    /// Get the CSS class for this variant.
    #[must_use]
    pub const fn css_class(&self) -> &'static str {
        match self {
            Self::Complete => "status-complete",
            Self::Failed => "status-failed",
            Self::Pending => "status-pending",
            Self::Processing => "status-processing",
            Self::Skipped => "status-skipped",
        }
    }

    /// Get the icon for this variant.
    #[must_use]
    pub const fn icon(&self) -> &'static str {
        match self {
            Self::Complete => "\u{2713}",   // ✓
            Self::Failed => "\u{2717}",     // ✗
            Self::Pending => "\u{23F3}",    // ⏳
            Self::Processing => "\u{27F3}", // ⟳
            Self::Skipped => "\u{2298}",    // ⊘
        }
    }

    /// Get the label for this variant.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Failed => "failed",
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Skipped => "skipped",
        }
    }

    /// Get the title/tooltip for this variant.
    #[must_use]
    pub const fn title(&self) -> &'static str {
        match self {
            Self::Complete => "Archive completed successfully",
            Self::Failed => "Archive failed",
            Self::Pending => "Archive pending",
            Self::Processing => "Archive in progress",
            Self::Skipped => "Archive skipped",
        }
    }

    /// Create a variant from a status string.
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "complete" => Some(Self::Complete),
            "failed" => Some(Self::Failed),
            "pending" => Some(Self::Pending),
            "processing" => Some(Self::Processing),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

/// A status badge showing the archive state.
#[derive(Debug, Clone)]
pub struct StatusBadge<'a> {
    pub variant: StatusVariant,
    /// Optional error message for failed status (shown in tooltip).
    pub error_message: Option<&'a str>,
}

impl<'a> StatusBadge<'a> {
    /// Create a new status badge.
    #[must_use]
    pub const fn new(variant: StatusVariant) -> Self {
        Self {
            variant,
            error_message: None,
        }
    }

    /// Create a badge from a status string.
    #[must_use]
    pub fn from_status(status: &str) -> Self {
        Self {
            variant: StatusVariant::from_str(status).unwrap_or(StatusVariant::Pending),
            error_message: None,
        }
    }

    /// Add an error message (shown in tooltip for failed status).
    #[must_use]
    pub const fn with_error(mut self, error: Option<&'a str>) -> Self {
        self.error_message = error;
        self
    }
}

impl Render for StatusBadge<'_> {
    fn render(&self) -> Markup {
        let class = self.variant.css_class();
        let icon = self.variant.icon();
        let label = self.variant.label();

        // Use error message as title for failed status, otherwise use default title
        let title = if self.variant == StatusVariant::Failed {
            self.error_message.unwrap_or(self.variant.title())
        } else {
            self.variant.title()
        };

        html! {
            span class=(class) title=(title) {
                (icon) " " (label)
            }
        }
    }
}

/// Media type badge variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaTypeVariant {
    Video,
    Audio,
    Image,
    Text,
    Gallery,
    Thread,
    Playlist,
    Unknown,
}

impl MediaTypeVariant {
    /// Get the CSS class for this variant.
    #[must_use]
    pub const fn css_class(&self) -> &'static str {
        match self {
            Self::Video => "media-type-badge media-type-video",
            Self::Audio => "media-type-badge media-type-audio",
            Self::Image | Self::Gallery => "media-type-badge media-type-image",
            Self::Text | Self::Thread => "media-type-badge media-type-text",
            Self::Playlist => "media-type-badge media-type-video",
            Self::Unknown => "media-type-badge",
        }
    }

    /// Get the label for this variant.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Image => "Image",
            Self::Text => "Text",
            Self::Gallery => "Gallery",
            Self::Thread => "Thread",
            Self::Playlist => "Playlist",
            Self::Unknown => "Unknown",
        }
    }

    /// Create a variant from a content type string.
    #[must_use]
    pub fn from_str(s: &str) -> Self {
        match s {
            "video" => Self::Video,
            "audio" => Self::Audio,
            "image" => Self::Image,
            "gallery" => Self::Gallery,
            "text" => Self::Text,
            "thread" => Self::Thread,
            "playlist" => Self::Playlist,
            _ => Self::Unknown,
        }
    }
}

/// A media type badge showing the content type.
#[derive(Debug, Clone, Copy)]
pub struct MediaTypeBadge {
    pub variant: MediaTypeVariant,
}

impl MediaTypeBadge {
    /// Create a new media type badge.
    #[must_use]
    pub const fn new(variant: MediaTypeVariant) -> Self {
        Self { variant }
    }

    /// Create a badge from a content type string.
    #[must_use]
    pub fn from_content_type(content_type: &str) -> Self {
        Self {
            variant: MediaTypeVariant::from_str(content_type),
        }
    }
}

impl Render for MediaTypeBadge {
    fn render(&self) -> Markup {
        let class = self.variant.css_class();
        let label = self.variant.label();

        html! {
            span class=(class) { (label) }
        }
    }
}

/// A domain badge linking to the site archives.
#[derive(Debug, Clone)]
pub struct DomainBadge<'a> {
    pub domain: &'a str,
}

impl<'a> DomainBadge<'a> {
    /// Create a new domain badge.
    #[must_use]
    pub const fn new(domain: &'a str) -> Self {
        Self { domain }
    }
}

impl Render for DomainBadge<'_> {
    fn render(&self) -> Markup {
        html! {
            a href=(format!("/site/{}", self.domain)) class="domain-badge" {
                (self.domain)
            }
        }
    }
}

/// An NSFW badge indicating adult content.
#[derive(Debug, Clone, Copy)]
pub struct NsfwBadge;

impl NsfwBadge {
    /// Create a new NSFW badge.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for NsfwBadge {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for NsfwBadge {
    fn render(&self) -> Markup {
        html! {
            span class="nsfw-badge" { "NSFW" }
        }
    }
}

/// A size badge showing file/archive size.
#[derive(Debug, Clone, Copy)]
pub struct SizeBadge {
    pub bytes: i64,
}

impl SizeBadge {
    /// Create a new size badge.
    #[must_use]
    pub const fn new(bytes: i64) -> Self {
        Self { bytes }
    }

    /// Format bytes into human-readable format (e.g., "1.5 MB").
    #[must_use]
    pub fn format_bytes(bytes: i64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = bytes as f64;
        let mut unit_idx = 0;

        while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
            size /= 1024.0;
            unit_idx += 1;
        }

        if unit_idx == 0 {
            format!("{} {}", bytes, UNITS[unit_idx])
        } else {
            format!("{:.1} {}", size, UNITS[unit_idx])
        }
    }
}

impl Render for SizeBadge {
    fn render(&self) -> Markup {
        if self.bytes > 0 {
            html! {
                span class="archive-size" { (Self::format_bytes(self.bytes)) }
            }
        } else {
            html! {}
        }
    }
}

/// Known artifact kind variants with specific styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKindVariant {
    /// Complete self-contained HTML (monolith)
    CompleteHtml,
    /// View HTML with banner
    ViewHtml,
    /// Raw HTML without modifications
    RawHtml,
    /// Screenshot image
    Screenshot,
    /// PDF document
    Pdf,
    /// Video file
    Video,
    /// Thumbnail image
    Thumb,
    /// Metadata JSON
    Metadata,
    /// Image file
    Image,
    /// Subtitles file
    Subtitles,
    /// Unknown/other kind
    Other,
}

impl ArtifactKindVariant {
    /// Creates a variant from a string (case-insensitive).
    #[must_use]
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().replace(['-', '_'], "").as_str() {
            "completehtml" => Self::CompleteHtml,
            "viewhtml" => Self::ViewHtml,
            "rawhtml" => Self::RawHtml,
            "screenshot" => Self::Screenshot,
            "pdf" => Self::Pdf,
            "video" => Self::Video,
            "thumb" | "thumbnail" => Self::Thumb,
            "metadata" | "meta" => Self::Metadata,
            "image" | "img" => Self::Image,
            "subtitles" | "subs" | "subtitle" => Self::Subtitles,
            _ => Self::Other,
        }
    }

    /// Returns the CSS class suffix for this variant.
    #[must_use]
    pub const fn class_suffix(&self) -> Option<&'static str> {
        match self {
            Self::CompleteHtml => Some("complete_html"),
            Self::ViewHtml => Some("view_html"),
            Self::RawHtml => Some("raw_html"),
            Self::Screenshot => Some("screenshot"),
            Self::Pdf => Some("pdf"),
            Self::Video => Some("video"),
            Self::Thumb => Some("thumb"),
            Self::Metadata => Some("metadata"),
            Self::Image => Some("image"),
            Self::Subtitles => Some("subtitles"),
            Self::Other => None,
        }
    }
}

/// An artifact kind badge component.
///
/// Displays the type of artifact (complete_html, video, screenshot, etc.)
/// with appropriate styling based on the artifact kind.
///
/// # Example
///
/// ```ignore
/// use crate::components::badge::ArtifactKindBadge;
///
/// let badge = ArtifactKindBadge::new("complete_html");
/// let badge = ArtifactKindBadge::new("video");
/// ```
#[derive(Debug, Clone)]
pub struct ArtifactKindBadge<'a> {
    /// The artifact kind string
    pub kind: &'a str,
    /// Parsed variant for known kinds
    variant: ArtifactKindVariant,
}

impl<'a> ArtifactKindBadge<'a> {
    /// Creates a new artifact kind badge.
    #[must_use]
    pub fn new(kind: &'a str) -> Self {
        let variant = ArtifactKindVariant::from_str(kind);
        Self { kind, variant }
    }

    /// Returns the full CSS class string for this badge.
    fn build_class(&self) -> String {
        let mut class = String::from("artifact-kind");
        if let Some(suffix) = self.variant.class_suffix() {
            class.push_str(" artifact-kind-");
            class.push_str(suffix);
        }
        class
    }
}

impl Render for ArtifactKindBadge<'_> {
    fn render(&self) -> Markup {
        let class = self.build_class();
        html! {
            span class=(class) {
                (self.kind)
            }
        }
    }
}

/// A generic badge component with custom class and content.
///
/// Use this for badges that don't fit the predefined types.
///
/// # Example
///
/// ```ignore
/// use crate::components::badge::Badge;
///
/// let badge = Badge::new("Custom", "my-badge-class");
/// ```
#[derive(Debug, Clone)]
pub struct Badge<'a> {
    /// Badge content
    pub content: &'a str,
    /// CSS class(es)
    pub class: &'a str,
}

impl<'a> Badge<'a> {
    /// Creates a new generic badge.
    #[must_use]
    pub const fn new(content: &'a str, class: &'a str) -> Self {
        Self { content, class }
    }
}

impl Render for Badge<'_> {
    fn render(&self) -> Markup {
        html! {
            span class=(self.class) {
                (self.content)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_badge_complete() {
        let badge = StatusBadge::new(StatusVariant::Complete);
        let html = badge.render().into_string();
        assert!(html.contains("status-complete"));
        assert!(html.contains("\u{2713}")); // ✓
        assert!(html.contains("complete"));
    }

    #[test]
    fn test_status_badge_failed_with_error() {
        let badge = StatusBadge::new(StatusVariant::Failed).with_error(Some("Connection timeout"));
        let html = badge.render().into_string();
        assert!(html.contains("status-failed"));
        assert!(html.contains("Connection timeout"));
    }

    #[test]
    fn test_status_badge_from_string() {
        let badge = StatusBadge::from_status("processing");
        assert_eq!(badge.variant, StatusVariant::Processing);
    }

    #[test]
    fn test_media_type_badge() {
        let badge = MediaTypeBadge::from_content_type("video");
        let html = badge.render().into_string();
        assert!(html.contains("media-type-video"));
        assert!(html.contains("Video"));
    }

    #[test]
    fn test_media_type_badge_unknown() {
        let badge = MediaTypeBadge::from_content_type("something_weird");
        assert_eq!(badge.variant, MediaTypeVariant::Unknown);
    }

    #[test]
    fn test_domain_badge() {
        let badge = DomainBadge::new("reddit.com");
        let html = badge.render().into_string();
        assert!(html.contains("domain-badge"));
        assert!(html.contains("reddit.com"));
        assert!(html.contains("/site/reddit.com"));
    }

    #[test]
    fn test_nsfw_badge() {
        let badge = NsfwBadge::new();
        let html = badge.render().into_string();
        assert!(html.contains("nsfw-badge"));
        assert!(html.contains("NSFW"));
    }

    #[test]
    fn test_size_badge_format() {
        assert_eq!(SizeBadge::format_bytes(512), "512 B");
        assert_eq!(SizeBadge::format_bytes(1024), "1.0 KB");
        assert_eq!(SizeBadge::format_bytes(1536), "1.5 KB");
        assert_eq!(SizeBadge::format_bytes(1048576), "1.0 MB");
        assert_eq!(SizeBadge::format_bytes(1572864), "1.5 MB");
    }

    #[test]
    fn test_size_badge_render() {
        let badge = SizeBadge::new(1048576);
        let html = badge.render().into_string();
        assert!(html.contains("archive-size"));
        assert!(html.contains("1.0 MB"));
    }

    #[test]
    fn test_size_badge_zero() {
        let badge = SizeBadge::new(0);
        let html = badge.render().into_string();
        assert!(html.is_empty());
    }

    // ArtifactKindBadge tests
    #[test]
    fn test_artifact_kind_badge_complete_html() {
        let badge = ArtifactKindBadge::new("complete_html");
        let html = badge.render().into_string();
        assert!(html.contains("artifact-kind"));
        assert!(html.contains("artifact-kind-complete_html"));
        assert!(html.contains("complete_html"));
    }

    #[test]
    fn test_artifact_kind_badge_view_html() {
        let badge = ArtifactKindBadge::new("view_html");
        let html = badge.render().into_string();
        assert!(html.contains("artifact-kind-view_html"));
    }

    #[test]
    fn test_artifact_kind_badge_raw_html() {
        let badge = ArtifactKindBadge::new("raw_html");
        let html = badge.render().into_string();
        assert!(html.contains("artifact-kind-raw_html"));
    }

    #[test]
    fn test_artifact_kind_badge_screenshot() {
        let badge = ArtifactKindBadge::new("screenshot");
        let html = badge.render().into_string();
        assert!(html.contains("artifact-kind-screenshot"));
    }

    #[test]
    fn test_artifact_kind_badge_pdf() {
        let badge = ArtifactKindBadge::new("pdf");
        let html = badge.render().into_string();
        assert!(html.contains("artifact-kind-pdf"));
    }

    #[test]
    fn test_artifact_kind_badge_video() {
        let badge = ArtifactKindBadge::new("video");
        let html = badge.render().into_string();
        assert!(html.contains("artifact-kind-video"));
    }

    #[test]
    fn test_artifact_kind_badge_thumb() {
        let badge = ArtifactKindBadge::new("thumb");
        let html = badge.render().into_string();
        assert!(html.contains("artifact-kind-thumb"));
    }

    #[test]
    fn test_artifact_kind_badge_metadata() {
        let badge = ArtifactKindBadge::new("metadata");
        let html = badge.render().into_string();
        assert!(html.contains("artifact-kind-metadata"));
    }

    #[test]
    fn test_artifact_kind_badge_image() {
        let badge = ArtifactKindBadge::new("image");
        let html = badge.render().into_string();
        assert!(html.contains("artifact-kind-image"));
    }

    #[test]
    fn test_artifact_kind_badge_subtitles() {
        let badge = ArtifactKindBadge::new("subtitles");
        let html = badge.render().into_string();
        assert!(html.contains("artifact-kind-subtitles"));
    }

    #[test]
    fn test_artifact_kind_badge_unknown() {
        let badge = ArtifactKindBadge::new("custom_artifact");
        let html = badge.render().into_string();
        assert!(html.contains("artifact-kind"));
        assert!(!html.contains("artifact-kind-custom"));
        assert!(html.contains("custom_artifact"));
    }

    #[test]
    fn test_artifact_kind_variant_from_str() {
        assert_eq!(
            ArtifactKindVariant::from_str("complete_html"),
            ArtifactKindVariant::CompleteHtml
        );
        assert_eq!(
            ArtifactKindVariant::from_str("COMPLETE-HTML"),
            ArtifactKindVariant::CompleteHtml
        );
        assert_eq!(
            ArtifactKindVariant::from_str("view-html"),
            ArtifactKindVariant::ViewHtml
        );
        assert_eq!(
            ArtifactKindVariant::from_str("thumbnail"),
            ArtifactKindVariant::Thumb
        );
        assert_eq!(
            ArtifactKindVariant::from_str("unknown"),
            ArtifactKindVariant::Other
        );
    }

    // Generic Badge tests
    #[test]
    fn test_generic_badge() {
        let badge = Badge::new("Custom", "my-custom-class");
        let html = badge.render().into_string();
        assert!(html.contains("my-custom-class"));
        assert!(html.contains("Custom"));
    }
}
