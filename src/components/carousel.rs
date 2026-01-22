//! Carousel component for image galleries.
//!
//! Provides a responsive, accessible carousel with navigation controls
//! and thumbnail strip for browsing multi-image galleries (TikTok, Twitter).

use maud::{html, Markup, PreEscaped, Render};

/// A single image in a carousel.
#[derive(Debug, Clone)]
pub struct CarouselImage<'a> {
    /// S3 key for the image source
    pub src: &'a str,
    /// Alt text for accessibility
    pub alt: String,
}

impl<'a> CarouselImage<'a> {
    /// Create a new carousel image.
    #[must_use]
    pub fn new(src: &'a str, alt: String) -> Self {
        Self { src, alt }
    }
}

/// Carousel component for browsing image galleries.
///
/// # Example
///
/// ```ignore
/// use crate::components::Carousel;
///
/// let carousel = Carousel::new("tiktok-123")
///     .add_image("/archives/123/photo_001.jpg", "Photo 1".to_string())
///     .add_image("/archives/123/photo_002.jpg", "Photo 2".to_string())
///     .show_thumbnails(true);
///
/// html! {
///     (carousel)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Carousel<'a> {
    /// Unique ID for this carousel instance
    pub id: String,
    /// Images to display in the carousel
    pub images: Vec<CarouselImage<'a>>,
    /// Whether to show thumbnail strip
    pub show_thumbnails: bool,
    /// Initial index to display
    pub initial_index: usize,
    /// Whether this carousel contains NSFW content
    pub is_nsfw: bool,
}

impl<'a> Carousel<'a> {
    /// Create a new carousel with the given ID.
    #[must_use]
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            images: Vec::new(),
            show_thumbnails: true,
            initial_index: 0,
            is_nsfw: false,
        }
    }

    /// Add an image to the carousel.
    #[must_use]
    pub fn add_image(mut self, src: &'a str, alt: String) -> Self {
        self.images.push(CarouselImage::new(src, alt));
        self
    }

    /// Set all images at once.
    #[must_use]
    pub fn with_images(mut self, images: Vec<CarouselImage<'a>>) -> Self {
        self.images = images;
        self
    }

    /// Set whether to show thumbnail strip.
    #[must_use]
    pub fn show_thumbnails(mut self, show: bool) -> Self {
        self.show_thumbnails = show;
        self
    }

    /// Set the initial index to display.
    #[must_use]
    pub fn initial_index(mut self, index: usize) -> Self {
        self.initial_index = index;
        self
    }

    /// Mark this carousel as containing NSFW content.
    #[must_use]
    pub fn nsfw(mut self, is_nsfw: bool) -> Self {
        self.is_nsfw = is_nsfw;
        self
    }

    /// Helper to escape HTML in attributes
    fn html_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#x27;")
    }
}

