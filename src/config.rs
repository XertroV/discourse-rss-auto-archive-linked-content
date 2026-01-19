use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    MissingEnvVar(String),
    #[error("invalid value for {name}: {message}")]
    InvalidValue { name: String, message: String },
    #[error("failed to parse {name} as integer: {source}")]
    ParseInt {
        name: String,
        #[source]
        source: std::num::ParseIntError,
    },
    #[error("failed to parse {name} as boolean: {value}")]
    ParseBool { name: String, value: String },
}

/// Application configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    // RSS Feed
    pub rss_url: String,
    pub poll_interval: Duration,
    pub cache_window: Duration,

    // Database
    pub database_path: PathBuf,

    // S3 Storage
    pub s3_bucket: String,
    pub s3_region: String,
    pub s3_endpoint: Option<String>,
    pub s3_prefix: String,

    // Archive Workers
    pub worker_concurrency: usize,
    pub per_domain_concurrency: usize,
    pub work_dir: PathBuf,
    pub yt_dlp_path: String,
    pub gallery_dl_path: String,
    pub cookies_file_path: Option<PathBuf>,

    // Archive Policy
    pub archive_mode: ArchiveMode,
    pub archive_quote_only_links: bool,

    // Web Server
    pub web_host: String,
    pub web_port: u16,

    // TLS / Let's Encrypt
    pub tls_enabled: bool,
    pub tls_domains: Vec<String>,
    pub tls_contact_email: Option<String>,
    pub tls_cache_dir: PathBuf,
    pub tls_use_staging: bool,
    pub tls_https_port: u16,

    // Wayback Machine
    pub wayback_enabled: bool,
    pub wayback_rate_limit_per_min: u32,

    // Archive.today
    pub archive_today_enabled: bool,
    pub archive_today_rate_limit_per_min: u32,

    // Backup
    pub backup_enabled: bool,
    pub backup_interval_hours: u64,
    pub backup_retention_count: usize,

    // Logging
    pub log_format: LogFormat,

    // IPFS
    pub ipfs_enabled: bool,
    pub ipfs_api_url: String,
    pub ipfs_gateway_urls: Vec<String>,

    // Manual Submission
    pub submission_enabled: bool,
    pub submission_rate_limit_per_hour: u32,
}

/// Log output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogFormat {
    /// Pretty-printed human-readable logs (default)
    #[default]
    Pretty,
    /// Structured JSON logs for production
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveMode {
    /// Only archive content from sites known for deletable content
    Deletable,
    /// Archive all external links
    All,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// # Errors
    ///
    /// Returns an error if required environment variables are missing or invalid.
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            // RSS Feed
            rss_url: required_env("RSS_URL")?,
            poll_interval: Duration::from_secs(parse_env_u64("POLL_INTERVAL_SECS", 60)?),
            cache_window: Duration::from_secs(parse_env_u64("CACHE_WINDOW_SECS", 3600)?),

            // Database
            database_path: PathBuf::from(env_or_default("DATABASE_PATH", "./data/archive.sqlite")),

            // S3 Storage
            s3_bucket: required_env("S3_BUCKET")?,
            s3_region: env_or_default("S3_REGION", "us-east-1"),
            s3_endpoint: optional_env("S3_ENDPOINT"),
            s3_prefix: env_or_default("S3_PREFIX", "archives/"),

            // Archive Workers
            worker_concurrency: parse_env_usize("WORKER_CONCURRENCY", 4)?,
            per_domain_concurrency: parse_env_usize("PER_DOMAIN_CONCURRENCY", 1)?,
            work_dir: PathBuf::from(env_or_default("WORK_DIR", "./data/tmp")),
            yt_dlp_path: env_or_default("YT_DLP_PATH", "yt-dlp"),
            gallery_dl_path: env_or_default("GALLERY_DL_PATH", "gallery-dl"),
            cookies_file_path: optional_env("COOKIES_FILE_PATH").map(PathBuf::from),

            // Archive Policy
            archive_mode: parse_archive_mode(&env_or_default("ARCHIVE_MODE", "deletable"))?,
            archive_quote_only_links: parse_env_bool("ARCHIVE_QUOTE_ONLY_LINKS", false)?,

            // Web Server
            web_host: env_or_default("WEB_HOST", "0.0.0.0"),
            web_port: parse_env_u16("WEB_PORT", 8080)?,

            // TLS / Let's Encrypt
            tls_enabled: parse_env_bool("TLS_ENABLED", false)?,
            tls_domains: parse_domain_list(&env_or_default("TLS_DOMAINS", "")),
            tls_contact_email: optional_env("TLS_CONTACT_EMAIL"),
            tls_cache_dir: PathBuf::from(env_or_default("TLS_CACHE_DIR", "./data/acme_cache")),
            tls_use_staging: parse_env_bool("TLS_USE_STAGING", false)?,
            tls_https_port: parse_env_u16("TLS_HTTPS_PORT", 443)?,

            // Wayback Machine
            wayback_enabled: parse_env_bool("WAYBACK_ENABLED", true)?,
            wayback_rate_limit_per_min: parse_env_u32("WAYBACK_RATE_LIMIT_PER_MIN", 5)?,

            // Archive.today
            archive_today_enabled: parse_env_bool("ARCHIVE_TODAY_ENABLED", false)?,
            archive_today_rate_limit_per_min: parse_env_u32("ARCHIVE_TODAY_RATE_LIMIT_PER_MIN", 3)?,

            // Backup
            backup_enabled: parse_env_bool("BACKUP_ENABLED", true)?,
            backup_interval_hours: parse_env_u64("BACKUP_INTERVAL_HOURS", 24)?,
            backup_retention_count: parse_env_usize("BACKUP_RETENTION_COUNT", 30)?,

            // Logging
            log_format: parse_log_format(&env_or_default("LOG_FORMAT", "pretty"))?,

            // IPFS
            ipfs_enabled: parse_env_bool("IPFS_ENABLED", false)?,
            ipfs_api_url: env_or_default("IPFS_API_URL", "http://127.0.0.1:5001"),
            ipfs_gateway_urls: parse_gateway_urls(&env_or_default(
                "IPFS_GATEWAY_URLS",
                "https://ipfs.io/ipfs/,https://cloudflare-ipfs.com/ipfs/,https://dweb.link/ipfs/",
            )),

            // Manual Submission
            submission_enabled: parse_env_bool("SUBMISSION_ENABLED", true)?,
            submission_rate_limit_per_hour: parse_env_u32("SUBMISSION_RATE_LIMIT_PER_HOUR", 60)?,
        })
    }

    /// Validate that the configuration is usable.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.worker_concurrency == 0 {
            return Err(ConfigError::InvalidValue {
                name: "WORKER_CONCURRENCY".to_string(),
                message: "must be at least 1".to_string(),
            });
        }
        if self.per_domain_concurrency == 0 {
            return Err(ConfigError::InvalidValue {
                name: "PER_DOMAIN_CONCURRENCY".to_string(),
                message: "must be at least 1".to_string(),
            });
        }
        if self.rss_url.is_empty() {
            return Err(ConfigError::InvalidValue {
                name: "RSS_URL".to_string(),
                message: "cannot be empty".to_string(),
            });
        }
        if self.s3_bucket.is_empty() {
            return Err(ConfigError::InvalidValue {
                name: "S3_BUCKET".to_string(),
                message: "cannot be empty".to_string(),
            });
        }
        if self.tls_enabled && self.tls_domains.is_empty() {
            return Err(ConfigError::InvalidValue {
                name: "TLS_DOMAINS".to_string(),
                message: "at least one domain required when TLS is enabled".to_string(),
            });
        }
        Ok(())
    }
}

