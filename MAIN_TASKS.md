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
- [ ] Add config.toml file support as alternative
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
- [ ] Configure structured JSON logging for production
- [ ] Add request tracing middleware for web server

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
- [ ] Write integration test for poll cycle

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
  - [ ] Resolve redd.it shortlinks
  - [x] Archive via yt-dlp
  - [ ] Fetch JSON API data
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
- [ ] Implement per-domain rate limiting
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
- [ ] Write integration tests for archive pipeline

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
- [ ] Set up static file serving
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
- [ ] Add media player styling

### Integration Tests
- [ ] Test all routes return expected status codes
- [ ] Test search returns relevant results
- [ ] Test pagination works correctly

## Phase 8: Deployment

### Docker
- [x] Create Dockerfile with multi-stage build
- [x] Install yt-dlp, gallery-dl, ffmpeg in image
- [x] Create docker-compose.yml
- [x] Add MinIO service for local testing
- [ ] Document Docker deployment

### Native Linux
- [x] Create install_dependencies.sh script
- [x] Create systemd service file
- [ ] Document manual installation steps

### Configuration
- [ ] Create config.example.toml
- [x] Document all environment variables
- [x] Add validation for required config

## Phase 9: Testing & Polish

- [ ] Achieve >80% code coverage on core logic
- [ ] Run load testing on web UI
- [ ] Profile memory usage during archive processing
- [ ] Fix any clippy warnings
- [ ] Ensure all public APIs are documented
- [ ] Update README with final instructions

---

## Discovered Tasks

Add new tasks here as they are discovered during development:

- [x] Create lib.rs to expose modules for integration tests
- [x] Write database integration tests
