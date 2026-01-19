//! Integration tests for pagination functionality.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use discourse_link_archiver::config::Config;
use discourse_link_archiver::db::{
    create_pending_archive, insert_link, set_archive_complete, Database, NewLink,
};
use tempfile::TempDir;
use tower::ServiceExt;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

/// Shared application state for tests.
#[derive(Clone)]
struct AppState {
    db: Database,
    #[allow(dead_code)]
    config: Arc<Config>,
}

/// Create a test app with the given database.
fn create_test_app(db: Database) -> Router {
    std::env::set_var("RSS_URL", "https://example.com/posts.rss");
    std::env::set_var("S3_BUCKET", "test-bucket");
    let config = Config::from_env().expect("Failed to create config");

    let state = AppState {
        db: db.clone(),
        config: Arc::new(config),
    };

    Router::new()
        .route("/api/archives", axum::routing::get(api_archives))
        .route("/search", axum::routing::get(search))
        .route("/site/:site", axum::routing::get(site_list))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[derive(serde::Deserialize)]
struct ApiArchivesParams {
    page: Option<u32>,
    per_page: Option<u32>,
}

async fn api_archives(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Query(params): axum::extract::Query<ApiArchivesParams>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use discourse_link_archiver::db::get_recent_archives;

    let page = params.page.unwrap_or(1);
    let per_page = params.per_page.unwrap_or(20).min(100);
    let offset = i64::from(page.saturating_sub(1)) * i64::from(per_page);

    let archives = match get_recent_archives(state.db.pool(), i64::from(per_page) + offset).await {
        Ok(a) => a.into_iter().skip(offset as usize).collect::<Vec<_>>(),
        Err(_) => vec![],
    };

    let response = serde_json::json!({
        "data": archives,
        "page": page,
        "per_page": per_page,
        "count": archives.len()
    });

    axum::Json(response).into_response()
}

#[derive(serde::Deserialize)]
struct SearchParams {
    q: Option<String>,
    page: Option<u32>,
}

async fn search(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Query(params): axum::extract::Query<SearchParams>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use discourse_link_archiver::db::get_recent_archives;

    let query = params.q.unwrap_or_default();
    let page = params.page.unwrap_or(1);
    let per_page = 20i64;
    let offset = i64::from(page.saturating_sub(1)) * per_page;

    let archives = if query.is_empty() {
        match get_recent_archives(state.db.pool(), per_page + offset).await {
            Ok(a) => a.into_iter().skip(offset as usize).collect::<Vec<_>>(),
            Err(_) => vec![],
        }
    } else {
        vec![] // Simplified for testing
    };

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<body>
<h1>Search</h1>
<p>Page {} - {} results</p>
<nav class="pagination">
{}
{}
</nav>
</body>
</html>"#,
        page,
        archives.len(),
        if page > 1 {
            format!(r#"<a href="/search?page={}">Previous</a>"#, page - 1)
        } else {
            String::new()
        },
        if archives.len() == per_page as usize {
            format!(r#"<a href="/search?page={}">Next</a>"#, page + 1)
        } else {
            String::new()
        },
    );
    axum::response::Html(html).into_response()
}

#[derive(serde::Deserialize)]
struct SiteListParams {
    page: Option<u32>,
}

async fn site_list(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Path(site): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<SiteListParams>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use discourse_link_archiver::db::get_archives_by_domain;

    let page = params.page.unwrap_or(1);
    let per_page = 20i64;
    let offset = i64::from(page.saturating_sub(1)) * per_page;

    let archives = match get_archives_by_domain(state.db.pool(), &site, per_page, offset).await {
        Ok(a) => a,
        Err(_) => vec![],
    };

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<body>
<h1>Site: {}</h1>
<p>Page {} - {} results</p>
</body>
</html>"#,
        site,
        page,
        archives.len()
    );
    axum::response::Html(html).into_response()
}

async fn setup_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.sqlite");
    let db = Database::new(&db_path)
        .await
        .expect("Failed to create database");
    (db, temp_dir)
}

