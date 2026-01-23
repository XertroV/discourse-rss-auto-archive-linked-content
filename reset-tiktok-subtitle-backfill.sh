#!/bin/bash
# Reset TikTok subtitle backfill markers to allow re-processing

set -e

echo "Resetting TikTok subtitle backfill markers..."

# Path to the database (adjust if needed)
DB_PATH="${DATABASE_PATH:-/app/data/archive.sqlite}"

# Delete subtitle_backfill_attempted markers
echo "Deleting backfill markers..."
MARKERS_DELETED=$(sqlite3 "$DB_PATH" "
DELETE FROM archive_artifacts
WHERE kind = 'subtitle_backfill_attempted'
  AND archive_id IN (
    SELECT a.id FROM archives a
    JOIN links l ON a.link_id = l.id
    WHERE l.domain LIKE '%tiktok%'
      AND a.content_type = 'video'
  );
SELECT changes();
")

echo "Deleted $MARKERS_DELETED backfill markers"

# # Delete JSON subtitle artifacts (will be replaced with VTT)
# echo "Deleting JSON subtitle artifacts..."
# JSON_DELETED=$(sqlite3 "$DB_PATH" "
# DELETE FROM archive_artifacts
# WHERE kind = 'subtitles'
#   AND s3_key LIKE '%.json'
#   AND archive_id IN (
#     SELECT a.id FROM archives a
#     JOIN links l ON a.link_id = l.id
#     WHERE l.domain LIKE '%tiktok%'
#   );
# SELECT changes();
# ")

# echo "Deleted $JSON_DELETED JSON subtitle artifacts"

echo ""
echo "Reset complete!"
echo "Markers deleted: $MARKERS_DELETED"
# echo "JSON artifacts deleted: $JSON_DELETED"
echo ""
echo "Now restart the archiver to trigger backfill:"
echo "  docker compose restart archiver"
