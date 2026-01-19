use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Form;
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use super::feeds;
use super::templates;
use super::AppState;
use crate::db::{
    count_archives_by_status, count_links, count_posts, count_submissions_from_ip_last_hour,
    create_pending_archive, get_archive, get_archive_by_link_id, get_archives_by_domain,
    get_archives_for_post, get_link, get_link_by_normalized_url, get_post_by_guid,
    get_recent_archives, insert_link, insert_submission, search_archives,
    submission_exists_for_url, NewLink, NewSubmission,
};
use crate::handlers::normalize_url;

/// Create the router with all routes.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(home))
        .route("/search", get(search))
        .route("/submit", get(submit_form).post(submit_url))
        .route("/archive/{id}", get(archive_detail))
        .route("/post/{guid}", get(post_detail))
        .route("/site/{site}", get(site_list))
        .route("/stats", get(stats))
        .route("/healthz", get(health))
        .route("/feed.rss", get(feed_rss))
        .route("/feed.atom", get(feed_atom))
        .route("/api/archives", get(api_archives))
        .route("/api/search", get(api_search))
}

// ========== HTML Routes ==========

async fn home(State(state): State<AppState>) -> Response {
    let archives = match get_recent_archives(state.db.pool(), 20).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to fetch recent archives: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let html = templates::render_home(&archives);
    Html(html).into_response()
}

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    q: Option<String>,
    #[allow(dead_code)]
    site: Option<String>,
    page: Option<u32>,
}

async fn search(State(state): State<AppState>, Query(params): Query<SearchParams>) -> Response {
    let query = params.q.unwrap_or_default();
    let page = params.page.unwrap_or(1);
    let per_page = 20i64;
    let offset = i64::from(page.saturating_sub(1)) * per_page;

    let archives = if query.is_empty() {
        match get_recent_archives(state.db.pool(), per_page + offset).await {
            Ok(a) => a.into_iter().skip(offset as usize).collect(),
            Err(e) => {
                tracing::error!("Failed to fetch archives: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
            }
        }
    } else {
        match search_archives(state.db.pool(), &query, per_page).await {
            Ok(a) => a,
            Err(e) => {
                tracing::error!("Failed to search archives: {e}");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Search error").into_response();
            }
        }
    };

    let html = templates::render_search(&query, &archives, page);
    Html(html).into_response()
}

async fn archive_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Response {
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

    let html = templates::render_archive_detail(&archive, &link);
    Html(html).into_response()
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

    let archives = match get_archives_for_post(state.db.pool(), post.id).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to fetch archives for post: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let html = templates::render_post_detail(&post, &archives);
    Html(html).into_response()
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

    let archives = match get_archives_by_domain(state.db.pool(), &site, per_page, offset).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Failed to fetch archives by domain: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let html = templates::render_site_list(&site, &archives, page);
    Html(html).into_response()
}

async fn stats(State(state): State<AppState>) -> Response {
    let status_counts = match count_archives_by_status(state.db.pool()).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to count archives: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let link_count = count_links(state.db.pool()).await.unwrap_or(0);
    let post_count = count_posts(state.db.pool()).await.unwrap_or(0);

    let html = templates::render_stats(&status_counts, link_count, post_count);
    Html(html).into_response()
}

async fn health() -> &'static str {
    "OK"
}

// ========== Submission Routes ==========

async fn submit_form(State(state): State<AppState>) -> Response {
    // Check if submissions are enabled
    if !state.config.submission_enabled {
        let html = templates::render_submit_error("URL submissions are currently disabled.");
        return Html(html).into_response();
    }

    let html = templates::render_submit_form(None, None);
    Html(html).into_response()
}

#[derive(Debug, Deserialize)]
pub struct SubmitForm {
    url: String,
}

