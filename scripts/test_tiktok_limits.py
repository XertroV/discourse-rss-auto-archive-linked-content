#!/usr/bin/env python3
"""
Test script to determine TikTok API limits for comment fetching
Tests:
- Maximum comments retrievable
- Optimal chunk size per request
- Request rate limits
"""

import requests
import sys
import time
from urllib.parse import urlparse


def extract_video_id(url):
    """Extract video ID from TikTok URL"""
    if 'vm.tiktok.com' in url or 'vt.tiktok.com' in url:
        response = requests.get(url, allow_redirects=True)
        url = response.url

    if '/video/' in url:
        return url.split('/video/')[1].split('?')[0].split('/')[0]

    return None


def test_chunk_size(video_id, chunk_size):
    """Test a specific chunk size to see if it works"""
    headers = {
        'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
        'Referer': f'https://www.tiktok.com/@i/video/{video_id}'
    }

    url = f'https://www.tiktok.com/api/comment/list/?aid=1988&aweme_id={video_id}&count={chunk_size}&cursor=0'

    try:
        response = requests.get(url, headers=headers, timeout=10)
        if response.status_code != 200:
            return False, 0, f"HTTP {response.status_code}"

        data = response.json()
        comments_received = len(data.get('comments', []))
        return True, comments_received, "OK"
    except Exception as e:
        return False, 0, str(e)


def fetch_comments_with_limit(video_id, target_count=1000, chunk_size=50):
    """
    Fetch comments up to target_count

    Args:
        video_id: TikTok video ID
        target_count: Target number of comments to fetch
        chunk_size: Number of comments per request

    Returns:
        List of comments and metadata about the fetch
    """
    comments = []
    cursor = 0
    request_count = 0
    start_time = time.time()

    headers = {
        'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
        'Referer': f'https://www.tiktok.com/@i/video/{video_id}'
    }

    print(f"Fetching up to {target_count} comments (chunk size: {chunk_size})...")
    print("-" * 60)

    while len(comments) < target_count:
        url = f'https://www.tiktok.com/api/comment/list/?aid=1988&aweme_id={video_id}&count={chunk_size}&cursor={cursor}'
        request_count += 1

        try:
            response = requests.get(url, headers=headers, timeout=10)

            if response.status_code != 200:
                print(f"✗ Request #{request_count} failed: HTTP {response.status_code}")
                break

            data = response.json()

            # Check for errors in response
            if data.get('status_code') != 0:
                print(f"✗ API error: {data.get('status_msg', 'Unknown error')}")
                break

            # Check if we got comments
            if 'comments' not in data or not data['comments']:
                print(f"✓ No more comments (reached end at cursor={cursor})")
                break

            batch_size = len(data['comments'])

            # Extract comment data
            for comment in data['comments']:
                comments.append({
                    'text': comment.get('text', ''),
                    'author': comment.get('user', {}).get('unique_id', 'unknown'),
                    'likes': comment.get('digg_count', 0),
                    'timestamp': comment.get('create_time', 0),
                })

            print(f"✓ Request #{request_count}: Retrieved {batch_size} comments (total: {len(comments)})")

            # Check if there are more comments
            has_more = data.get('has_more', False)
            if not has_more:
                print(f"✓ API reports no more comments available")
                break

            # Update cursor for next batch
            cursor = data.get('cursor', cursor + chunk_size)

            # Small delay to avoid rate limiting
            time.sleep(0.5)

        except requests.exceptions.RequestException as e:
            print(f"✗ Request error: {e}")
            break
        except (ValueError, KeyError) as e:
            print(f"✗ JSON parsing error: {e}")
            break

    elapsed_time = time.time() - start_time

    return {
        'comments': comments,
        'request_count': request_count,
        'elapsed_time': elapsed_time,
        'comments_per_second': len(comments) / elapsed_time if elapsed_time > 0 else 0,
    }


def main():
    if len(sys.argv) < 2:
        print("Usage: python test_tiktok_limits.py <tiktok_url> [target_count]")
        print("\nExample:")
        print("  python test_tiktok_limits.py https://www.tiktok.com/@username/video/7123456789 1000")
        sys.exit(1)

    url = sys.argv[1]
    target_count = int(sys.argv[2]) if len(sys.argv) > 2 else 1000

    video_id = extract_video_id(url)
    if not video_id:
        print(f"Error: Could not extract video ID from URL: {url}")
        sys.exit(1)

    print("=" * 60)
    print("TikTok Comment Scraper - Limits Testing")
    print("=" * 60)
    print(f"Video ID: {video_id}")
    print(f"Target: {target_count} comments")
    print()

    print("Test 1 showed max 50 chunk size")
    # # Test 1: Different chunk sizes
    # print("TEST 1: Testing different chunk sizes")
    # print("-" * 60)
    # chunk_sizes = [20, 50, 100, 200, 500, 1000]

    # for size in chunk_sizes:
    #     success, received, msg = test_chunk_size(video_id, size)
    #     status = "✓" if success else "✗"
    #     print(f"{status} Chunk size {size:4d}: received {received:4d} comments - {msg}")
    #     time.sleep(0.5)  # Rate limit protection

    # print()

    # Test 2: Fetch target number of comments
    print(f"TEST 2: Attempting to fetch {target_count} comments")
    print("-" * 60)

    # Use chunk size 50 as a reasonable default
    # todo: this should just get first and last chunk
    result = fetch_comments_with_limit(video_id, target_count, chunk_size=50)

    print()
    print("=" * 60)
    print("RESULTS")
    print("=" * 60)
    print(f"Comments retrieved: {len(result['comments'])}")
    print(f"Requests made: {result['request_count']}")
    print(f"Time elapsed: {result['elapsed_time']:.2f}s")
    print(f"Rate: {result['comments_per_second']:.1f} comments/sec")

    if len(result['comments']) >= target_count:
        print(f"\n✓ SUCCESS: Retrieved target of {target_count} comments")
    elif len(result['comments']) > 0:
        print(f"\n⚠ PARTIAL: Retrieved {len(result['comments'])}/{target_count} comments")
        print(f"  (May have reached end of available comments)")
    else:
        print(f"\n✗ FAILED: No comments retrieved")

    # Show sample of comments
    if result['comments']:
        print("\n" + "=" * 60)
        print("SAMPLE COMMENTS (first 5)")
        print("=" * 60)
        for i, comment in enumerate(result['comments'][:5], 1):
            print(f"\n#{i} @{comment['author']} (❤️ {comment['likes']})")
            print(f"  {comment['text'][:100]}{'...' if len(comment['text']) > 100 else ''}")
        print("SAMPLE COMMENTS (last 5)")
        print("=" * 60)
        # todo


if __name__ == '__main__':
    main()
