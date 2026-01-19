use anyhow::{Context, Result};
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde_json::json;
use std::io::Cursor;
use std::net::SocketAddr;
use tracing::{error, info, warn};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use super::AppState;
use crate::db::{
    count_exports_from_ip_last_hour, get_archives_with_artifacts_for_domain, insert_export,
};

const MAX_EXPORT_SIZE_BYTES: i64 = 2 * 1024 * 1024 * 1024; // 2 GB
const MAX_VIDEO_SIZE_BYTES: i64 = 50 * 1024 * 1024; // 50 MB
const EXPORTS_PER_HOUR: i64 = 1;

/// Handler for bulk export route (GET /export/{site}).
///
/// Creates a ZIP archive containing all archives for a specific site,
/// excluding large video files (>50MB). Includes a metadata.json manifest.
///
/// Rate limited to 1 export per hour per IP address.
pub async fn export_site(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(site): Path<String>,
) -> Response {
    let client_ip = addr.ip().to_string();

    // Check if exports are enabled (check config if we add a setting)
    // For now, exports are always enabled

    // Rate limit check
    match count_exports_from_ip_last_hour(state.db.pool(), &client_ip).await {
        Ok(count) => {
            if count >= EXPORTS_PER_HOUR {
                warn!(
                    client_ip = %client_ip,
                    site = %site,
                    "Export rate limit exceeded"
                );
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    format!("Rate limit exceeded. Maximum {EXPORTS_PER_HOUR} export per hour."),
                )
                    .into_response();
            }
        }
        Err(e) => {
            error!(error = ?e, "Failed to check export rate limit");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal error").into_response();
        }
    }

    // Fetch archives with artifacts for the domain
    let archives_with_artifacts =
        match get_archives_with_artifacts_for_domain(state.db.pool(), &site).await {
            Ok(data) => data,
            Err(e) => {
                error!(error = ?e, site = %site, "Failed to fetch archives for export");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to fetch archives",
                )
                    .into_response();
            }
        };

    if archives_with_artifacts.is_empty() {
        return (
            StatusCode::NOT_FOUND,
            format!("No archives found for site: {site}"),
        )
            .into_response();
    }

    // Generate ZIP file in memory (using spawn_blocking for CPU-intensive work)
    match generate_export_zip(&state, &site, archives_with_artifacts).await {
        Ok((zip_bytes, archive_count, total_size)) => {
            // Record the export
            if let Err(e) = insert_export(
                state.db.pool(),
                &site,
                &client_ip,
                archive_count,
                total_size,
            )
            .await
            {
                error!(error = ?e, "Failed to record export");
                // Don't fail the export if we can't record it
            }

            info!(
                client_ip = %client_ip,
                site = %site,
                archive_count = archive_count,
                total_size_mb = total_size / (1024 * 1024),
                "Export completed"
            );

            // Return ZIP as download
            let filename = format!("{site}-archives.zip");
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/zip"),
                    (
                        header::CONTENT_DISPOSITION,
                        &format!("attachment; filename=\"{filename}\""),
                    ),
                    (header::CONTENT_LENGTH, &zip_bytes.len().to_string()),
                ],
                zip_bytes,
            )
                .into_response()
        }
        Err(e) => {
            error!(error = ?e, site = %site, "Failed to generate export ZIP");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to generate export",
            )
                .into_response()
        }
    }
}

