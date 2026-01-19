use std::path::Path;

use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use scraper::{Html, Selector};

use super::traits::{ArchiveResult, SiteHandler};
use crate::constants::ARCHIVAL_USER_AGENT;

static PATTERNS: std::sync::LazyLock<Vec<Regex>> = std::sync::LazyLock::new(|| {
    vec![
        // Match any HTTP(S) URL as fallback
        Regex::new(r"^https?://").unwrap(),
    ]
});

pub struct GenericHandler;

impl GenericHandler {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for GenericHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SiteHandler for GenericHandler {
    fn site_id(&self) -> &'static str {
        "generic"
    }

    fn url_patterns(&self) -> &[Regex] {
        &PATTERNS
    }

    fn priority(&self) -> i32 {
        -100 // Lowest priority, fallback handler
    }

    async fn archive(
        &self,
        url: &str,
        work_dir: &Path,
        _cookies_file: Option<&Path>,
    ) -> Result<ArchiveResult> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to build HTTP client")?;

        let response = client
            .get(url)
            .header("User-Agent", ARCHIVAL_USER_AGENT)
            .send()
            .await
            .context("Failed to fetch URL")?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP request failed with status {}", response.status());
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html");

        // Only process HTML content
        if !content_type.contains("text/html") {
            return Ok(ArchiveResult {
                content_type: "file".to_string(),
                ..Default::default()
            });
        }

        let body = response
            .text()
            .await
            .context("Failed to read response body")?;

        // Save raw HTML
        let html_path = work_dir.join("raw.html");
        tokio::fs::write(&html_path, &body)
            .await
            .context("Failed to write HTML file")?;

        // Extract metadata
        let (title, text) = extract_metadata(&body);

        Ok(ArchiveResult {
            title,
            content_type: "text".to_string(),
            text,
            primary_file: Some("raw.html".to_string()),
            ..Default::default()
        })
    }
}

/// Extract title and readable text from HTML.
fn extract_metadata(html: &str) -> (Option<String>, Option<String>) {
    let document = Html::parse_document(html);

    // Extract title
    let title = extract_title(&document);

    // Extract main text content
    let text = extract_text(&document);

    (title, text)
}

fn extract_title(document: &Html) -> Option<String> {
    // Try og:title first
    if let Ok(selector) = Selector::parse("meta[property='og:title']") {
        if let Some(element) = document.select(&selector).next() {
            if let Some(content) = element.value().attr("content") {
                if !content.is_empty() {
                    return Some(content.to_string());
                }
            }
        }
    }

    // Try twitter:title
    if let Ok(selector) = Selector::parse("meta[name='twitter:title']") {
        if let Some(element) = document.select(&selector).next() {
            if let Some(content) = element.value().attr("content") {
                if !content.is_empty() {
                    return Some(content.to_string());
                }
            }
        }
    }

    // Try title tag
    if let Ok(selector) = Selector::parse("title") {
        if let Some(element) = document.select(&selector).next() {
            let text: String = element.text().collect();
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

fn extract_text(document: &Html) -> Option<String> {
    // Try to get og:description first
    if let Ok(selector) = Selector::parse("meta[property='og:description']") {
        if let Some(element) = document.select(&selector).next() {
            if let Some(content) = element.value().attr("content") {
                if !content.is_empty() {
                    return Some(content.to_string());
                }
            }
        }
    }

    // Try meta description
    if let Ok(selector) = Selector::parse("meta[name='description']") {
        if let Some(element) = document.select(&selector).next() {
            if let Some(content) = element.value().attr("content") {
                if !content.is_empty() {
                    return Some(content.to_string());
                }
            }
        }
    }

    // Try article or main content
    for tag in ["article", "main", "body"] {
        if let Ok(selector) = Selector::parse(tag) {
            if let Some(element) = document.select(&selector).next() {
                let text: String = element.text().collect::<Vec<_>>().join(" ");
                let cleaned = clean_text(&text);
                if !cleaned.is_empty() {
                    // Truncate to reasonable length
                    let truncated = if cleaned.len() > 5000 {
                        format!("{}...", &cleaned[..5000])
                    } else {
                        cleaned
                    };
                    return Some(truncated);
                }
            }
        }
    }

    None
}

fn clean_text(text: &str) -> String {
    // Remove excessive whitespace
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title() {
        let html = r#"
            <html>
            <head>
                <title>Test Page</title>
                <meta property="og:title" content="OG Title">
            </head>
            <body></body>
            </html>
        "#;

        let doc = Html::parse_document(html);
        let title = extract_title(&doc);
        assert_eq!(title, Some("OG Title".to_string()));
    }

    #[test]
    fn test_extract_title_fallback() {
        let html = r#"
            <html>
            <head>
                <title>Test Page</title>
            </head>
            <body></body>
            </html>
        "#;

        let doc = Html::parse_document(html);
        let title = extract_title(&doc);
        assert_eq!(title, Some("Test Page".to_string()));
    }

    #[test]
    fn test_can_handle() {
        let handler = GenericHandler::new();

        assert!(handler.can_handle("https://example.com/page"));
        assert!(handler.can_handle("http://example.com/page"));

        assert!(!handler.can_handle("ftp://example.com/"));
        assert!(!handler.can_handle("not a url"));
    }
}
