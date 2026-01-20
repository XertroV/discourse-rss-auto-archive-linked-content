#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# Clean restart: recreates containers but keeps volumes.
docker compose -f docker-compose.yml -f docker-compose.prod.yml down --remove-orphans

docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d --remove-orphans

docker compose -f docker-compose.yml -f docker-compose.prod.yml ps
