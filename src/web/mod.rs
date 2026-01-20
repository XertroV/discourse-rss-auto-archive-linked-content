mod auth;
mod diff;
pub mod export;
mod feeds;
mod routes;
pub mod templates;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::ConnectInfo;
use axum::extract::{FromRef, Host};
use axum::handler::HandlerWithoutStateExt;
use axum::http::header::HeaderValue;
use axum::http::Request;
use axum::http::Uri;
use axum::middleware::Next;
use axum::response::{Redirect, Response};
use axum::Router;
use futures_util::StreamExt;
use rustls_acme::AcmeState;
use sqlx::SqlitePool;
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
    pub s3: Arc<S3Client>,
}

// Implement FromRef for SqlitePool to enable auth extractors
impl FromRef<AppState> for SqlitePool {
    fn from_ref(state: &AppState) -> SqlitePool {
        state.db.pool().clone()
    }
}

/// Start the web server.
///
/// When TLS is enabled, this starts both an HTTP server (for redirects) and
/// an HTTPS server with automatic Let's Encrypt certificate management.
///
/// # Errors
///
/// Returns an error if the server fails to start.
pub async fn serve(config: Config, db: Database, s3: S3Client) -> Result<()> {
    if config.tls_enabled {
        serve_with_tls(config, db, s3).await
    } else {
        serve_http_only(config, db, s3).await
    }
}

/// Serve HTTP only (no TLS).
async fn serve_http_only(config: Config, db: Database, s3: S3Client) -> Result<()> {
    let addr: SocketAddr = format!("{}:{}", config.web_host, config.web_port)
        .parse()
        .context("Invalid web server address")?;

    let state = AppState {
        db,
        config: Arc::new(config),
        s3: Arc::new(s3),
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
async fn serve_with_tls(config: Config, db: Database, s3: S3Client) -> Result<()> {
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

    let state = AppState {
        db,
        config: Arc::new(config),
        s3: Arc::new(s3),
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

/// Add X-No-Archive header to all responses to signal archiving prevention.
async fn add_no_archive_header(req: Request<axum::body::Body>, next: Next) -> Response {
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert("X-No-Archive", HeaderValue::from_static("1"));
    response
}

/// Create the main application router.
fn create_app(state: AppState) -> Router {
    // Determine static files directory
    let static_dir = find_static_dir();
    info!(static_dir = ?static_dir, "Serving static files");

    Router::new()
        .merge(routes::router())
        .nest_service("/static", ServeDir::new(&static_dir))
        .layer(axum::middleware::from_fn(add_no_archive_header))
        .layer(CompressionLayer::new())
        .layer(
            TraceLayer::new_for_http().make_span_with(|req: &Request<_>| {
                let client_ip = best_effort_client_ip(req).unwrap_or_else(|| "unknown".to_string());
                let user_agent = req
                    .headers()
                    .get(axum::http::header::USER_AGENT)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");

                tracing::info_span!(
                    "http_request",
                    method = %req.method(),
                    uri = %req.uri(),
                    client_ip = %client_ip,
                    user_agent = %user_agent,
                )
            }),
        )
        .with_state(state)
}

fn best_effort_client_ip<B>(req: &Request<B>) -> Option<String> {
    // Prefer proxy headers if present.
    if let Some(v) = req.headers().get("forwarded").and_then(|v| v.to_str().ok()) {
        if let Some(ip) = parse_forwarded_for(v) {
            return Some(ip);
        }
    }

    if let Some(v) = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(first) = v.split(',').next().map(str::trim).filter(|s| !s.is_empty()) {
            return Some(strip_port_and_brackets(first));
        }
    }

    if let Some(v) = req.headers().get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let v = v.trim();
        if !v.is_empty() {
            return Some(strip_port_and_brackets(v));
        }
    }

    // Fall back to connect info (remote socket address).
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
}

fn parse_forwarded_for(header_value: &str) -> Option<String> {
    // Forwarded: for=1.2.3.4;proto=https;by=...
    // Forwarded: for="[2001:db8::1]:1234";proto=https
    // We take the first `for=` value in the first element.
    let first_elem = header_value.split(',').next()?.trim();
    for part in first_elem.split(';') {
        let part = part.trim();
        let Some(rest) = part.strip_prefix("for=") else {
            continue;
        };
        let rest = rest.trim().trim_matches('"');
        if rest.is_empty() {
            continue;
        }
        return Some(strip_port_and_brackets(rest));
    }
    None
}

fn strip_port_and_brackets(s: &str) -> String {
    let mut v = s.trim().trim_matches('"').to_string();
    if v.starts_with('[') {
        if let Some(end) = v.find(']') {
            v = v[1..end].to_string();
            return v;
        }
    }

    // IPv4:port or hostname:port => strip port.
    // For raw IPv6 without brackets, we leave it as-is.
    if let Some((host, port)) = v.rsplit_once(':') {
        if !host.contains(':') && port.chars().all(|c| c.is_ascii_digit()) {
            return host.to_string();
        }
    }

    v
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
