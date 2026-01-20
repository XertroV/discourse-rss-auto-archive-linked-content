//! Open Graph and Twitter Card metadata components.
//!
//! This module provides components for generating social media preview metadata.

use maud::{html, Markup};

/// Open Graph metadata for social media previews.
///
/// This component generates both Open Graph and Twitter Card meta tags
/// for better social media sharing previews.
#[derive(Debug, Clone)]
pub struct OpenGraphMetadata {
    /// Page title (og:title)
    pub title: String,
    /// Page description (og:description)
    pub description: String,
    /// Page URL (og:url)
    pub url: String,
    /// Open Graph type (og:type) - e.g., "website", "article"
    pub og_type: String,
    /// Image URL (og:image) - None for NSFW content
    pub image: Option<String>,
    /// Site name (og:site_name)
    pub site_name: String,
    /// Twitter card type - "summary" or "summary_large_image"
    pub twitter_card: String,
    /// Whether this is NSFW content (adds [NSFW] prefix to title)
    pub is_nsfw: bool,
}

impl Default for OpenGraphMetadata {
    fn default() -> Self {
        Self {
            title: "Discourse Link Archiver".to_string(),
            description: "Preserving online content from across the web".to_string(),
            url: "/".to_string(),
            og_type: "website".to_string(),
            image: None,
            site_name: "CF Archive".to_string(),
            twitter_card: "summary".to_string(),
            is_nsfw: false,
        }
    }
}

impl OpenGraphMetadata {
    /// Create a new metadata builder.
    #[must_use]
    pub fn new(
        title: impl Into<String>,
        description: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            url: url.into(),
            ..Default::default()
        }
    }

    /// Set the Open Graph type.
    #[must_use]
    pub fn with_type(mut self, og_type: impl Into<String>) -> Self {
        self.og_type = og_type.into();
        self
    }

    /// Set the image URL (only if not NSFW).
    #[must_use]
    pub fn with_image(mut self, image: Option<impl Into<String>>) -> Self {
        self.image = image.map(Into::into);
        self
    }

    /// Mark as NSFW content (adds [NSFW] prefix and removes image).
    #[must_use]
    pub fn with_nsfw(mut self, is_nsfw: bool) -> Self {
        self.is_nsfw = is_nsfw;
        if is_nsfw {
            self.image = None; // Never show images for NSFW content
        }
        self
    }

    /// Set the site name.
    #[must_use]
    pub fn with_site_name(mut self, site_name: impl Into<String>) -> Self {
        self.site_name = site_name.into();
        self
    }

    /// Set the Twitter card type.
    #[must_use]
    pub fn with_twitter_card(mut self, card_type: impl Into<String>) -> Self {
        self.twitter_card = card_type.into();
        self
    }

    /// Get the final title with NSFW prefix if needed.
    fn formatted_title(&self) -> String {
        if self.is_nsfw {
            format!("[NSFW] {}", self.title)
        } else {
            self.title.to_string()
        }
    }

    /// Render the metadata tags.
    pub fn render(&self) -> Markup {
        let title = self.formatted_title();
        let description = &self.description;

        html! {
            // Open Graph metadata
            meta property="og:title" content=(title);
            meta property="og:description" content=(description);
            meta property="og:url" content=(&self.url);
            meta property="og:type" content=(&self.og_type);
            meta property="og:site_name" content=(&self.site_name);

            @if let Some(ref image_url) = self.image {
                meta property="og:image" content=(image_url);
                meta property="og:image:alt" content=(&title);
            }

            // Twitter Card metadata
            meta name="twitter:card" content=(&self.twitter_card);
            meta name="twitter:title" content=(&title);
            meta name="twitter:description" content=(description);

            @if let Some(ref image_url) = self.image {
                meta name="twitter:image" content=(image_url);
                meta name="twitter:image:alt" content=(&title);
            }

            // Standard meta description
            meta name="description" content=(description);
        }
    }
}

/// Helper to truncate text to a maximum length with ellipsis.
pub fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        let mut truncated = text.chars().take(max_len - 3).collect::<String>();
        truncated.push_str("...");
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_metadata() {
        let meta = OpenGraphMetadata::default();
        assert_eq!(meta.title, "Discourse Link Archiver");
        assert_eq!(meta.og_type, "website");
        assert!(!meta.is_nsfw);
        assert!(meta.image.is_none());
    }

    #[test]
    fn test_nsfw_prefix() {
        let meta =
            OpenGraphMetadata::new("Test Title", "Test description", "/test").with_nsfw(true);

        assert_eq!(meta.formatted_title(), "[NSFW] Test Title");
        assert!(meta.image.is_none());
    }

    #[test]
    fn test_nsfw_removes_image() {
        let meta = OpenGraphMetadata::new("Title", "Description", "/url")
            .with_image(Some("https://example.com/image.jpg"))
            .with_nsfw(true);

        assert!(meta.image.is_none());
    }

    #[test]
    fn test_render_basic() {
        let meta = OpenGraphMetadata::new("Test Page", "A test page description", "/test");
        let html = meta.render().into_string();

        assert!(html.contains(r#"property="og:title" content="Test Page""#));
        assert!(html.contains(r#"property="og:description" content="A test page description""#));
        assert!(html.contains(r#"property="og:url" content="/test""#));
        assert!(html.contains(r#"name="twitter:card" content="summary""#));
    }

    #[test]
    fn test_render_with_image() {
        let meta = OpenGraphMetadata::new("Test", "Description", "/url")
            .with_image(Some("https://example.com/image.jpg"));
        let html = meta.render().into_string();

        assert!(html.contains(r#"property="og:image" content="https://example.com/image.jpg""#));
        assert!(html.contains(r#"name="twitter:image" content="https://example.com/image.jpg""#));
    }

    #[test]
    fn test_render_nsfw() {
        let meta = OpenGraphMetadata::new("NSFW Title", "Description", "/url").with_nsfw(true);
        let html = meta.render().into_string();

        assert!(html.contains(r#"content="[NSFW] NSFW Title""#));
        assert!(!html.contains(r#"property="og:image""#));
        assert!(!html.contains(r#"name="twitter:image""#));
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("Hello", 10), "Hello");
        assert_eq!(truncate_text("Hello World", 8), "Hello...");
        assert_eq!(truncate_text("Test", 4), "Test");
        assert_eq!(truncate_text("Testing", 5), "Te...");
    }
}
