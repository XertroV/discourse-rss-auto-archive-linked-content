# Discourse Link Archiver — Technical Specification

**Version**: 0.1.0  
**Status**: Draft

## Overview

A Rust service that monitors a Discourse forum's RSS feed, detects links to ephemeral user-generated content (Reddit, TikTok, Twitter/X, YouTube, etc.), archives that content to S3, and provides a public web interface for browsing and searching archived material. The goal is to preserve referenced content for ongoing discussion even if the original is deleted.

---

## Core Requirements

| Requirement | Solution |
|-------------|----------|
| Language | Rust (stable) |
| Database | SQLite (single file, easy backup) |
| Media storage | S3-compatible bucket |
| Polling interval | 60 seconds (configurable) |
| WebUI | Server-rendered HTML (no JS framework) |
| Authentication | None (fully public, read-only) |
| Deployment | Single Linux binary + setup script |

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         discourse-archiver                          │
├─────────────┬─────────────┬─────────────┬─────────────┬────────────┤
│  RSS Poller │ Link Parser │  Archiver   │  S3 Client  │  Web UI    │
│  (60s tick) │ (per-site)  │  Workers    │             │  (axum)    │
└──────┬──────┴──────┬──────┴──────┬──────┴──────┬──────┴─────┬──────┘
       │             │             │             │            │
       ▼             ▼             ▼             ▼            ▼
   [Discourse]   [SQLite DB]   [yt-dlp]      [S3 Bucket]  [Browser]
                              [gallery-dl]
                              [wayback API]
