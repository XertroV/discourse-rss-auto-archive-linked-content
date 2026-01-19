//! IPFS client for pinning archived content.
//!
//! Communicates with a local IPFS daemon via its HTTP API to pin files
//! and generate public gateway URLs for retrieval.

use std::path::Path;

use anyhow::{Context, Result};
use reqwest::multipart;
use serde::Deserialize;
use tracing::{debug, info, warn};

use crate::config::Config;

/// IPFS API response for add operation.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AddResponse {
    hash: String,
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    size: String,
}

/// IPFS client for pinning content.
#[derive(Clone)]
pub struct IpfsClient {
    http: reqwest::Client,
    api_url: String,
    gateway_urls: Vec<String>,
    enabled: bool,
}

impl IpfsClient {
    /// Create a new IPFS client from configuration.
    #[must_use]
    pub fn new(config: &Config) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            http,
            api_url: config.ipfs_api_url.clone(),
            gateway_urls: config.ipfs_gateway_urls.clone(),
            enabled: config.ipfs_enabled,
        }
    }

    /// Check if IPFS is enabled.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Pin a file to IPFS and return its CID.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or pinned.
    pub async fn pin_file(&self, path: &Path) -> Result<String> {
        if !self.enabled {
            anyhow::bail!("IPFS is not enabled");
        }

        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");

        let file_bytes = tokio::fs::read(path)
            .await
            .context("Failed to read file for IPFS pinning")?;

        let part = multipart::Part::bytes(file_bytes).file_name(filename.to_string());
        let form = multipart::Form::new().part("file", part);

        let url = format!("{}/api/v0/add?pin=true", self.api_url);
        debug!(url = %url, file = %path.display(), "Pinning file to IPFS");

        let response = self
            .http
            .post(&url)
            .multipart(form)
            .send()
            .await
            .context("Failed to send request to IPFS daemon")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            anyhow::bail!("IPFS add failed: {status} - {body}");
        }

        let add_response: AddResponse = response
            .json()
            .await
            .context("Failed to parse IPFS add response")?;

        info!(cid = %add_response.hash, file = %path.display(), "Pinned file to IPFS");

        Ok(add_response.hash)
    }

    /// Pin bytes to IPFS and return its CID.
    ///
    /// # Errors
    ///
    /// Returns an error if the content cannot be pinned.
    pub async fn pin_bytes(&self, data: &[u8], filename: &str) -> Result<String> {
        if !self.enabled {
            anyhow::bail!("IPFS is not enabled");
        }

        let part = multipart::Part::bytes(data.to_vec()).file_name(filename.to_string());
        let form = multipart::Form::new().part("file", part);

        let url = format!("{}/api/v0/add?pin=true", self.api_url);
        debug!(url = %url, filename = %filename, "Pinning bytes to IPFS");

        let response = self
            .http
            .post(&url)
            .multipart(form)
            .send()
            .await
            .context("Failed to send request to IPFS daemon")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            anyhow::bail!("IPFS add failed: {status} - {body}");
        }

        let add_response: AddResponse = response
            .json()
            .await
            .context("Failed to parse IPFS add response")?;

        info!(cid = %add_response.hash, filename = %filename, "Pinned bytes to IPFS");

        Ok(add_response.hash)
    }

    /// Pin a directory of files to IPFS recursively.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be pinned.
    pub async fn pin_directory(&self, path: &Path) -> Result<String> {
        if !self.enabled {
            anyhow::bail!("IPFS is not enabled");
        }

        // For directories, we use the add endpoint with wrap-with-directory
        let url = format!(
            "{}/api/v0/add?pin=true&recursive=true&wrap-with-directory=true",
            self.api_url
        );

        let mut form = multipart::Form::new();

        // Walk the directory and add all files
        let entries = walkdir(path).await?;
        for (relative_path, file_bytes) in entries {
            let part = multipart::Part::bytes(file_bytes)
                .file_name(relative_path.clone())
                .mime_str("application/octet-stream")
                .context("Failed to set mime type")?;
            form = form.part("file", part);
        }

        debug!(url = %url, dir = %path.display(), "Pinning directory to IPFS");

        let response = self
            .http
            .post(&url)
            .multipart(form)
            .send()
            .await
            .context("Failed to send request to IPFS daemon")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            anyhow::bail!("IPFS add failed: {status} - {body}");
        }

        // The response contains multiple JSON objects, one per file plus the directory
        // The last one is the root directory CID
        let body = response.text().await?;
        let mut last_hash = String::new();

        for line in body.lines() {
            if let Ok(resp) = serde_json::from_str::<AddResponse>(line) {
                last_hash = resp.hash;
            }
        }

        if last_hash.is_empty() {
            anyhow::bail!("No CID returned from IPFS");
        }

        info!(cid = %last_hash, dir = %path.display(), "Pinned directory to IPFS");

        Ok(last_hash)
    }

    /// Generate public gateway URLs for a CID.
    #[must_use]
    pub fn gateway_urls(&self, cid: &str) -> Vec<String> {
        self.gateway_urls
            .iter()
            .map(|base| format!("{base}{cid}"))
            .collect()
    }

    /// Check if the IPFS daemon is reachable.
    ///
    /// # Errors
    ///
    /// Returns an error if the daemon is not reachable.
    pub async fn health_check(&self) -> Result<bool> {
        if !self.enabled {
            return Ok(false);
        }

        let url = format!("{}/api/v0/id", self.api_url);

        match self.http.post(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(e) => {
                warn!(error = %e, "IPFS daemon health check failed");
                Ok(false)
            }
        }
    }
}

/// Walk a directory and return all files with their relative paths.
async fn walkdir(path: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    let mut entries = Vec::new();
    let mut stack = vec![path.to_path_buf()];

    while let Some(current) = stack.pop() {
        let mut dir = tokio::fs::read_dir(&current)
            .await
            .context("Failed to read directory")?;

        while let Some(entry) = dir.next_entry().await? {
            let entry_path = entry.path();
            let file_type = entry.file_type().await?;

            if file_type.is_dir() {
                stack.push(entry_path);
            } else if file_type.is_file() {
                let relative = entry_path
                    .strip_prefix(path)
                    .unwrap_or(&entry_path)
                    .to_string_lossy()
                    .to_string();
                let bytes = tokio::fs::read(&entry_path).await?;
                entries.push((relative, bytes));
            }
        }
    }

    Ok(entries)
}

impl std::fmt::Debug for IpfsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IpfsClient")
            .field("api_url", &self.api_url)
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_urls() {
        let config = Config {
            ipfs_enabled: true,
            ipfs_gateway_urls: vec![
                "https://ipfs.io/ipfs/".to_string(),
                "https://dweb.link/ipfs/".to_string(),
                "https://gateway.pinata.cloud/ipfs/".to_string(),
            ],
            ..Config::for_testing()
        };

        let client = IpfsClient::new(&config);
        let urls = client.gateway_urls("QmTest123");

        assert_eq!(urls.len(), 3);
        assert_eq!(urls[0], "https://ipfs.io/ipfs/QmTest123");
        assert_eq!(urls[1], "https://dweb.link/ipfs/QmTest123");
        assert_eq!(urls[2], "https://gateway.pinata.cloud/ipfs/QmTest123");
    }
}
