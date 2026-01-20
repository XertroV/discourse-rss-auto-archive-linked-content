//! Integration tests for the comment system.

use discourse_link_archiver::auth::hash_password;
use discourse_link_archiver::db::{
    add_comment_reaction, can_user_edit_comment, create_comment, create_comment_reply,
    create_pending_archive, create_user, get_comment_edit_history, get_comment_reaction_count,
    get_comment_with_author, get_comments_for_archive, has_user_reacted, insert_link, pin_comment,
    remove_comment_reaction, soft_delete_comment, unpin_comment, update_comment, Database, NewLink,
};
use tempfile::TempDir;

async fn setup_test_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path)
        .await
        .expect("Failed to create database");
    (db, temp_dir)
}

/// Helper to create a test user
async fn create_test_user(db: &Database, username: &str, is_admin: bool) -> i64 {
    let password_hash = hash_password("password123").unwrap();
    create_user(db.pool(), username, &password_hash, is_admin)
        .await
        .expect("Failed to create user")
}

/// Helper to create a test archive
async fn create_test_archive(db: &Database) -> i64 {
    let new_link = NewLink {
        original_url: "https://example.com/test".to_string(),
        normalized_url: "https://example.com/test".to_string(),
        canonical_url: None,
        domain: "example.com".to_string(),
    };
    let link_id = insert_link(db.pool(), &new_link).await.unwrap();
    create_pending_archive(db.pool(), link_id, None)
        .await
        .unwrap()
}

#[tokio::test]
async fn test_create_and_get_comment() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let archive_id = create_test_archive(&db).await;

    // Create a comment
    let comment_id = create_comment(db.pool(), archive_id, user_id, "This is a test comment")
        .await
        .expect("Failed to create comment");

    assert!(comment_id > 0);

    // Retrieve the comment
    let comment = get_comment_with_author(db.pool(), comment_id)
        .await
        .expect("Failed to get comment")
        .expect("Comment not found");

    assert_eq!(comment.id, comment_id);
    assert_eq!(comment.archive_id, archive_id);
    assert_eq!(comment.user_id, Some(user_id));
    assert_eq!(comment.content, "This is a test comment");
    assert_eq!(comment.author_username, Some("testuser".to_string()));
    assert!(!comment.is_deleted);
    assert!(!comment.is_pinned);
    assert_eq!(comment.edit_count, 0);
    assert_eq!(comment.helpful_count, 0);
}

#[tokio::test]
async fn test_create_comment_reply() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id1 = create_test_user(&db, "user1", false).await;
    let user_id2 = create_test_user(&db, "user2", false).await;
    let archive_id = create_test_archive(&db).await;

    // Create parent comment
    let parent_id = create_comment(db.pool(), archive_id, user_id1, "Parent comment")
        .await
        .unwrap();

    // Create reply
    let reply_id =
        create_comment_reply(db.pool(), archive_id, user_id2, parent_id, "Reply comment")
            .await
            .expect("Failed to create reply");

    assert!(reply_id > 0);

    // Retrieve the reply
    let reply = get_comment_with_author(db.pool(), reply_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(reply.parent_comment_id, Some(parent_id));
    assert_eq!(reply.content, "Reply comment");
    assert_eq!(reply.author_username, Some("user2".to_string()));
}

#[tokio::test]
async fn test_get_comments_for_archive() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let archive_id = create_test_archive(&db).await;

    // Create multiple comments
    create_comment(db.pool(), archive_id, user_id, "First comment")
        .await
        .unwrap();
    create_comment(db.pool(), archive_id, user_id, "Second comment")
        .await
        .unwrap();
    create_comment(db.pool(), archive_id, user_id, "Third comment")
        .await
        .unwrap();

    // Retrieve all comments
    let comments = get_comments_for_archive(db.pool(), archive_id)
        .await
        .expect("Failed to get comments");

    assert_eq!(comments.len(), 3);
    assert_eq!(comments[0].content, "First comment");
    assert_eq!(comments[1].content, "Second comment");
    assert_eq!(comments[2].content, "Third comment");
}

#[tokio::test]
async fn test_update_comment_creates_edit_history() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let archive_id = create_test_archive(&db).await;

    // Create a comment
    let comment_id = create_comment(db.pool(), archive_id, user_id, "Original content")
        .await
        .unwrap();

    // Update the comment
    update_comment(db.pool(), comment_id, "Updated content", user_id)
        .await
        .expect("Failed to update comment");

    // Verify updated content
    let comment = get_comment_with_author(db.pool(), comment_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(comment.content, "Updated content");
    assert_eq!(comment.edit_count, 1);

    // Check edit history
    let history = get_comment_edit_history(db.pool(), comment_id)
        .await
        .expect("Failed to get edit history");

    assert_eq!(history.len(), 1);
    assert_eq!(history[0].previous_content, "Original content");
    assert_eq!(history[0].edited_by_user_id, user_id);
}

