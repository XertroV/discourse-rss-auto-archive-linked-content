//! Thread page templates using maud.
//!
//! This module provides maud-based templates for thread-related pages:
//! - Thread list page
//! - Thread detail page
//! - Thread archive job status page

use chrono::NaiveDateTime;
use maud::{html, Markup, Render};
use urlencoding::encode;

use crate::components::{Alert, ArchiveGrid, BaseLayout, EmptyState, KeyValueTable};
use crate::db::{thread_key_from_url, ArchiveDisplay, Post, ThreadArchiveJob, ThreadDisplay, User};

/// Format a SQLite datetime string into a more readable format.
/// Input: "2024-01-15 12:34:56"
/// Output: "Jan 15, 2024 12:34"
fn format_datetime(datetime_str: &str) -> String {
    NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|dt| dt.format("%b %d, %Y %H:%M").to_string())
        .unwrap_or_else(|| datetime_str.to_string())
}

/// Sort option for threads list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThreadSortBy {
    /// Sort by creation date (most recent first)
    #[default]
    Created,
    /// Sort by last updated/activity
    Updated,
    /// Sort by thread name/title
    Name,
}

impl ThreadSortBy {
    /// Create from string value.
    #[must_use]
    pub fn from_str(s: &str) -> Self {
        match s {
            "updated" => Self::Updated,
            "name" => Self::Name,
            _ => Self::Created,
        }
    }

    /// Get the string value for URL parameters.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Name => "name",
        }
    }

    /// Get the display label.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Created => "Most Recent",
            Self::Updated => "Recently Updated",
            Self::Name => "Name",
        }
    }
}

/// A card component for displaying a thread summary.
#[derive(Debug, Clone)]
pub struct ThreadCard<'a> {
    pub thread: &'a ThreadDisplay,
}

impl<'a> ThreadCard<'a> {
    /// Create a new thread card.
    #[must_use]
    pub const fn new(thread: &'a ThreadDisplay) -> Self {
        Self { thread }
    }
}

impl Render for ThreadCard<'_> {
    fn render(&self) -> Markup {
        let thread = self.thread;
        let title = thread.title.as_deref().unwrap_or("Untitled Thread");
        let author = thread.author.as_deref().unwrap_or("Unknown");
        let published = thread.published_at.as_deref().unwrap_or("Unknown");
        let last_activity = thread
            .last_archived_at
            .as_deref()
            .unwrap_or("No archives yet");

        let thread_key = thread_key_from_url(&thread.discourse_url);
        let thread_key_encoded = encode(&thread_key);

        html! {
            article class="archive-card" {
                header {
                    h3 {
                        a href=(format!("/thread/{}", thread_key_encoded)) { (title) }
                    }
                }
                div {
                    p { strong { "Author:" } " " (author) }
                    p { strong { "Published:" } " " (published) }
                    p { strong { "Links:" } " " (thread.link_count) }
                    p { strong { "Archives:" } " " (thread.archive_count) }
                    p { strong { "Last Activity:" } " " (last_activity) }
                    p {
                        a href=(thread.discourse_url) target="_blank" rel="noopener" {
                            "View on Discourse \u{2192}"
                        }
                    }
                }
            }
        }
    }
}

/// A grid of thread cards.
#[derive(Debug, Clone)]
pub struct ThreadGrid<'a> {
    pub threads: &'a [ThreadDisplay],
}

impl<'a> ThreadGrid<'a> {
    /// Create a new thread grid.
    #[must_use]
    pub const fn new(threads: &'a [ThreadDisplay]) -> Self {
        Self { threads }
    }
}

impl Render for ThreadGrid<'_> {
    fn render(&self) -> Markup {
        html! {
            div class="archive-grid" {
                @for thread in self.threads {
                    (ThreadCard::new(thread))
                }
            }
        }
    }
}

/// Sort navigation component for threads list.
#[derive(Debug, Clone)]
pub struct SortNav {
    pub current_sort: ThreadSortBy,
}

impl SortNav {
    /// Create a new sort navigation.
    #[must_use]
    pub const fn new(current_sort: ThreadSortBy) -> Self {
        Self { current_sort }
    }
}

impl Render for SortNav {
    fn render(&self) -> Markup {
        let sort_options = [
            (ThreadSortBy::Created, "Most Recent"),
            (ThreadSortBy::Updated, "Recently Updated"),
            (ThreadSortBy::Name, "Name"),
        ];

        html! {
            nav class="sort-nav" style="margin-bottom: 1.5rem;" {
                span { "Sort by: " }
                @for (sort, label) in sort_options {
                    @if sort == self.current_sort {
                        strong { (label) }
                        " "
                    } @else {
                        a href=(format!("/threads?sort={}", sort.as_str())) { (label) }
                        " "
                    }
                }
            }
        }
    }
}

