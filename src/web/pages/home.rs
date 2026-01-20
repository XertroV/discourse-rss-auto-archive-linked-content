//! Home page rendering using maud templates.
//!
//! This module provides the home page and related archive listing pages
//! (Recent, All, Failed) using maud for HTML generation.

use maud::{html, Markup, Render};
use urlencoding::encode;

use crate::components::{
    archive_list_tabs, ArchiveGrid, ArchiveTab, BaseLayout, EmptyState, OpenGraphMetadata,
    Pagination,
};
use crate::db::{ArchiveDisplay, User};

/// Which archive tab is currently active.
///
/// This enum represents the three main archive listing views:
/// - Recent: Shows only successfully completed archives
/// - All: Shows all archives regardless of status
/// - Failed: Shows only failed archives
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecentArchivesTab {
    /// Recent successful archives (home page)
    Recent,
    /// All archives
    All,
    /// Failed archives only
    Failed,
}

impl RecentArchivesTab {
    /// Convert to the component's `ArchiveTab` enum.
    #[must_use]
    pub fn to_archive_tab(self) -> ArchiveTab {
        match self {
            Self::Recent => ArchiveTab::Recent,
            Self::All => ArchiveTab::All,
            Self::Failed => ArchiveTab::Failed,
        }
    }

    /// Get the base URL for this tab.
    #[must_use]
    pub fn base_url(&self) -> &'static str {
        match self {
            Self::Recent => "/",
            Self::All => "/archives/all",
            Self::Failed => "/archives/failed",
        }
    }

    /// Get the page title for this tab.
    #[must_use]
    pub fn page_title(&self) -> &'static str {
        match self {
            Self::Recent => "Home",
            Self::All => "All Archives",
            Self::Failed => "Failed Archives",
        }
    }

    /// Get the heading for this tab.
    #[must_use]
    pub fn heading(&self) -> &'static str {
        match self {
            Self::Recent => "Recent Archives",
            Self::All => "All Recent Archives",
            Self::Failed => "Recent Failed Archives",
        }
    }
}

/// Content type filter options for archive listings.
#[derive(Debug, Clone)]
pub struct ContentTypeFilter<'a> {
    base_url: &'a str,
    active_filter: Option<&'a str>,
    source_filter: Option<&'a str>,
}

impl<'a> ContentTypeFilter<'a> {
    /// Create a new content type filter.
    #[must_use]
    pub fn new(base_url: &'a str) -> Self {
        Self {
            base_url,
            active_filter: None,
            source_filter: None,
        }
    }

    /// Set the currently active content type filter.
    #[must_use]
    pub fn with_active(mut self, filter: Option<&'a str>) -> Self {
        self.active_filter = filter;
        self
    }

    /// Set the source filter to preserve in URLs.
    #[must_use]
    pub fn with_source_filter(mut self, filter: Option<&'a str>) -> Self {
        self.source_filter = filter;
        self
    }

    /// Build a URL for a specific content type filter.
    fn build_url(&self, type_value: Option<&str>) -> String {
        match (type_value, self.source_filter) {
            (Some(t), Some(s)) => {
                format!("{}?type={}&source={}", self.base_url, encode(t), encode(s))
            }
            (Some(t), None) => format!("{}?type={}", self.base_url, encode(t)),
            (None, Some(s)) => format!("{}?source={}", self.base_url, encode(s)),
            (None, None) => self.base_url.to_string(),
        }
    }
}

/// Available content types for filtering.
const CONTENT_TYPES: &[(&str, Option<&str>)] = &[
    ("All", None),
    ("Video", Some("video")),
    ("Image", Some("image")),
    ("Gallery", Some("gallery")),
    ("Text", Some("text")),
    ("Thread", Some("thread")),
    ("Playlist", Some("playlist")),
];

impl Render for ContentTypeFilter<'_> {
    fn render(&self) -> Markup {
        html! {
            div class="filter-section" {
                h3 { "Content Type" }
                div class="filter-buttons" {
                    @for (label, type_value) in CONTENT_TYPES {
                        @let is_active = match (self.active_filter, type_value) {
                            (None, None) => true,
                            (Some(a), Some(t)) if a == *t => true,
                            _ => false,
                        };
                        @let class = if is_active { "filter-btn active" } else { "filter-btn" };
                        a href=(self.build_url(*type_value)) class=(class) { (label) }
                    }
                }
            }
        }
    }
}

