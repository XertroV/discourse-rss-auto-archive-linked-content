//! Site list page rendering using maud templates.
//!
//! This module provides the site-specific archive listing page using maud
//! for HTML generation. It displays all archives from a specific domain/site.

use maud::{html, Markup};

use crate::components::{ArchiveGrid, BaseLayout, DomainBadge, EmptyState, Pagination};
use crate::db::{ArchiveDisplay, User};

/// Render the site list page showing archives from a specific domain.
///
/// This function renders a page with:
/// - Site name as the page title with a domain badge
/// - Count of archives from this site
/// - Archive grid showing the archives
/// - Pagination controls (if more than one page)
///
/// # Arguments
///
/// * `site` - The domain/site name (e.g., "reddit.com")
/// * `archives` - Archives to display from this site
/// * `page` - Current page number (0-indexed)
/// * `total_pages` - Total number of pages
/// * `user` - Optional authenticated user for navigation
///
/// # Example
///
/// ```ignore
/// use crate::web::pages::site::render_site_list_page;
///
/// let html = render_site_list_page("reddit.com", &archives, 0, 5, Some(&user));
/// ```
#[must_use]
pub fn render_site_list_page(
    site: &str,
    archives: &[ArchiveDisplay],
    page: i32,
    total_pages: i32,
    user: Option<&User>,
) -> Markup {
    let page_title = format!("Archives from {site}");
    let base_url = format!("/site/{site}");

    // Build pagination (convert to usize for Pagination component)
    let current_page = page.max(0) as usize;
    let total = total_pages.max(0) as usize;
    let pagination = Pagination::new(current_page, total, &base_url);

    // Calculate archive count for display
    let archive_count = archives.len();

    // Build the main content
    let content = html! {
        h1 {
            "Archives from "
            (DomainBadge::new(site))
        }

        // Show count of archives
        p class="archive-count" {
            @if archive_count == 1 {
                "1 archive from this site"
            } @else {
                (archive_count) " archives from this site"
            }
            @if total_pages > 1 {
                " (page " (page + 1) " of " (total_pages) ")"
            }
        }

        @if archives.is_empty() {
            (EmptyState::new("No archives from this site."))
        } @else {
            (ArchiveGrid::new(archives))

            // Show pagination if needed
            @if pagination.should_display() {
                (pagination)
            }
        }
    };

    BaseLayout::new(&page_title).with_user(user).render(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_archive(id: i64, domain: &str) -> ArchiveDisplay {
        ArchiveDisplay {
            id,
            link_id: id,
            status: "complete".to_string(),
            archived_at: Some("2024-01-15 12:00:00".to_string()),
            content_title: Some(format!("Test Content {id}")),
            content_author: Some("testuser".to_string()),
            content_type: Some("video".to_string()),
            is_nsfw: false,
            error_message: None,
            retry_count: 0,
            original_url: format!("https://{domain}/content/{id}"),
            domain: domain.to_string(),
            total_size_bytes: Some(1048576),
        }
    }

    fn sample_user() -> User {
        User {
            id: 1,
            username: "testuser".to_string(),
            password_hash: "hash".to_string(),
            email: Some("test@example.com".to_string()),
            display_name: Some("Test User".to_string()),
            is_approved: true,
            is_admin: false,
            is_active: true,
            failed_login_attempts: 0,
            locked_until: None,
            password_updated_at: "2024-01-01T00:00:00Z".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_render_site_list_page_basic() {
        let archives = vec![
            sample_archive(1, "reddit.com"),
            sample_archive(2, "reddit.com"),
        ];
        let html = render_site_list_page("reddit.com", &archives, 0, 1, None).into_string();

        // Check page structure
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>Archives from reddit.com - Discourse Link Archiver</title>"));

        // Check heading with domain badge
        assert!(html.contains("Archives from"));
        assert!(html.contains("domain-badge"));
        assert!(html.contains("reddit.com"));

        // Check archive count
        assert!(html.contains("2 archives from this site"));

        // Check archive grid
        assert!(html.contains("archive-grid"));
        assert!(html.contains("Test Content 1"));
        assert!(html.contains("Test Content 2"));
    }

    #[test]
    fn test_render_site_list_page_empty() {
        let archives: Vec<ArchiveDisplay> = vec![];
        let html = render_site_list_page("example.com", &archives, 0, 1, None).into_string();

        // Should show empty state
        assert!(html.contains("No archives from this site."));
        // Should show zero count
        assert!(html.contains("0 archives from this site"));
        // Should not have archive grid content
        assert!(!html.contains("archive-card"));
    }

    #[test]
    fn test_render_site_list_page_single_archive() {
        let archives = vec![sample_archive(1, "youtube.com")];
        let html = render_site_list_page("youtube.com", &archives, 0, 1, None).into_string();

        // Check singular form
        assert!(html.contains("1 archive from this site"));
    }

    #[test]
    fn test_render_site_list_page_with_pagination() {
        let archives = vec![sample_archive(1, "tiktok.com")];
        let html = render_site_list_page("tiktok.com", &archives, 2, 5, None).into_string();

        // Should show pagination info
        assert!(html.contains("(page 3 of 5)"));

        // Should show pagination controls
        assert!(html.contains("pagination"));
        assert!(html.contains("Previous"));
        assert!(html.contains("Next"));
    }

    #[test]
    fn test_render_site_list_page_first_page_no_previous() {
        let archives = vec![sample_archive(1, "twitter.com")];
        let html = render_site_list_page("twitter.com", &archives, 0, 3, None).into_string();

        // Previous should be disabled on first page
        assert!(html.contains(r#"class="disabled"#));
        assert!(html.contains("Previous"));
    }

    #[test]
    fn test_render_site_list_page_with_user() {
        let archives = vec![sample_archive(1, "reddit.com")];
        let user = sample_user();
        let html = render_site_list_page("reddit.com", &archives, 0, 1, Some(&user)).into_string();

        // Should show profile link for logged-in user
        assert!(html.contains(r#"<a href="/profile">Profile</a>"#));
        // Should not show login link
        assert!(!html.contains(r#"<a href="/login">Login</a>"#));
    }

    #[test]
    fn test_render_site_list_page_without_user() {
        let archives = vec![sample_archive(1, "reddit.com")];
        let html = render_site_list_page("reddit.com", &archives, 0, 1, None).into_string();

        // Should show login link for anonymous user
        assert!(html.contains(r#"<a href="/login">Login</a>"#));
        // Should not show profile link
        assert!(!html.contains(r#"<a href="/profile">Profile</a>"#));
    }

    #[test]
    fn test_render_site_list_page_domain_badge_links() {
        let archives = vec![sample_archive(1, "instagram.com")];
        let html = render_site_list_page("instagram.com", &archives, 0, 1, None).into_string();

        // Domain badge should link to site page
        assert!(html.contains(r#"href="/site/instagram.com""#));
    }

    #[test]
    fn test_render_site_list_page_pagination_urls() {
        let archives = vec![sample_archive(1, "twitch.tv")];
        let html = render_site_list_page("twitch.tv", &archives, 1, 3, None).into_string();

        // Check pagination URLs contain correct site
        assert!(html.contains("/site/twitch.tv"));
    }

    #[test]
    fn test_render_site_list_page_special_characters_in_site() {
        // Test that special characters in site names are handled
        let archives = vec![sample_archive(1, "sub.example.com")];
        let html = render_site_list_page("sub.example.com", &archives, 0, 1, None).into_string();

        assert!(html.contains("Archives from sub.example.com"));
        assert!(html.contains("/site/sub.example.com"));
    }

    #[test]
    fn test_render_site_list_page_no_pagination_single_page() {
        let archives = vec![sample_archive(1, "reddit.com")];
        let html = render_site_list_page("reddit.com", &archives, 0, 1, None).into_string();

        // Should not show page info when single page
        assert!(!html.contains("(page"));
        // Should not show pagination nav
        assert!(!html.contains(r#"class="pagination""#));
    }

    #[test]
    fn test_render_site_list_page_negative_page_handled() {
        // Test that negative page numbers are handled gracefully
        let archives = vec![sample_archive(1, "reddit.com")];
        let html = render_site_list_page("reddit.com", &archives, -1, 5, None).into_string();

        // Should still render without crashing
        assert!(html.contains("Archives from"));
    }
}
