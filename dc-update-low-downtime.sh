#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# Low-downtime update:
# - Builds the new image first (no downtime)
# - Then recreates only the archiver container (brief restart to swap ports)
#
# Usage:
#   ./dc-update-low-downtime.sh

export DOCKER_BUILDKIT=1

# Update code (optional; comment out if you deploy via other means)
git pull --ff-only

# Build new image without touching running containers
docker compose -f docker-compose.yml -f docker-compose.prod.yml build --pull archiver

# Swap in the new container without restarting dependencies
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d --no-deps --force-recreate archiver

docker compose -f docker-compose.yml -f docker-compose.prod.yml ps
