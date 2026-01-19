# Main Tasks

Development task tracker for discourse-link-archiver. Keep this file updated as work progresses.

Legend: `[ ]` pending, `[x]` complete, `[-]` skipped/blocked

---

## Phase 1: Project Setup

- [x] Initialize Cargo project with workspace structure
- [x] Configure Cargo.toml with initial dependencies
- [x] Set up basic project structure (src/, tests/, templates/)
- [x] Add rustfmt.toml and clippy configuration
- [x] Create .env.example with all environment variables
- [x] Set up GitHub Actions for CI (build, test, clippy, fmt)

## Phase 2: Core Infrastructure

### Configuration
- [x] Implement config loading from environment variables
- [x] Add config.toml file support as alternative
- [x] Validate configuration on startup
- [x] Write unit tests for config parsing

### Database
- [x] Set up SQLite connection with sqlx
- [x] Create initial migration (posts, links, link_occurrences, archives, archive_artifacts)
- [x] Add FTS5 virtual table and triggers
- [x] Implement database models (structs)
- [x] Write CRUD query functions
- [x] Configure WAL mode and pragmas
- [x] Write unit tests for database operations

### Logging
- [x] Set up tracing subscriber
- [x] Configure structured JSON logging for production
- [x] Add request tracing middleware for web server

## Phase 3: RSS Polling

### Poller
- [x] Implement RSS feed fetcher with reqwest
- [x] Parse RSS with feed-rs
- [x] Extract post metadata (guid, url, author, title, body)
- [x] Compute content hash for change detection
- [x] Store/update posts in database
- [x] Implement polling loop with configurable interval
- [x] Add adaptive polling (decay interval when no new content)
- [x] Write unit tests for RSS parsing
- [x] Write integration test for poll cycle

### Link Extraction
- [x] Parse HTML with scraper crate
- [x] Extract all `<a href>` tags
- [x] Detect quote context (aside.quote, blockquote)
- [x] Mark links as in_quote true/false
- [x] Extract context snippet around link
- [x] Deduplicate links by normalized URL
- [x] Write unit tests for link extraction
- [x] Write unit tests for quote detection

## Phase 4: URL Handling

### URL Normalization
- [x] Strip tracking parameters (utm_*, fbclid, etc.)
- [x] Force HTTPS, lowercase hostname
- [x] Remove default ports
- [x] Normalize trailing slashes
- [x] Write unit tests for normalization

### Site Handler Trait
- [x] Define SiteHandler trait
- [x] Define ArchiveResult struct
- [x] Create HandlerRegistry for URL dispatch
- [x] Implement URL pattern matching

### Individual Handlers
- [x] Reddit handler
  - [x] URL patterns for reddit.com, redd.it, old.reddit.com
  - [x] Normalize to old.reddit.com
  - [x] Resolve redd.it shortlinks
  - [x] Archive via yt-dlp
  - [x] Fetch JSON API data
  - [x] Write tests
- [x] TikTok handler
  - [x] URL patterns for tiktok.com, vm.tiktok.com
  - [x] Resolve vm.tiktok.com redirects
  - [x] Archive via yt-dlp
  - [x] Write tests
- [x] Twitter/X handler
  - [x] URL patterns for twitter.com, x.com
  - [x] Archive via yt-dlp and gallery-dl
  - [x] Write tests
- [x] YouTube handler
  - [x] URL patterns for youtube.com, youtu.be
  - [x] Archive via yt-dlp with subtitles
  - [x] Write tests
- [x] Instagram handler
  - [x] URL patterns for instagram.com
  - [x] Archive via gallery-dl
  - [x] Write tests
- [x] Imgur handler
  - [x] URL patterns for imgur.com, i.imgur.com
  - [x] Archive via gallery-dl
  - [x] Write tests
- [x] Generic HTTP handler for fallback
  - [x] Fetch raw HTML
  - [x] Extract metadata (title, OpenGraph)
  - [x] Extract readable text

## Phase 5: Archive Pipeline

### Worker Pool
- [x] Implement semaphore-based concurrency limiting
- [x] Create worker loop to process pending archives
- [x] Implement per-domain rate limiting
- [x] Handle worker errors gracefully

### External Tool Integration
- [x] yt-dlp subprocess wrapper
  - [x] Configure output format and options
  - [x] Parse metadata JSON output
  - [x] Handle timeouts and errors
- [x] gallery-dl subprocess wrapper
  - [x] Configure output options
  - [x] Parse output
  - [x] Handle timeouts and errors

### Archive Processing
- [x] Create temp working directory per job
- [x] Download content via appropriate tool
- [x] Extract metadata from downloaded files
- [x] Upload artifacts to S3
- [x] Update database with results
- [x] Clean up temp files
- [x] Implement retry logic with exponential backoff
- [x] Write integration tests for archive pipeline

