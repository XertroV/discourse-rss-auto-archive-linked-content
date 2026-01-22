# syntax=docker/dockerfile:1.4
# Build stage
FROM rust:1.88-bookworm AS builder

# Allows selecting a Cargo profile for builds.
# Options: release (default, optimized), release-fast (faster builds, less optimization), dev (fastest builds, no optimization)
# Examples:
#   docker build --build-arg CARGO_PROFILE=dev .           # Fast debug build
#   docker build --build-arg CARGO_PROFILE=release-fast . # Faster release build
#   docker build --build-arg CARGO_PROFILE=release .      # Fully optimized release build
ARG CARGO_PROFILE=release

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy all source files - no dummy file tricks
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build the application using BuildKit cache mounts for cargo registry and target
# This caches dependencies between builds without any fragile dummy file workarounds
# Note: 'dev' profile outputs to target/debug/, not target/dev/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --profile "${CARGO_PROFILE}" && \
    TARGET_DIR="$([ "${CARGO_PROFILE}" = "dev" ] && echo "debug" || echo "${CARGO_PROFILE}")" && \
    cp "target/${TARGET_DIR}/discourse-link-archiver" /usr/local/bin/

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies including Chromium for screenshots/PDF/MHTML
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    ffmpeg \
    python3 \
    python3-pip \
    python3-venv \
    unzip \
    sqlite3 \
    # Chromium for headless browser features (screenshots, PDF, MHTML)
    chromium \
    # Fonts for proper text rendering in screenshots
    fonts-liberation \
    fonts-noto-color-emoji \
    && rm -rf /var/lib/apt/lists/*

# Install Deno (JS runtime for yt-dlp)
RUN curl -fsSL https://deno.land/install.sh | DENO_INSTALL=/opt/deno sh
ENV PATH="/opt/deno/bin:$PATH"

# Create a virtual environment for Python tools
RUN python3 -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

# Install yt-dlp with curl-cffi for browser impersonation (required for TikTok)
# and gallery-dl
RUN pip install --no-cache-dir "yt-dlp[default,curl-cffi]" gallery-dl

# Install monolith for creating self-contained HTML archives
# Download pre-built binary from GitHub releases (faster than cargo install)
# Use latest version to get bug fixes for exit code 101 panics
ARG MONOLITH_VERSION=latest
RUN MONOLITH_LATEST=$(curl -fsSL "https://api.github.com/repos/Y2Z/monolith/releases/latest" | grep -oP '"tag_name": "v\K[^"]*' || echo "2.8.5") && \
    MONOLITH_VERSION=${MONOLITH_VERSION#latest} && \
    MONOLITH_VERSION=${MONOLITH_VERSION:-$MONOLITH_LATEST} && \
    echo "Installing monolith v${MONOLITH_VERSION}" && \
    curl -fsSL "https://github.com/Y2Z/monolith/releases/download/v${MONOLITH_VERSION}/monolith-gnu-linux-x86_64" \
    -o /usr/local/bin/monolith && chmod +x /usr/local/bin/monolith && \
    monolith --version

# Create non-root user
RUN useradd -r -s /bin/false -m -d /app archiver

# Set up yt-dlp cache directory for remote components
# yt-dlp stores downloaded challenge solvers in XDG_CACHE_HOME or ~/.cache
RUN mkdir -p /app/.cache/yt-dlp && chown -R archiver:archiver /app/.cache

# Set environment for yt-dlp cache before switching users
ENV HOME=/app
ENV XDG_CACHE_HOME=/app/.cache

# Pre-download yt-dlp remote components as the archiver user
# This downloads the JavaScript challenge solver script for YouTube bot detection
USER archiver
RUN echo "Installing yt-dlp remote components..." && \
    yt-dlp -U && \
    yt-dlp --version && \
    yt-dlp --remote-components ejs:github --simulate --verbose --print "%(title)s" https://www.youtube.com/watch?v=dQw4w9WgXcQ 2>&1 | head -50 || true
USER root

WORKDIR /app

# Copy binary from builder stage
COPY --from=builder /usr/local/bin/discourse-link-archiver /usr/local/bin/

# Copy static files directory
COPY --chown=archiver:archiver static ./static

# Entrypoint to fix volume permissions then drop privileges
COPY --chown=root:root scripts/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

# Create data directories (including ACME certificate cache)
RUN mkdir -p /app/data/tmp /app/data/acme_cache && chown -R archiver:archiver /app

USER root

# Expose web server ports (HTTP and HTTPS)
EXPOSE 8080 443

# Set default environment variables
ENV DATABASE_PATH=/app/data/archive.sqlite
ENV WORK_DIR=/app/data/tmp
ENV WEB_HOST=0.0.0.0
ENV WEB_PORT=8080
ENV YT_DLP_PATH=yt-dlp
ENV GALLERY_DL_PATH=gallery-dl
ENV MONOLITH_PATH=monolith

# Screenshot/PDF/MHTML settings (Chromium is installed in this image)
ENV SCREENSHOT_ENABLED=true
ENV PDF_ENABLED=true
ENV MHTML_ENABLED=true
ENV MONOLITH_ENABLED=true
ENV SCREENSHOT_CHROME_PATH=/usr/bin/chromium

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

# Run the application (entrypoint drops to non-root user)
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["/usr/local/bin/discourse-link-archiver"]
