#!/usr/bin/env sh
# Install dependencies for discourse-link-archiver on Alpine Linux
set -eu

echo "Installing dependencies for discourse-link-archiver (Alpine Linux)..."

# Install system packages
apk add --no-cache \
    build-base \
    pkgconfig \
    openssl-dev \
    sqlite-dev \
    ffmpeg \
    zstd \
    python3 \
    py3-pip \
    curl \
    rust \
    cargo

# Install yt-dlp and gallery-dl via pip
pip3 install --break-system-packages yt-dlp gallery-dl 2>/dev/null || \
    pip3 install yt-dlp gallery-dl

echo ""
echo "Dependencies installed successfully!"
echo ""
echo "Installed versions:"
echo "  rustc: $(rustc --version)"
echo "  ffmpeg: $(ffmpeg -version | head -1)"
echo "  yt-dlp: $(yt-dlp --version)"
echo "  gallery-dl: $(gallery-dl --version)"
echo "  zstd: $(zstd --version | head -1)"
