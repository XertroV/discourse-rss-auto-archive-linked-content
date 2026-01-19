use scraper::{ElementRef, Html, Selector};

/// A link extracted from HTML content.
#[derive(Debug, Clone)]
pub struct ExtractedLink {
    /// The href URL.
    pub url: String,
    /// Whether this link is inside a quote block.
    pub in_quote: bool,
    /// Context snippet around the link.
    pub context: Option<String>,
}

/// Extract all links from HTML content.
#[must_use]
pub fn extract_links(html: &str) -> Vec<ExtractedLink> {
    let document = Html::parse_fragment(html);
    let link_selector = Selector::parse("a[href]").expect("Invalid selector");

    let mut links = Vec::new();
    let mut seen_urls = std::collections::HashSet::new();

    for element in document.select(&link_selector) {
        if let Some(href) = element.value().attr("href") {
            // Skip empty hrefs, anchors, and javascript
            if href.is_empty()
                || href.starts_with('#')
                || href.starts_with("javascript:")
                || href.starts_with("mailto:")
            {
                continue;
            }

            // Skip if we've already seen this URL in this document
            if seen_urls.contains(href) {
                continue;
            }
            seen_urls.insert(href.to_string());

            let in_quote = is_in_quote(&element);
            let context = extract_context(&element);

            links.push(ExtractedLink {
                url: href.to_string(),
                in_quote,
                context,
            });
        }
    }

    links
}

/// Check if an element is inside a quote block.
fn is_in_quote(element: &ElementRef) -> bool {
    let mut current = element.parent();

    while let Some(parent_node) = current {
        if let Some(parent_element) = parent_node.value().as_element() {
            let tag = parent_element.name();
            let classes = parent_element.attr("class").unwrap_or("");

            // Check for blockquote tag
            if tag == "blockquote" {
                return true;
            }

            // Check for Discourse quote aside
            if tag == "aside" && classes.contains("quote") {
                return true;
            }

            // Check for generic quote divs
            if tag == "div" && classes.contains("quote") {
                return true;
            }
        }
        current = parent_node.parent();
    }

    false
}

/// Extract context around a link element.
fn extract_context(element: &ElementRef) -> Option<String> {
    // Get the link text
    let link_text: String = element.text().collect();

    // Try to get parent paragraph text
    let mut current = element.parent();
    while let Some(parent_node) = current {
        if let Some(parent_element) = parent_node.value().as_element() {
            let tag = parent_element.name();
            if tag == "p" || tag == "li" || tag == "div" {
                if let Some(parent_ref) = ElementRef::wrap(parent_node) {
                    let full_text: String = parent_ref.text().collect::<Vec<_>>().join(" ");
                    let trimmed = full_text.trim();
                    if !trimmed.is_empty() && trimmed.len() <= 500 {
                        return Some(trimmed.to_string());
                    } else if trimmed.len() > 500 {
                        // Truncate to ~500 chars around the link
                        return Some(truncate_around(&full_text, &link_text, 250));
                    }
                }
                break;
            }
        }
        current = parent_node.parent();
    }

    // Fall back to just the link text
    if link_text.is_empty() {
        None
    } else {
        Some(link_text)
    }
}

/// Truncate text around a target substring.
fn truncate_around(text: &str, target: &str, context_chars: usize) -> String {
    if let Some(pos) = text.find(target) {
        let start = pos.saturating_sub(context_chars);
        let end = (pos + target.len() + context_chars).min(text.len());

        let mut result = String::new();
        if start > 0 {
            result.push_str("...");
        }
        result.push_str(&text[start..end]);
        if end < text.len() {
            result.push_str("...");
        }
        result
    } else {
        // Target not found, just take the start
        let end = context_chars.min(text.len());
        let mut result = text[..end].to_string();
        if end < text.len() {
            result.push_str("...");
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_links() {
        let html = r#"
            <p>Check out <a href="https://example.com">this link</a>.</p>
        "#;

        let links = extract_links(html);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://example.com");
        assert!(!links[0].in_quote);
    }

    #[test]
    fn test_extract_multiple_links() {
        let html = r#"
            <p>
                <a href="https://example.com/1">One</a>
                <a href="https://example.com/2">Two</a>
            </p>
        "#;

        let links = extract_links(html);
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn test_deduplicate_links() {
        let html = r#"
            <p>
                <a href="https://example.com">First</a>
                <a href="https://example.com">Second</a>
            </p>
        "#;

        let links = extract_links(html);
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn test_quote_detection_blockquote() {
        let html = r#"
            <blockquote>
                <p><a href="https://quoted.com">Quoted link</a></p>
            </blockquote>
            <p><a href="https://normal.com">Normal link</a></p>
        "#;

        let links = extract_links(html);
        assert_eq!(links.len(), 2);

        let quoted = links
            .iter()
            .find(|l| l.url == "https://quoted.com")
            .unwrap();
        assert!(quoted.in_quote);

        let normal = links
            .iter()
            .find(|l| l.url == "https://normal.com")
            .unwrap();
        assert!(!normal.in_quote);
    }

    #[test]
    fn test_quote_detection_aside() {
        let html = r#"
            <aside class="quote">
                <p><a href="https://quoted.com">Quoted link</a></p>
            </aside>
        "#;

        let links = extract_links(html);
        assert_eq!(links.len(), 1);
        assert!(links[0].in_quote);
    }

    #[test]
    fn test_skip_anchors_and_javascript() {
        let html = r##"
            <a href="#section">Anchor</a>
            <a href="javascript:void(0)">JS</a>
            <a href="mailto:test@example.com">Email</a>
            <a href="https://valid.com">Valid</a>
        "##;

        let links = extract_links(html);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://valid.com");
    }

    #[test]
    fn test_context_extraction() {
        let html = r#"
            <p>Here is some text before <a href="https://example.com">the link</a> and after.</p>
        "#;

        let links = extract_links(html);
        assert_eq!(links.len(), 1);
        assert!(links[0].context.is_some());
        let context = links[0].context.as_ref().unwrap();
        assert!(context.contains("the link"));
    }

    #[test]
    fn test_truncate_around() {
        let text = "This is a long text with a target word somewhere in the middle of it.";
        let result = truncate_around(text, "target", 10);
        assert!(result.contains("target"));
        assert!(result.starts_with("...") || result.len() < text.len());
    }
}
