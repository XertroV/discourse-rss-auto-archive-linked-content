mod routes;
mod templates;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Router;
use tower_http::compression::CompressionLayer;
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

    let app = Router::new()
        .merge(routes::router())
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
