//! Integration tests for authentication system.

use discourse_link_archiver::auth::{
    generate_password, generate_username, hash_password, validate_password_strength,
    verify_password,
};
use discourse_link_archiver::db::{
    count_users, create_session, create_user, delete_session, delete_user_sessions,
    get_session_by_token, get_user_by_id, get_user_by_username, increment_failed_login_attempts,
    lock_user_until, reset_failed_login_attempts, update_user_active, update_user_admin,
    update_user_approval, update_user_password, update_user_profile, Database,
};
use serial_test::serial;
use tempfile::TempDir;

async fn setup_test_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path)
        .await
        .expect("Failed to create database");
    (db, temp_dir)
}

#[tokio::test]
async fn test_password_hashing() {
    let password = "SecureP@ssw0rd123";
    let hash = hash_password(password).expect("Failed to hash password");

    // Verify correct password
    assert!(verify_password(password, &hash).expect("Failed to verify password"));

    // Verify wrong password
    assert!(!verify_password("WrongPassword", &hash).expect("Failed to verify password"));
}

#[tokio::test]
async fn test_password_strength_validation() {
    // Valid passwords (10+ characters)
    assert!(validate_password_strength("abcdefghij").is_ok());
    assert!(validate_password_strength("1234567890").is_ok());
    assert!(validate_password_strength("MyP@ssw0rd123").is_ok());
    assert!(validate_password_strength("alllowercase").is_ok());
    assert!(validate_password_strength("ALLUPPERCASE").is_ok());

    // Too short (< 10 characters)
    assert!(validate_password_strength("Short1!").is_err());
    assert!(validate_password_strength("123456789").is_err());
}

#[tokio::test]
async fn test_username_generation() {
    let username1 = generate_username();
    let username2 = generate_username();

    // Should be at least 8 characters
    assert!(username1.len() >= 8);
    assert!(username2.len() >= 8);

    // Should end with 4 digits
    let last_four: String = username1.chars().rev().take(4).collect();
    assert!(last_four.chars().all(|c| c.is_ascii_digit()));
}

#[tokio::test]
async fn test_password_generation() {
    let password = generate_password(16);

    assert_eq!(password.len(), 16);

    // Should contain diverse characters
    let has_lowercase = password.chars().any(|c| c.is_ascii_lowercase());
    let has_uppercase = password.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_special = password.chars().any(|c| !c.is_alphanumeric());

    assert!(has_lowercase, "Password should have lowercase");
    assert!(has_uppercase, "Password should have uppercase");
    assert!(has_digit, "Password should have digit");
    assert!(has_special, "Password should have special char");
}

#[tokio::test]
#[serial]
async fn test_user_creation_first_user_is_admin() {
    let (db, _temp_dir) = setup_test_db().await;

    // First user should be admin
    let password_hash = hash_password("password123").expect("Failed to hash password");
    let user_id = create_user(db.pool(), "admin_user", &password_hash, true)
        .await
        .expect("Failed to create user");

    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    assert_eq!(user.username, "admin_user");
    assert!(user.is_admin, "First user should be admin");
    assert!(user.is_approved, "First user should be approved");
    assert!(user.is_active, "User should be active");
}

#[tokio::test]
#[serial]
async fn test_user_creation_subsequent_users_need_approval() {
    let (db, _temp_dir) = setup_test_db().await;

    // Create first user (admin)
    let password_hash1 = hash_password("password123").expect("Failed to hash password");
    create_user(db.pool(), "admin_user", &password_hash1, true)
        .await
        .expect("Failed to create first user");

    // Create second user (not admin)
    let password_hash2 = hash_password("password456").expect("Failed to hash password");
    let user_id2 = create_user(db.pool(), "regular_user", &password_hash2, false)
        .await
        .expect("Failed to create second user");

    let user2 = get_user_by_id(db.pool(), user_id2)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    assert!(!user2.is_admin, "Second user should not be admin");
    assert!(!user2.is_approved, "Second user should not be approved");
}

#[tokio::test]
#[serial]
async fn test_get_user_by_username() {
    let (db, _temp_dir) = setup_test_db().await;

    let password_hash = hash_password("password123").expect("Failed to hash password");
    let user_id = create_user(db.pool(), "test_user", &password_hash, true)
        .await
        .expect("Failed to create user");

    let user = get_user_by_username(db.pool(), "test_user")
        .await
        .expect("Failed to get user")
        .expect("User not found");

    assert_eq!(user.id, user_id);
    assert_eq!(user.username, "test_user");

    // Non-existent user
    let result = get_user_by_username(db.pool(), "nonexistent")
        .await
        .expect("Failed to query");
    assert!(result.is_none());
}

