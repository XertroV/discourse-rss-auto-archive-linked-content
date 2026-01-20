#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# Usage:
#   ./dc-logs.sh            # all services
#   ./dc-logs.sh archiver   # one service
service_name="${1:-}"

if [[ -n "$service_name" ]]; then
  docker compose -f docker-compose.yml -f docker-compose.prod.yml logs -f --tail=200 "$service_name"
else
  docker compose -f docker-compose.yml -f docker-compose.prod.yml logs -f --tail=200
fi
