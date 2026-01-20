//! Archive comparison page with diff view.
//!
//! This module renders a side-by-side comparison of two archives,
//! showing the text content differences with additions and deletions.

use maud::{html, Markup, Render};

use crate::components::BaseLayout;
use crate::db::{Archive, Link};
use crate::web::diff::{ChangeType, DiffLine, DiffResult};

/// Truncate a URL for display, adding ellipsis if too long.
fn truncate_url(url: &str, max_len: usize) -> String {
    if url.len() <= max_len {
        url.to_string()
    } else {
        format!("{}...", &url[..max_len])
    }
}

/// Statistics summary for a diff.
#[derive(Debug, Clone)]
pub struct DiffStats {
    pub additions: usize,
    pub deletions: usize,
}

impl DiffStats {
    /// Create new diff stats.
    #[must_use]
    pub const fn new(additions: usize, deletions: usize) -> Self {
        Self {
            additions,
            deletions,
        }
    }
}

impl Render for DiffStats {
    fn render(&self) -> Markup {
        html! {
            div class="diff-stats" {
                span class="diff-stat-additions" {
                    "+" (self.additions) " additions"
                }
                span class="diff-stat-deletions" {
                    "-" (self.deletions) " deletions"
                }
            }
        }
    }
}

/// A single line in the diff view.
#[derive(Debug, Clone)]
pub struct DiffLineView<'a> {
    line: &'a DiffLine,
}

impl<'a> DiffLineView<'a> {
    /// Create a new diff line view.
    #[must_use]
    pub const fn new(line: &'a DiffLine) -> Self {
        Self { line }
    }

    /// Get the appropriate line number to display.
    fn line_number(&self) -> String {
        match self.line.change_type {
            ChangeType::Added => self
                .line
                .new_line_num
                .map_or(String::new(), |n| n.to_string()),
            ChangeType::Removed => self
                .line
                .old_line_num
                .map_or(String::new(), |n| n.to_string()),
            ChangeType::Unchanged => self
                .line
                .old_line_num
                .map_or(String::new(), |n| n.to_string()),
        }
    }
}

impl Render for DiffLineView<'_> {
    fn render(&self) -> Markup {
        let css_class = self.line.change_type.css_class();
        let symbol = self.line.change_type.symbol();
        let line_num = self.line_number();

        html! {
            div class={"diff-line " (css_class)} {
                span class="diff-line-num" { (line_num) }
                span class="diff-symbol" { (symbol) }
                span class="diff-line-content" { (self.line.content) }
            }
        }
    }
}

/// The main diff container showing all lines.
#[derive(Debug, Clone)]
pub struct DiffView<'a> {
    diff_result: &'a DiffResult,
}

impl<'a> DiffView<'a> {
    /// Create a new diff view.
    #[must_use]
    pub const fn new(diff_result: &'a DiffResult) -> Self {
        Self { diff_result }
    }
}

impl Render for DiffView<'_> {
    fn render(&self) -> Markup {
        if self.diff_result.is_identical {
            html! {
                p {
                    em { "The content text of these archives is identical." }
                }
            }
        } else {
            html! {
                (DiffStats::new(self.diff_result.additions, self.diff_result.deletions))
                div class="diff-container" {
                    @for line in &self.diff_result.lines {
                        (DiffLineView::new(line))
                    }
                }
            }
        }
    }
}

/// Information about one archive in the comparison.
#[derive(Debug, Clone)]
pub struct ComparisonArchiveInfo<'a> {
    archive: &'a Archive,
    link: &'a Link,
}

impl<'a> ComparisonArchiveInfo<'a> {
    /// Create a new comparison archive info.
    #[must_use]
    pub const fn new(archive: &'a Archive, link: &'a Link) -> Self {
        Self { archive, link }
    }
}

impl Render for ComparisonArchiveInfo<'_> {
    fn render(&self) -> Markup {
        let title = self
            .archive
            .content_title
            .as_deref()
            .unwrap_or("Untitled Archive");
        let archived_at = self.archive.archived_at.as_deref().unwrap_or("pending");
        let url = &self.link.normalized_url;
        let display_url = truncate_url(url, 50);

        html! {
            div class="comparison-archive" {
                h3 {
                    a href=(format!("/archive/{}", self.archive.id)) { (title) }
                }
                p class="meta" {
                    strong { "URL:" }
                    " "
                    a href=(url) { (display_url) }
                    br;
                    strong { "Archived:" }
                    " " (archived_at)
                }
            }
        }
    }
}

/// The comparison header showing both archives' info side by side.
#[derive(Debug, Clone)]
pub struct ComparisonHeader<'a> {
    archive1: &'a Archive,
    link1: &'a Link,
    archive2: &'a Archive,
    link2: &'a Link,
}