```

### Component Responsibilities

| Component | Responsibility |
|-----------|----------------|
| **RSS Poller** | Fetches `posts.rss`, extracts new posts, deduplicates by GUID |
| **Link Parser** | Extracts URLs, classifies by site, detects quoted context |
| **Archiver Workers** | Async task pool; dispatches to appropriate archiver per URL type |
| **S3 Client** | Uploads media, manages presigned URLs for serving |
| **Web UI** | Browse, search, view archived content |
| **DB Backup Job** | Periodic SQLite dump to S3 |

---

## Data Model

### `posts` — Discourse posts seen

```sql
CREATE TABLE posts (
    id              INTEGER PRIMARY KEY,
    guid            TEXT NOT NULL UNIQUE,  -- RSS <guid>
    discourse_url   TEXT NOT NULL,         -- link back to post
    author          TEXT,
    title           TEXT,
    body_html       TEXT,                  -- raw HTML for re-parsing if needed
    published_at    TEXT NOT NULL,         -- ISO8601
    processed_at    TEXT NOT NULL
);
CREATE INDEX idx_posts_published ON posts(published_at);
```

### `links` — URLs found in posts

```sql
CREATE TABLE links (
    id              INTEGER PRIMARY KEY,
    post_id         INTEGER NOT NULL REFERENCES posts(id),
    url_original    TEXT NOT NULL,         -- as found in post
    url_normalized  TEXT NOT NULL,         -- canonical form
    site            TEXT NOT NULL,         -- 'reddit', 'tiktok', 'twitter', etc.
    in_quote        INTEGER NOT NULL DEFAULT 0,  -- 1 if inside [quote]
    UNIQUE(post_id, url_normalized)
);
CREATE INDEX idx_links_url ON links(url_normalized);
CREATE INDEX idx_links_site ON links(site);
```

### `archives` — Archived content

```sql
CREATE TABLE archives (
    id              INTEGER PRIMARY KEY,
    link_id         INTEGER NOT NULL REFERENCES links(id),
    status          TEXT NOT NULL DEFAULT 'pending',  
                    -- 'pending' | 'processing' | 'complete' | 'failed' | 'skipped'
    archived_at     TEXT,
    
    -- Metadata extracted from content
    content_title   TEXT,
    content_author  TEXT,
    content_text    TEXT,                  -- searchable text (post body, comments, etc.)
    content_type    TEXT,                  -- 'video', 'image', 'text', 'gallery', 'thread'
    
    -- S3 references
    s3_key_primary  TEXT,                  -- main file (video, image, or JSON)
    s3_key_thumb    TEXT,                  -- thumbnail if applicable
    s3_keys_extra   TEXT,                  -- JSON array of additional files (gallery images, etc.)
    
    -- Wayback Machine
    wayback_url     TEXT,                  -- snapshot URL if successful
    wayback_status  TEXT,                  -- 'pending' | 'submitted' | 'available' | 'failed'
    
    -- Error tracking
    error_message   TEXT,
    retry_count     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_archives_status ON archives(status);
CREATE INDEX idx_archives_content ON archives(content_text);  -- FTS later
```

### `archive_log` — Processing audit trail

```sql
CREATE TABLE archive_log (
    id          INTEGER PRIMARY KEY,
    archive_id  INTEGER NOT NULL REFERENCES archives(id),
    timestamp   TEXT NOT NULL,
    event       TEXT NOT NULL,  -- 'started', 'downloaded', 'uploaded', 'wayback_submitted', etc.
    details     TEXT            -- JSON blob
);
```

### Full-Text Search

```sql
CREATE VIRTUAL TABLE archives_fts USING fts5(
    content_title,
    content_author, 
    content_text,
    content='archives',
    content_rowid='id'
);

-- Triggers to keep FTS in sync
CREATE TRIGGER archives_ai AFTER INSERT ON archives BEGIN
    INSERT INTO archives_fts(rowid, content_title, content_author, content_text)
    VALUES (new.id, new.content_title, new.content_author, new.content_text);
END;

CREATE TRIGGER archives_ad AFTER DELETE ON archives BEGIN
    INSERT INTO archives_fts(archives_fts, rowid, content_title, content_author, content_text)
    VALUES ('delete', old.id, old.content_title, old.content_author, old.content_text);
END;

CREATE TRIGGER archives_au AFTER UPDATE ON archives BEGIN
    INSERT INTO archives_fts(archives_fts, rowid, content_title, content_author, content_text)
    VALUES ('delete', old.id, old.content_title, old.content_author, old.content_text);
    INSERT INTO archives_fts(rowid, content_title, content_author, content_text)
    VALUES (new.id, new.content_title, new.content_author, new.content_text);
END;
```

---

## Site Handlers

Each supported site implements a common trait:

```rust
#[async_trait]
pub trait SiteHandler: Send + Sync {
    /// Unique identifier for this handler
    fn site_id(&self) -> &'static str;
    
    /// Regex patterns that match URLs this handler processes
    fn url_patterns(&self) -> &[Regex];
    
    /// Normalize a matched URL to canonical form
    fn normalize_url(&self, url: &str) -> String;
    
    /// Archive the content, returning metadata and file paths
    async fn archive(&self, url: &str, work_dir: &Path) -> Result<ArchiveResult>;
}

pub struct ArchiveResult {
    pub content_title: Option<String>,
    pub content_author: Option<String>,
    pub content_text: Option<String>,
    pub content_type: ContentType,
    pub primary_file: Option<PathBuf>,
    pub thumbnail: Option<PathBuf>,
    pub extra_files: Vec<PathBuf>,
}
```

### Supported Sites (Initial)

| Site | Handler | Archive Method | Notes |
|------|---------|----------------|-------|
| **Reddit** | `reddit` | yt-dlp + JSON API | Normalize to `old.reddit.com`; archive post JSON + media |
| **TikTok** | `tiktok` | yt-dlp | Video + metadata |
| **Twitter/X** | `twitter` | yt-dlp + gallery-dl | Tweets, threads, media |
| **YouTube** | `youtube` | yt-dlp | Video + subtitles if available |
| **Instagram** | `instagram` | gallery-dl | Posts, reels, stories |
| **Imgur** | `imgur` | gallery-dl | Images, albums, gifv→mp4 |
| **Streamable** | `streamable` | yt-dlp | Video |
| **v.redd.it** | `vreddit` | yt-dlp | Reddit-hosted video (handled via reddit handler) |

### Adding New Handlers

1. Implement `SiteHandler` trait
2. Register in `HandlerRegistry`
3. No other changes needed—URL matching and dispatch is automatic

---

## Link Detection & Quote Handling

### Extraction Pipeline

```
Post HTML
    │
    ▼
┌───────────────────┐
│ Parse with scraper│
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ Walk DOM tree     │
│ Track quote depth │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ Extract <a href>  │
│ Tag: in_quote=T/F │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ Match against     │
│ handler patterns  │
└─────────┬─────────┘
          │
          ▼
┌───────────────────┐
│ Normalize URLs    │
│ Dedupe by normal. │
└───────────────────┘
```

### Quote Detection

Discourse wraps quotes in:
```html
<aside class="quote" data-post="..." data-topic="...">
  <blockquote>
    ...content with links...
  </blockquote>
</aside>
```

Logic:
- Links inside `<aside class="quote">` → `in_quote = true`
- Links outside → `in_quote = false`
- **Behavior**: Links with `in_quote = true` are recorded but *not* archived unless:
  - No prior archive exists for that normalized URL
  - This provides deduplication while still catching first occurrences

---

## Archiving Pipeline

### State Machine

```
           ┌──────────────────────────────────────┐
           │                                      │
           ▼                                      │
┌─────────────────┐                               │
│     pending     │ ◄─── New link inserted        │
└────────┬────────┘                               │
         │                                        │
         │ Worker picks up                        │
         ▼                                        │
┌─────────────────┐                               │
│   processing    │                               │
└────────┬────────┘                               │
         │                                        │
    ┌────┴────┐                                   │
    │         │                                   │
    ▼         ▼                                   │
┌────────┐ ┌────────┐                             │
│complete│ │ failed │───retry_count < 3?──────────┘
└────────┘ └────────┘
               │
               │ retry_count >= 3
               ▼
         ┌──────────┐
         │ skipped  │
         └──────────┘
```

### Worker Pool

```rust
// Configurable concurrency (default: 4 workers)
let semaphore = Arc::new(Semaphore::new(config.archive_workers));

loop {
    let pending = db.get_pending_archives(batch_size).await?;
    
    for archive in pending {
        let permit = semaphore.clone().acquire_owned().await?;
        let handler = registry.handler_for(&archive.site);
        
        tokio::spawn(async move {
            let _permit = permit;  // held until task completes
            process_archive(archive, handler).await
        });
    }
    
    tokio::time::sleep(Duration::from_secs(5)).await;
}
```

### Wayback Machine Integration

After successful S3 upload:

```rust
async fn submit_to_wayback(url: &str) -> Result<String> {
    // POST to https://web.archive.org/save/{url}
    // Returns job_id for status polling
    
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("https://web.archive.org/save/{}", url))
        .header("Accept", "application/json")
        .send()
        .await?;
    
    // Poll for completion or store job_id for background check
}
```

Rate limiting: Max 5 requests/minute to Wayback API (be a good citizen).

---

## S3 Storage Layout

```
{bucket}/
├── media/
│   ├── reddit/
│   │   ├── {post_id}/
│   │   │   ├── video.mp4
│   │   │   ├── thumb.jpg
│   │   │   └── metadata.json
│   │   └── ...
│   ├── tiktok/
│   │   └── {video_id}.mp4
│   ├── twitter/
│   │   └── {tweet_id}/
│   │       ├── media_1.jpg
│   │       └── metadata.json
│   └── ...
├── thumbnails/
│   └── {archive_id}.jpg
└── backups/
    └── db/
        ├── archive_2024-01-15.sqlite.zst
        └── archive_2024-01-16.sqlite.zst
