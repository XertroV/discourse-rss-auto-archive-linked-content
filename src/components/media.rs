//! Media display components for video, audio, and image content.
//!
//! This module provides reusable components for rendering media elements
//! including video players, audio players, images, and galleries.

use maud::{html, Markup, Render};

use super::badge::MediaTypeBadge;
#[cfg(test)]
use super::badge::MediaTypeVariant;

/// Video player component with responsive wrapper.
#[derive(Debug, Clone)]
pub struct VideoPlayer<'a> {
    /// Source URL for the video
    pub src: &'a str,
    /// Optional poster image URL
    pub poster: Option<&'a str>,
    /// Optional MIME type for the video source
    pub content_type: Option<&'a str>,
}

impl<'a> VideoPlayer<'a> {
    /// Create a new video player.
    #[must_use]
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            poster: None,
            content_type: None,
        }
    }

    /// Set a poster image for the video.
    #[must_use]
    pub fn with_poster(mut self, poster: &'a str) -> Self {
        self.poster = Some(poster);
        self
    }

    /// Set the content type (MIME type) for the video.
    #[must_use]
    pub fn with_content_type(mut self, content_type: &'a str) -> Self {
        self.content_type = Some(content_type);
        self
    }

    /// Infer the video MIME type from the source URL extension.
    fn inferred_type(&self) -> &'static str {
        let extension = self.src.rsplit('.').next().unwrap_or("").to_lowercase();

        match extension.as_str() {
            "mp4" | "m4v" => "video/mp4",
            "webm" => "video/webm",
            "mkv" => "video/x-matroska",
            "mov" => "video/quicktime",
            "avi" => "video/x-msvideo",
            "ogv" => "video/ogg",
            _ => "video/mp4", // default
        }
    }
}

impl Render for VideoPlayer<'_> {
    fn render(&self) -> Markup {
        let video_type = self.content_type.unwrap_or_else(|| self.inferred_type());

        html! {
            div class="media-container" {
                div class="video-wrapper" {
                    video controls preload="metadata" poster=[self.poster] {
                        source src=(self.src) type=(video_type);
                        "Your browser does not support the video tag."
                    }
                }
            }
        }
    }
}

/// Audio player component.
#[derive(Debug, Clone)]
pub struct AudioPlayer<'a> {
    /// Source URL for the audio
    pub src: &'a str,
    /// Optional MIME type for the audio source
    pub content_type: Option<&'a str>,
}

impl<'a> AudioPlayer<'a> {
    /// Create a new audio player.
    #[must_use]
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            content_type: None,
        }
    }

    /// Set the content type (MIME type) for the audio.
    #[must_use]
    pub fn with_content_type(mut self, content_type: &'a str) -> Self {
        self.content_type = Some(content_type);
        self
    }

    /// Infer the audio MIME type from the source URL extension.
    fn inferred_type(&self) -> &'static str {
        let extension = self.src.rsplit('.').next().unwrap_or("").to_lowercase();

        match extension.as_str() {
            "mp3" => "audio/mpeg",
            "wav" => "audio/wav",
            "ogg" | "oga" => "audio/ogg",
            "m4a" => "audio/mp4",
            "flac" => "audio/flac",
            "aac" => "audio/aac",
            "webm" => "audio/webm",
            _ => "audio/mpeg", // default
        }
    }
}

impl Render for AudioPlayer<'_> {
    fn render(&self) -> Markup {
        let audio_type = self.content_type.unwrap_or_else(|| self.inferred_type());

        html! {
            div class="media-container" {
                div class="audio-wrapper" {
                    audio controls preload="metadata" {
                        source src=(self.src) type=(audio_type);
                        "Your browser does not support the audio element."
                    }
                }
            }
        }
    }
}

/// Image viewer component.
#[derive(Debug, Clone)]
pub struct ImageViewer<'a> {
    /// Source URL for the image
    pub src: &'a str,
    /// Optional alt text for accessibility
    pub alt: Option<&'a str>,
    /// Optional CSS class(es) to add
    pub class: Option<&'a str>,
    /// Whether to use lazy loading
    pub lazy: bool,
}

impl<'a> ImageViewer<'a> {
    /// Create a new image viewer.
    #[must_use]
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            alt: None,
            class: None,
            lazy: true,
        }
    }

    /// Set the alt text for the image.
    #[must_use]
    pub fn with_alt(mut self, alt: &'a str) -> Self {
        self.alt = Some(alt);
        self
    }

    /// Add CSS class(es) to the image.
    #[must_use]
    pub fn with_class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }

    /// Enable or disable lazy loading.
    #[must_use]
    pub fn lazy(mut self, lazy: bool) -> Self {
        self.lazy = lazy;
        self
    }
}

