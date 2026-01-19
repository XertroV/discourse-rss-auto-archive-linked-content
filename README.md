# Discourse Link Archiver

A Rust service that monitors a Discourse forum's RSS feed and automatically archives linked content from ephemeral platforms (Reddit, TikTok, Twitter/X, YouTube, Instagram, etc.) to S3 storage before it disappears.

## Features

- Polls Discourse RSS feed for new posts
- Extracts and normalizes links from post content
- Archives media via yt-dlp and gallery-dl
- Stores artifacts in S3-compatible storage (AWS S3, MinIO, Cloudflare R2)
- SQLite database with full-text search
- Public web UI for browsing and searching archives
- Wayback Machine submission for redundancy
- Optional IPFS pinning for decentralized storage
- Manual URL submission form with rate limiting

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

**Mounting a cookies file:**

For authenticated downloads (e.g., age-restricted content), uncomment the cookies volume mount in `docker-compose.yml`:

```yaml
volumes:
  - archiver-data:/app/data
  - ./cookies.txt:/app/cookies.txt:ro  # Uncomment this line
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
