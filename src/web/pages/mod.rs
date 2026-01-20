//! Maud-based page templates for the web UI.
//!
//! This module contains full page implementations using maud templates.
//! Each page module exports a render function that produces the complete HTML.

pub mod admin;
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
    render_admin_excluded_domains_page, render_admin_panel, render_admin_password_reset_result,
};
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
pub use stats::{render_stats_page, StatsData};
pub use submit::{
    render_submit_error, render_submit_error_page, render_submit_form, render_submit_form_page,
    render_submit_success, render_submit_success_page, SubmitFormParams,
};
pub use threads::{
    render_thread_detail_page, render_thread_job_status_page, render_threads_list_page,
    JobStatusVariant, ProgressBar, SortNav, ThreadCard, ThreadDetailParams, ThreadGrid,
    ThreadJobStatusParams, ThreadSortBy, ThreadsListParams,
};
