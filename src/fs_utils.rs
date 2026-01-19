use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::debug;

/// Best-effort recursive directory copy.
///
/// Skips files that are unreadable (permission denied). Intended for copying browser
/// profiles mounted from shared volumes where some files may be restrictive.
pub async fn copy_dir_best_effort(src: &Path, dst: &Path, purpose: &str) -> Result<()> {
    // Async recursion is not allowed without boxing; use an explicit stack.
    let mut stack = vec![(src.to_path_buf(), dst.to_path_buf())];

    while let Some((src_dir, dst_dir)) = stack.pop() {
        tokio::fs::create_dir_all(&dst_dir).await.with_context(|| {
            format!(
                "Failed to create destination directory ({purpose}): {}",
                dst_dir.display()
            )
        })?;

        let mut entries = tokio::fs::read_dir(&src_dir).await.with_context(|| {
            format!(
                "Failed to read directory ({purpose}): {}",
                src_dir.display()
            )
        })?;

        while let Some(entry) = entries.next_entry().await? {
            let src_path = entry.path();
            let dst_path = dst_dir.join(entry.file_name());
            let file_type = entry.file_type().await?;

            if file_type.is_dir() {
                stack.push((src_path, dst_path));
                continue;
            }

            if file_type.is_file() {
                match tokio::fs::copy(&src_path, &dst_path).await {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                        debug!(
                            path = %src_path.display(),
                            purpose = %purpose,
                            "Skipping unreadable file during best-effort copy"
                        );
                    }
                    Err(e) => {
                        return Err(anyhow::Error::new(e)).context(format!(
                            "Failed to copy file ({purpose}): {}",
                            src_path.display()
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

/// A small helper to consistently join a child path.
#[allow(dead_code)]
fn join(dst_dir: &PathBuf, child: &std::ffi::OsStr) -> PathBuf {
    dst_dir.join(child)
}
