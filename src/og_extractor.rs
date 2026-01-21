//! Open Graph metadata extraction from HTML content.
//!
//! This module provides functionality to extract Open Graph metadata from archived
//! HTML pages and cache it in the database for better social media previews.

use anyhow::Result;
use scraper::{Html, Selector};

/// Extracted Open Graph metadata from an HTML page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractedOgMetadata {
    /// og:title - The title of the content
    pub title: Option<String>,
    /// og:description - A brief description of the content
    pub description: Option<String>,
    /// og:image - URL of an image for social sharing
    pub image: Option<String>,
    /// og:type - The type of content (article, website, video, etc.)
    pub og_type: Option<String>,
}

impl ExtractedOgMetadata {
    /// Check if any metadata was extracted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.description.is_none()
            && self.image.is_none()
            && self.og_type.is_none()
    }

    /// Check if this metadata has meaningful content (at least title or description).
    #[must_use]
    pub fn has_content(&self) -> bool {
        self.title.is_some() || self.description.is_some()
    }
}

/// Extract Open Graph metadata from HTML content.
///
/// This function parses the HTML and looks for `<meta property="og:*">` tags
/// in the `<head>` section. It returns all found OG metadata.
///
/// # Errors
///
/// Returns an error if HTML parsing fails.
///
/// # Examples
///
/// ```ignore
/// let html = r#"
/// <html>
///   <head>
///     <meta property="og:title" content="Example Page">
///     <meta property="og:description" content="This is an example">
///   </head>
/// </html>
/// "#;
///
/// let metadata = extract_og_metadata(html)?;
/// assert_eq!(metadata.title, Some("Example Page".to_string()));
/// ```
pub fn extract_og_metadata(html: &str) -> Result<ExtractedOgMetadata> {
    let document = Html::parse_document(html);

    // Create selectors for OG meta tags
    let og_meta_selector = Selector::parse(r#"meta[property^="og:"]"#)
        .map_err(|e| anyhow::anyhow!("Failed to create selector: {:?}", e))?;

    let mut metadata = ExtractedOgMetadata::default();

    // Extract all OG meta tags
    for element in document.select(&og_meta_selector) {
        if let Some(property) = element.value().attr("property") {
            let content = element
                .value()
                .attr("content")
                .map(|s| s.trim().to_string());

            match property {
                "og:title" => {
                    if let Some(title) = content {
                        if !title.is_empty() {
                            metadata.title = Some(title);
                        }
                    }
                }
                "og:description" => {
                    if let Some(desc) = content {
                        if !desc.is_empty() {
                            metadata.description = Some(desc);
                        }
                    }
                }
                "og:image" => {
                    if let Some(img) = content {
                        if !img.is_empty() {
                            metadata.image = Some(img);
                        }
                    }
                }
                "og:type" => {
                    if let Some(og_type) = content {
                        if !og_type.is_empty() {
                            metadata.og_type = Some(og_type);
                        }
                    }
                }
                _ => {
                    // Ignore other OG properties for now
                }
            }
        }
    }

    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_basic_og_metadata() {
        let html = r#"
            <html>
                <head>
                    <meta property="og:title" content="Test Page">
                    <meta property="og:description" content="A test description">
                    <meta property="og:image" content="https://example.com/image.jpg">
                    <meta property="og:type" content="article">
                </head>
            </html>
        "#;

        let metadata = extract_og_metadata(html).unwrap();

        assert_eq!(metadata.title, Some("Test Page".to_string()));
        assert_eq!(metadata.description, Some("A test description".to_string()));
        assert_eq!(
            metadata.image,
            Some("https://example.com/image.jpg".to_string())
        );
        assert_eq!(metadata.og_type, Some("article".to_string()));
        assert!(!metadata.is_empty());
        assert!(metadata.has_content());
    }

    #[test]
    fn test_extract_partial_og_metadata() {
        let html = r#"
            <html>
                <head>
                    <meta property="og:title" content="Only Title">
                </head>
            </html>
        "#;

        let metadata = extract_og_metadata(html).unwrap();

        assert_eq!(metadata.title, Some("Only Title".to_string()));
        assert_eq!(metadata.description, None);
        assert_eq!(metadata.image, None);
        assert_eq!(metadata.og_type, None);
        assert!(!metadata.is_empty());
        assert!(metadata.has_content());
    }

    #[test]
    fn test_extract_no_og_metadata() {
        let html = r#"
            <html>
                <head>
                    <title>Regular Title</title>
                    <meta name="description" content="Regular meta description">
                </head>
            </html>
        "#;

        let metadata = extract_og_metadata(html).unwrap();

        assert_eq!(metadata.title, None);
        assert_eq!(metadata.description, None);
        assert_eq!(metadata.image, None);
        assert_eq!(metadata.og_type, None);
        assert!(metadata.is_empty());
        assert!(!metadata.has_content());
    }

    #[test]
    fn test_extract_empty_og_content() {
        let html = r#"
            <html>
                <head>
                    <meta property="og:title" content="">
                    <meta property="og:description" content="  ">
                </head>
            </html>
        "#;

        let metadata = extract_og_metadata(html).unwrap();

        // Empty strings should not be stored
        assert_eq!(metadata.title, None);
        assert_eq!(metadata.description, None);
        assert!(metadata.is_empty());
    }

    #[test]
    fn test_extract_with_other_meta_tags() {
        let html = r#"
            <html>
                <head>
                    <meta name="viewport" content="width=device-width">
                    <meta property="og:title" content="OG Title">
                    <meta name="description" content="Regular description">
                    <meta property="og:description" content="OG Description">
                    <meta property="twitter:card" content="summary">
                </head>
            </html>
        "#;

        let metadata = extract_og_metadata(html).unwrap();

        // Should only extract OG tags
        assert_eq!(metadata.title, Some("OG Title".to_string()));
        assert_eq!(metadata.description, Some("OG Description".to_string()));
        assert!(metadata.has_content());
    }

    #[test]
    fn test_extract_ignores_other_og_properties() {
        let html = r#"
            <html>
                <head>
                    <meta property="og:title" content="Title">
                    <meta property="og:url" content="https://example.com">
                    <meta property="og:site_name" content="Example Site">
                    <meta property="og:locale" content="en_US">
                </head>
            </html>
        "#;

        let metadata = extract_og_metadata(html).unwrap();

        // Should extract title but ignore other properties we don't track
        assert_eq!(metadata.title, Some("Title".to_string()));
        assert_eq!(metadata.description, None);
        assert_eq!(metadata.image, None);
        assert_eq!(metadata.og_type, None);
    }

    #[test]
    fn test_is_empty() {
        let empty = ExtractedOgMetadata::default();
        assert!(empty.is_empty());
        assert!(!empty.has_content());

        let with_title = ExtractedOgMetadata {
            title: Some("Title".to_string()),
            ..Default::default()
        };
        assert!(!with_title.is_empty());
        assert!(with_title.has_content());
    }

    #[test]
    fn test_has_content() {
        let only_image = ExtractedOgMetadata {
            image: Some("https://example.com/img.jpg".to_string()),
            ..Default::default()
        };
        // Image alone doesn't count as "has_content"
        assert!(!only_image.has_content());

        let with_desc = ExtractedOgMetadata {
            description: Some("Description".to_string()),
            ..Default::default()
        };
        assert!(with_desc.has_content());
    }

    #[test]
    fn test_extract_trims_whitespace() {
        let html = r#"
            <html>
                <head>
                    <meta property="og:title" content="  Trimmed Title  ">
                    <meta property="og:description" content="
                        Multi-line
                        description
                    ">
                </head>
            </html>
        "#;

        let metadata = extract_og_metadata(html).unwrap();

        assert_eq!(metadata.title, Some("Trimmed Title".to_string()));
        // Note: trim() only removes leading/trailing whitespace
        assert!(metadata.description.is_some());
    }
}
