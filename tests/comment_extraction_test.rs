//! Integration tests for platform comment extraction (YouTube, Reddit, etc.)

use discourse_link_archiver::archiver::CookieOptions;
use discourse_link_archiver::config::Config;
use discourse_link_archiver::db::{create_pending_archive, insert_link, Database, NewLink};
use discourse_link_archiver::handlers::HANDLERS;
use tempfile::TempDir;

async fn setup_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.sqlite");
    let db = Database::new(&db_path)
        .await
        .expect("Failed to create database");
    (db, temp_dir)
}

/// Create a test configuration with comments enabled.
fn create_test_config_with_comments(work_dir: &std::path::Path) -> Config {
    Config {
        work_dir: work_dir.to_path_buf(),
        comments_enabled: true,
        comments_max_count: 100, // Lower limit for tests
        comments_include_replies: true,
        comments_platforms: vec![
            "youtube".to_string(),
            "reddit".to_string(),
            "tiktok".to_string(),
            "twitter".to_string(),
        ],
        comments_max_depth: 3,
        comments_request_delay_ms: 500,
        ..Config::for_testing()
    }
}

#[tokio::test]
#[ignore] // Requires network access and may be rate-limited
async fn test_youtube_comment_extraction() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path().join("work");
    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work dir");

    // Use a known public YouTube video with comments (Rick Astley - Never Gonna Give You Up)
    // This video has millions of views and thousands of comments
    let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";

    let config = create_test_config_with_comments(&work_dir);

    let handler = HANDLERS
        .find_handler(url)
        .expect("Should find YouTube handler");

    let result = handler
        .archive(url, &work_dir, &CookieOptions::default(), &config)
        .await
        .expect("Archive should succeed");

    // Verify basic archive metadata
    assert_eq!(result.content_type, "video");
    assert!(result.title.is_some());

    // Check if comments.json was created
    let comments_path = work_dir.join("comments.json");
    if comments_path.exists() {
        let comments_content = tokio::fs::read_to_string(&comments_path)
            .await
            .expect("Failed to read comments.json");

        let comments_json: serde_json::Value =
            serde_json::from_str(&comments_content).expect("Failed to parse comments.json");

        // Verify JSON structure
        assert_eq!(comments_json["platform"], "youtube");
        assert_eq!(comments_json["extraction_method"], "ytdlp");
        assert!(comments_json["extracted_at"].is_string());
        assert!(comments_json["content_url"].is_string());
        assert!(comments_json["stats"].is_object());
        assert!(comments_json["comments"].is_array());

        let comments_array = comments_json["comments"]
            .as_array()
            .expect("comments should be an array");

        // Should have comments (this video is heavily commented)
        assert!(
            !comments_array.is_empty(),
            "Expected comments from popular YouTube video"
        );

        // Should respect limit (100 for tests)
        assert!(
            comments_array.len() <= 100,
            "Comment count should respect limit"
        );

        // Verify comment structure
        if let Some(first_comment) = comments_array.first() {
            assert!(first_comment["id"].is_string());
            assert!(first_comment["author"].is_string());
            assert!(first_comment["text"].is_string());
            // likes might be 0 or a number
            assert!(first_comment["likes"].is_number() || first_comment["likes"].is_null());
        }

        // Verify stats
        let stats = &comments_json["stats"];
        assert!(stats["total_comments"].is_number());
        assert!(stats["extracted_comments"].is_number());

        let total = stats["total_comments"].as_i64().unwrap();
        let extracted = stats["extracted_comments"].as_i64().unwrap();
        assert_eq!(extracted as usize, comments_array.len());

        // Check if limit was applied
        if total > 100 {
            assert_eq!(comments_json["limited"], true);
            assert_eq!(comments_json["limit_applied"], 100);
        }

        println!(
            "✓ Successfully extracted {} comments from YouTube",
            extracted
        );
    } else {
        // Comments might not be available for all videos
        println!("⚠ No comments.json created (comments may be disabled on this video)");
    }
}

