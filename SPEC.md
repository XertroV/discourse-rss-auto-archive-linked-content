# Discourse Link Archiver - Technical Specification

**Version**: 1.0.0
**Status**: Approved

## 1. Overview

A Rust service that monitors a Discourse forum's RSS feed, detects links to ephemeral user-generated content (Reddit, TikTok, Twitter/X, YouTube, Instagram, etc.), archives that content to S3-compatible storage, and provides a public web interface for browsing and searching archived material.

The goal is to preserve referenced content for ongoing discussion even if the original is deleted.

### Target Forum

- Feed URL: `https://discuss.criticalfallibilism.com/posts.rss`
- Poll interval: 60 seconds (configurable)

---

## 2. Goals

- Poll Discourse `posts.rss` at regular intervals; ingest new posts and detect edits
- Extract links with special handling for quoted content (avoid redundant archiving)
- Normalize/canonicalize URLs (especially Reddit → `old.reddit.com`)
- Archive content via yt-dlp, gallery-dl, and HTTP fetch
- Store all archive artifacts in S3 (media, screenshots, metadata JSON)
- Maintain a publishable SQLite database with FTS5 search
- Serve a public, read-only Web UI for browsing and searching archives
- Support both Docker and native Linux deployment

## 3. Non-Goals (Initial Release)

- Moderation features
- User accounts or authentication on the web UI

## 3.1 Authentication & Cookie Support

The archiver **will** attempt to access authenticated content for archival purposes:
- Support cookies.txt files for authenticated sessions (TikTok, Instagram, etc.)
- Use browser cookie extraction where feasible
- Best-effort paywall bypass for archival (not guaranteed to work)
- Cookie paths configurable via `COOKIES_FILE_PATH` environment variable

---

## 4. Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                     discourse-link-archiver                          │
├─────────────┬─────────────┬─────────────┬─────────────┬────────────┤
│  RSS Poller │ Link Parser │  Archiver   │  S3 Client  │  Web UI    │
│  (60s tick) │ (per-site)  │  Workers    │             │  (axum)    │
└──────┬──────┴──────┬──────┴──────┬──────┴──────┬──────┴─────┬──────┘
       │             │             │             │            │
       ▼             ▼             ▼             ▼            ▼
   [Discourse]   [SQLite DB]   [yt-dlp]      [S3 Bucket]  [Browser]
                              [gallery-dl]
                              [HTTP fetch]
