//! Debug queue page rendering using maud templates.
//!
//! This module provides the debug queue page which displays archive queue
//! statistics, recent failures, and administrative actions.

use maud::{html, Markup, Render};

use crate::components::{BaseLayout, KeyValueTable, StatusBadge, Table, TableRow, TableVariant};
use crate::db::{Archive, QueueStats, User};

/// Parameters for rendering the debug queue page.
#[derive(Debug)]
pub struct DebugQueueParams<'a> {
    /// Queue statistics.
    pub stats: &'a QueueStats,
    /// Recent failed/skipped archives.
    pub recent_failures: &'a [Archive],
    /// Currently logged in user (for header navigation).
    pub user: Option<&'a User>,
    /// CSRF token for form submissions.
    pub csrf_token: Option<&'a str>,
}

impl<'a> DebugQueueParams<'a> {
    /// Create new debug queue parameters.
    #[must_use]
    pub fn new(stats: &'a QueueStats, recent_failures: &'a [Archive]) -> Self {
        Self {
            stats,
            recent_failures,
            user: None,
            csrf_token: None,
        }
    }

    /// Set the current user.
    #[must_use]
    pub fn with_user(mut self, user: Option<&'a User>) -> Self {
        self.user = user;
        self
    }

    /// Set the CSRF token.
    #[must_use]
    pub fn with_csrf_token(mut self, token: Option<&'a str>) -> Self {
        self.csrf_token = token;
        self
    }
}

/// Render the debug queue status page.
///
/// # Arguments
///
/// * `params` - The debug queue page parameters
///
/// # Returns
///
/// The rendered HTML markup for the debug queue page.
#[must_use]
pub fn render_debug_queue_page(params: &DebugQueueParams<'_>) -> Markup {
    let content = html! {
        h1 { "Debug: Archive Queue Status" }

        // Queue Statistics Section
        (QueueStatsSection::new(params.stats))

        // Actions Section
        (ActionsSection::new(params.stats.skipped_count, params.csrf_token))

        // Recent Failures Section
        (RecentFailuresSection::new(params.recent_failures, params.csrf_token))

        // Navigation Section
        (NavigationSection)
    };

    BaseLayout::new("Debug: Queue Status", params.user).render(content)
}

/// Queue statistics section component.
struct QueueStatsSection<'a> {
    stats: &'a QueueStats,
}

impl<'a> QueueStatsSection<'a> {
    fn new(stats: &'a QueueStats) -> Self {
        Self { stats }
    }
}

impl Render for QueueStatsSection<'_> {
    fn render(&self) -> Markup {
        let mut table = KeyValueTable::new().variant(TableVariant::Stats);

        table = table.item_markup(
            "Pending",
            html! { span class="stat-pending" { (self.stats.pending_count) } },
        );

        table = table.item_markup(
            "Processing",
            html! { span class="stat-processing" { (self.stats.processing_count) } },
        );

        table = table.item_markup(
            "Failed (awaiting retry)",
            html! { span class="stat-failed" { (self.stats.failed_awaiting_retry) } },
        );

        table = table.item_markup(
            "Failed (max retries reached)",
            html! { span class="stat-failed" { (self.stats.failed_max_retries) } },
        );

        table = table.item_markup(
            "Skipped",
            html! { span class="stat-skipped" { (self.stats.skipped_count) } },
        );

        table = table.item_markup(
            "Complete",
            html! { span class="stat-complete" { (self.stats.complete_count) } },
        );

        if let Some(ref next_retry) = self.stats.next_retry_at {
            table = table.item("Next Retry At", next_retry);
        }

        if let Some(ref oldest) = self.stats.oldest_pending_at {
            table = table.item("Oldest Pending", oldest);
        }

        html! {
            section class="queue-stats" {
                h2 { "Queue Statistics" }
                (table)
            }
        }
    }
}

/// Actions section component.
struct ActionsSection<'a> {
    skipped_count: i64,
    csrf_token: Option<&'a str>,
}

impl<'a> ActionsSection<'a> {
    fn new(skipped_count: i64, csrf_token: Option<&'a str>) -> Self {
        Self {
            skipped_count,
            csrf_token,
        }
    }
}

