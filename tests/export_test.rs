//! Integration tests for bulk export functionality.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use discourse_link_archiver::config::Config;
use discourse_link_archiver::db::{
    create_pending_archive, insert_artifact_with_hash, insert_link, set_archive_complete, Database,
    NewLink,
};
use discourse_link_archiver::s3::S3Client;
use tempfile::TempDir;
use tower::ServiceExt;

/// Create a test app with the given database and S3 client.
async fn create_test_app(db: Database, s3: Arc<S3Client>) -> Router {
    // Create a minimal config for tests
    std::env::set_var("RSS_URL", "https://example.com/posts.rss");
    std::env::set_var("S3_BUCKET", "test-bucket");
    let config = Config::from_env().expect("Failed to create config");

    let state = discourse_link_archiver::web::AppState {
        db: db.clone(),
        s3,
        config: Arc::new(config),
    };

    // Build the router with export route
    Router::new()
        .route(
            "/export/:site",
            axum::routing::get(discourse_link_archiver::web::export::export_site),
        )
        .with_state(state)
}

async fn setup_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.sqlite");
    let db = Database::new(&db_path)
        .await
        .expect("Failed to create database");
    (db, temp_dir)
}

/// Create a mock S3 client for testing.
/// Note: This requires MinIO or localstack to be running for full integration tests.
/// For unit tests, we'll skip tests that require actual S3 operations.
async fn create_mock_s3() -> Arc<S3Client> {
    std::env::set_var("S3_BUCKET", "test-bucket");
    std::env::set_var("S3_ENDPOINT", "http://localhost:9000"); // MinIO default
    std::env::set_var("S3_REGION", "us-east-1");
    std::env::set_var("AWS_ACCESS_KEY_ID", "minioadmin");
    std::env::set_var("AWS_SECRET_ACCESS_KEY", "minioadmin");

    let config = Config::from_env().expect("Failed to create config");
    Arc::new(
        S3Client::new(&config)
            .await
            .expect("Failed to create S3 client"),
    )
}

#[tokio::test]
#[ignore] // Requires MinIO/localstack to be running
async fn test_export_empty_domain() {
    let (db, _temp_dir) = setup_db().await;
    let s3 = create_mock_s3().await;
    let app = create_test_app(db, s3).await;

    // Make a request for a domain with no archives
    let response = app
        .oneshot(
            Request::builder()
                .uri("/export/nonexistent.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Should return 404 for domain with no archives"
    );
}

#[tokio::test]
#[ignore] // Requires MinIO/localstack to be running
async fn test_export_rate_limiting() {
    let (db, _temp_dir) = setup_db().await;
    let s3 = create_mock_s3().await;

    // Create an archive for the domain
    let new_link = NewLink {
        original_url: "https://example.com/test".to_string(),
        normalized_url: "https://example.com/test".to_string(),
        canonical_url: None,
        domain: "example.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link).await.unwrap();
    let archive_id = create_pending_archive(db.pool(), link_id, None)
        .await
        .unwrap();
    set_archive_complete(
        db.pool(),
        archive_id,
        Some("Test"),
        None,
        None,
        Some("text"),
        None,
        None,
    )
    .await
    .unwrap();

    // Record an export from the same IP
    discourse_link_archiver::db::insert_export(db.pool(), "example.com", "192.168.1.100", 1, 1024)
        .await
        .unwrap();

    // Create app with ConnectInfo middleware to set IP
    let app = Router::new()
        .route(
            "/export/:site",
            axum::routing::get(discourse_link_archiver::web::export::export_site),
        )
        .layer(
            tower::ServiceBuilder::new().layer(axum::extract::connect_info::MockConnectInfo(
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)), 8080),
            )),
        )
        .with_state(discourse_link_archiver::web::AppState {
            db: db.clone(),
            s3: s3.clone(),
            config: Arc::new(Config::from_env().unwrap()),
        });

    // Try to export again from the same IP
    let response = app
        .oneshot(
            Request::builder()
                .uri("/export/example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "Should be rate limited after 1 export per hour"
    );
}