impl Render for Carousel<'_> {
    fn render(&self) -> Markup {
        let total = self.images.len();
        if total == 0 {
            return html! {};
        }

        let nsfw_attr = if self.is_nsfw { Some("true") } else { None };

        html! {
            div class="carousel" id=(format!("carousel-{}", self.id)) data-nsfw=[nsfw_attr] {
                // Main viewport
                div class="carousel-viewport" {
                    // Previous button
                    button class="carousel-nav carousel-nav-prev"
                           type="button"
                           aria-label="Previous image" {
                        // Left arrow SVG
                        (PreEscaped(r#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M15.41 7.41L14 6l-6 6 6 6 1.41-1.41L10.83 12z"/></svg>"#))
                    }

                    // Next button
                    button class="carousel-nav carousel-nav-next"
                           type="button"
                           aria-label="Next image" {
                        // Right arrow SVG
                        (PreEscaped(r#"<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M10 6L8.59 7.41 13.17 12l-4.58 4.59L10 18l6-6z"/></svg>"#))
                    }

                    // Scrollable track
                    div class="carousel-track" role="region" aria-label="Image gallery" {
                        @for (index, image) in self.images.iter().enumerate() {
                            div class="carousel-item" data-index=(index) {
                                img src=(format!("/s3/{}", Self::html_escape(image.src)))
                                    alt=(image.alt)
                                    loading=(if index < 3 { "eager" } else { "lazy" });
                            }
                        }
                    }
                }

                // Thumbnail strip
                @if self.show_thumbnails {
                    div class="carousel-thumbnails" role="tablist" aria-label="Gallery thumbnails" {
                        @for (index, image) in self.images.iter().enumerate() {
                            @let is_active = index == self.initial_index;
                            @let active_class = if is_active { "carousel-thumb active" } else { "carousel-thumb" };
                            button class=(active_class)
                                   type="button"
                                   role="tab"
                                   data-index=(index)
                                   aria-label=(format!("Go to image {}", index + 1))
                                   aria-current=[is_active.then_some("true")] {
                                img src=(format!("/s3/{}", Self::html_escape(image.src)))
                                    alt=""
                                    loading="lazy";
                            }
                        }
                    }
                }

                // Image counter
                div class="carousel-counter" aria-live="polite" {
                    span class="carousel-current" { "1" }
                    " / "
                    span class="carousel-total" { (total) }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_carousel_new() {
        let carousel = Carousel::new("test-123");
        assert_eq!(carousel.id, "test-123");
        assert!(carousel.images.is_empty());
        assert!(carousel.show_thumbnails);
        assert_eq!(carousel.initial_index, 0);
        assert!(!carousel.is_nsfw);
    }

    #[test]
    fn test_carousel_add_image() {
        let carousel = Carousel::new("test")
            .add_image("photo_001.jpg", "Photo 1".to_string())
            .add_image("photo_002.jpg", "Photo 2".to_string());

        assert_eq!(carousel.images.len(), 2);
        assert_eq!(carousel.images[0].src, "photo_001.jpg");
        assert_eq!(carousel.images[1].alt, "Photo 2");
    }

    #[test]
    fn test_carousel_builder_methods() {
        let carousel = Carousel::new("test")
            .show_thumbnails(false)
            .initial_index(2)
            .nsfw(true);

        assert!(!carousel.show_thumbnails);
        assert_eq!(carousel.initial_index, 2);
        assert!(carousel.is_nsfw);
    }

    #[test]
    fn test_carousel_render_empty() {
        let carousel = Carousel::new("test");
        let html = carousel.render().into_string();

        // Empty carousel should render nothing
        assert!(html.is_empty() || html.trim().is_empty());
    }

    #[test]
    fn test_carousel_render_with_images() {
        let carousel = Carousel::new("test-123")
            .add_image("photo_001.jpg", "First photo".to_string())
            .add_image("photo_002.jpg", "Second photo".to_string());

        let html = carousel.render().into_string();

        assert!(html.contains("carousel"));
        assert!(html.contains("carousel-track"));
        assert!(html.contains("carousel-thumbnails"));
        assert!(html.contains("photo_001.jpg"));
        assert!(html.contains("photo_002.jpg"));
        assert!(html.contains("First photo"));
        assert!(html.contains("2")); // Total count
    }

    #[test]
    fn test_carousel_nsfw_attribute() {
        let carousel = Carousel::new("test")
            .add_image("image.jpg", "Test".to_string())
            .nsfw(true);

        let html = carousel.render().into_string();
        assert!(html.contains("data-nsfw=\"true\""));
    }

    #[test]
    fn test_carousel_html_escape() {
        let escaped = Carousel::html_escape("test<>&\"'");
        assert_eq!(escaped, "test&lt;&gt;&amp;&quot;&#x27;");
    }

    #[test]
    fn test_carousel_with_images() {
        let images = vec![
            CarouselImage::new("img1.jpg", "Image 1".to_string()),
            CarouselImage::new("img2.jpg", "Image 2".to_string()),
        ];

        let carousel = Carousel::new("test").with_images(images);

        assert_eq!(carousel.images.len(), 2);
        assert_eq!(carousel.images[0].src, "img1.jpg");
    }
}
