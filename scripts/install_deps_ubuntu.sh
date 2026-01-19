#!/usr/bin/env bash
# Install dependencies for discourse-link-archiver on Ubuntu/Debian
set -euo pipefail

echo "Installing dependencies for discourse-link-archiver (Ubuntu/Debian)..."

# Update package lists
sudo apt-get update

# Install system packages
sudo apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    ffmpeg \
    zstd \
    python3 \
    python3-pip \
    curl

# Install yt-dlp (prefer pipx or pip for latest version)
if command -v pipx &> /dev/null; then
    pipx install yt-dlp
    pipx install gallery-dl
elif command -v pip3 &> /dev/null; then
    pip3 install --user yt-dlp gallery-dl
else
    echo "Warning: pip3 not found, installing yt-dlp from apt (may be outdated)"
    sudo apt-get install -y yt-dlp
fi

# Install Rust if not present
if ! command -v rustc &> /dev/null; then
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

echo ""
echo "Dependencies installed successfully!"
echo ""
echo "Installed versions:"
echo "  rustc: $(rustc --version 2>/dev/null || echo 'not in PATH - run: source ~/.cargo/env')"
echo "  ffmpeg: $(ffmpeg -version 2>/dev/null | head -1 || echo 'not found')"
echo "  yt-dlp: $(yt-dlp --version 2>/dev/null || echo 'not in PATH')"
echo "  gallery-dl: $(gallery-dl --version 2>/dev/null || echo 'not in PATH')"
echo "  zstd: $(zstd --version 2>/dev/null | head -1 || echo 'not found')"
echo ""
echo "You may need to restart your shell or run: source ~/.cargo/env"