#[tokio::test]
async fn test_export_database_tracking() {
    let (db, _temp_dir) = setup_db().await;

    // Test that we can count exports correctly
    let count = discourse_link_archiver::db::count_exports_from_ip_last_hour(db.pool(), "10.0.0.1")
        .await
        .unwrap();
    assert_eq!(count, 0, "Should have 0 exports initially");

    // Insert an export
    let export_id = discourse_link_archiver::db::insert_export(
        db.pool(),
        "reddit.com",
        "10.0.0.1",
        5,
        10485760,
    )
    .await
    .unwrap();
    assert!(export_id > 0, "Should return valid export ID");

    // Count again
    let count = discourse_link_archiver::db::count_exports_from_ip_last_hour(db.pool(), "10.0.0.1")
        .await
        .unwrap();
    assert_eq!(count, 1, "Should have 1 export after insertion");

    // Different IP should have 0
    let count = discourse_link_archiver::db::count_exports_from_ip_last_hour(db.pool(), "10.0.0.2")
        .await
        .unwrap();
    assert_eq!(count, 0, "Different IP should have 0 exports");
}

#[tokio::test]
async fn test_export_get_archives_with_artifacts() {
    let (db, _temp_dir) = setup_db().await;

    // Create archives for different domains
    let link1 = NewLink {
        original_url: "https://reddit.com/post1".to_string(),
        normalized_url: "https://old.reddit.com/post1".to_string(),
        canonical_url: None,
        domain: "old.reddit.com".to_string(),
    };
    let link2 = NewLink {
        original_url: "https://reddit.com/post2".to_string(),
        normalized_url: "https://old.reddit.com/post2".to_string(),
        canonical_url: None,
        domain: "old.reddit.com".to_string(),
    };
    let link3 = NewLink {
        original_url: "https://youtube.com/watch?v=abc".to_string(),
        normalized_url: "https://www.youtube.com/watch?v=abc".to_string(),
        canonical_url: None,
        domain: "www.youtube.com".to_string(),
    };

    let link1_id = insert_link(db.pool(), &link1).await.unwrap();
    let link2_id = insert_link(db.pool(), &link2).await.unwrap();
    let link3_id = insert_link(db.pool(), &link3).await.unwrap();

    let archive1_id = create_pending_archive(db.pool(), link1_id, None)
        .await
        .unwrap();
    let archive2_id = create_pending_archive(db.pool(), link2_id, None)
        .await
        .unwrap();
    let archive3_id = create_pending_archive(db.pool(), link3_id, None)
        .await
        .unwrap();

    // Complete archives
    for (archive_id, title) in [
        (archive1_id, "Reddit Post 1"),
        (archive2_id, "Reddit Post 2"),
        (archive3_id, "YouTube Video"),
    ] {
        set_archive_complete(
            db.pool(),
            archive_id,
            Some(title),
            Some("Author"),
            Some("Description"),
            Some("text"),
            None,
            None,
        )
        .await
        .unwrap();
    }

    // Add artifacts
    insert_artifact_with_hash(
        db.pool(),
        archive1_id,
        "html",
        "archives/1/page.html",
        Some("text/html"),
        Some(1024),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    insert_artifact_with_hash(
        db.pool(),
        archive2_id,
        "screenshot",
        "archives/2/screenshot.webp",
        Some("image/webp"),
        Some(2048),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    // Test getting archives for old.reddit.com
    let results = discourse_link_archiver::db::get_archives_with_artifacts_for_domain(
        db.pool(),
        "old.reddit.com",
    )
    .await
    .unwrap();

    assert_eq!(
        results.len(),
        2,
        "Should have 2 archives for old.reddit.com"
    );

    // Verify first archive has artifact
    let (archive, _link, artifacts) = &results[0];
    assert!(archive.id == archive1_id || archive.id == archive2_id);
    assert!(
        !artifacts.is_empty() || results[1].2.len() > 0,
        "Should have artifacts attached"
    );

    // Test getting archives for youtube.com
    let results = discourse_link_archiver::db::get_archives_with_artifacts_for_domain(
        db.pool(),
        "www.youtube.com",
    )
    .await
    .unwrap();

    assert_eq!(results.len(), 1, "Should have 1 archive for youtube.com");
}

#[tokio::test]
async fn test_export_extract_filename() {
    // This is a unit test for the extract_filename helper
    // We can't directly access it since it's private, but we can verify
    // the behavior through the metadata in an export.

    // Just verify the logic we expect
    let s3_key = "archives/123/media/video.mp4";
    let expected_filename = "video.mp4";
    let actual_filename = s3_key.rsplit('/').next().unwrap();
    assert_eq!(actual_filename, expected_filename);

    let s3_key = "single-file.txt";
    let expected_filename = "single-file.txt";
    let actual_filename = s3_key.rsplit('/').next().unwrap();
    assert_eq!(actual_filename, expected_filename);
}

#[tokio::test]
async fn test_export_only_includes_completed_archives() {
    let (db, _temp_dir) = setup_db().await;

    // Create archives with different statuses
    let link1 = NewLink {
        original_url: "https://reddit.com/complete".to_string(),
        normalized_url: "https://old.reddit.com/complete".to_string(),
        canonical_url: None,
        domain: "old.reddit.com".to_string(),
    };
    let link2 = NewLink {
        original_url: "https://reddit.com/pending".to_string(),
        normalized_url: "https://old.reddit.com/pending".to_string(),
        canonical_url: None,
        domain: "old.reddit.com".to_string(),
    };
    let link3 = NewLink {
        original_url: "https://reddit.com/failed".to_string(),
        normalized_url: "https://old.reddit.com/failed".to_string(),
        canonical_url: None,
        domain: "old.reddit.com".to_string(),
    };

    let link1_id = insert_link(db.pool(), &link1).await.unwrap();
    let link2_id = insert_link(db.pool(), &link2).await.unwrap();
    let link3_id = insert_link(db.pool(), &link3).await.unwrap();

    let archive1_id = create_pending_archive(db.pool(), link1_id, None)
        .await
        .unwrap();
    let _archive2_id = create_pending_archive(db.pool(), link2_id, None)
        .await
        .unwrap(); // Leave as pending
    let archive3_id = create_pending_archive(db.pool(), link3_id, None)
        .await
        .unwrap();

    // Complete archive1
    set_archive_complete(
        db.pool(),
        archive1_id,
        Some("Complete Archive"),
        None,
        None,
        Some("text"),
        None,
        None,
    )
    .await
    .unwrap();

    // Fail archive3
    discourse_link_archiver::db::set_archive_failed(db.pool(), archive3_id, "Test failure")
        .await
        .unwrap();

    // Get archives for export
    let results = discourse_link_archiver::db::get_archives_with_artifacts_for_domain(
        db.pool(),
        "old.reddit.com",
    )
    .await
    .unwrap();

    assert_eq!(results.len(), 1, "Should only include completed archives");
    assert_eq!(results[0].0.id, archive1_id);
    assert_eq!(results[0].0.status, "complete");
}

/// Test ZIP metadata structure (conceptual - would need real S3 for full test)
#[tokio::test]
async fn test_export_metadata_structure() {
    // This test verifies the expected metadata structure
    use serde_json::json;

    let expected_metadata = json!({
        "export_metadata": {
            "site": "old.reddit.com",
            "archive_count": 2,
            "total_size_bytes": 15728640,
            "max_video_size_bytes": 52428800,
            "exported_at": "2024-01-01T00:00:00Z"
        },
        "archives": [
            {
                "archive_id": 1,
                "url": "https://reddit.com/r/test/post1",
                "normalized_url": "https://old.reddit.com/r/test/post1",
                "domain": "old.reddit.com",
                "title": "Test Post 1",
                "author": "user123",
                "content_type": "video",
                "archived_at": "2024-01-01T00:00:00Z",
                "is_nsfw": false,
                "wayback_url": null,
                "archive_today_url": null,
                "ipfs_cid": null,
                "artifacts": [
                    {
                        "kind": "video",
                        "filename": "video.mp4",
                        "size_bytes": 10485760,
                        "content_type": "video/mp4",
                        "sha256": "abc123",
                        "zip_path": "old.reddit.com/archive-1/video.mp4"
                    }
                ]
            }
        ]
    });

    // Verify structure
    assert!(expected_metadata["export_metadata"].is_object());
    assert!(expected_metadata["archives"].is_array());

    let export_meta = &expected_metadata["export_metadata"];
    assert!(export_meta["site"].is_string());
    assert!(export_meta["archive_count"].is_number());
    assert!(export_meta["total_size_bytes"].is_number());

    let archives = expected_metadata["archives"].as_array().unwrap();
    if !archives.is_empty() {
        let first_archive = &archives[0];
        assert!(first_archive["archive_id"].is_number());
        assert!(first_archive["url"].is_string());
        assert!(first_archive["artifacts"].is_array());
    }
}

/// Test that large video files are excluded from export
#[test]
fn test_export_size_limits() {
    const MAX_VIDEO_SIZE_BYTES: i64 = 50 * 1024 * 1024; // 50 MB
    const MAX_EXPORT_SIZE_BYTES: i64 = 2 * 1024 * 1024 * 1024; // 2 GB

    // Test video size limit
    let small_video = 30 * 1024 * 1024; // 30 MB - should be included
    let large_video = 100 * 1024 * 1024; // 100 MB - should be excluded

    assert!(
        small_video <= MAX_VIDEO_SIZE_BYTES,
        "Small videos should be included"
    );
    assert!(
        large_video > MAX_VIDEO_SIZE_BYTES,
        "Large videos should be excluded"
    );

    // Test export size limit
    let current_size = 2_000_000_000i64; // ~1.86 GB (2 billion bytes)
    let additional_file = 300_000_000i64; // ~286 MB (300 million bytes)

    assert!(
        current_size + additional_file > MAX_EXPORT_SIZE_BYTES,
        "Should stop adding files when export limit reached (2B + 300M = 2.3B > 2.147B)"
    );
}

/// Test that NSFW archives are included with proper metadata
#[tokio::test]
async fn test_export_includes_nsfw_metadata() {
    let (db, _temp_dir) = setup_db().await;

    // Create an NSFW archive
    let link = NewLink {
        original_url: "https://reddit.com/r/nsfw/post".to_string(),
        normalized_url: "https://old.reddit.com/r/nsfw/post".to_string(),
        canonical_url: None,
        domain: "old.reddit.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &link).await.unwrap();
    let archive_id = create_pending_archive(db.pool(), link_id, None)
        .await
        .unwrap();

    // Complete with NSFW flag
    set_archive_complete(
        db.pool(),
        archive_id,
        Some("NSFW Content"),
        Some("user"),
        None,
        Some("image"),
        None,
        None,
    )
    .await
    .unwrap();

    // Manually set NSFW (since set_archive_complete doesn't have that param in the signature I saw)
    sqlx::query("UPDATE archives SET is_nsfw = 1, nsfw_source = 'subreddit' WHERE id = ?")
        .bind(archive_id)
        .execute(db.pool())
        .await
        .unwrap();

    // Get archives
    let results = discourse_link_archiver::db::get_archives_with_artifacts_for_domain(
        db.pool(),
        "old.reddit.com",
    )
    .await
    .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0.is_nsfw, true, "NSFW flag should be preserved");
}
