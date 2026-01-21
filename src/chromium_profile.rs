use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::process::Command;
use tracing::warn;

use crate::fs_utils::copy_dir_best_effort;

/// Parse a yt-dlp-style `--cookies-from-browser` spec into Chromium's `--user-data-dir`
/// and optional `--profile-directory` name.
///
/// Spec format examples:
/// - `chromium+basictext:/app/cookies/chromium-profile`
/// - `chromium+basictext:/app/cookies/chromium-profile/Default`
/// - `chromium+basictext:/path::container` (container suffix is ignored)
#[must_use]
pub fn chromium_user_data_and_profile_from_spec(spec: &str) -> (PathBuf, Option<String>) {
    let path_part = spec.split_once(':').map_or(spec, |(_, rest)| rest);

    let profile_raw = path_part.split_once("::").map_or(path_part, |(p, _)| p);

    let p = PathBuf::from(profile_raw);

    let cookies_db_present =
        |dir: &Path| dir.join("Cookies").is_file() || dir.join("Network").join("Cookies").is_file();

    // If they point at a profile dir directly (Default/), use its parent as user-data-dir.
    if cookies_db_present(&p) {
        let user_data_dir = p.parent().unwrap_or(&p).to_path_buf();
        let profile_name = p
            .file_name()
            .and_then(|s| s.to_str())
            .map(std::string::ToString::to_string);
        return (user_data_dir, profile_name);
    }

    // If they point at a user-data-dir (contains Default/), assume Default.
    if cookies_db_present(&p.join("Default")) {
        return (p, Some("Default".to_string()));
    }

    // Fallback: treat as user-data-dir without specifying profile.
    (p, None)
}

/// Clone a Chromium user-data-dir into a work directory for isolated browser sessions.
///
/// This is needed because Chromium locks its profile when running, so we can't use the
/// same profile for multiple concurrent browser instances. The clone includes cookies
/// and other session data needed for authenticated page access.
///
/// # Arguments
/// * `work_dir` - Working directory where the clone will be created
/// * `source` - Source Chromium user-data-dir to clone
/// * `profile_dir` - Optional profile directory name (e.g., "Default")
/// * `context` - Description of the context for logging (e.g., "twitter html", "reddit html")
///
/// # Returns
/// Path to the cloned user-data-dir
pub async fn clone_chromium_user_data_dir(
    work_dir: &Path,
    source: &Path,
    profile_dir: Option<&str>,
    context: &str,
) -> Result<PathBuf> {
    let dest = work_dir.join("chromium-user-data");

    // Clean up any previous attempt
    let _ = tokio::fs::remove_dir_all(&dest).await;
    tokio::fs::create_dir_all(&dest)
        .await
        .context("Failed to create chromium-user-data dir")?;

    // Prefer cp -a to preserve Chromium's expected layout
    let cp_output = Command::new("cp")
        .arg("-a")
        .arg(format!("{}/.", source.display()))
        .arg(&dest)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await;

    match cp_output {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                status = %output.status,
                source = %source.display(),
                stderr = %stderr.trim(),
                context = %context,
                "cp -a failed while cloning Chromium profile; falling back to best-effort copy"
            );
            copy_dir_best_effort(
                source,
                &dest,
                &format!("chromium profile clone ({context})"),
            )
            .await?;
        }
        Err(e) => {
            warn!(
                error = %e,
                source = %source.display(),
                context = %context,
                "Failed to spawn cp -a for Chromium profile copy; falling back to best-effort copy"
            );
            copy_dir_best_effort(
                source,
                &dest,
                &format!("chromium profile clone ({context})"),
            )
            .await?;
        }
    }

    // Remove singleton lock/socket artifacts so Chromium doesn't think the profile is in-use
    for name in ["SingletonLock", "SingletonCookie", "SingletonSocket"] {
        let _ = tokio::fs::remove_file(dest.join(name)).await;
    }

    // Validate that the clone contains critical cookie materials
    let local_state = dest.join("Local State");
    if !local_state.is_file() {
        anyhow::bail!(
            "Cloned Chromium profile is missing 'Local State'. Ensure the cookies volume is readable."
        );
    }

    let profile_name = profile_dir.unwrap_or("Default");
    let cookie_db_candidates = [
        dest.join(profile_name).join("Cookies"),
        dest.join(profile_name).join("Network").join("Cookies"),
        dest.join("Default").join("Cookies"),
        dest.join("Default").join("Network").join("Cookies"),
    ];
    let has_cookie_db = cookie_db_candidates.iter().any(|p| p.is_file());
    if !has_cookie_db {
        anyhow::bail!("Cloned Chromium profile does not contain a readable Cookies database.");
    }

    Ok(dest)
}

/// Fetch HTML from a URL using Chromium's --dump-dom to get rendered content after JS execution.
///
/// This is useful for sites that require JavaScript to render content (Twitter, Reddit, etc.).
/// The browser uses the provided Chromium profile for authentication cookies.
///
/// # Arguments
/// * `url` - URL to fetch
/// * `work_dir` - Working directory for cloning the Chromium profile
/// * `browser_profile_spec` - yt-dlp-style browser profile spec
/// * `timeout_secs` - Timeout in seconds for the browser operation
/// * `context` - Description of the context for logging
///
/// # Returns
/// The rendered HTML content
pub async fn fetch_html_with_chromium(
    url: &str,
    work_dir: &Path,
    browser_profile_spec: &str,
    timeout_secs: u64,
    context: &str,
) -> Result<String> {
    let (source_user_data_dir, profile_dir) =
        chromium_user_data_and_profile_from_spec(browser_profile_spec);

    // Clone the chromium profile to avoid lock contention
    let user_data_dir = clone_chromium_user_data_dir(
        work_dir,
        &source_user_data_dir,
        profile_dir.as_deref(),
        context,
    )
    .await
    .with_context(|| format!("Failed to clone Chromium user-data-dir for {context}"))?;

    let chrome_path =
        std::env::var("SCREENSHOT_CHROME_PATH").unwrap_or_else(|_| "chromium".to_string());

    let mut cmd = Command::new(chrome_path);
    cmd.arg("--headless=new")
        .arg("--no-sandbox")
        .arg("--disable-gpu")
        .arg("--disable-dev-shm-usage")
        .arg("--disable-software-rasterizer")
        .arg("--disable-extensions")
        .arg("--disable-background-networking")
        .arg("--disable-blink-features=AutomationControlled")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--window-size=1280,1600")
        .arg("--lang=en-US,en")
        .arg(format!(
            "--user-agent={}",
            crate::constants::ARCHIVAL_USER_AGENT
        ))
        .arg(format!("--user-data-dir={}", user_data_dir.display()));

    if let Some(ref profile_dir) = profile_dir {
        cmd.arg(format!("--profile-directory={profile_dir}"));
    }

    // Dump final DOM after JS execution/navigation
    cmd.arg("--dump-dom").arg(url);

    let output = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output())
        .await
        .context("Chromium dump-dom timed out")?
        .context("Failed to execute Chromium")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "Chromium dump-dom failed (exit {:?}): {}",
            output.status.code(),
            stderr.trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let html = stdout.trim();
    if html.is_empty() {
        anyhow::bail!("Chromium dump-dom returned empty output");
    }
    Ok(html.to_string())
}
