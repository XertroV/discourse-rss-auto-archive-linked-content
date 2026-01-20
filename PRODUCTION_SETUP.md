# Production R2 Setup Guide

This guide covers setting up the archiver with Cloudflare R2 (or any external S3-compatible storage).

## Configuration Files

- **docker-compose.yml** - Base configuration (MinIO dependency commented out for production)
- **docker-compose.prod.yml** - Production override (uses R2 credentials from .env)
- **.env** - Your production credentials (not in git)

**Note:** The MinIO dependency is commented out in docker-compose.yml by default for production use. If you want to run locally with MinIO, uncomment the `depends_on` section in docker-compose.yml.

## Environment Variables

Your `.env` file should contain:

```bash
# S3/R2 Configuration
S3_BUCKET=cf-archiver
S3_REGION=us-east-1
S3_ENDPOINT=https://78b58ea549bde2caee1829e6c5b4135e.r2.cloudflarestorage.com
S3_PREFIX=a/
S3_PUBLIC_URL_BASE=https://pub-xxxxx.r2.dev  # Your R2 public bucket URL (for redirects)
AWS_ACCESS_KEY_ID=your-access-key-id
AWS_SECRET_ACCESS_KEY=your-secret-access-key

# Other settings...
RSS_URL=https://discuss.criticalfallibilism.com/posts.rss
DATABASE_PATH=/app/data/archive.sqlite
WEB_PORT=8080
# etc.
```

## Updated Scripts

All `dc-*.sh` scripts now automatically use both:
- `docker-compose.yml` (base configuration)
- `docker-compose.prod.yml` (production overrides)

This means:
- MinIO is disabled in production
- R2 credentials from `.env` are used
- No need to run separate commands

## Fresh Start Steps

### On Your Server

1. **Stop the current service:**
   ```bash
   cd /path/to/discourse-rss-auto-archive-linked-content
   ./dc-stop.sh
   ```

2. **Pull the updated code:**
   ```bash
   git pull
   ```

3. **Verify your .env file has R2 credentials:**
   ```bash
   grep -E "^(S3_|AWS_)" .env
   ```

4. **Reset the database:**
   ```bash
   ./dc-reset-db.sh
   ```
   This will:
   - Stop the archiver
   - Delete the SQLite database
   - Preserve TLS certificates
   - Restart the archiver

5. **View logs to verify R2 connection:**
   ```bash
   ./dc-logs.sh archiver
   ```

6. **Check service status:**
   ```bash
   ./dc-ps.sh
   ```

7. **Test the web interface:**
   ```bash
   curl http://localhost:8080/healthz
   ```

## Common Commands

```bash
# Start services
./dc-start.sh

# Stop services
./dc-stop.sh

# View logs
./dc-logs.sh archiver

# Check status
./dc-ps.sh

# Restart (recreate containers)
./dc-restart.sh

# Rebuild and restart
./dc-rebuild.sh && ./dc-restart.sh

# Low-downtime update (git pull + rebuild + restart)
./dc-update-low-downtime.sh

# Reset database
./dc-reset-db.sh
```

## Verification

After starting, check that:

1. **Service is running:**
   ```bash
   ./dc-ps.sh
   # Should show archiver as "Up" (no MinIO)
   ```

2. **Healthcheck passes:**
   ```bash
   curl http://localhost:8080/healthz
   # Should return "OK"
   ```

3. **Logs show R2 connection:**
   ```bash
   ./dc-logs.sh archiver
   # Look for successful S3 operations, no MinIO references
   ```

4. **Database initialized:**
   ```bash
   docker exec discourse-rss-auto-archive-linked-content-archiver-1 ls -lh /app/data/
   # Should show archive.sqlite
   ```

## Troubleshooting

### R2 connection errors
Check logs:
```bash
./dc-logs.sh archiver | grep -i "s3\|error"
```

Common issues:
- Wrong endpoint URL format (should include `https://`)
- Incorrect access keys
- Bucket doesn't exist or wrong region

### Database not found
If you see "database not found" errors, the database will be created automatically on first run. Check:
```bash
./dc-logs.sh archiver | grep -i "database\|migration"
```

## Switching Back to Local Dev (MinIO)

If you need to test locally with MinIO:

```bash
# Use only the base docker-compose.yml
docker compose -f docker-compose.yml up -d

# Or temporarily rename docker-compose.prod.yml
mv docker-compose.prod.yml docker-compose.prod.yml.disabled
./dc-start.sh
mv docker-compose.prod.yml.disabled docker-compose.prod.yml
```

## Next Steps

After setup:
1. The archiver will start polling the RSS feed
2. Links will be detected and queued for archiving
3. Archives will be stored in R2 under `a/` prefix
4. View progress at `http://your-server:8080`

Monitor with:
```bash
./dc-logs.sh archiver -f
```
