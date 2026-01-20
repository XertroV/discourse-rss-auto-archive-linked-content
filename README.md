# Discourse Link Archiver

A Rust service that monitors a Discourse forum's RSS feed and automatically archives linked content from ephemeral platforms (Reddit, TikTok, Twitter/X, YouTube, Instagram, etc.) to S3 storage before it disappears.

## Features

### Archiving Engine

**Supported Platforms:**
- Reddit (posts, comments, galleries, videos)
- YouTube (videos, playlists, live streams, shorts)
- TikTok (videos with metadata)
- Twitter/X (tweets, quoted tweets, reply chains)
- Instagram (posts, reels, stories)
- Imgur (images and albums)
- Bluesky (posts and threads)
- Streamable (videos)
- Generic fallback for any URL

**Archive Artifacts:**
- Video downloads via yt-dlp (best quality with subtitles/transcripts)
- Images via gallery-dl with metadata
- Self-contained HTML archives (Monolith, MHTML)
- Screenshots and PDF snapshots
- Comments extraction (YouTube, Reddit, TikTok, Twitter)
- Platform metadata (JSON format)

**Advanced Archiving:**
- Video deduplication (canonical storage with references)
- Perceptual hashing for content deduplication
- Real-time download progress tracking
- Automatic NSFW detection and tagging
- Retry logic with exponential backoff
- Per-domain rate limiting
- Cookie support for authenticated downloads
- Playlist archiving support

### RSS Feed Processing

- Automatic RSS polling with configurable intervals
- Multi-page feed fetching
- New post detection and edit monitoring
- Smart link extraction with quote detection
- Quote-only policy (skip links only in quotes)
- URL normalization and redirect resolution
- Thread-level archiving via RSS

### Web Interface

**Browse & Discover:**
- Recent archives grid with card-based layout
- All archives table view (high-density, 1000 items/page)
- Failed archives monitoring
- Thread view (archives grouped by Discourse thread)
- Post view (archives from specific posts)
- Site browsing (filter by domain)
- Statistics dashboard

**Search & Filter:**
- Full-text search (FTS5) across titles, authors, content
- Content type filters (video, image, gallery, text, thread, playlist)
- Source filters (Reddit, YouTube, TikTok, Twitter/X)
- Status filters (complete, failed, pending, processing, skipped)
- Combined filter preservation across pagination

**Interactive Features:**
- Comment system with threaded replies
- Comment reactions (helpful votes)
- Comment edit history tracking
- Pin important comments (admin)
- Re-archive and retry failed archives
- NSFW toggle for content
- Archive comparison (text diff between versions)
- Manual URL submission form
- Bulk thread archiving

**Export & Feeds:**
- RSS/Atom feeds of recent archives
- Bulk ZIP export by domain with metadata manifest
- API endpoints (JSON, search, comments)

### User Management & Security

**Authentication:**
- Self-service registration with admin approval
- Session-based authentication with CSRF protection
- Password hashing (bcrypt)
- Account lockout after failed login attempts
- Discourse forum account linking via verification

**Admin Features:**
- User management panel (approve, revoke, promote, deactivate)
- Role-based access control
- Excluded domains configuration
- Audit logging
- Comment moderation
- Per-user archive filtering

**Security:**
- Rate limiting on submissions (per IP/user)
- Secure session management
- IP logging with proxy header support
- X-No-Archive header support

### Storage & Redundancy

**S3-Compatible Storage:**
- AWS S3, MinIO, Cloudflare R2 support
- Custom endpoint configuration
- Streaming uploads for large files
- Configurable prefixes and regions
- Public URL serving via `/s3/*` proxy

**Database:**
- SQLite with WAL mode
- FTS5 full-text search
- Automated schema migrations
- Foreign key constraints
- Optimized indexes

**Backups:**
- Automatic database backups to S3
- zstd compression
- Configurable retention policy
- Hourly scheduling

**External Archives:**
- Wayback Machine submission with rate limiting
- Archive.today submission
- Optional IPFS pinning with multi-gateway support

### Production Ready

**TLS/HTTPS:**
- Automatic Let's Encrypt certificates (ACME protocol)
- Multi-domain support
- HTTP to HTTPS redirect
- Certificate auto-renewal
- Staging mode for testing

**Monitoring:**
- Health check endpoint (`/healthz`)
- Structured logging (JSON or pretty format)
- Queue inspection (debug mode)
- Worker statistics tracking
- Request tracing with client IPs

**Configuration:**
- Environment variable based
- Optional TOML config files
- Comprehensive defaults
- Validation on startup

