#!/bin/sh
set -eu

# This image normally runs the app as the non-root `archiver` user.
# When using Docker named volumes, /app/data is often root-owned by default,
# which makes SQLite open the DB in read-only mode for the `archiver` user.
# We fix ownership on startup (as root) and then drop privileges.

APP_BIN=${APP_BIN:-/usr/local/bin/discourse-link-archiver}

if [ "$(id -u)" = "0" ]; then
  # Ensure directories exist (best-effort)
  mkdir -p /app/data /app/data/tmp /app/data/acme_cache /app/.cache || true

  # Make sure the archiver user can write to its working directories.
  chown -R archiver:archiver /app/data /app/.cache || true

  # Run the actual command as the archiver user.
  # Background-update yt-dlp and gallery-dl on startup so they stay fresh between rebuilds.
  if [ "$#" -gt 0 ]; then
    exec su -s /bin/sh archiver -c "yt-dlp -U >/dev/null 2>&1 & gallery-dl -U >/dev/null 2>&1 & $*"
  fi
  exec su -s /bin/sh archiver -c "yt-dlp -U >/dev/null 2>&1 & gallery-dl -U >/dev/null 2>&1 & $APP_BIN"
fi

# Already non-root.
yt-dlp -U >/dev/null 2>&1 &
gallery-dl -U >/dev/null 2>&1 &
if [ "$#" -gt 0 ]; then
  exec "$@"
fi
exec "$APP_BIN"
