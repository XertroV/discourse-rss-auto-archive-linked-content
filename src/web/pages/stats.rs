//! Statistics page module.
//!
//! This module provides the maud-based stats page rendering.

use maud::{html, Markup, Render};

use crate::components::{BaseLayout, StatsCard, StatsCardGrid, Table, TableRow, TableVariant};
use crate::db::{User, UserSubmissionDetail};

/// Data for the statistics page.
#[derive(Debug, Clone)]
pub struct StatsData {
    /// Total number of posts
    pub post_count: i64,
    /// Total number of links
    pub link_count: i64,
    /// Archive counts by status (status name, count)
    pub status_counts: Vec<(String, i64)>,
    /// Archive counts by content type (content type, count)
    pub content_type_counts: Vec<(String, i64)>,
    /// Top domains by archive count (domain, count)
    pub top_domains: Vec<(String, i64)>,
    /// Recent activity counts (24h, 7d, 30d)
    pub recent_activity: (i64, i64, i64),
    /// Storage stats (total_bytes, avg_bytes, max_bytes)
    pub storage_stats: (i64, f64, i64),
    /// Timeline data by month (month, count)
    pub timeline: Vec<(String, i64)>,
    /// Queue stats (pending, processing)
    pub queue_stats: (i64, i64),
    /// Quality metrics (with_video, with_complete_html, with_screenshot)
    pub quality_metrics: (i64, i64, i64),
    /// NSFW count
    pub nsfw_count: i64,
    /// Total completed archives count (for percentages)
    pub total_complete: i64,
}

impl StatsData {
    /// Create new stats data.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        post_count: i64,
        link_count: i64,
        status_counts: Vec<(String, i64)>,
        content_type_counts: Vec<(String, i64)>,
        top_domains: Vec<(String, i64)>,
        recent_activity: (i64, i64, i64),
        storage_stats: (i64, f64, i64),
        timeline: Vec<(String, i64)>,
        queue_stats: (i64, i64),
        quality_metrics: (i64, i64, i64),
        nsfw_count: i64,
        total_complete: i64,
    ) -> Self {
        Self {
            post_count,
            link_count,
            status_counts,
            content_type_counts,
            top_domains,
            recent_activity,
            storage_stats,
            timeline,
            queue_stats,
            quality_metrics,
            nsfw_count,
            total_complete,
        }
    }

    /// Get the total number of archives across all statuses.
    #[must_use]
    pub fn total_archives(&self) -> i64 {
        self.status_counts.iter().map(|(_, count)| count).sum()
    }

    /// Get count for a specific status.
    #[must_use]
    pub fn count_for_status(&self, status: &str) -> i64 {
        self.status_counts
            .iter()
            .find(|(s, _)| s == status)
            .map(|(_, c)| *c)
            .unwrap_or(0)
    }
}

/// User-specific statistics.
#[derive(Debug, Clone)]
pub struct UserStats {
    /// Total submissions by user
    pub total_submissions: i64,
    /// Completed submissions
    pub complete_submissions: i64,
    /// Pending submissions
    pub pending_submissions: i64,
    /// Failed submissions
    pub failed_submissions: i64,
    /// Recent submission details
    pub recent_submissions: Vec<UserSubmissionDetail>,
}

