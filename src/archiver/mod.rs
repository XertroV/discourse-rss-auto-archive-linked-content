use sha2::{Digest, Sha256};
use std::path::Path;

pub mod comment_worker;
pub mod gallerydl;
pub mod monolith;
pub mod playlist;
pub mod rate_limiter;
pub mod screenshot;
pub mod tiktok_comments;
pub mod transcript;
pub mod worker;
pub mod ytdlp;

pub use monolith::{create_complete_html, MonolithConfig};
pub use rate_limiter::DomainRateLimiter;
pub use screenshot::{MhtmlConfig, PdfConfig, ScreenshotConfig, ScreenshotService};
pub use worker::{
    extract_platform_name, is_comments_supported_platform, process_subtitle_files, ArchiveWorker,
};

/// Sanitize a filename to be URL-safe and filesystem-safe.
///
/// This function:
/// - Replaces spaces with underscores
/// - Removes or replaces problematic characters (#, ?, &, quotes, etc.)
/// - Preserves file extension
/// - Limits length to avoid filesystem issues
/// - Ensures the result is safe for use in URLs
///
/// # Examples
///
/// ```
/// # use discourse_link_archiver::archiver::sanitize_filename;
/// assert_eq!(sanitize_filename("My Video #1.mp4"), "My_Video_1.mp4");
/// assert_eq!(sanitize_filename("Test & Demo?.webm"), "Test_Demo.webm");
/// ```
pub fn sanitize_filename(filename: &str) -> String {
    // Split filename into name and extension
    let (name, ext) = if let Some(dot_pos) = filename.rfind('.') {
        let (n, e) = filename.split_at(dot_pos);
        (n, e) // ext includes the dot
    } else {
        (filename, "")
    };

    // Sanitize the name part
    let sanitized_name: String =
        name.chars()
            .map(|c| match c {
                // Replace spaces with underscores
                ' ' => '_',
                // Remove problematic URL characters and periods
                '#' | '?' | '&' | '%' | '"' | '\'' | '<' | '>' | '|' | '*' | ':' | '\\' | '/'
                | '.' => '_',
                // Keep parentheses, brackets, hyphens, underscores, and alphanumerics
                '(' | ')' | '[' | ']' | '-' | '_' => c,
                // Keep alphanumerics
                c if c.is_alphanumeric() => c,
                // Replace everything else with underscore
                _ => '_',
            })
            .collect();

    // Remove consecutive underscores and trim underscores from edges
    let sanitized_name = sanitized_name
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");

    // Limit name length (leaving room for hash and extension)
    const MAX_NAME_LENGTH: usize = 20;
    let final_name = if sanitized_name.len() > MAX_NAME_LENGTH {
        // Create a hash of the full sanitized name
        let mut hasher = Sha256::new();
        hasher.update(sanitized_name.as_bytes());
        let hash = hasher.finalize();
        let hash_suffix = format!("{:x}", hash)[..7].to_string();

        // Truncate to max length and append hash
        format!("{}_{}", &sanitized_name[..MAX_NAME_LENGTH], hash_suffix)
    } else {
        sanitized_name.to_string()
    };

    // Recombine with extension
    format!("{final_name}{ext}")
}

