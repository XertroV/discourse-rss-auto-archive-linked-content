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
/// * `input` - The URL (or file path / file:// URL) to archive
/// * `output_path` - Where to save the self-contained HTML file
/// * `cookies_file` - Optional path to a cookies.txt file for authenticated requests
/// * `config` - Monolith configuration
///
/// # Errors
///
/// Returns an error if monolith fails to execute or times out.
pub async fn create_complete_html(
    input: &str,
    output_path: &Path,
    cookies_file: Option<&Path>,
    config: &MonolithConfig,
) -> Result<()> {
    if !config.enabled {
        anyhow::bail!("Monolith archiving is disabled");
    }

    debug!(input = %input, output = %output_path.display(), "Creating self-contained HTML with monolith");

    let mut cmd = Command::new(&config.path);

    // Input URL
    cmd.arg(input);

    // Output file
    cmd.arg("-o").arg(output_path);

    // Isolate mode - prevents external requests when viewing the saved file
    cmd.arg("-I");

    // NOTE: In monolith v3.0+, flags were inverted to be exclusion flags.
    // By default, monolith includes CSS, images, fonts, JS, and frames.
    // We use flags to EXCLUDE what we don't want.

    // CSS is included by default (no flag needed)
    // Images are included by default (no flag needed)
    // Fonts are included by default (no flag needed)
    // Frames/iframes are included by default (no flag needed)

    // JavaScript handling (configurable - some sites need it, but it can also cause issues)
    // In v3.0+, JS is included by default. Use `-j` to EXCLUDE it.
    if !config.include_js {
        cmd.arg("-j");
    }

    // Exclude archive sites from asset fetching to avoid recursive archive references
    // Web Archive domains
    cmd.arg("-B").arg("web.archive.org");
    cmd.arg("-B").arg("archive.org");

    // Archive.today and its many aliases/mirrors
    cmd.arg("-B").arg("archive.today");
    cmd.arg("-B").arg("archive.is");
    cmd.arg("-B").arg("archive.ph");
    cmd.arg("-B").arg("archive.fo");
    cmd.arg("-B").arg("archive.li");
    cmd.arg("-B").arg("archive.md");
    cmd.arg("-B").arg("archive.vn");

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

    // Capture stdout+stderr. Some monolith errors are printed to stdout.
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // Execute with timeout
    let output = tokio::time::timeout(config.timeout, cmd.output())
        .await
        .context("Monolith execution timed out")?
        .context("Failed to execute monolith")?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Limit output size for logging but preserve as much error context as possible
        // Max 2000 chars per stream to avoid excessive log spam while keeping useful details
        const MAX_OUTPUT_LEN: usize = 2000;
        let stdout_trimmed = if stdout.len() > MAX_OUTPUT_LEN {
            format!(
                "{}...[truncated {} more chars]",
                &stdout[..MAX_OUTPUT_LEN],
                stdout.len() - MAX_OUTPUT_LEN
            )
        } else {
            stdout.trim().to_string()
        };
        let stderr_trimmed = if stderr.len() > MAX_OUTPUT_LEN {
            format!(
                "{}...[truncated {} more chars]",
                &stderr[..MAX_OUTPUT_LEN],
                stderr.len() - MAX_OUTPUT_LEN
            )
        } else {
            stderr.trim().to_string()
        };

        // Log warning but don't fail - monolith can fail on some pages but still produce useful output
        if !output_path.exists() {
            anyhow::bail!(
                "Monolith failed with exit code {:?}.\nInput: {}\nStderr:\n{}\nStdout:\n{}",
                output.status.code(),
                input,
                stderr_trimmed,
                stdout_trimmed
            );
        }
        warn!(
            input = %input,
            exit_code = ?output.status.code(),
            stderr = %stderr_trimmed,
            stdout = %stdout_trimmed,
            "Monolith completed with warnings but produced output file"
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
        input = %input,
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
