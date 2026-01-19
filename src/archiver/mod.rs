use std::path::Path;

pub mod gallerydl;
pub mod monolith;
pub mod rate_limiter;
pub mod screenshot;
pub mod worker;
pub mod ytdlp;

pub use monolith::{create_complete_html, MonolithConfig};
pub use rate_limiter::DomainRateLimiter;
pub use screenshot::{MhtmlConfig, PdfConfig, ScreenshotConfig, ScreenshotService};
pub use worker::ArchiveWorker;

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
/// # use discourse_rss_auto_archive_linked_content::archiver::sanitize_filename;
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
    let sanitized_name: String = name
        .chars()
        .map(|c| match c {
            // Replace spaces with underscores
            ' ' => '_',
            // Remove problematic URL characters
            '#' | '?' | '&' | '%' | '"' | '\'' | '<' | '>' | '|' | '*' | ':' | '\\' | '/' => '_',
            // Keep parentheses, brackets, hyphens, underscores, dots, and alphanumerics
            '(' | ')' | '[' | ']' | '-' | '_' | '.' => c,
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

    // Limit name length (leaving room for extension)
    const MAX_NAME_LENGTH: usize = 200;
    let truncated_name = if sanitized_name.len() > MAX_NAME_LENGTH {
        &sanitized_name[..MAX_NAME_LENGTH]
    } else {
        &sanitized_name
    };

    // Recombine with extension
    format!("{truncated_name}{ext}")
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
        assert_eq!(
            sanitize_filename("Let's talk #tallowskincare #tallow.mp4"),
            "Let_s_talk_tallowskincare_tallow.mp4"
        );
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
        // Should be truncated to 200 chars + ".mp4"
        assert_eq!(sanitized.len(), 204);
        assert!(sanitized.ends_with(".mp4"));
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
}

/// Cookie options for archive downloads.
///
/// Supports two methods:
/// - `cookies_file`: Path to a Netscape-format cookies.txt file (works with yt-dlp and gallery-dl)
/// - `browser_profile`: Browser profile spec for yt-dlp's --cookies-from-browser (yt-dlp only)
///
/// If both are set, yt-dlp prefers browser_profile; gallery-dl only uses cookies_file.
#[derive(Debug, Clone, Default)]
pub struct CookieOptions<'a> {
    /// Path to cookies.txt file (Netscape format).
    pub cookies_file: Option<&'a Path>,
    /// Browser profile spec for yt-dlp (e.g., "chromium+basictext:/path/to/profile").
    pub browser_profile: Option<&'a str>,
}
