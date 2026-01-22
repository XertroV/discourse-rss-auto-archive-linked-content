//! Integration tests for RSS polling.

use std::time::Duration;

use discourse_link_archiver::config::Config;
use discourse_link_archiver::db::{
    get_link_by_normalized_url, get_pending_archives, get_post_by_guid, Database,
};
use discourse_link_archiver::rss::poll_once;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Create a test configuration with the given RSS URL.
fn create_test_config(rss_url: &str, work_dir: &std::path::Path) -> Config {
    Config {
        rss_url: rss_url.to_string(),
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

/// Sample JSON feed with a single post containing links.
const SAMPLE_JSON: &str = r#"{
  "latest_posts": [
    {
      "id": 123,
      "post_number": 1,
      "username": "testuser",
      "topic_id": 100,
      "topic_slug": "test-post",
      "topic_title": "Test Post with Links",
      "created_at": "2024-01-01T12:00:00.000Z",
      "updated_at": "2024-01-01T12:00:00.000Z",
      "cooked": "<p>Check out this video: <a href=\"https://www.youtube.com/watch?v=dQw4w9WgXcQ\">YouTube Link</a></p><p>And this reddit post: <a href=\"https://www.reddit.com/r/test/comments/abc123\">Reddit</a></p>",
      "post_url": "/t/test-post/100/1"
    }
  ]
}"#;

/// JSON feed with multiple posts.
const MULTI_POST_JSON: &str = r#"{
  "latest_posts": [
    {
      "id": 1,
      "post_number": 1,
      "username": "testuser",
      "topic_id": 101,
      "topic_slug": "first",
      "topic_title": "First Post",
      "created_at": "2024-01-01T12:00:00.000Z",
      "updated_at": "2024-01-01T12:00:00.000Z",
      "cooked": "<p>Link: <a href=\"https://twitter.com/user/status/123\">Tweet</a></p>",
      "post_url": "/t/first/101/1"
    },
    {
      "id": 2,
      "post_number": 1,
      "username": "testuser",
      "topic_id": 102,
      "topic_slug": "second",
      "topic_title": "Second Post",
      "created_at": "2024-01-01T12:05:00.000Z",
      "updated_at": "2024-01-01T12:05:00.000Z",
      "cooked": "<p>Link: <a href=\"https://www.tiktok.com/@user/video/456\">TikTok</a></p>",
      "post_url": "/t/second/102/1"
    }
  ]
}"#;

/// JSON feed with a post containing a quoted link.
const QUOTED_LINK_JSON: &str = r#"{
  "latest_posts": [
    {
      "id": 3,
      "post_number": 1,
      "username": "testuser",
      "topic_id": 103,
      "topic_slug": "quoted",
      "topic_title": "Post with Quoted Link",
      "created_at": "2024-01-01T12:00:00.000Z",
      "updated_at": "2024-01-01T12:00:00.000Z",
      "cooked": "<aside class=\"quote\"><p>Someone said: <a href=\"https://example.com/quoted-link\">link</a></p></aside><p>My response here.</p>",
      "post_url": "/t/quoted/103/1"
    }
  ]
}"#;

