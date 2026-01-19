#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# Recreate containers without rebuilding images.
# (Use after updating .env / compose config, or after a separate build.)
docker compose up -d --force-recreate --no-build --remove-orphans
docker compose ps
