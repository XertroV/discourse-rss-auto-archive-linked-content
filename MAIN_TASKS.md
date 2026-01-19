# Main Tasks

Development task tracker for discourse-link-archiver. Keep this file updated as work progresses.

Legend: `[ ]` pending, `[x]` complete, `[-]` skipped/blocked

---

## Phase 1: Project Setup

- [ ] Initialize Cargo project with workspace structure
- [ ] Configure Cargo.toml with initial dependencies
- [ ] Set up basic project structure (src/, tests/, templates/)
- [ ] Add rustfmt.toml and clippy configuration
- [ ] Create .env.example with all environment variables
- [ ] Set up GitHub Actions for CI (build, test, clippy, fmt)

## Phase 2: Core Infrastructure

### Configuration
- [ ] Implement config loading from environment variables
- [ ] Add config.toml file support as alternative
- [ ] Validate configuration on startup
- [ ] Write unit tests for config parsing

### Database
- [ ] Set up SQLite connection with sqlx
- [ ] Create initial migration (posts, links, link_occurrences, archives, archive_artifacts)
- [ ] Add FTS5 virtual table and triggers
- [ ] Implement database models (structs)
- [ ] Write CRUD query functions
- [ ] Configure WAL mode and pragmas
- [ ] Write unit tests for database operations

### Logging
- [ ] Set up tracing subscriber
- [ ] Configure structured JSON logging for production
- [ ] Add request tracing middleware for web server

## Phase 3: RSS Polling

### Poller
- [ ] Implement RSS feed fetcher with reqwest
- [ ] Parse RSS with feed-rs
- [ ] Extract post metadata (guid, url, author, title, body)
- [ ] Compute content hash for change detection
- [ ] Store/update posts in database
- [ ] Implement polling loop with configurable interval
- [ ] Add adaptive polling (decay interval when no new content)
- [ ] Write unit tests for RSS parsing
- [ ] Write integration test for poll cycle

### Link Extraction
- [ ] Parse HTML with scraper crate
- [ ] Extract all `<a href>` tags
- [ ] Detect quote context (aside.quote, blockquote)
- [ ] Mark links as in_quote true/false
- [ ] Extract context snippet around link
- [ ] Deduplicate links by normalized URL
- [ ] Write unit tests for link extraction
- [ ] Write unit tests for quote detection

## Phase 4: URL Handling

### URL Normalization
- [ ] Strip tracking parameters (utm_*, fbclid, etc.)
- [ ] Force HTTPS, lowercase hostname
- [ ] Remove default ports
- [ ] Normalize trailing slashes
- [ ] Write unit tests for normalization

### Site Handler Trait
- [ ] Define SiteHandler trait
- [ ] Define ArchiveResult struct
- [ ] Create HandlerRegistry for URL dispatch
- [ ] Implement URL pattern matching

### Individual Handlers
- [ ] Reddit handler
  - [ ] URL patterns for reddit.com, redd.it, old.reddit.com
  - [ ] Normalize to old.reddit.com
  - [ ] Resolve redd.it shortlinks
  - [ ] Archive via yt-dlp
  - [ ] Fetch JSON API data
  - [ ] Write tests
- [ ] TikTok handler
  - [ ] URL patterns for tiktok.com, vm.tiktok.com
  - [ ] Resolve vm.tiktok.com redirects
  - [ ] Archive via yt-dlp
  - [ ] Write tests
- [ ] Twitter/X handler
  - [ ] URL patterns for twitter.com, x.com
  - [ ] Archive via yt-dlp and gallery-dl
  - [ ] Write tests
- [ ] YouTube handler
  - [ ] URL patterns for youtube.com, youtu.be
  - [ ] Archive via yt-dlp with subtitles
  - [ ] Write tests
- [ ] Instagram handler
  - [ ] URL patterns for instagram.com
  - [ ] Archive via gallery-dl
  - [ ] Write tests
- [ ] Imgur handler
  - [ ] URL patterns for imgur.com, i.imgur.com
  - [ ] Archive via gallery-dl
  - [ ] Write tests