```

### 4.1 Component Responsibilities

| Component | Responsibility |
|-----------|----------------|
| **RSS Poller** | Fetches `posts.rss`, extracts new/changed posts, deduplicates by GUID |
| **Link Parser** | Extracts URLs from HTML, classifies by site, detects quote context |
| **Site Handlers** | Per-site URL normalization and archive strategies |
| **Archiver Workers** | Async worker pool dispatching to appropriate tools per URL type |
| **S3 Client** | Uploads media, manages URLs for serving |
| **Web UI** | Browse, search, view archived content |
| **DB Backup Job** | Periodic SQLite backup to S3 |

---

## 5. Technology Stack

| Component | Technology |
|-----------|------------|
| Language | Rust (stable, 2021 edition) |
| Async Runtime | tokio |
| Web Framework | axum + tower-http |
| Database | SQLite via sqlx (with FTS5) |
| S3 Client | aws-sdk-s3 |
| HTTP Client | reqwest |
| RSS Parsing | feed-rs |
| HTML Parsing | scraper |
| Templates | Askama |
| Media Download | yt-dlp, gallery-dl (subprocess) |
| Logging | tracing + tracing-subscriber |

### External Tools (Subprocess)

- `yt-dlp` - Video/media downloading
- `gallery-dl` - Image/gallery downloading
- `ffmpeg` - Media processing (required by yt-dlp)
- `zstd` - Database backup compression

---

## 6. Data Model

### 6.1 `posts` - Discourse Posts

| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | Internal ID |
| guid | TEXT UNIQUE | RSS `<guid>` |
| discourse_url | TEXT | Link back to post |
| author | TEXT | Post author |
| title | TEXT | Post title |
| body_html | TEXT | Raw HTML content |
| content_hash | TEXT | Hash for change detection |
| published_at | TEXT | ISO8601 timestamp |
| processed_at | TEXT | When we processed it |

### 6.2 `links` - URLs Found in Posts

| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | Internal ID |
| original_url | TEXT | URL as found in post |
| normalized_url | TEXT | Tracking params stripped |
| canonical_url | TEXT | Site-specific canonical (e.g., old.reddit.com) |
| final_url | TEXT | After redirect resolution |
| domain | TEXT | Extracted domain |
| first_seen_at | TEXT | First occurrence |
| last_archived_at | TEXT | For cache window checking |

### 6.3 `link_occurrences` - Link-Post Relationships

| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | Internal ID |
| link_id | INTEGER FK | Reference to links |
| post_id | INTEGER FK | Reference to posts |
| in_quote | INTEGER | 1 if inside blockquote |
| context_snippet | TEXT | Short excerpt around link |
| seen_at | TEXT | When this occurrence was seen |

### 6.4 `archives` - Archived Content

| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | Internal ID |
| link_id | INTEGER FK | Reference to links |
| status | TEXT | pending/processing/complete/failed/skipped |
| archived_at | TEXT | Completion timestamp |
| content_title | TEXT | Extracted title |
| content_author | TEXT | Original author |
| content_text | TEXT | Searchable extracted text |
| content_type | TEXT | video/image/text/gallery/thread |
| s3_key_primary | TEXT | Main file S3 key |
| s3_key_thumb | TEXT | Thumbnail S3 key |
| s3_keys_extra | TEXT | JSON array of additional files |
| wayback_url | TEXT | Wayback Machine snapshot URL |
| error_message | TEXT | Last error if failed |
| retry_count | INTEGER | Number of retry attempts |

### 6.5 `archive_artifacts` - Individual Files

| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | Internal ID |
| archive_id | INTEGER FK | Reference to archives |
| kind | TEXT | raw_html/screenshot/pdf/video/thumb/metadata |
| s3_key | TEXT | S3 object key |
| content_type | TEXT | MIME type |
| size_bytes | INTEGER | File size |
| sha256 | TEXT | Content hash |
| created_at | TEXT | Upload timestamp |

### 6.6 Full-Text Search (FTS5)

```sql
CREATE VIRTUAL TABLE archives_fts USING fts5(
    content_title,
    content_author,
    content_text,
    content='archives',
    content_rowid='id'
);
```

With appropriate triggers for INSERT/UPDATE/DELETE synchronization.

---

## 7. Link Detection & Quote Handling

### 7.1 Extraction Pipeline

1. Parse HTML with scraper
2. Walk DOM tree, tracking quote depth
3. Extract `<a href>` tags, marking `in_quote` status
4. Match URLs against handler patterns
5. Normalize and deduplicate

### 7.2 Quote Detection

Links inside these elements are marked `in_quote = true`:
- `<aside class="quote">` (Discourse quote wrapper)
- `<blockquote>`
- `<div class="quote">`

### 7.3 Quote Link Behavior

- Links with `in_quote = true` are recorded but NOT archived unless:
  - No prior archive exists for that normalized URL
  - This provides deduplication while catching first occurrences

---

## 8. Site Handlers

### 8.1 Handler Trait

```rust
#[async_trait]
pub trait SiteHandler: Send + Sync {
    fn site_id(&self) -> &'static str;
    fn url_patterns(&self) -> &[Regex];
    fn normalize_url(&self, url: &str) -> String;
    async fn archive(&self, url: &str, work_dir: &Path) -> Result<ArchiveResult>;
}
```

### 8.2 Supported Sites (Initial)

| Site | Handler ID | Method | Notes |
|------|------------|--------|-------|
| Reddit | `reddit` | yt-dlp + JSON API | Normalize to old.reddit.com |
| TikTok | `tiktok` | yt-dlp | Video + metadata |
| Twitter/X | `twitter` | yt-dlp + gallery-dl | Tweets, threads, media |
| YouTube | `youtube` | yt-dlp | Video + subtitles |
| Instagram | `instagram` | gallery-dl | Posts, reels, stories |
| Imgur | `imgur` | gallery-dl | Images, albums |
| Streamable | `streamable` | yt-dlp | Video (simple domain registration) |
| Bluesky | `bluesky` | HTTP API + media fetch | Posts via AT Protocol public API |
| Facebook | `facebook` | yt-dlp | Best effort |

### 8.4 Bluesky Handler Details

Bluesky uses the AT Protocol with a simple public HTTP API:

**URL Patterns:**
- `bsky.app/profile/{handle}/post/{postid}`
- `bsky.social/profile/{handle}/post/{postid}`

**Archive Process:**
1. Parse handle and post ID from URL
2. Resolve handle to DID via `https://bsky.social/xrpc/com.atproto.identity.resolveHandle`
3. Fetch post via `https://bsky.social/xrpc/app.bsky.feed.getPostThread?uri=at://{did}/app.bsky.feed.post/{postid}`
4. Download embedded images/media from CDN URLs
5. Store post JSON, text content, and media files

