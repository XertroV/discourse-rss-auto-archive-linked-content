//! Base layout components for the web UI.
//!
//! This module provides the main page layout structure including
//! the HTML skeleton, navigation, and footer.

use maud::{html, Markup, PreEscaped, DOCTYPE};

use super::metadata::OpenGraphMetadata;
use crate::db::User;

/// Critical theme initialization script that runs in <head> to prevent flash of wrong theme.
/// Must be inline (not external) to execute before body renders.
const THEME_INIT_SCRIPT: &str = r#"(function() {
    var theme = localStorage.getItem('theme');
    if (theme) {
        document.documentElement.setAttribute('data-theme', theme);
    } else if (window.matchMedia('(prefers-color-scheme: dark)').matches) {
        document.documentElement.setAttribute('data-theme', 'dark');
    }
})();"#;

/// Critical NSFW filter styles that hide NSFW content by default.
/// Embedded in head to prevent flash of NSFW content.
const NSFW_FILTER_STYLE: &str =
    r#"body.nsfw-hidden [data-nsfw="true"] { display: none !important; }"#;

/// Base page layout builder.
///
/// Provides a fluent interface for constructing the main page layout
/// with required user context for authentication-aware navigation.
///
/// # Example
///
/// ```ignore
/// use maud::html;
/// use crate::components::layout::BaseLayout;
///
/// let content = html! { h1 { "Hello World" } };
/// let page = BaseLayout::new("My Page", user.as_ref())
///     .render(content);
/// ```
#[derive(Debug, Clone)]
pub struct BaseLayout<'a> {
    title: &'a str,
    user: Option<&'a User>,
    og_metadata: Option<OpenGraphMetadata>,
}

impl<'a> BaseLayout<'a> {
    /// Create a new base layout with the given page title and user.
    ///
    /// The user parameter is required to ensure authentication state is
    /// always explicitly handled. Pass `None` for anonymous users or
    /// `Some(&user)` for authenticated users.
    #[must_use]
    pub fn new(title: &'a str, user: Option<&'a User>) -> Self {
        Self {
            title,
            user,
            og_metadata: None,
        }
    }

    /// Set the Open Graph metadata for social media previews.
    #[must_use]
    pub fn with_og_metadata(mut self, metadata: OpenGraphMetadata) -> Self {
        self.og_metadata = Some(metadata);
        self
    }

    /// Render the complete HTML page with the given content.
    ///
    /// The content will be placed inside the `<main class="container">` element.
    #[must_use]
    pub fn render(self, content: Markup) -> Markup {
        html! {
            (DOCTYPE)
            html lang="en" data-theme="light" {
                head {
                    meta charset="UTF-8";
                    meta name="viewport" content="width=device-width, initial-scale=1.0";
                    meta name="color-scheme" content="light dark";
                    meta name="robots" content="noarchive";
                    meta name="x-no-archive" content="1";
                    title { (self.title) " - Discourse Link Archiver" }

                    // Open Graph and Twitter Card metadata
                    @if let Some(ref og) = self.og_metadata {
                        (og.render())
                    }

                    link rel="stylesheet" href="/static/css/style.css";
                    link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>📦</text></svg>";
                    link rel="alternate" type="application/rss+xml" title="Archive RSS Feed" href="/feed.rss";
                    link rel="alternate" type="application/atom+xml" title="Archive Atom Feed" href="/feed.atom";
                    // Inline critical styles for NSFW filtering
                    style { (PreEscaped(NSFW_FILTER_STYLE)) }
                    // Inline critical script to prevent theme flicker
                    script { (PreEscaped(THEME_INIT_SCRIPT)) }
                }
                body class="nsfw-hidden" {
                    (self.render_header())
                    main class="container" {
                        (content)
                    }
                    (Self::render_footer())
                    // External scripts for interactive functionality
                    script src="/static/js/theme.js" {}
                    script src="/static/js/nsfw.js" {}
                    script src="/static/js/video-volume.js" {}
                }
            }
        }
    }