#[tokio::test]
#[serial]
async fn test_count_users() {
    let (db, _temp_dir) = setup_test_db().await;

    let count = count_users(db.pool()).await.expect("Failed to count users");
    assert_eq!(count, 0);

    // Create users
    let password_hash = hash_password("password123").expect("Failed to hash password");
    create_user(db.pool(), "user1", &password_hash, true)
        .await
        .expect("Failed to create user");
    create_user(db.pool(), "user2", &password_hash, false)
        .await
        .expect("Failed to create user");

    let count = count_users(db.pool()).await.expect("Failed to count users");
    assert_eq!(count, 2);
}

#[tokio::test]
#[serial]
async fn test_user_approval() {
    let (db, _temp_dir) = setup_test_db().await;

    let password_hash = hash_password("password123").expect("Failed to hash password");
    let user_id = create_user(db.pool(), "pending_user", &password_hash, false)
        .await
        .expect("Failed to create user");

    // User should not be approved initially
    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert!(!user.is_approved);

    // Approve user
    update_user_approval(db.pool(), user_id, true)
        .await
        .expect("Failed to approve user");

    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert!(user.is_approved);
}

#[tokio::test]
#[serial]
async fn test_user_admin_promotion() {
    let (db, _temp_dir) = setup_test_db().await;

    let password_hash = hash_password("password123").expect("Failed to hash password");
    let user_id = create_user(db.pool(), "regular_user", &password_hash, false)
        .await
        .expect("Failed to create user");

    // User should not be admin initially
    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert!(!user.is_admin);

    // Promote to admin
    update_user_admin(db.pool(), user_id, true)
        .await
        .expect("Failed to promote user");

    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert!(user.is_admin);

    // Demote from admin
    update_user_admin(db.pool(), user_id, false)
        .await
        .expect("Failed to demote user");

    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert!(!user.is_admin);
}

#[tokio::test]
#[serial]
async fn test_user_deactivation() {
    let (db, _temp_dir) = setup_test_db().await;

    let password_hash = hash_password("password123").expect("Failed to hash password");
    let user_id = create_user(db.pool(), "active_user", &password_hash, true)
        .await
        .expect("Failed to create user");

    // User should be active initially
    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert!(user.is_active);

    // Deactivate user
    update_user_active(db.pool(), user_id, false)
        .await
        .expect("Failed to deactivate user");

    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert!(!user.is_active);
}

#[tokio::test]
#[serial]
async fn test_password_update() {
    let (db, _temp_dir) = setup_test_db().await;

    let old_password = "OldP@ssw0rd123";
    let new_password = "NewP@ssw0rd456";

    let old_hash = hash_password(old_password).expect("Failed to hash old password");
    let user_id = create_user(db.pool(), "test_user", &old_hash, true)
        .await
        .expect("Failed to create user");

    // Verify old password works
    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert!(verify_password(old_password, &user.password_hash).unwrap());

    // Update password
    let new_hash = hash_password(new_password).expect("Failed to hash new password");
    update_user_password(db.pool(), user_id, &new_hash)
        .await
        .expect("Failed to update password");

    // Verify new password works and old doesn't
    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert!(verify_password(new_password, &user.password_hash).unwrap());
    assert!(!verify_password(old_password, &user.password_hash).unwrap());
}

#[tokio::test]
#[serial]
async fn test_profile_update() {
    let (db, _temp_dir) = setup_test_db().await;

    let password_hash = hash_password("password123").expect("Failed to hash password");
    let user_id = create_user(db.pool(), "test_user", &password_hash, true)
        .await
        .expect("Failed to create user");

    // Update profile
    update_user_profile(
        db.pool(),
        user_id,
        Some("test@example.com"),
        Some("Test User"),
    )
    .await
    .expect("Failed to update profile");

    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");

    assert_eq!(user.email, Some("test@example.com".to_string()));
    assert_eq!(user.display_name, Some("Test User".to_string()));
}

#[tokio::test]
#[serial]
async fn test_failed_login_attempts() {
    let (db, _temp_dir) = setup_test_db().await;

    let password_hash = hash_password("password123").expect("Failed to hash password");
    let user_id = create_user(db.pool(), "test_user", &password_hash, true)
        .await
        .expect("Failed to create user");

    // Initial failed attempts should be 0
    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert_eq!(user.failed_login_attempts, 0);

    // Increment failed attempts
    increment_failed_login_attempts(db.pool(), user_id)
        .await
        .expect("Failed to increment");

    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert_eq!(user.failed_login_attempts, 1);

    // Increment again
    increment_failed_login_attempts(db.pool(), user_id)
        .await
        .expect("Failed to increment");

    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert_eq!(user.failed_login_attempts, 2);

    // Reset
    reset_failed_login_attempts(db.pool(), user_id)
        .await
        .expect("Failed to reset");

    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert_eq!(user.failed_login_attempts, 0);
    assert!(user.locked_until.is_none());
}