```

### Naming Convention

```rust
fn s3_key(site: &str, content_id: &str, filename: &str) -> String {
    format!("media/{site}/{content_id}/{filename}")
}
```

### Serving Media

Two options (configurable):

1. **Public bucket**: Direct S3 URLs stored and served
2. **Private bucket**: Generate presigned URLs on-the-fly (1 hour expiry)

Default: Public bucket with appropriate CORS headers.

---

## Web UI

### Routes

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | Home/recent archives |
| GET | `/search` | Search form + results |
| GET | `/archive/{id}` | Single archive detail view |
| GET | `/post/{guid}` | All archives from a Discourse post |
| GET | `/site/{site}` | Browse by site (reddit, tiktok, etc.) |
| GET | `/stats` | Processing statistics |
| GET | `/api/archives` | JSON API for archives (paginated) |
| GET | `/api/search` | JSON search endpoint |

### Search

Query params:
- `q` — Full-text search (FTS5)
- `site` — Filter by site
- `from` / `to` — Date range
- `has_media` — Only entries with video/images
- `page` / `per_page` — Pagination

Example:
```
/search?q=funny+cat&site=reddit&has_media=1&from=2024-01-01
```

### UI Components

Built with server-rendered templates (Askama):

```
templates/
├── base.html           # Layout with header, nav, footer
├── home.html           # Recent archives grid
├── search.html         # Search form + results
├── archive_detail.html # Single archive with media player
├── post_detail.html    # All links from one Discourse post
├── site_list.html      # Browse by site
└── partials/
    ├── archive_card.html
    ├── pagination.html
    └── media_embed.html