**Data Captured:**
- Post text content
- Author handle and display name
- Created timestamp
- Embedded images (full size from CDN)
- Embed cards (link previews)
- Reply/quote context if present

### 8.5 Streamable Handler Details

Streamable is a simple video hosting platform fully supported by yt-dlp.

**URL Patterns:**
- `streamable.com/{video_id}`

**Archive Process:**
1. Pass URL directly to yt-dlp
2. yt-dlp handles video download, metadata extraction
3. Store video file, thumbnail, and metadata JSON

### 8.3 URL Normalization Rules

**General:**
- Force HTTPS where appropriate
- Lowercase hostname
- Strip tracking parameters: `utm_*`, `fbclid`, `gclid`, `ref`, etc.
- Remove default ports
- Normalize trailing slashes

**Reddit-specific:**
- Convert `reddit.com`, `www.reddit.com`, `m.reddit.com` → `old.reddit.com`
- Resolve `redd.it` shortlinks
- Preserve comment permalinks

**TikTok-specific:**
- Resolve `vm.tiktok.com` redirects to canonical URLs

---

## 9. Archive Pipeline

### 9.1 Status Flow

```
pending → processing → complete
                    ↘ failed → (retry if count < 3) → pending
                             → (retry count >= 3) → skipped
```

### 9.2 Worker Pool

- Configurable concurrency (default: 4 workers)
- Per-domain rate limiting
- Semaphore-based permit system

### 9.3 Capture Strategies

**All Sites:**
1. HTTP fetch (raw HTML + headers)
2. Metadata extraction (title, OpenGraph, publish date)
3. Text extraction (readable content)
4. Screenshot (PNG via headless browser, optional)
5. PDF print (optional)

**Media Sites (YouTube, TikTok, etc.):**
1. yt-dlp video download
2. Thumbnail capture
3. Subtitle extraction (if available)
4. Metadata JSON

### 9.4 Retry Policy

- Exponential backoff: 5m, 30m, 2h, 12h
- Max 3 retries before marking as skipped
- Per-attempt error logging

### 9.5 Wayback Machine Integration

After successful archive:
- Submit original URL to `web.archive.org/save/`
- Rate limit: max 5 requests/minute
- Store snapshot URL when available

### 9.6 Archive.today Integration

Alternative/complementary to Wayback Machine:
- Submit URL to `archive.today/submit/`
- Often faster and more reliable than Wayback
- Better at capturing dynamic JavaScript content
- Rate limit: max 3 requests/minute (more restrictive)
- Store archive.today URL when available
- Configurable: enabled/disabled via `ARCHIVE_TODAY_ENABLED`

### 9.7 Screenshot Capture

Browser-based page rendering for visual archive:
- Use `chromiumoxide` or `headless_chrome` crate
- Capture full-page screenshots as PNG
- Configurable viewport size (default: 1280x3000)
- Wait for page load completion
- Store in S3 under `archives/{link_id}/render/screenshot.png`
- Optional: Capture at multiple viewport sizes (mobile, tablet, desktop)

Configuration:
```bash
SCREENSHOT_ENABLED=true
SCREENSHOT_VIEWPORT_WIDTH=1280
SCREENSHOT_VIEWPORT_HEIGHT=3000
CHROMIUM_PATH=/usr/bin/chromium
```

### 9.8 PDF Generation

Print-quality document export:
- Use same headless browser infrastructure as screenshots
- Generate PDF via Chrome DevTools Protocol print-to-PDF
- Optimized for articles and text content
- Store in S3 under `archives/{link_id}/render/page.pdf`
- Configurable paper size (default: A4)

Configuration:
```bash
PDF_ENABLED=true
PDF_PAPER_WIDTH=8.27  # A4 width in inches
PDF_PAPER_HEIGHT=11.69  # A4 height in inches
```

### 9.9 Content Deduplication

Perceptual hashing to detect duplicate media:
- Use `image_hasher` crate for perceptual image hashing (pHash/dHash)
- Store hash in `archive_artifacts` table
- Before downloading, check if similar content already archived
- Configurable similarity threshold (default: 90%)
- Skip re-archiving if near-duplicate exists (link to existing)

Database additions:
```sql
ALTER TABLE archive_artifacts ADD COLUMN perceptual_hash TEXT;
CREATE INDEX idx_artifacts_phash ON archive_artifacts(perceptual_hash);
```

---

## 10. S3 Storage Layout

