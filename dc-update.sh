#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

git pull --ff-only

./dc-rebuild.sh
./dc-restart.sh

# clean up dangling images & docker usage
docker image prune -f
docker buildx prune --filter "until=12h" -f
docker builder prune -a --filter until=12h -f
