#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

git pull --ff-only

./dc-rebuild.sh
./dc-restart.sh

./dc-prune-safe.sh