#[tokio::test]
#[serial]
async fn test_account_locking() {
    let (db, _temp_dir) = setup_test_db().await;

    let password_hash = hash_password("password123").expect("Failed to hash password");
    let user_id = create_user(db.pool(), "test_user", &password_hash, true)
        .await
        .expect("Failed to create user");

    // Lock account
    let locked_until = "2026-12-31T23:59:59Z";
    lock_user_until(db.pool(), user_id, locked_until)
        .await
        .expect("Failed to lock user");

    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert_eq!(user.locked_until, Some(locked_until.to_string()));

    // Reset (unlock)
    reset_failed_login_attempts(db.pool(), user_id)
        .await
        .expect("Failed to reset");

    let user = get_user_by_id(db.pool(), user_id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert!(user.locked_until.is_none());
}

#[tokio::test]
#[serial]
async fn test_session_creation() {
    let (db, _temp_dir) = setup_test_db().await;

    let password_hash = hash_password("password123").expect("Failed to hash password");
    let user_id = create_user(db.pool(), "test_user", &password_hash, true)
        .await
        .expect("Failed to create user");

    // Create session
    let token = "test_session_token_12345";
    let csrf_token = "test_csrf_token_67890";
    let ip_address = "127.0.0.1";
    let user_agent = Some("Mozilla/5.0");
    let expires_at = "2026-12-31T23:59:59Z";

    let session_id = create_session(
        db.pool(),
        user_id,
        token,
        csrf_token,
        ip_address,
        user_agent,
        expires_at,
    )
    .await
    .expect("Failed to create session");

    // Retrieve session
    let session = get_session_by_token(db.pool(), token)
        .await
        .expect("Failed to get session")
        .expect("Session not found");

    assert_eq!(session.id, session_id);
    assert_eq!(session.user_id, user_id);
    assert_eq!(session.token, token);
    assert_eq!(session.csrf_token, csrf_token);
    assert_eq!(session.ip_address, ip_address);
    assert_eq!(session.user_agent, user_agent.map(String::from));
}

#[tokio::test]
#[serial]
async fn test_session_deletion() {
    let (db, _temp_dir) = setup_test_db().await;

    let password_hash = hash_password("password123").expect("Failed to hash password");
    let user_id = create_user(db.pool(), "test_user", &password_hash, true)
        .await
        .expect("Failed to create user");

    // Create session
    let token = "test_session_token";
    create_session(
        db.pool(),
        user_id,
        token,
        "csrf_token",
        "127.0.0.1",
        None,
        "2026-12-31T23:59:59Z",
    )
    .await
    .expect("Failed to create session");

    // Verify session exists
    let session = get_session_by_token(db.pool(), token)
        .await
        .expect("Failed to get session");
    assert!(session.is_some());

    // Delete session
    delete_session(db.pool(), token)
        .await
        .expect("Failed to delete session");

    // Verify session no longer exists
    let session = get_session_by_token(db.pool(), token)
        .await
        .expect("Failed to get session");
    assert!(session.is_none());
}

#[tokio::test]
#[serial]
async fn test_delete_user_sessions() {
    let (db, _temp_dir) = setup_test_db().await;

    let password_hash = hash_password("password123").expect("Failed to hash password");
    let user_id = create_user(db.pool(), "test_user", &password_hash, true)
        .await
        .expect("Failed to create user");

    // Create multiple sessions for the user
    create_session(
        db.pool(),
        user_id,
        "token1",
        "csrf1",
        "127.0.0.1",
        None,
        "2026-12-31T23:59:59Z",
    )
    .await
    .expect("Failed to create session");

    create_session(
        db.pool(),
        user_id,
        "token2",
        "csrf2",
        "127.0.0.1",
        None,
        "2026-12-31T23:59:59Z",
    )
    .await
    .expect("Failed to create session");

    // Verify both sessions exist
    assert!(get_session_by_token(db.pool(), "token1")
        .await
        .unwrap()
        .is_some());
    assert!(get_session_by_token(db.pool(), "token2")
        .await
        .unwrap()
        .is_some());

    // Delete all user sessions
    delete_user_sessions(db.pool(), user_id)
        .await
        .expect("Failed to delete user sessions");

    // Verify both sessions are gone
    assert!(get_session_by_token(db.pool(), "token1")
        .await
        .unwrap()
        .is_none());
    assert!(get_session_by_token(db.pool(), "token2")
        .await
        .unwrap()
        .is_none());
}
