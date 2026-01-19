#!/usr/bin/env bash
# Install dependencies for discourse-link-archiver on Arch Linux
set -euo pipefail

echo "Installing dependencies for discourse-link-archiver (Arch Linux)..."

# Install system packages
sudo pacman -Syu --noconfirm \
    base-devel \
    pkgconf \
    openssl \
    sqlite \
    ffmpeg \
    zstd \
    python \
    python-pip \
    yt-dlp \
    rust

# Install gallery-dl (from AUR or pip)
if command -v yay &> /dev/null; then
    yay -S --noconfirm gallery-dl
elif command -v paru &> /dev/null; then
    paru -S --noconfirm gallery-dl
else
    echo "Installing gallery-dl via pip (no AUR helper found)..."
    pip install --user gallery-dl
fi

echo ""
echo "Dependencies installed successfully!"
echo ""
echo "Installed versions:"
echo "  rustc: $(rustc --version)"
echo "  ffmpeg: $(ffmpeg -version | head -1)"
echo "  yt-dlp: $(yt-dlp --version)"
echo "  gallery-dl: $(gallery-dl --version 2>/dev/null || echo 'not in PATH - check ~/.local/bin')"
echo "  zstd: $(zstd --version | head -1)"
