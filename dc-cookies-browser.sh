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
echo "Starting cookie-browser (will stop when you Ctrl+C)..."
echo "Access UI securely via SSH tunnel:"
echo "  ssh -L 7900:127.0.0.1:7900 root@YOUR_SERVER"
echo "Then open: http://127.0.0.1:7900 (password: secret)"
echo

# Start detached so we can run an auto-launch command inside the container.
docker compose --profile manual up -d cookie-browser

echo "Waiting for cookie-browser to be ready..."
for _ in {1..60}; do
	if docker compose --profile manual exec -T cookie-browser bash -lc 'echo ok' >/dev/null 2>&1; then
		break
	fi
	sleep 0.5
done

echo "Auto-launching Chromium in the noVNC desktop..."
docker compose --profile manual exec -T cookie-browser bash -lc '
	set -euo pipefail
	url="about:blank"
	if command -v chromium >/dev/null 2>&1; then
		nohup chromium --no-sandbox "$url" >/tmp/chromium-autostart.log 2>&1 &
	elif command -v chromium-browser >/dev/null 2>&1; then
		nohup chromium-browser --no-sandbox "$url" >/tmp/chromium-autostart.log 2>&1 &
	elif command -v google-chrome >/dev/null 2>&1; then
		nohup google-chrome --no-sandbox "$url" >/tmp/chromium-autostart.log 2>&1 &
	else
		echo "No Chromium/Chrome binary found in container" >&2
		exit 1
	fi
'

echo
echo "cookie-browser is running. Open noVNC in your browser now."
echo "Press Ctrl+C here when you're done."
echo

while true; do
	sleep 3600
done
