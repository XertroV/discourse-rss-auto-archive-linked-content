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
- [x] Implement streaming upload for large files (completed in Phase 13 Stage 2)
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
- [x] Generate public gateway URLs (ipfs.io, dweb.link, gateway.pinata.cloud)
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
- [x] Add optional site/type query filters
- [x] Write unit tests for feed generation

### Content Deduplication
- [x] Add image_hasher dependency
- [x] Add perceptual_hash column to archive_artifacts
- [x] Compute pHash for images during archiving
- [x] Check for near-duplicates before downloading
- [x] Link to existing archive if duplicate found
- [x] Add similarity threshold configuration
- [x] Write unit tests for hash comparison

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
- [x] Use browser print-to-PDF capability
- [x] Configure paper size settings (PDF_PAPER_WIDTH, PDF_PAPER_HEIGHT)
- [x] Generate PDF for article content
- [x] Store in S3 render/page.pdf
- [x] Add configuration options (PDF_ENABLED, PDF_PAPER_WIDTH, PDF_PAPER_HEIGHT)
- [x] Write unit tests

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
- [x] Create export route /export/{site}
- [x] Generate ZIP archive of domain content
- [x] Exclude large video files (>50MB)
- [x] Include metadata.json manifest
- [x] Add rate limiting (1/hour per IP)
- [x] Add max export size limit (2GB)
- [x] Stream ZIP generation to avoid memory issues (using spawn_blocking)
- [x] Add database migration v8 for exports table
- [x] Add export tracking and rate limiting queries
- [x] Write unit tests

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
- [x] Add optional `?nsfw=show|hide|only` query parameter to API endpoints
- [x] Filter archives in database queries based on NSFW filter parameter

### Testing
- [x] Write comprehensive unit tests for NSFW subreddit detection in Reddit handler

---

## Phase 14: User Accounts & Admin Login

- [ ] Add auth database tables (users, sessions, approvals, roles) with indexes and timestamps
- [ ] Hash passwords with Argon2, enforce minimum length/entropy, and store password updated_at
- [ ] Registration flow that generates random usernames/passwords; first registered account becomes admin
- [ ] Approval workflow: admins approve users; approved users can be granted/revoked admin role; user deactivation
- [ ] Login/logout with HTTP-only, Secure session cookies, CSRF protection, session expiry/rotation, and IP/user-agent binding
- [ ] Brute-force safeguards: rate limit login/registration, lockout/backoff after repeated failures, audit log for auth events
- [ ] User profile: approved users can change password, set optional email, and set display name formatted like a username
- [ ] Admin/user submission permissions: only admins or approved users can submit links for archiving
- [ ] Admin UI: list users, approve users, promote/demote admins, reset passwords, view audit log
- [ ] UI and templates should use/extend the shadcn-inspired styles in static/css/style.css (login, registration, profile, admin views)
- [ ] Tests: unit and integration coverage for registration, approval, login, session handling, role checks, and password changes
- [ ] Approved users can toggle NSFW status on archived posts (should show in audit log)

## Discovered Tasks

Add new tasks here as they are discovered during development:

- [x] Create lib.rs to expose modules for integration tests
- [x] Write database integration tests
- [x] Write web routes integration tests
- [x] Improve archive list display (show original URL, domain, author, timestamp)
- [x] Add ArchiveDisplay struct for flattened archive+link data
- [x] Update SCREENSHOT_VIEWPORT_HEIGHT default to 3000 (taller screenshots)

### Archive Retry Improvements
- [x] Add `next_retry_at` and `last_attempt_at` columns to archives table (migration v5)
- [x] Implement exponential backoff for failed archives (5, 10, 20, 40 minutes)
- [x] Update retry query to respect `next_retry_at` timestamp
- [x] Reset stuck "processing" archives to "pending" on startup
- [x] Reset failed archives from today for retry on container restart
- [x] Add startup recovery function to archive worker

### Route Fixes
- [x] Fix path parameter syntax for axum 0.7 (use `:param` not `{param}`)
- [x] Add integration tests for /archive/:id and /post/:guid routes

### Archive Media Caching & Display
- [x] Cache filesize of archived media in database (archive_artifacts.size_bytes)
- [x] Insert artifact records when uploading files to S3 (primary, thumb, metadata, screenshot, PDF)
- [x] Show archived content list on /archive/:id page with direct links and file sizes
- [x] Display total size of all artifacts for an archive
- [x] Show embedded webpage preview (collapsible iframe) for HTML archives
- [x] Fix extra_files upload in worker - handlers can return extra files but they weren't uploaded
- [x] Embed CSS in archive banner for offline viewing (inline styles in view.html)

### HTML/PDF Archiving Workflow
Status: Complete. Full offline archiving implemented with multiple output formats.
Implemented:
- [x] Embed external CSS inline in HTML archives for offline viewing (via monolith)
- [x] Embed/download referenced images for HTML archives (via monolith)
- [x] Support font embedding for archived webpages (via monolith)
- [x] Create self-contained HTML option with all resources embedded (complete.html via monolith)
- [x] Add MHTML archive format (complete.mhtml via Chrome CDP)
- [x] Screenshot capture (screenshot.png via headless Chrome)
- [x] PDF generation (page.pdf via headless Chrome)

### Video Archiving Improvements

#### Stage 1: Safety Measures & Quality Selection (Complete)
- [x] Add YOUTUBE_MAX_DURATION_SECONDS config (default: 60 minutes, configurable up to 3 hours)
- [x] Add YOUTUBE_DOWNLOAD_TIMEOUT_SECONDS config (default: 2 hours)
- [x] Implement pre-flight metadata check before download (fetch duration without downloading)
- [x] Add timeout wrapper around yt-dlp download to prevent hung workers
- [x] Implement adaptive quality selection based on video characteristics:
  - [x] Short videos (<10 min): native resolution if ≤1920x1080, else 1080p
  - [x] Long videos with low bitrate (<500 KB/s, highly compressed): 1080p
  - [x] Long videos with normal bitrate: 720p for storage efficiency