### Wayback Machine Integration
- [x] Submit URLs to web.archive.org/save/
- [x] Rate limit submissions (5/minute)
- [x] Store wayback snapshot URL in database
- [x] Handle submission failures gracefully

## Phase 6: S3 Storage

- [x] Initialize S3 client with aws-sdk-s3
- [x] Support custom endpoints (MinIO, R2)
- [x] Implement file upload function
- [ ] Implement streaming upload for large files
- [x] Generate consistent S3 keys per storage layout spec
- [ ] Implement presigned URL generation (if needed)
- [ ] Write integration tests (with localstack or minio)

### Database Backup
- [x] Implement SQLite backup with VACUUM INTO
- [x] Compress backup with zstd
- [x] Upload backup to S3
- [x] Schedule daily backups
- [x] Implement backup retention (keep last 30)

## Phase 7: Web UI

### Server Setup
- [x] Initialize axum application
- [x] Configure tower-http middleware (CORS, compression)
- [x] Set up static file serving
- [x] Add request logging middleware

### Templates
- [x] Create base.html layout
- [x] Create home.html (recent archives grid)
- [x] Create search.html (search form + results)
- [x] Create archive_detail.html (single archive view)
- [x] Create post_detail.html (archives from one post)
- [x] Create site_list.html (browse by site)
- [x] Create stats.html (statistics page)
- [x] Create partials (archive_card, pagination, media_embed)

### Routes
- [x] GET / - home page with recent archives
- [x] GET /search - search with FTS
- [x] GET /archive/{id} - single archive detail
- [x] GET /post/{guid} - archives from discourse post
- [x] GET /site/{site} - browse by source site
- [x] GET /stats - processing statistics
- [x] GET /healthz - health check
- [x] GET /api/archives - JSON API
- [x] GET /api/search - JSON search API

### Styling
- [x] Add PicoCSS or similar classless framework
- [x] Create custom styles for archive cards
- [x] Ensure responsive design
- [x] Add media player styling

### Integration Tests
- [x] Test all routes return expected status codes
- [x] Test search returns relevant results
- [x] Test pagination works correctly

## Phase 8: Deployment

### Docker
- [x] Create Dockerfile with multi-stage build
- [x] Install yt-dlp, gallery-dl, ffmpeg in image
- [x] Create docker-compose.yml
- [x] Add MinIO service for local testing
- [x] Document Docker deployment

### Native Linux
- [x] Create install_dependencies.sh script
- [x] Create systemd service file
- [x] Document manual installation steps

### Configuration
- [x] Create config.example.toml
- [x] Document all environment variables
- [x] Add validation for required config

## Phase 9: Testing & Polish

- [ ] Achieve >80% code coverage on core logic
- [ ] Run load testing on web UI
- [ ] Profile memory usage during archive processing
- [x] Fix any clippy warnings
- [ ] Ensure all public APIs are documented
- [x] Update README with final instructions

---

## Phase 10: IPFS Integration

### IPFS Pinning
- [x] Add IPFS client module for local daemon communication
- [x] Add IPFS configuration (daemon URL, enabled flag, gateway URLs)
- [x] Add `ipfs_cid` field to archives table
- [x] Pin archived content to local IPFS daemon after S3 upload
- [x] Store IPFS CID in database
- [x] Generate public gateway URLs (ipfs.io, cloudflare-ipfs.com, dweb.link)
- [x] Update archive detail template to show IPFS links
- [x] Write unit tests for IPFS client
- [x] Handle IPFS daemon unavailability gracefully

## Phase 11: Manual Submission

### Submission Form
- [x] Create IP-based rate limiter (database-backed)
- [x] Add submission routes (GET /submit, POST /submit)
- [x] Create submission form template
- [x] Validate submitted URLs (URL format validation)
- [x] Create pending submissions table in database
- [x] Queue submissions for archiving
- [x] Add submission success/error templates
- [x] Write integration tests for submission flow

---

## Phase 12: High-Value Feature Additions

### New Site Handlers
- [x] Bluesky handler
  - [x] URL patterns for bsky.app, bsky.social
  - [x] Resolve handle to DID via AT Protocol
  - [x] Fetch post via getPostThread API
  - [x] Download embedded images from CDN
  - [x] Store post JSON and media
  - [x] Write unit tests
- [x] Streamable handler
  - [x] URL patterns for streamable.com
  - [x] Archive via yt-dlp (already supported)
  - [x] Write unit tests

### Archive.today Integration
- [x] Add archive.today client module
- [x] Submit URLs to archive.today/submit/
- [x] Rate limit submissions (3/minute)
- [x] Store archive.today URL in database
- [x] Add `archive_today_url` field to archives table
- [x] Handle submission failures gracefully
- [x] Add configuration (ARCHIVE_TODAY_ENABLED)

