//! Integration tests for web routes.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use discourse_link_archiver::config::Config;
use discourse_link_archiver::db::{
    create_pending_archive, insert_link, insert_post, set_archive_complete, Database, NewLink,
    NewPost,
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
    // Create a minimal config for tests
    std::env::set_var("RSS_URL", "https://example.com/posts.rss");
    std::env::set_var("S3_BUCKET", "test-bucket");
    let config = Config::from_env().expect("Failed to create config");

    let state = AppState {
        db: db.clone(),
        config: Arc::new(config),
    };

    // Build the router manually for testing
    Router::new()
        .route("/", axum::routing::get(home))
        .route("/search", axum::routing::get(search))
        .route("/stats", axum::routing::get(stats))
        .route("/healthz", axum::routing::get(health))
        .route("/archive/:id", axum::routing::get(archive_detail))
        .route("/archive/:id/rearchive", axum::routing::post(rearchive))
        .route("/post/:guid", axum::routing::get(post_detail))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

// Re-implement route handlers for testing since we need direct access
async fn home(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use discourse_link_archiver::db::get_recent_archives;

    let archives = match get_recent_archives(state.db.pool(), 20).await {
        Ok(a) => a,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let html = format!(
        r#"<!DOCTYPE html><html><body><h1>Recent Archives</h1><p>{} archives</p></body></html>"#,
        archives.len()
    );
    axum::response::Html(html).into_response()
}

#[derive(serde::Deserialize)]
struct SearchParams {
    q: Option<String>,
}

async fn search(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Query(params): axum::extract::Query<SearchParams>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use discourse_link_archiver::db::{get_recent_archives, search_archives};

    let query = params.q.unwrap_or_default();
    let archives = if query.is_empty() {
        get_recent_archives(state.db.pool(), 20)
            .await
            .unwrap_or_default()
    } else {
        search_archives(state.db.pool(), &query, 20)
            .await
            .unwrap_or_default()
    };

    let html = format!(
        r#"<!DOCTYPE html><html><body><h1>Search</h1><p>Query: {}, {} results</p></body></html>"#,
        query,
        archives.len()
    );
    axum::response::Html(html).into_response()
}

async fn stats(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use discourse_link_archiver::db::{count_archives_by_status, count_links, count_posts};

    let _status_counts = count_archives_by_status(state.db.pool())
        .await
        .unwrap_or_default();
    let link_count = count_links(state.db.pool()).await.unwrap_or(0);
    let post_count = count_posts(state.db.pool()).await.unwrap_or(0);

    let html = format!(
        r#"<!DOCTYPE html><html><body><h1>Stats</h1><p>Posts: {}, Links: {}</p></body></html>"#,
        post_count, link_count
    );
    axum::response::Html(html).into_response()
}

async fn health() -> &'static str {
    "OK"
}

async fn archive_detail(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use discourse_link_archiver::db::get_archive;

    let archive = match get_archive(state.db.pool(), id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Archive not found").into_response();
        }
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let html = format!(
        r#"<!DOCTYPE html><html><body><h1>Archive {}</h1><p>Status: {}</p></body></html>"#,
        archive.id, archive.status
    );
    axum::response::Html(html).into_response()
}

/// Handler for re-archiving an archive (POST /archive/:id/rearchive).
///
/// Mirrors the real web route behavior: resets the archive to pending state.
async fn rearchive(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use discourse_link_archiver::db::{get_archive, reset_archive_for_rearchive};

    let archive = match get_archive(state.db.pool(), id).await {
        Ok(Some(a)) => a,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Archive not found").into_response();
        }
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    if archive.status == "processing" {
        return (
            StatusCode::CONFLICT,
            "Archive is currently being processed. Please wait.",
        )
            .into_response();
    }

    if let Err(_) = reset_archive_for_rearchive(state.db.pool(), id).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to reset archive",
        )
            .into_response();
    }

    axum::response::Redirect::to(&format!("/archive/{id}")).into_response()
}

