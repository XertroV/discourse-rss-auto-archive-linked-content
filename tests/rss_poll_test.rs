//! Integration tests for RSS polling.

use std::path::PathBuf;
use std::time::Duration;

use discourse_link_archiver::config::{ArchiveMode, Config, LogFormat};
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
        poll_interval: Duration::from_secs(60),
        cache_window: Duration::from_secs(3600),
        database_path: PathBuf::from("./test.db"),
        s3_bucket: "test-bucket".to_string(),
        s3_region: "us-east-1".to_string(),
        s3_endpoint: None,
        s3_prefix: "archives/".to_string(),
        worker_concurrency: 4,
        per_domain_concurrency: 1,
        work_dir: work_dir.to_path_buf(),
        yt_dlp_path: "yt-dlp".to_string(),
        gallery_dl_path: "gallery-dl".to_string(),
        cookies_file_path: None,
        archive_mode: ArchiveMode::All,
        archive_quote_only_links: false,
        web_host: "0.0.0.0".to_string(),
        web_port: 8080,
        tls_enabled: false,
        tls_domains: vec![],
        tls_contact_email: None,
        tls_cache_dir: PathBuf::from("./acme_cache"),
        tls_use_staging: false,
        tls_https_port: 443,
        wayback_enabled: false,
        wayback_rate_limit_per_min: 5,
        archive_today_enabled: false,
        archive_today_rate_limit_per_min: 3,
        backup_enabled: false,
        backup_interval_hours: 24,
        backup_retention_count: 30,
        log_format: LogFormat::Pretty,
        ipfs_enabled: false,
        ipfs_api_url: "http://127.0.0.1:5001".to_string(),
        ipfs_gateway_urls: vec![],
        submission_enabled: false,
        submission_rate_limit_per_hour: 10,
        screenshot_enabled: false,
        screenshot_viewport_width: 1280,
        screenshot_viewport_height: 800,
        screenshot_timeout_secs: 30,
        screenshot_chrome_path: None,
        pdf_enabled: false,
        pdf_paper_width: 8.27,
        pdf_paper_height: 11.69,
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

/// Sample RSS feed with a single post containing links.
const SAMPLE_RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
  <channel>
    <title>Test Forum</title>
    <link>https://forum.example.com</link>
    <description>Test forum for RSS polling</description>
    <atom:link href="https://forum.example.com/posts.rss" rel="self" type="application/rss+xml"/>
    <item>
      <title>Test Post with Links</title>
      <link>https://forum.example.com/t/test-post/123</link>
      <guid isPermaLink="false">forum.example.com-post-123</guid>
      <pubDate>Mon, 01 Jan 2024 12:00:00 +0000</pubDate>
      <creator><![CDATA[testuser]]></creator>
      <description><![CDATA[
        <p>Check out this video: <a href="https://www.youtube.com/watch?v=dQw4w9WgXcQ">YouTube Link</a></p>
        <p>And this reddit post: <a href="https://www.reddit.com/r/test/comments/abc123">Reddit</a></p>
      ]]></description>
    </item>
  </channel>
</rss>"#;

/// RSS feed with multiple posts.
const MULTI_POST_RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Test Forum</title>
    <link>https://forum.example.com</link>
    <item>
      <title>First Post</title>
      <link>https://forum.example.com/t/first/1</link>
      <guid>forum.example.com-post-1</guid>
      <description><![CDATA[
        <p>Link: <a href="https://twitter.com/user/status/123">Tweet</a></p>
      ]]></description>
    </item>
    <item>
      <title>Second Post</title>
      <link>https://forum.example.com/t/second/2</link>
      <guid>forum.example.com-post-2</guid>
      <description><![CDATA[
        <p>Link: <a href="https://www.tiktok.com/@user/video/456">TikTok</a></p>
      ]]></description>
    </item>
  </channel>
</rss>"#;

/// RSS feed with a post containing a quoted link.
const QUOTED_LINK_RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Test Forum</title>
    <link>https://forum.example.com</link>
    <item>
      <title>Post with Quoted Link</title>
      <link>https://forum.example.com/t/quoted/3</link>
      <guid>forum.example.com-post-3</guid>
      <description><![CDATA[
        <aside class="quote">
          <p>Someone said: <a href="https://example.com/quoted-link">link</a></p>
        </aside>
        <p>My response here.</p>
      ]]></description>
    </item>
  </channel>
</rss>"#;

#[tokio::test]
async fn test_poll_once_processes_new_posts() {
    let (db, temp_dir) = setup_db().await;

    // Start mock server
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/posts.rss"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_RSS, "application/rss+xml"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
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
    let post = get_post_by_guid(db.pool(), "forum.example.com-post-123")
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
        .and(path("/posts.rss"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_RSS, "application/rss+xml"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
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

    // Check that Reddit link was extracted (generic normalization preserves www.reddit.com)
    let reddit_link =
        get_link_by_normalized_url(db.pool(), "https://www.reddit.com/r/test/comments/abc123")
            .await
            .expect("Database error");
    assert!(reddit_link.is_some(), "Reddit link should be extracted");
}

#[tokio::test]
async fn test_poll_once_creates_pending_archives() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/posts.rss"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_RSS, "application/rss+xml"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
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
        .and(path("/posts.rss"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(MULTI_POST_RSS, "application/rss+xml"),
        )
        .mount(&mock_server)
        .await;

    let config = create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
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
    let post1 = get_post_by_guid(db.pool(), "forum.example.com-post-1")
        .await
        .expect("Database error");
    assert!(post1.is_some());

    let post2 = get_post_by_guid(db.pool(), "forum.example.com-post-2")
        .await
        .expect("Database error");
    assert!(post2.is_some());
}

#[tokio::test]
async fn test_poll_once_idempotent() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/posts.rss"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(SAMPLE_RSS, "application/rss+xml"))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
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
        .and(path("/posts.rss"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(QUOTED_LINK_RSS, "application/rss+xml"),
        )
        .mount(&mock_server)
        .await;

    let config = create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
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
        .and(path("/posts.rss"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let config = create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let result = poll_once(&client, &config, &db).await;
    assert!(result.is_err(), "Should fail on HTTP 500");
}

#[tokio::test]
async fn test_poll_once_handles_invalid_rss() {
    let (db, temp_dir) = setup_db().await;

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/posts.rss"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw("not valid xml <><>", "application/rss+xml"),
        )
        .mount(&mock_server)
        .await;

    let config = create_test_config(&format!("{}/posts.rss", mock_server.uri()), temp_dir.path());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let result = poll_once(&client, &config, &db).await;
    assert!(result.is_err(), "Should fail on invalid RSS");
}
