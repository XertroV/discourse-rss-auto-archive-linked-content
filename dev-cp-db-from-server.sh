#!/usr/bin/env bash
set -euo pipefail

# Copy production database from server for local development
cd "$(dirname "$0")"

ssh cf-archive.xk.io "./svr-copy-db.sh"
scp cf-archive.xk.io:discourse-rss-auto-archive-linked-content/archive.sqlite ./archive.sqlite
