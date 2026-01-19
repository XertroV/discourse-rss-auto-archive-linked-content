#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# Reset the SQLite DB used by the archiver while preserving TLS cert cache.
#
# Deletes:
#   /app/data/archive.sqlite
#   /app/data/archive.sqlite-* (wal/shm/journal)
#
# Preserves:
#   /app/data/acme_cache (Let's Encrypt cert cache)
#   everything else in the volume

# Stop only the archiver to avoid any open SQLite handles.
docker compose stop archiver >/dev/null || true

# Find the archiver data volume name created by docker compose.
# Typically looks like: <project>_archiver-data
mapfile -t volumes < <(docker volume ls --format '{{.Name}}' | grep -E '(^|_)archiver-data$' || true)

if [[ ${#volumes[@]} -eq 0 ]]; then
  echo "ERROR: Could not find a docker volume ending with '_archiver-data'." >&2
  echo "Run: docker volume ls | grep archiver-data" >&2
  exit 1
fi

if [[ ${#volumes[@]} -gt 1 ]]; then
  echo "ERROR: Multiple '*_archiver-data' volumes found:" >&2
  printf '  - %s\n' "${volumes[@]}" >&2
  echo "Set ARCHIVER_DATA_VOLUME to choose one, e.g.:" >&2
  echo "  ARCHIVER_DATA_VOLUME=${volumes[0]} ./dc-reset-db.sh" >&2
  exit 1
fi

data_volume="${ARCHIVER_DATA_VOLUME:-${volumes[0]}}"

echo "Using data volume: ${data_volume}"

# Show what we're about to delete (best-effort)
docker run --rm -v "${data_volume}:/data" alpine:3.20 \
  sh -lc 'ls -la /data; ls -la /data/acme_cache || true; ls -la /data/archive.sqlite* 2>/dev/null || true'

# Delete only the sqlite db and sidecar files.
docker run --rm -v "${data_volume}:/data" alpine:3.20 \
  sh -lc 'rm -f /data/archive.sqlite /data/archive.sqlite-*; echo "Deleted /data/archive.sqlite and /data/archive.sqlite-*"; ls -la /data; ls -la /data/acme_cache || true'

# Start archiver back up.
docker compose up -d --no-deps archiver

docker compose ps