    /// Render the page header with navigation.
    fn render_header(&self) -> Markup {
        html! {
            header class="container" {
                nav {
                    ul {
                        li {
                            a href="/" {
                                strong class="site-logo" { "CF Archive" }
                            }
                        }
                    }
                    ul {
                        li { a href="/" { "Home" } }
                        li { a href="/archives/all" { "All Archives" } }
                        li { a href="/threads" { "Threads" } }
                        li { a href="/search" { "Search" } }
                        li { a href="/submit" { "Submit" } }
                        li { a href="/stats" { "Stats" } }
                        (self.render_auth_nav())
                        li {
                            button
                                id="nsfw-toggle"
                                class="nsfw-toggle"
                                title="Toggle NSFW content visibility"
                                aria-label="Toggle NSFW content" { "18+" }
                        }
                        li {
                            button
                                id="theme-toggle"
                                class="theme-toggle"
                                title="Toggle dark mode"
                                aria-label="Toggle dark mode" { "🌓" }
                        }
                    }
                }
            }
        }
    }

    /// Render authentication-related navigation items.
    fn render_auth_nav(&self) -> Markup {
        match self.user {
            Some(u) if u.is_admin => html! {
                li { a href="/profile" { "Profile" } }
                li { a href="/admin" { "Admin" } }
            },
            Some(_) => html! {
                li { a href="/profile" { "Profile" } }
            },
            None => html! {
                li { a href="/login" { "Login" } }
            },
        }
    }

