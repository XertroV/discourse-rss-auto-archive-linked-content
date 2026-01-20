#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# Clean restart: recreates containers but keeps volumes.
# Stop only archiver (avoid parsing issues with undefined minio in prod)
docker compose -f docker-compose.yml -f docker-compose.prod.yml stop archiver || true
docker compose -f docker-compose.yml -f docker-compose.prod.yml rm -f archiver || true

# Start only archiver service (MinIO not needed in production with R2)
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d --remove-orphans archiver

docker compose -f docker-compose.yml -f docker-compose.prod.yml ps