/// Parameters for the threads list page.
#[derive(Debug, Clone)]
pub struct ThreadsListParams<'a> {
    pub threads: &'a [ThreadDisplay],
    pub sort_by: ThreadSortBy,
    pub page: u32,
    pub user: Option<&'a User>,
}

/// Render the threads list page.
#[must_use]
pub fn render_threads_list_page(params: &ThreadsListParams<'_>) -> Markup {
    let content = html! {
        h1 { "Discourse Threads" }

        // Sort navigation
        (SortNav::new(params.sort_by))

        @if params.threads.is_empty() {
            (EmptyState::new("No threads found."))
        } @else {
            (ThreadGrid::new(params.threads))

            // Simple pagination - show next link if we have a full page
            @if params.threads.len() >= 20 {
                nav style="margin-top: 1.5rem;" {
                    a href=(format!("/threads?sort={}&page={}", params.sort_by.as_str(), params.page + 1)) {
                        "Next page"
                    }
                }
            }
        }
    };

    BaseLayout::new("Threads")
        .with_user(params.user)
        .render(content)
}

/// Parameters for the thread detail page.
#[derive(Debug, Clone)]
pub struct ThreadDetailParams<'a> {
    pub thread_key: &'a str,
    pub posts: &'a [Post],
    pub archives: &'a [ArchiveDisplay],
    pub user: Option<&'a User>,
}

/// Render the thread detail page showing archives across all posts.
#[must_use]
pub fn render_thread_detail_page(params: &ThreadDetailParams<'_>) -> Markup {
    let title = params
        .posts
        .iter()
        .find_map(|p| p.title.clone())
        .unwrap_or_else(|| "Untitled Thread".to_string());

    let author = params
        .posts
        .iter()
        .find_map(|p| p.author.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    let published = params
        .posts
        .iter()
        .filter_map(|p| p.published_at.clone())
        .min()
        .unwrap_or_else(|| "Unknown".to_string());

    let discourse_url = params
        .posts
        .first()
        .map_or("", |p| p.discourse_url.as_str());

    let last_activity = params
        .archives
        .iter()
        .filter_map(|a| a.archived_at.clone())
        .max()
        .unwrap_or_else(|| "No archives yet".to_string());

    let content = html! {
        h1 { (title) }

        article {
            header {
                p class="meta" {
                    strong { "Author:" } " " (author)
                    br;
                    strong { "Published:" } " " (published)
                    br;
                    strong { "Posts in thread:" } " " (params.posts.len())
                    br;
                    strong { "Archives found:" } " " (params.archives.len())
                    br;
                    strong { "Last activity:" } " " (last_activity)
                    br;
                    strong { "Source:" } " "
                    a href=(discourse_url) target="_blank" rel="noopener" { (discourse_url) }
                }
                p {
                    small { "Thread key: " (params.thread_key) }
                }
            }
        }

        // Posts list section
        section {
            h2 { "Posts" }
            table style="width: 100%; border-collapse: collapse;" {
                thead {
                    tr {
                        th style="text-align: left; padding: 0.5rem; border-bottom: 1px solid var(--border, #e4e4e7);" { "Title" }
                        th style="text-align: left; padding: 0.5rem; border-bottom: 1px solid var(--border, #e4e4e7);" { "Author" }
                        th style="text-align: left; padding: 0.5rem; border-bottom: 1px solid var(--border, #e4e4e7); white-space: nowrap;" { "Published" }
                    }
                }
                tbody {
                    @for post in params.posts {
                        @let post_title = post.title.as_deref().unwrap_or("Untitled Post");
                        @let post_author = post.author.as_deref().unwrap_or("Unknown");
                        @let published_at = post.published_at.as_deref().unwrap_or("Unknown");
                        @let formatted_date = if published_at != "Unknown" {
                            format_datetime(published_at)
                        } else {
                            published_at.to_string()
                        };
                        tr {
                            td style="padding: 0.5rem; border-bottom: 1px solid var(--border, #e4e4e7);" {
                                a href=(format!("/post/{}", post.guid)) { (post_title) }
                            }
                            td style="padding: 0.5rem; border-bottom: 1px solid var(--border, #e4e4e7);" {
                                (post_author)
                            }
                            td style="padding: 0.5rem; border-bottom: 1px solid var(--border, #e4e4e7); white-space: nowrap;" {
                                (formatted_date)
                            }
                        }
                    }
                }
            }
        }

        // Archives section
        section {
            h2 { "Archived Links" }

            @if params.archives.is_empty() {
                p { "No archives from this thread." }
            } @else {
                p {
                    "Found " (params.archives.len()) " archived link(s) across the thread."
                }
                (ArchiveGrid::new(params.archives))
            }
        }
    };

    BaseLayout::new(&format!("Thread: {title}"))
        .with_user(params.user)
        .render(content)
}

/// Job status variant for display styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatusVariant {
    Pending,
    Processing,
    Complete,
    Failed,
}

