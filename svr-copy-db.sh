#!/usr/bin/env bash
set -euo pipefail

# Extract database from Docker container (run on server)
cd "$(dirname "$0")"

n=discourse-rss-auto-archive-linked-content-archiver-1
docker cp "$n":/app/data/archive.sqlite ./archive.sqlite