```

Styling: Minimal CSS (PicoCSS or similar classless framework), ~10KB total.

### Media Embedding

```html
<!-- Video -->
<video controls preload="metadata" poster="{{ thumb_url }}">
    <source src="{{ video_url }}" type="video/mp4">
</video>

<!-- Image -->
<img src="{{ image_url }}" alt="{{ title }}" loading="lazy">

<!-- Gallery -->
<div class="gallery">
    {% for img in images %}
    <a href="{{ img.full }}" target="_blank">
        <img src="{{ img.thumb }}" loading="lazy">
    </a>
    {% endfor %}
</div>

<!-- Text content (Reddit post, etc.) -->
<article class="archived-text">
    <h2>{{ title }}</h2>
    <p class="meta">by {{ author }} • {{ date }}</p>
    <div class="content">{{ content_text | safe }}</div>
</article>
```

---

## Configuration

### `config.toml`

```toml
[discourse]
feed_url = "https://forum.example.com/posts.rss"
poll_interval_secs = 60

[database]
path = "./data/archive.sqlite"
backup_interval_hours = 24

[s3]
bucket = "discourse-archives"
region = "us-east-1"
endpoint = ""  # Leave empty for AWS, set for MinIO/R2/etc.
public_url_base = "https://discourse-archives.s3.amazonaws.com"
# Credentials via AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY env vars

[archiver]
workers = 4
max_retries = 3
work_dir = "./data/tmp"

[archiver.ytdlp]
path = "yt-dlp"  # or absolute path
format = "best[height<=1080]"
extra_args = ["--no-playlist", "--write-thumbnail"]

[archiver.gallery_dl]
path = "gallery-dl"
extra_args = []

[wayback]
enabled = true
rate_limit_per_minute = 5

[web]
host = "0.0.0.0"
port = 8080

[handlers]
# Per-handler config
[handlers.reddit]
prefer_old_reddit = true
archive_comments = true
max_comment_depth = 3

[handlers.youtube]
archive_subtitles = true
```

---

## External Dependencies

### System Packages

```bash
# setup.sh installs these
yt-dlp          # Video downloading
gallery-dl      # Image/gallery downloading  
ffmpeg          # Media processing (used by yt-dlp)
zstd            # Database backup compression
```

### Rust Crates

```toml
[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }

