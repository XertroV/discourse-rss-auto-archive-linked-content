#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# Load shared functions
source ./dc-lib.sh

git pull --ff-only

# Check if container image is older than 1 week and force rebuild if needed
check_image_age_and_set_rebuild_flag

./dc-rebuild.sh
./dc-restart.sh

./dc-prune-safe.sh
