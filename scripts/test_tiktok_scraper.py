#!/usr/bin/env python3
"""
Test script for TikTok comment scraping
Based on xtekky/TikTok-Comment-Scraper approach
"""

import requests
import sys
from urllib.parse import urlparse, parse_qs


def extract_video_id(url):
    """Extract video ID from TikTok URL"""
    # Handle shortened URLs (vm.tiktok.com, vt.tiktok.com)
    if 'vm.tiktok.com' in url or 'vt.tiktok.com' in url:
        response = requests.get(url, allow_redirects=True)
        url = response.url

    # Extract ID from full URL
    # Format: https://www.tiktok.com/@username/video/7123456789
    if '/video/' in url:
        return url.split('/video/')[1].split('?')[0].split('/')[0]

    return None


def fetch_comments(video_id, max_comments=100):
    """
    Fetch comments from TikTok video

    Args:
        video_id: TikTok video ID
        max_comments: Maximum number of comments to fetch (for testing)

    Returns:
        List of comment dictionaries with text, author, likes, etc.
    """
    comments = []
    cursor = 0

    headers = {
        'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
        'Referer': f'https://www.tiktok.com/@i/video/{video_id}'
    }

    print(f"Fetching comments for video ID: {video_id}")
    print("-" * 60)

    while len(comments) < max_comments:
        url = f'https://www.tiktok.com/api/comment/list/?aid=1988&aweme_id={video_id}&count=50&cursor={cursor}'

        try:
            response = requests.get(url, headers=headers, timeout=10)

            # Check for successful response
            if response.status_code != 200:
                print(f"Error: HTTP {response.status_code}")
                print(f"Response: {response.text[:200]}")
                break

            data = response.json()

            # Check if we got comments
            if 'comments' not in data or not data['comments']:
                print(f"No more comments found (cursor={cursor})")
                break

            # Extract comment data
            for comment in data['comments']:
                comments.append({
                    'text': comment.get('text', ''),
                    'author': comment.get('user', {}).get('unique_id', 'unknown'),
                    'likes': comment.get('digg_count', 0),
                    'timestamp': comment.get('create_time', 0),
                })

                if len(comments) >= max_comments:
                    break

            # Check if there are more comments
            if not data.get('has_more', False):
                print("Reached end of comments")
                break

            # Update cursor for next batch
            cursor = data.get('cursor', cursor + 50)

        except requests.exceptions.RequestException as e:
            print(f"Request error: {e}")
            break
        except (ValueError, KeyError) as e:
            print(f"JSON parsing error: {e}")
            print(f"Response text: {response.text[:500]}")
            break

    return comments


def main():
    if len(sys.argv) < 2:
        print("Usage: python test_tiktok_scraper.py <tiktok_url>")
        print("\nExample:")
        print("  python test_tiktok_scraper.py https://www.tiktok.com/@username/video/7123456789")
        sys.exit(1)

    url = sys.argv[1]
    video_id = extract_video_id(url)

    if not video_id:
        print(f"Error: Could not extract video ID from URL: {url}")
        sys.exit(1)

    # Fetch a small number of comments for testing
    comments = fetch_comments(video_id, max_comments=20)

    print("\n" + "=" * 60)
    print(f"Retrieved {len(comments)} comments:")
    print("=" * 60)

    for i, comment in enumerate(comments, 1):
        print(f"\n#{i} @{comment['author']} (❤️ {comment['likes']})")
        print(f"  {comment['text']}")

    if not comments:
        print("\n⚠️  No comments retrieved - the API may have changed or requires authentication")
        print("This is common with TikTok scrapers as they frequently update their API")
    else:
        print(f"\n✓ Successfully retrieved {len(comments)} comments")


if __name__ == '__main__':
    main()
