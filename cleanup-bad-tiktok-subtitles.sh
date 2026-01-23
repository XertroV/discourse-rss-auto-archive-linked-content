#!/bin/bash
# Clean up TikTok subtitles with wrong format/naming and reset for re-download

set -e

echo "Cleaning up bad TikTok subtitle artifacts..."

# Path to database
DB_PATH="${DATABASE_PATH:-/app/data/archive.sqlite}"

# Find and delete subtitle artifacts with old naming pattern (tiktok_subtitles_*)
echo "Deleting subtitle artifacts with old naming pattern (tiktok_subtitles_*)..."
OLD_NAME_DELETED=$(docker compose exec -T archiver sqlite3 "$DB_PATH" "
DELETE FROM archive_artifacts
WHERE kind = 'subtitles'
  AND s3_key LIKE '%/subtitles/tiktok_subtitles_%'
  AND archive_id IN (
    SELECT a.id FROM archives a
    JOIN links l ON a.link_id = l.id
    WHERE l.domain LIKE '%tiktok%'
  );
SELECT changes();
")

echo "Deleted $OLD_NAME_DELETED subtitle artifacts with old naming pattern"

# Delete any JSON subtitle artifacts (just in case)
echo "Deleting JSON subtitle artifacts..."
JSON_DELETED=$(docker compose exec -T archiver sqlite3 "$DB_PATH" "
DELETE FROM archive_artifacts
WHERE kind = 'subtitles'
  AND s3_key LIKE '%.json'
  AND archive_id IN (
    SELECT a.id FROM archives a
    JOIN links l ON a.link_id = l.id
    WHERE l.domain LIKE '%tiktok%'
  );
SELECT changes();
")

echo "Deleted $JSON_DELETED JSON subtitle artifacts"

# Get list of affected archive IDs for transcript cleanup
echo "Finding archives that need transcript cleanup..."
AFFECTED_ARCHIVES=$(docker compose exec -T archiver sqlite3 "$DB_PATH" "
SELECT DISTINCT a.id
FROM archives a
JOIN links l ON a.link_id = l.id
WHERE l.domain LIKE '%tiktok%'
  AND a.content_type = 'video'
  AND NOT EXISTS (
    SELECT 1 FROM archive_artifacts aa
    WHERE aa.archive_id = a.id
      AND aa.kind = 'subtitles'
      AND aa.s3_key LIKE '%/subtitles/tiktok.%'
  )
  AND EXISTS (
    SELECT 1 FROM archive_artifacts marker
    WHERE marker.archive_id = a.id
      AND marker.kind = 'subtitle_backfill_attempted'
  );
")

# Count affected archives
AFFECTED_COUNT=$(echo "$AFFECTED_ARCHIVES" | grep -c . || echo "0")
echo "Found $AFFECTED_COUNT archives with bad/missing subtitles"

# Delete transcripts for affected archives (they're based on bad data)
echo "Deleting transcripts for affected archives..."
TRANSCRIPT_DELETED=$(docker compose exec -T archiver sqlite3 "$DB_PATH" "
DELETE FROM archive_artifacts
WHERE kind = 'transcript'
  AND archive_id IN (
    SELECT DISTINCT a.id
    FROM archives a
    JOIN links l ON a.link_id = l.id
    WHERE l.domain LIKE '%tiktok%'
      AND a.content_type = 'video'
      AND NOT EXISTS (
        SELECT 1 FROM archive_artifacts aa
        WHERE aa.archive_id = a.id
          AND aa.kind = 'subtitles'
          AND aa.s3_key LIKE '%/subtitles/tiktok.%'
      )
  );
SELECT changes();
")

echo "Deleted $TRANSCRIPT_DELETED transcript artifacts"

# Clear transcript_text for affected archives
echo "Clearing transcript_text for affected archives..."
TRANSCRIPT_TEXT_CLEARED=$(docker compose exec -T archiver sqlite3 "$DB_PATH" "
UPDATE archives
SET transcript_text = NULL
WHERE id IN (
  SELECT DISTINCT a.id
  FROM archives a
  JOIN links l ON a.link_id = l.id
  WHERE l.domain LIKE '%tiktok%'
    AND a.content_type = 'video'
    AND NOT EXISTS (
      SELECT 1 FROM archive_artifacts aa
      WHERE aa.archive_id = a.id
        AND aa.kind = 'subtitles'
        AND aa.s3_key LIKE '%/subtitles/tiktok.%'
    )
);
SELECT changes();
")

echo "Cleared transcript_text for $TRANSCRIPT_TEXT_CLEARED archives"

# Delete backfill markers for affected archives
echo "Deleting backfill markers for affected archives..."
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
        SELECT 1 FROM archive_artifacts aa
        WHERE aa.archive_id = a.id
          AND aa.kind = 'subtitles'
          AND aa.s3_key LIKE '%/subtitles/tiktok.%'
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
echo "  - Transcripts deleted: $TRANSCRIPT_DELETED"
echo "  - Transcript text cleared: $TRANSCRIPT_TEXT_CLEARED"
echo "  - Backfill markers deleted: $MARKERS_DELETED"
echo "  - Archives ready for re-processing: $AFFECTED_COUNT"
echo ""
echo "Now restart the archiver to trigger backfill:"
echo "  docker compose restart archiver"