/// Render the statistics page.
///
/// Displays comprehensive statistics about archives, including content types,
/// domains, activity, storage, and user-specific stats.
#[must_use]
pub fn render_stats_page(
    stats: &StatsData,
    user: Option<&User>,
    user_stats: Option<&UserStats>,
) -> Markup {
    // Build the stats cards
    let overview_card = StatsCard::new("Overview")
        .item("Known Forum Posts", stats.post_count.to_string())
        .item("Archived Links", stats.link_count.to_string())
        .item("Total Completed", stats.total_complete.to_string())
        .item(
            "NSFW Content",
            format!(
                "{} ({})",
                stats.nsfw_count,
                if stats.total_complete > 0 {
                    format!(
                        "{:.1}%",
                        (stats.nsfw_count as f64 / stats.total_complete as f64) * 100.0
                    )
                } else {
                    "0.0%".to_string()
                }
            ),
        );

    let activity_card = StatsCard::new("Recent Activity")
        .item(
            "Last 24 Hours",
            format!("{} archives", stats.recent_activity.0),
        )
        .item(
            "Last 7 Days",
            format!("{} archives", stats.recent_activity.1),
        )
        .item(
            "Last 30 Days",
            format!("{} archives", stats.recent_activity.2),
        );

    let queue_card = StatsCard::new("Queue Health")
        .item("Pending", format!("{} archives", stats.queue_stats.0))
        .item("Processing", format!("{} archives", stats.queue_stats.1));

    let storage_card = StatsCard::new("Storage")
        .item("Total Size", format_bytes(stats.storage_stats.0))
        .item("Average Size", format_bytes(stats.storage_stats.1 as i64))
        .item("Largest Archive", format_bytes(stats.storage_stats.2));

    let quality_card = StatsCard::new("Archive Quality")
        .item(
            "With Video Files",
            format!(
                "{}{}",
                stats.quality_metrics.0,
                if stats.total_complete > 0 {
                    format!(
                        " ({:.1}%)",
                        (stats.quality_metrics.0 as f64 / stats.total_complete as f64) * 100.0
                    )
                } else {
                    String::new()
                }
            ),
        )
        .item(
            "With Complete HTML",
            format!(
                "{}{}",
                stats.quality_metrics.1,
                if stats.total_complete > 0 {
                    format!(
                        " ({:.1}%)",
                        (stats.quality_metrics.1 as f64 / stats.total_complete as f64) * 100.0
                    )
                } else {
                    String::new()
                }
            ),
        )
        .item(
            "With Screenshots",
            format!(
                "{}{}",
                stats.quality_metrics.2,
                if stats.total_complete > 0 {
                    format!(
                        " ({:.1}%)",
                        (stats.quality_metrics.2 as f64 / stats.total_complete as f64) * 100.0
                    )
                } else {
                    String::new()
                }
            ),
        );

    let stats_grid = StatsCardGrid::new()
        .card(overview_card)
        .card(activity_card)
        .card(queue_card)
        .card(storage_card)
        .card(quality_card);

    let content = html! {
        h1 { "Statistics" }

        // Stats cards grid for the short stats sections
        (stats_grid)

        // Timeline Chart
        section class="stats-card" {
            h2 class="stats-card-title" { "Archive Timeline (Last 12 Months)" }
            div class="stats-card-content" {
                (render_timeline_chart(&stats.timeline))
            }
        }

        // Tables grid - Top Domains, Content Type, Status
        div class="stats-card-grid" {
            section class="stats-card" {
                h2 class="stats-card-title" { "Top Domains" }
                div class="stats-card-content" {
                    (render_domain_table(&stats.top_domains))
                }
            }

            section class="stats-card" {
                h2 class="stats-card-title" { "Archives by Content Type" }
                div class="stats-card-content" {
                    (render_content_type_table(&stats.content_type_counts))
                }
            }

            section class="stats-card" {
                h2 class="stats-card-title" { "Archives by Status" }
                div class="stats-card-content" {
                    (render_status_table(&stats.status_counts))
                }
            }
        }

        // User-specific stats (if logged in)
        @if let Some(user_stats) = user_stats {
            (render_user_stats_section(user_stats))
        }
    };

    BaseLayout::new("Statistics", user).render(content)
}

/// Render the content type counts table.
fn render_content_type_table(content_type_counts: &[(String, i64)]) -> Markup {
    let rows: Vec<Markup> = content_type_counts
        .iter()
        .map(|(content_type, count)| {
            TableRow::new()
                .cell(content_type)
                .cell(&count.to_string())
                .render()
        })
        .collect();

    Table::new(vec!["Content Type", "Count"])
        .variant(TableVariant::Stats)
        .rows(rows)
        .render()
}

/// Render the status counts table.
fn render_status_table(status_counts: &[(String, i64)]) -> Markup {
    let rows: Vec<Markup> = status_counts
        .iter()
        .map(|(status, count)| {
            let status_class = format!("status-{status}");
            TableRow::new()
                .cell_with_class(status, &status_class)
                .cell(&count.to_string())
                .render()
        })
        .collect();

    Table::new(vec!["Status", "Count"])
        .variant(TableVariant::Stats)
        .rows(rows)
        .render()
}

