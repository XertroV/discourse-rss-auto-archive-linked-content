use anyhow::{Context, Result};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use discourse_link_archiver::config::Config;
use discourse_link_archiver::db::Database;
use discourse_link_archiver::{rss, web};

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        error!("Fatal error: {e:#}");
        std::process::exit(1);
    }
}

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

    // Ensure data directories exist
    tokio::fs::create_dir_all(&config.work_dir)
        .await
        .with_context(|| format!("Failed to create work directory: {:?}", config.work_dir))?;

    if let Some(parent) = config.database_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create database directory: {parent:?}"))?;
    }

    // Initialize database
    let db = Database::new(&config.database_path)
        .await
        .context("Failed to initialize database")?;

    info!("Database initialized");

    // Start web server in background
    let web_config = config.clone();
    let web_db = db.clone();
    let web_handle = tokio::spawn(async move {
        if let Err(e) = web::serve(web_config, web_db).await {
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

    info!("Shutdown complete");

    Ok(())
}

fn init_tracing() -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,discourse_link_archiver=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .map_err(|e| anyhow::anyhow!("Failed to initialize tracing: {e}"))?;

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
