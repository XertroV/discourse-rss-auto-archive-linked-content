//! Full Text Search (FTS) utilities for safe query handling.
//!
//! This module provides parsing and sanitization functions for FTS5 queries.
//! It supports multi-keyword search, phrase matching, wildcards, column-specific
//! searches, and date range filtering.

use chrono::NaiveDate;

/// Result of parsing a user search query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedQuery {
    /// The FTS5 query string for MATCH clause.
    pub fts_query: String,
    /// Filter: archives from this date onward.
    pub date_after: Option<NaiveDate>,
    /// Filter: archives before this date.
    pub date_before: Option<NaiveDate>,
}

impl ParsedQuery {
    /// Returns true if there's no FTS query and no date filters.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fts_query.is_empty() && self.date_after.is_none() && self.date_before.is_none()
    }

    /// Returns true if there's an FTS query to execute.
    #[must_use]
    pub fn has_fts_query(&self) -> bool {
        !self.fts_query.is_empty()
    }

    /// Returns true if there are date filters.
    #[must_use]
    pub fn has_date_filters(&self) -> bool {
        self.date_after.is_some() || self.date_before.is_some()
    }
}

/// Column prefix mappings from user-friendly names to FTS5 column names.
const COLUMN_MAPPINGS: &[(&str, &str)] = &[
    ("title:", "content_title:"),
    ("author:", "content_author:"),
    ("transcript:", "transcript_text:"),
    ("text:", "full_text:"),
];

/// Parse a user search query into FTS5 syntax with date filters.
///
/// # Supported syntax
///
/// - Multiple keywords: `rust web` → finds both words (implicit AND)
/// - Quoted phrases: `"exact phrase"` → phrase match
/// - OR operator: `rust OR python` → either word
/// - NOT/exclude: `-beginner` or `NOT beginner` → exclude term
/// - Wildcards: `rust*` → prefix match (rusty, rustacean, etc.)
/// - Column search: `title:rust` → search only in titles
/// - Date filters: `after:2024-01-01` or `before:2024-06-01`
///
/// # Column prefixes
///
/// - `title:` → content_title
/// - `author:` → content_author
/// - `transcript:` → transcript_text
/// - `text:` → full_text
///
/// # Examples
///
/// ```
/// use discourse_link_archiver::db::parse_fts_query;
///
/// let result = parse_fts_query("rust web");
/// assert!(result.fts_query.contains("rust"));
/// assert!(result.fts_query.contains("web"));
///
/// let result = parse_fts_query("after:2024-01-01 rust");
/// assert!(result.date_after.is_some());
/// ```
#[must_use]
pub fn parse_fts_query(query: &str) -> ParsedQuery {
    let trimmed = query.trim();

    if trimmed.is_empty() {
        return ParsedQuery::default();
    }

    let mut result = ParsedQuery::default();
    let mut fts_parts: Vec<String> = Vec::new();
    let mut current_token = String::new();
    let mut in_quotes = false;
    let mut chars = trimmed.chars().peekable();

    // Tokenize the input, respecting quoted strings
    let mut tokens: Vec<String> = Vec::new();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                if in_quotes {
                    // End of quoted phrase
                    current_token.push('"');
                    tokens.push(current_token.clone());
                    current_token.clear();
                    in_quotes = false;
                } else {
                    // Start of quoted phrase
                    if !current_token.is_empty() {
                        tokens.push(current_token.clone());
                        current_token.clear();
                    }
                    current_token.push('"');
                    in_quotes = true;
                }
            }
            ' ' | '\t' if !in_quotes => {
                if !current_token.is_empty() {
                    tokens.push(current_token.clone());
                    current_token.clear();
                }
            }
            _ => {
                current_token.push(ch);
            }
        }
    }

    // Handle unclosed quote or remaining token
    if !current_token.is_empty() {
        if in_quotes {
            // Unclosed quote - close it
            current_token.push('"');
        }
        tokens.push(current_token);
    }

    // Process each token
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];

        // Handle date filters
        if let Some(date_str) = token.strip_prefix("after:") {
            if let Some(date) = parse_date(date_str) {
                result.date_after = Some(date);
            }
            i += 1;
            continue;
        }

        if let Some(date_str) = token.strip_prefix("before:") {
            if let Some(date) = parse_date(date_str) {
                result.date_before = Some(date);
            }
            i += 1;
            continue;
        }

        // Handle OR operator
        if token.eq_ignore_ascii_case("OR") {
            if !fts_parts.is_empty() && i + 1 < tokens.len() {
                fts_parts.push("OR".to_string());
            }
            i += 1;
            continue;
        }

        // Handle NOT operator (standalone)
        if token.eq_ignore_ascii_case("NOT") {
            if i + 1 < tokens.len() {
                let next_token = &tokens[i + 1];
                let processed = process_term(next_token);
                fts_parts.push(format!("NOT {}", processed));
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }

        // Handle - prefix (NOT)
        if let Some(rest) = token.strip_prefix('-') {
            if !rest.is_empty() {
                let processed = process_term(rest);
                fts_parts.push(format!("NOT {}", processed));
                i += 1;
                continue;
            }
        }

        // Handle column-prefixed terms
        let mut processed = token.clone();
        for (user_prefix, fts_prefix) in COLUMN_MAPPINGS {
            if let Some(rest) = token.strip_prefix(user_prefix) {
                let term = process_term(rest);
                processed = format!("{}{}", fts_prefix, term);
                break;
            }
        }

        // Regular term or quoted phrase
        if processed == *token {
            processed = process_term(token);
        }

        fts_parts.push(processed);
        i += 1;
    }

    result.fts_query = fts_parts.join(" ");
    result
}

