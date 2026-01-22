#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# Start development environment with static files mounted from host
# CSS/JS changes are immediately visible without rebuilding
docker compose -f docker-compose.yml -f docker-compose.dev.yml up -d

docker compose -f docker-compose.yml -f docker-compose.dev.yml ps
