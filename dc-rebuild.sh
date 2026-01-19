#!/usr/bin/env bash
set -euo pipefail

# Run from the repo root (directory containing this script)
cd "$(dirname "$0")"

export DOCKER_BUILDKIT=1

# Optional: speed up rebuilds during iteration:
#   CARGO_PROFILE=release-fast ./dc-rebuild.sh

docker compose build --pull archiver