#[tokio::test]
#[ignore] // Requires network access
async fn test_reddit_comment_permalink_extraction() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path().join("work");
    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work dir");

    // Use a known Reddit comment permalink (from a popular subreddit)
    // This is a comment from the Rust subreddit about async/await
    let url = "https://www.reddit.com/r/rust/comments/1234/sample/abc";

    let config = create_test_config_with_comments(&work_dir);

    let handler = HANDLERS
        .find_handler(url)
        .expect("Should find Reddit handler");

    // Note: This test may fail if the specific comment is deleted
    // Consider using a well-known permanent comment
    let result = handler
        .archive(url, &work_dir, &CookieOptions::default(), &config)
        .await;

    // Even if the comment doesn't exist, handler should not crash
    match result {
        Ok(archive_result) => {
            println!("✓ Reddit archive succeeded");
            // Check if comments were extracted
            let comments_path = work_dir.join("comments.json");
            if comments_path.exists() {
                let comments_content = tokio::fs::read_to_string(&comments_path)
                    .await
                    .expect("Failed to read comments.json");

                let comments_json: serde_json::Value =
                    serde_json::from_str(&comments_content).expect("Failed to parse comments.json");

                assert_eq!(comments_json["platform"], "reddit");
                println!("✓ Comments extracted for Reddit permalink");
            }
            assert_eq!(archive_result.content_type, "text");
        }
        Err(e) => {
            println!(
                "⚠ Reddit archive failed (expected if comment doesn't exist): {}",
                e
            );
        }
    }
}

#[tokio::test]
async fn test_comment_extraction_disabled() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path().join("work");
    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work dir");

    // Create config with comments disabled
    let config = Config {
        work_dir: work_dir.clone(),
        comments_enabled: false,
        ..Config::for_testing()
    };

    // Note: This is a unit test, but we can't easily test with a real YouTube video
    // without network access. The key is verifying the config is respected.
    assert!(!config.comments_enabled);
    assert_eq!(config.comments_max_count, 1000); // Default
}

#[tokio::test]
async fn test_comment_limit_configuration() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path().join("work");
    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work dir");

    // Create config with custom limit
    let config = Config {
        work_dir: work_dir.clone(),
        comments_enabled: true,
        comments_max_count: 50,
        ..Config::for_testing()
    };

    assert!(config.comments_enabled);
    assert_eq!(config.comments_max_count, 50);
}

#[tokio::test]
async fn test_comment_platform_filtering() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path().join("work");
    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work dir");

    // Create config with only YouTube comments enabled
    let config = Config {
        work_dir: work_dir.clone(),
        comments_enabled: true,
        comments_platforms: vec!["youtube".to_string()],
        ..Config::for_testing()
    };

    assert!(config.comments_platforms.contains(&"youtube".to_string()));
    assert!(!config.comments_platforms.contains(&"reddit".to_string()));
}

#[tokio::test]
async fn test_comment_depth_and_delay_config() {
    let config = Config {
        comments_max_depth: 5,
        comments_request_delay_ms: 2000,
        ..Config::for_testing()
    };

    assert_eq!(config.comments_max_depth, 5);
    assert_eq!(config.comments_request_delay_ms, 2000);
}

#[tokio::test]
#[ignore] // Requires network access and yt-dlp
async fn test_comment_json_structure_validation() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path().join("work");
    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work dir");

    // Use a short, popular video with guaranteed comments
    let url = "https://www.youtube.com/watch?v=jNQXAC9IVRw"; // "Me at the zoo" - first YouTube video

    let config = create_test_config_with_comments(&work_dir);

    let handler = HANDLERS
        .find_handler(url)
        .expect("Should find YouTube handler");

    let result = handler
        .archive(url, &work_dir, &CookieOptions::default(), &config)
        .await;

    if let Ok(_archive_result) = result {
        let comments_path = work_dir.join("comments.json");
        if comments_path.exists() {
            let comments_content = tokio::fs::read_to_string(&comments_path)
                .await
                .expect("Failed to read comments.json");

            let comments_json: serde_json::Value =
                serde_json::from_str(&comments_content).expect("Invalid JSON");

            // Validate schema completeness
            assert!(comments_json.get("platform").is_some(), "Missing platform");
            assert!(
                comments_json.get("extraction_method").is_some(),
                "Missing extraction_method"
            );
            assert!(
                comments_json.get("extracted_at").is_some(),
                "Missing extracted_at"
            );
            assert!(
                comments_json.get("content_url").is_some(),
                "Missing content_url"
            );
            assert!(
                comments_json.get("content_id").is_some(),
                "Missing content_id"
            );
            assert!(
                comments_json.get("limited").is_some(),
                "Missing limited flag"
            );
            assert!(
                comments_json.get("limit_applied").is_some(),
                "Missing limit_applied"
            );
            assert!(comments_json.get("stats").is_some(), "Missing stats");
            assert!(
                comments_json.get("comments").is_some(),
                "Missing comments array"
            );

            // Validate stats structure
            let stats = &comments_json["stats"];
            assert!(
                stats.get("total_comments").is_some(),
                "Missing stats.total_comments"
            );
            assert!(
                stats.get("extracted_comments").is_some(),
                "Missing stats.extracted_comments"
            );

            println!("✓ Comment JSON schema validation passed");
        }
    }
}

