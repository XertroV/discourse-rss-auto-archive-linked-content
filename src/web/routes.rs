use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::Form;
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use super::auth;
use super::diff;
use super::export;
use super::feeds;
use super::pages;
use super::AppState;
use crate::auth::{MaybeUser, RequireAdmin, RequireApproved, RequireUser};
use crate::db::{
    add_comment_reaction, can_user_edit_comment, count_all_threads, count_archives_by_status,
    count_links, count_posts, count_submissions_from_ip_last_hour,
    count_user_thread_archive_jobs_last_hour, create_comment, create_comment_reply,
    create_pending_archive, delete_archive, find_artifact_by_s3_key, get_all_threads, get_archive,
    get_archive_by_link_id, get_archives_by_domain_display, get_archives_for_post_display,
    get_archives_for_posts_display, get_archives_for_thread_job, get_artifacts_for_archive,
    get_comment_edit_history, get_comment_with_author, get_jobs_for_archive, get_link,
    get_link_by_normalized_url, get_link_occurrences_with_posts, get_post_by_guid,
    get_posts_by_thread_key, get_queue_stats, get_quote_reply_chain,
    get_recent_archives_display_filtered, get_recent_archives_filtered_full,
    get_recent_archives_with_filters, get_recent_failed_archives, get_thread_archive_job,
    get_video_file, has_missing_artifacts, insert_link, insert_submission,
    insert_thread_archive_job, pin_comment, remove_comment_reaction, reset_archive_for_rearchive,
    reset_single_skipped_archive, reset_skipped_archives, search_archives_display_filtered,
    search_archives_filtered_full, soft_delete_comment, submission_exists_for_url,
    thread_archive_job_exists_recent, toggle_archive_nsfw, unpin_comment, update_comment, NewLink,
    NewSubmission, NewThreadArchiveJob,
};
use crate::handlers::normalize_url;

/// Pagination query parameters.
#[derive(Debug, Deserialize)]
struct PaginationParams {
    #[serde(default)]
    page: usize,
    /// Filter by content type (e.g., "video", "image", "gallery", "text", "thread")
    #[serde(rename = "type")]
    content_type: Option<String>,
    /// Filter by source platform (e.g., "reddit", "youtube", "tiktok", "twitter")
    source: Option<String>,
}

const ITEMS_PER_PAGE: i64 = 50;

/// Create the router with all routes.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(home))
        .route("/login", get(auth::login_page).post(auth::login_post))
        .route("/logout", post(auth::logout))
        .route("/profile", get(auth::profile_page).post(auth::profile_post))
        .route("/admin", get(auth::admin_panel))
        .route("/admin/user/approve", post(auth::admin_approve_user))
        .route("/admin/user/revoke", post(auth::admin_revoke_user))
        .route("/admin/user/promote", post(auth::admin_promote_user))
        .route("/admin/user/demote", post(auth::admin_demote_user))
        .route("/admin/user/deactivate", post(auth::admin_deactivate_user))
        .route("/admin/user/reactivate", post(auth::admin_reactivate_user))
        .route(
            "/admin/user/reset-password",
            post(auth::admin_reset_password),
        )
        .route(
            "/admin/excluded-domains",
            get(auth::admin_excluded_domains_page),
        )
        .route(
            "/admin/excluded-domains/add",
            post(auth::admin_add_excluded_domain),
        )
        .route(
            "/admin/excluded-domains/toggle",
            post(auth::admin_toggle_excluded_domain),
        )
        .route(
            "/admin/excluded-domains/delete",
            post(auth::admin_delete_excluded_domain),
        )
        .route("/admin/user/:id", get(auth::admin_user_profile))
        .route(
            "/admin/forum-user/:username",
            get(auth::admin_forum_user_profile),
        )
        .route("/archives/failed", get(recent_failed_archives))
        .route("/archives/all", get(recent_all_archives))
        .route("/search", get(search))
        .route("/submit", get(submit_form).post(submit_url))
        .route("/submit/thread", post(submit_thread))
        .route("/submit/thread/:id", get(thread_job_status))
        .route("/archive/:id", get(archive_detail))
        .route("/archive/:id/rearchive", post(rearchive))
        .route(
            "/archive/:id/get-missing-artifacts",
            post(get_missing_artifacts),
        )
        .route("/archive/:id/toggle-nsfw", post(toggle_nsfw))
        .route("/archive/:id/delete", post(delete_archive_handler))
        .route("/archive/:id/retry-skipped", post(retry_skipped))
        .route("/archive/:id/comment", post(create_comment_handler))
        .route(
            "/archive/:id/comment/:comment_id/reply",
            post(create_reply_handler),
        )
        .route(
            "/archive/:id/comment/:comment_id",
            put(edit_comment_handler).delete(delete_comment_handler),
        )
        .route(
            "/archive/:id/comment/:comment_id/pin",
            post(pin_comment_handler),
        )
        .route(
            "/archive/:id/comment/:comment_id/unpin",
            post(unpin_comment_handler),
        )
        .route(
            "/archive/:id/comment/:comment_id/helpful",
            post(add_reaction_handler).delete(remove_reaction_handler),
        )
        .route(
            "/archive/:id/comment/:comment_id/history",
            get(comment_history_handler),
        )
        .route("/compare/:id1/:id2", get(compare_archives))
        .route("/post/:guid", get(post_detail))
        .route("/thread/:thread_key", get(thread_detail))
        .route("/threads", get(threads_list))
        .route("/site/:site", get(site_list))
        .route("/stats", get(stats))
        .route("/healthz", get(health))
        .route("/favicon.ico", get(favicon))
        .route("/feed.rss", get(feed_rss))
        .route("/feed.atom", get(feed_atom))
        .route("/export/:site", get(export::export_site))
        .route("/api/archives", get(api_archives))
        .route("/api/archive/:id/progress", get(api_archive_progress))
        .route("/api/archive/:id/comments", get(api_archive_comments))
        .route("/api/search", get(api_search))
        .route("/s3/*path", get(serve_s3_file))
        // Debug routes
        .route("/debug/queue", get(debug_queue))
        .route("/debug/reset-skipped", post(debug_reset_skipped))
}

// ========== HTML Routes ==========