```
{bucket}/
├── archives/
│   └── {link_id}/
│       ├── meta.json
│       ├── fetch/
│       │   ├── raw.html
│       │   └── headers.json
│       ├── render/
│       │   ├── screenshot.png
│       │   └── page.pdf
│       ├── text/
│       │   └── extracted.txt
│       └── media/
│           ├── video.mp4
│           ├── thumb.jpg
│           └── info.json
├── thumbnails/
│   └── {archive_id}.jpg
└── backups/
    └── db/
        └── archive_{timestamp}.sqlite.zst
```

---

## 11. Web UI

### 11.1 Routes

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Home/recent archives |
| GET | `/search` | Search form + results |
| GET | `/archive/{id}` | Single archive detail |
| GET | `/post/{guid}` | All archives from a post |
| GET | `/site/{site}` | Browse by source site |
| GET | `/stats` | Processing statistics |
| GET | `/healthz` | Health check endpoint |
| GET | `/api/archives` | JSON API (paginated) |
| GET | `/api/search` | JSON search endpoint |
| GET | `/feed.rss` | RSS feed of new archives |
| GET | `/feed.atom` | Atom feed of new archives |
| GET | `/compare/{id1}/{id2}` | Compare two archive versions |
| GET | `/export/{site}` | Bulk export archives for a domain |

### 11.2 Search Parameters

- `q` - Full-text search query
- `site` - Filter by source site
- `from` / `to` - Date range
- `has_media` - Only entries with video/images
- `status` - Filter by archive status
- `page` / `per_page` - Pagination

### 11.3 UI Technology

- Server-rendered HTML with Askama templates
- Minimal CSS (PicoCSS or similar classless framework)
- Optional htmx for dynamic search
- No heavy JavaScript frameworks

### 11.4 Dark Mode

- System preference detection via `prefers-color-scheme` media query
- Manual toggle switch in header
- Preference stored in localStorage
- PicoCSS dark theme classes
- Cookie fallback for SSR

### 11.5 RSS/Atom Feed

Feed of newly archived content for subscription:
- RSS 2.0 format at `/feed.rss`
- Atom 1.0 format at `/feed.atom`
- Include: title, original URL, archive URL, timestamp, content type
- Last 50 archives by default
- Optional filters via query params: `?site=reddit.com&type=video`

### 11.6 Archive Comparison

Show differences when content changes between archive versions:
- Side-by-side text diff view
- Highlight added/removed content
- Timestamp comparison
- Use `similar` crate for diff algorithm
- Link to both full archives

### 11.7 Bulk Export

Download multiple archives as ZIP:
- Export all archives for a specific domain
- Exclude large video files (>50MB by default)
- Include metadata.json with archive info
- Configurable max export size (default: 500MB)
- Rate limit: 1 export per hour per IP

---

## 12. Configuration

### Environment Variables

```bash
# Discourse
RSS_URL=https://discuss.criticalfallibilism.com/posts.rss
POLL_INTERVAL_SECS=60
CACHE_WINDOW_SECS=3600

# Database
DATABASE_PATH=./data/archive.sqlite

# S3
S3_BUCKET=discourse-archives
S3_REGION=us-east-1
S3_ENDPOINT=          # Optional, for MinIO/R2
S3_PREFIX=archives/
S3_PUBLIC_URL_BASE=   # Optional, for R2/custom domains (e.g., https://pub-xxxxx.r2.dev)
AWS_ACCESS_KEY_ID=xxx
AWS_SECRET_ACCESS_KEY=xxx

# Archiver
WORKER_CONCURRENCY=4
PER_DOMAIN_CONCURRENCY=1
WORK_DIR=./data/tmp
YT_DLP_PATH=yt-dlp
GALLERY_DL_PATH=gallery-dl
COOKIES_FILE_PATH=./cookies.txt  # Optional, for authenticated archiving

# Policy
ARCHIVE_MODE=deletable  # or 'all'
ARCHIVE_QUOTE_ONLY_LINKS=false

# Web
WEB_HOST=0.0.0.0
WEB_PORT=8080

# HTTPS / Let's Encrypt (optional)
TLS_ENABLED=false
TLS_DOMAINS=example.com,www.example.com  # Comma-separated domains
TLS_CONTACT_EMAIL=admin@example.com      # Optional, for cert expiry notifications
TLS_CACHE_DIR=./data/acme_cache          # Certificate cache directory
TLS_USE_STAGING=false                     # Use Let's Encrypt staging for testing
TLS_HTTPS_PORT=443                        # HTTPS port

# Wayback
WAYBACK_ENABLED=true
WAYBACK_RATE_LIMIT_PER_MIN=5
```

