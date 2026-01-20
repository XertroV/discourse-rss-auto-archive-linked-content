//! Pagination component for navigating through multi-page content.
//!
//! This module provides a reusable pagination component that renders
//! navigation controls with first, previous, page numbers, next, and last links.

use maud::{html, Markup, Render};
use urlencoding::encode;

/// Pagination component for navigating through multi-page content.
///
/// Displays: First, Prev, current-2, current-1, current, current+1, current+2, Next, Last
/// Automatically hides if there's only 1 page.
#[derive(Debug, Clone)]
pub struct Pagination {
    /// Current page number (0-indexed internally, displayed as 1-indexed)
    pub current_page: usize,
    /// Total number of pages
    pub total_pages: usize,
    /// Base URL for page links (query params will be appended)
    pub base_url: String,
    /// Content type filter to preserve in links
    pub content_type_filter: Option<String>,
    /// Source filter to preserve in links
    pub source_filter: Option<String>,
}

impl Pagination {
    /// Create a new pagination component.
    ///
    /// # Arguments
    /// * `current_page` - Current page number (0-indexed)
    /// * `total_pages` - Total number of pages
    /// * `base_url` - Base URL for page links
    #[must_use]
    pub fn new(current_page: usize, total_pages: usize, base_url: &str) -> Self {
        Self {
            current_page,
            total_pages,
            base_url: base_url.to_string(),
            content_type_filter: None,
            source_filter: None,
        }
    }

    /// Add a content type filter to preserve in pagination links.
    #[must_use]
    pub fn with_content_type_filter(mut self, filter: Option<&str>) -> Self {
        self.content_type_filter = filter.map(String::from);
        self
    }

    /// Add a source filter to preserve in pagination links.
    #[must_use]
    pub fn with_source_filter(mut self, filter: Option<&str>) -> Self {
        self.source_filter = filter.map(String::from);
        self
    }

    /// Build URL for a specific page number with all filters preserved.
    fn build_url(&self, page_num: usize) -> String {
        let mut params = Vec::new();

        if page_num > 0 {
            params.push(format!("page={page_num}"));
        }

        if let Some(ref ct) = self.content_type_filter {
            let encoded = encode(ct);
            params.push(format!("type={encoded}"));
        }

        if let Some(ref src) = self.source_filter {
            let encoded = encode(src);
            params.push(format!("source={encoded}"));
        }

        if params.is_empty() {
            self.base_url.clone()
        } else {
            let query = params.join("&");
            format!("{}?{}", self.base_url, query)
        }
    }

    /// Check if pagination should be displayed.
    #[must_use]
    pub fn should_display(&self) -> bool {
        self.total_pages > 1
    }
}