impl Render for ImageViewer<'_> {
    fn render(&self) -> Markup {
        let alt_text = self.alt.unwrap_or("Archived media");
        let loading = if self.lazy { Some("lazy") } else { None };

        html! {
            div class="media-container" {
                img src=(self.src) alt=(alt_text) class=[self.class] loading=[loading];
            }
        }
    }
}

/// Container wrapper for media content.
#[derive(Debug, Clone)]
pub struct MediaContainer {
    /// The inner media content
    pub content: Markup,
    /// Optional type badge to display
    pub type_badge: Option<MediaTypeBadge>,
}

impl MediaContainer {
    /// Create a new media container with content.
    #[must_use]
    pub fn new(content: Markup) -> Self {
        Self {
            content,
            type_badge: None,
        }
    }

    /// Add a type badge to the container.
    #[must_use]
    pub fn with_badge(mut self, badge: MediaTypeBadge) -> Self {
        self.type_badge = Some(badge);
        self
    }
}

impl Render for MediaContainer {
    fn render(&self) -> Markup {
        html! {
            div class="media-container" {
                @if let Some(ref badge) = self.type_badge {
                    (badge)
                }
                (self.content)
            }
        }
    }
}

/// Gallery component for displaying multiple images.
#[derive(Debug, Clone)]
pub struct MediaGallery<'a> {
    /// URLs of images in the gallery
    pub images: Vec<&'a str>,
}

impl<'a> MediaGallery<'a> {
    /// Create a new gallery with the given images.
    #[must_use]
    pub fn new(images: Vec<&'a str>) -> Self {
        Self { images }
    }

    /// Add an image to the gallery.
    #[must_use]
    pub fn add_image(mut self, src: &'a str) -> Self {
        self.images.push(src);
        self
    }
}

impl Render for MediaGallery<'_> {
    fn render(&self) -> Markup {
        html! {
            div class="media-gallery" {
                @for (index, src) in self.images.iter().enumerate() {
                    img src=(src) alt=(format!("Gallery image {}", index + 1)) loading="lazy";
                }
            }
        }
    }
}

/// Render a complete media player based on file extension and content type.
///
/// This function determines the appropriate player type based on the S3 key
/// extension and optional content type hint.
#[must_use]
pub fn render_media_player(
    s3_key: &str,
    content_type: Option<&str>,
    thumbnail: Option<&str>,
) -> Markup {
    render_media_player_with_options(s3_key, content_type, thumbnail, true)
}

/// Render a complete media player with options for badge display.
///
/// This function determines the appropriate player type based on the S3 key
/// extension and optional content type hint. The `show_badge` parameter controls
/// whether to render the media type badge inside the player.
#[must_use]
pub fn render_media_player_with_options(
    s3_key: &str,
    content_type: Option<&str>,
    thumbnail: Option<&str>,
    show_badge: bool,
) -> Markup {
    let extension = s3_key.rsplit('.').next().unwrap_or("").to_lowercase();
    let media_url = format!("/s3/{}", html_escape(s3_key));
    let type_badge = if show_badge {
        Some(MediaTypeBadge::from_content_type(
            content_type.unwrap_or("unknown"),
        ))
    } else {
        None
    };

    // Video formats
    if matches!(
        extension.as_str(),
        "mp4" | "webm" | "mkv" | "mov" | "avi" | "m4v"
    ) || content_type == Some("video")
    {
        let poster_url = thumbnail.map(|t| format!("/s3/{}", html_escape(t)));
        return html! {
            div class="media-container" {
                @if let Some(badge) = type_badge {
                    (badge)
                }
                div class="video-wrapper" {
                    video controls preload="metadata" poster=[poster_url] {
                        source src=(media_url) type=(video_mime_type(&extension));
                        "Your browser does not support the video tag."
                    }
                }
            }
        };
    }

    // Audio formats
    if matches!(
        extension.as_str(),
        "mp3" | "wav" | "ogg" | "m4a" | "flac" | "aac"
    ) || content_type == Some("audio")
    {
        return html! {
            div class="media-container" {
                @if let Some(badge) = type_badge {
                    (badge)
                }
                div class="audio-wrapper" {
                    audio controls preload="metadata" {
                        source src=(media_url) type=(audio_mime_type(&extension));
                        "Your browser does not support the audio element."
                    }
                }
            }
        };
    }

    // Image formats
    if matches!(
        extension.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp"
    ) || content_type == Some("image")
        || content_type == Some("gallery")
    {
        return html! {
            div class="media-container" {
                @if let Some(badge) = type_badge {
                    (badge)
                }
                img src=(media_url) alt="Archived media" loading="lazy";
            }
        };
    }

    // Default: show thumbnail if available, otherwise just indicate file type
    if let Some(thumb) = thumbnail {
        let thumb_url = format!("/s3/{}", html_escape(thumb));
        html! {
            div class="media-container" {
                @if let Some(badge) = type_badge {
                    (badge)
                }
                img src=(thumb_url) alt="Thumbnail" class="archive-thumb";
                p {
                    small { "Preview image shown. Full media available for download." }
                }
            }
        }
    } else {
        html! {
            div class="media-container" {
                @if let Some(badge) = type_badge {
                    (badge)
                }
                p { "Media preview not available." }
            }
        }
    }
}

