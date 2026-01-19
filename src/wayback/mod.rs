use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::Client;
use tokio::sync::Semaphore;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::constants::ARCHIVAL_USER_AGENT;

/// Rate-limited Wayback Machine client.
pub struct WaybackClient {
    client: Client,
    /// Semaphore for rate limiting (permits per minute).
    rate_limiter: Arc<Semaphore>,
    /// Interval between permit releases.
    permit_interval: Duration,
}

impl WaybackClient {
    /// Create a new Wayback client with rate limiting.
    ///
    /// # Arguments
    ///
    /// * `rate_limit_per_min` - Maximum submissions per minute.
    #[must_use]
    pub fn new(rate_limit_per_min: u32) -> Self {
        let rate_limit = rate_limit_per_min.max(1) as usize;
        let permit_interval = Duration::from_secs(60) / rate_limit as u32;

        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(ARCHIVAL_USER_AGENT)
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

    /// Submit a URL to the Wayback Machine for archiving.
    ///
    /// Returns the snapshot URL if successful.
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
        sleep(Duration::from_millis(100)).await;

        debug!(url = %url, "Submitting URL to Wayback Machine");

        // Use the save API
        let save_url = format!("https://web.archive.org/save/{url}");

        let response = self
            .client
            .get(&save_url)
            .send()
            .await
            .context("Failed to submit to Wayback Machine")?;

        let status = response.status();

        if status.is_success() || status.as_u16() == 302 {
            // Check for the Content-Location header which contains the snapshot URL
            if let Some(location) = response.headers().get("content-location") {
                if let Ok(loc_str) = location.to_str() {
                    let snapshot_url = format!("https://web.archive.org{loc_str}");
                    info!(url = %url, snapshot = %snapshot_url, "Wayback snapshot created");
                    return Ok(Some(snapshot_url));
                }
            }

            // Try to extract from Link header
            if let Some(link) = response.headers().get("link") {
                if let Ok(link_str) = link.to_str() {
                    if let Some(memento) = extract_memento_url(link_str) {
                        info!(url = %url, snapshot = %memento, "Wayback snapshot created");
                        return Ok(Some(memento));
                    }
                }
            }

            // Return a generic URL if we can't find the exact snapshot
            let generic_url = format!("https://web.archive.org/web/*/{url}");
            info!(url = %url, "Wayback submission accepted (no specific snapshot URL)");
            Ok(Some(generic_url))
        } else if status.as_u16() == 429 {
            warn!(url = %url, "Wayback Machine rate limited, will retry later");
            // Wait extra time on rate limit
            sleep(self.permit_interval * 2).await;
            Ok(None)
        } else if status.as_u16() == 523 || status.as_u16() == 520 {
            // Cloudflare errors - the target site may be blocking archival
            warn!(url = %url, status = %status, "Target site may be blocking Wayback archival");
            Ok(None)
        } else {
            warn!(url = %url, status = %status, "Wayback Machine submission failed");
            Ok(None)
        }
    }

    /// Check if a URL has been archived.
    ///
    /// Returns the most recent snapshot URL if available.
    pub async fn check_existing(&self, url: &str) -> Result<Option<String>> {
        let check_url = format!(
            "https://archive.org/wayback/available?url={}",
            urlencoding::encode(url)
        );

        let response = self
            .client
            .get(&check_url)
            .send()
            .await
            .context("Failed to check Wayback availability")?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let json: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse Wayback response")?;

        // Extract the closest snapshot URL
        let snapshot_url = json
            .get("archived_snapshots")
            .and_then(|s| s.get("closest"))
            .and_then(|c| c.get("url"))
            .and_then(|u| u.as_str())
            .map(String::from);

        Ok(snapshot_url)
    }
}

/// Extract memento URL from Link header.
fn extract_memento_url(link_header: &str) -> Option<String> {
    // Link headers look like: <url>; rel="memento"; ...
    for part in link_header.split(',') {
        if part.contains("rel=\"memento\"") || part.contains("rel=memento") {
            if let Some(start) = part.find('<') {
                if let Some(end) = part.find('>') {
                    return Some(part[start + 1..end].to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_memento_url() {
        let header = r#"<https://web.archive.org/web/20240101000000/https://example.com>; rel="memento"; datetime="Mon, 01 Jan 2024 00:00:00 GMT""#;
        let result = extract_memento_url(header);
        assert_eq!(
            result,
            Some("https://web.archive.org/web/20240101000000/https://example.com".to_string())
        );
    }

    #[test]
    fn test_extract_memento_url_no_match() {
        let header = r#"<https://example.com>; rel="original""#;
        let result = extract_memento_url(header);
        assert_eq!(result, None);
    }
}
