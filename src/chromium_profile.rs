use std::path::{Path, PathBuf};

/// Parse a yt-dlp-style `--cookies-from-browser` spec into Chromium's `--user-data-dir`
/// and optional `--profile-directory` name.
///
/// Spec format examples:
/// - `chromium+basictext:/app/cookies/chromium-profile`
/// - `chromium+basictext:/app/cookies/chromium-profile/Default`
/// - `chromium+basictext:/path::container` (container suffix is ignored)
#[must_use]
pub fn chromium_user_data_and_profile_from_spec(spec: &str) -> (PathBuf, Option<String>) {
    let path_part = spec.split_once(':').map(|(_, rest)| rest).unwrap_or(spec);

    let profile_raw = path_part
        .split_once("::")
        .map(|(p, _)| p)
        .unwrap_or(path_part);

    let p = PathBuf::from(profile_raw);

    let cookies_db_present =
        |dir: &Path| dir.join("Cookies").is_file() || dir.join("Network").join("Cookies").is_file();

    // If they point at a profile dir directly (Default/), use its parent as user-data-dir.
    if cookies_db_present(&p) {
        let user_data_dir = p.parent().unwrap_or(&p).to_path_buf();
        let profile_name = p
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        return (user_data_dir, profile_name);
    }

    // If they point at a user-data-dir (contains Default/), assume Default.
    if cookies_db_present(&p.join("Default")) {
        return (p, Some("Default".to_string()));
    }

    // Fallback: treat as user-data-dir without specifying profile.
    (p, None)
}