impl<'a> ComparisonHeader<'a> {
    /// Create a new comparison header.
    #[must_use]
    pub const fn new(
        archive1: &'a Archive,
        link1: &'a Link,
        archive2: &'a Archive,
        link2: &'a Link,
    ) -> Self {
        Self {
            archive1,
            link1,
            archive2,
            link2,
        }
    }
}

impl Render for ComparisonHeader<'_> {
    fn render(&self) -> Markup {
        html! {
            div class="comparison-header" {
                (ComparisonArchiveInfo::new(self.archive1, self.link1))
                (ComparisonArchiveInfo::new(self.archive2, self.link2))
            }
        }
    }
}

/// Render the complete archive comparison page.
///
/// # Arguments
///
/// * `archive1` - The first archive (older)
/// * `link1` - The link for the first archive
/// * `archive2` - The second archive (newer)
/// * `link2` - The link for the second archive
/// * `diff_result` - The computed diff between the archives
///
/// # Returns
///
/// Complete HTML page as maud Markup.
#[must_use]
pub fn render_comparison_page(
    archive1: &Archive,
    link1: &Link,
    archive2: &Archive,
    link2: &Link,
    diff_result: &DiffResult,
) -> Markup {
    let content = html! {
        h1 { "Archive Comparison" }

        // Comparison header with both archives' info
        (ComparisonHeader::new(archive1, link1, archive2, link2))

        // Diff section
        section {
            h2 { "Content Diff" }
            (DiffView::new(diff_result))

            // Handle case where both archives have no content text
            @if archive1.content_text.is_none() && archive2.content_text.is_none() {
                p {
                    em { "Neither archive has text content to compare. They may contain only media." }
                }
            }
        }
    };

    BaseLayout::new("Archive Comparison").render(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_archive(id: i64, title: &str) -> Archive {
        Archive {
            id,
            link_id: 1,
            status: "complete".to_string(),
            archived_at: Some("2024-01-15 12:00:00".to_string()),
            content_title: Some(title.to_string()),
            content_author: Some("testuser".to_string()),
            content_text: Some("Sample content text".to_string()),
            content_type: Some("article".to_string()),
            s3_key_primary: None,
            s3_key_thumb: None,
            s3_keys_extra: None,
            wayback_url: None,
            archive_today_url: None,
            ipfs_cid: None,
            error_message: None,
            retry_count: 0,
            created_at: "2024-01-15 12:00:00".to_string(),
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

    fn sample_link() -> Link {
        Link {
            id: 1,
            original_url: "https://example.com/article/123".to_string(),
            normalized_url: "https://example.com/article/123".to_string(),
            canonical_url: None,
            final_url: None,
            domain: "example.com".to_string(),
            first_seen_at: "2024-01-01 00:00:00".to_string(),
            last_archived_at: Some("2024-01-15 12:00:00".to_string()),
        }
    }

    #[test]
    fn test_truncate_url_short() {
        let url = "https://example.com";
        assert_eq!(truncate_url(url, 50), url);
    }

    #[test]
    fn test_truncate_url_long() {
        let url = "https://example.com/very/long/path/to/some/resource/that/exceeds/the/limit";
        let truncated = truncate_url(url, 30);
        assert!(truncated.ends_with("..."));
        assert_eq!(truncated.len(), 33); // 30 chars + "..."
    }

    #[test]
    fn test_diff_stats_render() {
        let stats = DiffStats::new(5, 3);
        let html = stats.render().into_string();

        assert!(html.contains("diff-stats"));
        assert!(html.contains("diff-stat-additions"));
        assert!(html.contains("diff-stat-deletions"));
        assert!(html.contains("+5 additions"));
        assert!(html.contains("-3 deletions"));
    }

    #[test]
    fn test_diff_line_view_added() {
        let line = DiffLine {
            content: "new line".to_string(),
            change_type: ChangeType::Added,
            old_line_num: None,
            new_line_num: Some(5),
        };
        let view = DiffLineView::new(&line);
        let html = view.render().into_string();

        assert!(html.contains("diff-added"));
        assert!(html.contains("diff-symbol"));
        assert!(html.contains("+"));
        assert!(html.contains("new line"));
        assert!(html.contains(">5<")); // line number
    }

    #[test]
    fn test_diff_line_view_removed() {
        let line = DiffLine {
            content: "old line".to_string(),
            change_type: ChangeType::Removed,
            old_line_num: Some(3),
            new_line_num: None,
        };
        let view = DiffLineView::new(&line);
        let html = view.render().into_string();

        assert!(html.contains("diff-removed"));
        assert!(html.contains("-"));
        assert!(html.contains("old line"));
        assert!(html.contains(">3<")); // line number
    }

    #[test]
    fn test_diff_line_view_unchanged() {
        let line = DiffLine {
            content: "same line".to_string(),
            change_type: ChangeType::Unchanged,
            old_line_num: Some(2),
            new_line_num: Some(2),
        };
        let view = DiffLineView::new(&line);
        let html = view.render().into_string();

        assert!(html.contains("diff-unchanged"));
        assert!(html.contains("same line"));
    }

    #[test]
    fn test_diff_view_identical() {
        let diff_result = DiffResult {
            lines: vec![],
            additions: 0,
            deletions: 0,
            is_identical: true,
        };
        let view = DiffView::new(&diff_result);
        let html = view.render().into_string();

        assert!(html.contains("identical"));
        assert!(!html.contains("diff-container"));
    }

    #[test]
    fn test_diff_view_with_changes() {
        let diff_result = DiffResult {
            lines: vec![
                DiffLine {
                    content: "old line".to_string(),
                    change_type: ChangeType::Removed,
                    old_line_num: Some(1),
                    new_line_num: None,
                },
                DiffLine {
                    content: "new line".to_string(),
                    change_type: ChangeType::Added,
                    old_line_num: None,
                    new_line_num: Some(1),
                },
            ],
            additions: 1,
            deletions: 1,
            is_identical: false,
        };
        let view = DiffView::new(&diff_result);
        let html = view.render().into_string();

        assert!(html.contains("diff-container"));
        assert!(html.contains("diff-stats"));
        assert!(html.contains("diff-added"));
        assert!(html.contains("diff-removed"));
    }

    #[test]
    fn test_comparison_archive_info() {
        let archive = sample_archive(1, "Test Article");
        let link = sample_link();
        let info = ComparisonArchiveInfo::new(&archive, &link);
        let html = info.render().into_string();

        assert!(html.contains("comparison-archive"));
        assert!(html.contains("Test Article"));
        assert!(html.contains("/archive/1"));
        assert!(html.contains("example.com"));
        assert!(html.contains("2024-01-15 12:00:00"));
    }

    #[test]
    fn test_comparison_header() {
        let archive1 = sample_archive(1, "Article v1");
        let archive2 = sample_archive(2, "Article v2");
        let link = sample_link();
        let header = ComparisonHeader::new(&archive1, &link, &archive2, &link);
        let html = header.render().into_string();

        assert!(html.contains("comparison-header"));
        assert!(html.contains("Article v1"));
        assert!(html.contains("Article v2"));
        assert!(html.contains("/archive/1"));
        assert!(html.contains("/archive/2"));
    }

    #[test]
    fn test_render_comparison_page() {
        let archive1 = sample_archive(1, "Article v1");
        let archive2 = sample_archive(2, "Article v2");
        let link = sample_link();
        let diff_result = DiffResult {
            lines: vec![DiffLine {
                content: "changed line".to_string(),
                change_type: ChangeType::Added,
                old_line_num: None,
                new_line_num: Some(1),
            }],
            additions: 1,
            deletions: 0,
            is_identical: false,
        };

        let page = render_comparison_page(&archive1, &link, &archive2, &link, &diff_result);
        let html = page.into_string();

        // Check page structure
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>Archive Comparison"));
        assert!(html.contains("<h1>Archive Comparison</h1>"));

        // Check comparison header
        assert!(html.contains("comparison-header"));
        assert!(html.contains("Article v1"));
        assert!(html.contains("Article v2"));

        // Check diff content
        assert!(html.contains("Content Diff"));
        assert!(html.contains("diff-container"));
        assert!(html.contains("+1 additions"));
    }

    #[test]
    fn test_render_comparison_page_identical() {
        let archive1 = sample_archive(1, "Same Article");
        let archive2 = sample_archive(2, "Same Article");
        let link = sample_link();
        let diff_result = DiffResult {
            lines: vec![],
            additions: 0,
            deletions: 0,
            is_identical: true,
        };

        let page = render_comparison_page(&archive1, &link, &archive2, &link, &diff_result);
        let html = page.into_string();

        assert!(html.contains("identical"));
    }

    #[test]
    fn test_render_comparison_page_no_content() {
        let mut archive1 = sample_archive(1, "Media Only");
        let mut archive2 = sample_archive(2, "Media Only");
        archive1.content_text = None;
        archive2.content_text = None;
        let link = sample_link();
        let diff_result = DiffResult {
            lines: vec![],
            additions: 0,
            deletions: 0,
            is_identical: true,
        };

        let page = render_comparison_page(&archive1, &link, &archive2, &link, &diff_result);
        let html = page.into_string();

        assert!(html.contains("Neither archive has text content"));
        assert!(html.contains("only media"));
    }

    #[test]
    fn test_comparison_archive_info_long_url() {
        let archive = sample_archive(1, "Test");
        let mut link = sample_link();
        link.normalized_url =
            "https://example.com/very/long/path/to/resource/that/is/too/long/to/display"
                .to_string();
        let info = ComparisonArchiveInfo::new(&archive, &link);
        let html = info.render().into_string();

        // The full URL should still be in the href
        assert!(html.contains("https://example.com/very/long/path"));
        // But display should be truncated
        assert!(html.contains("..."));
    }
}
