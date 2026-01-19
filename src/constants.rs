//! Shared constants used across the application.

/// User agent string used for archival HTTP requests.
///
/// This is a realistic browser user agent that is indistinguishable from a real browser,
/// making archival requests appear as normal browser traffic.
pub const ARCHIVAL_USER_AGENT: &str =
    // "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36";