async fn post_detail(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Path(guid): axum::extract::Path<String>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use discourse_link_archiver::db::get_post_by_guid;

    let post = match get_post_by_guid(state.db.pool(), &guid).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return (StatusCode::NOT_FOUND, "Post not found").into_response();
        }
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    let html = format!(
        r#"<!DOCTYPE html><html><body><h1>Post</h1><p>GUID: {}</p></body></html>"#,
        post.guid
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

#[tokio::test]
async fn test_rearchive_failed_archive_with_no_artifacts() {
    use discourse_link_archiver::db::{get_archive, set_archive_failed};

    let (db, _temp_dir) = setup_db().await;

    // Create a link + archive with a failed status (no artifacts inserted)
    let new_link = NewLink {
        original_url: "https://reddit.com/r/test/comments/abc123/test".to_string(),
        normalized_url: "https://reddit.com/r/test/comments/abc123/test".to_string(),
        canonical_url: None,
        domain: "reddit.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link).await.unwrap();
    let archive_id = create_pending_archive(db.pool(), link_id).await.unwrap();
    set_archive_failed(db.pool(), archive_id, "boom").await.unwrap();

    let app = create_test_app(db.clone());

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(&format!("/archive/{archive_id}/rearchive"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response.status().is_redirection(),
        "rearchive should redirect on success"
    );

    let updated = get_archive(db.pool(), archive_id)
        .await
        .unwrap()
        .expect("archive should still exist");
    assert_eq!(updated.status, "pending");
    assert!(updated.error_message.is_none());
    assert_eq!(updated.retry_count, 0);
}

#[tokio::test]
async fn test_health_endpoint() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(&body[..], b"OK");
}

#[tokio::test]
async fn test_home_page() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db);

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("Recent Archives"));
}

#[tokio::test]
async fn test_search_page_empty_query() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/search")
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
    assert!(body_str.contains("Search"));
}

#[tokio::test]
async fn test_search_page_with_query() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/search?q=test")
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
    assert!(body_str.contains("Query: test"));
}

#[tokio::test]
async fn test_stats_page() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/stats")
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
    assert!(body_str.contains("Stats"));
    assert!(body_str.contains("Posts: 0"));
    assert!(body_str.contains("Links: 0"));
}

#[tokio::test]
async fn test_archive_detail_not_found() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/archive/9999")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_archive_detail_found() {
    let (db, _temp_dir) = setup_db().await;

    // Create a link and archive
    let new_link = NewLink {
        original_url: "https://example.com/test-archive".to_string(),
        normalized_url: "https://example.com/test-archive".to_string(),
        canonical_url: None,
        domain: "example.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link).await.unwrap();
    let archive_id = create_pending_archive(db.pool(), link_id).await.unwrap();

    // Complete the archive
    set_archive_complete(
        db.pool(),
        archive_id,
        Some("Test Archive Title"),
        Some("Test Author"),
        Some("Test description"),
        Some("text"),
        None,
        None,
    )
    .await
    .unwrap();

    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri(&format!("/archive/{archive_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Archive detail should return 200 for existing archive"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains(&format!("Archive {archive_id}")),
        "Response should contain archive ID"
    );
    assert!(body_str.contains("complete"), "Response should show status");
}

#[tokio::test]
async fn test_post_detail_not_found() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/post/nonexistent-guid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_post_detail_found() {
    let (db, _temp_dir) = setup_db().await;

    // Create a post
    let new_post = NewPost {
        guid: "test-post-guid-12345".to_string(),
        discourse_url: "https://forum.example.com/t/test/1".to_string(),
        author: Some("test_user".to_string()),
        title: Some("Test Post Title".to_string()),
        body_html: Some("<p>Test body content</p>".to_string()),
        content_hash: Some("abc123".to_string()),
        published_at: None,
    };
    insert_post(db.pool(), &new_post).await.unwrap();

    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/post/test-post-guid-12345")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Post detail should return 200 for existing post"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("test-post-guid-12345"),
        "Response should contain post GUID"
    );
}

#[tokio::test]
async fn test_stats_with_data() {
    let (db, _temp_dir) = setup_db().await;

    // Create some test data
    let new_post = NewPost {
        guid: "stats-test-post".to_string(),
        discourse_url: "https://forum.example.com/t/stats/1".to_string(),
        author: Some("user".to_string()),
        title: Some("Stats Test".to_string()),
        body_html: None,
        content_hash: None,
        published_at: None,
    };
    insert_post(db.pool(), &new_post).await.unwrap();

    let new_link = NewLink {
        original_url: "https://example.com/stats".to_string(),
        normalized_url: "https://example.com/stats".to_string(),
        canonical_url: None,
        domain: "example.com".to_string(),
    };
    insert_link(db.pool(), &new_link).await.unwrap();

    let app = create_test_app(db);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/stats")
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
    assert!(body_str.contains("Posts: 1"));
    assert!(body_str.contains("Links: 1"));
}

