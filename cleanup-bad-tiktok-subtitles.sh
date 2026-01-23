#!/bin/bash
# Clean up TikTok subtitles with wrong format/naming and reset for re-download

set -e

echo "Cleaning up bad TikTok subtitle artifacts..."

# Path to database
DB_PATH="${DATABASE_PATH:-/app/data/archive.sqlite}"

# Only clean up subtitles for archives that DON'T have transcripts
# (if transcript exists, the subtitle was probably valid even with old naming)
echo "Deleting subtitle artifacts with old naming pattern (only for archives without transcripts)..."
OLD_NAME_DELETED=$(docker compose exec -T archiver sqlite3 "$DB_PATH" "
DELETE FROM archive_artifacts
WHERE kind = 'subtitles'
  AND s3_key LIKE '%/subtitles/tiktok_subtitles_%'
  AND archive_id IN (
    SELECT a.id FROM archives a
    JOIN links l ON a.link_id = l.id
    WHERE l.domain LIKE '%tiktok%'
      AND NOT EXISTS (
        SELECT 1 FROM archive_artifacts t
        WHERE t.archive_id = a.id AND t.kind = 'transcript'
      )
  );
SELECT changes();
")

echo "Deleted $OLD_NAME_DELETED subtitle artifacts with old naming pattern"

# Delete any JSON subtitle artifacts (only for archives without transcripts)
echo "Deleting JSON subtitle artifacts (archives without transcripts)..."
JSON_DELETED=$(docker compose exec -T archiver sqlite3 "$DB_PATH" "
DELETE FROM archive_artifacts
WHERE kind = 'subtitles'
  AND s3_key LIKE '%.json'
  AND archive_id IN (
    SELECT a.id FROM archives a
    JOIN links l ON a.link_id = l.id
    WHERE l.domain LIKE '%tiktok%'
      AND NOT EXISTS (
        SELECT 1 FROM archive_artifacts t
        WHERE t.archive_id = a.id AND t.kind = 'transcript'
      )
  );
SELECT changes();
")

echo "Deleted $JSON_DELETED JSON subtitle artifacts"

# Count affected archives (TikTok videos without transcripts that were attempted)
echo "Finding archives that need re-processing..."
AFFECTED_COUNT=$(docker compose exec -T archiver sqlite3 "$DB_PATH" "
SELECT COUNT(DISTINCT a.id)
FROM archives a
JOIN links l ON a.link_id = l.id
WHERE l.domain LIKE '%tiktok%'
  AND a.content_type = 'video'
  AND NOT EXISTS (
    SELECT 1 FROM archive_artifacts t
    WHERE t.archive_id = a.id AND t.kind = 'transcript'
  )
  AND EXISTS (
    SELECT 1 FROM archive_artifacts marker
    WHERE marker.archive_id = a.id
      AND marker.kind = 'subtitle_backfill_attempted'
  );
")

echo "Found $AFFECTED_COUNT archives without transcripts that need re-processing"

# Delete backfill markers for archives without transcripts
echo "Deleting backfill markers for archives without transcripts..."
MARKERS_DELETED=$(docker compose exec -T archiver sqlite3 "$DB_PATH" "
DELETE FROM archive_artifacts
WHERE kind = 'subtitle_backfill_attempted'
  AND archive_id IN (
    SELECT DISTINCT a.id
    FROM archives a
    JOIN links l ON a.link_id = l.id
    WHERE l.domain LIKE '%tiktok%'
      AND a.content_type = 'video'
      AND NOT EXISTS (
        SELECT 1 FROM archive_artifacts t
        WHERE t.archive_id = a.id AND t.kind = 'transcript'
      )
  );
SELECT changes();
")

echo "Deleted $MARKERS_DELETED backfill markers"

echo ""
echo "Cleanup complete!"
echo "Summary:"
echo "  - Old naming pattern subtitles deleted: $OLD_NAME_DELETED"
echo "  - JSON subtitles deleted: $JSON_DELETED"
echo "  - Backfill markers deleted: $MARKERS_DELETED"
echo "  - Archives ready for re-processing: $AFFECTED_COUNT"
echo ""
echo "Note: Archives with working transcripts were NOT touched."
echo "      Only archives without transcripts were reset for re-download."
echo ""
echo "Now restart the archiver to trigger backfill:"
echo "  docker compose restart archiver"