/// Source filter options for archive listings.
#[derive(Debug, Clone)]
pub struct SourceFilter<'a> {
    base_url: &'a str,
    active_filter: Option<&'a str>,
    content_type_filter: Option<&'a str>,
}

impl<'a> SourceFilter<'a> {
    /// Create a new source filter.
    #[must_use]
    pub fn new(base_url: &'a str) -> Self {
        Self {
            base_url,
            active_filter: None,
            content_type_filter: None,
        }
    }

    /// Set the currently active source filter.
    #[must_use]
    pub fn with_active(mut self, filter: Option<&'a str>) -> Self {
        self.active_filter = filter;
        self
    }

    /// Set the content type filter to preserve in URLs.
    #[must_use]
    pub fn with_content_type_filter(mut self, filter: Option<&'a str>) -> Self {
        self.content_type_filter = filter;
        self
    }

    /// Build a URL for a specific source filter.
    fn build_url(&self, source_value: Option<&str>) -> String {
        match (source_value, self.content_type_filter) {
            (Some(s), Some(ct)) => {
                format!("{}?source={}&type={}", self.base_url, encode(s), encode(ct))
            }
            (Some(s), None) => format!("{}?source={}", self.base_url, encode(s)),
            (None, Some(ct)) => format!("{}?type={}", self.base_url, encode(ct)),
            (None, None) => self.base_url.to_string(),
        }
    }
}

/// Available sources for filtering.
const SOURCES: &[(&str, Option<&str>)] = &[
    ("All", None),
    ("Reddit", Some("reddit")),
    ("YouTube", Some("youtube")),
    ("TikTok", Some("tiktok")),
    ("Twitter/X", Some("twitter")),
];

impl Render for SourceFilter<'_> {
    fn render(&self) -> Markup {
        html! {
            div class="filter-section" {
                h3 { "Source" }
                div class="filter-buttons" {
                    @for (label, source_value) in SOURCES {
                        @let is_active = match (self.active_filter, source_value) {
                            (None, None) => true,
                            (Some(a), Some(s)) if a == *s => true,
                            _ => false,
                        };
                        @let class = if is_active { "filter-btn active" } else { "filter-btn" };
                        a href=(self.build_url(*source_value)) class=(class) { (label) }
                    }
                }
            }
        }
    }
}

/// Parameters for rendering the home page.
#[derive(Debug, Clone)]
pub struct HomePageParams<'a> {
    /// Archives to display
    pub archives: &'a [ArchiveDisplay],
    /// Which tab is active
    pub active_tab: RecentArchivesTab,
    /// Number of recent failed archives (for badge)
    pub recent_failed_count: usize,
    /// Current page (0-indexed)
    pub page: usize,
    /// Total number of pages
    pub total_pages: usize,
    /// Active content type filter
    pub content_type_filter: Option<&'a str>,
    /// Active source filter
    pub source_filter: Option<&'a str>,
    /// Current user (for auth-aware navigation)
    pub user: Option<&'a User>,
    /// Optional Open Graph metadata for social media previews
    pub og_metadata: Option<OpenGraphMetadata>,
}

impl<'a> HomePageParams<'a> {
    /// Create parameters for a simple page without pagination.
    #[must_use]
    pub fn simple(
        archives: &'a [ArchiveDisplay],
        active_tab: RecentArchivesTab,
        recent_failed_count: usize,
    ) -> Self {
        Self {
            archives,
            active_tab,
            recent_failed_count,
            page: 0,
            total_pages: 1,
            content_type_filter: None,
            source_filter: None,
            user: None,
            og_metadata: None,
        }
    }

    /// Create parameters for a paginated page.
    #[must_use]
    pub fn paginated(
        archives: &'a [ArchiveDisplay],
        active_tab: RecentArchivesTab,
        recent_failed_count: usize,
        page: usize,
        total_pages: usize,
    ) -> Self {
        Self {
            archives,
            active_tab,
            recent_failed_count,
            page,
            total_pages,
            content_type_filter: None,
            source_filter: None,
            user: None,
            og_metadata: None,
        }
    }

