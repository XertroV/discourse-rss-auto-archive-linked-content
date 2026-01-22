#!/usr/bin/env bash
set -euo pipefail

# Copy production database from server for local development
cd "$(dirname "$0")"

ssh cf-archiver.xk.io "discourse-rss-auto-archive-linked-content/svr-copy-db.sh"
scp cf-archiver.xk.io:discourse-rss-auto-archive-linked-content/archive.sqlite ./archive.sqlite