/// Generate export ZIP file containing archives and metadata.
///
/// Returns (zip_bytes, archive_count, total_size_bytes).
async fn generate_export_zip(
    state: &AppState,
    site: &str,
    archives_with_artifacts: Vec<(
        crate::db::Archive,
        crate::db::Link,
        Vec<crate::db::ArchiveArtifact>,
    )>,
) -> Result<(Vec<u8>, i64, i64)> {
    let s3 = state.s3.clone();
    let site_owned = site.to_string();

    // Spawn blocking task for ZIP creation (CPU-intensive)
    tokio::task::spawn_blocking(move || {
        let mut zip_buffer = Vec::new();
        let cursor = Cursor::new(&mut zip_buffer);
        let mut zip = ZipWriter::new(cursor);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        let mut metadata = Vec::new();
        let mut total_size = 0i64;
        let mut included_count = 0i64;
        let mut current_export_size = 0i64;

        for (archive, link, artifacts) in archives_with_artifacts {
            // Build metadata for this archive
            let mut archive_metadata = json!({
                "archive_id": archive.id,
                "url": link.original_url,
                "normalized_url": link.normalized_url,
                "domain": link.domain,
                "title": archive.content_title,
                "author": archive.content_author,
                "content_type": archive.content_type,
                "archived_at": archive.archived_at,
                "is_nsfw": archive.is_nsfw,
                "wayback_url": archive.wayback_url,
                "archive_today_url": archive.archive_today_url,
                "ipfs_cid": archive.ipfs_cid,
                "artifacts": []
            });

            let archive_artifacts = archive_metadata["artifacts"].as_array_mut().unwrap();

            // Process artifacts
            for artifact in artifacts {
                let size = artifact.size_bytes.unwrap_or(0);

                // Skip large video files
                if artifact.kind == "video" && size > MAX_VIDEO_SIZE_BYTES {
                    warn!(
                        archive_id = archive.id,
                        size_mb = size / (1024 * 1024),
                        "Skipping large video file"
                    );
                    archive_artifacts.push(json!({
                        "kind": artifact.kind,
                        "filename": extract_filename(&artifact.s3_key),
                        "size_bytes": size,
                        "skipped": true,
                        "reason": "File too large (>50MB)"
                    }));
                    continue;
                }

                // Check if adding this file would exceed export size limit
                if current_export_size + size > MAX_EXPORT_SIZE_BYTES {
                    warn!(
                        archive_id = archive.id,
                        current_size_mb = current_export_size / (1024 * 1024),
                        "Export size limit reached, stopping"
                    );
                    archive_artifacts.push(json!({
                        "kind": artifact.kind,
                        "filename": extract_filename(&artifact.s3_key),
                        "size_bytes": size,
                        "skipped": true,
                        "reason": "Export size limit reached (2GB)"
                    }));
                    continue;
                }

                // Download file from S3 (blocking operation)
                let file_data = match tokio::runtime::Handle::current()
                    .block_on(s3.download_file(&artifact.s3_key))
                {
                    Ok((bytes, _content_type)) => bytes,
                    Err(e) => {
                        warn!(
                            error = ?e,
                            s3_key = %artifact.s3_key,
                            "Failed to download artifact from S3"
                        );
                        archive_artifacts.push(json!({
                            "kind": artifact.kind,
                            "filename": extract_filename(&artifact.s3_key),
                            "size_bytes": size,
                            "skipped": true,
                            "reason": format!("Download failed: {e}")
                        }));
                        continue;
                    }
                };

                // Add file to ZIP
                let zip_path = format!(
                    "{}/archive-{}/{}",
                    site_owned,
                    archive.id,
                    extract_filename(&artifact.s3_key)
                );

                zip.start_file(&zip_path, options)
                    .context("Failed to start ZIP entry")?;
                std::io::Write::write_all(&mut zip, &file_data)
                    .context("Failed to write file data to ZIP")?;

                current_export_size += size;
                total_size += size;

                archive_artifacts.push(json!({
                    "kind": artifact.kind,
                    "filename": extract_filename(&artifact.s3_key),
                    "size_bytes": size,
                    "content_type": artifact.content_type,
                    "sha256": artifact.sha256,
                    "zip_path": zip_path
                }));
            }

            metadata.push(archive_metadata);
            included_count += 1;
        }

        // Add metadata.json to ZIP
        let manifest = json!({
            "export_metadata": {
                "site": site_owned,
                "archive_count": included_count,
                "total_size_bytes": total_size,
                "max_video_size_bytes": MAX_VIDEO_SIZE_BYTES,
                "exported_at": Utc::now().to_rfc3339()
            },
            "archives": metadata
        });

        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        zip.start_file("metadata.json", options)
            .context("Failed to start metadata entry")?;
        std::io::Write::write_all(&mut zip, manifest_json.as_bytes())
            .context("Failed to write metadata JSON")?;

        // Finalize ZIP
        zip.finish().context("Failed to finish ZIP file")?;

        Ok((zip_buffer, included_count, total_size))
    })
    .await
    .context("ZIP generation task panicked")?
}

/// Extract filename from S3 key.
fn extract_filename(s3_key: &str) -> String {
    s3_key.rsplit('/').next().unwrap_or(s3_key).to_string()
}