async fn submit_url(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Form(form): Form<SubmitForm>,
) -> Response {
    // Check if submissions are enabled
    if !state.config.submission_enabled {
        let html = templates::render_submit_error("URL submissions are currently disabled.");
        return Html(html).into_response();
    }

    let client_ip = addr.ip().to_string();

    // Rate limit check
    let rate_limit = state.config.submission_rate_limit_per_hour;
    match count_submissions_from_ip_last_hour(state.db.pool(), &client_ip).await {
        Ok(count) => {
            if count >= i64::from(rate_limit) {
                let html = templates::render_submit_form(
                    Some(&format!(
                        "Rate limit exceeded. Maximum {rate_limit} submissions per hour."
                    )),
                    None,
                );
                return Html(html).into_response();
            }
        }
        Err(e) => {
            tracing::error!("Failed to check rate limit: {e}");
            let html = templates::render_submit_form(Some("Internal error"), None);
            return Html(html).into_response();
        }
    }

    // Validate URL
    let url = form.url.trim();
    if url.is_empty() {
        let html = templates::render_submit_form(Some("URL is required"), None);
        return Html(html).into_response();
    }

    // Parse and validate URL
    let parsed_url = if let Ok(u) = url::Url::parse(url) {
        u
    } else {
        let html = templates::render_submit_form(Some("Invalid URL format"), None);
        return Html(html).into_response();
    };

    // Only allow http/https
    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        let html = templates::render_submit_form(Some("Only HTTP/HTTPS URLs are allowed"), None);
        return Html(html).into_response();
    }

    // Normalize URL
    let normalized = normalize_url(url);
    let domain = parsed_url.host_str().unwrap_or("unknown").to_string();

    // Check if this URL was submitted recently
    match submission_exists_for_url(state.db.pool(), &normalized).await {
        Ok(true) => {
            let html = templates::render_submit_form(
                Some("This URL was already submitted recently"),
                None,
            );
            return Html(html).into_response();
        }
        Ok(false) => {}
        Err(e) => {
            tracing::error!("Failed to check existing submission: {e}");
            let html = templates::render_submit_form(Some("Internal error"), None);
            return Html(html).into_response();
        }
    }

    // Create submission record
    let submission = NewSubmission {
        url: url.to_string(),
        normalized_url: normalized.clone(),
        submitted_by_ip: client_ip,
    };

    let submission_id = match insert_submission(state.db.pool(), &submission).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Failed to insert submission: {e}");
            let html = templates::render_submit_form(Some("Failed to save submission"), None);
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
                    let html = templates::render_submit_error("Failed to process URL");
                    return Html(html).into_response();
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to check existing link: {e}");
            let html = templates::render_submit_error("Internal error");
            return Html(html).into_response();
        }
    };

    // Check if archive already exists
    match get_archive_by_link_id(state.db.pool(), link_id).await {
        Ok(Some(_)) => {
            // Archive already exists, show success with note
            let html = templates::render_submit_form(
                None,
                Some("This URL has already been archived. Check the search for results."),
            );
            return Html(html).into_response();
        }
        Ok(None) => {
            // Create pending archive
            if let Err(e) = create_pending_archive(state.db.pool(), link_id).await {
                tracing::error!("Failed to create pending archive: {e}");
                let html = templates::render_submit_error("Failed to queue for archiving");
                return Html(html).into_response();
            }
        }
        Err(e) => {
            tracing::error!("Failed to check existing archive: {e}");
            let html = templates::render_submit_error("Internal error");
            return Html(html).into_response();
        }
    }

    tracing::info!(
        submission_id = submission_id,
        url = %normalized,
        "New URL submitted for archiving"
    );

    let html = templates::render_submit_success(submission_id);
    Html(html).into_response()
}

// ========== Feed Routes ==========

#[derive(Debug, Deserialize)]
pub struct FeedParams {
    #[allow(dead_code)]
    site: Option<String>,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    content_type: Option<String>,
    limit: Option<i64>,
}

async fn feed_rss(State(state): State<AppState>, Query(params): Query<FeedParams>) -> Response {
    let limit = params.limit.unwrap_or(50).min(100);

    // TODO: Add site and content_type filtering once we have the query functions
    let archives = match get_recent_archives(state.db.pool(), limit).await {
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

    // TODO: Add site and content_type filtering once we have the query functions
    let archives = match get_recent_archives(state.db.pool(), limit).await {
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

#[derive(Debug, Deserialize)]
pub struct ApiArchivesParams {
    page: Option<u32>,
    per_page: Option<u32>,
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

    let archives = match get_recent_archives(state.db.pool(), i64::from(per_page) + offset).await {
        Ok(a) => a.into_iter().skip(offset as usize).collect::<Vec<_>>(),
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

#[derive(Debug, Deserialize)]
pub struct ApiSearchParams {
    q: String,
    page: Option<u32>,
    per_page: Option<u32>,
}

async fn api_search(
    State(state): State<AppState>,
    Query(params): Query<ApiSearchParams>,
) -> Response {
    let page = params.page.unwrap_or(1);
    let per_page = params.per_page.unwrap_or(20).min(100);

    let archives = match search_archives(state.db.pool(), &params.q, i64::from(per_page)).await {
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