async fn home(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    MaybeUser(user): MaybeUser,
) -> Response {
    let page = params.page;

    let all_recent = match get_recent_archives_display_filtered(
        state.db.pool(),
        100,
        params.content_type.as_deref(),
        params.source.as_deref(),
    )
    .await
    {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to fetch recent archives: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let recent_failed_count = all_recent.iter().filter(|a| a.status == "failed").count();

    // Home page: show pending + processing + complete, but not failed.
    // Skipped is intentionally excluded to keep the page focused.
    let mut archives: Vec<_> = all_recent
        .into_iter()
        .filter(|a| matches!(a.status.as_str(), "pending" | "processing" | "complete"))
        .collect();

    // Apply pagination
    let total_items = archives.len();
    let total_pages = total_items.div_ceil(ITEMS_PER_PAGE as usize);
    let start = (page * ITEMS_PER_PAGE as usize).min(total_items);
    let end = ((page + 1) * ITEMS_PER_PAGE as usize).min(total_items);
    archives = archives.into_iter().skip(start).take(end - start).collect();

    let markup = pages::render_home_paginated(
        &archives,
        recent_failed_count,
        page,
        total_pages,
        params.content_type.as_deref(),
        params.source.as_deref(),
        user.as_ref(),
    );
    Html(markup.into_string()).into_response()
}

async fn recent_failed_archives(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    MaybeUser(user): MaybeUser,
) -> Response {
    let page = params.page;

    let all_recent = match get_recent_archives_display_filtered(
        state.db.pool(),
        100,
        params.content_type.as_deref(),
        params.source.as_deref(),
    )
    .await
    {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to fetch recent archives: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let recent_failed_count = all_recent.iter().filter(|a| a.status == "failed").count();

    let mut failed: Vec<_> = all_recent
        .into_iter()
        .filter(|a| a.status == "failed")
        .collect();

    // Apply pagination
    let total_items = failed.len();
    let total_pages = total_items.div_ceil(ITEMS_PER_PAGE as usize);
    let start = (page * ITEMS_PER_PAGE as usize).min(total_items);
    let end = ((page + 1) * ITEMS_PER_PAGE as usize).min(total_items);
    failed = failed.into_iter().skip(start).take(end - start).collect();

    let markup = pages::render_recent_failed_archives_paginated(
        &failed,
        recent_failed_count,
        page,
        total_pages,
        params.content_type.as_deref(),
        params.source.as_deref(),
        user.as_ref(),
    );
    Html(markup.into_string()).into_response()
}

async fn recent_all_archives(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
    MaybeUser(user): MaybeUser,
) -> Response {
    let page = params.page;

    let all_recent = match get_recent_archives_display_filtered(
        state.db.pool(),
        100,
        params.content_type.as_deref(),
        params.source.as_deref(),
    )
    .await
    {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to fetch recent archives: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let recent_failed_count = all_recent.iter().filter(|a| a.status == "failed").count();

    // Apply pagination
    let total_items = all_recent.len();
    let total_pages = total_items.div_ceil(ITEMS_PER_PAGE as usize);
    let start = (page * ITEMS_PER_PAGE as usize).min(total_items);
    let end = ((page + 1) * ITEMS_PER_PAGE as usize).min(total_items);
    let archives = all_recent
        .into_iter()
        .skip(start)
        .take(end - start)
        .collect::<Vec<_>>();

    let markup = pages::render_recent_all_archives_paginated(
        &archives,
        recent_failed_count,
        page,
        total_pages,
        params.content_type.as_deref(),
        params.source.as_deref(),
        user.as_ref(),
    );
    Html(markup.into_string()).into_response()
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    q: Option<String>,
    #[allow(dead_code)]
    site: Option<String>,
    page: Option<u32>,
    /// Filter by content type (e.g., "video", "image", "gallery", "text", "thread")
    #[serde(rename = "type")]
    content_type: Option<String>,
    /// Filter by source platform (e.g., "reddit", "youtube", "tiktok", "twitter")
    source: Option<String>,
}

async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
    MaybeUser(user): MaybeUser,
) -> Response {
    tracing::debug!(q = ?params.q, page = ?params.page, "HTTP API: GET /search");
    let query = params.q.unwrap_or_default();
    let page = params.page.unwrap_or(1);
    let per_page = 20i64;
    let offset = i64::from(page.saturating_sub(1)) * per_page;

    let archives = if query.is_empty() {
        match get_recent_archives_display_filtered(
            state.db.pool(),
            per_page + offset,
            params.content_type.as_deref(),
            params.source.as_deref(),
        )
        .await
        {
            Ok(a) => a.into_iter().skip(offset as usize).collect(),
            Err(e) => {
                tracing::error!("Failed to fetch archives: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
            }
        }
    } else {
        match search_archives_display_filtered(
            state.db.pool(),
            &query,
            per_page,
            params.content_type.as_deref(),
            params.source.as_deref(),
        )
        .await
        {
            Ok(a) => a,
            Err(e) => {
                tracing::error!("Failed to search archives: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Search error").into_response();
            }
        }
    };

    let markup = pages::render_search_page(
        if query.is_empty() { None } else { Some(&query) },
        &archives,
        page as i32,
        1, // total_pages not calculated in old code
        user.as_ref(),
    );
    Html(markup.into_string()).into_response()
}

async fn archive_detail(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    MaybeUser(user): MaybeUser,
) -> Response {
    let archive = match get_archive(state.db.pool(), id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Archive not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch archive: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let link = match get_link(state.db.pool(), archive.link_id).await {
        Ok(Some(l)) => l,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Link not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch link: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let artifacts = match get_artifacts_for_archive(state.db.pool(), id).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to fetch artifacts: {e}");
            Vec::new()
        }
    };

    let occurrences = match get_link_occurrences_with_posts(state.db.pool(), archive.link_id).await
    {
        Ok(o) => o,
        Err(e) => {
            tracing::error!("Failed to fetch link occurrences: {e}");
            Vec::new()
        }
    };

    let jobs = match get_jobs_for_archive(state.db.pool(), archive.id).await {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("Failed to fetch archive jobs: {e}");
            Vec::new()
        }
    };

    // Fetch quote/reply chain for Twitter/X archives
    let quote_reply_chain =
        if archive.quoted_archive_id.is_some() || archive.reply_to_archive_id.is_some() {
            match get_quote_reply_chain(state.db.pool(), archive.id).await {
                Ok(chain) => chain,
                Err(e) => {
                    tracing::error!("Failed to fetch quote/reply chain: {e}");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

    // Check if archive has missing artifacts
    let has_missing_artifacts = match has_missing_artifacts(state.db.pool(), archive.id).await {
        Ok(missing) => missing,
        Err(e) => {
            tracing::error!("Failed to check for missing artifacts: {e}");
            false
        }
    };

    let params = pages::ArchiveDetailParams {
        archive: &archive,
        link: &link,
        artifacts: &artifacts,
        occurrences: &occurrences,
        jobs: &jobs,
        quote_reply_chain: &quote_reply_chain,
        user: user.as_ref(),
        has_missing_artifacts,
    };
    let markup = pages::render_archive_detail_page(&params);
    Html(markup.into_string()).into_response()
}

/// Handler for re-archiving an archive (POST /archive/:id/rearchive).
///
/// This resets the archive to pending state and triggers a fresh archive
/// through the full pipeline, including redirect handling.
async fn rearchive(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    RequireAdmin(_admin): RequireAdmin,
) -> Response {
    tracing::debug!(archive_id = id, "HTTP API: POST /archive/:id/rearchive");
    // Check that the archive exists
    let archive = match get_archive(state.db.pool(), id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Archive not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch archive for rearchive: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // Don't allow re-archiving if currently processing
    if archive.status == "processing" {
        return (
            StatusCode::CONFLICT,
            "Archive is currently being processed. Please wait.",
        )
            .into_response();
    }

    // Reset the archive for re-processing
    if let Err(e) = reset_archive_for_rearchive(state.db.pool(), id).await {
        tracing::error!(error = ?e, "Failed to reset archive for rearchive");
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to reset archive").into_response();
    }

    tracing::info!(archive_id = id, "Archive queued for re-archiving");

    // Redirect back to archive detail page
    axum::response::Redirect::to(&format!("/archive/{id}")).into_response()
}

/// Handler for fetching missing artifacts (POST /archive/:id/get-missing-artifacts).
///
/// This creates a background job to download missing supplementary artifacts
/// (e.g., subtitles, transcripts, comments) without re-archiving the entire content.
/// Returns immediately and allows monitoring progress through the jobs table.
async fn get_missing_artifacts(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    RequireAdmin(_admin): RequireAdmin,
) -> Response {
    use crate::db::{has_artifact_kind, ArchiveJobType, ArtifactKind};

    let archive_id = id;

    tracing::debug!(
        archive_id,
        "HTTP API: POST /archive/:id/get-missing-artifacts"
    );

    // Check that the archive exists
    let archive = match get_archive(state.db.pool(), id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Archive not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch archive: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // Don't allow if currently processing
    if archive.status == "processing" {
        return (
            StatusCode::CONFLICT,
            "Archive is currently being processed. Please wait.",
        )
            .into_response();
    }

    // Only process complete video archives
    if archive.status != "complete" {
        return (
            StatusCode::BAD_REQUEST,
            format!("Archive is not complete (status: {})", archive.status),
        )
            .into_response();
    }

    if archive.content_type.as_deref() != Some("video") {
        return (StatusCode::BAD_REQUEST, "Archive is not video content").into_response();
    }

    // Check what artifacts are missing
    let needs_subtitles = !has_artifact_kind(state.db.pool(), id, ArtifactKind::Subtitles.as_str())
        .await
        .unwrap_or(true);
    let needs_transcript =
        !has_artifact_kind(state.db.pool(), id, ArtifactKind::Transcript.as_str())
            .await
            .unwrap_or(true);
    let needs_comments = !has_artifact_kind(state.db.pool(), id, ArtifactKind::Comments.as_str())
        .await
        .unwrap_or(true);

    if !needs_subtitles && !needs_transcript && !needs_comments {
        tracing::info!(archive_id = id, "No missing artifacts to fetch");
        return axum::response::Redirect::to(&format!("/archive/{id}")).into_response();
    }

    // Create a job for fetching supplementary artifacts
    let job_id = match crate::db::create_archive_job(
        state.db.pool(),
        id,
        ArchiveJobType::SupplementaryArtifacts,
    )
    .await
    {
        Ok(job_id) => job_id,
        Err(e) => {
            tracing::error!(error = ?e, "Failed to create supplementary artifacts job");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create background job",
            )
                .into_response();
        }
    };

    tracing::info!(
        archive_id = id,
        job_id,
        needs_subtitles,
        needs_transcript,
        needs_comments,
        "Created job to fetch missing supplementary artifacts"
    );

    // Spawn background task to fetch artifacts
    let state_clone = state.clone();
    tokio::spawn(async move {
        process_supplementary_artifacts_job(state_clone, id, job_id).await;
    });

    // Redirect back to archive detail page immediately
    axum::response::Redirect::to(&format!("/archive/{id}")).into_response()
}

/// Background task to fetch supplementary artifacts for a video archive.
async fn process_supplementary_artifacts_job(state: AppState, archive_id: i64, job_id: i64) {
    use crate::archiver::ytdlp;
    use crate::archiver::CookieOptions;
    use crate::db::{set_job_completed, set_job_failed, set_job_running, ArtifactKind};

    // Mark job as running
    if let Err(e) = set_job_running(state.db.pool(), job_id).await {
        tracing::error!(archive_id, job_id, error = ?e, "Failed to mark job as running");
        return;
    }

    // Get the link
    let link = match get_link(
        state.db.pool(),
        match get_archive(state.db.pool(), archive_id).await {
            Ok(Some(a)) => a.link_id,
            Ok(None) => {
                let _ = set_job_failed(state.db.pool(), job_id, "Archive not found").await;
                return;
            }
            Err(e) => {
                let _ =
                    set_job_failed(state.db.pool(), job_id, &format!("Database error: {e}")).await;
                return;
            }
        },
    )
    .await
    {
        Ok(Some(l)) => l,
        Ok(None) => {
            let _ = set_job_failed(state.db.pool(), job_id, "Link not found").await;
            return;
        }
        Err(e) => {
            let _ = set_job_failed(state.db.pool(), job_id, &format!("Database error: {e}")).await;
            return;
        }
    };

    // Create a temporary work directory
    let work_dir = std::env::temp_dir().join(format!("archive-supplementary-{}", archive_id));
    if let Err(e) = std::fs::create_dir_all(&work_dir) {
        tracing::error!(archive_id, error = %e, "Failed to create temp directory");
        let _ = set_job_failed(
            state.db.pool(),
            job_id,
            &format!("Failed to create temp directory: {e}"),
        )
        .await;
        return;
    }

    // Set up cookies
    let cookies_file = match state.config.cookies_file_path.as_deref() {
        Some(path) if path.exists() => Some(path),
        _ => None,
    };
    let cookies = CookieOptions {
        cookies_file,
        browser_profile: state.config.yt_dlp_cookies_from_browser.as_deref(),
    };

    // Download supplementary artifacts (subtitles, transcripts, comments)
    let should_download_comments = state.config.comments_enabled
        && crate::archiver::is_comments_supported_platform(&link.domain, &state.config);

    let result = match ytdlp::download_supplementary_artifacts(
        &link.normalized_url,
        &work_dir,
        &cookies,
        &state.config,
        true, // download subtitles
        should_download_comments,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(archive_id, error = ?e, "Failed to download supplementary artifacts");
            let _ = std::fs::remove_dir_all(&work_dir);
            let _ = set_job_failed(state.db.pool(), job_id, &format!("Download failed: {e}")).await;
            return;
        }
    };

    // Process subtitle files and comments if any were downloaded
    let mut artifacts_count = 0;
    if !result.extra_files.is_empty() {
        let subtitle_files: Vec<String> = result
            .extra_files
            .iter()
            .filter(|f| f.ends_with(".vtt") || f.ends_with(".srt"))
            .cloned()
            .collect();

        if !subtitle_files.is_empty() {
            use crate::archiver::transcript::build_transcript_from_file;
            use crate::archiver::ytdlp::parse_subtitle_info;

            let s3_prefix = format!("archives/{}/", archive_id);

            // Upload each subtitle file with metadata
            for subtitle_file in &subtitle_files {
                let local_path = work_dir.join(subtitle_file);
                if !local_path.exists() {
                    tracing::warn!(archive_id, file = %subtitle_file, "Subtitle file not found");
                    continue;
                }

                let (language, is_auto, format) = parse_subtitle_info(subtitle_file);
                let key = format!("{s3_prefix}subtitles/{subtitle_file}");
                let size_bytes = tokio::fs::metadata(&local_path)
                    .await
                    .ok()
                    .map(|m| m.len() as i64);

                let content_type = if format == "vtt" {
                    "text/vtt"
                } else {
                    "application/x-subrip"
                };

                // Upload subtitle file
                if let Err(e) = state
                    .s3
                    .upload_file(&local_path, &key, Some(archive_id))
                    .await
                {
                    tracing::warn!(archive_id, file = %subtitle_file, error = %e, "Failed to upload subtitle file");
                    continue;
                }

                // Store metadata about the subtitle in JSON format
                let subtitle_metadata = serde_json::json!({
                    "language": language,
                    "is_auto": is_auto,
                    "format": format,
                });

                // Insert subtitle artifact with metadata
                if let Err(e) = crate::db::insert_artifact_with_metadata(
                    state.db.pool(),
                    archive_id,
                    ArtifactKind::Subtitles.as_str(),
                    &key,
                    Some(content_type),
                    size_bytes,
                    None, // sha256
                    Some(&subtitle_metadata.to_string()),
                )
                .await
                {
                    tracing::warn!(archive_id, file = %subtitle_file, error = %e, "Failed to insert subtitle artifact");
                } else {
                    artifacts_count += 1;
                }
            }

            // Generate transcript from the first English subtitle file
            if let Some(subtitle_file) = subtitle_files.iter().find(|f| f.contains(".en")) {
                let local_path = work_dir.join(subtitle_file);
                if let Ok(transcript) = build_transcript_from_file(&local_path).await {
                    if !transcript.is_empty() {
                        let transcript_key = format!("{s3_prefix}subtitles/transcript.txt");
                        let size_bytes = transcript.len() as i64;

                        // Upload transcript
                        if let Err(e) = state
                            .s3
                            .upload_bytes(transcript.as_bytes(), &transcript_key, "text/plain")
                            .await
                        {
                            tracing::warn!(archive_id, error = %e, "Failed to upload transcript");
                        } else {
                            // Insert transcript artifact
                            if let Err(e) = crate::db::insert_artifact(
                                state.db.pool(),
                                archive_id,
                                ArtifactKind::Transcript.as_str(),
                                &transcript_key,
                                Some("text/plain"),
                                Some(size_bytes),
                                None,
                            )
                            .await
                            {
                                tracing::warn!(archive_id, error = %e, "Failed to insert transcript artifact");
                            } else {
                                artifacts_count += 1;
                            }
                        }
                    }
                }
            }
        }

        // Process comments.json if present
        let comments_file = result
            .extra_files
            .iter()
            .find(|f| f.ends_with("comments.json"));

        if let Some(comments_filename) = comments_file {
            let comments_path = work_dir.join(comments_filename);
            if comments_path.exists() {
                let s3_prefix = format!("archives/{}/", archive_id);
                let s3_key = format!("{}comments.json", s3_prefix);

                // Read comment stats from JSON for metadata
                let comment_stats = tokio::fs::read_to_string(&comments_path)
                    .await
                    .ok()
                    .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
                    .and_then(|json| json.get("stats").cloned());

                // Upload to S3
                match state
                    .s3
                    .upload_file(&comments_path, &s3_key, Some(archive_id))
                    .await
                {
                    Ok(_) => {
                        let size = comments_path.metadata().ok().map(|m| m.len() as i64);

                        // Prepare metadata JSON
                        let metadata_json = comment_stats.map(|stats| {
                            serde_json::json!({
                                "stats": stats,
                                "platform": crate::archiver::extract_platform_name(&link.domain)
                            })
                            .to_string()
                        });

                        // Insert artifact with Comments kind
                        if let Err(e) = crate::db::insert_artifact_with_metadata(
                            state.db.pool(),
                            archive_id,
                            ArtifactKind::Comments.as_str(),
                            &s3_key,
                            Some("application/json"),
                            size,
                            None, // sha256
                            metadata_json.as_deref(),
                        )
                        .await
                        {
                            tracing::warn!(
                                archive_id,
                                error = %e,
                                "Failed to insert comments artifact"
                            );
                        } else {
                            artifacts_count += 1;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            archive_id,
                            error = %e,
                            "Failed to upload comments.json to S3"
                        );
                    }
                }
            }
        }
        tracing::info!(
            archive_id,
            job_id,
            count = artifacts_count,
            "Successfully fetched missing artifacts"
        );
    }

    // Clean up temp directory
    if let Err(e) = std::fs::remove_dir_all(&work_dir) {
        tracing::warn!(error = %e, "Failed to clean up temp directory");
    }

    // Mark job as completed
    let metadata = Some(format!("Fetched {} artifact(s)", artifacts_count));
    if let Err(e) = set_job_completed(state.db.pool(), job_id, metadata.as_deref()).await {
        tracing::error!(archive_id, job_id, error = ?e, "Failed to mark job as completed");
    }
}

/// Handler for toggling NSFW status (POST /archive/:id/toggle-nsfw).
async fn toggle_nsfw(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    RequireApproved(user): RequireApproved,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    tracing::debug!(
        archive_id = id,
        user_id = user.id,
        "HTTP API: POST /archive/:id/toggle-nsfw"
    );
    let client_ip = addr.ip().to_string();
    let forwarded_for = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    match toggle_archive_nsfw(state.db.pool(), id).await {
        Ok(new_status) => {
            tracing::info!(archive_id = id, is_nsfw = new_status, "Toggled NSFW status");

            // Audit log the NSFW toggle
            if let Err(e) = crate::db::create_audit_event(
                state.db.pool(),
                Some(user.id),
                if new_status {
                    "nsfw_enabled"
                } else {
                    "nsfw_disabled"
                },
                Some("archive"),
                Some(id),
                None,
                Some(&client_ip),
                forwarded_for,
                user_agent.as_deref(),
            )
            .await
            {
                tracing::error!("Failed to create audit event: {e}");
            }

            axum::response::Redirect::to(&format!("/archive/{id}")).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to toggle NSFW status: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to toggle NSFW").into_response()
        }
    }
}

/// Handler for deleting an archive (POST /archive/:id/delete).
async fn delete_archive_handler(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    RequireAdmin(_admin): RequireAdmin,
) -> Response {
    tracing::debug!(archive_id = id, "HTTP API: POST /archive/:id/delete");
    // Get the archive first to log what we're deleting
    let archive = match get_archive(state.db.pool(), id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Archive not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch archive for deletion: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // Don't allow deletion if currently processing
    if archive.status == "processing" {
        return (
            StatusCode::CONFLICT,
            "Cannot delete archive while processing",
        )
            .into_response();
    }

    if let Err(e) = delete_archive(state.db.pool(), id).await {
        tracing::error!("Failed to delete archive: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to delete archive",
        )
            .into_response();
    }

    tracing::info!(archive_id = id, "Archive deleted");

    // Redirect to home page since archive no longer exists
    axum::response::Redirect::to("/").into_response()
}

/// Handler for retrying a single skipped archive (POST /archive/:id/retry-skipped).
async fn retry_skipped(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    RequireAdmin(_admin): RequireAdmin,
) -> Response {
    tracing::debug!(archive_id = id, "HTTP API: POST /archive/:id/retry-skipped");
    match reset_single_skipped_archive(state.db.pool(), id).await {
        Ok(true) => {
            tracing::info!(archive_id = id, "Reset skipped archive for retry");
            axum::response::Redirect::to(&format!("/archive/{id}")).into_response()
        }
        Ok(false) => (StatusCode::BAD_REQUEST, "Archive is not in skipped status").into_response(),
        Err(e) => {
            tracing::error!("Failed to reset skipped archive: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to reset archive").into_response()
        }
    }
}

// ========== Debug Routes ==========

const MAX_RETRIES: i32 = 3;

/// Handler for debug queue page (GET /debug/queue).
async fn debug_queue(
    State(state): State<AppState>,
    RequireAdmin(_admin): RequireAdmin,
) -> Response {
    let stats = match get_queue_stats(state.db.pool(), MAX_RETRIES).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Failed to get queue stats: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let recent_failures = match get_recent_failed_archives(state.db.pool(), 20).await {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("Failed to get recent failures: {e}");
            Vec::new()
        }
    };

    let params = pages::DebugQueueParams::new(&stats, &recent_failures);
    let markup = pages::render_debug_queue_page(&params);
    Html(markup.into_string()).into_response()
}

/// Handler for resetting all skipped archives (POST /debug/reset-skipped).
async fn debug_reset_skipped(
    State(state): State<AppState>,
    RequireAdmin(_admin): RequireAdmin,
) -> Response {
    match reset_skipped_archives(state.db.pool()).await {
        Ok(count) => {
            tracing::info!(count, "Reset all skipped archives");
            axum::response::Redirect::to("/debug/queue").into_response()
        }
        Err(e) => {
            tracing::error!("Failed to reset skipped archives: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to reset archives",
            )
                .into_response()
        }
    }
}

/// Path parameters for archive comparison.
#[derive(Debug, Deserialize)]
pub struct CompareParams {
    id1: i64,
    id2: i64,
}

async fn compare_archives(
    State(state): State<AppState>,
    Path(params): Path<CompareParams>,
) -> Response {
    // Fetch both archives
    let archive1 = match get_archive(state.db.pool(), params.id1).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                format!("Archive {} not found", params.id1),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch archive {}: {e}", params.id1);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let archive2 = match get_archive(state.db.pool(), params.id2).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                format!("Archive {} not found", params.id2),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch archive {}: {e}", params.id2);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // Fetch associated links for display
    let link1 = match get_link(state.db.pool(), archive1.link_id).await {
        Ok(Some(l)) => l,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Link not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch link: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let link2 = match get_link(state.db.pool(), archive2.link_id).await {
        Ok(Some(l)) => l,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Link not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch link: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // Compute diff on content_text
    let text1 = archive1.content_text.as_deref().unwrap_or("");
    let text2 = archive2.content_text.as_deref().unwrap_or("");
    let diff_result = diff::compute_diff(text1, text2);

    let markup = pages::render_comparison_page(&archive1, &link1, &archive2, &link2, &diff_result);
    Html(markup.into_string()).into_response()
}

async fn post_detail(State(state): State<AppState>, Path(guid): Path<String>) -> Response {
    let post = match get_post_by_guid(state.db.pool(), &guid).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Post not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch post: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let archives = match get_archives_for_post_display(state.db.pool(), post.id).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to fetch archives for post: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let params = pages::PostDetailParams {
        post: &post,
        archives: &archives,
        user: None,
    };
    let markup = pages::render_post_detail_page(&params);
    Html(markup.into_string()).into_response()
}

async fn thread_detail(State(state): State<AppState>, Path(thread_key): Path<String>) -> Response {
    let posts = match get_posts_by_thread_key(state.db.pool(), &thread_key).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Failed to fetch posts for thread: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    if posts.is_empty() {
        return (StatusCode::NOT_FOUND, "Thread not found").into_response();
    }

    let post_ids: Vec<i64> = posts.iter().map(|p| p.id).collect();

    let archives = match get_archives_for_posts_display(state.db.pool(), &post_ids).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to fetch archives for thread: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let params = pages::ThreadDetailParams {
        thread_key: &thread_key,
        posts: &posts,
        archives: &archives,
        user: None,
    };
    let markup = pages::render_thread_detail_page(&params);
    Html(markup.into_string()).into_response()
}

#[derive(Debug, Deserialize)]
pub struct ThreadsListParams {
    sort: Option<String>,
    page: Option<u32>,
}

async fn threads_list(
    State(state): State<AppState>,
    Query(params): Query<ThreadsListParams>,
) -> Response {
    let sort_by = params.sort.as_deref().unwrap_or("created");
    let page = params.page.unwrap_or(1);
    let per_page = 20i64;
    let offset = i64::from(page.saturating_sub(1)) * per_page;

    // Get total count for pagination
    let total_threads = match count_all_threads(state.db.pool()).await {
        Ok(count) => count,
        Err(e) => {
            tracing::error!("Failed to count threads: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let total_pages = ((total_threads + per_page - 1) / per_page).max(1) as usize;

    let threads = match get_all_threads(state.db.pool(), sort_by, per_page, offset).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to fetch threads: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let params = pages::ThreadsListParams {
        threads: &threads,
        sort_by: pages::ThreadSortBy::from_str(sort_by),
        page: (page as usize).saturating_sub(1), // Convert to 0-indexed
        total_pages,
        user: None,
    };
    let markup = pages::render_threads_list_page(&params);
    Html(markup.into_string()).into_response()
}

#[derive(Debug, Deserialize)]
pub struct SiteListParams {
    page: Option<u32>,
}

async fn site_list(
    State(state): State<AppState>,
    Path(site): Path<String>,
    Query(params): Query<SiteListParams>,
) -> Response {
    let page = params.page.unwrap_or(1);
    let per_page = 20i64;
    let offset = i64::from(page.saturating_sub(1)) * per_page;

    let archives =
        match get_archives_by_domain_display(state.db.pool(), &site, per_page, offset).await {
            Ok(a) => a,
            Err(e) => {
                tracing::error!("Failed to fetch archives by domain: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
            }
        };

    // Calculate total pages (approximation based on current page having data)
    let total_pages = if archives.len() < per_page as usize {
        page as i32
    } else {
        (page as i32) + 1
    };
    let markup =
        pages::render_site_list_page(&site, &archives, (page - 1) as i32, total_pages, None);
    Html(markup.into_string()).into_response()
}

async fn stats(State(state): State<AppState>, MaybeUser(user): MaybeUser) -> Response {
    let status_counts = match count_archives_by_status(state.db.pool()).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to count archives: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let link_count = count_links(state.db.pool()).await.unwrap_or(0);
    let post_count = count_posts(state.db.pool()).await.unwrap_or(0);

    let stats_data = pages::StatsData::new(post_count, link_count, status_counts);
    let markup = pages::render_stats_page(&stats_data, user.as_ref());
    Html(markup.into_string()).into_response()
}

async fn health() -> &'static str {
    "OK"
}

async fn favicon() -> Response {
    // Return a simple SVG favicon (box emoji)
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><text y=".9em" font-size="90">📦</text></svg>"#;
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/svg+xml")],
        svg,
    )
        .into_response()
}

// ========== Submission Routes ==========

async fn submit_form(State(state): State<AppState>, MaybeUser(user): MaybeUser) -> Response {
    // Check if submissions are enabled
    if !state.config.submission_enabled {
        let html = pages::render_submit_error("URL submissions are currently disabled.");
        return Html(html).into_response();
    }

    // Determine auth warning and whether user can submit
    let (auth_warning, can_submit) = match &user {
        None => (
            Some("You must be logged in to submit URLs. <a href=\"/login\">Log in</a> or register first."),
            false,
        ),
        Some(u) if !u.is_approved => (
            Some("Your account is pending admin approval. You cannot submit URLs yet."),
            false,
        ),
        Some(_) => (None, true),
    };

    let html = pages::render_submit_form(None, None, auth_warning, can_submit);
    Html(html).into_response()
}

#[derive(Debug, Deserialize)]
pub struct SubmitForm {
    url: String,
}

async fn submit_url(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    RequireApproved(user): RequireApproved,
    Form(form): Form<SubmitForm>,
) -> Response {
    tracing::debug!(user_id = user.id, "HTTP API: POST /submit");
    // Check if submissions are enabled
    if !state.config.submission_enabled {
        let html = pages::render_submit_error("URL submissions are currently disabled.");
        return Html(html).into_response();
    }

    let client_ip = addr.ip().to_string();

    // Rate limit check
    let rate_limit = state.config.submission_rate_limit_per_hour;
    match count_submissions_from_ip_last_hour(state.db.pool(), &client_ip).await {
        Ok(count) => {
            if count >= i64::from(rate_limit) {
                let html = pages::render_submit_form(
                    Some(&format!(
                        "Rate limit exceeded. Maximum {rate_limit} submissions per hour."
                    )),
                    None,
                    None,
                    true,
                );
                return Html(html).into_response();
            }
        }
        Err(e) => {
            tracing::error!("Failed to check rate limit: {e}");
            let html = pages::render_submit_form(Some("Internal error"), None, None, true);
            return Html(html).into_response();
        }
    }

    // Validate URL
    let url = form.url.trim();
    if url.is_empty() {
        let html = pages::render_submit_form(Some("URL is required"), None, None, true);
        return Html(html).into_response();
    }

    // Parse and validate URL
    let parsed_url = if let Ok(u) = url::Url::parse(url) {
        u
    } else {
        let html = pages::render_submit_form(Some("Invalid URL format"), None, None, true);
        return Html(html).into_response();
    };

    // Only allow http/https
    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        let html =
            pages::render_submit_form(Some("Only HTTP/HTTPS URLs are allowed"), None, None, true);
        return Html(html).into_response();
    }

    // Normalize URL
    let normalized = normalize_url(url);
    let domain = parsed_url.host_str().unwrap_or("unknown").to_string();

    // Detect and log Twitter URL submissions from authenticated users
    let is_twitter = domain.contains("twitter.com") || domain.contains("x.com");
    if is_twitter {
        tracing::info!(
            user_id = user.id,
            username = &user.username,
            url = url,
            "Twitter URL submitted for archival by authenticated user"
        );
    }

    // Check if this URL was submitted recently
    match submission_exists_for_url(state.db.pool(), &normalized).await {
        Ok(true) => {
            let html = pages::render_submit_form(
                Some("This URL was already submitted recently"),
                None,
                None,
                true,
            );
            return Html(html).into_response();
        }
        Ok(false) => {}
        Err(e) => {
            tracing::error!("Failed to check existing submission: {e}");
            let html = pages::render_submit_form(Some("Internal error"), None, None, true);
            return Html(html).into_response();
        }
    }

    // Create submission record
    let submission = NewSubmission {
        url: url.to_string(),
        normalized_url: normalized.clone(),
        submitted_by_ip: client_ip,
        submitted_by_user_id: Some(user.id),
    };

    let submission_id = match insert_submission(state.db.pool(), &submission).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Failed to insert submission: {e}");
            let html =
                pages::render_submit_form(Some("Failed to save submission"), None, None, true);
            return Html(html).into_response();
        }
    };

    // Check if we already have this link
    let link_id = match get_link_by_normalized_url(state.db.pool(), &normalized).await {
        Ok(Some(link)) => link.id,
        Ok(None) => {
            // Create new link
            let new_link = NewLink {
                original_url: url.to_string(),
                normalized_url: normalized.clone(),
                canonical_url: None,
                domain,
            };
            match insert_link(state.db.pool(), &new_link).await {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!("Failed to insert link: {e}");
                    let html = pages::render_submit_error("Failed to process URL");
                    return Html(html).into_response();
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to check existing link: {e}");
            let html = pages::render_submit_error("Internal error");
            return Html(html).into_response();
        }
    };

    // Check if archive already exists
    match get_archive_by_link_id(state.db.pool(), link_id).await {
        Ok(Some(_)) => {
            // Archive already exists, show success with note
            let html = pages::render_submit_form(
                None,
                Some("This URL has already been archived. Check the search for results."),
                None,
                true,
            );
            return Html(html).into_response();
        }
        Ok(None) => {
            // Create pending archive (no post_date for manual submissions)
            if let Err(e) = create_pending_archive(state.db.pool(), link_id, None).await {
                tracing::error!("Failed to create pending archive: {e}");
                let html = pages::render_submit_error("Failed to queue for archiving");
                return Html(html).into_response();
            }
        }
        Err(e) => {
            tracing::error!("Failed to check existing archive: {e}");
            let html = pages::render_submit_error("Internal error");
            return Html(html).into_response();
        }
    }

    tracing::info!(
        submission_id = submission_id,
        url = %normalized,
        "New URL submitted for archiving"
    );

    let html = pages::render_submit_success(submission_id);
    Html(html).into_response()
}

// ========== Thread Archive Routes ==========

#[derive(Debug, Deserialize)]
pub struct SubmitThreadForm {
    thread_url: String,
}

/// Submit a thread for archiving all links.
async fn submit_thread(
    State(state): State<AppState>,
    RequireApproved(user): RequireApproved,
    Form(form): Form<SubmitThreadForm>,
) -> Response {
    tracing::debug!(user_id = user.id, "HTTP API: POST /submit/thread");

    let thread_url = form.thread_url.trim();
    if thread_url.is_empty() {
        let html = pages::render_submit_form(Some("Thread URL is required"), None, None, true);
        return Html(html).into_response();
    }

    // Parse and validate URL
    let parsed_url = match url::Url::parse(thread_url) {
        Ok(u) => u,
        Err(_) => {
            let html = pages::render_submit_form(Some("Invalid URL format"), None, None, true);
            return Html(html).into_response();
        }
    };

    // Only allow http/https
    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        let html =
            pages::render_submit_form(Some("Only HTTP/HTTPS URLs are allowed"), None, None, true);
        return Html(html).into_response();
    }

    // Extract forum domain from config.rss_url
    let config_forum_domain = url::Url::parse(&state.config.rss_url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()));

    let submitted_domain = parsed_url.host_str().map(|s| s.to_string());

    // Validate domain matches the configured forum
    if config_forum_domain.is_none() || submitted_domain.is_none() {
        let html = pages::render_submit_form(Some("Could not validate domain"), None, None, true);
        return Html(html).into_response();
    }

    let config_domain = config_forum_domain.unwrap();
    let thread_domain = submitted_domain.unwrap();

    if !domains_match_for_thread(&thread_domain, &config_domain) {
        let html = pages::render_submit_form(
            Some(&format!("Only threads from {} are allowed", config_domain)),
            None,
            None,
            true,
        );
        return Html(html).into_response();
    }

    // Clean the URL: remove query string, fragment, and post number
    let clean_url = {
        let mut clean = parsed_url.clone();
        clean.set_query(None);
        clean.set_fragment(None);
        normalize_discourse_thread_url(&mut clean);
        clean.to_string().trim_end_matches('/').to_string()
    };

    // Build RSS URL by appending .rss
    let rss_url = format!("{}.rss", clean_url);

    // Rate limit check: 5 thread jobs per hour per user
    const THREAD_RATE_LIMIT: i64 = 5;
    match count_user_thread_archive_jobs_last_hour(state.db.pool(), user.id).await {
        Ok(count) => {
            if count >= THREAD_RATE_LIMIT {
                let html = pages::render_submit_form(
                    Some(&format!(
                        "Rate limit exceeded. Maximum {} thread archives per hour.",
                        THREAD_RATE_LIMIT
                    )),
                    None,
                    None,
                    true,
                );
                return Html(html).into_response();
            }
        }
        Err(e) => {
            tracing::error!("Failed to check thread rate limit: {e}");
            let html = pages::render_submit_form(Some("Internal error"), None, None, true);
            return Html(html).into_response();
        }
    }

    // Check if this thread was recently submitted
    match thread_archive_job_exists_recent(state.db.pool(), &clean_url).await {
        Ok(true) => {
            let html = pages::render_submit_form(
                Some("This thread was already submitted recently. Check the status page."),
                None,
                None,
                true,
            );
            return Html(html).into_response();
        }
        Ok(false) => {}
        Err(e) => {
            tracing::error!("Failed to check existing thread job: {e}");
            let html = pages::render_submit_form(Some("Internal error"), None, None, true);
            return Html(html).into_response();
        }
    }

    // Create thread archive job
    let job = NewThreadArchiveJob {
        thread_url: clean_url,
        rss_url,
        user_id: user.id,
    };

    let job_id = match insert_thread_archive_job(state.db.pool(), &job).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Failed to create thread archive job: {e}");
            let html = pages::render_submit_form(
                Some("Failed to create thread archive job"),
                None,
                None,
                true,
            );
            return Html(html).into_response();
        }
    };

    tracing::info!(
        job_id = job_id,
        thread_url = %job.thread_url,
        user_id = user.id,
        "Thread archive job created"
    );

    // Redirect to status page
    (
        StatusCode::SEE_OTHER,
        [(header::LOCATION, format!("/submit/thread/{}", job_id))],
    )
        .into_response()
}

/// Check if two domains match (ignoring www prefix and case).
fn domains_match_for_thread(domain1: &str, domain2: &str) -> bool {
    let d1 = domain1.to_ascii_lowercase();
    let d2 = domain2.to_ascii_lowercase();
    let d1 = d1.strip_prefix("www.").unwrap_or(&d1);
    let d2 = d2.strip_prefix("www.").unwrap_or(&d2);
    d1 == d2
}

/// Normalize a Discourse thread URL by removing post numbers from the path.
///
/// Discourse URLs can include a post number: `/t/{slug}/{thread_id}/{post_number}`
/// We want to normalize to: `/t/{slug}/{thread_id}`
fn normalize_discourse_thread_url(url: &mut url::Url) {
    let path = url.path();
    let segments: Vec<&str> = path.split('/').collect();

    // Filter out empty segments (from trailing slashes)
    let non_empty_segments: Vec<&str> =
        segments.iter().filter(|s| !s.is_empty()).copied().collect();

    // Discourse thread URLs have: ["t", slug, thread_id] or ["t", slug, thread_id, post_number]
    // Check if we have 4+ segments and the pattern matches /t/{slug}/{thread_id}/{post_number}
    if non_empty_segments.len() >= 4 && non_empty_segments[0] == "t" {
        if let Some(last_segment) = non_empty_segments.last() {
            if last_segment.parse::<u32>().is_ok() {
                // Build new path without the post number
                // Reconstruct with leading slash
                let new_segments = &non_empty_segments[..non_empty_segments.len() - 1];
                let new_path = format!("/{}", new_segments.join("/"));
                url.set_path(&new_path);
            }
        }
    }
}

/// Show thread archive job status page.
async fn thread_job_status(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    MaybeUser(user): MaybeUser,
) -> Response {
    let job = match get_thread_archive_job(state.db.pool(), id).await {
        Ok(Some(j)) => j,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Job not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch thread archive job: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // Fetch archives for processing/completed jobs
    let archives = if matches!(job.status.as_str(), "processing" | "complete") {
        match get_archives_for_thread_job(state.db.pool(), &job).await {
            Ok(archives) => archives,
            Err(e) => {
                tracing::error!("Failed to fetch archives for thread job {}: {e}", job.id);
                Vec::new() // Degrade gracefully
            }
        }
    } else {
        Vec::new()
    };

    let params = pages::ThreadJobStatusParams {
        job: &job,
        archives: &archives,
        user: user.as_ref(),
    };
    let markup = pages::render_thread_job_status_page(&params);
    Html(markup.into_string()).into_response()
}

// ========== Feed Routes ==========

#[derive(Debug, Deserialize)]
pub struct FeedParams {
    /// Filter by domain (e.g., "old.reddit.com")
    site: Option<String>,
    /// Filter by content type (e.g., "video", "article", "image")
    #[serde(rename = "type")]
    content_type: Option<String>,
    /// Maximum number of items to return (default 50, max 100)
    limit: Option<i64>,
}

async fn feed_rss(State(state): State<AppState>, Query(params): Query<FeedParams>) -> Response {
    let limit = params.limit.unwrap_or(50).min(100);

    let archives = match get_recent_archives_with_filters(
        state.db.pool(),
        limit,
        params.site.as_deref(),
        params.content_type.as_deref(),
    )
    .await
    {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to fetch archives for RSS feed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // Determine base URL from config or default
    let base_url = format!("http://{}:{}", state.config.web_host, state.config.web_port);

    let rss = feeds::generate_rss(&archives, &base_url);

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
        rss,
    )
        .into_response()
}

async fn feed_atom(State(state): State<AppState>, Query(params): Query<FeedParams>) -> Response {
    let limit = params.limit.unwrap_or(50).min(100);

    let archives = match get_recent_archives_with_filters(
        state.db.pool(),
        limit,
        params.site.as_deref(),
        params.content_type.as_deref(),
    )
    .await
    {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to fetch archives for Atom feed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // Determine base URL from config or default
    let base_url = format!("http://{}:{}", state.config.web_host, state.config.web_port);

    let atom = feeds::generate_atom(&archives, &base_url);

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/atom+xml; charset=utf-8")],
        atom,
    )
        .into_response()
}

// ========== JSON API Routes ==========

/// NSFW filter mode for API queries.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum NsfwFilter {
    /// Show all archives (both SFW and NSFW)
    #[default]
    Show,
    /// Hide NSFW archives (show only SFW)
    Hide,
    /// Show only NSFW archives
    Only,
}

#[derive(Debug, Deserialize)]
pub struct ApiArchivesParams {
    page: Option<u32>,
    per_page: Option<u32>,
    #[serde(default)]
    nsfw: NsfwFilter,
    /// Filter by content type (e.g., "video", "image", "gallery", "text", "thread")
    #[serde(rename = "type")]
    content_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    data: T,
    page: u32,
    per_page: u32,
}

async fn api_archives(
    State(state): State<AppState>,
    Query(params): Query<ApiArchivesParams>,
) -> Response {
    let page = params.page.unwrap_or(1);
    let per_page = params.per_page.unwrap_or(20).min(100);
    let offset = i64::from(page.saturating_sub(1)) * i64::from(per_page);

    // Convert NsfwFilter enum to Option<bool>
    // None = show all, Some(false) = hide NSFW, Some(true) = only NSFW
    let nsfw_filter = match params.nsfw {
        NsfwFilter::Show => None,
        NsfwFilter::Hide => Some(false),
        NsfwFilter::Only => Some(true),
    };

    let archives = match get_recent_archives_filtered_full(
        state.db.pool(),
        i64::from(per_page),
        offset,
        nsfw_filter,
        params.content_type.as_deref(),
    )
    .await
    {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to fetch archives: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    Json(ApiResponse {
        data: archives,
        page,
        per_page,
    })
    .into_response()
}

/// API endpoint to get download progress for an archive.
///
/// Returns JSON with progress percentage and details.
#[derive(Debug, Serialize)]
struct ProgressResponse {
    progress_percent: Option<f64>,
    progress_details: Option<serde_json::Value>,
    status: String,
}

async fn api_archive_progress(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    // Fetch archive progress from database
    let result = sqlx::query_as::<_, (String, Option<f64>, Option<String>)>(
        "SELECT status, progress_percent, progress_details FROM archives WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(state.db.pool())
    .await;

    match result {
        Ok(Some((status, progress_percent, progress_details))) => {
            let details_json = progress_details.and_then(|s| serde_json::from_str(&s).ok());

            Json(ProgressResponse {
                progress_percent,
                progress_details: details_json,
                status,
            })
            .into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, "Archive not found").into_response(),
        Err(e) => {
            tracing::error!("Failed to fetch archive progress: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

/// API route to fetch comments.json from S3 for an archive
async fn api_archive_comments(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
    use axum::http::header;

    // Get archive (to verify it exists)
    let _archive = match get_archive(state.db.pool(), id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Archive not found").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch archive: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // Get artifacts
    let artifacts = match get_artifacts_for_archive(state.db.pool(), id).await {
        Ok(arts) => arts,
        Err(e) => {
            tracing::error!("Failed to fetch artifacts: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // Find comments artifact
    let comments_artifact = match artifacts.iter().find(|a| a.kind == "comments") {
        Some(a) => a,
        None => {
            return (StatusCode::NOT_FOUND, "Comments not found").into_response();
        }
    };

    // Fetch from S3
    let comments_data = match state.s3.get_object(&comments_artifact.s3_key).await {
        Ok(Some((bytes, _content_type))) => bytes,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Comments file not found in S3").into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch comments from S3: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch comments",
            )
                .into_response();
        }
    };

    // Return JSON directly
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        comments_data,
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct ApiSearchParams {
    q: String,
    page: Option<u32>,
    per_page: Option<u32>,
    #[serde(default)]
    nsfw: NsfwFilter,
    /// Filter by content type (e.g., "video", "image", "gallery", "text", "thread")
    #[serde(rename = "type")]
    content_type: Option<String>,
}

async fn api_search(
    State(state): State<AppState>,
    Query(params): Query<ApiSearchParams>,
) -> Response {
    let page = params.page.unwrap_or(1);
    let per_page = params.per_page.unwrap_or(20).min(100);

    // Convert NsfwFilter enum to Option<bool>
    // None = show all, Some(false) = hide NSFW, Some(true) = only NSFW
    let nsfw_filter = match params.nsfw {
        NsfwFilter::Show => None,
        NsfwFilter::Hide => Some(false),
        NsfwFilter::Only => Some(true),
    };

    let archives = match search_archives_filtered_full(
        state.db.pool(),
        &params.q,
        i64::from(per_page),
        nsfw_filter,
        params.content_type.as_deref(),
    )
    .await
    {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to search archives: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Search error").into_response();
        }
    };

    Json(ApiResponse {
        data: archives,
        page,
        per_page,
    })
    .into_response()
}

// ========== S3 File Serving ==========

async fn serve_s3_file(State(state): State<AppState>, Path(path): Path<String>) -> Response {
    // Path already contains the full path after /s3/, use it directly as S3 key
    let s3_key = &path;

    // Check if this is an archive-specific video that should redirect to canonical path
    if let Some(canonical_key) = try_get_canonical_video_redirect(state.db.pool(), s3_key).await {
        // Redirect to canonical video URL
        let redirect_url = format!("/s3/{}", canonical_key);
        return axum::response::Redirect::temporary(&redirect_url).into_response();
    }

    // Check if S3 is public (AWS S3) - if so, redirect to public URL
    if state.s3.is_public() {
        let public_url = state.s3.get_public_url(s3_key);
        return axum::response::Redirect::permanent(&public_url).into_response();
    }

    // For HTML files, prefer view.html over raw.html
    let mut final_key = s3_key.to_string();
    if s3_key.ends_with("/raw.html") {
        let view_key = s3_key.replace("/raw.html", "/view.html");
        // Check if view.html exists
        match state.s3.object_exists(&view_key).await {
            Ok(true) => {
                final_key = view_key;
            }
            Ok(false) => {
                // Fallback to raw.html
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to check if view.html exists, using raw.html");
            }
        }
    }

    // Download file from S3
    let (content, content_type) = match state.s3.download_file(&final_key).await {
        Ok((bytes, ct)) => (bytes, ct),
        Err(e) => {
            tracing::error!(key = %final_key, error = %e, "Failed to download file from S3");
            return (StatusCode::NOT_FOUND, "File not found").into_response();
        }
    };

    // Determine proper content type
    let mime_type = if content_type == "application/octet-stream" || content_type.is_empty() {
        // Try to guess from file extension
        mime_guess::from_path(&final_key)
            .first_or_octet_stream()
            .to_string()
    } else {
        content_type
    };

    // For HTML files, ensure charset is set
    let final_content_type = if mime_type.starts_with("text/html") {
        "text/html; charset=utf-8"
    } else {
        &mime_type
    };

    let content_disposition = suggest_content_disposition_filename(&final_key).map_or_else(
        || "inline".to_string(),
        |name| format!("inline; filename=\"{name}\""),
    );

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, final_content_type),
            (header::CONTENT_DISPOSITION, content_disposition.as_str()),
        ],
        content,
    )
        .into_response()
}

fn suggest_content_disposition_filename(s3_key: &str) -> Option<String> {
    // S3 keys typically look like: archives/{id}/media/view.html
    let filename = s3_key.rsplit('/').next()?;
    if filename.is_empty() {
        return None;
    }

    let archive_id = s3_key
        .strip_prefix("archives/")
        .and_then(|rest| rest.split('/').next())
        .and_then(|s| s.parse::<u64>().ok());

    let mut name = if let Some(id) = archive_id {
        format!("archive_{id}_{filename}")
    } else {
        filename.to_string()
    };

    // Sanitize for header safety.
    name = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();

    Some(name)
}

/// Check if an S3 key is an archive-specific video that should redirect to canonical.
///
/// Returns the canonical S3 key if this is a video alias, None otherwise.
async fn try_get_canonical_video_redirect(pool: &sqlx::SqlitePool, s3_key: &str) -> Option<String> {
    // Only process archive-specific paths
    if !s3_key.starts_with("archives/") {
        return None;
    }

    // Check if this is a video file
    let video_exts = ["mp4", "webm", "mkv", "mov", "avi"];
    let ext = s3_key.rsplit('.').next()?;
    if !video_exts.contains(&ext) {
        return None;
    }

    // Look up the artifact by S3 key
    let artifact = match find_artifact_by_s3_key(pool, s3_key).await {
        Ok(Some(a)) => a,
        Ok(None) => return None,
        Err(e) => {
            tracing::debug!(s3_key, error = %e, "Failed to find artifact by S3 key");
            return None;
        }
    };

    // Check if artifact has a video_file_id
    let video_file_id = artifact.video_file_id?;

    // Get the canonical video file
    let video_file = match get_video_file(pool, video_file_id).await {
        Ok(Some(vf)) => vf,
        Ok(None) => return None,
        Err(e) => {
            tracing::debug!(video_file_id, error = %e, "Failed to get video file");
            return None;
        }
    };

    // Only redirect if the canonical path is different
    if video_file.s3_key == s3_key {
        return None; // Already canonical
    }

    Some(video_file.s3_key)
}

// ========== Comment Handlers ==========

#[derive(Debug, Deserialize)]
struct CreateCommentForm {
    content: String,
}

#[derive(Debug, Deserialize)]
struct EditCommentForm {
    content: String,
}

async fn create_comment_handler(
    State(state): State<AppState>,
    Path(archive_id): Path<i64>,
    RequireApproved(user): RequireApproved,
    Form(form): Form<CreateCommentForm>,
) -> Response {
    tracing::debug!(
        archive_id,
        user_id = user.id,
        "HTTP API: POST /archive/:id/comment"
    );
    let content = form.content.trim();

    // Validate content length (max 5000 chars)
    if content.is_empty() || content.len() > 5000 {
        tracing::warn!(
            "Invalid comment length from user {}: {} chars",
            user.id,
            content.len()
        );
        return (StatusCode::BAD_REQUEST, "Comment must be 1-5000 characters").into_response();
    }

    // Verify archive exists
    if get_archive(state.db.pool(), archive_id)
        .await
        .ok()
        .flatten()
        .is_none()
    {
        return (StatusCode::NOT_FOUND, "Archive not found").into_response();
    }

    // Create comment
    match create_comment(state.db.pool(), archive_id, user.id, content).await {
        Ok(_comment_id) => {
            tracing::debug!(
                "Comment created by user {} on archive {}",
                user.id,
                archive_id
            );
            // Redirect back to archive page
            (
                StatusCode::SEE_OTHER,
                [("Location", format!("/archive/{}", archive_id).as_str())],
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to create comment: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create comment",
            )
                .into_response()
        }
    }
}

async fn create_reply_handler(
    State(state): State<AppState>,
    Path((archive_id, parent_comment_id)): Path<(i64, i64)>,
    RequireApproved(user): RequireApproved,
    Form(form): Form<CreateCommentForm>,
) -> Response {
    tracing::debug!(
        archive_id,
        parent_comment_id,
        user_id = user.id,
        "HTTP API: POST /archive/:id/comment/:comment_id/reply"
    );
    let content = form.content.trim();

    // Validate content length (max 5000 chars)
    if content.is_empty() || content.len() > 5000 {
        tracing::warn!(
            "Invalid reply length from user {}: {} chars",
            user.id,
            content.len()
        );
        return (StatusCode::BAD_REQUEST, "Reply must be 1-5000 characters").into_response();
    }

    // Verify parent comment exists and belongs to the archive
    match get_comment_with_author(state.db.pool(), parent_comment_id).await {
        Ok(Some(parent)) => {
            if parent.archive_id != archive_id {
                return (StatusCode::BAD_REQUEST, "Comment not on this archive").into_response();
            }
        }
        Ok(None) => return (StatusCode::NOT_FOUND, "Parent comment not found").into_response(),
        Err(e) => {
            tracing::error!("Failed to verify parent comment: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response();
        }
    }

    // Create reply
    match create_comment_reply(
        state.db.pool(),
        archive_id,
        user.id,
        parent_comment_id,
        content,
    )
    .await
    {
        Ok(_comment_id) => {
            tracing::debug!(
                "Reply created by user {} on comment {}",
                user.id,
                parent_comment_id
            );
            (
                StatusCode::SEE_OTHER,
                [("Location", format!("/archive/{}", archive_id).as_str())],
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to create reply: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create reply").into_response()
        }
    }
}

async fn edit_comment_handler(
    State(state): State<AppState>,
    Path((archive_id, comment_id)): Path<(i64, i64)>,
    RequireUser(user): RequireUser,
    Form(form): Form<EditCommentForm>,
) -> Response {
    tracing::debug!(
        archive_id,
        comment_id,
        user_id = user.id,
        "HTTP API: PUT /archive/:id/comment/:comment_id"
    );
    let content = form.content.trim();

    // Validate content length
    if content.is_empty() || content.len() > 5000 {
        return (StatusCode::BAD_REQUEST, "Comment must be 1-5000 characters").into_response();
    }

    // Check if user can edit
    match can_user_edit_comment(state.db.pool(), comment_id, user.id, user.is_admin).await {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!(
                "User {} attempted to edit comment {} but not allowed",
                user.id,
                comment_id
            );
            return (
                StatusCode::FORBIDDEN,
                "You can only edit your comments within 1 hour of creation",
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to check edit permission: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response();
        }
    }

    // Update comment
    match update_comment(state.db.pool(), comment_id, content, user.id).await {
        Ok(()) => {
            tracing::debug!("Comment {} edited by user {}", comment_id, user.id);
            (
                StatusCode::SEE_OTHER,
                [("Location", format!("/archive/{}", archive_id).as_str())],
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to update comment: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to update comment",
            )
                .into_response()
        }
    }
}

async fn delete_comment_handler(
    State(state): State<AppState>,
    Path((archive_id, comment_id)): Path<(i64, i64)>,
    RequireUser(user): RequireUser,
) -> Response {
    tracing::debug!(
        archive_id,
        comment_id,
        user_id = user.id,
        "HTTP API: DELETE /archive/:id/comment/:comment_id"
    );
    // Get comment to verify ownership
    let comment = match get_comment_with_author(state.db.pool(), comment_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return (StatusCode::NOT_FOUND, "Comment not found").into_response(),
        Err(e) => {
            tracing::error!("Failed to fetch comment: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response();
        }
    };

    // Check ownership or admin
    if comment.user_id != Some(user.id) && !user.is_admin {
        tracing::warn!(
            "User {} attempted to delete comment {} they don't own",
            user.id,
            comment_id
        );
        return (
            StatusCode::FORBIDDEN,
            "You can only delete your own comments",
        )
            .into_response();
    }

    // Soft delete
    match soft_delete_comment(state.db.pool(), comment_id, user.is_admin).await {
        Ok(()) => {
            tracing::debug!("Comment {} deleted by user {}", comment_id, user.id);
            (
                StatusCode::SEE_OTHER,
                [("Location", format!("/archive/{}", archive_id).as_str())],
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to delete comment: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to delete comment",
            )
                .into_response()
        }
    }
}

async fn pin_comment_handler(
    State(state): State<AppState>,
    Path((archive_id, comment_id)): Path<(i64, i64)>,
    RequireAdmin(user): RequireAdmin,
) -> Response {
    match pin_comment(state.db.pool(), comment_id, user.id).await {
        Ok(()) => {
            tracing::debug!("Comment {} pinned by admin {}", comment_id, user.id);
            (
                StatusCode::SEE_OTHER,
                [("Location", format!("/archive/{}", archive_id).as_str())],
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to pin comment: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to pin comment").into_response()
        }
    }
}

async fn unpin_comment_handler(
    State(state): State<AppState>,
    Path((archive_id, comment_id)): Path<(i64, i64)>,
    RequireAdmin(_user): RequireAdmin,
) -> Response {
    match unpin_comment(state.db.pool(), comment_id).await {
        Ok(()) => {
            tracing::debug!("Comment {} unpinned", comment_id);
            (
                StatusCode::SEE_OTHER,
                [("Location", format!("/archive/{}", archive_id).as_str())],
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to unpin comment: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to unpin comment").into_response()
        }
    }
}

async fn add_reaction_handler(
    State(state): State<AppState>,
    Path((archive_id, comment_id)): Path<(i64, i64)>,
    RequireApproved(user): RequireApproved,
) -> Response {
    match add_comment_reaction(state.db.pool(), comment_id, user.id).await {
        Ok(()) => {
            tracing::debug!(
                "User {} added helpful reaction to comment {}",
                user.id,
                comment_id
            );
            (
                StatusCode::SEE_OTHER,
                [("Location", format!("/archive/{}", archive_id).as_str())],
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to add reaction: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to add reaction").into_response()
        }
    }
}

async fn remove_reaction_handler(
    State(state): State<AppState>,
    Path((archive_id, comment_id)): Path<(i64, i64)>,
    RequireApproved(user): RequireApproved,
) -> Response {
    match remove_comment_reaction(state.db.pool(), comment_id, user.id).await {
        Ok(()) => {
            tracing::debug!(
                "User {} removed helpful reaction from comment {}",
                user.id,
                comment_id
            );
            (
                StatusCode::SEE_OTHER,
                [("Location", format!("/archive/{}", archive_id).as_str())],
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to remove reaction: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to remove reaction",
            )
                .into_response()
        }
    }
}

async fn comment_history_handler(
    State(state): State<AppState>,
    Path((_archive_id, comment_id)): Path<(i64, i64)>,
    RequireUser(_user): RequireUser,
) -> Response {
    match get_comment_edit_history(state.db.pool(), comment_id).await {
        Ok(edits) => {
            let markup = pages::render_comment_edit_history_page(&edits);
            Html(markup.into_string()).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to get edit history: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load edit history",
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_discourse_thread_url_with_post_number() {
        let mut url = url::Url::parse("https://discuss.example.com/t/topic-name/1491/16").unwrap();
        normalize_discourse_thread_url(&mut url);
        assert_eq!(
            url.as_str(),
            "https://discuss.example.com/t/topic-name/1491"
        );
    }

    #[test]
    fn test_normalize_discourse_thread_url_without_post_number() {
        let mut url = url::Url::parse("https://discuss.example.com/t/topic-name/1491").unwrap();
        normalize_discourse_thread_url(&mut url);
        assert_eq!(
            url.as_str(),
            "https://discuss.example.com/t/topic-name/1491"
        );
    }

    #[test]
    fn test_normalize_discourse_thread_url_with_query_and_fragment() {
        let mut url =
            url::Url::parse("https://discuss.example.com/t/topic-name/1491/16?foo=bar#reply")
                .unwrap();
        normalize_discourse_thread_url(&mut url);
        // The function only removes post number, not query/fragment
        assert_eq!(
            url.as_str(),
            "https://discuss.example.com/t/topic-name/1491?foo=bar#reply"
        );
    }

    #[test]
    fn test_normalize_discourse_thread_url_non_discourse_url() {
        let mut url = url::Url::parse("https://example.com/some/path/123").unwrap();
        normalize_discourse_thread_url(&mut url);
        // Should not modify non-Discourse URLs
        assert_eq!(url.as_str(), "https://example.com/some/path/123");
    }

    #[test]
    fn test_normalize_discourse_thread_url_with_large_post_number() {
        let mut url =
            url::Url::parse("https://discuss.example.com/t/topic-name/1491/999999").unwrap();
        normalize_discourse_thread_url(&mut url);
        assert_eq!(
            url.as_str(),
            "https://discuss.example.com/t/topic-name/1491"
        );
    }

    #[test]
    fn test_normalize_discourse_thread_url_with_trailing_slash() {
        let mut url = url::Url::parse("https://discuss.example.com/t/topic-name/1491/16/").unwrap();
        normalize_discourse_thread_url(&mut url);
        // The function removes both the post number and normalizes the trailing slash
        assert_eq!(
            url.as_str(),
            "https://discuss.example.com/t/topic-name/1491"
        );
    }

    #[test]
    fn test_normalize_discourse_thread_url_slug_with_numeric_ending() {
        // Slug ends with number - shouldn't be removed
        let mut url = url::Url::parse("https://discuss.example.com/t/topic-name-123/1491").unwrap();
        normalize_discourse_thread_url(&mut url);
        assert_eq!(
            url.as_str(),
            "https://discuss.example.com/t/topic-name-123/1491"
        );
    }

    #[test]
    fn test_domains_match_for_thread() {
        assert!(domains_match_for_thread(
            "discuss.example.com",
            "discuss.example.com"
        ));
        assert!(domains_match_for_thread(
            "www.discuss.example.com",
            "discuss.example.com"
        ));
        assert!(domains_match_for_thread(
            "discuss.example.com",
            "www.discuss.example.com"
        ));
        assert!(domains_match_for_thread(
            "Discuss.Example.COM",
            "discuss.example.com"
        ));
        assert!(!domains_match_for_thread(
            "discuss.example.com",
            "other.example.com"
        ));
    }
}
