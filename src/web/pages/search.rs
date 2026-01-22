//! Search page for the web UI.
//!
//! This module provides the search page implementation using maud templates.

use maud::{html, Markup, Render};
use urlencoding::encode;

use crate::components::{ArchiveGrid, BaseLayout, Button, EmptyState, Form, Input};
use crate::db::{ArchiveDisplay, User};

/// Parameters for rendering the search page.
#[derive(Debug)]
pub struct SearchPageParams<'a> {
    /// The search query string (empty if no search performed)
    pub query: Option<&'a str>,
    /// Archives matching the search
    pub archives: &'a [ArchiveDisplay],
    /// Current page number (0-indexed)
    pub page: i32,
    /// Total number of pages
    pub total_pages: i32,
    /// Authenticated user, if any
    pub user: Option<&'a User>,
}

impl<'a> SearchPageParams<'a> {
    /// Create new search page parameters.
    #[must_use]
    pub fn new(
        query: Option<&'a str>,
        archives: &'a [ArchiveDisplay],
        page: i32,
        total_pages: i32,
        user: Option<&'a User>,
    ) -> Self {
        Self {
            query,
            archives,
            page,
            total_pages,
            user,
        }
    }
}

/// Render the search page.
///
/// This function creates a complete search page with:
/// - A search form at the top
/// - Search help/tips section
/// - Result count (when a query is provided)
/// - Archive grid showing search results
/// - Pagination controls at the bottom
#[must_use]
pub fn render_search_page(
    query: Option<&str>,
    archives: &[ArchiveDisplay],
    page: i32,
    total_pages: i32,
    user: Option<&User>,
) -> Markup {
    let query_str = query.unwrap_or("");

    let content = html! {
        h1 { "Search Archives" }

        // Search form
        (SearchForm::new(query_str))

        // Search help section
        (SearchHelp)

        // Show result count if search was performed
        @if !query_str.is_empty() {
            p {
                "Found " (archives.len()) " results for \""
                (query_str)
                "\""
            }
        }

        // Show archives or empty state
        @if archives.is_empty() && !query_str.is_empty() {
            (EmptyState::no_results())
        } @else if !archives.is_empty() {
            (ArchiveGrid::new(archives))
        }

        // Pagination
        @if total_pages > 1 {
            (SearchPagination::new(query_str, page as usize, total_pages as usize))
        }
    };

    BaseLayout::new("Search", user).render(content)
}

/// Search help component showing available search syntax.
struct SearchHelp;

impl Render for SearchHelp {
    fn render(&self) -> Markup {
        html! {
            details class="search-help" {
                summary { "Search Tips" }
                div class="help-content" {
                    div class="help-section" {
                        h4 { "Basic Search" }
                        dl {
                            dt { code { "rust async" } }
                            dd { "finds archives with both words (any order)" }

                            dt { code { "\"error handling\"" } }
                            dd { "exact phrase match" }

                            dt { code { "rust OR python" } }
                            dd { "matches either word" }

                            dt { code { "rust -beginner" } }
                            dd { "rust but NOT beginner" }

                            dt { code { "test*" } }
                            dd { "wildcard: test, testing, tested..." }
                        }
                    }

                    div class="help-section" {
                        h4 { "Advanced" }
                        dl {
                            dt { code { "title:rust" } }
                            dd { "search only in titles" }

                            dt { code { "author:john" } }
                            dd { "search only in author names" }

                            dt { code { "transcript:hello" } }
                            dd { "search in video transcripts" }

                            dt { code { "after:2024-01-01" } }
                            dd { "archives from date onward" }

                            dt { code { "before:2024-06-01" } }
                            dd { "archives before date" }
                        }
                    }

                    p class="help-note" {
                        "Searches titles, authors, transcripts, page text, and URLs."
                    }
                }
            }
        }
    }
}

/// A search form component.
#[derive(Debug)]
struct SearchForm<'a> {
    query: &'a str,
}

impl<'a> SearchForm<'a> {
    fn new(query: &'a str) -> Self {
        Self { query }
    }
}

impl Render for SearchForm<'_> {
    fn render(&self) -> Markup {
        let form_content = html! {
            (Input::search("q").value_opt(if self.query.is_empty() { None } else { Some(self.query) }).placeholder("Search..."))
            (Button::primary("Search").r#type("submit"))
        };

        Form::get("/search", form_content).render()
    }
}