    /// Set the content type filter.
    #[must_use]
    pub fn with_content_type_filter(mut self, filter: Option<&'a str>) -> Self {
        self.content_type_filter = filter;
        self
    }

    /// Set the source filter.
    #[must_use]
    pub fn with_source_filter(mut self, filter: Option<&'a str>) -> Self {
        self.source_filter = filter;
        self
    }

    /// Set the current user.
    #[must_use]
    pub fn with_user(mut self, user: Option<&'a User>) -> Self {
        self.user = user;
        self
    }

    /// Set the Open Graph metadata.
    #[must_use]
    pub fn with_og_metadata(mut self, og: OpenGraphMetadata) -> Self {
        self.og_metadata = Some(og);
        self
    }
}

/// Render the home page (or archive listing page).
///
/// This function renders the main archive listing pages with:
/// - Tabs for switching between Recent/All/Failed views
/// - Content type filter buttons
/// - Source filter buttons
/// - Archive grid showing the archives
/// - Pagination controls (if more than one page)
///
/// # Arguments
///
/// * `params` - Page parameters including archives, filters, and pagination info
///
/// # Example
///
/// ```ignore
/// use crate::web::pages::home::{render_home_page, HomePageParams, RecentArchivesTab};
///
/// let params = HomePageParams::paginated(&archives, RecentArchivesTab::Recent, 5, 0, 10)
///     .with_content_type_filter(Some("video"))
///     .with_user(Some(&user));
///
/// let html = render_home_page(&params);
/// ```
#[must_use]
pub fn render_home_page(params: &HomePageParams) -> Markup {
    let base_url = params.active_tab.base_url();
    let heading = params.active_tab.heading();
    let page_title = params.active_tab.page_title();

    // Build the tabs component
    let tabs = archive_list_tabs(
        params.active_tab.to_archive_tab(),
        params.recent_failed_count,
    );

    // Build the content type filter
    let content_filter = ContentTypeFilter::new(base_url)
        .with_active(params.content_type_filter)
        .with_source_filter(params.source_filter);

    // Build the source filter
    let source_filter = SourceFilter::new(base_url)
        .with_active(params.source_filter)
        .with_content_type_filter(params.content_type_filter);

    // Build the pagination
    let pagination = Pagination::new(params.page, params.total_pages, base_url)
        .with_content_type_filter(params.content_type_filter)
        .with_source_filter(params.source_filter);

    // Build the main content
    let content = html! {
        h1 { (heading) }
        (tabs)

        // Show filters on paginated pages
        @if params.total_pages > 0 {
            div class="filters-container" {
                (content_filter)
                (source_filter)
            }
        }

        @if params.archives.is_empty() {
            (EmptyState::no_archives())
        } @else {
            (ArchiveGrid::new(params.archives))

            // Show pagination if needed
            @if pagination.should_display() {
                (pagination)
            }
        }
    };

    let mut layout = BaseLayout::new(page_title).with_user(params.user);

    if let Some(ref og) = params.og_metadata {
        layout = layout.with_og_metadata(og.clone());
    }

    layout.render(content)
}

/// Render the recent archives home page (simple version without pagination).
#[must_use]
pub fn render_home(archives: &[ArchiveDisplay], recent_failed_count: usize) -> Markup {
    let params = HomePageParams::simple(archives, RecentArchivesTab::Recent, recent_failed_count);
    render_home_page(&params)
}

/// Render the recent archives home page with pagination.
#[must_use]
pub fn render_home_paginated(
    archives: &[ArchiveDisplay],
    recent_failed_count: usize,
    page: usize,
    total_pages: usize,
    content_type_filter: Option<&str>,
    source_filter: Option<&str>,
    user: Option<&User>,
    og_metadata: Option<OpenGraphMetadata>,
) -> Markup {
    let mut params = HomePageParams::paginated(
        archives,
        RecentArchivesTab::Recent,
        recent_failed_count,
        page,
        total_pages,
    )
    .with_content_type_filter(content_type_filter)
    .with_source_filter(source_filter)
    .with_user(user);

    if let Some(og) = og_metadata {
        params = params.with_og_metadata(og);
    }

    render_home_page(&params)
}

