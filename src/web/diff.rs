//! Text diffing utilities for archive comparison.

use similar::{ChangeTag, TextDiff};

/// A single line in a diff with its change type.
#[derive(Debug, Clone)]
pub struct DiffLine {
    /// The line content.
    pub content: String,
    /// The type of change: added, removed, or unchanged.
    pub change_type: ChangeType,
    /// Line number in the old text (None for added lines).
    pub old_line_num: Option<usize>,
    /// Line number in the new text (None for removed lines).
    pub new_line_num: Option<usize>,
}

/// Type of change for a diff line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    /// Line was added in the new text.
    Added,
    /// Line was removed from the old text.
    Removed,
    /// Line is unchanged.
    Unchanged,
}

impl ChangeType {
    /// Get a CSS class name for this change type.
    #[must_use]
    pub const fn css_class(self) -> &'static str {
        match self {
            Self::Added => "diff-added",
            Self::Removed => "diff-removed",
            Self::Unchanged => "diff-unchanged",
        }
    }

    /// Get a display symbol for this change type.
    #[must_use]
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Added => "+",
            Self::Removed => "-",
            Self::Unchanged => " ",
        }
    }
}

/// Result of comparing two texts.
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// The diff lines.
    pub lines: Vec<DiffLine>,
    /// Number of added lines.
    pub additions: usize,
    /// Number of removed lines.
    pub deletions: usize,
    /// Whether the texts are identical.
    pub is_identical: bool,
}

/// Compare two texts and return a diff result.
#[must_use]
pub fn compute_diff(old_text: &str, new_text: &str) -> DiffResult {
    let diff = TextDiff::from_lines(old_text, new_text);

    let mut lines = Vec::new();
    let mut additions = 0;
    let mut deletions = 0;
    let mut old_line_num = 1usize;
    let mut new_line_num = 1usize;

    for change in diff.iter_all_changes() {
        let (change_type, old_num, new_num) = match change.tag() {
            ChangeTag::Delete => {
                deletions += 1;
                let num = old_line_num;
                old_line_num += 1;
                (ChangeType::Removed, Some(num), None)
            }
            ChangeTag::Insert => {
                additions += 1;
                let num = new_line_num;
                new_line_num += 1;
                (ChangeType::Added, None, Some(num))
            }
            ChangeTag::Equal => {
                let old_num = old_line_num;
                let new_num = new_line_num;
                old_line_num += 1;
                new_line_num += 1;
                (ChangeType::Unchanged, Some(old_num), Some(new_num))
            }
        };

        lines.push(DiffLine {
            content: change.to_string_lossy().trim_end_matches('\n').to_string(),
            change_type,
            old_line_num: old_num,
            new_line_num: new_num,
        });
    }

    let is_identical = additions == 0 && deletions == 0;

    DiffResult {
        lines,
        additions,
        deletions,
        is_identical,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_texts() {
        let text = "Hello\nWorld";
        let result = compute_diff(text, text);

        assert!(result.is_identical);
        assert_eq!(result.additions, 0);
        assert_eq!(result.deletions, 0);
        assert_eq!(result.lines.len(), 2);
    }

    #[test]
    fn test_added_line() {
        // Use consistent newline endings to avoid diff artifacts
        let old = "Line 1\nLine 2\n";
        let new = "Line 1\nLine 2\nLine 3\n";
        let result = compute_diff(old, new);

        assert!(!result.is_identical);
        assert_eq!(result.additions, 1);
        assert_eq!(result.deletions, 0);
        assert!(result
            .lines
            .iter()
            .any(|l| l.change_type == ChangeType::Added && l.content == "Line 3"));
    }

    #[test]
    fn test_removed_line() {
        // Use consistent newline endings to avoid diff artifacts
        let old = "Line 1\nLine 2\nLine 3\n";
        let new = "Line 1\nLine 2\n";
        let result = compute_diff(old, new);

        assert!(!result.is_identical);
        assert_eq!(result.additions, 0);
        assert_eq!(result.deletions, 1);
    }

    #[test]
    fn test_changed_line() {
        let old = "Hello World";
        let new = "Hello Rust";
        let result = compute_diff(old, new);

        assert!(!result.is_identical);
        assert_eq!(result.additions, 1);
        assert_eq!(result.deletions, 1);
    }

    #[test]
    fn test_empty_texts() {
        let result = compute_diff("", "");
        assert!(result.is_identical);
        assert_eq!(result.lines.len(), 0);
    }

    #[test]
    fn test_css_classes() {
        assert_eq!(ChangeType::Added.css_class(), "diff-added");
        assert_eq!(ChangeType::Removed.css_class(), "diff-removed");
        assert_eq!(ChangeType::Unchanged.css_class(), "diff-unchanged");
    }

    #[test]
    fn test_symbols() {
        assert_eq!(ChangeType::Added.symbol(), "+");
        assert_eq!(ChangeType::Removed.symbol(), "-");
        assert_eq!(ChangeType::Unchanged.symbol(), " ");
    }
}