# Web framework
axum = "0.7"
tower-http = { version = "0.5", features = ["fs", "cors"] }

# Database
sqlx = { version = "0.7", features = ["runtime-tokio", "sqlite"] }

# S3
aws-sdk-s3 = "1"
aws-config = "1"

# HTTP client
reqwest = { version = "0.12", features = ["json"] }

# HTML parsing
scraper = "0.19"

# RSS parsing
feed-rs = "1"

# Templates
askama = "0.12"
askama_axum = "0.4"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# Utilities
regex = "1"
url = "2"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
tracing-subscriber = "0.3"
thiserror = "1"
anyhow = "1"

# Process execution (yt-dlp, gallery-dl)
tokio-process = "1"
```

---

## Setup & Deployment

### `setup.sh`

```bash
#!/bin/bash
set -euo pipefail

echo "=== Discourse Link Archiver Setup ==="

# Check for required tools
command -v cargo >/dev/null || { echo "Error: cargo not found"; exit 1; }
command -v curl >/dev/null || { echo "Error: curl not found"; exit 1; }

# Install yt-dlp
echo "Installing yt-dlp..."
curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o /usr/local/bin/yt-dlp
chmod +x /usr/local/bin/yt-dlp

# Install gallery-dl
echo "Installing gallery-dl..."
pip install --user gallery-dl

# Install ffmpeg if not present
if ! command -v ffmpeg &> /dev/null; then
    echo "Installing ffmpeg..."
    apt-get update && apt-get install -y ffmpeg
fi

# Install zstd for backups
if ! command -v zstd &> /dev/null; then
    echo "Installing zstd..."
    apt-get install -y zstd
fi

# Create data directories
mkdir -p data/tmp

# Build the application
echo "Building discourse-archiver..."
cargo build --release

echo "=== Setup complete ==="
echo "1. Copy config.example.toml to config.toml and edit"
echo "2. Set AWS credentials: AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY"
echo "3. Run: ./target/release/discourse-archiver"
```

### Systemd Service

```ini
# /etc/systemd/system/discourse-archiver.service
[Unit]
Description=Discourse Link Archiver
After=network.target

[Service]
Type=simple
User=archiver
WorkingDirectory=/opt/discourse-archiver
ExecStart=/opt/discourse-archiver/target/release/discourse-archiver
Restart=always
RestartSec=10

Environment=RUST_LOG=info
Environment=AWS_ACCESS_KEY_ID=xxx
Environment=AWS_SECRET_ACCESS_KEY=xxx

[Install]
WantedBy=multi-user.target
```

---

## Database Backup Strategy

### Automated Backups

```rust
async fn backup_database(db_path: &Path, s3: &S3Client, bucket: &str) -> Result<()> {
    let timestamp = Utc::now().format("%Y-%m-%d_%H%M%S");
    let backup_name = format!("archive_{}.sqlite", timestamp);
    let compressed_name = format!("{}.zst", backup_name);
    
    // 1. Create consistent backup using SQLite backup API
    let backup_path = temp_dir().join(&backup_name);
    sqlx::sqlite::SqliteConnection::connect(db_path.to_str().unwrap())
        .await?
        .execute(&format!("VACUUM INTO '{}'", backup_path.display()))
        .await?;
    
    // 2. Compress with zstd
    let compressed_path = temp_dir().join(&compressed_name);
    Command::new("zstd")
        .args(["-19", "-f", backup_path.to_str().unwrap(), "-o", compressed_path.to_str().unwrap()])
        .status()
        .await?;
    
    // 3. Upload to S3
    let key = format!("backups/db/{}", compressed_name);
    s3.upload_file(&compressed_path, bucket, &key).await?;
    
    // 4. Clean up old backups (keep last 30)
    cleanup_old_backups(s3, bucket, 30).await?;
    
    Ok(())
}
```

Schedule: Daily at 03:00 UTC (configurable).

---

## Observability

### Logging

```rust
// Structured JSON logging for production
tracing_subscriber::fmt()
    .json()
    .with_env_filter(EnvFilter::from_default_env())
    .init();