/// Process a single term for FTS5.
///
/// - Quoted phrases are passed through (with internal quotes escaped).
/// - Terms with wildcards (*) are left unquoted.
/// - Other terms are quoted for safety.
fn process_term(term: &str) -> String {
    // Already quoted phrase
    if term.starts_with('"') && term.ends_with('"') && term.len() > 1 {
        // Escape any internal quotes (except the outer ones)
        let inner = &term[1..term.len() - 1];
        let escaped = inner.replace('"', "\"\"");
        return format!("\"{}\"", escaped);
    }

    // Has wildcard - don't quote (FTS5 needs unquoted wildcards)
    if term.ends_with('*') {
        // Validate it's a simple alphanumeric prefix
        let prefix = &term[..term.len() - 1];
        if prefix.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return term.to_string();
        }
    }

    // Regular term - quote it for safety
    let escaped = term.replace('"', "\"\"");
    format!("\"{}\"", escaped)
}

/// Parse a date string in various formats.
fn parse_date(s: &str) -> Option<NaiveDate> {
    // Try YYYY-MM-DD
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(date);
    }
    // Try YYYY-MM (assume first of month)
    if let Ok(date) = NaiveDate::parse_from_str(&format!("{}-01", s), "%Y-%m-%d") {
        return Some(date);
    }
    // Try YYYY (assume Jan 1)
    if let Ok(date) = NaiveDate::parse_from_str(&format!("{}-01-01", s), "%Y-%m-%d") {
        return Some(date);
    }
    None
}