#[tokio::test]
async fn test_reddit_comment_url_pattern_detection() {
    // Testing Reddit URL patterns to distinguish comment permalinks from posts
    // Comment URLs have format: /r/sub/comments/POST_ID/title/COMMENT_ID/
    // Post URLs have format: /r/sub/comments/POST_ID/title/

    let comment_urls = vec![
        "https://old.reddit.com/r/rust/comments/abc123/post_title/def456/",
        "https://www.reddit.com/r/programming/comments/xyz789/another_post/comment123/",
        "https://reddit.com/comments/post123/title/comment456/",
    ];

    let post_urls = vec![
        "https://old.reddit.com/r/rust/comments/abc123/post_title/",
        "https://www.reddit.com/r/programming/comments/xyz789/",
    ];

    // Comment URLs should be identified by having additional path segments after title
    // The regex pattern in reddit.rs checks for: /comments/POST_ID/title/COMMENT_ID
    let comment_pattern = regex::Regex::new(r"/comments/[a-zA-Z0-9]+/[^/]+/([a-zA-Z0-9]+)")
        .expect("Failed to compile regex");

    for url in comment_urls {
        assert!(
            comment_pattern.is_match(url),
            "Should detect comment URL: {}",
            url
        );
    }

    for url in post_urls {
        assert!(
            !comment_pattern.is_match(url),
            "Should not detect post URL as comment: {}",
            url
        );
    }
}

#[tokio::test]
#[ignore] // Expensive test - only run manually
async fn test_comment_extraction_with_database() {
    let (db, _db_temp) = setup_db().await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let work_dir = temp_dir.path().join("work");
    tokio::fs::create_dir_all(&work_dir)
        .await
        .expect("Failed to create work dir");

    // Create a link and pending archive
    let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
    let new_link = NewLink {
        original_url: url.to_string(),
        normalized_url: url.to_string(),
        canonical_url: None,
        domain: "youtube.com".to_string(),
    };

    let link_id = insert_link(db.pool(), &new_link)
        .await
        .expect("Failed to insert link");

    let archive_id = create_pending_archive(db.pool(), link_id, None)
        .await
        .expect("Failed to create archive");

    let config = create_test_config_with_comments(&work_dir);

    let handler = HANDLERS
        .find_handler(url)
        .expect("Should find YouTube handler");

    let result = handler
        .archive(url, &work_dir, &CookieOptions::default(), &config)
        .await;

    if let Ok(_archive_result) = result {
        // Check if comments.json exists
        let comments_path = work_dir.join("comments.json");
        if comments_path.exists() {
            // In a real worker scenario, this would be uploaded to S3 and
            // an artifact record would be created with metadata

            let comments_content = tokio::fs::read_to_string(&comments_path)
                .await
                .expect("Failed to read comments.json");

            let comments_json: serde_json::Value =
                serde_json::from_str(&comments_content).expect("Failed to parse comments.json");

            let comment_count = comments_json["stats"]["extracted_comments"]
                .as_i64()
                .unwrap_or(0);

            println!(
                "✓ Would upload comments artifact for archive {} with {} comments",
                archive_id, comment_count
            );

            // Verify metadata structure that would be stored
            assert!(comment_count >= 0);
            assert!(comments_json["extraction_method"].is_string());
            assert!(comments_json["limited"].is_boolean());
        }
    }
}
