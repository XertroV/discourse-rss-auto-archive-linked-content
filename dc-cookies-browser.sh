#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

# Start an interactive browser (noVNC) container to log into sites (Reddit/YouTube/etc)
# and export a cookies.txt into a shared docker volume.
#
# Access UI securely via SSH tunnel:
#   ssh -L 7900:127.0.0.1:7900 root@YOUR_SERVER
# Then open:
#   http://127.0.0.1:7900
#
# After logging in and downloading/exporting cookies.txt inside the browser container,
# copy it into the shared volume (mounted at /cookies):
#   docker compose exec cookie-browser bash -lc 'ls -la ~/Downloads'
#   docker compose exec cookie-browser bash -lc 'cp -f ~/Downloads/cookies.txt /cookies/cookies.txt'
#
# Then restart archiver:
#   ./dc-restart.sh

cleanup() {
	# Best-effort cleanup; don't fail the script if stop/rm fails.
	docker compose stop cookie-browser >/dev/null 2>&1 || true
	docker compose rm -f cookie-browser >/dev/null 2>&1 || true
}

trap cleanup EXIT INT TERM

echo
echo "Starting cookie-browser (will stop when you exit this script)..."
echo "Access UI securely via SSH tunnel:"
echo "  ssh -L 7900:127.0.0.1:7900 root@YOUR_SERVER"
echo "Then open: http://127.0.0.1:7900"
echo

# Run in the foreground so the browser stays active while this script runs.
# cookie-browser is in the `manual` compose profile so it doesn't start by default.
docker compose --profile manual up cookie-browser
