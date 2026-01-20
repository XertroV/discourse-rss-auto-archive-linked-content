//! All Archives table page module.
//!
//! This module provides a table view of all archives optimized for browsing
//! large datasets with 1000 entries per page.

use maud::{html, Markup};

use crate::components::{BaseLayout, MediaTypeBadge, Pagination};
use crate::db::{ArchiveDisplay, User};

/// Parameters for the all archives table page.
#[derive(Debug)]
pub struct AllArchivesPageParams<'a> {
    pub archives: &'a [ArchiveDisplay],
    pub page: usize,
    pub total_pages: usize,
    pub content_type_filter: Option<&'a str>,
    pub source_filter: Option<&'a str>,
    pub user: Option<&'a User>,
}

/// Render the all archives table page.
#[must_use]
pub fn render_all_archives_table_page(params: &AllArchivesPageParams) -> Markup {
    let content = html! {
        div class="all-archives-container" {
            h1 { "All Archives" }

            p class="text-muted" {
                "Browse all archives in a table format. Use Ctrl+F to search within the current page."
            }

            // Pagination at top
            @if params.total_pages > 1 {
                (Pagination::new(params.page, params.total_pages, "/archives/all")
                    .with_content_type_filter(params.content_type_filter)
                    .with_source_filter(params.source_filter))
            }

            // Table
            @if params.archives.is_empty() {
                p class="text-muted" { "No archives found." }
            } @else {
                table class="archives-table" {
                    thead {
                        tr {
                            th { "ID" }
                            th { "Type" }
                            th { "Title" }
                            th { "URL" }
                        }
                    }
                    tbody {
                        @for archive in params.archives {
                            (render_archive_table_row(archive))
                        }
                    }
                }
            }

            // Pagination at bottom
            @if params.total_pages > 1 {
                (Pagination::new(params.page, params.total_pages, "/archives/all")
                    .with_content_type_filter(params.content_type_filter)
                    .with_source_filter(params.source_filter))
            }
        }
    };

    BaseLayout::new("All Archives")
        .with_user(params.user)
        .render(content)
}

/// Render a single archive table row.
fn render_archive_table_row(archive: &ArchiveDisplay) -> Markup {
    let title = archive
        .content_title
        .as_deref()
        .unwrap_or("Untitled")
        .to_string();
    let content_type = archive.content_type.as_deref().unwrap_or("text");

    html! {
        tr {
            td { (archive.id) }
            td { (MediaTypeBadge::from_content_type(content_type)) }
            td {
                a href=(format!("/archive/{}", archive.id)) {
                    (title)
                }
            }
            td class="url-cell" title=(archive.original_url) {
                (archive.original_url)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_archive_table_row() {
        let archive = ArchiveDisplay {
            id: 1,
            link_id: 1,
            status: "complete".to_string(),
            archived_at: Some("2024-01-01T00:00:00Z".to_string()),
            content_title: Some("Test Archive".to_string()),
            content_author: Some("Test Author".to_string()),
            content_type: Some("video".to_string()),
            is_nsfw: false,
            error_message: None,
            retry_count: 0,
            original_url: "https://example.com/video".to_string(),
            domain: "example.com".to_string(),
            total_size_bytes: Some(1024),
        };

        let html = render_archive_table_row(&archive).into_string();

        assert!(html.contains("<tr>"));
        assert!(html.contains("<td>1</td>")); // ID
        assert!(html.contains("href=\"/archive/1\"")); // Link
        assert!(html.contains("Test Archive")); // Title
        assert!(html.contains("https://example.com/video")); // URL
    }

    #[test]
    fn test_render_all_archives_table_page_empty() {
        let params = AllArchivesPageParams {
            archives: &[],
            page: 0,
            total_pages: 0,
            content_type_filter: None,
            source_filter: None,
            user: None,
        };

        let html = render_all_archives_table_page(&params).into_string();

        assert!(html.contains("No archives found"));
    }

    #[test]
    fn test_render_all_archives_table_page_with_data() {
        let archive = ArchiveDisplay {
            id: 1,
            link_id: 1,
            status: "complete".to_string(),
            archived_at: Some("2024-01-01T00:00:00Z".to_string()),
            content_title: Some("Test".to_string()),
            content_author: None,
            content_type: Some("image".to_string()),
            is_nsfw: false,
            error_message: None,
            retry_count: 0,
            original_url: "https://example.com".to_string(),
            domain: "example.com".to_string(),
            total_size_bytes: None,
        };

        let params = AllArchivesPageParams {
            archives: &[archive],
            page: 0,
            total_pages: 1,
            content_type_filter: None,
            source_filter: None,
            user: None,
        };

        let html = render_all_archives_table_page(&params).into_string();

        assert!(html.contains("<table class=\"archives-table\">"));
        assert!(html.contains("<th>ID</th>"));
        assert!(html.contains("<th>Type</th>"));
        assert!(html.contains("<th>Title</th>"));
        assert!(html.contains("<th>URL</th>"));
    }
}