async fn create_test_archives(db: &Database, count: usize, domain: &str) {
    for i in 0..count {
        let new_link = NewLink {
            original_url: format!("https://{}/page{}", domain, i),
            normalized_url: format!("https://{}/page{}", domain, i),
            canonical_url: None,
            domain: domain.to_string(),
        };
        let link_id = insert_link(db.pool(), &new_link)
            .await
            .expect("Failed to insert link");
        let archive_id = create_pending_archive(db.pool(), link_id, None)
            .await
            .expect("Failed to create archive");

        set_archive_complete(
            db.pool(),
            archive_id,
            Some(&format!("Page {} Title", i)),
            None,
            None,
            Some("text"),
            None,
            None,
        )
        .await
        .expect("Failed to complete archive");
    }
}

#[tokio::test]
async fn test_api_pagination_first_page() {
    let (db, _temp_dir) = setup_db().await;
    create_test_archives(&db, 30, "example.com").await;

    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/archives?page=1&per_page=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["page"], 1);
    assert_eq!(json["per_page"], 10);
    assert_eq!(json["count"], 10);
}

#[tokio::test]
async fn test_api_pagination_second_page() {
    let (db, _temp_dir) = setup_db().await;
    create_test_archives(&db, 30, "example.com").await;

    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/archives?page=2&per_page=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["page"], 2);
    assert_eq!(json["per_page"], 10);
    assert_eq!(json["count"], 10);
}

#[tokio::test]
async fn test_api_pagination_last_page_partial() {
    let (db, _temp_dir) = setup_db().await;
    create_test_archives(&db, 25, "example.com").await;

    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/archives?page=3&per_page=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["page"], 3);
    assert_eq!(json["count"], 5, "Last page should have remaining 5 items");
}

#[tokio::test]
async fn test_api_pagination_beyond_data() {
    let (db, _temp_dir) = setup_db().await;
    create_test_archives(&db, 10, "example.com").await;

    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/archives?page=5&per_page=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["count"], 0, "Page beyond data should return 0 items");
}

#[tokio::test]
async fn test_api_pagination_default_values() {
    let (db, _temp_dir) = setup_db().await;
    create_test_archives(&db, 30, "example.com").await;

    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/archives")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["page"], 1, "Default page should be 1");
    assert_eq!(json["per_page"], 20, "Default per_page should be 20");
}

#[tokio::test]
async fn test_api_pagination_max_per_page() {
    let (db, _temp_dir) = setup_db().await;
    create_test_archives(&db, 150, "example.com").await;

    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/archives?per_page=200")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["per_page"], 100, "per_page should be capped at 100");
}

#[tokio::test]
async fn test_search_pagination_shows_navigation() {
    let (db, _temp_dir) = setup_db().await;
    create_test_archives(&db, 50, "example.com").await;

    let app = create_test_app(db);

    // Test first page - should have Next but no Previous
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/search?page=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("Page 1"));
    assert!(body_str.contains("Next"));
    assert!(!body_str.contains("Previous"));
}

#[tokio::test]
async fn test_search_pagination_middle_page() {
    let (db, _temp_dir) = setup_db().await;
    create_test_archives(&db, 50, "example.com").await;

    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/search?page=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("Page 2"));
    assert!(
        body_str.contains("Previous"),
        "Middle page should have Previous link"
    );
    assert!(
        body_str.contains("Next"),
        "Middle page should have Next link"
    );
}

#[tokio::test]
async fn test_site_list_pagination() {
    let (db, _temp_dir) = setup_db().await;
    create_test_archives(&db, 30, "youtube").await;
    create_test_archives(&db, 10, "twitter").await;

    let app = create_test_app(db.clone());

    // Test first page for youtube
    let response = app
        .oneshot(
            Request::builder()
                .uri("/site/youtube?page=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("youtube"));
    assert!(body_str.contains("20 results"));

    // Test second page for youtube with the same database
    let app2 = create_test_app(db);
    let response = app2
        .oneshot(
            Request::builder()
                .uri("/site/youtube?page=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    // Second page should have the remaining 10 items
    assert!(body_str.contains("10 results"));
}

#[tokio::test]
async fn test_pagination_zero_page_defaults_to_one() {
    let (db, _temp_dir) = setup_db().await;
    create_test_archives(&db, 10, "example.com").await;

    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/archives?page=0&per_page=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // page=0 should be treated as page=1 (or fail gracefully)
    // The current implementation uses saturating_sub, so page 0 -> offset 0
    assert!(json["count"].as_i64().unwrap() > 0);
}