impl Render for Pagination {
    fn render(&self) -> Markup {
        // Don't render anything if only one page
        if !self.should_display() {
            return html! {};
        }

        let current = self.current_page;
        let total = self.total_pages;

        // Calculate the range of page numbers to display
        let start = current.saturating_sub(2);
        let end = (current + 3).min(total);

        html! {
            nav class="pagination" {
                // Previous button
                @if current > 0 {
                    a href=(self.build_url(current - 1)) { "\u{00ab} Previous" }
                } @else {
                    span class="disabled" { "\u{00ab} Previous" }
                }

                // First page and ellipsis if needed
                @if start > 0 {
                    a href=(self.build_url(0)) { "1" }
                    @if start > 1 {
                        span { "..." }
                    }
                }

                // Page numbers around current page
                @for page_num in start..end {
                    @if page_num == current {
                        span class="current" { (page_num + 1) }
                    } @else {
                        a href=(self.build_url(page_num)) { (page_num + 1) }
                    }
                }

                // Ellipsis and last page if needed
                @if end < total {
                    @if end < total - 1 {
                        span { "..." }
                    }
                    a href=(self.build_url(total - 1)) { (total) }
                }

                // Next button
                @if current + 1 < total {
                    a href=(self.build_url(current + 1)) { "Next \u{00bb}" }
                } @else {
                    span class="disabled" { "Next \u{00bb}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_new() {
        let pagination = Pagination::new(0, 10, "/");
        assert_eq!(pagination.current_page, 0);
        assert_eq!(pagination.total_pages, 10);
        assert_eq!(pagination.base_url, "/");
        assert!(pagination.content_type_filter.is_none());
        assert!(pagination.source_filter.is_none());
    }

    #[test]
    fn test_pagination_with_filters() {
        let pagination = Pagination::new(0, 10, "/")
            .with_content_type_filter(Some("video"))
            .with_source_filter(Some("reddit.com"));

        assert_eq!(pagination.content_type_filter, Some("video".to_string()));
        assert_eq!(pagination.source_filter, Some("reddit.com".to_string()));
    }

    #[test]
    fn test_build_url_no_params() {
        let pagination = Pagination::new(0, 10, "/archives");
        // Page 0 should not have page param
        assert_eq!(pagination.build_url(0), "/archives");
    }

    #[test]
    fn test_build_url_with_page() {
        let pagination = Pagination::new(0, 10, "/archives");
        assert_eq!(pagination.build_url(5), "/archives?page=5");
    }

    #[test]
    fn test_build_url_with_filters() {
        let pagination = Pagination::new(0, 10, "/")
            .with_content_type_filter(Some("video"))
            .with_source_filter(Some("reddit.com"));

        let url = pagination.build_url(2);
        assert!(url.contains("page=2"));
        assert!(url.contains("type=video"));
        assert!(url.contains("source=reddit.com"));
    }

    #[test]
    fn test_should_display_single_page() {
        let pagination = Pagination::new(0, 1, "/");
        assert!(!pagination.should_display());
    }

    #[test]
    fn test_should_display_multiple_pages() {
        let pagination = Pagination::new(0, 5, "/");
        assert!(pagination.should_display());
    }

    #[test]
    fn test_render_single_page_empty() {
        let pagination = Pagination::new(0, 1, "/");
        let html = pagination.render().into_string();
        assert!(html.is_empty());
    }

    #[test]
    fn test_render_first_page() {
        let pagination = Pagination::new(0, 10, "/");
        let html = pagination.render().into_string();

        // Should have disabled previous
        assert!(html.contains("class=\"disabled\""));
        assert!(html.contains("Previous"));

        // Should have current page marked
        assert!(html.contains("class=\"current\""));
        assert!(html.contains(">1<")); // Page 1 should be displayed

        // Should have next link
        assert!(html.contains("Next"));
    }

    #[test]
    fn test_render_middle_page() {
        let pagination = Pagination::new(5, 10, "/");
        let html = pagination.render().into_string();

        // Should have first page link
        assert!(html.contains(">1<"));

        // Should have ellipsis
        assert!(html.contains("..."));

        // Should have page numbers around current
        assert!(html.contains(">4<")); // current - 2 + 1
        assert!(html.contains(">5<")); // current - 1 + 1
        assert!(html.contains(">6<")); // current + 1 (displayed)
        assert!(html.contains(">7<")); // current + 1 + 1
        assert!(html.contains(">8<")); // current + 2 + 1

        // Should have last page link
        assert!(html.contains(">10<"));
    }

    #[test]
    fn test_render_last_page() {
        let pagination = Pagination::new(9, 10, "/");
        let html = pagination.render().into_string();

        // Should have previous link
        assert!(html.contains("Previous"));
        assert!(html.contains("page=8")); // Previous page link

        // Should have disabled next
        assert!(html.contains("class=\"disabled\""));
    }

    #[test]
    fn test_render_preserves_filters() {
        let pagination = Pagination::new(2, 10, "/archives/all")
            .with_content_type_filter(Some("video"))
            .with_source_filter(Some("youtube.com"));

        let html = pagination.render().into_string();

        // All links should preserve filters
        assert!(html.contains("type=video"));
        assert!(html.contains("source=youtube.com"));
    }
}