impl Render for ActionsSection<'_> {
    fn render(&self) -> Markup {
        html! {
            section class="queue-actions" {
                h2 { "Actions" }

                @if self.skipped_count > 0 {
                    form
                        method="post"
                        action="/debug/reset-skipped"
                        style="display: inline;"
                        onsubmit=(format!("return confirm('Reset all {} skipped archives for retry?');", self.skipped_count))
                    {
                        @if let Some(token) = self.csrf_token {
                            input type="hidden" name="csrf_token" value=(token);
                        }
                        button type="submit" class="btn btn-primary" {
                            "Reset All Skipped (" (self.skipped_count) " archives)"
                        }
                    }
                } @else {
                    p { "No skipped archives to reset." }
                }
            }
        }
    }
}

/// Recent failures section component.
struct RecentFailuresSection<'a> {
    failures: &'a [Archive],
    csrf_token: Option<&'a str>,
}

impl<'a> RecentFailuresSection<'a> {
    fn new(failures: &'a [Archive], csrf_token: Option<&'a str>) -> Self {
        Self {
            failures,
            csrf_token,
        }
    }

    /// Truncate error message to a reasonable display length.
    fn truncate_error(error: &str, max_len: usize) -> String {
        if error.len() <= max_len {
            error.to_string()
        } else {
            format!("{}...", &error[..max_len])
        }
    }
}

impl Render for RecentFailuresSection<'_> {
    fn render(&self) -> Markup {
        html! {
            section class="recent-failures" {
                h2 { "Recent Failures" }

                @if self.failures.is_empty() {
                    p { "No recent failures." }
                } @else {
                    (self.render_failures_table())
                }
            }
        }
    }
}

impl RecentFailuresSection<'_> {
    fn render_failures_table(&self) -> Markup {
        let headers = vec![
            "ID",
            "Status",
            "Retries",
            "Last Attempt",
            "Error",
            "Actions",
        ];

        let rows: Vec<Markup> = self
            .failures
            .iter()
            .map(|archive| self.render_failure_row(archive))
            .collect();

        Table::new(headers)
            .variant(TableVariant::Debug)
            .class("failures-table")
            .rows(rows)
            .render()
    }

    fn render_failure_row(&self, archive: &Archive) -> Markup {
        let status_badge =
            StatusBadge::from_status(&archive.status).with_error(archive.error_message.as_deref());

        let error_display = archive
            .error_message
            .as_deref()
            .map(|e| Self::truncate_error(e, 80))
            .unwrap_or_else(|| "\u{2014}".to_string()); // em dash

        let full_error = archive.error_message.as_deref().unwrap_or("");

        let last_attempt = archive.last_attempt_at.as_deref().unwrap_or("\u{2014}"); // em dash

        TableRow::new()
            .cell_markup(html! {
                a href=(format!("/archive/{}", archive.id)) {
                    "#" (archive.id)
                }
            })
            .cell_markup(status_badge.render())
            .cell(&archive.retry_count.to_string())
            .cell(last_attempt)
            .cell_markup(html! {
                code title=(full_error) { (error_display) }
            })
            .cell_markup(self.render_action_buttons(archive))
            .render()
    }

    fn render_action_buttons(&self, archive: &Archive) -> Markup {
        html! {
            form
                method="post"
                action=(format!("/archive/{}/rearchive", archive.id))
                style="display: inline;"
            {
                @if let Some(token) = self.csrf_token {
                    input type="hidden" name="csrf_token" value=(token);
                }
                button type="submit" class="btn btn-sm" title="Retry archive" {
                    "\u{1F504}" // Refresh symbol
                }
            }
        }
    }
}

/// Navigation section component.
struct NavigationSection;

