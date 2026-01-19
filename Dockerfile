# syntax=docker/dockerfile:1.4
# Build stage
FROM rust:1.85-bookworm AS builder

# Allows selecting a faster Cargo profile for iterative builds.
# Examples:
#   docker build --build-arg CARGO_PROFILE=release-fast .
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
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --profile "${CARGO_PROFILE}" && \
    cp "target/${CARGO_PROFILE}/discourse-link-archiver" /usr/local/bin/

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
    unzip \
    && rm -rf /var/lib/apt/lists/*

# Install Deno (JS runtime for yt-dlp)
RUN curl -fsSL https://deno.land/install.sh | DENO_INSTALL=/opt/deno sh
ENV PATH="/opt/deno/bin:$PATH"

# Create a virtual environment for Python tools
RUN python3 -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

# Install yt-dlp and gallery-dl
RUN pip install --no-cache-dir yt-dlp gallery-dl

# Create non-root user
RUN useradd -r -s /bin/false -m -d /app archiver

WORKDIR /app

# Copy binary from builder stage
COPY --from=builder /usr/local/bin/discourse-link-archiver /usr/local/bin/

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