### Config File (Alternative)

Support `config.toml` as an alternative to environment variables.

---

## 13. Deployment

### 13.1 Docker (Recommended)

Single container including:
- Rust binary
- yt-dlp
- gallery-dl
- ffmpeg
- Headless Chromium (optional)

Provide `docker-compose.yml` with:
- Volume for `/data`
- Environment variable configuration
- Optional MinIO for local S3 testing

### 13.2 Native Linux

Setup script installs:
- yt-dlp
- gallery-dl
- ffmpeg
- zstd

Systemd service file for daemon management.

### 13.3 Database Backup

- Daily automated backup to S3
- Compression with zstd
- Retention: 30 days
- Optional: litestream for continuous replication

---

## 14. Acceptance Criteria

### AC1: RSS Polling
- [ ] Service polls RSS feed every 60 seconds (configurable)
- [ ] New posts are detected and stored in database
- [ ] Changed posts (different content hash) trigger re-processing
- [ ] Duplicate GUIDs are handled gracefully

### AC2: Link Extraction
- [ ] All `<a href>` tags are extracted from post HTML
- [ ] Links inside quote elements are marked as `in_quote`
- [ ] Quote-only links only archived if never archived before
- [ ] URLs are properly normalized (tracking params removed)

### AC3: Site Handlers
- [ ] Reddit URLs normalized to old.reddit.com
- [ ] TikTok vm.tiktok.com shortlinks resolved
- [ ] At least 5 site handlers implemented and working
- [ ] Unknown domains can be configured for archive-all mode

### AC4: Archive Pipeline
- [ ] Worker pool processes pending archives concurrently
- [ ] yt-dlp successfully downloads videos from supported sites
- [ ] Failed archives retry with exponential backoff
- [ ] Permanently failed archives marked as skipped after 3 retries

### AC5: S3 Storage
- [ ] Media files uploaded to configured S3 bucket
- [ ] Consistent key structure maintained
- [ ] Database backups uploaded daily
- [ ] Works with AWS S3 and S3-compatible services (MinIO, R2)

### AC6: Web UI
- [ ] Home page displays recent archives
- [ ] Search returns relevant results using FTS
- [ ] Archive detail page shows all captured artifacts
- [ ] Media (video/images) can be viewed directly
- [ ] Responsive design works on mobile

### AC7: Database
- [ ] SQLite database is created with all required tables
- [ ] FTS5 search index maintained automatically
- [ ] Database contains no private/sensitive information
- [ ] Database file can be safely published

### AC8: Deployment
- [ ] Docker image builds and runs successfully
- [ ] Native Linux setup script works on Ubuntu 22.04+
- [ ] Systemd service starts and restarts automatically
- [ ] Health endpoint returns service status

### AC9: Performance
- [ ] Handles 100+ links per day without issues
- [ ] Web UI responds within 200ms for typical queries
- [ ] Memory usage stays under 512MB during normal operation
- [ ] Worker pool doesn't overwhelm external services

### AC10: Reliability
- [ ] Service recovers from network failures
- [ ] Database remains consistent after crashes
- [ ] Partial downloads are cleaned up properly
- [ ] Logging provides sufficient debugging information

---

## 15. Future Enhancements (Post-MVP)

**Implemented:**
- [x] RSS/Atom feed of newly archived content (Phase 12)
- [x] Content deduplication via perceptual hashing (Phase 12)
- [x] Archive.today integration (Phase 12)
- [x] Manual submission web form (Phase 11)
- [x] IPFS pinning for redundancy (Phase 10)
- [x] Screenshot capture (Phase 12)
- [x] PDF generation (Phase 12)
- [x] Dark mode UI (Phase 12)
- [x] Archive comparison/diff (Phase 12)
- [x] Bulk export (Phase 12)
- [x] Bluesky handler (Phase 12)
- [x] Streamable handler (Phase 12)

**Still Pending:**
1. Webhook notifications (Discord/Slack)
2. Prometheus metrics endpoint
3. Browser extension for one-click archiving
4. Email notifications for failed archives
5. Admin dashboard for monitoring
6. Custom archive request priorities

---

## 16. Security & Privacy

- Public UI with no authentication required
- Database is publishable (only public data stored)
- S3 credentials and cookies via environment variables/files only (never in DB/logs)
- Network fetch timeouts enforced
- Archived HTML not executed (display via screenshot/text)
- Rate limiting on external API calls
- Cookie files stored securely with restricted permissions
