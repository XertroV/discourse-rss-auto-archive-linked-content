//! Integration tests for database operations.

use discourse_link_archiver::db::{
    count_archives_for_video_file, create_pending_archive, find_video_file, get_archive,
    get_archive_by_link_id, get_link_by_normalized_url, get_or_create_video_file, get_post_by_guid,
    get_recent_archives, get_top_domains, get_video_file, insert_artifact_with_video_file,
    insert_link, insert_link_occurrence, insert_post, insert_video_file, link_occurrence_exists,
    search_archives, set_archive_complete, update_video_file_metadata,
    update_video_file_metadata_key, Database, NewLink, NewLinkOccurrence, NewPost,
};
use tempfile::TempDir;

async fn setup_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.sqlite");
    let db = Database::new(&db_path)
        .await
        .expect("Failed to create database");
    (db, temp_dir)
}

#[tokio::test]
async fn test_insert_and_get_post() {
    let (db, _temp_dir) = setup_db().await;

    let new_post = NewPost {
        guid: "test-guid-123".to_string(),
        discourse_url: "https://forum.example.com/t/test/123".to_string(),
        author: Some("testuser".to_string()),
        title: Some("Test Post".to_string()),
        body_html: Some("<p>Hello world</p>".to_string()),
        content_hash: Some("abc123".to_string()),
        published_at: Some("2024-01-01T00:00:00Z".to_string()),
    };

    let post_id = insert_post(db.pool(), &new_post)
        .await
        .expect("Failed to insert post");
    assert!(post_id > 0);

    let retrieved = get_post_by_guid(db.pool(), "test-guid-123")
        .await
        .expect("Failed to get post")
        .expect("Post not found");

    assert_eq!(retrieved.guid, "test-guid-123");
    assert_eq!(retrieved.author.as_deref(), Some("testuser"));
    assert_eq!(retrieved.title.as_deref(), Some("Test Post"));
}

#[tokio::test]
async fn test_insert_and_get_link() {
    let (db, _temp_dir) = setup_db().await;

    let new_link = NewLink {
        original_url: "https://www.reddit.com/r/test/comments/abc".to_string(),
        normalized_url: "https://old.reddit.com/r/test/comments/abc".to_string(),
        canonical_url: Some("https://old.reddit.com/r/test/comments/abc".to_string()),
        domain: "old.reddit.com".to_string(),
    };

    let link_id = insert_link(db.pool(), &new_link)
        .await
        .expect("Failed to insert link");
    assert!(link_id > 0);

    let retrieved =
        get_link_by_normalized_url(db.pool(), "https://old.reddit.com/r/test/comments/abc")
            .await
            .expect("Failed to get link")
            .expect("Link not found");

    assert_eq!(retrieved.domain, "old.reddit.com");
}

