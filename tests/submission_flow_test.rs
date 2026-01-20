//! Integration tests for the URL submission flow.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use discourse_link_archiver::config::Config;
use discourse_link_archiver::db::{
    count_submissions_from_ip_last_hour, get_link_by_normalized_url, get_pending_archives,
    insert_submission, submission_exists_for_url, Database, NewSubmission,
};
use tempfile::TempDir;
use tower::ServiceExt;

/// Shared application state for tests.
#[derive(Clone)]
struct AppState {
    db: Database,
    config: Arc<Config>,
}

/// Create a test app with the given database.
fn create_test_app(db: Database, submission_enabled: bool, rate_limit: u32) -> Router {
    std::env::set_var("RSS_URL", "https://example.com/posts.rss");
    std::env::set_var("S3_BUCKET", "test-bucket");
    std::env::set_var(
        "SUBMISSION_ENABLED",
        if submission_enabled { "true" } else { "false" },
    );
    std::env::set_var("SUBMISSION_RATE_LIMIT_PER_HOUR", rate_limit.to_string());
    let config = Config::from_env().expect("Failed to create config");

    let state = AppState {
        db: db.clone(),
        config: Arc::new(config),
    };

    Router::new()
        .route("/submit", axum::routing::get(submit_form).post(submit_url))
        .with_state(state)
}

async fn submit_form(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    if !state.config.submission_enabled {
        return (StatusCode::OK, "Submissions disabled").into_response();
    }

    (StatusCode::OK, "Submit form").into_response()
}

#[derive(serde::Deserialize)]
struct SubmitForm {
    url: String,
}

async fn submit_url(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Form(form): axum::extract::Form<SubmitForm>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    use discourse_link_archiver::db::{create_pending_archive, insert_link, NewLink};
    use discourse_link_archiver::handlers::normalize_url;

    if !state.config.submission_enabled {
        return (StatusCode::OK, "Submissions disabled").into_response();
    }

    // Simulate rate limiting with a fixed IP for tests
    let client_ip = "127.0.0.1".to_string();
    let rate_limit = state.config.submission_rate_limit_per_hour;
    match count_submissions_from_ip_last_hour(state.db.pool(), &client_ip).await {
        Ok(count) => {
            if count >= i64::from(rate_limit) {
                return (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded").into_response();
            }
        }
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Error").into_response();
        }
    }

    // Validate URL
    let url = form.url.trim();
    if url.is_empty() {
        return (StatusCode::BAD_REQUEST, "URL required").into_response();
    }

    let parsed_url = match url::Url::parse(url) {
        Ok(u) => u,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Invalid URL").into_response();
        }
    };

    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        return (StatusCode::BAD_REQUEST, "Only HTTP/HTTPS allowed").into_response();
    }

    let normalized = normalize_url(url);
    let domain = parsed_url.host_str().unwrap_or("unknown").to_string();

    // Check for duplicate submission
    match submission_exists_for_url(state.db.pool(), &normalized).await {
        Ok(true) => {
            return (StatusCode::CONFLICT, "Already submitted").into_response();
        }
        Ok(false) => {}
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Error").into_response();
        }
    }

    // Create submission record
    let submission = NewSubmission {
        url: url.to_string(),
        normalized_url: normalized.clone(),
        submitted_by_ip: client_ip,
        submitted_by_user_id: None,
    };

    if insert_submission(state.db.pool(), &submission)
        .await
        .is_err()
    {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save").into_response();
    }

    // Create or find link
    let link_id = match get_link_by_normalized_url(state.db.pool(), &normalized).await {
        Ok(Some(link)) => link.id,
        Ok(None) => {
            let new_link = NewLink {
                original_url: url.to_string(),
                normalized_url: normalized.clone(),
                canonical_url: None,
                domain,
            };
            match insert_link(state.db.pool(), &new_link).await {
                Ok(id) => id,
                Err(_) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create link")
                        .into_response();
                }
            }
        }
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, "Error").into_response();
        }
    };

    // Create pending archive
    if let Err(_) = create_pending_archive(state.db.pool(), link_id, None).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to queue").into_response();
    }

    (StatusCode::OK, "Submitted successfully").into_response()
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
async fn test_submit_form_when_enabled() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db, true, 10);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/submit")
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
    assert!(body_str.contains("Submit form"));
}

#[tokio::test]
async fn test_submit_form_when_disabled() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db, false, 10);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/submit")
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
    assert!(body_str.contains("disabled"));
}

