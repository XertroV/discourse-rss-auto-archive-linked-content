mod diff;
mod feeds;
mod routes;
pub mod templates;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::Host;
use axum::handler::HandlerWithoutStateExt;
use axum::http::Uri;
use axum::response::Redirect;
use axum::Router;
use futures_util::StreamExt;
use rustls_acme::AcmeState;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::{error, info};

use crate::config::Config;
use crate::db::Database;
use crate::s3::S3Client;
use crate::tls;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub config: Arc<Config>,
    pub s3: S3Client,
}

/// Start the web server.
///
/// When TLS is enabled, this starts both an HTTP server (for redirects) and
/// an HTTPS server with automatic Let's Encrypt certificate management.
///
/// # Errors
///
/// Returns an error if the server fails to start.
pub async fn serve(config: Config, db: Database) -> Result<()> {
    if config.tls_enabled {
        serve_with_tls(config, db).await
    } else {
        serve_http_only(config, db).await
    }
}

/// Serve HTTP only (no TLS).
async fn serve_http_only(config: Config, db: Database) -> Result<()> {
    let addr: SocketAddr = format!("{}:{}", config.web_host, config.web_port)
        .parse()
        .context("Invalid web server address")?;

    let s3_client = S3Client::new(&config)
        .await
        .context("Failed to initialize S3 client")?;

    let state = AppState {
        db,
        config: Arc::new(config),
        s3: s3_client,
    };

    let app = create_app(state);

    info!(addr = %addr, "Starting HTTP web server");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("Failed to bind web server")?;

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .context("Web server error")?;

    Ok(())
}

/// Serve with TLS using automatic Let's Encrypt certificates.
async fn serve_with_tls(config: Config, db: Database) -> Result<()> {
    let http_addr: SocketAddr = format!("{}:{}", config.web_host, config.web_port)
        .parse()
        .context("Invalid HTTP address")?;

    let https_addr: SocketAddr = format!("{}:{}", config.web_host, config.tls_https_port)
        .parse()
        .context("Invalid HTTPS address")?;

    let https_port = config.tls_https_port;

    // Create ACME configuration for automatic certificate management
    let acme_config = tls::create_acme_config(&config)?;
    let mut acme_state = acme_config.state();
    let acceptor = acme_state.axum_acceptor(acme_state.default_rustls_config());

    let s3_client = S3Client::new(&config)
        .await
        .context("Failed to initialize S3 client")?;

    let state = AppState {
        db,
        config: Arc::new(config),
        s3: s3_client,
    };

    let app = create_app(state);

    // Spawn task to log certificate events
    tokio::spawn(async move {
        log_acme_events(&mut acme_state).await;
    });

    // Spawn HTTP server for redirects to HTTPS
    tokio::spawn(async move {
        if let Err(e) = serve_http_redirect(http_addr, https_port).await {
            error!("HTTP redirect server error: {e:#}");
        }
    });

    info!(addr = %https_addr, "Starting HTTPS web server with Let's Encrypt");

    // Start HTTPS server with ACME acceptor
    axum_server::bind(https_addr)
        .acceptor(acceptor)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .context("HTTPS server error")?;

    Ok(())
}

/// Serve HTTP redirect to HTTPS.
async fn serve_http_redirect(addr: SocketAddr, https_port: u16) -> Result<()> {
    info!(addr = %addr, "Starting HTTP redirect server");

    let redirect = move |Host(host): Host, uri: Uri| async move {
        // Extract hostname without port
        let host = host.split(':').next().unwrap_or(&host);

        // Build HTTPS URL
        let https_uri = if https_port == 443 {
            format!(
                "https://{host}{}",
                uri.path_and_query().map_or("", |p| p.as_str())
            )
        } else {
            format!(
                "https://{host}:{}{}",
                https_port,
                uri.path_and_query().map_or("", |p| p.as_str())
            )
        };

        Redirect::permanent(&https_uri)
    };

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("Failed to bind HTTP redirect server")?;

    axum::serve(listener, redirect.into_make_service())
        .await
        .context("HTTP redirect server error")?;

    Ok(())
}

/// Log ACME certificate events.
async fn log_acme_events<EC, EA>(state: &mut AcmeState<EC, EA>)
where
    EC: std::fmt::Debug + 'static,
    EA: std::fmt::Debug + 'static,
{
    loop {
        match state.next().await {
            Some(Ok(ok)) => info!("ACME event: {ok:?}"),
            Some(Err(err)) => error!("ACME error: {err:?}"),
            None => break,
        }
    }
}

/// Create the main application router.
fn create_app(state: AppState) -> Router {
    // Determine static files directory
    let static_dir = find_static_dir();
    info!(static_dir = ?static_dir, "Serving static files");

    Router::new()
        .merge(routes::router())
        .nest_service("/static", ServeDir::new(&static_dir))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
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
