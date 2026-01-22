#!/usr/bin/env bash
set -euo pipefail

# Run from the repo root (directory containing this script)
cd "$(dirname "$0")"

# Load shared functions
source ./dc-lib.sh

export DOCKER_BUILDKIT=1

# Optional: speed up rebuilds during iteration:
#   CARGO_PROFILE=dev ./dc-rebuild.sh           # Fastest builds (no optimization)
#   CARGO_PROFILE=release-fast ./dc-rebuild.sh  # Faster builds (less optimization)

# Add --no-cache if FORCE_REBUILD is set (e.g., when image is >1 week old)
BUILD_ARGS=$(get_build_args)

docker compose -f docker-compose.yml -f docker-compose.prod.yml build $BUILD_ARGS archiver
