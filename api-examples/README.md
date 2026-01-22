# Discourse API Examples

Contains example JSON and RSS responses from the Discourse API.

**Base URL:** https://discuss.criticalfallibilism.com/

## File Listing

- `posts.rss` - posts.rss
- `posts.json` - posts.json
- `t/2144/posts.json` - t_2144_posts.json (30 posts total)
- `t/2144/posts.json?post_number=5` - t_2144_posts_pn_5.json
- `t/2144/posts.json?post_number=21` - t_2144_posts_pn_21.json
- `t/2144/posts.json?post_number=41` - t_2144_posts_pn_41.json
- `t/2144/posts.json?post_number=61` - t_2144_posts_pn_61.json
- `t/2144/posts.json?post_number=81` - t_2144_posts_pn_81.json
- `t/2108/posts.json` - t_2108_posts.json (446 posts total)
- `t/2108/posts.json?post_number=5` - t_2108_posts_pn_5.json
- `t/2108/posts.json?post_number=21` - t_2108_posts_pn_21.json
- `t/2108/posts.json?post_number=41` - t_2108_posts_pn_41.json
- `t/2108/posts.json?post_number=61` - t_2108_posts_pn_61.json
- `t/2108/posts.json?post_number=81` - t_2108_posts_pn_81.json

## Understanding the `post_number` Parameter

The `post_number` query parameter in Discourse's `/t/{topic_id}/posts.json` endpoint controls pagination and determines which posts are returned:

### Pagination Behavior

- **Without `post_number`:** Returns posts 1-20 in ascending order (posts 1, 2, 3, ... 20)
- **With `post_number`:** Returns ~20 posts centered around and **ending at** the specified post number, in **descending order**

### Examples from Thread 2144 (30 posts total):

| Request | Posts Returned | Order | Count |
|---------|---------------|-------|-------|
| `posts.json` | 1-21 | Ascending (1→21) | 20 posts |
| `?post_number=5` | 4-1 | Descending (4→1) | 4 posts |
| `?post_number=21` | 20-1 | Descending (20→1) | 19 posts |
| `?post_number=41` | 30-11 | Descending (30→11) | 20 posts |
| `?post_number=61` | 30-11 | Descending (30→11) | 20 posts |
| `?post_number=81` | 30-11 | Descending (30→11) | 20 posts |

### Examples from Thread 2108 (446 posts total):

| Request | Posts Returned | Order | Count |
|---------|---------------|-------|-------|
| `posts.json` | 1-20 | Ascending (1→20) | 20 posts |
| `?post_number=21` | 20-1 | Descending (20→1) | 20 posts |
| `?post_number=41` | 40-21 | Descending (40→21) | 20 posts |
| `?post_number=61` | 59-37 | Descending (59→37) | 20 posts |
| `?post_number=81` | 80-60 | Descending (80→60) | 20 posts |

### Key Observations

1. **Direction Change:** The base endpoint returns posts in ascending order (oldest first), but using `post_number` returns posts in descending order (newest first relative to the specified post)
2. **Ending Point:** The `post_number` parameter specifies where to *end* the page, not where to start
3. **Chunk Size:** Discourse returns approximately 20 posts per request
4. **Bounds Handling:** When `post_number` is beyond the thread's end, it returns the last ~20 posts
5. **Small Offsets:** When `post_number` is small (e.g., 5), it only returns posts up to that number

### Thread Metadata

Each post in the response includes a `posts_count` field indicating the total number of posts in the thread. This is useful for determining pagination bounds.

## Downloading the Files

Use `fetch.py` to download all API examples:

```bash
python3 fetch.py
```

The script downloads all URLs and pretty-prints JSON files for readability.

## Latest Posts Endpoints: JSON vs RSS

When polling for new posts (e.g., every 60 seconds), both `posts.json` and `posts.rss` provide the latest posts, but with important differences:

### Structural Comparison

| Aspect | posts.json | posts.rss |
|--------|-----------|-----------|
| **Format** | JSON | XML (RSS 2.0) |
| **Count** | 50 posts | 50 posts |
| **Ordering** | Newest first (descending by created_at) | Newest first (descending by pubDate) |
| **ID Range** | IDs 20169-20218 | IDs 20169-20218 |
| **Root Element** | `latest_posts` array | `<channel>` with `<item>` elements |

