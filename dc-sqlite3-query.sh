#!/bin/bash
# dc-sqlite3-query.sh - Run SQLite queries in Docker Compose archiver container
#
# Usage:
#   ./dc-sqlite3-query.sh                          # Interactive shell
#   ./dc-sqlite3-query.sh "SELECT * FROM links;"   # Run query
#   ./dc-sqlite3-query.sh --tables                 # List all tables
#   ./dc-sqlite3-query.sh --schema [TABLE]         # Show schema
#   ./dc-sqlite3-query.sh --file query.sql         # Run queries from file
#   ./dc-sqlite3-query.sh --stats                  # Show database stats
#   ./dc-sqlite3-query.sh --backup [FILE]          # Create backup

set -euo pipefail

CONTAINER_NAME="archiver"
DB_PATH="/app/data/archive.sqlite"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Helper function to run sqlite3 in container
run_sqlite() {
    local query="$1"
    docker compose exec -T "$CONTAINER_NAME" sqlite3 "$DB_PATH" "$query"
}

# Helper function to run sqlite3 interactively
run_sqlite_interactive() {
    docker compose exec "$CONTAINER_NAME" sqlite3 "$DB_PATH"
}

# Helper function to check if container is running
check_container() {
    if ! docker compose ps "$CONTAINER_NAME" | grep -q "Up"; then
        echo -e "${RED}Error: Container '$CONTAINER_NAME' is not running${NC}" >&2
        echo "Start it with: docker compose up -d" >&2
        exit 1
    fi
}

# Parse arguments
case "${1:-}" in
    --help|-h)
        cat <<EOF
Usage: $0 [COMMAND|QUERY]

Commands:
  (no args)              Open interactive SQLite shell
  --tables, -t           List all tables
  --schema, -s [TABLE]   Show schema (optionally for specific table)
  --stats                Show database statistics
  --backup, -b [FILE]    Create database backup
  --file, -f FILE        Run queries from SQL file
  --recent-archives, -r [N]  Show N recent archives (default: 10)
  --failed-archives      Show failed archives
  --help, -h             Show this help

Query:
  "SELECT ..."          Run a specific SQL query

Examples:
  $0                                    # Interactive mode
  $0 --tables                           # List tables
  $0 --schema links                     # Show links table schema
  $0 --stats                            # Database statistics
  $0 "SELECT * FROM posts LIMIT 5;"    # Run custom query
  $0 --file queries.sql                # Run queries from file
  $0 --recent-archives 20              # Show 20 recent archives

Database Path: $DB_PATH
Container: $CONTAINER_NAME
EOF
        exit 0
        ;;
esac

# Check if container is running (after --help check)
check_container

# Execute commands
case "${1:-}" in
    "")
        # No arguments - interactive mode
        echo -e "${GREEN}Opening SQLite interactive shell...${NC}"
        echo -e "${YELLOW}Tip: Use .help for SQLite commands, .tables to list tables${NC}"
        run_sqlite_interactive
        ;;

    --tables|-t)
        # List all tables
        echo -e "${GREEN}Tables in database:${NC}"
        run_sqlite ".tables"
        ;;

    --schema|-s)
        # Show schema for table or entire database
        if [ -n "${2:-}" ]; then
            echo -e "${GREEN}Schema for table '$2':${NC}"
            run_sqlite ".schema $2"
        else
            echo -e "${GREEN}Complete database schema:${NC}"
            run_sqlite ".schema"
        fi
        ;;

    --stats)
        # Show database statistics
        echo -e "${GREEN}Database Statistics:${NC}"
        run_sqlite "
SELECT 'Posts' as table_name, COUNT(*) as count FROM posts
UNION ALL
SELECT 'Links', COUNT(*) FROM links
UNION ALL
SELECT 'Archives', COUNT(*) FROM archives
UNION ALL
SELECT 'Link Occurrences', COUNT(*) FROM link_occurrences
UNION ALL
SELECT 'Thread Archive Jobs', COUNT(*) FROM thread_archive_jobs;

SELECT '---' as separator;

SELECT 'Archive Status' as metric, status, COUNT(*) as count
FROM archives
GROUP BY status
ORDER BY status;

SELECT '---' as separator;

SELECT 'Database Size' as metric,
       ROUND(page_count * page_size / 1024.0 / 1024.0, 2) || ' MB' as value
FROM pragma_page_count(), pragma_page_size();
"
        ;;

    --backup|-b)
        # Create backup
        BACKUP_FILE="${2:-backup_$(date +%Y%m%d_%H%M%S).sqlite}"
        echo -e "${GREEN}Creating backup to: $BACKUP_FILE${NC}"
        docker compose exec -T "$CONTAINER_NAME" sqlite3 "$DB_PATH" ".backup /app/data/$BACKUP_FILE"
        echo -e "${GREEN}Backup created successfully${NC}"
        echo "To copy to host: docker compose cp $CONTAINER_NAME:/app/data/$BACKUP_FILE ./$BACKUP_FILE"
        ;;

    --file|-f)
        # Run queries from file
        if [ -z "${2:-}" ]; then
            echo -e "${RED}Error: --file requires a filename${NC}" >&2
            exit 1
        fi
        if [ ! -f "$2" ]; then
            echo -e "${RED}Error: File '$2' not found${NC}" >&2
            exit 1
        fi
        echo -e "${GREEN}Running queries from: $2${NC}"
        cat "$2" | docker compose exec -T "$CONTAINER_NAME" sqlite3 "$DB_PATH"
        ;;

    --recent-archives|-r)
        # Show recent archives
        LIMIT="${2:-10}"
        echo -e "${GREEN}Recent $LIMIT archives:${NC}"
        run_sqlite "
SELECT
    a.id,
    substr(l.original_url, 1, 60) as url,
    a.status,
    a.created_at
FROM archives a
JOIN links l ON a.link_id = l.id
ORDER BY a.created_at DESC
LIMIT $LIMIT;
"
        ;;

    --failed-archives)
        # Show failed archives
        echo -e "${GREEN}Failed archives:${NC}"
        run_sqlite "
SELECT
    a.id,
    l.original_url,
    a.error_message,
    a.updated_at
FROM archives a
JOIN links l ON a.link_id = l.id
WHERE a.status = 'failed'
ORDER BY a.updated_at DESC
LIMIT 20;
"
        ;;

    *)
        # Treat as SQL query
        echo -e "${GREEN}Running query...${NC}"
        run_sqlite "$1"
        ;;
esac
