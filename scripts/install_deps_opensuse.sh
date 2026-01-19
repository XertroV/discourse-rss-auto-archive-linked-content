#!/usr/bin/env bash
# Install dependencies for discourse-link-archiver on openSUSE
set -euo pipefail

echo "Installing dependencies for discourse-link-archiver (openSUSE)..."

# Install system packages
sudo zypper install -y \
    gcc \
    make \
    pkg-config \
    libopenssl-devel \
    sqlite3-devel \
    ffmpeg \
    zstd \
    python3 \
    python3-pip

# Install yt-dlp and gallery-dl via pip
pip3 install --user yt-dlp gallery-dl

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
echo "  ffmpeg: $(ffmpeg -version | head -1)"
echo "  yt-dlp: $(yt-dlp --version 2>/dev/null || echo 'not in PATH - check ~/.local/bin')"
echo "  gallery-dl: $(gallery-dl --version 2>/dev/null || echo 'not in PATH - check ~/.local/bin')"
echo "  zstd: $(zstd --version | head -1)"
echo ""
echo "You may need to restart your shell or run: source ~/.cargo/env"
echo "Ensure ~/.local/bin is in your PATH for yt-dlp and gallery-dl"