### Content Available

#### posts.json provides:
- **Post ID**: `id` (numeric, e.g., 20218)
- **Timestamps**: `created_at`, `updated_at` (ISO 8601 format)
- **Content**:
  - `raw` - Original markdown (full, not truncated)
  - `cooked` - Rendered HTML
  - `excerpt` - Truncated preview with ellipsis
- **Topic metadata**: `topic_id`, `topic_slug`, `topic_title`, `topic_html_title`, `category_id`
- **User info**: `username`, `user_id`, `name`, `avatar_template`, role flags
- **Post metadata**: `post_number`, `post_type`, `posts_count`, `version`
- **Engagement**: `reads`, `readers_count`, `score`, `reply_count`, `quote_count`, `incoming_link_count`
- **Permissions**: `can_edit`, `can_delete`, `can_recover`, etc.
- **URL**: `post_url` (relative path, e.g., `/t/topic-slug/2148/1`)

#### posts.rss provides:
- **Post ID**: Extractable from `<guid>` (format: `discuss.criticalfallibilism.com-post-20218`)
- **Timestamp**: `<pubDate>` (RFC 2822 format, e.g., "Wed, 21 Jan 2026 23:29:42 +0000")
- **Content**:
  - `<description>` - Rendered HTML (same as JSON's `cooked`)
  - No raw markdown
  - No excerpt field
- **Topic metadata**: `<title>` (topic title only)
- **User info**: `<dc:creator>` (username with @ prefix, e.g., `@Elliot`)
- **URL**: `<link>` (absolute URL, e.g., `https://discuss.criticalfallibilism.com/t/people-being-bad-at-logic/2148#post_1`)

### Key Differences for Polling

#### ✅ Advantages of posts.json:
1. **Rich metadata**: Topic IDs, post numbers, categories, user IDs
2. **Raw markdown**: Original unrendered content (`raw` field)
3. **Version tracking**: `version` field tracks edits
4. **Update timestamp**: `updated_at` to detect edited posts
5. **Structured data**: Easy parsing, no HTML/XML escaping issues
6. **Query capabilities**: More fields for filtering and processing
7. **Post context**: `reply_to_post_number`, `posts_count` for thread navigation

#### ✅ Advantages of posts.rss:
1. **Simpler parsing**: Standard RSS format with libraries
2. **Absolute URLs**: Direct links without needing base URL
3. **Standard format**: Works with RSS readers and aggregators
4. **Lightweight**: Fewer fields mean less bandwidth

#### ⚠️ Critical Differences:

**For detecting NEW posts:**
- Both work equally well - use `id` field (JSON) or extract from `<guid>` (RSS)
- Both ordered newest-first, both return last 50 posts
- Track highest seen ID to detect new posts

**For detecting UPDATED posts:**
- JSON has `updated_at` and `version` fields ✅
- RSS only has `pubDate` (creation time) ❌
- **Verdict: Must use JSON to track edits**

**For extracting content:**
- JSON provides both `raw` (markdown) and `cooked` (HTML) ✅
- RSS only provides rendered HTML in `<description>` ❌
- **Verdict: Use JSON if you need markdown source**

**For getting topic/post metadata:**
- JSON provides `topic_id`, `post_number`, `category_id` ✅
- RSS requires parsing URLs to extract IDs ❌
- **Verdict: JSON much easier for metadata**

### Recommendation for 60-Second Polling

**Use `posts.json`** because it provides:
- Edit detection via `updated_at` and `version`
- Raw markdown content
- Structured metadata (no URL parsing needed)
- Better error handling (JSON vs XML parsing)

**Polling strategy:**
1. Track the highest `id` seen
2. On each poll, check for posts with `id > last_seen_id`
3. For existing posts, check `updated_at > last_seen_timestamp` to detect edits
4. Store both `id` and `updated_at` for each post

Note: topic 2108 has 446 posts in the thread (before 1769043607). Some earlier posts were moved to another thread so ids are not contiguous.
