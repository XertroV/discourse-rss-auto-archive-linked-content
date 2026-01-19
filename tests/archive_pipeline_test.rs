//! Integration tests for the archive pipeline.


use discourse_link_archiver::archiver::CookieOptions;
use discourse_link_archiver::config::Config;
use discourse_link_archiver::db::{
    create_pending_archive, get_archive, get_pending_archives, insert_link, set_archive_complete,
    set_archive_failed, set_archive_processing, Database, NewLink,
};
use discourse_link_archiver::handlers::HANDLERS;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Create a test configuration.
fn create_test_config(work_dir: &std::path::Path) -> Config {
    Config {
        work_dir: work_dir.to_path_buf(),
        ..Config::for_testing()
    }
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
async fn test_pending_archive_workflow() {
    let (db, _temp_dir) = setup_db().await;

    // Create a link
    let new_link = NewLink {
        original_url: "https://example.com/test-page".to_string(),
        normalized_url: "https://example.com/test-page".to_string(),
        canonical_url: None,
        domain: "example.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link)
        .await
        .expect("Failed to insert link");

    // Create pending archive
    let archive_id = create_pending_archive(db.pool(), link_id, None)
        .await
        .expect("Failed to create pending archive");

    // Verify pending archives can be fetched
    let pending = get_pending_archives(db.pool(), 10)
        .await
        .expect("Failed to get pending archives");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, archive_id);
    assert_eq!(pending[0].status, "pending");

    // Simulate processing
    set_archive_processing(db.pool(), archive_id)
        .await
        .expect("Failed to set processing");

    // Verify it's no longer in pending list
    let pending = get_pending_archives(db.pool(), 10)
        .await
        .expect("Failed to get pending archives");
    assert!(pending.is_empty());

    // Verify status changed
    let archive = get_archive(db.pool(), archive_id)
        .await
        .expect("Failed to get archive")
        .expect("Archive should exist");
    assert_eq!(archive.status, "processing");
}

#[tokio::test]
async fn test_archive_completion() {
    let (db, _temp_dir) = setup_db().await;

    // Create a link and archive
    let new_link = NewLink {
        original_url: "https://example.com/video".to_string(),
        normalized_url: "https://example.com/video".to_string(),
        canonical_url: None,
        domain: "example.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link)
        .await
        .expect("Failed to insert link");

    let archive_id = create_pending_archive(db.pool(), link_id, None)
        .await
        .expect("Failed to create pending archive");

    // Mark as processing then complete
    set_archive_processing(db.pool(), archive_id)
        .await
        .expect("Failed to set processing");

    set_archive_complete(
        db.pool(),
        archive_id,
        Some("Test Video Title"),
        Some("TestAuthor"),
        Some("This is test content for the video"),
        Some("video"),
        Some("archives/123/media/video.mp4"),
        Some("archives/123/thumb/thumb.jpg"),
    )
    .await
    .expect("Failed to set complete");

    // Verify completion
    let archive = get_archive(db.pool(), archive_id)
        .await
        .expect("Failed to get archive")
        .expect("Archive should exist");
    assert_eq!(archive.status, "complete");
    assert_eq!(archive.content_title.as_deref(), Some("Test Video Title"));
    assert_eq!(archive.content_author.as_deref(), Some("TestAuthor"));
    assert_eq!(archive.content_type.as_deref(), Some("video"));
    assert!(archive.s3_key_primary.is_some());
}

#[tokio::test]
async fn test_archive_failure() {
    let (db, _temp_dir) = setup_db().await;

    let new_link = NewLink {
        original_url: "https://example.com/broken".to_string(),
        normalized_url: "https://example.com/broken".to_string(),
        canonical_url: None,
        domain: "example.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link)
        .await
        .expect("Failed to insert link");

    let archive_id = create_pending_archive(db.pool(), link_id, None)
        .await
        .expect("Failed to create pending archive");

    set_archive_processing(db.pool(), archive_id)
        .await
        .expect("Failed to set processing");

    // Mark as failed
    set_archive_failed(db.pool(), archive_id, "HTTP 404 Not Found")
        .await
        .expect("Failed to set failed");

    // Verify failure state
    let archive = get_archive(db.pool(), archive_id)
        .await
        .expect("Failed to get archive")
        .expect("Archive should exist");
    assert_eq!(archive.status, "failed");
    assert_eq!(archive.error_message.as_deref(), Some("HTTP 404 Not Found"));
}

#[tokio::test]
async fn test_handler_registry_finds_handlers() {
    // Test that handler registry correctly identifies URLs
    assert!(
        HANDLERS
            .find_handler("https://www.youtube.com/watch?v=test")
            .is_some(),
        "Should find YouTube handler"
    );
    assert!(
        HANDLERS
            .find_handler("https://www.reddit.com/r/test")
            .is_some(),
        "Should find Reddit handler"
    );
    assert!(
        HANDLERS
            .find_handler("https://twitter.com/user/status/123")
            .is_some(),
        "Should find Twitter handler"
    );
    assert!(
        HANDLERS
            .find_handler("https://www.tiktok.com/@user/video/123")
            .is_some(),
        "Should find TikTok handler"
    );
    assert!(
        HANDLERS.find_handler("https://example.com/page").is_some(),
        "Should find generic handler for unknown URLs"
    );
}

#[tokio::test]
async fn test_handler_url_normalization() {
    // Test URL normalization for specific handlers
    let reddit_handler = HANDLERS
        .find_handler("https://www.reddit.com/r/test")
        .expect("Should find Reddit handler");
    let normalized = reddit_handler.normalize_url("https://www.reddit.com/r/test/comments/123");
    assert!(
        normalized.contains("old.reddit.com"),
        "Reddit URLs should be normalized to old.reddit.com"
    );

    // YouTube handler is found and can handle youtu.be URLs
    let youtube_handler = HANDLERS
        .find_handler("https://youtu.be/abc123")
        .expect("Should find YouTube handler");
    assert_eq!(youtube_handler.site_id(), "youtube");
}

#[tokio::test]
async fn test_generic_handler_archives_html() {
    let (_db, temp_dir) = setup_db().await;
    let work_dir = temp_dir.path().join("work");
    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work dir");

    // Start mock server with HTML content
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/test-page"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(
                    r#"<!DOCTYPE html>
<html>
<head>
    <title>Test Page Title</title>
    <meta property="og:description" content="This is the test description">
</head>
<body>
    <article>
        <p>This is the main content of the page.</p>
    </article>
</body>
</html>"#,
                    "text/html",
                )
                .insert_header("content-type", "text/html; charset=utf-8"),
        )
        .mount(&mock_server)
        .await;

    let url = format!("{}/test-page", mock_server.uri());

    // Create test config
    let config = create_test_config(&work_dir);

    // Find handler and archive
    let handler = HANDLERS.find_handler(&url).expect("Should find handler");
    let result = handler
        .archive(&url, &work_dir, &CookieOptions::default(), &config)
        .await
        .expect("Archive should succeed");

    assert_eq!(result.content_type, "text");
    assert_eq!(result.title.as_deref(), Some("Test Page Title"));
    assert!(result.text.is_some());

    // Verify HTML file was saved
    assert!(result.primary_file.is_some());
    let html_path = work_dir.join(result.primary_file.unwrap());
    assert!(html_path.exists(), "HTML file should be saved");
}

#[tokio::test]
async fn test_generic_handler_handles_non_html() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path().join("work");
    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work dir");

    // Start mock server with non-HTML content
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/data.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(r#"{"key": "value"}"#, "application/json")
                .insert_header("content-type", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let url = format!("{}/data.json", mock_server.uri());

    // Create test config
    let config = create_test_config(&work_dir);

    let handler = HANDLERS.find_handler(&url).expect("Should find handler");
    let result = handler
        .archive(&url, &work_dir, &CookieOptions::default(), &config)
        .await
        .expect("Archive should succeed");

    // Non-HTML content should return "file" type
    assert_eq!(result.content_type, "file");
}

#[tokio::test]
async fn test_generic_handler_handles_http_error() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path().join("work");
    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work dir");

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/not-found"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let url = format!("{}/not-found", mock_server.uri());

    // Create test config
    let config = create_test_config(&work_dir);

    let handler = HANDLERS.find_handler(&url).expect("Should find handler");
    let result = handler
        .archive(&url, &work_dir, &CookieOptions::default(), &config)
        .await;

    assert!(result.is_err(), "Should fail on HTTP 404");
}

#[tokio::test]
async fn test_multiple_pending_archives() {
    let (db, _temp_dir) = setup_db().await;

    // Create multiple links and archives
    let urls = vec![
        "https://example.com/page1",
        "https://example.com/page2",
        "https://example.com/page3",
        "https://example.com/page4",
        "https://example.com/page5",
    ];

    let mut archive_ids = Vec::new();
    for url in &urls {
        let new_link = NewLink {
            original_url: url.to_string(),
            normalized_url: url.to_string(),
            canonical_url: None,
            domain: "example.com".to_string(),
        };
        let link_id = insert_link(db.pool(), &new_link)
            .await
            .expect("Failed to insert link");
        let archive_id = create_pending_archive(db.pool(), link_id, None)
            .await
            .expect("Failed to create archive");
        archive_ids.push(archive_id);
    }

    // Fetch limited number of pending archives
    let pending = get_pending_archives(db.pool(), 3)
        .await
        .expect("Failed to get pending");
    assert_eq!(pending.len(), 3, "Should return at most 3 pending archives");

    // Process first one
    set_archive_processing(db.pool(), archive_ids[0])
        .await
        .expect("Failed to set processing");

    // Remaining pending should be 4
    let pending = get_pending_archives(db.pool(), 10)
        .await
        .expect("Failed to get pending");
    assert_eq!(pending.len(), 4, "Should have 4 pending after processing 1");
}

#[tokio::test]
async fn test_archive_with_metadata_extraction() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path().join("work");
    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work dir");

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/article"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(
                    r#"<!DOCTYPE html>
<html>
<head>
    <meta property="og:title" content="Article: How to Test">
    <meta property="og:description" content="A guide to testing">
    <meta name="author" content="Test Author">
</head>
<body>
    <article>
        <h1>How to Test Your Code</h1>
        <p>Testing is important for software quality.</p>
    </article>
</body>
</html>"#,
                    "text/html",
                )
                .insert_header("content-type", "text/html"),
        )
        .mount(&mock_server)
        .await;

    let url = format!("{}/article", mock_server.uri());

    // Create test config
    let config = create_test_config(&work_dir);

    let handler = HANDLERS.find_handler(&url).expect("Should find handler");
    let result = handler
        .archive(&url, &work_dir, &CookieOptions::default(), &config)
        .await
        .expect("Archive should succeed");

    // Verify metadata extraction
    assert_eq!(result.title.as_deref(), Some("Article: How to Test"));
    assert!(result
        .text
        .as_ref()
        .is_some_and(|t| t.contains("guide to testing")));
}
