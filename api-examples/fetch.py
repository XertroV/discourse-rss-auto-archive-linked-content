#!/usr/bin/env python3
"""
Fetch Discourse API examples and save them to files.
"""

import requests
import json
import os
from pathlib import Path

# Base URL
BASE_URL = "https://discuss.criticalfallibilism.com"

# URL to filename mappings
URLS = [
    ("posts.rss", "posts.rss"),
    ("posts.json", "posts.json"),
    ("t/2144/posts.json", "t_2144_posts.json"),
    ("t/2144/posts.json?post_number=5", "t_2144_posts_pn_5.json"),
    ("t/2144/posts.json?post_number=21", "t_2144_posts_pn_21.json"),
    ("t/2144/posts.json?post_number=41", "t_2144_posts_pn_41.json"),
    ("t/2144/posts.json?post_number=61", "t_2144_posts_pn_61.json"),
    ("t/2144/posts.json?post_number=81", "t_2144_posts_pn_81.json"),
    ("t/2108/posts.json", "t_2108_posts.json"),
    ("t/2108/posts.json?post_number=5", "t_2108_posts_pn_5.json"),
    ("t/2108/posts.json?post_number=21", "t_2108_posts_pn_21.json"),
    ("t/2108/posts.json?post_number=41", "t_2108_posts_pn_41.json"),
    ("t/2108/posts.json?post_number=61", "t_2108_posts_pn_61.json"),
    ("t/2108/posts.json?post_number=81", "t_2108_posts_pn_81.json"),
]

def fetch_and_save(endpoint, filename):
    """Fetch content from URL and save to file."""
    url = f"{BASE_URL}/{endpoint}"
    print(f"Fetching: {url}")

    try:
        response = requests.get(url, timeout=30)
        response.raise_for_status()

        # Get the directory of this script
        script_dir = Path(__file__).parent
        filepath = script_dir / filename

        # Pretty-print JSON files
        if filename.endswith('.json'):
            try:
                json_data = response.json()
                content = json.dumps(json_data, indent=2, ensure_ascii=False)
                with open(filepath, 'w', encoding='utf-8') as f:
                    f.write(content)
            except json.JSONDecodeError as e:
                print(f"  ! Warning: Could not parse JSON, saving raw: {e}")
                with open(filepath, 'wb') as f:
                    f.write(response.content)
        else:
            # Save non-JSON files as-is (like RSS)
            with open(filepath, 'wb') as f:
                f.write(response.content)

        print(f"  ✓ Saved to: {filename} ({len(response.content)} bytes)")
        return True

    except requests.exceptions.RequestException as e:
        print(f"  ✗ Error fetching {url}: {e}")
        return False

def main():
    """Main function to fetch all URLs."""
    print("Starting to fetch Discourse API examples...")
    print(f"Base URL: {BASE_URL}\n")

    success_count = 0
    fail_count = 0

    for endpoint, filename in URLS:
        if fetch_and_save(endpoint, filename):
            success_count += 1
        else:
            fail_count += 1
        print()

    print("=" * 50)
    print(f"Complete! Success: {success_count}, Failed: {fail_count}")

if __name__ == "__main__":
    main()