#[tokio::test]
async fn test_update_comment_multiple_edits() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let archive_id = create_test_archive(&db).await;

    // Create a comment
    let comment_id = create_comment(db.pool(), archive_id, user_id, "Version 1")
        .await
        .unwrap();

    // Multiple updates
    update_comment(db.pool(), comment_id, "Version 2", user_id)
        .await
        .unwrap();
    update_comment(db.pool(), comment_id, "Version 3", user_id)
        .await
        .unwrap();

    // Verify final content
    let comment = get_comment_with_author(db.pool(), comment_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(comment.content, "Version 3");
    assert_eq!(comment.edit_count, 2);

    // Check edit history preserves all versions
    let history = get_comment_edit_history(db.pool(), comment_id)
        .await
        .unwrap();

    assert_eq!(history.len(), 2);
    assert_eq!(history[0].previous_content, "Version 1");
    assert_eq!(history[1].previous_content, "Version 2");
}

#[tokio::test]
async fn test_soft_delete_comment() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let archive_id = create_test_archive(&db).await;

    // Create a comment
    let comment_id = create_comment(db.pool(), archive_id, user_id, "To be deleted")
        .await
        .unwrap();

    // Soft delete by user
    soft_delete_comment(db.pool(), comment_id, false)
        .await
        .expect("Failed to delete comment");

    // Verify deletion
    let comment = get_comment_with_author(db.pool(), comment_id)
        .await
        .unwrap()
        .unwrap();

    assert!(comment.is_deleted);
    assert!(!comment.deleted_by_admin);
    assert!(comment.deleted_at.is_some());
    // Content should still be preserved
    assert_eq!(comment.content, "To be deleted");
}

#[tokio::test]
async fn test_soft_delete_comment_by_admin() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let _admin_id = create_test_user(&db, "admin", true).await;
    let archive_id = create_test_archive(&db).await;

    // Create a comment
    let comment_id = create_comment(db.pool(), archive_id, user_id, "Inappropriate content")
        .await
        .unwrap();

    // Soft delete by admin
    soft_delete_comment(db.pool(), comment_id, true)
        .await
        .expect("Failed to delete comment");

    // Verify deletion
    let comment = get_comment_with_author(db.pool(), comment_id)
        .await
        .unwrap()
        .unwrap();

    assert!(comment.is_deleted);
    assert!(comment.deleted_by_admin);
    assert!(comment.deleted_at.is_some());
}

#[tokio::test]
async fn test_pin_and_unpin_comment() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let admin_id = create_test_user(&db, "admin", true).await;
    let archive_id = create_test_archive(&db).await;

    // Create a comment
    let comment_id = create_comment(db.pool(), archive_id, user_id, "Important comment")
        .await
        .unwrap();

    // Pin comment
    pin_comment(db.pool(), comment_id, admin_id)
        .await
        .expect("Failed to pin comment");

    // Verify pinned
    let comment = get_comment_with_author(db.pool(), comment_id)
        .await
        .unwrap()
        .unwrap();

    assert!(comment.is_pinned);
    assert_eq!(comment.pinned_by_user_id, Some(admin_id));

    // Unpin comment
    unpin_comment(db.pool(), comment_id)
        .await
        .expect("Failed to unpin comment");

    // Verify unpinned
    let comment = get_comment_with_author(db.pool(), comment_id)
        .await
        .unwrap()
        .unwrap();

    assert!(!comment.is_pinned);
    assert_eq!(comment.pinned_by_user_id, None);
}

#[tokio::test]
async fn test_pinned_comments_appear_first() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let admin_id = create_test_user(&db, "admin", true).await;
    let archive_id = create_test_archive(&db).await;

    // Create multiple comments
    let comment1_id = create_comment(db.pool(), archive_id, user_id, "First comment")
        .await
        .unwrap();
    let comment2_id = create_comment(db.pool(), archive_id, user_id, "Second comment")
        .await
        .unwrap();
    let comment3_id = create_comment(db.pool(), archive_id, user_id, "Third comment")
        .await
        .unwrap();

    // Pin the second comment
    pin_comment(db.pool(), comment2_id, admin_id).await.unwrap();

    // Retrieve comments
    let comments = get_comments_for_archive(db.pool(), archive_id)
        .await
        .unwrap();

    // Pinned comment should be first
    assert_eq!(comments.len(), 3);
    assert_eq!(comments[0].id, comment2_id);
    assert!(comments[0].is_pinned);
    assert_eq!(comments[1].id, comment1_id);
    assert_eq!(comments[2].id, comment3_id);
}

