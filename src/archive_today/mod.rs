use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::Client;
use tokio::sync::Semaphore;
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Rate-limited Archive.today client.
pub struct ArchiveTodayClient {
    client: Client,
    /// Semaphore for rate limiting (permits per minute).
    rate_limiter: Arc<Semaphore>,
    /// Interval between permit releases.
    permit_interval: Duration,
}

impl ArchiveTodayClient {
    /// Create a new Archive.today client with rate limiting.
    ///
    /// # Arguments
    ///
    /// * `rate_limit_per_min` - Maximum submissions per minute (default 3).
    #[must_use]
    pub fn new(rate_limit_per_min: u32) -> Self {
        let rate_limit = rate_limit_per_min.max(1) as usize;
        let permit_interval = Duration::from_secs(60) / rate_limit as u32;

        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .expect("Failed to create HTTP client");

        let rate_limiter = Arc::new(Semaphore::new(rate_limit));

        // Start background task to release permits over time
        let limiter = rate_limiter.clone();
        let interval = permit_interval;
        tokio::spawn(async move {
            loop {
                sleep(interval).await;
                // Add permit back if below limit
                if limiter.available_permits() < rate_limit {
                    limiter.add_permits(1);
                }
            }
        });

        Self {
            client,
            rate_limiter,
            permit_interval,
        }
    }

    /// Submit a URL to Archive.today for archiving.
    ///
    /// Returns the archive URL if successful.
    ///
    /// # Errors
    ///
    /// Returns an error if the submission fails or times out.
    pub async fn submit(&self, url: &str) -> Result<Option<String>> {
        // Acquire rate limit permit
        let _permit = self
            .rate_limiter
            .acquire()
            .await
            .context("Rate limiter closed")?;

        // Brief pause to respect rate limiting
        sleep(Duration::from_millis(500)).await;

        debug!(url = %url, "Submitting URL to Archive.today");

        // Archive.today uses form submission
        // First, check if the URL is already archived
        if let Some(existing) = self.check_existing(url).await? {
            info!(url = %url, archive = %existing, "URL already archived on Archive.today");
            return Ok(Some(existing));
        }

        // Submit the URL for archiving
        // Archive.today's submission endpoint
        let submit_url = "https://archive.today/submit/";

        let form = [("url", url)];

        let response = self
            .client
            .post(submit_url)
            .form(&form)
            .send()
            .await
            .context("Failed to submit to Archive.today")?;

        let status = response.status();
        let final_url = response.url().to_string();

        // Archive.today typically redirects to the archived page
        if status.is_success() || status.is_redirection() {
            // Read the response body once
            let body = response.text().await.unwrap_or_default();

            // Check if we got redirected to an archive page
            if final_url.contains("archive.today/") || final_url.contains("archive.ph/") {
                // Look for the canonical archive URL in the response
                if let Some(archive_url) = extract_archive_url(&body) {
                    info!(url = %url, archive = %archive_url, "Archive.today snapshot created");
                    return Ok(Some(archive_url));
                }

                // If we're on an archive.today URL, return it
                if final_url.starts_with("https://archive.today/")
                    || final_url.starts_with("https://archive.ph/")
                    || final_url.starts_with("https://archive.is/")
                {
                    info!(url = %url, archive = %final_url, "Archive.today snapshot created");
                    return Ok(Some(final_url));
                }
            }

            // Try to get archive URL from response body
            if let Some(archive_url) = extract_archive_url(&body) {
                info!(url = %url, archive = %archive_url, "Archive.today snapshot created");
                return Ok(Some(archive_url));
            }

            // Return generic search URL if we can't determine exact archive
            let search_url = format!("https://archive.today/{}", urlencoding::encode(url));
            info!(url = %url, "Archive.today submission accepted");
            Ok(Some(search_url))
        } else if status.as_u16() == 429 {
            warn!(url = %url, "Archive.today rate limited, will retry later");
            // Wait extra time on rate limit
            sleep(self.permit_interval * 3).await;
            Ok(None)
        } else {
            warn!(url = %url, status = %status, "Archive.today submission failed");
            Ok(None)
        }
    }