/// Sanitize a user query for safe use with SQLite FTS5 MATCH queries.
///
/// This is the legacy function that wraps the entire query in quotes for phrase matching.
/// For advanced search features, use `parse_fts_query` instead.
///
/// # Arguments
///
/// * `query` - The raw user search query
///
/// # Returns
///
/// A sanitized query string safe for FTS5 MATCH, or an empty string if the
/// input is empty or whitespace-only.
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

    // Tests for the new parse_fts_query function

    #[test]
    fn test_parse_empty() {
        let result = parse_fts_query("");
        assert!(result.is_empty());
        assert_eq!(result.fts_query, "");
    }

    #[test]
    fn test_parse_single_word() {
        let result = parse_fts_query("rust");
        assert_eq!(result.fts_query, "\"rust\"");
        assert!(result.date_after.is_none());
        assert!(result.date_before.is_none());
    }

    #[test]
    fn test_parse_multiple_words() {
        let result = parse_fts_query("rust web async");
        assert!(result.fts_query.contains("\"rust\""));
        assert!(result.fts_query.contains("\"web\""));
        assert!(result.fts_query.contains("\"async\""));
    }

    #[test]
    fn test_parse_quoted_phrase() {
        let result = parse_fts_query("\"exact phrase\"");
        assert_eq!(result.fts_query, "\"exact phrase\"");
    }

    #[test]
    fn test_parse_mixed_words_and_phrases() {
        let result = parse_fts_query("rust \"error handling\" async");
        assert!(result.fts_query.contains("\"rust\""));
        assert!(result.fts_query.contains("\"error handling\""));
        assert!(result.fts_query.contains("\"async\""));
    }

    #[test]
    fn test_parse_or_operator() {
        let result = parse_fts_query("rust OR python");
        assert!(result.fts_query.contains("OR"));
        assert!(result.fts_query.contains("\"rust\""));
        assert!(result.fts_query.contains("\"python\""));
    }

    #[test]
    fn test_parse_not_operator_prefix() {
        let result = parse_fts_query("rust -beginner");
        assert!(result.fts_query.contains("\"rust\""));
        assert!(result.fts_query.contains("NOT"));
        assert!(result.fts_query.contains("\"beginner\""));
    }

    #[test]
    fn test_parse_not_operator_word() {
        let result = parse_fts_query("rust NOT beginner");
        assert!(result.fts_query.contains("\"rust\""));
        assert!(result.fts_query.contains("NOT"));
        assert!(result.fts_query.contains("\"beginner\""));
    }

    #[test]
    fn test_parse_wildcard() {
        let result = parse_fts_query("rust*");
        assert_eq!(result.fts_query, "rust*");
    }

    #[test]
    fn test_parse_wildcard_with_other_terms() {
        let result = parse_fts_query("test* programming");
        assert!(result.fts_query.contains("test*"));
        assert!(result.fts_query.contains("\"programming\""));
    }

    #[test]
    fn test_parse_column_title() {
        let result = parse_fts_query("title:rust");
        assert!(result.fts_query.contains("content_title:"));
    }

    #[test]
    fn test_parse_column_author() {
        let result = parse_fts_query("author:john");
        assert!(result.fts_query.contains("content_author:"));
    }

    #[test]
    fn test_parse_column_transcript() {
        let result = parse_fts_query("transcript:hello");
        assert!(result.fts_query.contains("transcript_text:"));
    }

    #[test]
    fn test_parse_column_text() {
        let result = parse_fts_query("text:world");
        assert!(result.fts_query.contains("full_text:"));
    }

    #[test]
    fn test_parse_date_after() {
        let result = parse_fts_query("after:2024-01-15 rust");
        assert_eq!(
            result.date_after,
            Some(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap())
        );
        assert!(result.fts_query.contains("\"rust\""));
        assert!(!result.fts_query.contains("after"));
    }

    #[test]
    fn test_parse_date_before() {
        let result = parse_fts_query("before:2024-06-30 python");
        assert_eq!(
            result.date_before,
            Some(NaiveDate::from_ymd_opt(2024, 6, 30).unwrap())
        );
        assert!(result.fts_query.contains("\"python\""));
    }

    #[test]
    fn test_parse_date_range() {
        let result = parse_fts_query("after:2024-01-01 before:2024-12-31 rust");
        assert!(result.date_after.is_some());
        assert!(result.date_before.is_some());
        assert!(result.fts_query.contains("\"rust\""));
    }

    #[test]
    fn test_parse_date_year_only() {
        let result = parse_fts_query("after:2024");
        assert_eq!(
            result.date_after,
            Some(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())
        );
    }

    #[test]
    fn test_parse_date_year_month() {
        let result = parse_fts_query("after:2024-06");
        assert_eq!(
            result.date_after,
            Some(NaiveDate::from_ymd_opt(2024, 6, 1).unwrap())
        );
    }

    #[test]
    fn test_parse_complex_query() {
        let result =
            parse_fts_query("title:rust OR python -beginner after:2024-01-01 \"error handling\"");
        assert!(result.fts_query.contains("content_title:"));
        assert!(result.fts_query.contains("OR"));
        assert!(result.fts_query.contains("NOT"));
        assert!(result.fts_query.contains("\"error handling\""));
        assert!(result.date_after.is_some());
    }

    #[test]
    fn test_parse_apostrophes() {
        let result = parse_fts_query("let's don't");
        assert!(result.fts_query.contains("\"let's\""));
        assert!(result.fts_query.contains("\"don't\""));
    }

    #[test]
    fn test_parse_unclosed_quote() {
        let result = parse_fts_query("\"unclosed phrase");
        // Should auto-close the quote
        assert!(result.fts_query.contains("\"unclosed phrase\""));
    }

    #[test]
    fn test_parse_unicode() {
        let result = parse_fts_query("café 日本語");
        assert!(result.fts_query.contains("\"café\""));
        assert!(result.fts_query.contains("\"日本語\""));
    }

    // Legacy sanitize_fts_query tests

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
