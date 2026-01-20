//! Statistics page module.
//!
//! This module provides the maud-based stats page rendering.

use maud::{html, Markup, Render};

use crate::components::{BaseLayout, Table, TableRow, TableVariant};
use crate::db::User;

/// Data for the statistics page.
#[derive(Debug, Clone)]
pub struct StatsData {
    /// Total number of posts
    pub post_count: i64,
    /// Total number of links
    pub link_count: i64,
    /// Archive counts by status (status name, count)
    pub status_counts: Vec<(String, i64)>,
}

impl StatsData {
    /// Create new stats data.
    #[must_use]
    pub fn new(post_count: i64, link_count: i64, status_counts: Vec<(String, i64)>) -> Self {
        Self {
            post_count,
            link_count,
            status_counts,
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

/// Render the statistics page.
///
/// Displays an overview of posts and links, followed by a table
/// showing archive counts by status.
#[must_use]
pub fn render_stats_page(stats: &StatsData, user: Option<&User>) -> Markup {
    let content = html! {
        h1 { "Statistics" }

        // Overview section
        section {
            h2 { "Overview" }
            p {
                strong { "Total Posts:" }
                " " (stats.post_count)
            }
            p {
                strong { "Total Links:" }
                " " (stats.link_count)
            }
        }

        // Archives by status section
        section {
            h2 { "Archives by Status" }
            (render_status_table(&stats.status_counts))
        }
    };

    BaseLayout::new("Statistics")
        .with_user(user)
        .render(content)
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

    #[test]
    fn test_stats_data_new() {
        let status_counts = vec![
            ("complete".to_string(), 100),
            ("pending".to_string(), 20),
            ("failed".to_string(), 5),
        ];
        let stats = StatsData::new(50, 200, status_counts);

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
        let stats = StatsData::new(50, 200, status_counts);

        assert_eq!(stats.total_archives(), 125);
    }

    #[test]
    fn test_stats_data_count_for_status() {
        let status_counts = vec![
            ("complete".to_string(), 100),
            ("pending".to_string(), 20),
            ("failed".to_string(), 5),
        ];
        let stats = StatsData::new(50, 200, status_counts);

        assert_eq!(stats.count_for_status("complete"), 100);
        assert_eq!(stats.count_for_status("pending"), 20);
        assert_eq!(stats.count_for_status("failed"), 5);
        assert_eq!(stats.count_for_status("nonexistent"), 0);
    }

    #[test]
    fn test_stats_data_empty() {
        let stats = StatsData::new(0, 0, vec![]);

        assert_eq!(stats.post_count, 0);
        assert_eq!(stats.link_count, 0);
        assert_eq!(stats.total_archives(), 0);
    }

    #[test]
    fn test_render_stats_page_basic_structure() {
        let status_counts = vec![("complete".to_string(), 100), ("pending".to_string(), 20)];
        let stats = StatsData::new(50, 200, status_counts);
        let html = render_stats_page(&stats, None).into_string();

        // Check page title
        assert!(html.contains("<title>Statistics - Discourse Link Archiver</title>"));

        // Check main heading
        assert!(html.contains("<h1>Statistics</h1>"));

        // Check overview section
        assert!(html.contains("<h2>Overview</h2>"));
        assert!(html.contains("<strong>Total Posts:</strong>"));
        assert!(html.contains(" 50"));
        assert!(html.contains("<strong>Total Links:</strong>"));
        assert!(html.contains(" 200"));

        // Check archives by status section
        assert!(html.contains("<h2>Archives by Status</h2>"));
    }

    #[test]
    fn test_render_stats_page_status_table() {
        let status_counts = vec![
            ("complete".to_string(), 100),
            ("pending".to_string(), 20),
            ("failed".to_string(), 5),
        ];
        let stats = StatsData::new(50, 200, status_counts);
        let html = render_stats_page(&stats, None).into_string();

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
        let stats = StatsData::new(10, 50, vec![]);
        let html = render_stats_page(&stats, Some(&user)).into_string();

        // Should show profile link for authenticated users
        assert!(html.contains(r#"<a href="/profile">Profile</a>"#));
        // Should not show login link
        assert!(!html.contains(r#"<a href="/login">Login</a>"#));
    }

    #[test]
    fn test_render_stats_page_with_admin_user() {
        let user = test_user(true);
        let stats = StatsData::new(10, 50, vec![]);
        let html = render_stats_page(&stats, Some(&user)).into_string();

        // Should show both profile and admin links
        assert!(html.contains(r#"<a href="/profile">Profile</a>"#));
        assert!(html.contains(r#"<a href="/admin">Admin</a>"#));
    }

    #[test]
    fn test_render_stats_page_anonymous() {
        let stats = StatsData::new(10, 50, vec![]);
        let html = render_stats_page(&stats, None).into_string();

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
}