- [ ] Generic HTTP handler for fallback
  - [ ] Fetch raw HTML
  - [ ] Extract metadata (title, OpenGraph)
  - [ ] Extract readable text

## Phase 5: Archive Pipeline

### Worker Pool
- [ ] Implement semaphore-based concurrency limiting
- [ ] Create worker loop to process pending archives
- [ ] Implement per-domain rate limiting
- [ ] Handle worker errors gracefully

### External Tool Integration
- [ ] yt-dlp subprocess wrapper
  - [ ] Configure output format and options
  - [ ] Parse metadata JSON output
  - [ ] Handle timeouts and errors
- [ ] gallery-dl subprocess wrapper
  - [ ] Configure output options
  - [ ] Parse output
  - [ ] Handle timeouts and errors

### Archive Processing
- [ ] Create temp working directory per job
- [ ] Download content via appropriate tool
- [ ] Extract metadata from downloaded files
- [ ] Upload artifacts to S3
- [ ] Update database with results
- [ ] Clean up temp files
- [ ] Implement retry logic with exponential backoff
- [ ] Write integration tests for archive pipeline

### Wayback Machine Integration
- [ ] Submit URLs to web.archive.org/save/
- [ ] Rate limit submissions (5/minute)
- [ ] Store wayback snapshot URL in database
- [ ] Handle submission failures gracefully

## Phase 6: S3 Storage

- [ ] Initialize S3 client with aws-sdk-s3
- [ ] Support custom endpoints (MinIO, R2)
- [ ] Implement file upload function
- [ ] Implement streaming upload for large files
- [ ] Generate consistent S3 keys per storage layout spec
- [ ] Implement presigned URL generation (if needed)
- [ ] Write integration tests (with localstack or minio)

### Database Backup
- [ ] Implement SQLite backup with VACUUM INTO
- [ ] Compress backup with zstd
- [ ] Upload backup to S3
- [ ] Schedule daily backups
- [ ] Implement backup retention (keep last 30)

## Phase 7: Web UI

### Server Setup
- [ ] Initialize axum application
- [ ] Configure tower-http middleware (CORS, compression)
- [ ] Set up static file serving
- [ ] Add request logging middleware

### Templates
- [ ] Create base.html layout
- [ ] Create home.html (recent archives grid)
- [ ] Create search.html (search form + results)
- [ ] Create archive_detail.html (single archive view)
- [ ] Create post_detail.html (archives from one post)
- [ ] Create site_list.html (browse by site)
- [ ] Create stats.html (statistics page)
- [ ] Create partials (archive_card, pagination, media_embed)

### Routes
- [ ] GET / - home page with recent archives
- [ ] GET /search - search with FTS
- [ ] GET /archive/{id} - single archive detail
- [ ] GET /post/{guid} - archives from discourse post
- [ ] GET /site/{site} - browse by source site
- [ ] GET /stats - processing statistics
- [ ] GET /healthz - health check
- [ ] GET /api/archives - JSON API
- [ ] GET /api/search - JSON search API

### Styling
- [ ] Add PicoCSS or similar classless framework
- [ ] Create custom styles for archive cards
- [ ] Ensure responsive design
- [ ] Add media player styling

### Integration Tests
- [ ] Test all routes return expected status codes
- [ ] Test search returns relevant results
- [ ] Test pagination works correctly

## Phase 8: Deployment

### Docker
- [ ] Create Dockerfile with multi-stage build
- [ ] Install yt-dlp, gallery-dl, ffmpeg in image
- [ ] Create docker-compose.yml
- [ ] Add MinIO service for local testing
- [ ] Document Docker deployment

### Native Linux
- [ ] Create install_dependencies.sh script
- [ ] Create systemd service file
- [ ] Document manual installation steps

### Configuration
- [ ] Create config.example.toml
- [ ] Document all environment variables
- [ ] Add validation for required config

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

- [ ] (placeholder for discovered tasks)