fn required_env(name: &str) -> Result<String, ConfigError> {
    std::env::var(name).map_err(|_| ConfigError::MissingEnvVar(name.to_string()))
}

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

fn env_or_default(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn parse_env_u64(name: &str, default: u64) -> Result<u64, ConfigError> {
    match std::env::var(name) {
        Ok(val) if !val.is_empty() => val.parse().map_err(|e| ConfigError::ParseInt {
            name: name.to_string(),
            source: e,
        }),
        _ => Ok(default),
    }
}

fn parse_env_u32(name: &str, default: u32) -> Result<u32, ConfigError> {
    match std::env::var(name) {
        Ok(val) if !val.is_empty() => val.parse().map_err(|e| ConfigError::ParseInt {
            name: name.to_string(),
            source: e,
        }),
        _ => Ok(default),
    }
}

fn parse_env_u16(name: &str, default: u16) -> Result<u16, ConfigError> {
    match std::env::var(name) {
        Ok(val) if !val.is_empty() => val.parse().map_err(|e| ConfigError::ParseInt {
            name: name.to_string(),
            source: e,
        }),
        _ => Ok(default),
    }
}

fn parse_env_usize(name: &str, default: usize) -> Result<usize, ConfigError> {
    match std::env::var(name) {
        Ok(val) if !val.is_empty() => val.parse().map_err(|e| ConfigError::ParseInt {
            name: name.to_string(),
            source: e,
        }),
        _ => Ok(default),
    }
}

fn parse_env_bool(name: &str, default: bool) -> Result<bool, ConfigError> {
    match std::env::var(name) {
        Ok(val) if !val.is_empty() => match val.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" => Ok(false),
            _ => Err(ConfigError::ParseBool {
                name: name.to_string(),
                value: val,
            }),
        },
        _ => Ok(default),
    }
}

fn parse_archive_mode(value: &str) -> Result<ArchiveMode, ConfigError> {
    match value.to_lowercase().as_str() {
        "deletable" => Ok(ArchiveMode::Deletable),
        "all" => Ok(ArchiveMode::All),
        _ => Err(ConfigError::InvalidValue {
            name: "ARCHIVE_MODE".to_string(),
            message: format!("must be 'deletable' or 'all', got '{value}'"),
        }),
    }
}

fn parse_log_format(value: &str) -> Result<LogFormat, ConfigError> {
    match value.to_lowercase().as_str() {
        "pretty" | "text" | "human" => Ok(LogFormat::Pretty),
        "json" | "structured" => Ok(LogFormat::Json),
        _ => Err(ConfigError::InvalidValue {
            name: "LOG_FORMAT".to_string(),
            message: format!("must be 'pretty' or 'json', got '{value}'"),
        }),
    }
}

fn parse_gateway_urls(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn parse_domain_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_archive_mode() {
        assert_eq!(
            parse_archive_mode("deletable").unwrap(),
            ArchiveMode::Deletable
        );
        assert_eq!(
            parse_archive_mode("DELETABLE").unwrap(),
            ArchiveMode::Deletable
        );
        assert_eq!(parse_archive_mode("all").unwrap(), ArchiveMode::All);
        assert_eq!(parse_archive_mode("ALL").unwrap(), ArchiveMode::All);
        assert!(parse_archive_mode("invalid").is_err());
    }

    #[test]
    fn test_parse_bool() {
        assert!(parse_env_bool("NONEXISTENT_VAR", true).unwrap());
        assert!(!parse_env_bool("NONEXISTENT_VAR", false).unwrap());
    }
}
