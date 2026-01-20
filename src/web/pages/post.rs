//! Post page templates using maud.
//!
//! This module provides maud-based templates for post-related pages:
//! - Post detail page showing archives from a specific Discourse post

use maud::{html, Markup};

use crate::components::{ArchiveGrid, BaseLayout, EmptyState};
use crate::db::{ArchiveDisplay, Post, User};

/// Parameters for the post detail page.
#[derive(Debug, Clone)]
pub struct PostDetailParams<'a> {
    pub post: &'a Post,
    pub archives: &'a [ArchiveDisplay],
    pub user: Option<&'a User>,
}

/// Render the post detail page showing all archives from a post.
#[must_use]
pub fn render_post_detail_page(params: &PostDetailParams<'_>) -> Markup {
    let post = params.post;
    let title = post.title.as_deref().unwrap_or("Untitled Post");
    let author = post.author.as_deref().unwrap_or("Unknown");
    let published = post.published_at.as_deref().unwrap_or("Unknown");

    let content = html! {
        h1 { (title) }

        article {
            header {
                p class="meta" {
                    strong { "Author:" } " " (author)
                    br;
                    strong { "Published:" } " " (published)
                    br;
                    strong { "Source:" } " "
                    a href=(post.discourse_url) target="_blank" rel="noopener" { (post.discourse_url) }
                }
            }
        }

        section {
            h2 { "Archived Links" }

            @if params.archives.is_empty() {
                (EmptyState::new("No archives from this post."))
            } @else {
                p {
                    "Found " (params.archives.len()) " archived link(s) from this post."
                }
                (ArchiveGrid::new(params.archives))
            }
        }
    };

    BaseLayout::new(&format!("Post: {title}"))
        .with_user(params.user)
        .render(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_post() -> Post {
        Post {
            id: 1,
            guid: "post-guid-123".to_string(),
            discourse_url: "https://forum.example.com/t/test-thread/123/1".to_string(),
            author: Some("testauthor".to_string()),
            title: Some("Test Post Title".to_string()),
            body_html: Some("<p>Post content</p>".to_string()),
            content_hash: Some("abc123".to_string()),
            published_at: Some("2024-01-15 12:00:00".to_string()),
            processed_at: "2024-01-15 12:30:00".to_string(),
        }
    }

    fn sample_archive() -> ArchiveDisplay {
        ArchiveDisplay {
            id: 1,
            link_id: 1,
            status: "complete".to_string(),
            archived_at: Some("2024-01-15 14:00:00".to_string()),
            content_title: Some("Archived Content".to_string()),
            content_author: Some("contentauthor".to_string()),
            content_type: Some("video".to_string()),
            is_nsfw: false,
            error_message: None,
            retry_count: 0,
            original_url: "https://example.com/video/123".to_string(),
            domain: "example.com".to_string(),
            total_size_bytes: Some(1048576),
        }
    }

    #[test]
    fn test_post_detail_page_with_archives() {
        let post = sample_post();
        let archives = vec![sample_archive()];
        let params = PostDetailParams {
            post: &post,
            archives: &archives,
            user: None,
        };
        let html = render_post_detail_page(&params).into_string();

        // Check page structure
        assert!(html.contains("Test Post Title"));
        assert!(html.contains("testauthor"));
        assert!(html.contains("2024-01-15 12:00:00"));
        assert!(html.contains("forum.example.com"));

        // Check archives section
        assert!(html.contains("Archived Links"));
        assert!(html.contains("Found 1 archived link(s) from this post."));
        assert!(html.contains("archive-grid"));
        assert!(html.contains("Archived Content"));
    }

    #[test]
    fn test_post_detail_page_no_archives() {
        let post = sample_post();
        let archives: Vec<ArchiveDisplay> = vec![];
        let params = PostDetailParams {
            post: &post,
            archives: &archives,
            user: None,
        };
        let html = render_post_detail_page(&params).into_string();

        assert!(html.contains("No archives from this post."));
        assert!(!html.contains("archive-grid"));
    }

    #[test]
    fn test_post_detail_page_untitled() {
        let mut post = sample_post();
        post.title = None;
        let archives: Vec<ArchiveDisplay> = vec![];
        let params = PostDetailParams {
            post: &post,
            archives: &archives,
            user: None,
        };
        let html = render_post_detail_page(&params).into_string();

        assert!(html.contains("Untitled Post"));
        assert!(html.contains("<title>Post: Untitled Post"));
    }

    #[test]
    fn test_post_detail_page_unknown_author() {
        let mut post = sample_post();
        post.author = None;
        let archives: Vec<ArchiveDisplay> = vec![];
        let params = PostDetailParams {
            post: &post,
            archives: &archives,
            user: None,
        };
        let html = render_post_detail_page(&params).into_string();

        assert!(html.contains("<strong>Author:</strong> Unknown"));
    }

    #[test]
    fn test_post_detail_page_unknown_date() {
        let mut post = sample_post();
        post.published_at = None;
        let archives: Vec<ArchiveDisplay> = vec![];
        let params = PostDetailParams {
            post: &post,
            archives: &archives,
            user: None,
        };
        let html = render_post_detail_page(&params).into_string();

        assert!(html.contains("<strong>Published:</strong> Unknown"));
    }

    #[test]
    fn test_post_detail_page_external_link() {
        let post = sample_post();
        let archives: Vec<ArchiveDisplay> = vec![];
        let params = PostDetailParams {
            post: &post,
            archives: &archives,
            user: None,
        };
        let html = render_post_detail_page(&params).into_string();

        // Check that the source link opens in new tab
        assert!(html.contains("target=\"_blank\""));
        assert!(html.contains("rel=\"noopener\""));
    }

    #[test]
    fn test_post_detail_page_multiple_archives() {
        let post = sample_post();
        let archives = vec![
            sample_archive(),
            {
                let mut a = sample_archive();
                a.id = 2;
                a.content_title = Some("Second Archive".to_string());
                a
            },
            {
                let mut a = sample_archive();
                a.id = 3;
                a.content_title = Some("Third Archive".to_string());
                a
            },
        ];
        let params = PostDetailParams {
            post: &post,
            archives: &archives,
            user: None,
        };
        let html = render_post_detail_page(&params).into_string();

        assert!(html.contains("Found 3 archived link(s) from this post."));
        assert!(html.contains("Archived Content"));
        assert!(html.contains("Second Archive"));
        assert!(html.contains("Third Archive"));
    }

    #[test]
    fn test_post_detail_page_with_user() {
        let post = sample_post();
        let archives: Vec<ArchiveDisplay> = vec![];
        let user = User {
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
        };
        let params = PostDetailParams {
            post: &post,
            archives: &archives,
            user: Some(&user),
        };
        let html = render_post_detail_page(&params).into_string();

        // Check that user-specific navigation is rendered
        assert!(html.contains("/profile"));
    }
}
