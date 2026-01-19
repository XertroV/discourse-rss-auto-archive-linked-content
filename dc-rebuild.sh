#!/usr/bin/env bash
set -euo pipefail

# Run from the repo root (directory containing this script)
cd "$(dirname "$0")"

export DOCKER_BUILDKIT=1

# Optional: speed up rebuilds during iteration:
#   CARGO_PROFILE=dev ./dc-rebuild.sh           # Fastest builds (no optimization)
#   CARGO_PROFILE=release-fast ./dc-rebuild.sh  # Faster builds (less optimization)

docker compose build --pull archiver