/// Render the domain counts table.
fn render_domain_table(domains: &[(String, i64)]) -> Markup {
    let rows: Vec<Markup> = domains
        .iter()
        .map(|(domain, count)| {
            TableRow::new()
                .cell(domain)
                .cell(&count.to_string())
                .render()
        })
        .collect();

    Table::new(vec!["Domain", "Count"])
        .variant(TableVariant::Stats)
        .rows(rows)
        .render()
}

/// Format bytes to human-readable format.
fn format_bytes(bytes: i64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    if bytes == 0 {
        return "0 B".to_string();
    }

    let bytes_f = bytes as f64;
    let index = (bytes_f.log10() / 1024_f64.log10()).floor() as usize;
    let index = index.min(UNITS.len() - 1);
    let value = bytes_f / 1024_f64.powi(index as i32);

    format!("{:.2} {}", value, UNITS[index])
}

/// Render timeline chart (GitHub-style activity chart).
fn render_timeline_chart(timeline: &[(String, i64)]) -> Markup {
    if timeline.is_empty() {
        return html! {
            p { "No data available" }
        };
    }

    let max_count = timeline.iter().map(|(_, count)| count).max().unwrap_or(&1);
    let max_count = (*max_count).max(1); // Avoid division by zero

    html! {
        div class="timeline-chart" {
            @for (month, count) in timeline {
                div class="timeline-bar" {
                    div class="timeline-label" { (month) }
                    div class="timeline-bar-container" {
                        div
                            class="timeline-bar-fill"
                            style=(format!("width: {}%", ((*count as f64 / max_count as f64) * 100.0) as i32))
                            {
                            (count)
                        }
                    }
                }
            }
        }
    }
}

/// Render user stats section (collapsible).
fn render_user_stats_section(user_stats: &UserStats) -> Markup {
    html! {
        section class="user-stats-section" {
            details {
                summary {
                    h2 style="display: inline;" { "Your Stats" }
                }
                div class="user-stats-content" {
                    h3 { "Submission Summary" }
                    div class="submission-stats-grid" {
                        div class="stat-item" {
                            strong { "Total Submissions:" }
                            " " (user_stats.total_submissions)
                        }
                        div class="stat-item" {
                            strong { "Completed:" }
                            " " (user_stats.complete_submissions)
                            @if user_stats.total_submissions > 0 {
                                " (" (format!("{:.1}%", (user_stats.complete_submissions as f64 / user_stats.total_submissions as f64) * 100.0)) ")"
                            }
                        }
                        div class="stat-item" {
                            strong { "Pending:" }
                            " " (user_stats.pending_submissions)
                        }
                        div class="stat-item" {
                            strong { "Failed:" }
                            " " (user_stats.failed_submissions)
                        }
                    }

                    @if !user_stats.recent_submissions.is_empty() {
                        h3 { "Recent Submissions" }
                        (render_user_submissions_table(&user_stats.recent_submissions))
                    }
                }
            }
        }
    }
}

/// Render user submissions table.
fn render_user_submissions_table(submissions: &[UserSubmissionDetail]) -> Markup {
    let rows: Vec<Markup> = submissions
        .iter()
        .map(|submission| {
            let status_class = format!("status-{}", submission.status);
            TableRow::new()
                .cell_markup(html! {
                    @if let Some(archive_id) = submission.archive_id {
                        a href=(format!("/archive/{}", archive_id)) { "#" (submission.id) }
                    } @else {
                        "#" (submission.id)
                    }
                })
                .cell_markup(html! {
                    @if let Some(link_id) = submission.link_id {
                        a href=(format!("/link/{}", link_id)) { (truncate_url(&submission.url, 60)) }
                    } @else {
                        (truncate_url(&submission.url, 60))
                    }
                })
                .cell_with_class(&submission.status, &status_class)
                .cell(&format_datetime(&submission.created_at))
                .cell_markup(html! {
                    @if let Some(error) = &submission.error_message {
                        span title=(error) class="text-muted" { "Error" }
                    } @else {
                        span { "" }
                    }
                })
                .render()
        })
        .collect();

    Table::new(vec!["ID", "URL", "Status", "Submitted", "Error"])
        .variant(TableVariant::Stats)
        .rows(rows)
        .render()
}

