//! Monolith wrapper for creating self-contained HTML archives.
//!
//! Monolith is a CLI tool that bundles a web page with all its resources
//! (CSS, images, fonts, JavaScript) into a single HTML file using data URIs.
//! This ensures the archived page renders correctly offline.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::{debug, warn};

/// Default timeout for monolith execution in seconds.
pub const DEFAULT_MONOLITH_TIMEOUT_SECS: u64 = 60;

/// Configuration for monolith HTML archiving.
#[derive(Debug, Clone)]
pub struct MonolithConfig {
    /// Whether monolith archiving is enabled.
    pub enabled: bool,
    /// Path to the monolith executable.
    pub path: String,
    /// Timeout for monolith execution.
    pub timeout: Duration,
    /// Whether to include JavaScript in the archive.
    pub include_js: bool,
}

impl Default for MonolithConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            path: "monolith".to_string(),
            timeout: Duration::from_secs(DEFAULT_MONOLITH_TIMEOUT_SECS),
            include_js: false,
        }
    }
}

/// Create a self-contained HTML file from a URL using monolith.
///
/// The output file will contain all CSS, images, and fonts embedded as data URIs,
/// making it viewable offline without any external dependencies.
///
/// # Arguments
///
/// * `url` - The URL to archive
/// * `output_path` - Where to save the self-contained HTML file
/// * `cookies_file` - Optional path to a cookies.txt file for authenticated requests
/// * `config` - Monolith configuration
///
/// # Errors
///
/// Returns an error if monolith fails to execute or times out.
pub async fn create_complete_html(
    url: &str,
    output_path: &Path,
    cookies_file: Option<&Path>,
    config: &MonolithConfig,
) -> Result<()> {
    if !config.enabled {
        anyhow::bail!("Monolith archiving is disabled");
    }

    debug!(url = %url, output = %output_path.display(), "Creating self-contained HTML with monolith");

    let mut cmd = Command::new(&config.path);

    // Input URL
    cmd.arg(url);

    // Output file
    cmd.arg("-o").arg(output_path);

    // Isolate mode - prevents external requests when viewing the saved file
    cmd.arg("-I");

    // Include CSS (always)
    cmd.arg("-s");

    // Include images (always)
    cmd.arg("-i");

    // Include fonts (always)
    cmd.arg("-f");

    // Include JavaScript (configurable - some sites need it, but it can also cause issues)
    // NOTE: monolith v2.8.3 supports `-j` to include JS, but does not accept `-J`.
    // To exclude JS, simply omit `-j`.
    if config.include_js {
        cmd.arg("-j");
    }

    // Include iframes
    cmd.arg("-F");

    // Set a reasonable timeout for network requests (in seconds)
    cmd.arg("-t").arg("30");

    // Use a reasonable user agent
    cmd.arg("-u").arg("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36");

    // Cookies file if provided
    if let Some(cookies) = cookies_file {
        if cookies.exists() {
            cmd.arg("-c").arg(cookies);
        }
    }

    // Suppress stdout, capture stderr
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());

    // Execute with timeout
    let output = tokio::time::timeout(config.timeout, cmd.output())
        .await
        .context("Monolith execution timed out")?
        .context("Failed to execute monolith")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Log warning but don't fail - monolith can fail on some pages but still produce useful output
        if !output_path.exists() {
            anyhow::bail!(
                "Monolith failed with exit code {:?}: {}",
                output.status.code(),
                stderr.trim()
            );
        }
        warn!(
            url = %url,
            exit_code = ?output.status.code(),
            stderr = %stderr.trim(),
            "Monolith completed with warnings"
        );
    }

    // Verify output file was created
    if !output_path.exists() {
        anyhow::bail!("Monolith did not create output file");
    }

    let file_size = tokio::fs::metadata(output_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    debug!(
        url = %url,
        output = %output_path.display(),
        size = file_size,
        "Self-contained HTML created successfully"
    );

    Ok(())
}

/// Check if monolith is available and working.
pub async fn check_monolith(path: &str) -> Result<String> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .await
        .context("Failed to execute monolith")?;

    if !output.status.success() {
        anyhow::bail!("Monolith version check failed");
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MonolithConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.path, "monolith");
        assert_eq!(config.timeout, Duration::from_secs(60));
        assert!(!config.include_js);
    }

    #[test]
    fn test_custom_config() {
        let config = MonolithConfig {
            enabled: true,
            path: "/usr/local/bin/monolith".to_string(),
            timeout: Duration::from_secs(120),
            include_js: true,
        };
        assert!(config.enabled);
        assert_eq!(config.path, "/usr/local/bin/monolith");
        assert_eq!(config.timeout, Duration::from_secs(120));
        assert!(config.include_js);
    }
}