**Deployment:**
- Docker Compose setup with MinIO
- Systemd service files
- Multi-distribution install scripts (Ubuntu, Fedora, Arch, Alpine, openSUSE)
- Low-downtime update scripts

## Requirements

- Rust 1.70+
- yt-dlp
- gallery-dl
- ffmpeg
- SQLite 3.35+ (with FTS5)

## Installation

### Docker (Recommended)

The easiest way to run the archiver is with Docker Compose, which includes MinIO for local S3-compatible storage.

1. **Clone the repository:**
   ```bash
   git clone https://github.com/XertroV/discourse-rss-auto-archive-linked-content
   cd discourse-rss-auto-archive-linked-content
   ```

2. **Configure environment:**
   ```bash
   cp .env.example .env
   # Edit .env with your settings (at minimum, set RSS_URL)
   ```

3. **Start the services:**
   ```bash
   docker-compose up -d
   ```

4. **Access the web UI:**
   - Archiver: http://localhost:8080
   - MinIO Console: http://localhost:9001 (user: minioadmin, password: minioadmin)

**Using with external S3:**

To use AWS S3 or another S3-compatible service instead of MinIO, modify `docker-compose.yml`:

```yaml
services:
  archiver:
    environment:
      S3_BUCKET: your-bucket-name
      S3_REGION: us-east-1
      S3_ENDPOINT: ""  # Leave empty for AWS S3, or set for R2/other
      AWS_ACCESS_KEY_ID: your-access-key
      AWS_SECRET_ACCESS_KEY: your-secret-key
```

Then remove or comment out the `minio` and `minio-init` services.

**Production with Cloudflare R2 or AWS S3:**

See [PRODUCTION_SETUP.md](PRODUCTION_SETUP.md) for complete setup guide with external S3/R2 storage.

**Mounting a cookies file:**

For authenticated downloads (e.g., age-restricted content), uncomment the cookies volume mount in `docker-compose.yml`:

```yaml
volumes:
  - archiver-data:/app/data
  - ./cookies.txt:/app/cookies.txt:ro  # Uncomment this line
```

### Common Docker Commands

Convenience scripts for managing the Docker deployment:

**Basic Operations:**
```bash
./dc-start.sh         # Start services
./dc-stop.sh          # Stop services
./dc-restart.sh       # Restart services (recreate containers)
./dc-logs.sh          # View logs (all services)
./dc-logs.sh archiver # View logs (archiver only)
./dc-ps.sh            # Check service status
```

**Build & Update:**
```bash
./dc-rebuild.sh                              # Rebuild Docker image
./dc-update-low-downtime.sh                  # Git pull + rebuild + restart (minimal downtime)
./dc-rebuild.sh && ./dc-restart.sh           # Full rebuild and restart
```

**Database Operations:**
```bash
./dc-reset-db.sh                             # Delete database, restart fresh
./dc-rebuild.sh && ./dc-reset-db.sh          # Rebuild + fresh database + logs
```

**Common Sequences:**

```bash
# Fresh start after git pull (rebuild, reset DB, watch logs)
./dc-rebuild.sh && ./dc-reset-db.sh && ./dc-logs.sh

# Quick update without database reset
./dc-update-low-downtime.sh

# Full clean restart (preserves data)
./dc-down-up.sh

# Rebuild after code changes and restart
./dc-rebuild.sh && ./dc-restart.sh
```

### Manual Installation (Native Linux)

1. **Install dependencies:**

   Choose the script for your distribution:
   ```bash
   # Ubuntu/Debian
   ./scripts/install_deps_ubuntu.sh

   # Fedora/RHEL
   ./scripts/install_deps_fedora.sh

   # Arch Linux
   ./scripts/install_deps_arch.sh

   # Alpine Linux
   ./scripts/install_deps_alpine.sh

   # openSUSE
   ./scripts/install_deps_opensuse.sh
   ```

   Or install manually:
   ```bash
   # Install Rust
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source ~/.cargo/env

   # Install system packages (Ubuntu/Debian example)
   sudo apt-get install -y build-essential pkg-config libssl-dev libsqlite3-dev ffmpeg zstd

   # Install Python tools
   pip3 install --user yt-dlp gallery-dl
   ```

2. **Build the application:**
   ```bash
   cargo build --release
   ```

3. **Configure:**
   ```bash
   cp .env.example .env
   # Edit .env with your settings
   ```

4. **Create data directories:**
   ```bash
   mkdir -p data/tmp
   ```

5. **Run:**
   ```bash
   ./target/release/discourse-link-archiver
   ```