impl Render for NavigationSection {
    fn render(&self) -> Markup {
        html! {
            section class="debug-nav" {
                p {
                    a href="/" { "\u{2190} Back to Home" }
                    " | "
                    a href="/stats" { "View Stats" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create test queue stats.
    fn test_queue_stats() -> QueueStats {
        QueueStats {
            pending_count: 5,
            processing_count: 2,
            failed_awaiting_retry: 3,
            failed_max_retries: 1,
            skipped_count: 4,
            complete_count: 100,
            next_retry_at: Some("2024-01-15 10:30:00".to_string()),
            oldest_pending_at: Some("2024-01-15 09:00:00".to_string()),
        }
    }

    /// Create a test archive.
    fn test_archive(id: i64, status: &str, error: Option<&str>) -> Archive {
        Archive {
            id,
            link_id: 1,
            status: status.to_string(),
            archived_at: None,
            content_title: Some("Test Archive".to_string()),
            content_author: None,
            content_text: None,
            content_type: Some("video".to_string()),
            s3_key_primary: None,
            s3_key_thumb: None,
            s3_keys_extra: None,
            wayback_url: None,
            archive_today_url: None,
            ipfs_cid: None,
            error_message: error.map(String::from),
            retry_count: 3,
            created_at: "2024-01-15 10:00:00".to_string(),
            is_nsfw: false,
            nsfw_source: None,
            next_retry_at: Some("2024-01-15 11:00:00".to_string()),
            last_attempt_at: Some("2024-01-15 10:30:00".to_string()),
            http_status_code: Some(404),
            post_date: None,
            quoted_archive_id: None,
            reply_to_archive_id: None,
            submitted_by_user_id: None,
            progress_percent: None,
            progress_details: None,
            last_progress_update: None,
            og_title: None,
            og_description: None,
            og_image: None,
            og_type: None,
            og_extracted_at: None,
            og_extraction_attempted: false,
            transcript_text: None,
            full_text: None,
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
    fn test_render_debug_queue_page_basic() {
        let stats = test_queue_stats();
        let failures: Vec<Archive> = vec![];
        let params = DebugQueueParams::new(&stats, &failures);

        let html = render_debug_queue_page(&params).into_string();

        // Check page structure
        assert!(html.contains("Debug: Archive Queue Status"));
        assert!(html.contains("Queue Statistics"));
        assert!(html.contains("Actions"));
        assert!(html.contains("Recent Failures"));

        // Check stats are displayed
        assert!(html.contains("Pending"));
        assert!(html.contains("5")); // pending_count
        assert!(html.contains("Processing"));
        assert!(html.contains("Complete"));
        assert!(html.contains("100")); // complete_count
    }

    #[test]
    fn test_render_debug_queue_page_with_user() {
        let stats = test_queue_stats();
        let failures: Vec<Archive> = vec![];
        let user = test_user(true);
        let params = DebugQueueParams::new(&stats, &failures).with_user(Some(&user));

        let html = render_debug_queue_page(&params).into_string();

        // Admin user should see admin link
        assert!(html.contains("/admin"));
    }

    #[test]
    fn test_render_debug_queue_page_with_failures() {
        let stats = test_queue_stats();
        let failures = vec![
            test_archive(1, "failed", Some("Connection timeout")),
            test_archive(2, "skipped", Some("Unsupported format")),
        ];
        let params = DebugQueueParams::new(&stats, &failures);

        let html = render_debug_queue_page(&params).into_string();

        // Check failures table
        assert!(html.contains("#1"));
        assert!(html.contains("#2"));
        assert!(html.contains("Connection timeout"));
        assert!(html.contains("Unsupported format"));
        assert!(html.contains("/archive/1"));
        assert!(html.contains("/archive/2"));
    }

    #[test]
    fn test_render_debug_queue_page_no_skipped() {
        let mut stats = test_queue_stats();
        stats.skipped_count = 0;
        let failures: Vec<Archive> = vec![];
        let params = DebugQueueParams::new(&stats, &failures);

        let html = render_debug_queue_page(&params).into_string();

        // Should show "no skipped" message
        assert!(html.contains("No skipped archives to reset"));
        // Should not show reset button
        assert!(!html.contains("Reset All Skipped"));
    }

    #[test]
    fn test_render_debug_queue_page_with_skipped() {
        let stats = test_queue_stats();
        let failures: Vec<Archive> = vec![];
        let params =
            DebugQueueParams::new(&stats, &failures).with_csrf_token(Some("test_csrf_token"));

        let html = render_debug_queue_page(&params).into_string();

        // Should show reset button with count
        assert!(html.contains("Reset All Skipped"));
        assert!(html.contains("4 archives"));
        assert!(html.contains("/debug/reset-skipped"));
        // Should include CSRF token
        assert!(html.contains("csrf_token"));
        assert!(html.contains("test_csrf_token"));
    }

    #[test]
    fn test_render_debug_queue_page_navigation() {
        let stats = test_queue_stats();
        let failures: Vec<Archive> = vec![];
        let params = DebugQueueParams::new(&stats, &failures);

        let html = render_debug_queue_page(&params).into_string();

        // Check navigation links
        assert!(html.contains("Back to Home"));
        assert!(html.contains("href=\"/\""));
        assert!(html.contains("View Stats"));
        assert!(html.contains("href=\"/stats\""));
    }

    #[test]
    fn test_render_debug_queue_page_status_classes() {
        let stats = test_queue_stats();
        let failures: Vec<Archive> = vec![];
        let params = DebugQueueParams::new(&stats, &failures);

        let html = render_debug_queue_page(&params).into_string();

        // Check status-specific CSS classes
        assert!(html.contains("stat-pending"));
        assert!(html.contains("stat-processing"));
        assert!(html.contains("stat-failed"));
        assert!(html.contains("stat-skipped"));
        assert!(html.contains("stat-complete"));
    }

    #[test]
    fn test_render_debug_queue_page_optional_timestamps() {
        let mut stats = test_queue_stats();
        stats.next_retry_at = None;
        stats.oldest_pending_at = None;
        let failures: Vec<Archive> = vec![];
        let params = DebugQueueParams::new(&stats, &failures);

        let html = render_debug_queue_page(&params).into_string();

        // Should not show optional fields when None
        assert!(!html.contains("Next Retry At"));
        assert!(!html.contains("Oldest Pending"));
    }

    #[test]
    fn test_render_debug_queue_page_with_timestamps() {
        let stats = test_queue_stats();
        let failures: Vec<Archive> = vec![];
        let params = DebugQueueParams::new(&stats, &failures);

        let html = render_debug_queue_page(&params).into_string();

        // Should show optional fields when present
        assert!(html.contains("Next Retry At"));
        assert!(html.contains("2024-01-15 10:30:00"));
        assert!(html.contains("Oldest Pending"));
        assert!(html.contains("2024-01-15 09:00:00"));
    }

    #[test]
    fn test_truncate_error() {
        assert_eq!(RecentFailuresSection::truncate_error("short", 80), "short");

        let long_error = "a".repeat(100);
        let truncated = RecentFailuresSection::truncate_error(&long_error, 80);
        assert_eq!(truncated.len(), 83); // 80 + "..."
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_queue_stats_section() {
        let stats = test_queue_stats();
        let section = QueueStatsSection::new(&stats);
        let html = section.render().into_string();

        assert!(html.contains("queue-stats"));
        assert!(html.contains("Queue Statistics"));
        assert!(html.contains("Pending"));
        assert!(html.contains("Processing"));
        assert!(html.contains("Failed (awaiting retry)"));
        assert!(html.contains("Failed (max retries reached)"));
        assert!(html.contains("Skipped"));
        assert!(html.contains("Complete"));
    }

    #[test]
    fn test_actions_section_with_skipped() {
        let section = ActionsSection::new(10, Some("csrf123"));
        let html = section.render().into_string();

        assert!(html.contains("queue-actions"));
        assert!(html.contains("Reset All Skipped"));
        assert!(html.contains("10 archives"));
        assert!(html.contains("csrf123"));
    }

    #[test]
    fn test_actions_section_no_skipped() {
        let section = ActionsSection::new(0, None);
        let html = section.render().into_string();

        assert!(html.contains("No skipped archives to reset"));
        assert!(!html.contains("Reset All Skipped"));
    }

    #[test]
    fn test_recent_failures_section_empty() {
        let failures: Vec<Archive> = vec![];
        let section = RecentFailuresSection::new(&failures, None);
        let html = section.render().into_string();

        assert!(html.contains("recent-failures"));
        assert!(html.contains("No recent failures"));
    }

    #[test]
    fn test_recent_failures_section_with_data() {
        let failures = vec![test_archive(42, "failed", Some("Network error"))];
        let section = RecentFailuresSection::new(&failures, Some("token123"));
        let html = section.render().into_string();

        assert!(html.contains("#42"));
        assert!(html.contains("Network error"));
        assert!(html.contains("/archive/42/rearchive"));
        assert!(html.contains("token123"));
    }

    #[test]
    fn test_navigation_section() {
        let section = NavigationSection;
        let html = section.render().into_string();

        assert!(html.contains("debug-nav"));
        assert!(html.contains("Back to Home"));
        assert!(html.contains("/stats"));
    }
}