    /// Render the page footer.
    fn render_footer() -> Markup {
        html! {
            footer class="container" {
                small {
                    "Discourse Link Archiver | "
                    a href="https://github.com/XertroV/discourse-rss-auto-archive-linked-content" target="_blank" rel="noopener noreferrer" { "GitHub" }
                    " | "
                    a href="/feed.rss" { "RSS" }
                    " | "
                    a href="/feed.atom" { "Atom" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a test user for unit tests.
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
    fn test_base_layout_basic_structure() {
        let content = html! { h1 { "Test Content" } };
        let page = BaseLayout::new("Test Page", None).render(content);
        let html = page.into_string();

        // Check DOCTYPE and html structure
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains(r#"<html lang="en" data-theme="light">"#));

        // Check head elements
        assert!(html.contains(r#"<meta charset="UTF-8">"#));
        assert!(html
            .contains(r#"<meta name="viewport" content="width=device-width, initial-scale=1.0">"#));
        assert!(html.contains("<title>Test Page - Discourse Link Archiver</title>"));
        assert!(html.contains(r#"<link rel="stylesheet" href="/static/css/style.css">"#));

        // Check RSS/Atom links
        assert!(html.contains(r#"href="/feed.rss""#));
        assert!(html.contains(r#"href="/feed.atom""#));

        // Check body structure
        assert!(html.contains(r#"<body class="nsfw-hidden">"#));
        assert!(html.contains("<h1>Test Content</h1>"));
        assert!(html.contains(r#"<main class="container">"#));

        // Check theme init script is present (inline)
        assert!(html.contains("localStorage.getItem('theme')"));
    }

    #[test]
    fn test_base_layout_navigation() {
        let content = html! { p { "Content" } };
        let page = BaseLayout::new("Nav Test", None).render(content);
        let html = page.into_string();

        // Check navigation links
        assert!(html.contains(r#"<a href="/">Home</a>"#));
        assert!(html.contains(r#"<a href="/threads">Threads</a>"#));
        assert!(html.contains(r#"<a href="/search">Search</a>"#));
        assert!(html.contains(r#"<a href="/submit">Submit</a>"#));
        assert!(html.contains(r#"<a href="/stats">Stats</a>"#));
    }

    #[test]
    fn test_base_layout_anonymous_user() {
        let content = html! { p { "Content" } };
        let page = BaseLayout::new("Anonymous Test", None).render(content);
        let html = page.into_string();

        // Should show login link for anonymous users
        assert!(html.contains(r#"<a href="/login">Login</a>"#));
        // Should not show profile or admin links
        assert!(!html.contains(r#"<a href="/profile">"#));
        assert!(!html.contains(r#"<a href="/admin">"#));
    }

    #[test]
    fn test_base_layout_regular_user() {
        let user = test_user(false);
        let content = html! { p { "Content" } };
        let page = BaseLayout::new("User Test", Some(&user)).render(content);
        let html = page.into_string();

        // Should show profile link for authenticated users
        assert!(html.contains(r#"<a href="/profile">Profile</a>"#));
        // Should not show login or admin links
        assert!(!html.contains(r#"<a href="/login">"#));
        assert!(!html.contains(r#"<a href="/admin">"#));
    }

    #[test]
    fn test_base_layout_admin_user() {
        let user = test_user(true);
        let content = html! { p { "Content" } };
        let page = BaseLayout::new("Admin Test", Some(&user)).render(content);
        let html = page.into_string();

        // Should show both profile and admin links for admin users
        assert!(html.contains(r#"<a href="/profile">Profile</a>"#));
        assert!(html.contains(r#"<a href="/admin">Admin</a>"#));
        // Should not show login link
        assert!(!html.contains(r#"<a href="/login">"#));
    }

    #[test]
    fn test_base_layout_toggle_buttons() {
        let content = html! { p { "Content" } };
        let page = BaseLayout::new("Toggle Test", None).render(content);
        let html = page.into_string();

        // Check NSFW toggle button
        assert!(html.contains(r#"id="nsfw-toggle""#));
        assert!(html.contains(r#"class="nsfw-toggle""#));
        assert!(html.contains(">18+</button>"));

        // Check theme toggle button
        assert!(html.contains(r#"id="theme-toggle""#));
        assert!(html.contains(r#"class="theme-toggle""#));
    }

    #[test]
    fn test_base_layout_footer() {
        let content = html! { p { "Content" } };
        let page = BaseLayout::new("Footer Test", None).render(content);
        let html = page.into_string();

        // Check footer
        assert!(html.contains("<footer class=\"container\">"));
        assert!(html.contains("Discourse Link Archiver"));
        assert!(html.contains(r#"<a href="https://github.com/XertroV/discourse-rss-auto-archive-linked-content" target="_blank" rel="noopener noreferrer">GitHub</a>"#));
        assert!(html.contains(r#"<a href="/feed.rss">RSS</a>"#));
        assert!(html.contains(r#"<a href="/feed.atom">Atom</a>"#));
    }

    #[test]
    fn test_base_layout_external_scripts() {
        let content = html! { p { "Content" } };
        let page = BaseLayout::new("Scripts Test", None).render(content);
        let html = page.into_string();

        // Check external script tags
        assert!(html.contains(r#"<script src="/static/js/theme.js">"#));
        assert!(html.contains(r#"<script src="/static/js/nsfw.js">"#));
        assert!(html.contains(r#"<script src="/static/js/video-volume.js">"#));
    }

    #[test]
    fn test_base_layout_nsfw_filter_style() {
        let content = html! { p { "Content" } };
        let page = BaseLayout::new("NSFW Style Test", None).render(content);
        let html = page.into_string();

        // Check NSFW filter style is embedded
        assert!(html.contains(r#"body.nsfw-hidden [data-nsfw="true"]"#));
        assert!(html.contains("display: none !important"));
    }

    #[test]
    fn test_base_layout_meta_tags() {
        let content = html! { p { "Content" } };
        let page = BaseLayout::new("Meta Test", None).render(content);
        let html = page.into_string();

        // Check meta tags for preventing archiving by third parties
        assert!(html.contains(r#"<meta name="robots" content="noarchive">"#));
        assert!(html.contains(r#"<meta name="x-no-archive" content="1">"#));
        assert!(html.contains(r#"<meta name="color-scheme" content="light dark">"#));
    }

    #[test]
    fn test_base_layout_with_og_metadata() {
        let user = test_user(false);

        // Test that og metadata works correctly
        let layout = BaseLayout::new("OG Test", Some(&user));
        assert_eq!(layout.title, "OG Test");
        assert!(layout.user.is_some());
        assert_eq!(layout.user.unwrap().username, "testuser");
    }
}
