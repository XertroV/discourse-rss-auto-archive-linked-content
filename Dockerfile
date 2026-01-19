# Build stage
FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./

# Create dummy source files to build dependencies
# The dummy lib.rs must declare the same modules as the real lib.rs
# to ensure dependencies are compiled correctly
RUN mkdir -p src && \
    echo 'fn main() {}' > src/main.rs && \
    echo 'pub mod archive_today {} pub mod archiver {} pub mod backup {} pub mod config {} pub mod db {} pub mod handlers {} pub mod ipfs {} pub mod rss {} pub mod s3 {} pub mod tls {} pub mod wayback {} pub mod web {}' > src/lib.rs

# Build dependencies only (this caches all external crates)
RUN cargo build --release

# Remove ALL project-specific artifacts to force complete rebuild
# This includes fingerprints which track file modification times
RUN rm -rf src && \
    rm -rf target/release/deps/discourse* && \
    rm -rf target/release/deps/libdiscourse* && \
    rm -rf target/release/.fingerprint/discourse* && \
    rm -rf target/release/incremental/discourse* && \
    rm -rf target/release/discourse*

# Copy actual source code
COPY src ./src

# Touch all source files to ensure they're seen as newer than any cached metadata
# This guarantees Cargo will rebuild the project with the real source
RUN find src -name '*.rs' -exec touch {} +

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    ffmpeg \
    python3 \
    python3-pip \
    python3-venv \
    && rm -rf /var/lib/apt/lists/*

# Create a virtual environment for Python tools
RUN python3 -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

# Install yt-dlp and gallery-dl
RUN pip install --no-cache-dir yt-dlp gallery-dl

# Create non-root user
RUN useradd -r -s /bin/false -m -d /app archiver

WORKDIR /app

# Copy binary from builder stage
COPY --from=builder /app/target/release/discourse-link-archiver /usr/local/bin/

# Create data directories (including ACME certificate cache)
RUN mkdir -p /app/data/tmp /app/data/acme_cache && chown -R archiver:archiver /app

# Switch to non-root user
USER archiver

# Expose web server ports (HTTP and HTTPS)
EXPOSE 8080 443

# Set default environment variables
ENV DATABASE_PATH=/app/data/archive.sqlite
ENV WORK_DIR=/app/data/tmp
ENV WEB_HOST=0.0.0.0
ENV WEB_PORT=8080
ENV YT_DLP_PATH=yt-dlp
ENV GALLERY_DL_PATH=gallery-dl

# TLS settings (disabled by default; set TLS_ENABLED=true and TLS_DOMAINS to enable)
ENV TLS_ENABLED=false
ENV TLS_CACHE_DIR=/app/data/acme_cache
ENV TLS_HTTPS_PORT=443
# TLS_DOMAINS - comma-separated list of domains (required when TLS_ENABLED=true)
# TLS_CONTACT_EMAIL - optional, recommended for cert expiry notifications
# TLS_USE_STAGING - set to true for testing with staging certificates

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/healthz || exit 1

# Run the application
CMD ["discourse-link-archiver"]
