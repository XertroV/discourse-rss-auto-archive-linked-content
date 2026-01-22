#!/usr/bin/env bash
# Shared library functions for Docker Compose scripts

# Check if the Docker image is older than 1 week and set FORCE_REBUILD=1 if so
# This ensures yt-dlp and other dependencies stay current
check_image_age_and_set_rebuild_flag() {
    local IMAGE_NAME="discourse-rss-auto-archive-linked-content-archiver"
    local ONE_WEEK_AGO=$(date -d '7 days ago' +%s 2>/dev/null || date -v-7d +%s 2>/dev/null || echo "0")

    if docker image inspect "$IMAGE_NAME" >/dev/null 2>&1; then
        local IMAGE_CREATED=$(docker image inspect "$IMAGE_NAME" --format '{{.Created}}')
        local IMAGE_TIMESTAMP=$(date -d "$IMAGE_CREATED" +%s 2>/dev/null || date -j -f "%Y-%m-%dT%H:%M:%S" "$IMAGE_CREATED" +%s 2>/dev/null || echo "0")

        if [ "$IMAGE_TIMESTAMP" -lt "$ONE_WEEK_AGO" ]; then
            echo "Container image is older than 1 week (created: $IMAGE_CREATED)"
            echo "Forcing rebuild with --no-cache to update yt-dlp and other dependencies..."
            export FORCE_REBUILD=1
        fi
    fi
}

# Get build arguments for docker compose build based on FORCE_REBUILD flag
get_build_args() {
    local BUILD_ARGS="--pull"
    if [ "${FORCE_REBUILD:-0}" = "1" ]; then
        echo "Using --no-cache to ensure fresh dependencies"
        BUILD_ARGS="$BUILD_ARGS --no-cache"
    fi
    echo "$BUILD_ARGS"
}
