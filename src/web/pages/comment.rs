//! Comment-related pages using maud templates.
//!
//! This module provides maud-based templates for comment-related functionality,
//! including the edit history modal/page.

use maud::{html, Markup, Render};

use crate::db::CommentEdit;

/// Render comment edit history as a modal/AJAX response.
///
/// This function renders the edit history for a comment in a format suitable
/// for display in a modal or as an AJAX response. It shows each previous
/// version of the comment with timestamps.
///
/// # Arguments
///
/// * `edits` - Slice of comment edits, ordered by `edited_at` ascending
///
/// # Returns
///
/// The rendered HTML markup for the edit history.
///
/// # Example
///
/// ```ignore
/// use crate::web::pages::comment::render_comment_edit_history_page;
/// use crate::db::CommentEdit;
///
/// let edits = vec![
///     CommentEdit {
///         id: 1,
///         comment_id: 100,
///         previous_content: "Original text".to_string(),
///         edited_by_user_id: 1,
///         edited_at: "2024-01-15 12:00:00".to_string(),
///     },
/// ];
/// let html = render_comment_edit_history_page(&edits);
/// ```
#[must_use]
pub fn render_comment_edit_history_page(edits: &[CommentEdit]) -> Markup {
    if edits.is_empty() {
        return html! {
            div style="padding: 1rem; text-align: center; color: var(--text-secondary, #52525b);" {
                "No edit history available"
            }
        };
    }

    html! {
        div style="max-width: 600px;" {
            h3 { "Edit History" }
            div style="max-height: 400px; overflow-y: auto;" {
                @for (i, edit) in edits.iter().enumerate() {
                    (EditHistoryEntry::new(edit, edits.len() - i))
                }
            }
        }
    }
}

/// A single edit history entry component.
struct EditHistoryEntry<'a> {
    edit: &'a CommentEdit,
    version: usize,
}

impl<'a> EditHistoryEntry<'a> {
    fn new(edit: &'a CommentEdit, version: usize) -> Self {
        Self { edit, version }
    }
}

impl Render for EditHistoryEntry<'_> {
    fn render(&self) -> Markup {
        html! {
            div style="padding: 0.75rem; margin-bottom: 0.75rem; background: var(--bg-secondary, #fafafa); border-radius: var(--radius, 0.375rem); border-left: 3px solid var(--primary, #ec4899);" {
                p style="margin: 0 0 0.5rem 0; font-size: 0.875rem; color: var(--text-secondary, #52525b);" {
                    strong { "Version " (self.version) }
                    " — Edited on "
                    (self.edit.edited_at)
                }
                p style="margin: 0; padding: 0.5rem; background: var(--bg-primary, white); border-radius: 4px; font-size: 0.875rem; word-break: break-word;" {
                    (self.edit.previous_content)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_edit(id: i64, content: &str, edited_at: &str) -> CommentEdit {
        CommentEdit {
            id,
            comment_id: 100,
            previous_content: content.to_string(),
            edited_by_user_id: 1,
            edited_at: edited_at.to_string(),
        }
    }

    #[test]
    fn test_render_empty_edit_history() {
        let edits: Vec<CommentEdit> = vec![];
        let html = render_comment_edit_history_page(&edits).into_string();

        assert!(html.contains("No edit history available"));
        assert!(html.contains("text-align: center"));
    }

    #[test]
    fn test_render_single_edit() {
        let edits = vec![sample_edit(1, "Original content", "2024-01-15 12:00:00")];
        let html = render_comment_edit_history_page(&edits).into_string();

        // Check structure
        assert!(html.contains("Edit History"));
        assert!(html.contains("Version 1"));
        assert!(html.contains("Edited on"));
        assert!(html.contains("2024-01-15 12:00:00"));
        assert!(html.contains("Original content"));
    }

    #[test]
    fn test_render_multiple_edits() {
        let edits = vec![
            sample_edit(1, "First version", "2024-01-15 10:00:00"),
            sample_edit(2, "Second version", "2024-01-15 11:00:00"),
            sample_edit(3, "Third version", "2024-01-15 12:00:00"),
        ];
        let html = render_comment_edit_history_page(&edits).into_string();

        // Check all versions are present (most recent first in numbering)
        assert!(html.contains("Version 3"));
        assert!(html.contains("Version 2"));
        assert!(html.contains("Version 1"));

        // Check content
        assert!(html.contains("First version"));
        assert!(html.contains("Second version"));
        assert!(html.contains("Third version"));

        // Check timestamps
        assert!(html.contains("2024-01-15 10:00:00"));
        assert!(html.contains("2024-01-15 11:00:00"));
        assert!(html.contains("2024-01-15 12:00:00"));
    }

    #[test]
    fn test_version_numbering_order() {
        // Edits are ordered by edited_at ascending (oldest first)
        // Version numbers should count down from total (newest = highest version)
        let edits = vec![
            sample_edit(1, "First", "2024-01-15 10:00:00"),
            sample_edit(2, "Second", "2024-01-15 11:00:00"),
        ];
        let html = render_comment_edit_history_page(&edits).into_string();

        // First edit (oldest) should be Version 2, second should be Version 1
        // This matches the original implementation: edits.len() - i
        let v2_pos = html.find("Version 2").expect("Version 2 should exist");
        let v1_pos = html.find("Version 1").expect("Version 1 should exist");

        // Version 2 should appear before Version 1 in the HTML
        assert!(v2_pos < v1_pos, "Version 2 should appear before Version 1");
    }

    #[test]
    fn test_html_escaping() {
        let edits = vec![sample_edit(
            1,
            "<script>alert('xss')</script>",
            "2024-01-15 12:00:00",
        )];
        let html = render_comment_edit_history_page(&edits).into_string();

        // Should be escaped, not contain raw script tags
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_styling_classes() {
        let edits = vec![sample_edit(1, "Content", "2024-01-15 12:00:00")];
        let html = render_comment_edit_history_page(&edits).into_string();

        // Check CSS variables are used for theming
        assert!(html.contains("var(--bg-secondary"));
        assert!(html.contains("var(--primary"));
        assert!(html.contains("var(--text-secondary"));
        assert!(html.contains("var(--bg-primary"));
    }

    #[test]
    fn test_max_height_scrollable() {
        let edits = vec![sample_edit(1, "Content", "2024-01-15 12:00:00")];
        let html = render_comment_edit_history_page(&edits).into_string();

        // Should have max-height for scrollability
        assert!(html.contains("max-height: 400px"));
        assert!(html.contains("overflow-y: auto"));
    }

    #[test]
    fn test_long_content_word_break() {
        let long_content = "verylongwordwithoutspaces".repeat(20);
        let edits = vec![sample_edit(1, &long_content, "2024-01-15 12:00:00")];
        let html = render_comment_edit_history_page(&edits).into_string();

        // Should have word-break for long content
        assert!(html.contains("word-break: break-word"));
    }

    #[test]
    fn test_edit_entry_border_styling() {
        let edits = vec![sample_edit(1, "Content", "2024-01-15 12:00:00")];
        let html = render_comment_edit_history_page(&edits).into_string();

        // Check the pink left border styling
        assert!(html.contains("border-left: 3px solid"));
    }

    #[test]
    fn test_max_width_constraint() {
        let edits = vec![sample_edit(1, "Content", "2024-01-15 12:00:00")];
        let html = render_comment_edit_history_page(&edits).into_string();

        // Should have max-width for readability
        assert!(html.contains("max-width: 600px"));
    }
}
