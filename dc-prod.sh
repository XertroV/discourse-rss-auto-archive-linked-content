#!/usr/bin/env bash
# Production docker-compose wrapper
# Uses both docker-compose.yml and docker-compose.prod.yml
set -euo pipefail

cd "$(dirname "$0")"

exec docker compose -f docker-compose.yml -f docker-compose.prod.yml "$@"
