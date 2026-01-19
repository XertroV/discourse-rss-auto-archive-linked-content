use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use discourse_link_archiver::archiver::ArchiveWorker;
use discourse_link_archiver::backup::BackupManager;
use discourse_link_archiver::config::Config;
use discourse_link_archiver::db::Database;
use discourse_link_archiver::ipfs::IpfsClient;
use discourse_link_archiver::s3::S3Client;
use discourse_link_archiver::{rss, web};

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        error!("Fatal error: {e:#}");
        std::process::exit(1);
    }
}

#[allow(clippy::too_many_lines)]
async fn run() -> Result<()> {
    // Load .env file if present
    let _ = dotenvy::dotenv();

    // Initialize logging
    init_tracing()?;

    info!("Starting discourse-link-archiver");

    // Load and validate configuration
    let config = Config::from_env().context("Failed to load configuration")?;
    config.validate().context("Invalid configuration")?;

    info!(rss_url = %config.rss_url, "Configuration loaded");

    // Log cookie configuration status
    match (
        config.yt_dlp_cookies_from_browser.as_deref(),
        config.cookies_file_path.as_deref(),
    ) {
        (Some(browser_profile), Some(cookies_path)) => {
            warn!(
                spec = %browser_profile,
                cookies_path = %cookies_path.display(),
                "Both YT_DLP_COOKIES_FROM_BROWSER and COOKIES_FILE_PATH are set; yt-dlp will use cookies-from-browser and ignore cookies.txt (gallery-dl will still use cookies.txt if present)."
            );
        }
        (Some(browser_profile), None) => {
            info!(spec = %browser_profile, "yt-dlp cookies-from-browser enabled");
        }
        (None, Some(cookies_path)) => {
            if cookies_path.exists() {
                info!(path = %cookies_path.display(), "Cookies file configured and found");
            } else {
                warn!(path = %cookies_path.display(), "Cookies file configured but not found - will not be used until created");
            }
        }
        (None, None) => {
            warn!("No cookies configured - authenticated downloads may fail");
        }
    }

    // If cookies-from-browser is enabled, best-effort warn when the profile path doesn't exist yet.
    if let Some(spec) = config.yt_dlp_cookies_from_browser.as_deref() {
        if let Some((_, rest)) = spec.split_once(':') {
            let profile = rest.split("::").next().unwrap_or("");
            if !profile.is_empty() {
                let profile_path = std::path::Path::new(profile);
                if profile_path.is_absolute() && !profile_path.exists() {
                    warn!(path = %profile_path.display(), "yt-dlp cookies-from-browser profile path does not exist (yet)");
                }
            }
        }
    }

    // Ensure data directories exist
    tokio::fs::create_dir_all(&config.work_dir)
        .await
        .with_context(|| {
            format!(
                "Failed to create work directory: {}",
                config.work_dir.display()
            )
        })?;

    if let Some(parent) = config.database_path.parent() {
        tokio::fs::create_dir_all(parent).await.with_context(|| {
            format!("Failed to create database directory: {}", parent.display())
        })?;
    }

    // Initialize database
    let db = Database::new(&config.database_path)
        .await
        .context("Failed to initialize database")?;

    info!("Database initialized");

    // Initialize S3 client
    let s3_client = S3Client::new(&config)
        .await
        .context("Failed to initialize S3 client")?;

    // Initialize IPFS client
    let ipfs_client = IpfsClient::new(&config);
    if ipfs_client.is_enabled() {
        info!(api_url = %config.ipfs_api_url, "IPFS pinning enabled");
        if let Ok(healthy) = ipfs_client.health_check().await {
            if healthy {
                info!("IPFS daemon is reachable");
            } else {
                info!("IPFS daemon not reachable, will retry on each pin");
            }
        }
    } else {
        info!("IPFS pinning disabled");
    }

    // Start backup scheduler if enabled
    let backup_handle = if config.backup_enabled {
        let backup_manager = BackupManager::new(&config, s3_client.clone());
        let interval = Duration::from_secs(config.backup_interval_hours * 3600);
        info!(
            interval_hours = config.backup_interval_hours,
            retention = config.backup_retention_count,
            "Backup scheduler enabled"
        );
        Some(tokio::spawn(async move {
            backup_manager.run_loop(interval).await;
        }))
    } else {
        info!("Backup scheduler disabled");
        None
    };

    // Start archive worker in background
    let worker_config = config.clone();
    let worker_db = db.clone();
    let worker_s3 = s3_client.clone();
    let worker_ipfs = ipfs_client;
    let worker = ArchiveWorker::new(worker_config, worker_db, worker_s3, worker_ipfs);

    // Recover from any interrupted processing on startup
    if let Err(e) = worker.recover_on_startup().await {
        error!("Failed to recover archives on startup: {e:#}");
    }

    let worker_handle = tokio::spawn(async move {
        worker.run().await;
    });
    info!("Archive worker started");

    // Start web server in background
    let web_config = config.clone();
    let web_db = db.clone();
    let web_s3 = s3_client;
    let web_handle = tokio::spawn(async move {
        if let Err(e) = web::serve(web_config, web_db, web_s3).await {
            error!("Web server error: {e:#}");
        }
    });

    // Start RSS polling loop
    let poll_handle = tokio::spawn(async move {
        rss::poll_loop(config, db).await;
    });

    // Wait for shutdown signal
    shutdown_signal().await;

    info!("Shutting down...");

    // Cancel tasks
    web_handle.abort();
    poll_handle.abort();
    worker_handle.abort();
    if let Some(handle) = backup_handle {
        handle.abort();
    }

    info!("Shutdown complete");

    Ok(())
}

fn init_tracing() -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,discourse_link_archiver=debug"));

    // Check if JSON logging is requested
    let use_json = std::env::var("LOG_FORMAT")
        .map(|v| matches!(v.to_lowercase().as_str(), "json" | "structured"))
        .unwrap_or(false);

    if use_json {
        // Structured JSON logging for production
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().json())
            .try_init()
            .map_err(|e| anyhow::anyhow!("Failed to initialize tracing: {e}"))?;
    } else {
        // Pretty-printed logging for development
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .try_init()
            .map_err(|e| anyhow::anyhow!("Failed to initialize tracing: {e}"))?;
    }

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