// Key events logged:
// - RSS poll results (new posts found)
// - Archive job start/complete/fail
// - S3 upload success/failure  
// - Wayback submission status
// - HTTP requests (web UI)
```

### `/stats` Endpoint

```json
{
    "total_posts_processed": 15234,
    "total_links_found": 8921,
    "total_archives": 7845,
    "archives_by_status": {
        "complete": 7521,
        "failed": 234,
        "pending": 45,
        "processing": 12,
        "skipped": 33
    },
    "archives_by_site": {
        "reddit": 3245,
        "tiktok": 1876,
        "twitter": 1543,
        "youtube": 892,
        "other": 289
    },
    "storage_used_bytes": 158934567890,
    "last_poll_at": "2024-01-15T10:23:45Z",
    "uptime_seconds": 864000
}
```

---

## Future Enhancements

Potential additions not in initial scope:

1. **RSS/Atom feed of archives** — Subscribe to newly archived content
2. **Webhook notifications** — POST to Discord/Slack when interesting content archived
3. **Content deduplication** — Perceptual hashing to detect reposts
4. **Browser extension** — One-click archive from browser
5. **Archive.today integration** — Alternative to Wayback Machine
6. **Tor/proxy support** — For sites that block datacenter IPs
7. **Manual submission** — Web form to request archiving of specific URLs
8. **Moderation queue** — Flag/remove inappropriate archives
9. **IPFS pinning** — Additional redundancy layer
10. **Analytics dashboard** — Which links get archived most, failure rates, etc.

---

## Project Structure

```
discourse-archiver/
├── Cargo.toml
├── config.example.toml
├── setup.sh
├── README.md
├── src/
│   ├── main.rs
│   ├── config.rs
│   ├── db/
│   │   ├── mod.rs
│   │   ├── models.rs
│   │   ├── queries.rs
│   │   └── migrations/
│   │       └── 001_initial.sql
│   ├── rss/
│   │   ├── mod.rs
│   │   ├── poller.rs
│   │   └── parser.rs
│   ├── handlers/
│   │   ├── mod.rs
│   │   ├── registry.rs
│   │   ├── reddit.rs
│   │   ├── tiktok.rs
│   │   ├── twitter.rs
│   │   ├── youtube.rs
│   │   ├── instagram.rs
│   │   ├── imgur.rs
│   │   └── streamable.rs
│   ├── archiver/
│   │   ├── mod.rs
│   │   ├── worker.rs
│   │   ├── ytdlp.rs
│   │   ├── gallery_dl.rs
│   │   └── wayback.rs
│   ├── s3/
│   │   ├── mod.rs
│   │   └── client.rs
│   ├── web/
│   │   ├── mod.rs
│   │   ├── routes.rs
│   │   ├── api.rs
│   │   └── templates.rs
│   └── backup.rs
├── templates/
│   ├── base.html
│   ├── home.html
│   ├── search.html
│   ├── archive_detail.html
│   └── partials/
│       └── ...
└── static/
    ├── style.css
    └── favicon.ico
```

---

## Summary

This spec describes a single-binary Rust service that:

1. Polls Discourse RSS every 60 seconds
2. Extracts and classifies links to ephemeral content platforms
3. Skips links in quotes (unless first occurrence)
4. Archives via yt-dlp/gallery-dl to S3
5. Submits to Wayback Machine as backup
6. Stores searchable metadata in SQLite (with FTS5)
7. Backs up database to S3 daily
8. Serves a minimal, public web UI for browsing/searching

The handler architecture makes adding new sites straightforward. The entire system is stateless enough that the database can be published without privacy concerns.
