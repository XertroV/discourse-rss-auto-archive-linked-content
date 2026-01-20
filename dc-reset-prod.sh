#!/usr/bin/env bash
# Reset production database for a fresh start
set -euo pipefail

cd "$(dirname "$0")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}========================================${NC}"
echo -e "${YELLOW}Production Database Reset${NC}"
echo -e "${YELLOW}========================================${NC}"
echo ""
echo "This script will:"
echo "  1. Stop the archiver service"
echo "  2. Delete the SQLite database (preserving TLS certs)"
echo "  3. Restart the archiver service"
echo ""
echo -e "${RED}WARNING: This will delete all archive metadata!${NC}"
echo ""
read -p "Continue? (yes/no): " -r
echo
if [[ ! $REPLY =~ ^[Yy][Ee][Ss]$ ]]; then
    echo "Aborted."
    exit 1
fi

echo ""
echo -e "${GREEN}Step 1: Stopping archiver service...${NC}"
./dc-prod.sh stop archiver

echo ""
echo -e "${GREEN}Step 2: Finding data volume...${NC}"
mapfile -t volumes < <(docker volume ls --format '{{.Name}}' | grep -E '(^|_)archiver-data$' || true)

if [[ ${#volumes[@]} -eq 0 ]]; then
  echo -e "${RED}ERROR: Could not find archiver-data volume${NC}" >&2
  exit 1
fi

if [[ ${#volumes[@]} -gt 1 ]]; then
  echo -e "${RED}ERROR: Multiple archiver-data volumes found:${NC}" >&2
  printf '  - %s\n' "${volumes[@]}" >&2
  exit 1
fi

data_volume="${volumes[0]}"
echo "Using data volume: ${data_volume}"

echo ""
echo -e "${GREEN}Step 3: Current data volume contents:${NC}"
docker run --rm -v "${data_volume}:/data" alpine:3.20 \
  sh -c 'ls -lah /data 2>/dev/null || true'

echo ""
echo -e "${GREEN}Step 4: Deleting SQLite database...${NC}"
docker run --rm -v "${data_volume}:/data" alpine:3.20 \
  sh -c 'rm -f /data/archive.sqlite /data/archive.sqlite-*; echo "Deleted database files"; ls -lah /data 2>/dev/null || true'

echo ""
echo -e "${GREEN}Step 5: Starting archiver service...${NC}"
./dc-prod.sh up -d archiver

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}Reset Complete!${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "The archiver will create a fresh database on startup."
echo ""
echo "Check service status:"
echo "  ./dc-prod.sh ps"
echo ""
echo "View logs:"
echo "  ./dc-prod.sh logs -f archiver"
echo ""
echo "Check health:"
echo "  curl http://localhost:8080/healthz"