/// Pagination specifically for search results, preserving the query parameter.
#[derive(Debug)]
struct SearchPagination<'a> {
    query: &'a str,
    current_page: usize,
    total_pages: usize,
}

impl<'a> SearchPagination<'a> {
    fn new(query: &'a str, current_page: usize, total_pages: usize) -> Self {
        Self {
            query,
            current_page,
            total_pages,
        }
    }

    /// Build URL for a specific page number.
    fn build_url(&self, page_num: usize) -> String {
        let encoded_query = encode(self.query);
        if page_num == 0 {
            format!("/search?q={encoded_query}")
        } else {
            format!("/search?q={encoded_query}&page={page_num}")
        }
    }
}

impl Render for SearchPagination<'_> {
    fn render(&self) -> Markup {
        // Don't render if only one page
        if self.total_pages <= 1 {
            return html! {};
        }

        let current = self.current_page;
        let total = self.total_pages;

        // Calculate the range of page numbers to display
        let start = current.saturating_sub(2);
        let end = (current + 3).min(total);

        html! {
            nav class="pagination" {
                // Previous button
                @if current > 0 {
                    a href=(self.build_url(current - 1)) { "\u{00ab} Previous" }
                } @else {
                    span class="disabled" { "\u{00ab} Previous" }
                }

                // First page and ellipsis if needed
                @if start > 0 {
                    a href=(self.build_url(0)) { "1" }
                    @if start > 1 {
                        span { "..." }
                    }
                }

                // Page numbers around current page
                @for page_num in start..end {
                    @if page_num == current {
                        span class="current" { (page_num + 1) }
                    } @else {
                        a href=(self.build_url(page_num)) { (page_num + 1) }
                    }
                }

                // Ellipsis and last page if needed
                @if end < total {
                    @if end < total - 1 {
                        span { "..." }
                    }
                    a href=(self.build_url(total - 1)) { (total) }
                }

                // Next button
                @if current + 1 < total {
                    a href=(self.build_url(current + 1)) { "Next \u{00bb}" }
                } @else {
                    span class="disabled" { "Next \u{00bb}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a sample archive for testing.
    fn sample_archive(id: i64, title: &str) -> ArchiveDisplay {
        ArchiveDisplay {
            id,
            link_id: id,
            status: "complete".to_string(),
            archived_at: Some("2024-01-15 12:00:00".to_string()),
            content_title: Some(title.to_string()),
            content_author: Some("testuser".to_string()),
            content_type: Some("video".to_string()),
            is_nsfw: false,
            error_message: None,
            retry_count: 0,
            original_url: format!("https://example.com/video/{id}"),
            domain: "example.com".to_string(),
            total_size_bytes: Some(1048576),
        }
    }

    /// Create a test user.
    fn test_user(is_admin: bool) -> User {
        User {
            id: 1,
            username: "testuser".to_string(),
            password_hash: "hash".to_string(),
            email: Some("test@example.com".to_string()),
            display_name: Some("Test User".to_string()),
            is_approved: true,
            is_admin,
            is_active: true,
            failed_login_attempts: 0,
            locked_until: None,
            password_updated_at: "2024-01-01T00:00:00Z".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_render_search_page_empty_query() {
        let archives: Vec<ArchiveDisplay> = vec![];
        let html = render_search_page(None, &archives, 0, 1, None).into_string();

        // Check basic structure
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Search Archives"));
        assert!(html.contains("Search - Discourse Link Archiver"));

        // Check search form
        assert!(html.contains(r#"action="/search""#));
        assert!(html.contains(r#"method="get""#));
        assert!(html.contains(r#"type="search""#));
        assert!(html.contains(r#"name="q""#));
        assert!(html.contains(r#"placeholder="Search...""#));

        // Should not show result count for empty query
        assert!(!html.contains("Found"));
        assert!(!html.contains("results for"));
    }

    #[test]
    fn test_render_search_page_with_query_no_results() {
        let archives: Vec<ArchiveDisplay> = vec![];
        let html = render_search_page(Some("nonexistent"), &archives, 0, 1, None).into_string();

        // Check result count
        assert!(html.contains("Found 0 results for"));
        assert!(html.contains("nonexistent"));

        // Check empty state
        assert!(html.contains("No results found"));
    }

    #[test]
    fn test_render_search_page_with_results() {
        let archives = vec![
            sample_archive(1, "First Result"),
            sample_archive(2, "Second Result"),
        ];
        let html = render_search_page(Some("test"), &archives, 0, 1, None).into_string();

        // Check result count
        assert!(html.contains("Found 2 results for"));
        assert!(html.contains("test"));

        // Check archive grid
        assert!(html.contains("archive-grid"));
        assert!(html.contains("First Result"));
        assert!(html.contains("Second Result"));
    }

    #[test]
    fn test_render_search_page_with_pagination() {
        let archives = vec![sample_archive(1, "Result")];
        let html = render_search_page(Some("test"), &archives, 0, 5, None).into_string();

        // Check pagination is present
        assert!(html.contains(r#"class="pagination""#));
        assert!(html.contains("Next"));

        // Check query is preserved in pagination URLs
        assert!(html.contains("/search?q=test"));
    }

    #[test]
    fn test_render_search_page_with_user() {
        let archives: Vec<ArchiveDisplay> = vec![];
        let user = test_user(false);
        let html = render_search_page(None, &archives, 0, 1, Some(&user)).into_string();

        // Check user-specific navigation
        assert!(html.contains(r#"href="/profile"#));
        assert!(!html.contains(r#"href="/login"#));
    }

    #[test]
    fn test_render_search_page_with_admin_user() {
        let archives: Vec<ArchiveDisplay> = vec![];
        let user = test_user(true);
        let html = render_search_page(None, &archives, 0, 1, Some(&user)).into_string();

        // Check admin-specific navigation
        assert!(html.contains(r#"href="/admin"#));
    }

    #[test]
    fn test_search_form_with_query() {
        let form = SearchForm::new("my search");
        let html = form.render().into_string();

        assert!(html.contains(r#"value="my search""#));
    }

    #[test]
    fn test_search_form_empty_query() {
        let form = SearchForm::new("");
        let html = form.render().into_string();

        // Empty query should not have value attribute
        assert!(!html.contains(r#"value="""#));
    }

    #[test]
    fn test_search_pagination_build_url() {
        let pagination = SearchPagination::new("test query", 0, 5);

        // Page 0 should not have page parameter
        assert_eq!(pagination.build_url(0), "/search?q=test%20query");

        // Other pages should have page parameter
        assert_eq!(pagination.build_url(2), "/search?q=test%20query&page=2");
    }

    #[test]
    fn test_search_pagination_special_characters() {
        let pagination = SearchPagination::new("test&query=value", 0, 5);

        // Special characters should be URL encoded
        let url = pagination.build_url(0);
        assert!(url.contains("test%26query%3Dvalue"));
    }

    #[test]
    fn test_search_pagination_single_page() {
        let pagination = SearchPagination::new("test", 0, 1);
        let html = pagination.render().into_string();

        // Single page should render nothing
        assert!(html.is_empty());
    }

    #[test]
    fn test_search_pagination_first_page() {
        let pagination = SearchPagination::new("test", 0, 10);
        let html = pagination.render().into_string();

        // Previous should be disabled
        assert!(html.contains(r#"class="disabled">"#));
        assert!(html.contains("Previous"));

        // Current page should be marked
        assert!(html.contains(r#"class="current""#));

        // Next should be active
        assert!(html.contains("page=1"));
    }

    #[test]
    fn test_search_pagination_last_page() {
        let pagination = SearchPagination::new("test", 9, 10);
        let html = pagination.render().into_string();

        // Previous should be active
        assert!(html.contains("page=8"));

        // Next should be disabled
        assert!(html.contains(r#"class="disabled">Next"#));
    }

    #[test]
    fn test_search_pagination_middle_page() {
        let pagination = SearchPagination::new("test", 5, 10);
        let html = pagination.render().into_string();

        // Should have first page
        assert!(html.contains(">1<"));

        // Should have ellipsis
        assert!(html.contains("..."));

        // Should have last page
        assert!(html.contains(">10<"));

        // Both previous and next should be active
        assert!(html.contains("page=4")); // Previous
        assert!(html.contains("page=6")); // Next
    }

    #[test]
    fn test_search_page_params() {
        let archives = vec![sample_archive(1, "Test")];
        let user = test_user(false);
        let params = SearchPageParams::new(Some("query"), &archives, 0, 5, Some(&user));

        assert_eq!(params.query, Some("query"));
        assert_eq!(params.archives.len(), 1);
        assert_eq!(params.page, 0);
        assert_eq!(params.total_pages, 5);
        assert!(params.user.is_some());
    }
}