#[tokio::test]
async fn test_add_and_remove_comment_reaction() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let reactor_id = create_test_user(&db, "reactor", false).await;
    let archive_id = create_test_archive(&db).await;

    // Create a comment
    let comment_id = create_comment(db.pool(), archive_id, user_id, "Helpful comment")
        .await
        .unwrap();

    // Initially no reactions
    let count = get_comment_reaction_count(db.pool(), comment_id)
        .await
        .unwrap();
    assert_eq!(count, 0);

    let has_reacted = has_user_reacted(db.pool(), comment_id, reactor_id)
        .await
        .unwrap();
    assert!(!has_reacted);

    // Add reaction
    add_comment_reaction(db.pool(), comment_id, reactor_id)
        .await
        .expect("Failed to add reaction");

    // Verify reaction added
    let count = get_comment_reaction_count(db.pool(), comment_id)
        .await
        .unwrap();
    assert_eq!(count, 1);

    let has_reacted = has_user_reacted(db.pool(), comment_id, reactor_id)
        .await
        .unwrap();
    assert!(has_reacted);

    // Check comment helpful count
    let comment = get_comment_with_author(db.pool(), comment_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(comment.helpful_count, 1);

    // Remove reaction
    remove_comment_reaction(db.pool(), comment_id, reactor_id)
        .await
        .expect("Failed to remove reaction");

    // Verify reaction removed
    let count = get_comment_reaction_count(db.pool(), comment_id)
        .await
        .unwrap();
    assert_eq!(count, 0);

    let has_reacted = has_user_reacted(db.pool(), comment_id, reactor_id)
        .await
        .unwrap();
    assert!(!has_reacted);
}

#[tokio::test]
async fn test_multiple_users_can_react() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let reactor1_id = create_test_user(&db, "reactor1", false).await;
    let reactor2_id = create_test_user(&db, "reactor2", false).await;
    let reactor3_id = create_test_user(&db, "reactor3", false).await;
    let archive_id = create_test_archive(&db).await;

    // Create a comment
    let comment_id = create_comment(db.pool(), archive_id, user_id, "Very helpful")
        .await
        .unwrap();

    // Multiple users react
    add_comment_reaction(db.pool(), comment_id, reactor1_id)
        .await
        .unwrap();
    add_comment_reaction(db.pool(), comment_id, reactor2_id)
        .await
        .unwrap();
    add_comment_reaction(db.pool(), comment_id, reactor3_id)
        .await
        .unwrap();

    // Verify count
    let count = get_comment_reaction_count(db.pool(), comment_id)
        .await
        .unwrap();
    assert_eq!(count, 3);

    // Verify each user's reaction status
    assert!(has_user_reacted(db.pool(), comment_id, reactor1_id)
        .await
        .unwrap());
    assert!(has_user_reacted(db.pool(), comment_id, reactor2_id)
        .await
        .unwrap());
    assert!(has_user_reacted(db.pool(), comment_id, reactor3_id)
        .await
        .unwrap());
}

#[tokio::test]
async fn test_duplicate_reaction_ignored() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let reactor_id = create_test_user(&db, "reactor", false).await;
    let archive_id = create_test_archive(&db).await;

    // Create a comment
    let comment_id = create_comment(db.pool(), archive_id, user_id, "Comment")
        .await
        .unwrap();

    // Add reaction twice
    add_comment_reaction(db.pool(), comment_id, reactor_id)
        .await
        .unwrap();
    add_comment_reaction(db.pool(), comment_id, reactor_id)
        .await
        .unwrap();

    // Should only count once
    let count = get_comment_reaction_count(db.pool(), comment_id)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_can_user_edit_comment_owner() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let other_user_id = create_test_user(&db, "otheruser", false).await;
    let archive_id = create_test_archive(&db).await;

    // Create a comment
    let comment_id = create_comment(db.pool(), archive_id, user_id, "Test comment")
        .await
        .unwrap();

    // Owner can edit (within 1 hour)
    let can_edit = can_user_edit_comment(db.pool(), comment_id, user_id, false)
        .await
        .expect("Failed to check edit permission");
    assert!(can_edit);

    // Other user cannot edit
    let can_edit = can_user_edit_comment(db.pool(), comment_id, other_user_id, false)
        .await
        .expect("Failed to check edit permission");
    assert!(!can_edit);
}

