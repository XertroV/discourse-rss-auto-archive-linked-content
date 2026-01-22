//! Compatibility tests to ensure JSON API behaves identically to RSS.
//!
//! These tests verify that the migration from RSS to JSON maintains
//! exact compatibility with existing database records and behavior.

use discourse_link_archiver::config::Config;
use discourse_link_archiver::db::{get_post_by_guid, Database};
use discourse_link_archiver::rss::poll_once;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn setup_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.sqlite");
    let db = Database::new(&db_path)
        .await
        .expect("Failed to create database");
    (db, temp_dir)
}

fn create_test_config(rss_url: &str, work_dir: &std::path::Path) -> Config {
    Config {
        rss_url: rss_url.to_string(),
        work_dir: work_dir.to_path_buf(),
        ..Config::for_testing()
    }
}

/// Test that GUID format matches the expected RSS-compatible format.
#[tokio::test]
async fn test_guid_format_compatibility() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;

    // JSON response with a known post ID
    let json = r#"{
      "latest_posts": [
        {
          "id": 12345,
          "post_number": 1,
          "username": "testuser",
          "topic_id": 100,
          "topic_slug": "test-topic",
          "topic_title": "Test Topic",
          "created_at": "2024-01-01T12:00:00.000Z",
          "updated_at": "2024-01-01T12:00:00.000Z",
          "cooked": "<p>Test content</p>",
          "post_url": "/t/test-topic/100/1"
        }
      ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(json, "application/json"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let count = poll_once(&client, &config, &db)
        .await
        .expect("poll_once failed");

    assert_eq!(count, 1, "Should have processed 1 post");

    // Query all posts to see what GUIDs are actually stored
    let all_posts: Vec<(String,)> = sqlx::query_as("SELECT guid FROM posts")
        .fetch_all(db.pool())
        .await
        .expect("Failed to query posts");

    eprintln!("Mock server URI: {}", mock_server.uri());
    eprintln!("All GUIDs in database: {:?}", all_posts);

    assert_eq!(all_posts.len(), 1, "Should have exactly one post");
    let actual_guid = &all_posts[0].0;

    // Verify GUID format matches RSS pattern: {domain}-post-{id}
    assert!(
        actual_guid.ends_with("-post-12345"),
        "GUID should end with -post-12345, got: {}",
        actual_guid
    );

    let post = get_post_by_guid(db.pool(), actual_guid)
        .await
        .expect("Database error")
        .expect("Post not found");

    // Verify the GUID is stored correctly
    assert_eq!(post.guid, *actual_guid);

    // Verify it matches the RSS pattern: {domain}-post-{id}
    assert!(post.guid.ends_with("-post-12345"));
    assert!(post.guid.contains("-post-"));
}

/// Test that author format is standardized to @username.
#[tokio::test]
async fn test_author_format_standardization() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;

    // Test various username formats
    let json = r#"{
      "latest_posts": [
        {
          "id": 1,
          "post_number": 1,
          "username": "SimpleUser",
          "topic_id": 100,
          "topic_slug": "test",
          "topic_title": "Test",
          "created_at": "2024-01-01T12:00:00.000Z",
          "updated_at": "2024-01-01T12:00:00.000Z",
          "cooked": "<p>Test</p>",
          "post_url": "/t/test/100/1"
        },
        {
          "id": 2,
          "post_number": 1,
          "username": "user_with_underscores",
          "topic_id": 101,
          "topic_slug": "test2",
          "topic_title": "Test 2",
          "created_at": "2024-01-01T12:05:00.000Z",
          "updated_at": "2024-01-01T12:05:00.000Z",
          "cooked": "<p>Test 2</p>",
          "post_url": "/t/test2/101/1"
        },
        {
          "id": 3,
          "post_number": 1,
          "username": "User123",
          "topic_id": 102,
          "topic_slug": "test3",
          "topic_title": "Test 3",
          "created_at": "2024-01-01T12:10:00.000Z",
          "updated_at": "2024-01-01T12:10:00.000Z",
          "cooked": "<p>Test 3</p>",
          "post_url": "/t/test3/102/1"
        }
      ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(json, "application/json"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let count = poll_once(&client, &config, &db)
        .await
        .expect("poll_once failed");

    assert_eq!(count, 3, "Should have processed 3 posts");

    // Query all posts
    let all_posts: Vec<(i64, String, Option<String>)> =
        sqlx::query_as("SELECT id, guid, author FROM posts ORDER BY id")
            .fetch_all(db.pool())
            .await
            .expect("Failed to query posts");

    assert_eq!(all_posts.len(), 3, "Should have exactly three posts");

    // Verify all authors follow @username format
    let post1 = &all_posts[0];
    assert!(post1.1.ends_with("-post-1"));
    assert_eq!(post1.2.as_deref(), Some("@SimpleUser"));

    let post2 = &all_posts[1];
    assert!(post2.1.ends_with("-post-2"));
    assert_eq!(post2.2.as_deref(), Some("@user_with_underscores"));

    let post3 = &all_posts[2];
    assert!(post3.1.ends_with("-post-3"));
    assert_eq!(post3.2.as_deref(), Some("@User123"));
}

/// Test that discourse_url is constructed correctly from post_url.
#[tokio::test]
async fn test_discourse_url_construction() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;

    let json = r#"{
      "latest_posts": [
        {
          "id": 100,
          "post_number": 5,
          "username": "testuser",
          "topic_id": 200,
          "topic_slug": "long-topic-slug-with-dashes",
          "topic_title": "Long Topic",
          "created_at": "2024-01-01T12:00:00.000Z",
          "updated_at": "2024-01-01T12:00:00.000Z",
          "cooked": "<p>Test</p>",
          "post_url": "/t/long-topic-slug-with-dashes/200/5"
        }
      ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(json, "application/json"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let count = poll_once(&client, &config, &db)
        .await
        .expect("poll_once failed");

    assert_eq!(count, 1, "Should have processed 1 post");

    // Query the post
    let all_posts: Vec<(String, String)> = sqlx::query_as("SELECT guid, discourse_url FROM posts")
        .fetch_all(db.pool())
        .await
        .expect("Failed to query posts");

    assert_eq!(all_posts.len(), 1);
    let (guid, discourse_url) = &all_posts[0];

    assert!(guid.ends_with("-post-100"));

    // Verify discourse_url is constructed correctly and contains the expected path
    assert!(
        discourse_url.ends_with("/t/long-topic-slug-with-dashes/200/5"),
        "URL should end with correct path, got: {}",
        discourse_url
    );
    assert!(
        discourse_url.starts_with("http://"),
        "URL should start with http://"
    );
}

/// Test that content changes are detected correctly with content hash.
#[tokio::test]
async fn test_content_change_detection() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;

    // First poll - original content
    let json_v1 = r#"{
      "latest_posts": [
        {
          "id": 500,
          "post_number": 1,
          "username": "testuser",
          "topic_id": 100,
          "topic_slug": "test",
          "topic_title": "Test",
          "created_at": "2024-01-01T12:00:00.000Z",
          "updated_at": "2024-01-01T12:00:00.000Z",
          "cooked": "<p>Original content</p>",
          "post_url": "/t/test/100/1"
        }
      ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(json_v1, "application/json"))
        .up_to_n_times(1)
        .named("first_poll")
        .mount(&mock_server)
        .await;

    let config = create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let first_count = poll_once(&client, &config, &db)
        .await
        .expect("First poll failed");
    assert_eq!(first_count, 1);

    // Get the GUID and initial hash
    let posts_v1: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT guid, content_hash FROM posts")
            .fetch_all(db.pool())
            .await
            .expect("Failed to query posts");

    assert_eq!(posts_v1.len(), 1);
    let (guid, hash_v1) = &posts_v1[0];
    assert!(guid.ends_with("-post-500"));

    // Second poll - updated content
    let json_v2 = r#"{
      "latest_posts": [
        {
          "id": 500,
          "post_number": 1,
          "username": "testuser",
          "topic_id": 100,
          "topic_slug": "test",
          "topic_title": "Test",
          "created_at": "2024-01-01T12:00:00.000Z",
          "updated_at": "2024-01-01T13:00:00.000Z",
          "cooked": "<p>Updated content - this is different!</p>",
          "post_url": "/t/test/100/1"
        }
      ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(json_v2, "application/json"))
        .named("second_poll")
        .mount(&mock_server)
        .await;

    let second_count = poll_once(&client, &config, &db)
        .await
        .expect("Second poll failed");

    // Should not count as new since same GUID, but should be updated
    assert_eq!(second_count, 0);

    // Get updated hash
    let posts_v2: Vec<(String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT guid, content_hash, body_html FROM posts")
            .fetch_all(db.pool())
            .await
            .expect("Failed to query posts");

    assert_eq!(posts_v2.len(), 1);
    let (guid_v2, hash_v2, body_html) = &posts_v2[0];

    // Verify same GUID
    assert_eq!(guid, guid_v2);

    // Verify content hash changed
    assert_ne!(hash_v1, hash_v2);
    assert_eq!(
        body_html.as_deref(),
        Some("<p>Updated content - this is different!</p>")
    );
}

/// Test that link_archive_account commands are still detected.
#[tokio::test]
async fn test_account_linking_command_detection() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;

    let json = r#"{
      "latest_posts": [
        {
          "id": 999,
          "post_number": 1,
          "username": "forumuser",
          "topic_id": 100,
          "topic_slug": "account-link",
          "topic_title": "Account Link",
          "created_at": "2024-01-01T12:00:00.000Z",
          "updated_at": "2024-01-01T12:00:00.000Z",
          "cooked": "<p>link_archive_account:archiveuser</p><p>Rest of the post content.</p>",
          "post_url": "/t/account-link/100/1"
        }
      ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(json, "application/json"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let count = poll_once(&client, &config, &db)
        .await
        .expect("poll_once failed");

    assert_eq!(count, 1, "Should have processed 1 post");

    // Verify post was created
    let all_posts: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT guid, body_html FROM posts")
            .fetch_all(db.pool())
            .await
            .expect("Failed to query posts");

    assert_eq!(all_posts.len(), 1);
    let (guid, body_html) = &all_posts[0];
    assert!(guid.ends_with("-post-999"));

    // Verify content contains the command
    assert!(body_html
        .as_ref()
        .unwrap()
        .contains("link_archive_account:archiveuser"));
}

/// Test that pagination works identically to RSS (using 'before' parameter).
#[tokio::test]
async fn test_pagination_compatibility() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;

    // First page
    let json_page1 = r#"{
      "latest_posts": [
        {
          "id": 100,
          "post_number": 1,
          "username": "user1",
          "topic_id": 1,
          "topic_slug": "topic1",
          "topic_title": "Topic 1",
          "created_at": "2024-01-01T12:00:00.000Z",
          "updated_at": "2024-01-01T12:00:00.000Z",
          "cooked": "<p>Post 100</p>",
          "post_url": "/t/topic1/1/1"
        },
        {
          "id": 99,
          "post_number": 1,
          "username": "user2",
          "topic_id": 2,
          "topic_slug": "topic2",
          "topic_title": "Topic 2",
          "created_at": "2024-01-01T11:55:00.000Z",
          "updated_at": "2024-01-01T11:55:00.000Z",
          "cooked": "<p>Post 99</p>",
          "post_url": "/t/topic2/2/1"
        }
      ]
    }"#;

    // Second page (before=99)
    let json_page2 = r#"{
      "latest_posts": [
        {
          "id": 98,
          "post_number": 1,
          "username": "user3",
          "topic_id": 3,
          "topic_slug": "topic3",
          "topic_title": "Topic 3",
          "created_at": "2024-01-01T11:50:00.000Z",
          "updated_at": "2024-01-01T11:50:00.000Z",
          "cooked": "<p>Post 98</p>",
          "post_url": "/t/topic3/3/1"
        }
      ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(json_page1, "application/json"))
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(json_page2, "application/json"))
        .mount(&mock_server)
        .await;

    let mut config =
        create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
    config.rss_max_pages = 2; // Fetch 2 pages

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let total_count = poll_once(&client, &config, &db)
        .await
        .expect("poll_once failed");

    // Should have fetched posts from both pages
    assert_eq!(total_count, 3);

    // Verify all posts were inserted
    let all_posts: Vec<(String,)> = sqlx::query_as("SELECT guid FROM posts ORDER BY id")
        .fetch_all(db.pool())
        .await
        .expect("Failed to query posts");

    eprintln!("All GUIDs: {:?}", all_posts);

    assert_eq!(all_posts.len(), 3, "Should have 3 posts");

    // Verify posts from both pages
    // Note: The posts might be in any order depending on when they were inserted
    let guids: Vec<&str> = all_posts.iter().map(|p| p.0.as_str()).collect();
    assert!(
        guids.iter().any(|g| g.ends_with("-post-100")),
        "Should have post-100"
    );
    assert!(
        guids.iter().any(|g| g.ends_with("-post-99")),
        "Should have post-99"
    );
    assert!(
        guids.iter().any(|g| g.ends_with("-post-98")),
        "Should have post-98"
    );
}

/// Test that posts with special characters in usernames/titles are handled correctly.
#[tokio::test]
async fn test_special_characters_handling() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;

    let json = r#"{
      "latest_posts": [
        {
          "id": 777,
          "post_number": 1,
          "username": "user.with.dots",
          "topic_id": 100,
          "topic_slug": "topic-with-unicode-文字",
          "topic_title": "Topic with Unicode: 文字 & Emoji 🎉",
          "created_at": "2024-01-01T12:00:00.000Z",
          "updated_at": "2024-01-01T12:00:00.000Z",
          "cooked": "<p>Content with &lt;escaped&gt; HTML &amp; quotes \"'</p>",
          "post_url": "/t/topic-with-unicode-文字/100/1"
        }
      ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/posts.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(json, "application/json"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap();

    let count = poll_once(&client, &config, &db)
        .await
        .expect("poll_once failed");
    assert_eq!(count, 1);

    // Query the post
    let all_posts: Vec<(String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT guid, author, title, body_html FROM posts")
            .fetch_all(db.pool())
            .await
            .expect("Failed to query posts");

    assert_eq!(all_posts.len(), 1);
    let (guid, author, title, body_html) = &all_posts[0];

    assert!(guid.ends_with("-post-777"));

    // Verify special characters are preserved
    assert_eq!(author.as_deref(), Some("@user.with.dots"));
    assert!(title.as_ref().unwrap().contains("文字"));
    assert!(title.as_ref().unwrap().contains("🎉"));
    assert!(body_html.as_ref().unwrap().contains("&lt;escaped&gt;"));
}