/// Render the failed archives page (simple version without pagination).
#[must_use]
pub fn render_recent_failed_archives(
    archives: &[ArchiveDisplay],
    recent_failed_count: usize,
) -> Markup {
    let params = HomePageParams::simple(archives, RecentArchivesTab::Failed, recent_failed_count);
    render_home_page(&params)
}

/// Render the failed archives page with pagination.
#[must_use]
pub fn render_recent_failed_archives_paginated(
    archives: &[ArchiveDisplay],
    recent_failed_count: usize,
    page: usize,
    total_pages: usize,
    content_type_filter: Option<&str>,
    source_filter: Option<&str>,
    user: Option<&User>,
) -> Markup {
    let params = HomePageParams::paginated(
        archives,
        RecentArchivesTab::Failed,
        recent_failed_count,
        page,
        total_pages,
    )
    .with_content_type_filter(content_type_filter)
    .with_source_filter(source_filter)
    .with_user(user);

    render_home_page(&params)
}

/// Render the all archives page (simple version without pagination).
#[must_use]
pub fn render_recent_all_archives(
    archives: &[ArchiveDisplay],
    recent_failed_count: usize,
) -> Markup {
    let params = HomePageParams::simple(archives, RecentArchivesTab::All, recent_failed_count);
    render_home_page(&params)
}