impl JobStatusVariant {
    /// Create from status string.
    #[must_use]
    pub fn from_str(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "processing" => Self::Processing,
            "complete" => Self::Complete,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }

    /// Get the CSS class for the status article.
    #[must_use]
    pub const fn css_class(&self) -> &'static str {
        match self {
            Self::Pending => "warning",
            Self::Processing => "info",
            Self::Complete => "success",
            Self::Failed => "error",
        }
    }

    /// Get the display label.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Processing => "Processing",
            Self::Complete => "Complete",
            Self::Failed => "Failed",
        }
    }

    /// Check if auto-refresh should be enabled.
    #[must_use]
    pub const fn should_auto_refresh(&self) -> bool {
        matches!(self, Self::Pending | Self::Processing)
    }
}

/// Progress bar component.
#[derive(Debug, Clone, Copy)]
pub struct ProgressBar {
    pub percent: u32,
}

impl ProgressBar {
    /// Create a new progress bar.
    #[must_use]
    pub const fn new(percent: u32) -> Self {
        Self { percent }
    }
}

impl Render for ProgressBar {
    fn render(&self) -> Markup {
        html! {
            div style="background: var(--muted, #f4f4f5); border-radius: var(--radius, 0.375rem); overflow: hidden; height: 1.5rem; margin-bottom: var(--spacing-md, 1rem);" {
                div style=(format!("background: var(--primary, #ec4899); height: 100%; width: {}%; transition: width 0.3s;", self.percent)) {}
            }
        }
    }
}

/// Parameters for the thread job status page.
#[derive(Debug, Clone)]
pub struct ThreadJobStatusParams<'a> {
    pub job: &'a ThreadArchiveJob,
    pub user: Option<&'a User>,
}

