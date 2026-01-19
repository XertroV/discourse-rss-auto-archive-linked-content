# Build stage
FROM rust:1.83-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for better caching
COPY Cargo.toml Cargo.lock ./

# Create dummy source files to build dependencies
RUN mkdir src && \
    echo 'fn main() {}' > src/main.rs && \
    echo 'pub fn lib() {}' > src/lib.rs

# Build dependencies only
RUN cargo build --release && \
    rm -rf src target/release/deps/discourse*

# Copy actual source code
COPY src ./src

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

# Create data directories
RUN mkdir -p /app/data/tmp && chown -R archiver:archiver /app

# Switch to non-root user
USER archiver

# Expose web server port
EXPOSE 8080

# Set default environment variables
ENV DATABASE_PATH=/app/data/archive.sqlite
ENV WORK_DIR=/app/data/tmp
ENV WEB_HOST=0.0.0.0
ENV WEB_PORT=8080
ENV YT_DLP_PATH=yt-dlp
ENV GALLERY_DL_PATH=gallery-dl

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/healthz || exit 1

# Run the application
CMD ["discourse-link-archiver"]