/// Sanitize a filename without truncating.
///
/// This is useful for gallery-dl output files where the original filename
/// contains a unique media ID that should be preserved for deduplication.
///
/// Unlike `sanitize_filename`, this function:
/// - Does NOT truncate long filenames
/// - Preserves the full sanitized name
///
/// Use this for files where the filename is already deterministic (e.g., Twitter media IDs).
pub fn sanitize_filename_preserve_length(filename: &str) -> String {
    // Split filename into name and extension
    let (name, ext) = if let Some(dot_pos) = filename.rfind('.') {
        let (n, e) = filename.split_at(dot_pos);
        (n, e) // ext includes the dot
    } else {
        (filename, "")
    };

    // Sanitize the name part
    let sanitized_name: String =
        name.chars()
            .map(|c| match c {
                // Replace spaces with underscores
                ' ' => '_',
                // Remove problematic URL characters and periods
                '#' | '?' | '&' | '%' | '"' | '\'' | '<' | '>' | '|' | '*' | ':' | '\\' | '/'
                | '.' => '_',
                // Keep parentheses, brackets, hyphens, underscores, and alphanumerics
                '(' | ')' | '[' | ']' | '-' | '_' => c,
                // Keep alphanumerics
                c if c.is_alphanumeric() => c,
                // Replace everything else with underscore
                _ => '_',
            })
            .collect();

    // Remove consecutive underscores and trim underscores from edges
    let sanitized_name = sanitized_name
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");

    // Recombine with extension (no truncation)
    format!("{sanitized_name}{ext}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename_spaces() {
        assert_eq!(sanitize_filename("My Video File.mp4"), "My_Video_File.mp4");
    }

    #[test]
    fn test_sanitize_filename_hash() {
        assert_eq!(sanitize_filename("Video #1.mp4"), "Video_1.mp4");
        // Long name with hash symbols - gets truncated with hash suffix
        let result = sanitize_filename("Let's talk #tallowskincare #tallow.mp4");
        assert!(result.starts_with("Let_s_talk_tallowski"));
        assert!(result.ends_with(".mp4"));
        // Should have the hash suffix format: 20 chars + "_" + 7 hex + ".mp4"
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn test_sanitize_filename_special_chars() {
        assert_eq!(sanitize_filename("Test & Demo?.webm"), "Test_Demo.webm");
        assert_eq!(sanitize_filename("File<name>.mp4"), "File_name.mp4");
        assert_eq!(sanitize_filename("Path/to\\file.mp4"), "Path_to_file.mp4");
    }

    #[test]
    fn test_sanitize_filename_quotes() {
        assert_eq!(sanitize_filename("\"Quoted\".mp4"), "Quoted.mp4");
        assert_eq!(sanitize_filename("'Single'.mp4"), "Single.mp4");
    }

    #[test]
    fn test_sanitize_filename_unicode() {
        // Fancy apostrophes and other Unicode characters
        assert_eq!(
            sanitize_filename("Let's talk about….mp4"),
            "Let_s_talk_about.mp4"
        );
    }

    #[test]
    fn test_sanitize_filename_consecutive_underscores() {
        assert_eq!(
            sanitize_filename("Multiple   Spaces.mp4"),
            "Multiple_Spaces.mp4"
        );
        assert_eq!(sanitize_filename("Test###File.mp4"), "Test_File.mp4");
    }

    #[test]
    fn test_sanitize_filename_preserve_valid_chars() {
        assert_eq!(
            sanitize_filename("Valid-File_Name(123).mp4"),
            "Valid-File_Name(123).mp4"
        );
        assert_eq!(sanitize_filename("Test[1].mp4"), "Test[1].mp4");
    }

    #[test]
    fn test_sanitize_filename_no_extension() {
        assert_eq!(sanitize_filename("filename"), "filename");
        assert_eq!(sanitize_filename("file name"), "file_name");
    }

    #[test]
    fn test_sanitize_filename_long_name() {
        let long_name = "a".repeat(250);
        let filename = format!("{long_name}.mp4");
        let sanitized = sanitize_filename(&filename);
        // Should be truncated to 20 chars + "_" + 7 char hash + ".mp4" = 32 total
        assert_eq!(sanitized.len(), 32);
        assert!(sanitized.starts_with("aaaaaaaaaaaaaaaaaaaa_")); // 20 a's + underscore
        assert!(sanitized.ends_with(".mp4"));
        // Verify that the hash portion exists (7 hex chars between last _ and .mp4)
        let parts: Vec<&str> = sanitized.trim_end_matches(".mp4").split('_').collect();
        assert_eq!(parts.last().unwrap().len(), 7);
    }

    #[test]
    fn test_sanitize_filename_short_name_no_hash() {
        // Names under 20 chars shouldn't get a hash
        assert_eq!(sanitize_filename("short.mp4"), "short.mp4");
        assert_eq!(sanitize_filename("My_Video_File.mp4"), "My_Video_File.mp4");
    }

    #[test]
    fn test_sanitize_filename_strips_periods() {
        // Periods in the name should be replaced with underscores
        assert_eq!(
            sanitize_filename("file.name.test.mp4"),
            "file_name_test.mp4"
        );
        assert_eq!(sanitize_filename("video.1.2.3.mp4"), "video_1_2_3.mp4");
        assert_eq!(sanitize_filename("Dr. Test.mp4"), "Dr_Test.mp4");
    }

    #[test]
    fn test_sanitize_filename_real_world_example() {
        // From the user's example URL
        let problematic =
            "Let's talk about the skincare industry. #tallowskincare #tallow #tall....mp4";
        let sanitized = sanitize_filename(problematic);
        // Should have no spaces, no hashes, no special apostrophes
        assert!(!sanitized.contains(' '));
        assert!(!sanitized.contains('#'));
        assert!(!sanitized.contains('\u{2019}')); // Right single quotation mark
        assert!(sanitized.ends_with(".mp4"));
    }

    #[test]
    fn test_sanitize_filename_preserve_length_no_truncation() {
        // Twitter media ID filenames should NOT be truncated
        let twitter_file = "twitter_G-VPn9IWQAAeSIV.jpg";
        let result = sanitize_filename_preserve_length(twitter_file);
        // Should preserve the full media ID
        assert_eq!(result, "twitter_G-VPn9IWQAAeSIV.jpg");
    }

    #[test]
    fn test_sanitize_filename_preserve_length_long_name() {
        // Even very long names should not be truncated
        let long_name = "twitter_very_long_media_id_that_exceeds_twenty_characters.jpg";
        let result = sanitize_filename_preserve_length(long_name);
        // Should NOT have a hash suffix, should preserve full name
        assert_eq!(result, long_name);
        assert!(!result.contains("_7")); // No hash suffix pattern
    }

    #[test]
    fn test_sanitize_filename_preserve_length_still_sanitizes_chars() {
        // Should still remove problematic characters
        let with_special = "twitter_media#id?.jpg";
        let result = sanitize_filename_preserve_length(with_special);
        assert_eq!(result, "twitter_media_id.jpg");
    }
}

/// Cookie options for archive downloads.
///
/// Supports two methods:
/// - `cookies_file`: Path to a Netscape-format cookies.txt file (works with yt-dlp and gallery-dl)
/// - `browser_profile`: Browser profile spec for yt-dlp's --cookies-from-browser (yt-dlp only)
///
/// If both are set, yt-dlp prefers browser_profile; gallery-dl only uses cookies_file.
pub struct CookieOptions<'a> {
    /// Path to cookies.txt file (Netscape format).
    pub cookies_file: Option<&'a Path>,
    /// Browser profile spec for yt-dlp (e.g., "chromium+basictext:/path/to/profile").
    pub browser_profile: Option<&'a str>,
    /// Screenshot service for rendering HTML via CDP (optional, for JS-heavy sites).
    pub screenshot_service: Option<&'a screenshot::ScreenshotService>,
}

impl Default for CookieOptions<'_> {
    fn default() -> Self {
        Self {
            cookies_file: None,
            browser_profile: None,
            screenshot_service: None,
        }
    }
}
