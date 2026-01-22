//! Maud-based page templates for the web UI.
//!
//! This module contains full page implementations using maud templates.
//! Each page module exports a render function that produces the complete HTML.

/// Generate a display title for a Discourse post.
///
/// For the first post in a topic (post_number == 1), returns the topic title.
/// For replies (post_number > 1), returns "Re: {topic_title}".
/// Falls back to "Reply" if no title is available.
///
/// Post number is extracted from the Discourse URL:
/// - `/t/slug/123` -> post 1 (first post)
/// - `/t/slug/123/4` -> post 4 (reply)
pub fn format_post_title(title: Option<&str>, discourse_url: &str) -> String {
    let is_reply = is_reply_post(discourse_url);

    match (title, is_reply) {
        (Some(t), true) => format!("Re: {}", t),
        (Some(t), false) => t.to_string(),
        (None, true) => "Reply".to_string(),
        (None, false) => "Topic".to_string(),
    }
}

/// Check if a Discourse URL points to a reply (post_number > 1).
fn is_reply_post(discourse_url: &str) -> bool {
    // URL format: /t/slug/topic_id or /t/slug/topic_id/post_number
    // If there's a 4th path segment after /t/, it's a reply
    let path = discourse_url
        .trim_start_matches("http://")
        .trim_start_matches("https://");

    // Find the path part after the domain
    let path = if let Some(idx) = path.find('/') {
        &path[idx..]
    } else {
        return false;
    };

    // Split path and check segments
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    // Expected: ["t", "slug", "topic_id"] for first post
    // Expected: ["t", "slug", "topic_id", "post_number"] for reply
    if segments.len() >= 4 && segments[0] == "t" {
        // Has post_number, check if > 1
        if let Ok(post_num) = segments[3].parse::<i64>() {
            return post_num > 1;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_post_title_first_post_with_title() {
        let title = format_post_title(Some("My Topic"), "https://forum.example.com/t/my-topic/123");
        assert_eq!(title, "My Topic");
    }

    #[test]
    fn test_format_post_title_reply_with_title() {
        let title = format_post_title(
            Some("My Topic"),
            "https://forum.example.com/t/my-topic/123/5",
        );
        assert_eq!(title, "Re: My Topic");
    }

    #[test]
    fn test_format_post_title_reply_no_title() {
        let title = format_post_title(None, "https://forum.example.com/t/my-topic/123/5");
        assert_eq!(title, "Reply");
    }

    #[test]
    fn test_format_post_title_first_post_no_title() {
        let title = format_post_title(None, "https://forum.example.com/t/my-topic/123");
        assert_eq!(title, "Topic");
    }

    #[test]
    fn test_is_reply_post_first_post() {
        assert!(!is_reply_post("https://forum.example.com/t/slug/123"));
    }

    #[test]
    fn test_is_reply_post_reply() {
        assert!(is_reply_post("https://forum.example.com/t/slug/123/4"));
    }

    #[test]
    fn test_is_reply_post_post_one_explicit() {
        // Post 1 explicitly mentioned is still the first post
        assert!(!is_reply_post("https://forum.example.com/t/slug/123/1"));
    }
}

pub mod admin;
pub mod all_archives;
pub mod archive;
pub mod auth;
pub mod banner;
pub mod comment;
pub mod comparison;
pub mod debug;
pub mod home;
pub mod post;
pub mod search;
pub mod site;
pub mod stats;
pub mod submit;
pub mod threads;

// Re-export page rendering functions for convenience
pub use admin::{
    render_admin_excluded_domains_page, render_admin_forum_user_profile, render_admin_panel,
    render_admin_password_reset_result, render_admin_user_profile, AdminPanelParams,
};
pub use all_archives::{render_all_archives_table_page, AllArchivesPageParams};
pub use archive::{render_archive_detail_page, ArchiveDetailParams};
pub use auth::{
    login_page, profile_page, profile_page_with_link_status, profile_page_with_message,
    render_login_page, render_profile_page, ProfilePageParams,
};
pub use banner::render_archive_banner;
pub use comment::render_comment_edit_history_page;
pub use comparison::render_comparison_page;
pub use debug::{render_debug_queue_page, DebugQueueParams};
pub use home::{
    render_home, render_home_page, render_home_paginated, render_recent_all_archives,
    render_recent_all_archives_paginated, render_recent_failed_archives,
    render_recent_failed_archives_paginated, ContentTypeFilter, HomePageParams, RecentArchivesTab,
    SourceFilter,
};
pub use post::{render_post_detail_page, PostDetailParams};
pub use search::{render_search_page, SearchPageParams};
pub use site::render_site_list_page;
pub use stats::{render_stats_page, StatsData, UserStats};
pub use submit::{
    render_submit_error, render_submit_error_page, render_submit_form, render_submit_form_page,
    render_submit_success, render_submit_success_page, SubmitFormParams,
};
pub use threads::{
    render_thread_detail_page, render_thread_job_status_page, render_threads_list_page,
    JobStatusVariant, ProgressBar, SortNav, ThreadCard, ThreadDetailParams, ThreadGrid,
    ThreadJobStatusParams, ThreadSortBy, ThreadsListParams,
};