/// Render the thread archive job status page.
#[must_use]
pub fn render_thread_job_status_page(params: &ThreadJobStatusParams<'_>) -> Markup {
    let job = params.job;
    let status_variant = JobStatusVariant::from_str(&job.status);

    // Auto-refresh meta tag for pending/processing jobs
    let auto_refresh = if status_variant.should_auto_refresh() {
        Some(html! {
            meta http-equiv="refresh" content="5";
        })
    } else {
        None
    };

    let content = html! {
        @if let Some(refresh) = auto_refresh {
            (refresh)
        }

        h1 { "Thread Archive Job #" (job.id) }

        article class=(status_variant.css_class()) {
            p {
                strong { "Status:" } " " (status_variant.label())
            }
        }

        // Details section
        section {
            h2 { "Details" }
            (KeyValueTable::new()
                .item_markup("Thread URL", html! {
                    a href=(job.thread_url) target="_blank" { (job.thread_url) }
                })
                .item_markup("RSS URL", html! {
                    a href=(job.rss_url) target="_blank" { (job.rss_url) }
                })
                .item("Created", &job.created_at)
                .item_markup("Started", html! {
                    @if let Some(started) = &job.started_at {
                        (started)
                    } @else {
                        "-"
                    }
                })
                .item_markup("Completed", html! {
                    @if let Some(completed) = &job.completed_at {
                        (completed)
                    } @else {
                        "-"
                    }
                }))
        }

        // Progress section (for processing/complete jobs)
        @if matches!(status_variant, JobStatusVariant::Processing | JobStatusVariant::Complete) {
            section {
                h2 { "Progress" }

                @let progress_percent = job.total_posts
                    .filter(|&total| total > 0)
                    .map(|total| ((job.processed_posts * 100 / total) as u32).min(100))
                    .unwrap_or(0);
                (ProgressBar::new(progress_percent))

                @let total_display = job.total_posts.map_or("?".to_string(), |t| t.to_string());
                (KeyValueTable::new()
                    .item("Posts Processed", &format!("{} / {}", job.processed_posts, total_display))
                    .item("New Links Found", &job.new_links_found.to_string())
                    .item("Archives Created", &job.archives_created.to_string())
                    .item("Skipped Links", &job.skipped_links.to_string()))
            }
        }

        // Error section (for failed jobs)
        @if let Some(error) = &job.error_message {
            section {
                h2 { "Error" }
                (Alert::error(error))
            }
        }

        // Auto-refresh notice
        @if status_variant.should_auto_refresh() {
            p style="color: var(--foreground-muted, #71717a); font-size: 0.875rem;" {
                "This page will automatically refresh every 5 seconds."
            }
        }

        p {
            a href="/submit" { "Submit another" }
        }
    };

    BaseLayout::new(&format!("Thread Archive Job #{}", job.id))
        .with_user(params.user)
        .render(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_thread() -> ThreadDisplay {
        ThreadDisplay {
            guid: "test-guid-123".to_string(),
            title: Some("Test Thread Title".to_string()),
            author: Some("testauthor".to_string()),
            discourse_url: "https://forum.example.com/t/test-thread/123".to_string(),
            published_at: Some("2024-01-15 12:00:00".to_string()),
            link_count: 5,
            archive_count: 3,
            last_archived_at: Some("2024-01-16 10:00:00".to_string()),
        }
    }

    fn sample_post() -> Post {
        Post {
            id: 1,
            guid: "post-guid-456".to_string(),
            discourse_url: "https://forum.example.com/t/test-thread/123/1".to_string(),
            author: Some("postauthor".to_string()),
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

    fn sample_job() -> ThreadArchiveJob {
        ThreadArchiveJob {
            id: 42,
            thread_url: "https://forum.example.com/t/test/123".to_string(),
            rss_url: "https://forum.example.com/t/test/123.rss".to_string(),
            status: "processing".to_string(),
            user_id: 1,
            total_posts: Some(10),
            processed_posts: 5,
            new_links_found: 8,
            archives_created: 6,
            skipped_links: 2,
            error_message: None,
            created_at: "2024-01-15 12:00:00".to_string(),
            started_at: Some("2024-01-15 12:01:00".to_string()),
            completed_at: None,
        }
    }

    #[test]
    fn test_thread_card_render() {
        let thread = sample_thread();
        let card = ThreadCard::new(&thread);
        let html = card.render().into_string();

        assert!(html.contains("archive-card"));
        assert!(html.contains("Test Thread Title"));
        assert!(html.contains("testauthor"));
        assert!(html.contains("/thread/"));
        assert!(html.contains("View on Discourse"));
    }

    #[test]
    fn test_thread_card_untitled() {
        let mut thread = sample_thread();
        thread.title = None;
        let card = ThreadCard::new(&thread);
        let html = card.render().into_string();

        assert!(html.contains("Untitled Thread"));
    }

    #[test]
    fn test_thread_grid_render() {
        let threads = vec![sample_thread()];
        let grid = ThreadGrid::new(&threads);
        let html = grid.render().into_string();

        assert!(html.contains("archive-grid"));
        assert!(html.contains("archive-card"));
    }

    #[test]
    fn test_sort_nav_render() {
        let nav = SortNav::new(ThreadSortBy::Updated);
        let html = nav.render().into_string();

        assert!(html.contains("Sort by:"));
        assert!(html.contains("<strong>Recently Updated</strong>"));
        assert!(html.contains("href=\"/threads?sort=created\""));
        assert!(html.contains("href=\"/threads?sort=name\""));
    }

    #[test]
    fn test_threads_list_page() {
        let threads = vec![sample_thread()];
        let params = ThreadsListParams {
            threads: &threads,
            sort_by: ThreadSortBy::Created,
            page: 0,
            user: None,
        };
        let html = render_threads_list_page(&params).into_string();

        assert!(html.contains("Discourse Threads"));
        assert!(html.contains("Test Thread Title"));
        assert!(html.contains("archive-grid"));
    }

    #[test]
    fn test_threads_list_page_empty() {
        let threads: Vec<ThreadDisplay> = vec![];
        let params = ThreadsListParams {
            threads: &threads,
            sort_by: ThreadSortBy::Created,
            page: 0,
            user: None,
        };
        let html = render_threads_list_page(&params).into_string();

        assert!(html.contains("No threads found."));
    }

    #[test]
    fn test_threads_list_page_pagination() {
        // Create 20 threads to trigger pagination
        let threads: Vec<ThreadDisplay> = (0..20)
            .map(|i| {
                let mut t = sample_thread();
                t.guid = format!("guid-{i}");
                t
            })
            .collect();
        let params = ThreadsListParams {
            threads: &threads,
            sort_by: ThreadSortBy::Created,
            page: 0,
            user: None,
        };
        let html = render_threads_list_page(&params).into_string();

        assert!(html.contains("Next page"));
        assert!(html.contains("page=1"));
    }

    #[test]
    fn test_thread_detail_page() {
        let posts = vec![sample_post()];
        let archives = vec![sample_archive()];
        let params = ThreadDetailParams {
            thread_key: "forum.example.com:123",
            posts: &posts,
            archives: &archives,
            user: None,
        };
        let html = render_thread_detail_page(&params).into_string();

        assert!(html.contains("Test Post Title"));
        assert!(html.contains("Posts in thread:"));
        assert!(html.contains("Archives found:"));
        assert!(html.contains("Archived Links"));
        assert!(html.contains("Archived Content"));
        // Check for table structure
        assert!(html.contains("<table"));
        assert!(html.contains("<thead>"));
        assert!(html.contains("<tbody>"));
        assert!(html.contains("Published")); // Column header
    }

    #[test]
    fn test_thread_detail_page_empty_archives() {
        let posts = vec![sample_post()];
        let archives: Vec<ArchiveDisplay> = vec![];
        let params = ThreadDetailParams {
            thread_key: "forum.example.com:123",
            posts: &posts,
            archives: &archives,
            user: None,
        };
        let html = render_thread_detail_page(&params).into_string();

        assert!(html.contains("No archives from this thread."));
    }

    #[test]
    fn test_job_status_page_processing() {
        let job = sample_job();
        let params = ThreadJobStatusParams {
            job: &job,
            user: None,
        };
        let html = render_thread_job_status_page(&params).into_string();

        assert!(html.contains("Thread Archive Job #42"));
        assert!(html.contains("Processing"));
        assert!(html.contains("Progress"));
        assert!(html.contains("5 / 10")); // processed / total
        assert!(html.contains("refresh")); // auto-refresh meta tag
    }

    #[test]
    fn test_job_status_page_complete() {
        let mut job = sample_job();
        job.status = "complete".to_string();
        job.completed_at = Some("2024-01-15 12:10:00".to_string());
        let params = ThreadJobStatusParams {
            job: &job,
            user: None,
        };
        let html = render_thread_job_status_page(&params).into_string();

        assert!(html.contains("Complete"));
        assert!(!html.contains("http-equiv=\"refresh\"")); // no auto-refresh for complete
    }

    #[test]
    fn test_job_status_page_failed() {
        let mut job = sample_job();
        job.status = "failed".to_string();
        job.error_message = Some("Connection timeout".to_string());
        let params = ThreadJobStatusParams {
            job: &job,
            user: None,
        };
        let html = render_thread_job_status_page(&params).into_string();

        assert!(html.contains("Failed"));
        assert!(html.contains("Error"));
        assert!(html.contains("Connection timeout"));
    }

    #[test]
    fn test_job_status_variant() {
        assert_eq!(
            JobStatusVariant::from_str("pending"),
            JobStatusVariant::Pending
        );
        assert_eq!(
            JobStatusVariant::from_str("processing"),
            JobStatusVariant::Processing
        );
        assert_eq!(
            JobStatusVariant::from_str("complete"),
            JobStatusVariant::Complete
        );
        assert_eq!(
            JobStatusVariant::from_str("failed"),
            JobStatusVariant::Failed
        );
        assert_eq!(
            JobStatusVariant::from_str("unknown"),
            JobStatusVariant::Pending
        );
    }

    #[test]
    fn test_thread_sort_by() {
        assert_eq!(ThreadSortBy::from_str("created"), ThreadSortBy::Created);
        assert_eq!(ThreadSortBy::from_str("updated"), ThreadSortBy::Updated);
        assert_eq!(ThreadSortBy::from_str("name"), ThreadSortBy::Name);
        assert_eq!(ThreadSortBy::from_str("invalid"), ThreadSortBy::Created);

        assert_eq!(ThreadSortBy::Created.as_str(), "created");
        assert_eq!(ThreadSortBy::Updated.label(), "Recently Updated");
    }

    #[test]
    fn test_progress_bar() {
        let bar = ProgressBar::new(50);
        let html = bar.render().into_string();

        assert!(html.contains("width: 50%"));
    }

    #[test]
    fn test_format_datetime() {
        assert_eq!(format_datetime("2024-01-15 12:34:56"), "Jan 15, 2024 12:34");
        assert_eq!(format_datetime("2023-12-31 23:59:59"), "Dec 31, 2023 23:59");
        // Invalid format should return original string
        assert_eq!(format_datetime("invalid"), "invalid");
        assert_eq!(format_datetime("Unknown"), "Unknown");
    }
}
