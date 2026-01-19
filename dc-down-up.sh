#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# Clean restart: recreates containers but keeps volumes.
docker compose down --remove-orphans

docker compose up -d --remove-orphans

docker compose ps
