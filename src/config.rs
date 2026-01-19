use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required configuration: {0}")]
    MissingRequired(String),
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
    #[error("failed to read config file: {0}")]
    FileRead(#[from] std::io::Error),
    #[error("failed to parse config file: {0}")]
    TomlParse(#[from] toml::de::Error),
}

/// Application configuration loaded from environment variables and/or config file.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
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

    // Screenshot Capture
    pub screenshot_enabled: bool,
    pub screenshot_viewport_width: u32,
    pub screenshot_viewport_height: u32,
    pub screenshot_timeout_secs: u64,
    pub screenshot_chrome_path: Option<String>,
}

/// Configuration file structure (all fields optional, loaded from TOML).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct FileConfig {
    #[serde(default)]
    pub rss: RssConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub s3: S3Config,
    #[serde(default)]
    pub workers: WorkersConfig,
    #[serde(default)]
    pub archive: ArchiveConfig,
    #[serde(default)]
    pub web: WebConfig,
    #[serde(default)]
    pub tls: TlsConfig,
    #[serde(default)]
    pub wayback: WaybackConfig,
    #[serde(default)]
    pub archive_today: ArchiveTodayConfig,
    #[serde(default)]
    pub backup: BackupConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub ipfs: IpfsConfig,
    #[serde(default)]
    pub submission: SubmissionConfig,
    #[serde(default)]
    pub screenshot: ScreenshotCaptureConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RssConfig {
    pub url: Option<String>,
    pub poll_interval_secs: Option<u64>,
    pub cache_window_secs: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct S3Config {
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WorkersConfig {
    pub concurrency: Option<usize>,
    pub per_domain_concurrency: Option<usize>,
    pub work_dir: Option<String>,
    pub yt_dlp_path: Option<String>,
    pub gallery_dl_path: Option<String>,
    pub cookies_file_path: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ArchiveConfig {
    pub mode: Option<String>,
    pub quote_only_links: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WebConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TlsConfig {
    pub enabled: Option<bool>,
    pub domains: Option<Vec<String>>,
    pub contact_email: Option<String>,
    pub cache_dir: Option<String>,
    pub use_staging: Option<bool>,
    pub https_port: Option<u16>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WaybackConfig {
    pub enabled: Option<bool>,
    pub rate_limit_per_min: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ArchiveTodayConfig {
    pub enabled: Option<bool>,
    pub rate_limit_per_min: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct BackupConfig {
    pub enabled: Option<bool>,
    pub interval_hours: Option<u64>,
    pub retention_count: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub format: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct IpfsConfig {
    pub enabled: Option<bool>,
    pub api_url: Option<String>,
    pub gateway_urls: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SubmissionConfig {
    pub enabled: Option<bool>,
    pub rate_limit_per_hour: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ScreenshotCaptureConfig {
    pub enabled: Option<bool>,
    pub viewport_width: Option<u32>,
    pub viewport_height: Option<u32>,
    pub timeout_secs: Option<u64>,
    pub chrome_path: Option<String>,
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

impl FileConfig {
    /// Load configuration from a TOML file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: FileConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Try to load configuration from the default config file path.
    /// Returns `None` if the file doesn't exist, or an error if it exists but can't be parsed.
    pub fn try_default() -> Result<Option<Self>, ConfigError> {
        let default_paths = [
            "config.toml",
            "./config.toml",
            "/etc/discourse-link-archiver/config.toml",
        ];

        for path_str in default_paths {
            let path = Path::new(path_str);
            if path.exists() {
                return Self::from_file(path).map(Some);
            }
        }

        Ok(None)
    }
}

impl Config {
    /// Load configuration from environment variables only.
    ///
    /// # Errors
    ///
    /// Returns an error if required environment variables are missing or invalid.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::load_with_file(None)
    }

    /// Load configuration from a TOML file with environment variable overrides.
    ///
    /// Environment variables always take precedence over file values.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read/parsed or required config is missing.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let file_config = FileConfig::from_file(path)?;
        Self::load_with_file(Some(file_config))
    }

    /// Load configuration, trying default config file locations first.
    ///
    /// Checks for config.toml in the current directory first, then falls back
    /// to environment variables for any missing values.
    ///
    /// # Errors
    ///
    /// Returns an error if configuration is invalid or required values are missing.
    pub fn load() -> Result<Self, ConfigError> {
        let file_config = FileConfig::try_default()?;
        Self::load_with_file(file_config)
    }

    /// Internal: Load configuration with optional file config.
    fn load_with_file(file_config: Option<FileConfig>) -> Result<Self, ConfigError> {
        let fc = file_config.unwrap_or_default();

        // Helper to get value from env first, then file, then default
        let get_string = |env_name: &str, file_val: Option<String>, default: &str| -> String {
            optional_env(env_name)
                .or(file_val)
                .unwrap_or_else(|| default.to_string())
        };

        let get_string_required =
            |env_name: &str, file_val: Option<String>| -> Result<String, ConfigError> {
                optional_env(env_name)
                    .or(file_val)
                    .ok_or_else(|| ConfigError::MissingRequired(env_name.to_string()))
            };

        Ok(Self {
            // RSS Feed
            rss_url: get_string_required("RSS_URL", fc.rss.url)?,
            poll_interval: Duration::from_secs(parse_env_u64(
                "POLL_INTERVAL_SECS",
                fc.rss.poll_interval_secs.unwrap_or(60),
            )?),
            cache_window: Duration::from_secs(parse_env_u64(
                "CACHE_WINDOW_SECS",
                fc.rss.cache_window_secs.unwrap_or(3600),
            )?),

            // Database
            database_path: PathBuf::from(get_string(
                "DATABASE_PATH",
                fc.database.path,
                "./data/archive.sqlite",
            )),

            // S3 Storage
            s3_bucket: get_string_required("S3_BUCKET", fc.s3.bucket)?,
            s3_region: get_string("S3_REGION", fc.s3.region, "us-east-1"),
            s3_endpoint: optional_env("S3_ENDPOINT").or(fc.s3.endpoint),
            s3_prefix: get_string("S3_PREFIX", fc.s3.prefix, "archives/"),

            // Archive Workers
            worker_concurrency: parse_env_usize(
                "WORKER_CONCURRENCY",
                fc.workers.concurrency.unwrap_or(4),
            )?,
            per_domain_concurrency: parse_env_usize(
                "PER_DOMAIN_CONCURRENCY",
                fc.workers.per_domain_concurrency.unwrap_or(1),
            )?,
            work_dir: PathBuf::from(get_string("WORK_DIR", fc.workers.work_dir, "./data/tmp")),
            yt_dlp_path: get_string("YT_DLP_PATH", fc.workers.yt_dlp_path, "yt-dlp"),
            gallery_dl_path: get_string(
                "GALLERY_DL_PATH",
                fc.workers.gallery_dl_path,
                "gallery-dl",
            ),
            cookies_file_path: optional_env("COOKIES_FILE_PATH")
                .or(fc.workers.cookies_file_path)
                .map(PathBuf::from),

            // Archive Policy
            archive_mode: parse_archive_mode(&get_string(
                "ARCHIVE_MODE",
                fc.archive.mode,
                "deletable",
            ))?,
            archive_quote_only_links: parse_env_bool(
                "ARCHIVE_QUOTE_ONLY_LINKS",
                fc.archive.quote_only_links.unwrap_or(false),
            )?,

            // Web Server
            web_host: get_string("WEB_HOST", fc.web.host, "0.0.0.0"),
            web_port: parse_env_u16("WEB_PORT", fc.web.port.unwrap_or(8080))?,

            // TLS / Let's Encrypt
            tls_enabled: parse_env_bool("TLS_ENABLED", fc.tls.enabled.unwrap_or(false))?,
            tls_domains: optional_env("TLS_DOMAINS")
                .map(|s| parse_domain_list(&s))
                .or(fc.tls.domains)
                .unwrap_or_default(),
            tls_contact_email: optional_env("TLS_CONTACT_EMAIL").or(fc.tls.contact_email),
            tls_cache_dir: PathBuf::from(get_string(
                "TLS_CACHE_DIR",
                fc.tls.cache_dir,
                "./data/acme_cache",
            )),
            tls_use_staging: parse_env_bool(
                "TLS_USE_STAGING",
                fc.tls.use_staging.unwrap_or(false),
            )?,
            tls_https_port: parse_env_u16("TLS_HTTPS_PORT", fc.tls.https_port.unwrap_or(443))?,

            // Wayback Machine
            wayback_enabled: parse_env_bool("WAYBACK_ENABLED", fc.wayback.enabled.unwrap_or(true))?,
            wayback_rate_limit_per_min: parse_env_u32(
                "WAYBACK_RATE_LIMIT_PER_MIN",
                fc.wayback.rate_limit_per_min.unwrap_or(5),
            )?,

            // Archive.today
            archive_today_enabled: parse_env_bool(
                "ARCHIVE_TODAY_ENABLED",
                fc.archive_today.enabled.unwrap_or(false),
            )?,
            archive_today_rate_limit_per_min: parse_env_u32(
                "ARCHIVE_TODAY_RATE_LIMIT_PER_MIN",
                fc.archive_today.rate_limit_per_min.unwrap_or(3),
            )?,

            // Backup
            backup_enabled: parse_env_bool("BACKUP_ENABLED", fc.backup.enabled.unwrap_or(true))?,
            backup_interval_hours: parse_env_u64(
                "BACKUP_INTERVAL_HOURS",
                fc.backup.interval_hours.unwrap_or(24),
            )?,
            backup_retention_count: parse_env_usize(
                "BACKUP_RETENTION_COUNT",
                fc.backup.retention_count.unwrap_or(30),
            )?,

            // Logging
            log_format: parse_log_format(&get_string("LOG_FORMAT", fc.logging.format, "pretty"))?,

            // IPFS
            ipfs_enabled: parse_env_bool("IPFS_ENABLED", fc.ipfs.enabled.unwrap_or(false))?,
            ipfs_api_url: get_string("IPFS_API_URL", fc.ipfs.api_url, "http://127.0.0.1:5001"),
            ipfs_gateway_urls: optional_env("IPFS_GATEWAY_URLS")
                .map(|s| parse_gateway_urls(&s))
                .or(fc.ipfs.gateway_urls)
                .unwrap_or_else(|| {
                    vec![
                        "https://ipfs.io/ipfs/".to_string(),
                        "https://cloudflare-ipfs.com/ipfs/".to_string(),
                        "https://dweb.link/ipfs/".to_string(),
                    ]
                }),

            // Manual Submission
            submission_enabled: parse_env_bool(
                "SUBMISSION_ENABLED",
                fc.submission.enabled.unwrap_or(true),
            )?,
            submission_rate_limit_per_hour: parse_env_u32(
                "SUBMISSION_RATE_LIMIT_PER_HOUR",
                fc.submission.rate_limit_per_hour.unwrap_or(60),
            )?,

            // Screenshot Capture
            screenshot_enabled: parse_env_bool(
                "SCREENSHOT_ENABLED",
                fc.screenshot.enabled.unwrap_or(false),
            )?,
            screenshot_viewport_width: parse_env_u32(
                "SCREENSHOT_VIEWPORT_WIDTH",
                fc.screenshot.viewport_width.unwrap_or(1280),
            )?,
            screenshot_viewport_height: parse_env_u32(
                "SCREENSHOT_VIEWPORT_HEIGHT",
                fc.screenshot.viewport_height.unwrap_or(800),
            )?,
            screenshot_timeout_secs: parse_env_u64(
                "SCREENSHOT_TIMEOUT_SECS",
                fc.screenshot.timeout_secs.unwrap_or(30),
            )?,
            screenshot_chrome_path: optional_env("SCREENSHOT_CHROME_PATH")
                .or(fc.screenshot.chrome_path),
        })
    }

    /// Create a ScreenshotConfig from this config.
    #[must_use]
    pub fn screenshot_config(&self) -> crate::archiver::ScreenshotConfig {
        crate::archiver::ScreenshotConfig {
            viewport_width: self.screenshot_viewport_width,
            viewport_height: self.screenshot_viewport_height,
            page_timeout: std::time::Duration::from_secs(self.screenshot_timeout_secs),
            chrome_path: self.screenshot_chrome_path.clone(),
            enabled: self.screenshot_enabled,
        }
    }

    /// Validate that the configuration is usable.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.worker_concurrency == 0 {
            return Err(ConfigError::InvalidValue {
                name: "worker_concurrency".to_string(),
                message: "must be at least 1".to_string(),
            });
        }
        if self.per_domain_concurrency == 0 {
            return Err(ConfigError::InvalidValue {
                name: "per_domain_concurrency".to_string(),
                message: "must be at least 1".to_string(),
            });
        }
        if self.rss_url.is_empty() {
            return Err(ConfigError::InvalidValue {
                name: "rss_url".to_string(),
                message: "cannot be empty".to_string(),
            });
        }
        if self.s3_bucket.is_empty() {
            return Err(ConfigError::InvalidValue {
                name: "s3_bucket".to_string(),
                message: "cannot be empty".to_string(),
            });
        }
        if self.tls_enabled && self.tls_domains.is_empty() {
            return Err(ConfigError::InvalidValue {
                name: "tls_domains".to_string(),
                message: "at least one domain required when TLS is enabled".to_string(),
            });
        }
        Ok(())
    }
}

fn optional_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
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
            name: "archive_mode".to_string(),
            message: format!("must be 'deletable' or 'all', got '{value}'"),
        }),
    }
}

fn parse_log_format(value: &str) -> Result<LogFormat, ConfigError> {
    match value.to_lowercase().as_str() {
        "pretty" | "text" | "human" => Ok(LogFormat::Pretty),
        "json" | "structured" => Ok(LogFormat::Json),
        _ => Err(ConfigError::InvalidValue {
            name: "log_format".to_string(),
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
    use std::io::Write;
    use tempfile::NamedTempFile;

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

    #[test]
    fn test_file_config_parse() {
        let toml_content = r#"
[rss]
url = "https://example.com/posts.rss"
poll_interval_secs = 120

[database]
path = "./test.sqlite"

[s3]
bucket = "my-bucket"
region = "eu-west-1"

[workers]
concurrency = 8

[web]
port = 9000
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();

        let config = FileConfig::from_file(file.path()).unwrap();

        assert_eq!(
            config.rss.url.as_deref(),
            Some("https://example.com/posts.rss")
        );
        assert_eq!(config.rss.poll_interval_secs, Some(120));
        assert_eq!(config.database.path.as_deref(), Some("./test.sqlite"));
        assert_eq!(config.s3.bucket.as_deref(), Some("my-bucket"));
        assert_eq!(config.s3.region.as_deref(), Some("eu-west-1"));
        assert_eq!(config.workers.concurrency, Some(8));
        assert_eq!(config.web.port, Some(9000));
    }

    #[test]
    fn test_file_config_empty_sections() {
        let toml_content = r#"
[rss]
url = "https://example.com/posts.rss"

[s3]
bucket = "test-bucket"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(toml_content.as_bytes()).unwrap();

        let config = FileConfig::from_file(file.path()).unwrap();

        // Unspecified sections should have default values
        assert_eq!(config.database.path, None);
        assert_eq!(config.workers.concurrency, None);
        assert_eq!(config.backup.enabled, None);
    }
}
