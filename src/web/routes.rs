use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};

use super::templates;
use super::AppState;
use crate::db::{
    count_archives_by_status, count_links, count_posts, get_archive, get_archives_by_domain,
    get_link, get_recent_archives, search_archives, Archive,
};

/// Create the router with all routes.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(home))
        .route("/search", get(search))
        .route("/archive/{id}", get(archive_detail))
        .route("/site/{site}", get(site_list))
        .route("/stats", get(stats))
        .route("/healthz", get(health))
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

    let archives =
        match get_recent_archives(state.db.pool(), i64::from(per_page) + offset).await {
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