#[tokio::test]
async fn test_search_returns_relevant_results() {
    let (db, _temp_dir) = setup_db().await;

    // Create a link and archive with searchable content
    let new_link = NewLink {
        original_url: "https://youtube.com/watch?v=test123".to_string(),
        normalized_url: "https://www.youtube.com/watch?v=test123".to_string(),
        canonical_url: None,
        domain: "www.youtube.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link).await.unwrap();

    // Create pending archive
    let archive_id = create_pending_archive(db.pool(), link_id).await.unwrap();

    // Complete the archive with searchable content
    set_archive_complete(
        db.pool(),
        archive_id,
        Some("Rust Programming Tutorial for Beginners"),
        Some("TechChannel"),
        Some("Learn Rust programming from scratch in this comprehensive tutorial"),
        Some("video"),
        Some("archives/youtube/test123.mp4"),
        None,
    )
    .await
    .unwrap();

    let app = create_test_app(db);

    // Search for "Rust" - should find our archive
    let response = app
        .oneshot(
            Request::builder()
                .uri("/search?q=Rust")
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
    assert!(
        body_str.contains("1 results"),
        "Should find 1 result for 'Rust'"
    );
}

#[tokio::test]
async fn test_search_no_results_for_unmatched_query() {
    let (db, _temp_dir) = setup_db().await;

    // Create a link and archive with searchable content
    let new_link = NewLink {
        original_url: "https://youtube.com/watch?v=abc".to_string(),
        normalized_url: "https://www.youtube.com/watch?v=abc".to_string(),
        canonical_url: None,
        domain: "www.youtube.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link).await.unwrap();
    let archive_id = create_pending_archive(db.pool(), link_id).await.unwrap();

    set_archive_complete(
        db.pool(),
        archive_id,
        Some("Python Tutorial"),
        Some("PythonDev"),
        Some("Learn Python programming"),
        Some("video"),
        None,
        None,
    )
    .await
    .unwrap();

    let app = create_test_app(db);

    // Search for something that doesn't exist
    let response = app
        .oneshot(
            Request::builder()
                .uri("/search?q=JavaScript")
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
    assert!(
        body_str.contains("0 results"),
        "Should find 0 results for 'JavaScript'"
    );
}

#[tokio::test]
async fn test_search_by_author() {
    let (db, _temp_dir) = setup_db().await;

    let new_link = NewLink {
        original_url: "https://twitter.com/user/status/123".to_string(),
        normalized_url: "https://twitter.com/user/status/123".to_string(),
        canonical_url: None,
        domain: "twitter.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link).await.unwrap();
    let archive_id = create_pending_archive(db.pool(), link_id).await.unwrap();

    set_archive_complete(
        db.pool(),
        archive_id,
        Some("Tweet about technology"),
        Some("TechInfluencer"),
        Some("Great insights about tech"),
        Some("text"),
        None,
        None,
    )
    .await
    .unwrap();

    let app = create_test_app(db);

    // Search by author name
    let response = app
        .oneshot(
            Request::builder()
                .uri("/search?q=TechInfluencer")
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
    assert!(
        body_str.contains("1 results"),
        "Should find 1 result when searching by author"
    );
}

#[tokio::test]
async fn test_home_page_with_archives() {
    let (db, _temp_dir) = setup_db().await;

    // Create multiple archives
    for i in 0..3 {
        let new_link = NewLink {
            original_url: format!("https://example.com/page{i}"),
            normalized_url: format!("https://example.com/page{i}"),
            canonical_url: None,
            domain: "example.com".to_string(),
        };
        let link_id = insert_link(db.pool(), &new_link).await.unwrap();
        let archive_id = create_pending_archive(db.pool(), link_id).await.unwrap();

        set_archive_complete(
            db.pool(),
            archive_id,
            Some(&format!("Page {i} Title")),
            None,
            None,
            Some("text"),
            None,
            None,
        )
        .await
        .unwrap();
    }

    let app = create_test_app(db);

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        body_str.contains("3 archives"),
        "Home page should show 3 archives"
    );
}