    /// Check if a URL has been archived on Archive.today.
    ///
    /// Returns the most recent archive URL if available.
    pub async fn check_existing(&self, url: &str) -> Result<Option<String>> {
        let check_url = format!("https://archive.today/{}", urlencoding::encode(url));

        let response = self
            .client
            .get(&check_url)
            .send()
            .await
            .context("Failed to check Archive.today")?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let final_url = response.url().to_string();

        // If we got redirected to an actual archive, return it
        // Archive URLs typically look like: https://archive.today/XXXXX
        if is_archive_url(&final_url) {
            return Ok(Some(final_url));
        }

        // Check the response body for archive links
        let body = response.text().await.unwrap_or_default();
        if let Some(archive_url) = extract_archive_url(&body) {
            return Ok(Some(archive_url));
        }

        Ok(None)
    }
}

/// Check if a URL is an Archive.today archive URL.
fn is_archive_url(url: &str) -> bool {
    // Known non-archive paths that should not be matched
    const EXCLUDED_PATHS: [&str; 5] = ["submit", "search", "about", "faq", "timegate"];

    let patterns = [
        "archive.today/",
        "archive.ph/",
        "archive.is/",
        "archive.li/",
        "archive.vn/",
        "archive.md/",
    ];

    for pattern in &patterns {
        if url.contains(pattern) {
            // Check that it's not just a search/submit page
            if let Some(after_domain) = url.split(pattern).nth(1) {
                // Archive URLs have a short hash after the domain
                let first_part = after_domain.split('/').next().unwrap_or("");
                // Exclude known non-archive paths
                let is_excluded = EXCLUDED_PATHS
                    .iter()
                    .any(|&p| first_part.eq_ignore_ascii_case(p));
                if !is_excluded
                    && first_part.len() >= 5
                    && first_part.len() <= 10
                    && first_part.chars().all(|c| c.is_alphanumeric())
                {
                    return true;
                }
            }
        }
    }

    false
}

/// Extract archive URL from HTML response body.
fn extract_archive_url(body: &str) -> Option<String> {
    // Look for canonical link
    if let Some(start) = body.find("rel=\"canonical\" href=\"") {
        let after_start = &body[start + 22..];
        if let Some(end) = after_start.find('"') {
            let url = &after_start[..end];
            if is_archive_url(url) {
                return Some(url.to_string());
            }
        }
    }

    // Look for og:url meta tag
    if let Some(start) = body.find("property=\"og:url\" content=\"") {
        let after_start = &body[start + 27..];
        if let Some(end) = after_start.find('"') {
            let url = &after_start[..end];
            if is_archive_url(url) {
                return Some(url.to_string());
            }
        }
    }

    // Look for archive links in the body
    for pattern in &[
        "https://archive.today/",
        "https://archive.ph/",
        "https://archive.is/",
    ] {
        if let Some(start) = body.find(pattern) {
            let after_start = &body[start..];
            // Find the end of the URL (quote, space, or angle bracket)
            let end = after_start
                .chars()
                .position(|c| c == '"' || c == '\'' || c == '<' || c == '>' || c.is_whitespace())
                .unwrap_or(after_start.len());

            let url = &after_start[..end];
            if is_archive_url(url) {
                return Some(url.to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_archive_url() {
        assert!(is_archive_url("https://archive.today/AbCd1"));
        assert!(is_archive_url("https://archive.ph/Xy9Zw"));
        assert!(is_archive_url("https://archive.is/12345"));
        assert!(!is_archive_url("https://archive.today/submit/"));
        assert!(!is_archive_url("https://example.com/archive.today/"));
        assert!(!is_archive_url("https://archive.today/")); // Just root URL
    }

    #[test]
    fn test_extract_archive_url() {
        let html = r#"<link rel="canonical" href="https://archive.today/AbCd1">"#;
        assert_eq!(
            extract_archive_url(html),
            Some("https://archive.today/AbCd1".to_string())
        );

        let html_no_match = r#"<link rel="canonical" href="https://example.com">"#;
        assert_eq!(extract_archive_url(html_no_match), None);
    }
}
