# Discourse Link Archiver

A Rust service that monitors a Discourse forum's RSS feed and automatically archives linked content from ephemeral platforms (Reddit, TikTok, Twitter/X, YouTube, Instagram, etc.) to S3 storage before it disappears.

## Features

- Polls Discourse RSS feed for new posts
- Extracts and normalizes links from post content
- Archives media via yt-dlp and gallery-dl
- Stores artifacts in S3-compatible storage
- SQLite database with full-text search
- Public web UI for browsing and searching archives
- Wayback Machine submission for redundancy

## Requirements

- Rust 1.70+
- yt-dlp
- gallery-dl
- ffmpeg
- SQLite 3.35+ (with FTS5)

## Quick Start

```bash
# Install dependencies
./scripts/install_dependencies.sh

# Configure
cp .env.example .env
# Edit .env with your settings

# Build and run
cargo build --release
./target/release/discourse-link-archiver
```

## Configuration

Set these environment variables (or use config.toml):

```bash
RSS_URL=https://your-forum.com/posts.rss
DATABASE_PATH=./data/archive.sqlite
S3_BUCKET=your-bucket
S3_REGION=us-east-1
AWS_ACCESS_KEY_ID=xxx
AWS_SECRET_ACCESS_KEY=xxx
```

See `SPEC.md` for full configuration options.

## Docker

```bash
docker-compose up -d
```

## Documentation

- `SPEC.md` - Full technical specification
- `CLAUDE.md` - Development guidelines
- `MAIN_TASKS.md` - Development task tracker

## License

MIT