#[tokio::test]
async fn test_submit_valid_url() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db.clone(), true, 10);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/submit")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("url=https://www.youtube.com/watch?v=test123"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify link was created
    let link = get_link_by_normalized_url(db.pool(), "https://www.youtube.com/watch?v=test123")
        .await
        .expect("DB error");
    assert!(link.is_some(), "Link should be created");

    // Verify pending archive was created
    let pending = get_pending_archives(db.pool(), 10).await.expect("DB error");
    assert_eq!(pending.len(), 1, "Should have 1 pending archive");
}

#[tokio::test]
async fn test_submit_invalid_url() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db, true, 10);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/submit")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("url=not-a-valid-url"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_submit_empty_url() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db, true, 10);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/submit")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("url="))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_submit_non_http_url() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db, true, 10);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/submit")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("url=ftp://example.com/file"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("HTTP/HTTPS"));
}

#[tokio::test]
async fn test_submit_duplicate_url() {
    let (db, _temp_dir) = setup_db().await;

    // First submission
    let submission = NewSubmission {
        url: "https://example.com/page".to_string(),
        normalized_url: "https://example.com/page".to_string(),
        submitted_by_ip: "127.0.0.1".to_string(),
        submitted_by_user_id: None,
    };
    insert_submission(db.pool(), &submission)
        .await
        .expect("Failed to insert submission");

    let app = create_test_app(db, true, 10);

    // Try to submit the same URL again
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/submit")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("url=https://example.com/page"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("Already submitted"));
}

#[tokio::test]
async fn test_submit_rate_limiting() {
    let (db, _temp_dir) = setup_db().await;

    // Create submissions to hit rate limit (limit is 2)
    for i in 0..2 {
        let submission = NewSubmission {
            url: format!("https://example.com/page{i}"),
            normalized_url: format!("https://example.com/page{i}"),
            submitted_by_ip: "127.0.0.1".to_string(),
            submitted_by_user_id: None,
        };
        insert_submission(db.pool(), &submission)
            .await
            .expect("Failed to insert submission");
    }

    let app = create_test_app(db, true, 2);

    // Try to submit another URL - should be rate limited
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/submit")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("url=https://example.com/new-page"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn test_submit_when_disabled() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db.clone(), false, 10);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/submit")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from("url=https://example.com/page"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("disabled"));

    // Verify no pending archives were created
    let pending = get_pending_archives(db.pool(), 10).await.expect("DB error");
    assert!(
        pending.is_empty(),
        "No archives should be created when disabled"
    );
}

#[tokio::test]
async fn test_submit_normalizes_url() {
    let (db, _temp_dir) = setup_db().await;
    let app = create_test_app(db.clone(), true, 10);

    // Submit URL with tracking parameters
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/submit")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(
                    "url=https://example.com/page?utm_source=test&utm_medium=social",
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // The link should be stored with normalized URL (tracking params removed)
    let link = get_link_by_normalized_url(db.pool(), "https://example.com/page")
        .await
        .expect("DB error");
    assert!(link.is_some(), "Link should be created with normalized URL");
}

#[tokio::test]
async fn test_submission_count_tracking() {
    let (db, _temp_dir) = setup_db().await;

    // Insert some submissions
    let client_ip = "192.168.1.1";
    for i in 0..5 {
        let submission = NewSubmission {
            url: format!("https://example.com/page{i}"),
            normalized_url: format!("https://example.com/page{i}"),
            submitted_by_ip: client_ip.to_string(),
            submitted_by_user_id: None,
        };
        insert_submission(db.pool(), &submission)
            .await
            .expect("Failed to insert");
    }

    // Count submissions from this IP
    let count = count_submissions_from_ip_last_hour(db.pool(), client_ip)
        .await
        .expect("Failed to count");
    assert_eq!(count, 5);

    // Count from different IP
    let count = count_submissions_from_ip_last_hour(db.pool(), "10.0.0.1")
        .await
        .expect("Failed to count");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_submission_exists_check() {
    let (db, _temp_dir) = setup_db().await;

    let url = "https://example.com/unique-page";

    // Should not exist initially
    let exists = submission_exists_for_url(db.pool(), url)
        .await
        .expect("DB error");
    assert!(!exists);

    // Create submission
    let submission = NewSubmission {
        url: url.to_string(),
        normalized_url: url.to_string(),
        submitted_by_ip: "127.0.0.1".to_string(),
        submitted_by_user_id: None,
    };
    insert_submission(db.pool(), &submission)
        .await
        .expect("Failed to insert");

    // Should exist now
    let exists = submission_exists_for_url(db.pool(), url)
        .await
        .expect("DB error");
    assert!(exists);
}