/// Truncate URL for display.
fn truncate_url(url: &str, max_len: usize) -> String {
    if url.len() <= max_len {
        url.to_string()
    } else {
        format!("{}...", &url[..max_len])
    }
}

/// Format datetime for display.
fn format_datetime(datetime: &str) -> String {
    // Simple formatting - just show date part for now
    datetime.split('T').next().unwrap_or(datetime).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn test_stats_data(
        post_count: i64,
        link_count: i64,
        status_counts: Vec<(String, i64)>,
        content_type_counts: Vec<(String, i64)>,
    ) -> StatsData {
        let total_complete = status_counts
            .iter()
            .find(|(s, _)| s == "complete")
            .map(|(_, c)| *c)
            .unwrap_or(0);

        StatsData::new(
            post_count,
            link_count,
            status_counts,
            content_type_counts,
            vec![],      // top_domains
            (0, 0, 0),   // recent_activity
            (0, 0.0, 0), // storage_stats
            vec![],      // timeline
            (0, 0),      // queue_stats
            (0, 0, 0),   // quality_metrics
            0,           // nsfw_count
            total_complete,
        )
    }

    #[test]
    fn test_stats_data_new() {
        let status_counts = vec![
            ("complete".to_string(), 100),
            ("pending".to_string(), 20),
            ("failed".to_string(), 5),
        ];
        let stats = test_stats_data(50, 200, status_counts, vec![]);

        assert_eq!(stats.post_count, 50);
        assert_eq!(stats.link_count, 200);
        assert_eq!(stats.status_counts.len(), 3);
    }

    #[test]
    fn test_stats_data_total_archives() {
        let status_counts = vec![
            ("complete".to_string(), 100),
            ("pending".to_string(), 20),
            ("failed".to_string(), 5),
        ];
        let stats = test_stats_data(50, 200, status_counts, vec![]);

        assert_eq!(stats.total_archives(), 125);
    }

    #[test]
    fn test_stats_data_count_for_status() {
        let status_counts = vec![
            ("complete".to_string(), 100),
            ("pending".to_string(), 20),
            ("failed".to_string(), 5),
        ];
        let stats = test_stats_data(50, 200, status_counts, vec![]);

        assert_eq!(stats.count_for_status("complete"), 100);
        assert_eq!(stats.count_for_status("pending"), 20);
        assert_eq!(stats.count_for_status("failed"), 5);
        assert_eq!(stats.count_for_status("nonexistent"), 0);
    }

    #[test]
    fn test_stats_data_empty() {
        let stats = test_stats_data(0, 0, vec![], vec![]);

        assert_eq!(stats.post_count, 0);
        assert_eq!(stats.link_count, 0);
        assert_eq!(stats.total_archives(), 0);
    }

    #[test]
    fn test_render_stats_page_basic_structure() {
        let status_counts = vec![("complete".to_string(), 100), ("pending".to_string(), 20)];
        let stats = test_stats_data(50, 200, status_counts, vec![]);
        let html = render_stats_page(&stats, None, None).into_string();

        // Check page title
        assert!(html.contains("<title>Statistics - Discourse Link Archiver</title>"));

        // Check main heading
        assert!(html.contains("<h1>Statistics</h1>"));

        // Check stats card grid structure
        assert!(html.contains("stats-card-grid"));
        assert!(html.contains("stats-card"));

        // Check overview card content
        assert!(html.contains("Overview"));
        assert!(html.contains("Known Forum Posts"));
        assert!(html.contains(">50<"));
        assert!(html.contains("Archived Links"));
        assert!(html.contains(">200<"));

        // Check other cards are present
        assert!(html.contains("Recent Activity"));
        assert!(html.contains("Queue Health"));
        assert!(html.contains("Storage"));
        assert!(html.contains("Archive Quality"));

        // Check section cards with titles
        assert!(html.contains("Archive Timeline (Last 12 Months)"));
        assert!(html.contains("Top Domains"));
        assert!(html.contains("Archives by Content Type"));
        assert!(html.contains("Archives by Status"));

        // Check for card structure
        assert!(html.contains("stats-card-title"));
    }

    #[test]
    fn test_render_stats_page_status_table() {
        let status_counts = vec![
            ("complete".to_string(), 100),
            ("pending".to_string(), 20),
            ("failed".to_string(), 5),
        ];
        let stats = test_stats_data(50, 200, status_counts, vec![]);
        let html = render_stats_page(&stats, None, None).into_string();

        // Check table headers
        assert!(html.contains("<th>Status</th>"));
        assert!(html.contains("<th>Count</th>"));

        // Check status rows with correct CSS classes
        assert!(html.contains(r#"class="status-complete"#));
        assert!(html.contains(">complete</td>"));
        assert!(html.contains(">100</td>"));

        assert!(html.contains(r#"class="status-pending"#));
        assert!(html.contains(">pending</td>"));
        assert!(html.contains(">20</td>"));

        assert!(html.contains(r#"class="status-failed"#));
        assert!(html.contains(">failed</td>"));
        assert!(html.contains(">5</td>"));

        // Check table has stats-table class
        assert!(html.contains(r#"class="stats-table"#));
    }

    #[test]
    fn test_render_stats_page_with_user() {
        let user = test_user(false);
        let stats = test_stats_data(10, 50, vec![], vec![]);
        let html = render_stats_page(&stats, Some(&user), None).into_string();

        // Should show profile link for authenticated users
        assert!(html.contains(r#"<a href="/profile">Profile</a>"#));
        // Should not show login link
        assert!(!html.contains(r#"<a href="/login">Login</a>"#));
    }

    #[test]
    fn test_render_stats_page_with_admin_user() {
        let user = test_user(true);
        let stats = test_stats_data(10, 50, vec![], vec![]);
        let html = render_stats_page(&stats, Some(&user), None).into_string();

        // Should show both profile and admin links
        assert!(html.contains(r#"<a href="/profile">Profile</a>"#));
        assert!(html.contains(r#"<a href="/admin">Admin</a>"#));
    }

    #[test]
    fn test_render_stats_page_anonymous() {
        let stats = test_stats_data(10, 50, vec![], vec![]);
        let html = render_stats_page(&stats, None, None).into_string();

        // Should show login link for anonymous users
        assert!(html.contains(r#"<a href="/login">Login</a>"#));
    }

    #[test]
    fn test_render_status_table_empty() {
        let html = render_status_table(&[]).into_string();

        // Should still have table structure even with no rows
        assert!(html.contains("<table"));
        assert!(html.contains("<thead>"));
        assert!(html.contains("<th>Status</th>"));
        assert!(html.contains("<th>Count</th>"));
        assert!(html.contains("<tbody>"));
    }

    #[test]
    fn test_render_status_table_with_data() {
        let status_counts = vec![("complete".to_string(), 42), ("error".to_string(), 3)];
        let html = render_status_table(&status_counts).into_string();

        // Check table variant
        assert!(html.contains(r#"class="stats-table"#));

        // Check rows
        assert!(html.contains(">complete</td>"));
        assert!(html.contains(">42</td>"));
        assert!(html.contains(r#"class="status-complete"#));

        assert!(html.contains(">error</td>"));
        assert!(html.contains(">3</td>"));
        assert!(html.contains(r#"class="status-error"#));
    }

    #[test]
    fn test_render_content_type_table_empty() {
        let html = render_content_type_table(&[]).into_string();

        // Should still have table structure even with no rows
        assert!(html.contains("<table"));
        assert!(html.contains("<thead>"));
        assert!(html.contains("<th>Content Type</th>"));
        assert!(html.contains("<th>Count</th>"));
        assert!(html.contains("<tbody>"));
    }

    #[test]
    fn test_render_content_type_table_with_data() {
        let content_type_counts = vec![
            ("video".to_string(), 150),
            ("text".to_string(), 75),
            ("image".to_string(), 30),
        ];
        let html = render_content_type_table(&content_type_counts).into_string();

        // Check table variant
        assert!(html.contains(r#"class="stats-table"#));

        // Check headers
        assert!(html.contains("<th>Content Type</th>"));
        assert!(html.contains("<th>Count</th>"));

        // Check rows
        assert!(html.contains(">video</td>"));
        assert!(html.contains(">150</td>"));

        assert!(html.contains(">text</td>"));
        assert!(html.contains(">75</td>"));

        assert!(html.contains(">image</td>"));
        assert!(html.contains(">30</td>"));
    }
}