/// Render the all archives page with pagination.
#[must_use]
pub fn render_recent_all_archives_paginated(
    archives: &[ArchiveDisplay],
    recent_failed_count: usize,
    page: usize,
    total_pages: usize,
    content_type_filter: Option<&str>,
    source_filter: Option<&str>,
    user: Option<&User>,
) -> Markup {
    let params = HomePageParams::paginated(
        archives,
        RecentArchivesTab::All,
        recent_failed_count,
        page,
        total_pages,
    )
    .with_content_type_filter(content_type_filter)
    .with_source_filter(source_filter)
    .with_user(user);

    render_home_page(&params)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_archive() -> ArchiveDisplay {
        ArchiveDisplay {
            id: 1,
            link_id: 1,
            status: "complete".to_string(),
            archived_at: Some("2024-01-15 12:00:00".to_string()),
            content_title: Some("Test Video".to_string()),
            content_author: Some("testuser".to_string()),
            content_type: Some("video".to_string()),
            is_nsfw: false,
            error_message: None,
            retry_count: 0,
            original_url: "https://example.com/video/123".to_string(),
            domain: "example.com".to_string(),
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
    fn test_recent_archives_tab_base_url() {
        assert_eq!(RecentArchivesTab::Recent.base_url(), "/");
        assert_eq!(RecentArchivesTab::All.base_url(), "/archives/all");
        assert_eq!(RecentArchivesTab::Failed.base_url(), "/archives/failed");
    }

    #[test]
    fn test_recent_archives_tab_page_title() {
        assert_eq!(RecentArchivesTab::Recent.page_title(), "Home");
        assert_eq!(RecentArchivesTab::All.page_title(), "All Archives");
        assert_eq!(RecentArchivesTab::Failed.page_title(), "Failed Archives");
    }

    #[test]
    fn test_recent_archives_tab_heading() {
        assert_eq!(RecentArchivesTab::Recent.heading(), "Recent Archives");
        assert_eq!(RecentArchivesTab::All.heading(), "All Recent Archives");
        assert_eq!(
            RecentArchivesTab::Failed.heading(),
            "Recent Failed Archives"
        );
    }

    #[test]
    fn test_content_type_filter_build_url_no_filters() {
        let filter = ContentTypeFilter::new("/");
        assert_eq!(filter.build_url(None), "/");
    }

    #[test]
    fn test_content_type_filter_build_url_with_type() {
        let filter = ContentTypeFilter::new("/");
        assert_eq!(filter.build_url(Some("video")), "/?type=video");
    }

    #[test]
    fn test_content_type_filter_build_url_preserves_source() {
        let filter = ContentTypeFilter::new("/").with_source_filter(Some("reddit"));
        assert_eq!(
            filter.build_url(Some("video")),
            "/?type=video&source=reddit"
        );
        assert_eq!(filter.build_url(None), "/?source=reddit");
    }

    #[test]
    fn test_source_filter_build_url_no_filters() {
        let filter = SourceFilter::new("/");
        assert_eq!(filter.build_url(None), "/");
    }

    #[test]
    fn test_source_filter_build_url_with_source() {
        let filter = SourceFilter::new("/");
        assert_eq!(filter.build_url(Some("reddit")), "/?source=reddit");
    }

    #[test]
    fn test_source_filter_build_url_preserves_content_type() {
        let filter = SourceFilter::new("/").with_content_type_filter(Some("video"));
        assert_eq!(
            filter.build_url(Some("reddit")),
            "/?source=reddit&type=video"
        );
        assert_eq!(filter.build_url(None), "/?type=video");
    }

    #[test]
    fn test_content_type_filter_render() {
        let filter = ContentTypeFilter::new("/").with_active(Some("video"));
        let html = filter.render().into_string();

        assert!(html.contains("filter-section"));
        assert!(html.contains("Content Type"));
        assert!(html.contains("filter-btn"));
        // Video should be active
        assert!(html.contains("filter-btn active"));
    }

    #[test]
    fn test_source_filter_render() {
        let filter = SourceFilter::new("/").with_active(Some("reddit"));
        let html = filter.render().into_string();

        assert!(html.contains("filter-section"));
        assert!(html.contains("Source"));
        assert!(html.contains("Reddit"));
        assert!(html.contains("YouTube"));
        assert!(html.contains("TikTok"));
        assert!(html.contains("filter-btn active"));
    }

    #[test]
    fn test_home_page_params_simple() {
        let archives = vec![sample_archive()];
        let params = HomePageParams::simple(&archives, RecentArchivesTab::Recent, 5);

        assert_eq!(params.page, 0);
        assert_eq!(params.total_pages, 1);
        assert_eq!(params.recent_failed_count, 5);
        assert!(params.content_type_filter.is_none());
        assert!(params.source_filter.is_none());
        assert!(params.user.is_none());
    }

    #[test]
    fn test_home_page_params_paginated() {
        let archives = vec![sample_archive()];
        let params = HomePageParams::paginated(&archives, RecentArchivesTab::All, 3, 2, 10);

        assert_eq!(params.page, 2);
        assert_eq!(params.total_pages, 10);
        assert_eq!(params.recent_failed_count, 3);
    }

    #[test]
    fn test_home_page_params_with_filters() {
        let archives = vec![sample_archive()];
        let user = sample_user();
        let params = HomePageParams::simple(&archives, RecentArchivesTab::Recent, 0)
            .with_content_type_filter(Some("video"))
            .with_source_filter(Some("reddit"))
            .with_user(Some(&user));

        assert_eq!(params.content_type_filter, Some("video"));
        assert_eq!(params.source_filter, Some("reddit"));
        assert!(params.user.is_some());
    }

    #[test]
    fn test_render_home_page_basic() {
        let archives = vec![sample_archive()];
        let params = HomePageParams::simple(&archives, RecentArchivesTab::Recent, 0);
        let html = render_home_page(&params).into_string();

        // Check page structure
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<title>Home - Discourse Link Archiver</title>"));
        assert!(html.contains("<h1>Recent Archives</h1>"));

        // Check tabs
        assert!(html.contains("archive-tabs"));
        assert!(html.contains("Recent"));
        assert!(html.contains("All"));
        assert!(html.contains("Failed"));

        // Check archive grid
        assert!(html.contains("archive-grid"));
        assert!(html.contains("Test Video"));
    }

    #[test]
    fn test_render_home_page_empty() {
        let archives: Vec<ArchiveDisplay> = vec![];
        let params = HomePageParams::simple(&archives, RecentArchivesTab::Recent, 0);
        let html = render_home_page(&params).into_string();

        // Should show empty state
        assert!(html.contains("No archives yet."));
        assert!(!html.contains("archive-grid"));
    }

    #[test]
    fn test_render_home_page_with_pagination() {
        let archives = vec![sample_archive()];
        let params = HomePageParams::paginated(&archives, RecentArchivesTab::Recent, 0, 5, 10);
        let html = render_home_page(&params).into_string();

        // Should show pagination
        assert!(html.contains("pagination"));
        assert!(html.contains("Previous"));
        assert!(html.contains("Next"));
    }

    #[test]
    fn test_render_home_page_with_failed_count() {
        let archives = vec![sample_archive()];
        let params = HomePageParams::simple(&archives, RecentArchivesTab::Recent, 7);
        let html = render_home_page(&params).into_string();

        // Should show failed count badge
        assert!(html.contains("archive-tab-count"));
        assert!(html.contains(">7<"));
    }

    #[test]
    fn test_render_home_page_failed_tab() {
        let archives = vec![sample_archive()];
        let params = HomePageParams::simple(&archives, RecentArchivesTab::Failed, 1);
        let html = render_home_page(&params).into_string();

        assert!(html.contains("<title>Failed Archives - Discourse Link Archiver</title>"));
        assert!(html.contains("<h1>Recent Failed Archives</h1>"));
    }

    #[test]
    fn test_render_home_page_all_tab() {
        let archives = vec![sample_archive()];
        let params = HomePageParams::simple(&archives, RecentArchivesTab::All, 0);
        let html = render_home_page(&params).into_string();

        assert!(html.contains("<title>All Archives - Discourse Link Archiver</title>"));
        assert!(html.contains("<h1>All Recent Archives</h1>"));
    }

    #[test]
    fn test_render_home_page_with_user() {
        let archives = vec![sample_archive()];
        let user = sample_user();
        let params =
            HomePageParams::simple(&archives, RecentArchivesTab::Recent, 0).with_user(Some(&user));
        let html = render_home_page(&params).into_string();

        // Should show profile link for logged-in user
        assert!(html.contains(r#"<a href="/profile">Profile</a>"#));
        // Should not show login link
        assert!(!html.contains(r#"<a href="/login">Login</a>"#));
    }

    #[test]
    fn test_render_home_convenience() {
        let archives = vec![sample_archive()];
        let html = render_home(&archives, 3).into_string();

        assert!(html.contains("<h1>Recent Archives</h1>"));
        assert!(html.contains("archive-tab-count"));
    }

    #[test]
    fn test_render_home_paginated_convenience() {
        let archives = vec![sample_archive()];
        let html = render_home_paginated(
            &archives,
            0,
            2,
            5,
            Some("video"),
            Some("reddit"),
            None,
            None,
        )
        .into_string();

        assert!(html.contains("<h1>Recent Archives</h1>"));
        assert!(html.contains("pagination"));
        // Should show filters
        assert!(html.contains("filter-section"));
    }

    #[test]
    fn test_render_recent_failed_archives() {
        let archives = vec![sample_archive()];
        let html = render_recent_failed_archives(&archives, 5).into_string();

        assert!(html.contains("<h1>Recent Failed Archives</h1>"));
    }

    #[test]
    fn test_render_recent_failed_archives_paginated() {
        let archives = vec![sample_archive()];
        let html = render_recent_failed_archives_paginated(&archives, 3, 0, 2, None, None, None)
            .into_string();

        assert!(html.contains("<h1>Recent Failed Archives</h1>"));
        assert!(html.contains("pagination"));
    }

    #[test]
    fn test_render_recent_all_archives() {
        let archives = vec![sample_archive()];
        let html = render_recent_all_archives(&archives, 0).into_string();

        assert!(html.contains("<h1>All Recent Archives</h1>"));
    }

    #[test]
    fn test_render_recent_all_archives_paginated() {
        let archives = vec![sample_archive()];
        let html =
            render_recent_all_archives_paginated(&archives, 0, 1, 3, Some("image"), None, None)
                .into_string();

        assert!(html.contains("<h1>All Recent Archives</h1>"));
        assert!(html.contains("pagination"));
    }

    #[test]
    fn test_filters_shown_on_paginated_pages() {
        let archives = vec![sample_archive()];
        let params = HomePageParams::paginated(&archives, RecentArchivesTab::Recent, 0, 0, 5);
        let html = render_home_page(&params).into_string();

        // Filters should be shown
        assert!(html.contains("Content Type"));
        assert!(html.contains("Source"));
    }

    #[test]
    fn test_url_encoding_in_filters() {
        let filter = ContentTypeFilter::new("/archives/all").with_source_filter(Some("twitter"));

        let url = filter.build_url(Some("video"));
        assert!(url.contains("/archives/all?"));
        assert!(url.contains("type=video"));
        assert!(url.contains("source=twitter"));
    }
}