#[tokio::test]
async fn test_can_user_edit_comment_admin() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let admin_id = create_test_user(&db, "admin", true).await;
    let archive_id = create_test_archive(&db).await;

    // Create a comment
    let comment_id = create_comment(db.pool(), archive_id, user_id, "Test comment")
        .await
        .unwrap();

    // Admin can always edit
    let can_edit = can_user_edit_comment(db.pool(), comment_id, admin_id, true)
        .await
        .expect("Failed to check edit permission");
    assert!(can_edit);
}

#[tokio::test]
async fn test_nested_comment_threads() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let archive_id = create_test_archive(&db).await;

    // Create a top-level comment
    let top_level_id = create_comment(db.pool(), archive_id, user_id, "Top level comment")
        .await
        .unwrap();

    // Create a reply
    let reply1_id =
        create_comment_reply(db.pool(), archive_id, user_id, top_level_id, "First reply")
            .await
            .unwrap();

    // Create a nested reply
    let reply2_id = create_comment_reply(db.pool(), archive_id, user_id, reply1_id, "Nested reply")
        .await
        .unwrap();

    // Verify structure
    let top_comment = get_comment_with_author(db.pool(), top_level_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(top_comment.parent_comment_id, None);

    let reply1 = get_comment_with_author(db.pool(), reply1_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reply1.parent_comment_id, Some(top_level_id));

    let reply2 = get_comment_with_author(db.pool(), reply2_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reply2.parent_comment_id, Some(reply1_id));
}

#[tokio::test]
async fn test_comments_for_different_archives_isolated() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let archive1_id = create_test_archive(&db).await;
    let archive2_id = create_test_archive(&db).await;

    // Create comments on different archives
    create_comment(db.pool(), archive1_id, user_id, "Comment on archive 1")
        .await
        .unwrap();
    create_comment(db.pool(), archive2_id, user_id, "Comment on archive 2")
        .await
        .unwrap();

    // Verify each archive only sees its own comments
    let comments1 = get_comments_for_archive(db.pool(), archive1_id)
        .await
        .unwrap();
    assert_eq!(comments1.len(), 1);
    assert_eq!(comments1[0].content, "Comment on archive 1");

    let comments2 = get_comments_for_archive(db.pool(), archive2_id)
        .await
        .unwrap();
    assert_eq!(comments2.len(), 1);
    assert_eq!(comments2[0].content, "Comment on archive 2");
}

#[tokio::test]
async fn test_edit_history_order() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let archive_id = create_test_archive(&db).await;

    // Create and edit a comment
    let comment_id = create_comment(db.pool(), archive_id, user_id, "Version 1")
        .await
        .unwrap();

    // Wait a tiny bit to ensure different timestamps
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    update_comment(db.pool(), comment_id, "Version 2", user_id)
        .await
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    update_comment(db.pool(), comment_id, "Version 3", user_id)
        .await
        .unwrap();

    // Get edit history
    let history = get_comment_edit_history(db.pool(), comment_id)
        .await
        .unwrap();

    // Should be ordered chronologically (oldest first)
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].previous_content, "Version 1");
    assert_eq!(history[1].previous_content, "Version 2");
    // Timestamps should be ascending or equal (SQLite datetime may have same second)
    assert!(history[0].edited_at <= history[1].edited_at);
}

#[tokio::test]
async fn test_comment_with_no_user() {
    let (db, _temp_dir) = setup_test_db().await;
    let archive_id = create_test_archive(&db).await;

    // Create a comment without a user (anonymous/system comment)
    let result = sqlx::query("INSERT INTO comments (archive_id, content) VALUES (?, ?)")
        .bind(archive_id)
        .bind("Anonymous comment")
        .execute(db.pool())
        .await
        .expect("Failed to insert anonymous comment");

    let comment_id = result.last_insert_rowid();

    // Retrieve the comment
    let comment = get_comment_with_author(db.pool(), comment_id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(comment.user_id, None);
    assert_eq!(comment.author_username, None);
    assert_eq!(comment.author_display_name, None);
    assert_eq!(comment.content, "Anonymous comment");
}

#[tokio::test]
async fn test_deleted_comments_still_retrievable() {
    let (db, _temp_dir) = setup_test_db().await;
    let user_id = create_test_user(&db, "testuser", false).await;
    let archive_id = create_test_archive(&db).await;

    // Create and delete a comment
    let comment_id = create_comment(db.pool(), archive_id, user_id, "To be deleted")
        .await
        .unwrap();

    soft_delete_comment(db.pool(), comment_id, false)
        .await
        .unwrap();

    // Comment should still be retrievable
    let comment = get_comment_with_author(db.pool(), comment_id)
        .await
        .unwrap()
        .unwrap();

    assert!(comment.is_deleted);
    // Content preserved for moderation/audit purposes
    assert_eq!(comment.content, "To be deleted");
}
