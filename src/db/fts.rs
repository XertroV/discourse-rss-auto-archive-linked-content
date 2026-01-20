//! Full Text Search (FTS) utilities for safe query handling.
//!
//! This module provides sanitization functions for FTS5 queries to prevent
//! syntax errors from special characters like apostrophes, quotes, and operators.

/// Sanitize a user query for safe use with SQLite FTS5 MATCH queries.
///
/// This function wraps the user's query in double quotes to enable phrase matching
/// and prevent FTS5 syntax errors from special characters (apostrophes, quotes,
/// operators like -, *, etc.).
///
/// Internal double quotes are escaped by doubling them, which is the FTS5 escape
/// mechanism within quoted strings.
///
/// # Arguments
///
/// * `query` - The raw user search query
///
/// # Returns
///
/// A sanitized query string safe for FTS5 MATCH, or an empty string if the
/// input is empty or whitespace-only.
///
/// # Examples
///
/// ```
/// use discourse_link_archiver::db::sanitize_fts_query;
///
/// assert_eq!(sanitize_fts_query("let's"), "\"let's\"");
/// assert_eq!(sanitize_fts_query("test query"), "\"test query\"");
/// assert_eq!(sanitize_fts_query("foo \"bar\" baz"), "\"foo \"\"bar\"\" baz\"");
/// assert_eq!(sanitize_fts_query(""), "");
/// assert_eq!(sanitize_fts_query("   "), "");
/// ```
#[must_use]
pub fn sanitize_fts_query(query: &str) -> String {
    let trimmed = query.trim();

    // Return empty string for empty/whitespace-only queries
    if trimmed.is_empty() {
        return String::new();
    }

    // Escape internal double quotes by doubling them
    let escaped = trimmed.replace('"', "\"\"");

    // Wrap in double quotes for phrase matching
    format!("\"{}\"", escaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_simple_query() {
        assert_eq!(sanitize_fts_query("rust"), "\"rust\"");
        assert_eq!(sanitize_fts_query("hello world"), "\"hello world\"");
    }

    #[test]
    fn test_sanitize_apostrophes() {
        assert_eq!(sanitize_fts_query("let's"), "\"let's\"");
        assert_eq!(sanitize_fts_query("don't"), "\"don't\"");
        assert_eq!(sanitize_fts_query("can't won't"), "\"can't won't\"");
    }

    #[test]
    fn test_sanitize_quotes() {
        assert_eq!(
            sanitize_fts_query("test \"query\""),
            "\"test \"\"query\"\"\""
        );
        assert_eq!(
            sanitize_fts_query("foo \"bar\" baz"),
            "\"foo \"\"bar\"\" baz\""
        );
        assert_eq!(sanitize_fts_query("\"quoted\""), "\"\"\"quoted\"\"\"");
    }

    #[test]
    fn test_sanitize_special_operators() {
        // Hyphens (NOT operator)
        assert_eq!(sanitize_fts_query("test-query"), "\"test-query\"");
        assert_eq!(sanitize_fts_query("-excluded"), "\"-excluded\"");

        // Asterisks (wildcard)
        assert_eq!(sanitize_fts_query("test*"), "\"test*\"");
        assert_eq!(sanitize_fts_query("*wildcard*"), "\"*wildcard*\"");

        // Parentheses (grouping)
        assert_eq!(sanitize_fts_query("(test)"), "\"(test)\"");
        assert_eq!(sanitize_fts_query("(foo OR bar)"), "\"(foo OR bar)\"");

        // Colons (column-specific search)
        assert_eq!(sanitize_fts_query("title:test"), "\"title:test\"");
    }

    #[test]
    fn test_sanitize_empty_and_whitespace() {
        assert_eq!(sanitize_fts_query(""), "");
        assert_eq!(sanitize_fts_query("   "), "");
        assert_eq!(sanitize_fts_query("\t\n  "), "");
    }

    #[test]
    fn test_sanitize_trimming() {
        assert_eq!(sanitize_fts_query("  test  "), "\"test\"");
        assert_eq!(sanitize_fts_query("\tquery\n"), "\"query\"");
    }

    #[test]
    fn test_sanitize_unicode() {
        assert_eq!(sanitize_fts_query("test 🔥"), "\"test 🔥\"");
        assert_eq!(sanitize_fts_query("café"), "\"café\"");
        assert_eq!(sanitize_fts_query("日本語"), "\"日本語\"");
    }

    #[test]
    fn test_sanitize_complex_queries() {
        assert_eq!(
            sanitize_fts_query("let's test \"complex\" query*"),
            "\"let's test \"\"complex\"\" query*\""
        );
        assert_eq!(
            sanitize_fts_query("don't use -operators (please)"),
            "\"don't use -operators (please)\""
        );
    }

    #[test]
    fn test_sanitize_multiple_quotes() {
        assert_eq!(
            sanitize_fts_query("\"one\" \"two\" \"three\""),
            "\"\"\"one\"\" \"\"two\"\" \"\"three\"\"\""
        );
    }
}
