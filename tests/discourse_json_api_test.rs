//! Live API test for Discourse JSON pagination.
//!
//! This test hits the real Discourse API to verify our pagination logic works correctly.

use discourse_link_archiver::db::DiscoursePostsResponse;

/// Test pagination with a long thread (446 posts).
#[tokio::test]
#[ignore] // Ignore by default since it makes real network requests
async fn test_pagination_long_thread() {
    let client = reqwest::Client::new();
    let base_url = "https://discuss.criticalfallibilism.com";
    let topic_id = 2108;

    let mut all_posts = Vec::new();
    let mut seen_post_ids = std::collections::HashSet::new();
    let mut page_num = 0;
    const PAGE_SIZE: i64 = 20;

    loop {
        let post_number = (page_num * PAGE_SIZE) + 1;
        let url = format!("{}/t/{}/{}.json", base_url, topic_id, post_number);

        println!("Fetching: {} (page {})", url, page_num);

        let response = client.get(&url).send().await.unwrap();
        assert!(response.status().is_success(), "Failed to fetch: {}", url);

        let batch: DiscoursePostsResponse = response.json().await.unwrap();

        if batch.post_stream.posts.is_empty() {
            println!("No more posts");
            break;
        }

        println!("Got {} posts", batch.post_stream.posts.len());

        let batch_size = batch.post_stream.posts.len();
        let mut batch_new_count = 0;
        for post in batch.post_stream.posts {
            // Track posts to detect duplicates
            if !seen_post_ids.insert(post.id) {
                // Skip duplicates
                continue;
            }
            batch_new_count += 1;
            all_posts.push(post);
        }

        println!(
            "  Found {} new posts, {} duplicates",
            batch_new_count,
            batch_size - batch_new_count
        );

        page_num += 1;

        // Stop when we've gone far enough - if we've requested beyond 2x the expected thread length
        // and haven't found new posts in this batch, we're done
        if page_num > 50 && batch_new_count == 0 {
            println!("Gone far enough with no new posts in this batch, stopping");
            break;
        }
    }

    // Final check: fetch the latest posts to ensure complete coverage
    println!("\nFinal check: fetching latest posts");
    let final_url = format!("{}/t/{}/9999.json", base_url, topic_id);
    let final_response = client.get(&final_url).send().await.unwrap();

    if final_response.status().is_success() {
        let final_batch: DiscoursePostsResponse = final_response.json().await.unwrap();
        let mut final_new_count = 0;

        for post in final_batch.post_stream.posts {
            if seen_post_ids.insert(post.id) {
                final_new_count += 1;
                all_posts.push(post);
            }
        }

        if final_new_count > 0 {
            println!(
                "  Found {} additional posts in final check",
                final_new_count
            );
        } else {
            println!("  No additional posts found");
        }
    }

    println!("\nTotal posts fetched: {}", all_posts.len());
    println!("Unique posts: {}", seen_post_ids.len());

    // Get post_number range
    let post_numbers: Vec<i64> = all_posts.iter().map(|p| p.post_number).collect();
    let min_post_number = post_numbers.iter().min().copied().unwrap_or(0);
    let max_post_number = post_numbers.iter().max().copied().unwrap_or(0);
    println!(
        "Post number range: {} to {}",
        min_post_number, max_post_number
    );

    // Thread 2108 should have at least 446 posts (may have more if new posts were added)
    assert!(
        all_posts.len() >= 446,
        "Expected at least 446 posts, got {}",
        all_posts.len()
    );

    // All posts should be unique (no duplicates)
    assert_eq!(
        all_posts.len(),
        seen_post_ids.len(),
        "Found duplicate posts!"
    );
}

/// Test pagination with a short thread to verify edge case behavior.
#[tokio::test]
#[ignore] // Ignore by default since it makes real network requests
async fn test_pagination_short_thread() {
    let client = reqwest::Client::new();
    let base_url = "https://discuss.criticalfallibilism.com";
    let topic_id = 2147; // Shorter thread

    let mut all_posts = Vec::new();
    let mut post_number = 1;
    const CHUNK_SIZE: i64 = 20;

    loop {
        let url = if post_number == 1 {
            format!("{}/t/{}/posts.json", base_url, topic_id)
        } else {
            format!(
                "{}/t/{}/posts.json?post_number={}",
                base_url, topic_id, post_number
            )
        };

        println!("Fetching: {}", url);

        let response = client.get(&url).send().await.unwrap();
        assert!(response.status().is_success());

        let batch: DiscoursePostsResponse = response.json().await.unwrap();

        if batch.post_stream.posts.is_empty() {
            println!("No more posts");
            break;
        }

        println!(
            "Got {} posts at post_number={}",
            batch.post_stream.posts.len(),
            post_number
        );

        all_posts.extend(batch.post_stream.posts);
        post_number += CHUNK_SIZE;
    }

    println!("\nTotal posts fetched: {}", all_posts.len());

    // Short thread should have fewer posts, verify pagination stops correctly
    assert!(
        all_posts.len() > 0,
        "Should have fetched at least some posts"
    );
}

/// Test that we can extract all required fields from a post.
#[tokio::test]
#[ignore]
async fn test_post_fields() {
    let client = reqwest::Client::new();
    let url = "https://discuss.criticalfallibilism.com/t/2108/posts.json";

    let response = client.get(url).send().await.unwrap();
    let batch: DiscoursePostsResponse = response.json().await.unwrap();

    assert!(!batch.post_stream.posts.is_empty());

    let post = &batch.post_stream.posts[0];
    assert!(post.id > 0);
    assert!(post.post_number > 0);
    assert!(!post.username.is_empty());
    assert!(post.topic_id > 0);
    assert!(!post.topic_slug.is_empty());
    assert!(!post.created_at.is_empty());
    assert!(!post.cooked.is_empty());

    // First post should have posts_count
    if post.post_number == 1 {
        assert!(
            post.posts_count.is_some(),
            "First post should have posts_count"
        );
    }

    println!("Post structure looks good:");
    println!("  id: {}", post.id);
    println!("  post_number: {}", post.post_number);
    println!("  username: {}", post.username);
    println!("  topic_id: {}", post.topic_id);
    println!("  posts_count: {:?}", post.posts_count);
}
