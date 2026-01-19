//! Integration tests for database operations.

use discourse_link_archiver::db::{
    create_pending_archive, get_archive, get_archive_by_link_id, get_link_by_normalized_url,
    get_post_by_guid, get_recent_archives, insert_link, insert_link_occurrence, insert_post,
    link_occurrence_exists, search_archives, Database, NewLink, NewLinkOccurrence, NewPost,
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
    let archive_id = create_pending_archive(db.pool(), link_id).await.unwrap();

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
