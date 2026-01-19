//! Screenshot capture module using headless Chrome/Chromium.
//!
//! This module provides functionality to capture full-page screenshots
//! of web pages using a headless browser.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::page::ScreenshotParams;
use futures_util::StreamExt;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// Default viewport width in pixels.
pub const DEFAULT_VIEWPORT_WIDTH: u32 = 1280;

/// Default viewport height in pixels.
pub const DEFAULT_VIEWPORT_HEIGHT: u32 = 800;

/// Default page load timeout in seconds.
pub const DEFAULT_PAGE_TIMEOUT_SECS: u64 = 30;

/// Screenshot capture configuration.
#[derive(Debug, Clone)]
pub struct ScreenshotConfig {
    /// Viewport width in pixels.
    pub viewport_width: u32,
    /// Viewport height in pixels.
    pub viewport_height: u32,
    /// Page load timeout.
    pub page_timeout: Duration,
    /// Path to Chrome/Chromium executable (None for auto-detection).
    pub chrome_path: Option<String>,
    /// Whether screenshot capture is enabled.
    pub enabled: bool,
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            viewport_width: DEFAULT_VIEWPORT_WIDTH,
            viewport_height: DEFAULT_VIEWPORT_HEIGHT,
            page_timeout: Duration::from_secs(DEFAULT_PAGE_TIMEOUT_SECS),
            chrome_path: None,
            enabled: false,
        }
    }
}

/// Screenshot capture service.
///
/// Manages a headless browser instance for capturing screenshots.
/// The browser is lazily initialized on first use.
pub struct ScreenshotService {
    config: ScreenshotConfig,
    browser: Arc<Mutex<Option<Browser>>>,
}

impl ScreenshotService {
    /// Create a new screenshot service.
    #[must_use]
    pub fn new(config: ScreenshotConfig) -> Self {
        Self {
            config,
            browser: Arc::new(Mutex::new(None)),
        }
    }

    /// Check if screenshot capture is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Initialize the browser if not already running.
    async fn ensure_browser(&self) -> Result<()> {
        let mut browser_guard = self.browser.lock().await;
        if browser_guard.is_some() {
            return Ok(());
        }

        info!("Initializing headless browser for screenshots");

        let mut config_builder = BrowserConfig::builder()
            .window_size(self.config.viewport_width, self.config.viewport_height)
            .request_timeout(self.config.page_timeout)
            .no_sandbox()
            .disable_default_args()
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--disable-dev-shm-usage")
            .arg("--disable-software-rasterizer")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-background-networking")
            .arg("--disable-extensions")
            .arg("--disable-sync")
            .arg("--disable-translate")
            .arg("--mute-audio")
            .arg("--hide-scrollbars");

        if let Some(ref chrome_path) = self.config.chrome_path {
            config_builder = config_builder.chrome_executable(chrome_path);
        }

        let browser_config = config_builder
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build browser config: {e}"))?;

        let (browser, mut handler) = Browser::launch(browser_config)
            .await
            .context("Failed to launch browser")?;

        // Spawn handler in background
        tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(e) = event {
                    debug!("Browser handler error: {e}");
                }
            }
        });

        *browser_guard = Some(browser);
        info!("Headless browser initialized");

        Ok(())
    }

    /// Capture a screenshot of the given URL.
    ///
    /// Returns the screenshot as PNG bytes.
    pub async fn capture(&self, url: &str) -> Result<Vec<u8>> {
        if !self.config.enabled {
            anyhow::bail!("Screenshot capture is disabled");
        }

        self.ensure_browser().await?;

        let browser_guard = self.browser.lock().await;
        let browser = browser_guard.as_ref().context("Browser not initialized")?;

        debug!(url = %url, "Capturing screenshot");

        // Create a new page
        let page = browser
            .new_page(url)
            .await
            .context("Failed to create new page")?;

        // Wait for the page to load
        page.wait_for_navigation()
            .await
            .context("Navigation timeout")?;

        // Give the page a bit more time to render dynamic content
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Capture full page screenshot using the high-level API
        let screenshot_params = ScreenshotParams::builder().full_page(true).build();

        let png_data = page
            .screenshot(screenshot_params)
            .await
            .context("Failed to capture screenshot")?;

        // Close the page
        if let Err(e) = page.close().await {
            warn!("Failed to close page: {e}");
        }

        debug!(url = %url, size = png_data.len(), "Screenshot captured");

        Ok(png_data)
    }

    /// Capture a screenshot and save it to a file.
    pub async fn capture_to_file(&self, url: &str, output_path: &Path) -> Result<()> {
        let png_data = self.capture(url).await?;
        tokio::fs::write(output_path, &png_data)
            .await
            .with_context(|| format!("Failed to write screenshot to {}", output_path.display()))?;
        Ok(())
    }

    /// Shutdown the browser gracefully.
    pub async fn shutdown(&self) {
        let mut browser_guard = self.browser.lock().await;
        if let Some(mut browser) = browser_guard.take() {
            if let Err(e) = browser.close().await {
                error!("Failed to close browser: {e}");
            } else {
                info!("Browser shutdown complete");
            }
        }
    }
}

impl Drop for ScreenshotService {
    fn drop(&mut self) {
        // Note: We can't do async cleanup in Drop, but the browser process
        // will be killed when the Browser struct is dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ScreenshotConfig::default();
        assert_eq!(config.viewport_width, DEFAULT_VIEWPORT_WIDTH);
        assert_eq!(config.viewport_height, DEFAULT_VIEWPORT_HEIGHT);
        assert!(!config.enabled);
    }

    #[test]
    fn test_service_disabled_by_default() {
        let config = ScreenshotConfig::default();
        let service = ScreenshotService::new(config);
        assert!(!service.is_enabled());
    }

    #[test]
    fn test_service_enabled() {
        let config = ScreenshotConfig {
            enabled: true,
            ..Default::default()
        };
        let service = ScreenshotService::new(config);
        assert!(service.is_enabled());
    }
}
