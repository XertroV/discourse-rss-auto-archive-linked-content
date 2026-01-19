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