### Systemd Service (Production)

For production deployments, use the provided systemd service file:

1. **Create a dedicated user:**
   ```bash
   sudo useradd -r -s /bin/false -m -d /opt/discourse-link-archiver archiver
   ```

2. **Set up the installation directory:**
   ```bash
   sudo mkdir -p /opt/discourse-link-archiver/{bin,data/tmp}
   sudo cp target/release/discourse-link-archiver /opt/discourse-link-archiver/bin/
   sudo cp .env /opt/discourse-link-archiver/config.env
   sudo chown -R archiver:archiver /opt/discourse-link-archiver
   sudo chmod 600 /opt/discourse-link-archiver/config.env
   ```

3. **Install the service:**
   ```bash
   sudo cp discourse-link-archiver.service /etc/systemd/system/
   sudo systemctl daemon-reload
   sudo systemctl enable discourse-link-archiver
   sudo systemctl start discourse-link-archiver
   ```

4. **Check status:**
   ```bash
   sudo systemctl status discourse-link-archiver
   sudo journalctl -u discourse-link-archiver -f
   ```

## Configuration

All configuration is done via environment variables. Create a `.env` file or set them in your environment.

### Required

| Variable | Description |
|----------|-------------|
| `RSS_URL` | Discourse posts.rss URL |
| `S3_BUCKET` | S3 bucket name |
| `AWS_ACCESS_KEY_ID` | AWS/S3 access key |
| `AWS_SECRET_ACCESS_KEY` | AWS/S3 secret key |

### HTTPS with Let's Encrypt

The service supports automatic HTTPS with Let's Encrypt certificates:

```bash
TLS_ENABLED=true
TLS_DOMAINS=cf-archiver.xk.io  # Your domain(s), comma-separated
TLS_CONTACT_EMAIL=admin@example.com  # Optional but recommended
TLS_HTTPS_PORT=443
```

When TLS is enabled:
- HTTPS server runs on `TLS_HTTPS_PORT` (default: 443)
- HTTP server on `WEB_PORT` redirects all traffic to HTTPS
- Certificates are automatically obtained and renewed via ACME TLS-ALPN-01
- Certificates are cached in `TLS_CACHE_DIR` (default: `./data/acme_cache`)

For testing, set `TLS_USE_STAGING=true` to use Let's Encrypt staging (avoids rate limits).

### Optional

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_PATH` | `./data/archive.sqlite` | SQLite database file path |
| `S3_REGION` | `us-east-1` | S3 region |
| `S3_ENDPOINT` | *(empty)* | Custom S3 endpoint (for MinIO/R2) |
| `S3_PREFIX` | `archives/` | Key prefix for uploaded files |
| `POLL_INTERVAL_SECS` | `60` | RSS polling interval |
| `WORKER_CONCURRENCY` | `4` | Max concurrent archive jobs |
| `PER_DOMAIN_CONCURRENCY` | `1` | Max concurrent jobs per domain |
| `ARCHIVE_MODE` | `deletable` | `deletable` or `all` |
| `WEB_HOST` | `0.0.0.0` | Web server bind address |
| `WEB_PORT` | `8080` | Web server port |
| `WAYBACK_ENABLED` | `true` | Submit URLs to Wayback Machine |
| `BACKUP_ENABLED` | `true` | Enable automatic database backups |
| `IPFS_ENABLED` | `false` | Enable IPFS pinning |
| `SUBMISSION_ENABLED` | `true` | Enable manual URL submission |
| `SUBMISSION_RATE_LIMIT_PER_HOUR` | `60` | Max submissions per IP per hour |
| `LOG_FORMAT` | `pretty` | `pretty` or `json` |

See `.env.example` for the complete list.

## Web UI

The web interface provides:

- **Home** (`/`) - Recent archives grid
- **Search** (`/search`) - Full-text search across archives
- **Archive Detail** (`/archive/{id}`) - View a single archive
- **Post Archives** (`/post/{guid}`) - All archives from a Discourse post
- **Site Browse** (`/site/{domain}`) - Browse by source site
- **Statistics** (`/stats`) - Processing statistics
- **Submit** (`/submit`) - Manual URL submission form

### API Endpoints

- `GET /api/archives` - List recent archives (JSON)
- `GET /api/search?q=query` - Search archives (JSON)
- `GET /healthz` - Health check

## Documentation

- `SPEC.md` - Full technical specification
- `CLAUDE.md` - Development guidelines
- `MAIN_TASKS.md` - Development task tracker

## License

MIT
