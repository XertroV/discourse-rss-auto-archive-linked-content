#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# Start only archiver service (MinIO not needed in production with R2)
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d --remove-orphans archiver
docker compose -f docker-compose.yml -f docker-compose.prod.yml ps
