# Discourse API Examples

Contains example JSON and RSS responses from the Discourse API.

**Base URL:** https://discuss.criticalfallibilism.com/

## File Listing

- `posts.rss` - posts.rss (global latest 50 posts)
- `posts.json` - posts.json (global latest 50 posts)
- `t/2144/posts.json` - t_2144_posts.json (30 posts total, returns 1-21)
- `t/2144/posts.json?post_number=5` - t_2144_posts_pn_5.json
- `t/2144/posts.json?post_number=21` - t_2144_posts_pn_21.json
- `t/2144/posts.json?post_number=41` - t_2144_posts_pn_41.json
- `t/2144/posts.json?post_number=61` - t_2144_posts_pn_61.json
- `t/2144/posts.json?post_number=81` - t_2144_posts_pn_81.json
- `t/x/2144.rss` - t_2144.rss (latest 25 posts in thread, posts 31-6)
- `t/2108/posts.json` - t_2108_posts.json (446 posts total, returns 1-20)
- `t/2108/posts.json?post_number=5` - t_2108_posts_pn_5.json
- `t/2108/posts.json?post_number=21` - t_2108_posts_pn_21.json
- `t/2108/posts.json?post_number=41` - t_2108_posts_pn_41.json
- `t/2108/posts.json?post_number=61` - t_2108_posts_pn_61.json
- `t/2108/posts.json?post_number=81` - t_2108_posts_pn_81.json
- `t/x/2108.rss` - t_2108.rss (latest 25 posts in thread, posts 452-428)

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

⚠️ **CRITICAL: Different Endpoints for Latest vs First Posts**

**The Problem with `/t/{topic_id}/posts.json`:**
- Returns posts 1-20 (oldest posts) ❌
- Misses the latest posts in active threads
- Thread 2144 has 30 posts, returns 1-21, **missing posts 22-30**
- Thread 2108 has 446 posts, returns 1-20, **missing posts 21-446**

**✅ SOLUTION: Use `/t/{topic_id}/{post_number}.json` to view around a specific post:**
- `/t/2108/1.json` returns posts 1-20 (first page)
- `/t/2108/10.json` returns posts 5-24 (centered around post 10)
- `/t/2108/100.json` returns posts 95-114 (centered around post 100)
- `/t/2108/446.json` returns posts 433-452 (last ~20, when 446 is near the end)
- `/t/2108/500.json` returns posts 433-452 (clamped to last ~20 when beyond end)
- Returns JSON with full metadata: `raw` markdown, `updated_at`, all post fields
- **To get latest posts: use a high post number at/beyond the thread end**

**Alternative: Thread RSS for latest posts:**
- `/t/x/{topic_id}.rss` returns the **last ~25 posts** in descending order
- Thread 2144 RSS: posts 31-6 (latest 25, skipping deleted posts)
- Thread 2108 RSS: posts 452-428 (latest 25)
- The `x` in the path is arbitrary (any slug works)
- RSS lacks `raw` markdown and edit metadata

**Recommended Strategy for Complete Coverage:**

**For monitoring ALL posts across the forum:**
- Use `posts.json` (global latest 50 posts) ✅
- Detects new posts across all threads
- Provides `updated_at` for edit detection
- Provides `raw` markdown content

**For fetching latest posts in a specific thread:**
- **Best: `/t/{topic_id}/9999.json`** (use high number like 9999) ✅
  - Returns last ~20 posts with full JSON metadata
  - No need to know exact post count
  - Includes `raw` markdown, `updated_at`, structured data
- **Alternative: `/t/x/{topic_id}.rss`** (returns ~25 posts, no raw markdown)

**For viewing posts around a specific post:**
- Use `/t/{topic_id}/{post_number}.json`
- Returns ~20 posts centered around the specified post number
- Useful for getting context around a specific post

**For fetching older posts in a thread:**
- Use `/t/{topic_id}/posts.json?post_number=N` with pagination
- Start with high post numbers and paginate backwards

**Recommended polling strategy:**
1. **Global monitoring**: Poll `posts.json` every 60s
   - Track highest `id` seen for new posts
   - Track `updated_at` for edits
2. **Thread-specific monitoring**: Use `/t/{topic_id}/9999.json` for active threads
   - Get latest ~25 posts per thread
   - Detect new posts beyond what `posts.json` might show
3. **Complete thread fetching**: Use `?post_number` pagination when needed
   - Fetch entire thread history by walking backwards from the end

Note: topic 2108 has 446 posts in the thread (before 1769043607). Some earlier posts were moved to another thread so ids are not contiguous.
