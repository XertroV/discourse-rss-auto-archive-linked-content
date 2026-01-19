#!/usr/bin/env bash
set -euo pipefail

# Run from the repo root (directory containing this script)
cd "$(dirname "$0")"

export DOCKER_BUILDKIT=1

docker compose build --pull archiver