#[tokio::test]
async fn test_link_occurrence() {
    let (db, _temp_dir) = setup_db().await;

    // Create a post
    let new_post = NewPost {
        guid: "post-for-link".to_string(),
        discourse_url: "https://forum.example.com/t/test/1".to_string(),
        author: None,
        title: None,
        body_html: None,
        content_hash: None,
        published_at: None,
    };
    let post_id = insert_post(db.pool(), &new_post).await.unwrap();

    // Create a link
    let new_link = NewLink {
        original_url: "https://example.com".to_string(),
        normalized_url: "https://example.com".to_string(),
        canonical_url: None,
        domain: "example.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link).await.unwrap();

    // Check occurrence doesn't exist yet
    let exists = link_occurrence_exists(db.pool(), link_id, post_id)
        .await
        .unwrap();
    assert!(!exists);

    // Create occurrence
    let occurrence = NewLinkOccurrence {
        link_id,
        post_id,
        in_quote: false,
        context_snippet: Some("Check out this link".to_string()),
    };
    insert_link_occurrence(db.pool(), &occurrence)
        .await
        .unwrap();

    // Check occurrence exists now
    let exists = link_occurrence_exists(db.pool(), link_id, post_id)
        .await
        .unwrap();
    assert!(exists);
}

#[tokio::test]
async fn test_archive_workflow() {
    let (db, _temp_dir) = setup_db().await;

    // Create a link
    let new_link = NewLink {
        original_url: "https://youtube.com/watch?v=abc".to_string(),
        normalized_url: "https://www.youtube.com/watch?v=abc".to_string(),
        canonical_url: None,
        domain: "www.youtube.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link).await.unwrap();

    // Create pending archive
    let archive_id = create_pending_archive(db.pool(), link_id, None)
        .await
        .unwrap();

    // Get archive by ID
    let archive = get_archive(db.pool(), archive_id)
        .await
        .unwrap()
        .expect("Archive not found");
    assert_eq!(archive.status, "pending");

    // Get archive by link ID
    let archive = get_archive_by_link_id(db.pool(), link_id)
        .await
        .unwrap()
        .expect("Archive not found");
    assert_eq!(archive.id, archive_id);
}

#[tokio::test]
async fn test_recent_archives() {
    let (db, _temp_dir) = setup_db().await;

    // Initially no archives
    let recent = get_recent_archives(db.pool(), 10).await.unwrap();
    assert!(recent.is_empty());
}

#[tokio::test]
async fn test_search_archives() {
    let (db, _temp_dir) = setup_db().await;

    // Search with no archives should return empty
    let results = search_archives(db.pool(), "test", 10).await.unwrap();
    assert!(results.is_empty());
}

// ========== Video File Tests ==========

#[tokio::test]
async fn test_insert_and_get_video_file() {
    let (db, _temp_dir) = setup_db().await;

    // Insert a video file
    let video_file_id = insert_video_file(
        db.pool(),
        "dQw4w9WgXcQ",
        "youtube",
        "videos/dQw4w9WgXcQ.mp4",
        Some("videos/dQw4w9WgXcQ.json"),
        Some(12345678),
        Some("video/mp4"),
        Some(212), // 3:32 in seconds
    )
    .await
    .expect("Failed to insert video file");

    assert!(video_file_id > 0);

    // Get by ID
    let vf = get_video_file(db.pool(), video_file_id)
        .await
        .expect("Failed to get video file")
        .expect("Video file not found");

    assert_eq!(vf.video_id, "dQw4w9WgXcQ");
    assert_eq!(vf.platform, "youtube");
    assert_eq!(vf.s3_key, "videos/dQw4w9WgXcQ.mp4");
    assert_eq!(
        vf.metadata_s3_key.as_deref(),
        Some("videos/dQw4w9WgXcQ.json")
    );
    assert_eq!(vf.size_bytes, Some(12345678));
    assert_eq!(vf.content_type.as_deref(), Some("video/mp4"));
    assert_eq!(vf.duration_seconds, Some(212));
}

#[tokio::test]
async fn test_find_video_file_by_platform_and_id() {
    let (db, _temp_dir) = setup_db().await;

    // Insert video file
    insert_video_file(
        db.pool(),
        "abc123xyz",
        "tiktok",
        "videos/abc123xyz.mp4",
        None,
        Some(5000000),
        Some("video/mp4"),
        Some(60),
    )
    .await
    .unwrap();

    // Find by platform and video_id
    let vf = find_video_file(db.pool(), "tiktok", "abc123xyz")
        .await
        .expect("Query failed")
        .expect("Video file not found");

    assert_eq!(vf.video_id, "abc123xyz");
    assert_eq!(vf.platform, "tiktok");

    // Should not find with wrong platform
    let not_found = find_video_file(db.pool(), "youtube", "abc123xyz")
        .await
        .expect("Query failed");
    assert!(not_found.is_none());

    // Should not find with wrong video_id
    let not_found = find_video_file(db.pool(), "tiktok", "wrong_id")
        .await
        .expect("Query failed");
    assert!(not_found.is_none());
}

#[tokio::test]
async fn test_get_or_create_video_file() {
    let (db, _temp_dir) = setup_db().await;

    // First call should create
    let vf1 = get_or_create_video_file(
        db.pool(),
        "test_video_123",
        "streamable",
        "videos/test_video_123.mp4",
        None,
        Some(1000000),
        Some("video/mp4"),
        None,
    )
    .await
    .expect("Failed to create video file");

    // Second call with same platform+video_id should return existing
    let vf2 = get_or_create_video_file(
        db.pool(),
        "test_video_123",
        "streamable",
        "videos/different_path.mp4", // Different path - should be ignored
        Some("videos/test_video_123.json"),
        Some(2000000), // Different size - should be ignored
        Some("video/webm"),
        Some(120),
    )
    .await
    .expect("Failed to get/create video file");

    // Should be the same record
    assert_eq!(vf1.id, vf2.id);
    // Original values should be preserved
    assert_eq!(vf2.s3_key, "videos/test_video_123.mp4");
    assert_eq!(vf2.size_bytes, Some(1000000));
}

#[tokio::test]
async fn test_insert_video_file_handles_duplicates() {
    let (db, _temp_dir) = setup_db().await;

    // Insert first video
    let id1 = insert_video_file(
        db.pool(),
        "duplicate_test",
        "youtube",
        "videos/duplicate_test.mp4",
        None,
        Some(1000),
        None,
        None,
    )
    .await
    .unwrap();

    // Insert same video_id + platform again (should return existing ID)
    let id2 = insert_video_file(
        db.pool(),
        "duplicate_test",
        "youtube",
        "videos/different_path.mp4",
        None,
        Some(2000),
        None,
        None,
    )
    .await
    .unwrap();

    // Should return the same ID
    assert_eq!(id1, id2);
}

#[tokio::test]
async fn test_update_video_file_metadata() {
    let (db, _temp_dir) = setup_db().await;

    // Insert video without all metadata
    let video_id = insert_video_file(
        db.pool(),
        "update_test",
        "youtube",
        "videos/update_test.mp4",
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    // Update metadata
    update_video_file_metadata(
        db.pool(),
        video_id,
        Some(9999999),
        Some("video/webm"),
        Some(300),
    )
    .await
    .unwrap();

    // Verify update
    let vf = get_video_file(db.pool(), video_id).await.unwrap().unwrap();

    assert_eq!(vf.size_bytes, Some(9999999));
    assert_eq!(vf.content_type.as_deref(), Some("video/webm"));
    assert_eq!(vf.duration_seconds, Some(300));
}

#[tokio::test]
async fn test_update_video_file_metadata_key() {
    let (db, _temp_dir) = setup_db().await;

    // Insert video without metadata key
    let video_id = insert_video_file(
        db.pool(),
        "meta_key_test",
        "youtube",
        "videos/meta_key_test.mp4",
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    // Update metadata key
    update_video_file_metadata_key(db.pool(), video_id, "videos/meta_key_test.json")
        .await
        .unwrap();

    // Verify update
    let vf = get_video_file(db.pool(), video_id).await.unwrap().unwrap();

    assert_eq!(
        vf.metadata_s3_key.as_deref(),
        Some("videos/meta_key_test.json")
    );
}

#[tokio::test]
async fn test_artifact_with_video_file_reference() {
    let (db, _temp_dir) = setup_db().await;

    // Create a link and archive
    let new_link = NewLink {
        original_url: "https://youtube.com/watch?v=test123".to_string(),
        normalized_url: "https://www.youtube.com/watch?v=test123".to_string(),
        canonical_url: None,
        domain: "www.youtube.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link).await.unwrap();
    let archive_id = create_pending_archive(db.pool(), link_id, None)
        .await
        .unwrap();

    // Create a video file
    let video_file_id = insert_video_file(
        db.pool(),
        "test123",
        "youtube",
        "videos/test123.mp4",
        None,
        Some(5000000),
        Some("video/mp4"),
        None,
    )
    .await
    .unwrap();

    // Insert artifact with video_file reference
    let artifact_id = insert_artifact_with_video_file(
        db.pool(),
        archive_id,
        "video",
        "videos/test123.mp4",
        Some("video/mp4"),
        Some(5000000),
        None,
        video_file_id,
    )
    .await
    .expect("Failed to insert artifact with video file");

    assert!(artifact_id > 0);
}

#[tokio::test]
async fn test_count_archives_for_video_file() {
    let (db, _temp_dir) = setup_db().await;

    // Create a video file
    let video_file_id = insert_video_file(
        db.pool(),
        "count_test",
        "youtube",
        "videos/count_test.mp4",
        None,
        Some(1000000),
        None,
        None,
    )
    .await
    .unwrap();

    // Initially no archives reference this video
    let count = count_archives_for_video_file(db.pool(), video_file_id)
        .await
        .unwrap();
    assert_eq!(count, 0);

    // Create two links and archives that use this video
    for i in 0..2 {
        let new_link = NewLink {
            original_url: format!("https://youtube.com/watch?v=count_test&post={}", i),
            normalized_url: format!("https://www.youtube.com/watch?v=count_test&post={}", i),
            canonical_url: None,
            domain: "www.youtube.com".to_string(),
        };
        let link_id = insert_link(db.pool(), &new_link).await.unwrap();
        let archive_id = create_pending_archive(db.pool(), link_id, None)
            .await
            .unwrap();

        // Mark archive complete so it shows up
        set_archive_complete(
            db.pool(),
            archive_id,
            Some("Test Video"),
            Some("Author"),
            Some("text"),
            Some("video"),
            None,
            None,
        )
        .await
        .unwrap();

        // Add artifact referencing the video file
        insert_artifact_with_video_file(
            db.pool(),
            archive_id,
            "video",
            "videos/count_test.mp4",
            Some("video/mp4"),
            Some(1000000),
            None,
            video_file_id,
        )
        .await
        .unwrap();
    }

    // Now should have 2 archives
    let count = count_archives_for_video_file(db.pool(), video_file_id)
        .await
        .unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn test_get_top_domains() {
    let (db, _temp_dir) = setup_db().await;

    // Create links with different domains
    let domains = vec![
        ("reddit.com", 3),  // 3 archives
        ("youtube.com", 2), // 2 archives
        ("twitter.com", 1), // 1 archive
    ];

    for (domain, count) in domains {
        for i in 0..count {
            let new_link = NewLink {
                original_url: format!("https://{}/test/{}", domain, i),
                normalized_url: format!("https://{}/test/{}", domain, i),
                canonical_url: None,
                domain: domain.to_string(),
            };
            let link_id = insert_link(db.pool(), &new_link).await.unwrap();

            // Create archive and mark as complete
            let archive_id = create_pending_archive(db.pool(), link_id, None)
                .await
                .unwrap();

            set_archive_complete(
                db.pool(),
                archive_id,
                Some("Test Title"),
                Some("Test Author"),
                Some("Test content"),
                Some("text"),
                None,
                None,
            )
            .await
            .unwrap();
        }
    }

    // Get top 3 domains
    let top_domains = get_top_domains(db.pool(), 3).await.unwrap();

    // Should return domains in order: reddit.com (3), youtube.com (2), twitter.com (1)
    assert_eq!(top_domains.len(), 3);
    assert_eq!(top_domains[0].0, "reddit.com");
    assert_eq!(top_domains[0].1, 3);
    assert_eq!(top_domains[1].0, "youtube.com");
    assert_eq!(top_domains[1].1, 2);
    assert_eq!(top_domains[2].0, "twitter.com");
    assert_eq!(top_domains[2].1, 1);

    // Test limit
    let top_2_domains = get_top_domains(db.pool(), 2).await.unwrap();
    assert_eq!(top_2_domains.len(), 2);
    assert_eq!(top_2_domains[0].0, "reddit.com");
    assert_eq!(top_2_domains[1].0, "youtube.com");
}
