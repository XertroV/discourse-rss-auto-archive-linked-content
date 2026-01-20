#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# Recreate containers without rebuilding images.
# (Use after updating .env / compose config, or after a separate build.)
# Start only archiver service (MinIO not needed in production with R2)
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d --force-recreate --no-build --remove-orphans archiver
docker compose -f docker-compose.yml -f docker-compose.prod.yml ps
