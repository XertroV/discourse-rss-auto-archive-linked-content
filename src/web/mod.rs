mod routes;
mod templates;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::config::Config;
use crate::db::Database;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub config: Arc<Config>,
}

/// Start the web server.
///
/// # Errors
///
/// Returns an error if the server fails to start.
pub async fn serve(config: Config, db: Database) -> Result<()> {
    let addr: SocketAddr = format!("{}:{}", config.web_host, config.web_port)
        .parse()
        .context("Invalid web server address")?;

    let state = AppState {
        db,
        config: Arc::new(config),
    };

    // Determine static files directory
    let static_dir = find_static_dir();
    info!(static_dir = ?static_dir, "Serving static files");

    let app = Router::new()
        .merge(routes::router())
        .nest_service("/static", ServeDir::new(&static_dir))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    info!(addr = %addr, "Starting web server");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("Failed to bind web server")?;

    axum::serve(listener, app)
        .await
        .context("Web server error")?;

    Ok(())
}

/// Find the static files directory.
///
/// Checks in order:
/// 1. ./static (development)
/// 2. /usr/share/discourse-link-archiver/static (installed)
/// 3. Falls back to ./static
fn find_static_dir() -> PathBuf {
    let candidates = [
        PathBuf::from("./static"),
        PathBuf::from("/usr/share/discourse-link-archiver/static"),
    ];

    for path in &candidates {
        if path.exists() && path.is_dir() {
            return path.clone();
        }
    }

    // Default fallback
    PathBuf::from("./static")
}
