//! TLS/HTTPS support with automatic Let's Encrypt certificate management.
//!
//! This module provides ACME-based automatic TLS certificate acquisition and renewal
//! using the TLS-ALPN-01 challenge method. Certificates are cached to disk to persist
//! across restarts and avoid rate limits.

use anyhow::{Context, Result};
use rustls_acme::caches::DirCache;
use rustls_acme::AcmeConfig;
use tracing::info;

use crate::config::Config;

/// Create an ACME configuration for automatic TLS certificate management.
///
/// This configures the TLS-ALPN-01 challenge method, which handles certificate
/// validation during the TLS handshake on the same port as HTTPS traffic.
///
/// # Errors
///
/// Returns an error if the cache directory cannot be created.
pub fn create_acme_config(config: &Config) -> Result<AcmeConfig<std::io::Error>> {
    // Ensure cache directory exists
    std::fs::create_dir_all(&config.tls_cache_dir).with_context(|| {
        format!(
            "Failed to create TLS cache directory: {}",
            config.tls_cache_dir.display()
        )
    })?;

    let domains: Vec<String> = config.tls_domains.clone();
    info!(domains = ?domains, "Configuring ACME for domains");

    if config.tls_use_staging {
        info!("Using Let's Encrypt staging environment (certificates will not be trusted)");
    } else {
        info!("Using Let's Encrypt production environment");
    }

    let cache_dir = config.tls_cache_dir.clone();
    let mut acme_config = AcmeConfig::new(domains)
        .cache(DirCache::new(cache_dir))
        .directory_lets_encrypt(!config.tls_use_staging); // true = production, false = staging

    // Add contact email if provided (recommended for certificate expiry notifications)
    if let Some(ref email) = config.tls_contact_email {
        let contact = format!("mailto:{email}");
        info!(contact = %contact, "Setting ACME contact");
        acme_config = acme_config.contact([contact]);
    }

    Ok(acme_config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_config(use_staging: bool) -> Config {
        Config {
            rss_url: "https://example.com/rss".to_string(),
            poll_interval: std::time::Duration::from_secs(60),
            cache_window: std::time::Duration::from_secs(3600),
            database_path: PathBuf::from("./test.db"),
            s3_bucket: "test".to_string(),
            s3_region: "us-east-1".to_string(),
            s3_endpoint: None,
            s3_prefix: "archives/".to_string(),
            worker_concurrency: 4,
            per_domain_concurrency: 1,
            work_dir: PathBuf::from("./tmp"),
            yt_dlp_path: "yt-dlp".to_string(),
            gallery_dl_path: "gallery-dl".to_string(),
            cookies_file_path: None,
            archive_mode: crate::config::ArchiveMode::Deletable,
            archive_quote_only_links: false,
            web_host: "0.0.0.0".to_string(),
            web_port: 8080,
            tls_enabled: true,
            tls_domains: vec!["example.com".to_string()],
            tls_contact_email: Some("test@example.com".to_string()),
            tls_cache_dir: PathBuf::from("./test_acme_cache"),
            tls_use_staging: use_staging,
            tls_https_port: 443,
            wayback_enabled: false,
            wayback_rate_limit_per_min: 5,
            archive_today_enabled: false,
            archive_today_rate_limit_per_min: 3,
            backup_enabled: false,
            backup_interval_hours: 24,
            backup_retention_count: 30,
            log_format: crate::config::LogFormat::Pretty,
            ipfs_enabled: false,
            ipfs_api_url: "http://127.0.0.1:5001".to_string(),
            ipfs_gateway_urls: vec![],
            submission_enabled: false,
            submission_rate_limit_per_hour: 10,
            screenshot_enabled: false,
            screenshot_viewport_width: 1280,
            screenshot_viewport_height: 800,
            screenshot_timeout_secs: 30,
            screenshot_chrome_path: None,
        }
    }

    #[test]
    fn test_create_acme_config_staging() {
        let config = test_config(true);
        let result = create_acme_config(&config);
        assert!(result.is_ok());
        // Clean up
        let _ = std::fs::remove_dir_all("./test_acme_cache");
    }

    #[test]
    fn test_create_acme_config_production() {
        let config = test_config(false);
        let result = create_acme_config(&config);
        assert!(result.is_ok());
        // Clean up
        let _ = std::fs::remove_dir_all("./test_acme_cache");
    }
}