/// HTML-escape a string for safe inclusion in HTML.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// Get the MIME type for a video file extension.
fn video_mime_type(extension: &str) -> &'static str {
    match extension {
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        _ => "video/mp4",
    }
}

/// Get the MIME type for an audio file extension.
fn audio_mime_type(extension: &str) -> &'static str {
    match extension {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        _ => "audio/mpeg",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_player_new() {
        let player = VideoPlayer::new("/video.mp4");
        assert_eq!(player.src, "/video.mp4");
        assert!(player.poster.is_none());
        assert!(player.content_type.is_none());
    }

    #[test]
    fn test_video_player_with_poster() {
        let player = VideoPlayer::new("/video.mp4").with_poster("/thumb.jpg");
        assert_eq!(player.poster, Some("/thumb.jpg"));
    }

    #[test]
    fn test_video_player_render() {
        let player = VideoPlayer::new("/video.mp4");
        let html = player.render().into_string();

        assert!(html.contains("class=\"media-container\""));
        assert!(html.contains("class=\"video-wrapper\""));
        assert!(html.contains("<video"));
        assert!(html.contains("controls"));
        assert!(html.contains("preload=\"metadata\""));
        assert!(html.contains("src=\"/video.mp4\""));
        assert!(html.contains("type=\"video/mp4\""));
    }

    #[test]
    fn test_video_player_render_with_poster() {
        let player = VideoPlayer::new("/video.mp4").with_poster("/thumb.jpg");
        let html = player.render().into_string();

        assert!(html.contains("poster=\"/thumb.jpg\""));
    }

    #[test]
    fn test_video_player_inferred_type() {
        assert_eq!(VideoPlayer::new("/test.mp4").inferred_type(), "video/mp4");
        assert_eq!(VideoPlayer::new("/test.webm").inferred_type(), "video/webm");
        assert_eq!(
            VideoPlayer::new("/test.mkv").inferred_type(),
            "video/x-matroska"
        );
    }

    #[test]
    fn test_audio_player_new() {
        let player = AudioPlayer::new("/audio.mp3");
        assert_eq!(player.src, "/audio.mp3");
        assert!(player.content_type.is_none());
    }

    #[test]
    fn test_audio_player_render() {
        let player = AudioPlayer::new("/audio.mp3");
        let html = player.render().into_string();

        assert!(html.contains("class=\"media-container\""));
        assert!(html.contains("class=\"audio-wrapper\""));
        assert!(html.contains("<audio"));
        assert!(html.contains("controls"));
        assert!(html.contains("preload=\"metadata\""));
        assert!(html.contains("src=\"/audio.mp3\""));
        assert!(html.contains("type=\"audio/mpeg\""));
    }

    #[test]
    fn test_audio_player_inferred_type() {
        assert_eq!(AudioPlayer::new("/test.mp3").inferred_type(), "audio/mpeg");
        assert_eq!(AudioPlayer::new("/test.wav").inferred_type(), "audio/wav");
        assert_eq!(AudioPlayer::new("/test.ogg").inferred_type(), "audio/ogg");
        assert_eq!(AudioPlayer::new("/test.m4a").inferred_type(), "audio/mp4");
    }

    #[test]
    fn test_image_viewer_new() {
        let viewer = ImageViewer::new("/image.jpg");
        assert_eq!(viewer.src, "/image.jpg");
        assert!(viewer.alt.is_none());
        assert!(viewer.class.is_none());
        assert!(viewer.lazy);
    }

    #[test]
    fn test_image_viewer_render() {
        let viewer = ImageViewer::new("/image.jpg");
        let html = viewer.render().into_string();

        assert!(html.contains("class=\"media-container\""));
        assert!(html.contains("<img"));
        assert!(html.contains("src=\"/image.jpg\""));
        assert!(html.contains("alt=\"Archived media\""));
        assert!(html.contains("loading=\"lazy\""));
    }

    #[test]
    fn test_image_viewer_with_alt() {
        let viewer = ImageViewer::new("/image.jpg").with_alt("Custom alt text");
        let html = viewer.render().into_string();

        assert!(html.contains("alt=\"Custom alt text\""));
    }

    #[test]
    fn test_image_viewer_with_class() {
        let viewer = ImageViewer::new("/image.jpg").with_class("custom-class");
        let html = viewer.render().into_string();

        assert!(html.contains("class=\"custom-class\""));
    }

    #[test]
    fn test_image_viewer_no_lazy() {
        let viewer = ImageViewer::new("/image.jpg").lazy(false);
        let html = viewer.render().into_string();

        assert!(!html.contains("loading="));
    }

    #[test]
    fn test_media_type_badge_render() {
        let badge = MediaTypeBadge::new(MediaTypeVariant::Video);
        let html = badge.render().into_string();

        assert!(html.contains("class=\"media-type-badge media-type-video\""));
        assert!(html.contains("Video"));
    }

    #[test]
    fn test_media_type_badge_from_content_type() {
        assert_eq!(
            MediaTypeBadge::from_content_type("video").variant,
            MediaTypeVariant::Video
        );
        assert_eq!(
            MediaTypeBadge::from_content_type("audio").variant,
            MediaTypeVariant::Audio
        );
        assert_eq!(
            MediaTypeBadge::from_content_type("image").variant,
            MediaTypeVariant::Image
        );
        assert_eq!(
            MediaTypeBadge::from_content_type("gallery").variant,
            MediaTypeVariant::Gallery
        );
        assert_eq!(
            MediaTypeBadge::from_content_type("unknown").variant,
            MediaTypeVariant::Unknown
        );
    }

    #[test]
    fn test_media_gallery_new() {
        let gallery = MediaGallery::new(vec!["/img1.jpg", "/img2.jpg"]);
        assert_eq!(gallery.images.len(), 2);
    }

    #[test]
    fn test_media_gallery_add_image() {
        let gallery = MediaGallery::new(vec!["/img1.jpg"]).add_image("/img2.jpg");
        assert_eq!(gallery.images.len(), 2);
    }

    #[test]
    fn test_media_gallery_render() {
        let gallery = MediaGallery::new(vec!["/img1.jpg", "/img2.jpg"]);
        let html = gallery.render().into_string();

        assert!(html.contains("class=\"media-gallery\""));
        assert!(html.contains("src=\"/img1.jpg\""));
        assert!(html.contains("src=\"/img2.jpg\""));
        assert!(html.contains("alt=\"Gallery image 1\""));
        assert!(html.contains("alt=\"Gallery image 2\""));
        assert!(html.contains("loading=\"lazy\""));
    }

    #[test]
    fn test_render_media_player_video() {
        let html = render_media_player("archives/test.mp4", Some("video"), None).into_string();

        assert!(html.contains("class=\"media-container\""));
        assert!(html.contains("class=\"video-wrapper\""));
        assert!(html.contains("<video"));
        assert!(html.contains("/s3/archives/test.mp4"));
    }

    #[test]
    fn test_render_media_player_audio() {
        let html = render_media_player("archives/test.mp3", Some("audio"), None).into_string();

        assert!(html.contains("class=\"audio-wrapper\""));
        assert!(html.contains("<audio"));
    }

    #[test]
    fn test_render_media_player_image() {
        let html = render_media_player("archives/test.jpg", Some("image"), None).into_string();

        assert!(html.contains("<img"));
        assert!(html.contains("alt=\"Archived media\""));
    }

    #[test]
    fn test_render_media_player_with_thumbnail() {
        let html = render_media_player("archives/test.mp4", Some("video"), Some("thumb.jpg"))
            .into_string();

        assert!(html.contains("poster=\"/s3/thumb.jpg\""));
    }

    #[test]
    fn test_render_media_player_unknown_type() {
        let html = render_media_player("archives/test.xyz", None, Some("thumb.jpg")).into_string();

        assert!(html.contains("archive-thumb"));
        assert!(html.contains("Preview image shown"));
    }

    #[test]
    fn test_render_media_player_no_preview() {
        let html = render_media_player("archives/test.xyz", None, None).into_string();

        assert!(html.contains("Media preview not available"));
    }

    #[test]
    fn test_media_container_render() {
        let content = html! { p { "Test content" } };
        let container =
            MediaContainer::new(content).with_badge(MediaTypeBadge::new(MediaTypeVariant::Video));
        let html = container.render().into_string();

        assert!(html.contains("class=\"media-container\""));
        assert!(html.contains("media-type-video"));
        assert!(html.contains("Test content"));
    }

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a & b"), "a &amp; b");
        assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
    }
}