- [x] Add config parameter to SiteHandler trait and update all handlers
- [x] Update S3Client with copy_object method for deduplication (uses download+re-upload fallback until Stage 2)
- [x] Document Stage 2 plan in STAGE2_STREAMING_UPLOAD.md

#### Stage 2: Streaming Upload (Complete)
See STAGE2_STREAMING_UPLOAD.md for full details:
- [x] Add aws-sdk-s3 dependency for multipart upload support
- [x] Implement multipart streaming upload (5MB chunks) to eliminate memory constraints
- [x] Add server-side S3 copy using aws-sdk-s3 CopyObject operation
- [x] Enable support for unlimited video lengths (no memory limit)
- [x] Add progress tracking for large file uploads
- [x] Keep rust-s3 for metadata operations (head_object, list_objects, etc.)
- [x] All existing tests pass with new implementation

### Video Path Aliasing (Complete)
Database-backed video deduplication system to store each video once and reference it from multiple archives:
- [x] Add `video_files` table with platform, video_id, s3_key, metadata_s3_key, size, content_type, duration
- [x] Add `video_file_id` column to `archive_artifacts` table (migration v10)
- [x] Add `VideoFile` model and query functions (find, get_or_create, insert, update)
- [x] Update worker to check database first for existing videos (with S3 fallback for migration)
- [x] Register new videos in database after uploading to canonical path (videos/{video_id}.{ext})
- [x] Create artifacts with `video_file_id` reference for deduplication tracking
- [x] Add video_id extraction to handlers: YouTube, TikTok, Streamable, Twitter, Reddit
- [x] Add comprehensive database tests for video file operations
- [x] Eliminates redundant storage on S3 (same video from different posts stored once)

### YouTube Transcripts & Subtitles
- [ ] Request English subtitles (manual and auto) in YouTube handler via yt-dlp (`--write-subs --write-auto-subs --sub-lang en --sub-format vtt`) and surface subtitle metadata in ArchiveResult
- [ ] Store subtitle artifacts separately (manual vs auto) with consistent S3 keys and artifact types; persist size, language, and kind in database records
- [ ] Add transcript build job that prefers manual subtitles (fallback to auto), flattens subtitle cues into a readable transcript with timestamps, and uploads as its own artifact
- [ ] Wire subtitle download and transcript build into archive worker/job tracking so YouTube archives enqueue transcript generation post-download without blocking video completion
- [ ] Render transcript on archive detail page as an auto-collapsed section (similar to plaintext content) with download links for manual/auto subtitle files when available
- [ ] Tests: YouTube handler requests subtitles, worker uploads subtitle/transcript artifacts, and web route renders transcript section when data exists
- [ ] Multi-language subs: download English tracks (manual and auto) plus the video's original language track when not English; label and store per-language artifacts
- [ ] Better formats: store both VTT and SRT for subtitle tracks; normalize filenames/S3 keys for consistency across archives (use s3 filename aliasing feature used for videos if appropriate)
- [ ] Quality/recency checks: record track source (manual/auto) and revision date if available; prefer freshest manual track, then manual, then auto when building transcript
- [ ] Resilience: retry/fallback when subtitles are missing or throttled; mark subtitle/transcript jobs as "subtitle-missing" without failing video archive
- [ ] UI polish: add per-cue timestamp links in transcript viewer to jump playback; support keyword highlighting and keep section auto-collapsed by default

### Future Improvements
- [x] Request largest RSS feed size via GET parameters (implemented via RSS_MAX_PAGES pagination)
- [ ] Upgrade axum from 0.7 to 0.8 (breaking change: path syntax changes from `:param` to `{param}`)
- [x] Archive failed log messages should include domain (e.g., `domain=old.reddit.com`) similar to `archive_id`

## Phase 14: Archive Page & Job Tracking Improvements

### Archive Page Display
- [x] Show screenshots on archive page (with status indication if missing/failed)
- [x] Open external/archived resource links in new tab (target=_blank)
- [x] Hide content immediately when NSFW toggled off on NSFW archive page

### Archive Method Improvements
- [x] Add cookie support for screenshot/PDF/MHTML capture (Chromium-based)
- [x] Save and show HTTP status code for archived pages (404, 401, 200, etc.)
- [x] Fix untitled YouTube videos issue (may be due to existing S3 video)
- [x] Add artifact and size tracking for cached YouTube videos
- [x] Save yt-dlp metadata JSON alongside video at videos/<video_id>.json

### Job Tracking System
- [x] Track archive jobs/steps and show on archive page (collapsible section)
- [x] Auto-collapse job details if all succeeded
- [x] Database schema for archive_jobs table with job_type, status, timestamps, error
- [x] Only run yt-dlp on Reddit if video is present (see reddit.rs:204-223)
- [ ] Only run yt-dlp on Twitter if video is present (requires API/scraping - future improvement)
- [ ] Design maintainable approach for job tracking and conditional tool execution

### Additional Improvements (Phase 14b)
- [x] Screenshots use webp format instead of png (better compression)
- [x] Plaintext content is collapsible with size info (default collapsed)
- [x] NSFW detection for Reddit posts/comments (metadata-based, not just subreddit)
- [x] Handle direct Reddit media URLs (i.redd.it images, v.redd.it videos)
- [x] More specific NSFW HTML detection (avoid false positives from user comments)
- [x] Add 18+ toggle tooltip with live NSFW count (updates via mutation observer)