#[tokio::test]
async fn test_poll_once_processes_new_posts() {
    let (db, temp_dir) = setup_db().await;

    // Start mock server
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_JSON, "application/json"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(
        &format!("{}/posts.json", mock_server.uri()),
        temp_dir.path(),
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    // Poll once
    let new_count = poll_once(&client, &config, &db)
        .await
        .expect("poll_once failed");

    // Should have processed 1 new post
    assert_eq!(new_count, 1);

    // Verify the post was inserted
    // Query all posts to see what GUIDs are actually stored
    let all_posts: Vec<(String,)> = sqlx::query_as("SELECT guid FROM posts")
        .fetch_all(db.pool())
        .await
        .expect("Failed to query posts");

    eprintln!("Mock server URI: {}", mock_server.uri());
    eprintln!("All GUIDs in database: {:?}", all_posts);

    // The GUID should match the pattern {domain}-post-{id}
    // where domain is extracted from the RSS URL
    assert_eq!(all_posts.len(), 1, "Should have exactly one post");
    let guid = &all_posts[0].0;
    assert!(
        guid.ends_with("-post-123"),
        "GUID should end with -post-123, got: {}",
        guid
    );

    let post = get_post_by_guid(db.pool(), guid)
        .await
        .expect("Database error")
        .expect("Post not found");
    assert_eq!(post.title.as_deref(), Some("Test Post with Links"));
}

#[tokio::test]
async fn test_poll_once_extracts_links() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_JSON, "application/json"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(
        &format!("{}/posts.json", mock_server.uri()),
        temp_dir.path(),
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    poll_once(&client, &config, &db)
        .await
        .expect("poll_once failed");

    // Check that YouTube link was extracted and normalized
    let youtube_link =
        get_link_by_normalized_url(db.pool(), "https://www.youtube.com/watch?v=dQw4w9WgXcQ")
            .await
            .expect("Database error");
    assert!(youtube_link.is_some(), "YouTube link should be extracted");

    // Check that Reddit link was extracted (generic normalization converts www.reddit.com to old.reddit.com)
    let reddit_link =
        get_link_by_normalized_url(db.pool(), "https://old.reddit.com/r/test/comments/abc123")
            .await
            .expect("Database error");
    assert!(reddit_link.is_some(), "Reddit link should be extracted");
}

#[tokio::test]
async fn test_poll_once_creates_pending_archives() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_JSON, "application/json"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(
        &format!("{}/posts.json", mock_server.uri()),
        temp_dir.path(),
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    poll_once(&client, &config, &db)
        .await
        .expect("poll_once failed");

    // Check that pending archives were created
    let pending = get_pending_archives(db.pool(), 10)
        .await
        .expect("Failed to get pending archives");

    // Should have 2 pending archives (YouTube + Reddit)
    assert_eq!(pending.len(), 2, "Should have 2 pending archives");
}

#[tokio::test]
async fn test_poll_once_handles_multiple_posts() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(MULTI_POST_JSON, "application/json"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(
        &format!("{}/posts.json", mock_server.uri()),
        temp_dir.path(),
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let new_count = poll_once(&client, &config, &db)
        .await
        .expect("poll_once failed");

    // Should have processed 2 new posts
    assert_eq!(new_count, 2);

    // Verify both posts were inserted
    let all_posts: Vec<(String,)> = sqlx::query_as("SELECT guid FROM posts ORDER BY guid")
        .fetch_all(db.pool())
        .await
        .expect("Failed to query posts");

    assert_eq!(all_posts.len(), 2, "Should have exactly two posts");
    let guid1 = &all_posts[0].0;
    let guid2 = &all_posts[1].0;

    assert!(
        guid1.ends_with("-post-1"),
        "First GUID should end with -post-1, got: {}",
        guid1
    );
    assert!(
        guid2.ends_with("-post-2"),
        "Second GUID should end with -post-2, got: {}",
        guid2
    );

    let post1 = get_post_by_guid(db.pool(), guid1)
        .await
        .expect("Database error");
    assert!(post1.is_some());

    let post2 = get_post_by_guid(db.pool(), guid2)
        .await
        .expect("Database error");
    assert!(post2.is_some());
}

#[tokio::test]
async fn test_poll_once_idempotent() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_JSON, "application/json"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(
        &format!("{}/posts.json", mock_server.uri()),
        temp_dir.path(),
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    // First poll
    let first_count = poll_once(&client, &config, &db)
        .await
        .expect("First poll failed");
    assert_eq!(first_count, 1);

    // Second poll - should return 0 new posts
    let second_count = poll_once(&client, &config, &db)
        .await
        .expect("Second poll failed");
    assert_eq!(second_count, 0, "Second poll should find no new posts");

    // Archives should still only have 2 (YouTube + Reddit)
    let pending = get_pending_archives(db.pool(), 10)
        .await
        .expect("Failed to get pending archives");
    assert_eq!(pending.len(), 2);
}

#[tokio::test]
async fn test_poll_once_detects_quoted_links() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(QUOTED_LINK_JSON, "application/json"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(
        &format!("{}/posts.json", mock_server.uri()),
        temp_dir.path(),
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    poll_once(&client, &config, &db)
        .await
        .expect("poll_once failed");

    // The quoted link should still be extracted
    let link = get_link_by_normalized_url(db.pool(), "https://example.com/quoted-link")
        .await
        .expect("Database error");
    assert!(link.is_some(), "Quoted link should be extracted");
}

#[tokio::test]
async fn test_poll_once_handles_http_error() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let config = create_test_config(
        &format!("{}/posts.json", mock_server.uri()),
        temp_dir.path(),
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let result = poll_once(&client, &config, &db).await;
    assert!(result.is_err(), "Should fail on HTTP 500");
}

#[tokio::test]
async fn test_poll_once_handles_invalid_json() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw("not valid json {}{", "application/json"),
        )
        .mount(&mock_server)
        .await;

    let config = create_test_config(
        &format!("{}/posts.json", mock_server.uri()),
        temp_dir.path(),
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let result = poll_once(&client, &config, &db).await;
    assert!(result.is_err(), "Should fail on invalid RSS");
}
