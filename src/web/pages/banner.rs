//! Archive banner rendering using maud templates.
//!
//! This module provides the archive banner that is injected into archived HTML pages.
//! The banner includes inline CSS to ensure it renders correctly when viewed offline
//! without access to our CSS files.

use maud::{html, PreEscaped};

use crate::db::{Archive, Link};

/// Inline CSS for the archive banner.
///
/// This CSS is embedded directly in the banner HTML to ensure proper rendering
/// when the archived page is viewed offline or without access to our stylesheets.
/// Includes both light and dark mode styles via `prefers-color-scheme`.
const BANNER_CSS: &str = r#"<style>
.archive-banner {
    border: 2px solid #4a90e2;
    border-radius: 4px;
    margin: 0 0 1rem 0;
    padding: 0;
    background-color: #f8f9fa;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
    font-size: 14px;
    line-height: 1.5;
    position: relative;
    z-index: 9999;
}
.archive-banner details { border: none; margin: 0; padding: 0; }
.archive-banner summary {
    padding: 10px 15px;
    cursor: pointer;
    list-style: none;
    display: flex;
    align-items: center;
    gap: 10px;
    user-select: none;
    background-color: #e8f0f8;
    border-bottom: 1px solid #4a90e2;
}
.archive-banner summary::-webkit-details-marker { display: none; }
.archive-banner summary::marker { display: none; }
.archive-banner .banner-icon { font-size: 16px; }
.archive-banner .banner-text { flex: 1; font-weight: 500; color: #333; }
.archive-banner .banner-toggle { font-size: 12px; color: #666; transition: transform 0.2s; }
.archive-banner details[open] .banner-toggle { transform: rotate(180deg); }
.archive-banner .banner-content { padding: 15px; background-color: #ffffff; }
.archive-banner .banner-row {
    margin-bottom: 10px;
    padding-bottom: 10px;
    border-bottom: 1px solid #e0e0e0;
    color: #333;
}
.archive-banner .banner-row:last-child { margin-bottom: 0; padding-bottom: 0; border-bottom: none; }
.archive-banner .banner-row strong { color: #333; margin-right: 8px; }
.archive-banner .banner-row a { color: #4a90e2; text-decoration: none; }
.archive-banner .banner-row a:hover { text-decoration: underline; }
.archive-banner .banner-links {
    margin-top: 15px;
    padding-top: 15px;
    border-top: 1px solid #e0e0e0;
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
}
.archive-banner .banner-link {
    display: inline-block;
    padding: 6px 12px;
    background-color: #4a90e2;
    color: white;
    border-radius: 3px;
    text-decoration: none;
    font-size: 13px;
    transition: background-color 0.2s;
}
.archive-banner .banner-link:hover { background-color: #357abd; text-decoration: none; }
@media (prefers-color-scheme: dark) {
    .archive-banner { background-color: #1a1a1a; border-color: #5a9ff0; }
    .archive-banner summary { background-color: #2a2a2a; border-bottom-color: #5a9ff0; }
    .archive-banner .banner-text { color: #e0e0e0; }
    .archive-banner .banner-toggle { color: #999; }
    .archive-banner .banner-content { background-color: #1a1a1a; }
    .archive-banner .banner-row { border-bottom-color: #333; color: #e0e0e0; }
    .archive-banner .banner-row strong { color: #e0e0e0; }
    .archive-banner .banner-links { border-top-color: #333; }
}
</style>"#;

/// Render archive banner for HTML files (archive.today style).
///
/// This banner is injected into archived HTML pages to show metadata about the archive.
/// It includes inline CSS to ensure it renders correctly when viewed offline without
/// access to our CSS files.
///
/// # Arguments
///
/// * `archive` - The archive record containing metadata
/// * `link` - The link record containing the original URL
///
/// # Returns
///
/// A `String` containing the complete banner HTML with inline styles.
/// Returns `String` (not `Markup`) because this will be injected into external HTML.
#[must_use]
pub fn render_archive_banner(archive: &Archive, link: &Link) -> String {
    let archived_at = archive.archived_at.as_deref().unwrap_or("pending");
    let archive_detail_url = format!("/archive/{}", archive.id);

    let markup = html! {
        // Inline CSS for offline viewing
        (PreEscaped(BANNER_CSS))

        div id="archive-banner" class="archive-banner" {
            details {
                summary {
                    span.banner-icon { "\u{1F4E6}" } // Package emoji
                    span.banner-text { "This is an archived page" }
                    span.banner-toggle { "\u{25BC}" } // Down arrow
                }
                div.banner-content {
                    // Archived timestamp
                    div.banner-row {
                        strong { "Archived:" }
                        (archived_at)
                    }

                    // Original URL
                    div.banner-row {
                        strong { "Original URL:" }
                        a href=(link.normalized_url) target="_blank" rel="noopener" {
                            (link.normalized_url)
                        }
                    }

                    // Domain
                    div.banner-row {
                        strong { "Domain:" }
                        (link.domain)
                    }

                    // Archive ID with link
                    div.banner-row {
                        strong { "Archive ID:" }
                        a href=(archive_detail_url) {
                            "#" (archive.id)
                        }
                    }

                    // Optional: Title
                    @if let Some(ref title) = archive.content_title {
                        div.banner-row {
                            strong { "Title:" }
                            (title)
                        }
                    }

                    // Optional: Author
                    @if let Some(ref author) = archive.content_author {
                        div.banner-row {
                            strong { "Author:" }
                            (author)
                        }
                    }

                    // External archive links
                    div.banner-links {
                        @if let Some(ref wayback) = archive.wayback_url {
                            a.banner-link href=(wayback) target="_blank" rel="noopener" {
                                "Wayback Machine"
                            }
                        }
                        @if let Some(ref archive_today) = archive.archive_today_url {
                            a.banner-link href=(archive_today) target="_blank" rel="noopener" {
                                "Archive.today"
                            }
                        }
                    }
                }
            }
        }
    };

    markup.into_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_archive() -> Archive {
        Archive {
            id: 42,
            link_id: 1,
            status: "complete".to_string(),
            archived_at: Some("2024-01-15T10:30:00Z".to_string()),
            content_title: Some("Test Article Title".to_string()),
            content_author: Some("Test Author".to_string()),
            content_text: None,
            content_type: Some("text".to_string()),
            s3_key_primary: Some("archives/42/view.html".to_string()),
            s3_key_thumb: None,
            s3_keys_extra: None,
            wayback_url: Some(
                "https://web.archive.org/web/20240115/example.com/article".to_string(),
            ),
            archive_today_url: Some("https://archive.today/abc123".to_string()),
            ipfs_cid: None,
            error_message: None,
            retry_count: 0,
            created_at: "2024-01-15T10:00:00Z".to_string(),
            is_nsfw: false,
            nsfw_source: None,
            next_retry_at: None,
            last_attempt_at: None,
            http_status_code: Some(200),
            post_date: None,
            quoted_archive_id: None,
            reply_to_archive_id: None,
            submitted_by_user_id: None,
        }
    }

    fn create_test_link() -> Link {
        Link {
            id: 1,
            original_url: "https://example.com/article?utm_source=test".to_string(),
            normalized_url: "https://example.com/article".to_string(),
            canonical_url: None,
            final_url: None,
            domain: "example.com".to_string(),
            first_seen_at: "2024-01-15T09:00:00Z".to_string(),
            last_archived_at: Some("2024-01-15T10:30:00Z".to_string()),
        }
    }

    #[test]
    fn test_render_archive_banner_includes_css() {
        let archive = create_test_archive();
        let link = create_test_link();

        let html = render_archive_banner(&archive, &link);

        // Should include inline CSS
        assert!(html.contains("<style>"));
        assert!(html.contains(".archive-banner"));
        assert!(html.contains("prefers-color-scheme: dark"));
    }

    #[test]
    fn test_render_archive_banner_structure() {
        let archive = create_test_archive();
        let link = create_test_link();

        let html = render_archive_banner(&archive, &link);

        // Should have main container
        assert!(html.contains(r#"id="archive-banner""#));
        assert!(html.contains(r#"class="archive-banner""#));

        // Should have collapsible details
        assert!(html.contains("<details>"));
        assert!(html.contains("<summary>"));
        assert!(html.contains("This is an archived page"));

        // Should have banner content
        assert!(html.contains(r#"class="banner-content""#));
    }

    #[test]
    fn test_render_archive_banner_displays_metadata() {
        let archive = create_test_archive();
        let link = create_test_link();

        let html = render_archive_banner(&archive, &link);

        // Should show archived timestamp
        assert!(html.contains("Archived:"));
        assert!(html.contains("2024-01-15T10:30:00Z"));

        // Should show original URL
        assert!(html.contains("Original URL:"));
        assert!(html.contains("https://example.com/article"));

        // Should show domain
        assert!(html.contains("Domain:"));
        assert!(html.contains("example.com"));

        // Should show archive ID
        assert!(html.contains("Archive ID:"));
        assert!(html.contains("#42"));
        assert!(html.contains(r#"href="/archive/42""#));

        // Should show title
        assert!(html.contains("Title:"));
        assert!(html.contains("Test Article Title"));

        // Should show author
        assert!(html.contains("Author:"));
        assert!(html.contains("Test Author"));
    }

    #[test]
    fn test_render_archive_banner_external_links() {
        let archive = create_test_archive();
        let link = create_test_link();

        let html = render_archive_banner(&archive, &link);

        // Should include Wayback Machine link
        assert!(html.contains("Wayback Machine"));
        assert!(html.contains("https://web.archive.org/web/20240115/example.com/article"));

        // Should include Archive.today link
        assert!(html.contains("Archive.today"));
        assert!(html.contains("https://archive.today/abc123"));
    }

    #[test]
    fn test_render_archive_banner_without_optional_fields() {
        let mut archive = create_test_archive();
        archive.content_title = None;
        archive.content_author = None;
        archive.wayback_url = None;
        archive.archive_today_url = None;
        archive.archived_at = None;

        let link = create_test_link();

        let html = render_archive_banner(&archive, &link);

        // Should show "pending" for missing timestamp
        assert!(html.contains("pending"));

        // Should NOT show title/author rows
        assert!(!html.contains("Title:"));
        assert!(!html.contains("Author:"));

        // Should NOT have external archive links (but links container still exists)
        assert!(!html.contains("Wayback Machine"));
        assert!(!html.contains("Archive.today"));

        // Core elements should still be present
        assert!(html.contains("Original URL:"));
        assert!(html.contains("example.com"));
    }

    #[test]
    fn test_render_archive_banner_escapes_html() {
        let mut archive = create_test_archive();
        archive.content_title = Some("<script>alert('xss')</script>".to_string());
        archive.content_author = Some("Author <with> special & chars".to_string());

        let mut link = create_test_link();
        link.normalized_url = "https://example.com/path?q=<test>&x=1".to_string();
        link.domain = "example.com".to_string();

        let html = render_archive_banner(&archive, &link);

        // Script tags should be escaped
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));

        // Special characters should be escaped
        assert!(html.contains("&lt;with&gt;"));
        assert!(html.contains("&amp;"));
    }

    #[test]
    fn test_render_archive_banner_returns_string() {
        let archive = create_test_archive();
        let link = create_test_link();

        let result = render_archive_banner(&archive, &link);

        // Should return a String, not Markup
        assert!(result.starts_with("<style>"));
        assert!(result.ends_with("</div>"));
    }
}