### RSS Feed of Archives
- [x] Add RSS 2.0 feed route at /feed.rss
- [x] Add Atom 1.0 feed route at /feed.atom
- [x] Include last 50 archives by default
- [ ] Add optional site/type query filters
- [x] Write unit tests for feed generation

### Content Deduplication
- [ ] Add image_hasher dependency
- [ ] Add perceptual_hash column to archive_artifacts
- [ ] Compute pHash for images during archiving
- [ ] Check for near-duplicates before downloading
- [ ] Link to existing archive if duplicate found
- [ ] Add similarity threshold configuration
- [ ] Write unit tests for hash comparison

### Screenshot Capture
- [x] Add chromiumoxide or headless_chrome dependency
- [x] Create screenshot capture module
- [x] Configure viewport dimensions
- [x] Capture full-page screenshots as PNG
- [x] Store in S3 render/ directory
- [x] Add configuration options
- [x] Handle browser startup/cleanup
- [x] Write unit tests

### PDF Generation
- [ ] Use browser print-to-PDF capability
- [ ] Configure paper size settings
- [ ] Generate PDF for article content
- [ ] Store in S3 render/ directory
- [ ] Add configuration options
- [ ] Write unit tests

### Dark Mode for Web UI
- [x] Add CSS dark mode variables
- [x] Implement prefers-color-scheme detection
- [x] Add manual toggle switch in header
- [x] Store preference in localStorage
- [x] Update PicoCSS to dark theme
- [x] Test all pages in dark mode

### Archive Comparison
- [x] Add similar crate for text diffing
- [x] Create comparison route /compare/{id1}/{id2}
- [x] Implement side-by-side diff view
- [x] Highlight additions/deletions
- [x] Show timestamp comparison
- [x] Create diff template

### Bulk Export
- [ ] Create export route /export/{site}
- [ ] Generate ZIP archive of domain content
- [ ] Exclude large video files (>50MB)
- [ ] Include metadata.json manifest
- [ ] Add rate limiting (1/hour per IP)
- [ ] Add max export size limit
- [ ] Stream ZIP generation to avoid memory issues
- [ ] Write unit tests

---

## Phase 13: NSFW Content Filtering

### Database Schema
- [x] Add `is_nsfw` boolean column to `archives` table (default false)
- [x] Add `nsfw_source` text column to track detection source (api/metadata/subreddit/manual)
- [x] Create migration v4 for NSFW columns
- [x] Create index for NSFW filtering queries

### Handler NSFW Detection
- [x] Reddit handler: Detect NSFW from subreddit `over_18` field or post data
  - [x] Parse `over_18` field from yt-dlp metadata JSON
  - [x] Detect NSFW subreddits by name patterns (nsfw, gonewild, porn, etc.)
- [x] YouTube/TikTok/Twitter handlers: Extract `age_limit` from yt-dlp info.json
  - [x] Parse age_limit field (>= 18 = NSFW)
- [x] Update ArchiveResult struct to include `is_nsfw: Option<bool>` and `nsfw_source: Option<String>`
- [x] Store NSFW status during archive completion in worker

### Frontend User Preference
- [x] Add NSFW visibility toggle in header (18+ button similar to dark mode toggle)
- [x] Store preference in localStorage (`nsfw_enabled` key)
- [x] Default to hiding NSFW content (safe by default)
- [x] Add JavaScript to toggle visibility dynamically without page reload

### Content Display Filtering
- [x] Add `data-nsfw="true"` attribute to archive cards for NSFW content
- [x] Add CSS to hide `[data-nsfw="true"]` elements when filter active
- [x] Add visual NSFW badge/indicator on archive cards (red badge)
- [x] Add warning banner on archive detail page for NSFW content
- [x] Respect filter on all pages: home, search, site list, post detail

### API Updates
- [x] Add `is_nsfw` field to Archive JSON response (automatic via serde)
- [ ] Add optional `?nsfw=show|hide|only` query parameter to API endpoints (optional enhancement)
- [ ] Filter archives in database queries when `nsfw=hide` (optional enhancement)

### Testing
- [x] Write unit tests for NSFW subreddit detection in Reddit handler

---

## Discovered Tasks

Add new tasks here as they are discovered during development:

- [x] Create lib.rs to expose modules for integration tests
- [x] Write database integration tests
- [x] Write web routes integration tests

### Archive Retry Improvements
- [x] Add `next_retry_at` and `last_attempt_at` columns to archives table (migration v5)
- [x] Implement exponential backoff for failed archives (5, 10, 20, 40 minutes)
- [x] Update retry query to respect `next_retry_at` timestamp
- [x] Reset stuck "processing" archives to "pending" on startup
- [x] Reset failed archives from today for retry on container restart
- [x] Add startup recovery function to archive worker
