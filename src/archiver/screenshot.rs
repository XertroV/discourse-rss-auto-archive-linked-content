//! Screenshot, PDF, and MHTML capture module using headless Chrome/Chromium.
//!
//! This module provides functionality to capture full-page screenshots,
//! generate PDFs, and create MHTML archives of web pages using a headless browser.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::network::{
    CookieParam, SetCookiesParams, TimeSinceEpoch,
};
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureSnapshotFormat, CaptureSnapshotParams, PrintToPdfParams,
};
use chromiumoxide::page::ScreenshotParams;
use futures_util::StreamExt;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use url::Url;

use crate::chromium_profile::chromium_user_data_and_profile_from_spec;
use crate::fs_utils::copy_dir_best_effort;

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
    /// Optional yt-dlp-style cookies-from-browser spec (e.g. "chromium+basictext:/app/cookies/chromium-profile").
    /// When set, ScreenshotService will launch Chromium with a cloned profile so authenticated pages render.
    pub cookies_from_browser: Option<String>,
    /// Base working directory for storing a cloned Chromium profile.
    pub work_dir: PathBuf,
    /// Whether screenshot capture is enabled.
    pub enabled: bool,
    /// Path to cookies file in Netscape format (for authenticated captures).
    pub cookies_file_path: Option<PathBuf>,
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            viewport_width: DEFAULT_VIEWPORT_WIDTH,
            viewport_height: DEFAULT_VIEWPORT_HEIGHT,
            page_timeout: Duration::from_secs(DEFAULT_PAGE_TIMEOUT_SECS),
            chrome_path: None,
            cookies_from_browser: None,
            work_dir: PathBuf::from("./data/tmp"),
            enabled: false,
            cookies_file_path: None,
        }
    }
}

/// Default PDF paper width in inches (A4).
pub const DEFAULT_PDF_PAPER_WIDTH: f64 = 8.27;

/// Default PDF paper height in inches (A4).
pub const DEFAULT_PDF_PAPER_HEIGHT: f64 = 11.69;

/// PDF generation configuration.
#[derive(Debug, Clone)]
pub struct PdfConfig {
    /// Paper width in inches.
    pub paper_width: f64,
    /// Paper height in inches.
    pub paper_height: f64,
    /// Whether PDF generation is enabled.
    pub enabled: bool,
}

impl Default for PdfConfig {
    fn default() -> Self {
        Self {
            paper_width: DEFAULT_PDF_PAPER_WIDTH,
            paper_height: DEFAULT_PDF_PAPER_HEIGHT,
            enabled: false,
        }
    }
}

/// MHTML archive configuration.
#[derive(Debug, Clone)]
pub struct MhtmlConfig {
    /// Whether MHTML generation is enabled.
    pub enabled: bool,
}

impl Default for MhtmlConfig {
    fn default() -> Self {
        Self { enabled: false }
    }
}

/// Screenshot, PDF, and MHTML capture service.
///
/// Manages a headless browser instance for capturing screenshots, generating PDFs,
/// and creating MHTML archives. The browser is lazily initialized on first use.
pub struct ScreenshotService {
    config: ScreenshotConfig,
    pdf_config: PdfConfig,
    mhtml_config: MhtmlConfig,
    browser: Arc<Mutex<Option<Browser>>>,
    chromium_user_data_dir: Arc<Mutex<Option<PathBuf>>>,
    chromium_profile_dir: Option<String>,
}

impl ScreenshotService {
    /// Create a new screenshot service.
    #[must_use]
    pub fn new(config: ScreenshotConfig) -> Self {
        let (chromium_profile_dir, _) = config
            .cookies_from_browser
            .as_deref()
            .map(chromium_user_data_and_profile_from_spec)
            .map(|(_ud, prof)| (prof, ()))
            .unwrap_or((None, ()));
        Self {
            config,
            pdf_config: PdfConfig::default(),
            mhtml_config: MhtmlConfig::default(),
            browser: Arc::new(Mutex::new(None)),
            chromium_user_data_dir: Arc::new(Mutex::new(None)),
            chromium_profile_dir,
        }
    }

    /// Create a new screenshot service with PDF configuration.
    #[must_use]
    pub fn with_pdf_config(config: ScreenshotConfig, pdf_config: PdfConfig) -> Self {
        let (chromium_profile_dir, _) = config
            .cookies_from_browser
            .as_deref()
            .map(chromium_user_data_and_profile_from_spec)
            .map(|(_ud, prof)| (prof, ()))
            .unwrap_or((None, ()));
        Self {
            config,
            pdf_config,
            mhtml_config: MhtmlConfig::default(),
            browser: Arc::new(Mutex::new(None)),
            chromium_user_data_dir: Arc::new(Mutex::new(None)),
            chromium_profile_dir,
        }
    }

    /// Create a new screenshot service with PDF and MHTML configuration.
    #[must_use]
    pub fn with_all_configs(
        config: ScreenshotConfig,
        pdf_config: PdfConfig,
        mhtml_config: MhtmlConfig,
    ) -> Self {
        let (chromium_profile_dir, _) = config
            .cookies_from_browser
            .as_deref()
            .map(chromium_user_data_and_profile_from_spec)
            .map(|(_ud, prof)| (prof, ()))
            .unwrap_or((None, ()));
        Self {
            config,
            pdf_config,
            mhtml_config,
            browser: Arc::new(Mutex::new(None)),
            chromium_user_data_dir: Arc::new(Mutex::new(None)),
            chromium_profile_dir,
        }
    }

    /// Check if screenshot capture is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Check if PDF generation is enabled.
    #[must_use]
    pub fn is_pdf_enabled(&self) -> bool {
        self.pdf_config.enabled
    }

    /// Check if MHTML generation is enabled.
    #[must_use]
    pub fn is_mhtml_enabled(&self) -> bool {
        self.mhtml_config.enabled
    }

    /// Initialize the browser if not already running.
    async fn ensure_browser(&self) -> Result<()> {
        let mut browser_guard = self.browser.lock().await;
        if browser_guard.is_some() {
            return Ok(());
        }

        info!("Initializing headless browser for screenshots");

        // If configured, clone the persisted cookies profile into a writable temp directory.
        // This avoids profile lock contention with cookie-browser and ensures auth cookies are present.
        let mut user_data_dir_guard = self.chromium_user_data_dir.lock().await;
        if user_data_dir_guard.is_none() {
            if let Some(spec) = self.config.cookies_from_browser.as_deref() {
                let (source_user_data_dir, profile_dir) =
                    chromium_user_data_and_profile_from_spec(spec);
                let cloned = clone_chromium_user_data_dir_for_service(
                    &self.config.work_dir,
                    &source_user_data_dir,
                    profile_dir.as_deref(),
                )
                .await
                .context("Failed to clone Chromium profile for screenshot/PDF/MHTML")?;
                *user_data_dir_guard = Some(cloned);

                // Store profile dir for chromium launch args.
                // If None, Chromium will use its default profile.
                // (We still keep a copy in self.chromium_profile_dir for consistency.)
                drop(user_data_dir_guard);
            }
        }

        let user_data_dir = self.chromium_user_data_dir.lock().await.clone();

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

        if let Some(dir) = user_data_dir {
            config_builder = config_builder.arg(format!("--user-data-dir={}", dir.display()));
            if let Some(ref profile) = self.chromium_profile_dir {
                config_builder = config_builder.arg(format!("--profile-directory={profile}"));
            }
        }

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

        // Create a new page (start with about:blank to inject cookies first)
        let page = browser
            .new_page("about:blank")
            .await
            .context("Failed to create new page")?;

        // Inject cookies if configured
        if let Some(ref cookies_path) = self.config.cookies_file_path {
            if cookies_path.exists() {
                match load_cookies_for_url(cookies_path, url).await {
                    Ok(cookies) if !cookies.is_empty() => {
                        match SetCookiesParams::builder().cookies(cookies).build() {
                            Ok(set_cookies) => {
                                if let Err(e) = page.execute(set_cookies).await {
                                    warn!(url = %url, error = %e, "Failed to set cookies");
                                } else {
                                    debug!(url = %url, "Cookies injected successfully");
                                }
                            }
                            Err(e) => {
                                warn!(url = %url, error = %e, "Failed to build SetCookiesParams")
                            }
                        }
                    }
                    Ok(_) => debug!(url = %url, "No matching cookies found"),
                    Err(e) => warn!(url = %url, error = %e, "Failed to load cookies"),
                }
            }
        }

        // Navigate to the actual URL
        page.goto(url).await.context("Failed to navigate to URL")?;

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

    /// Generate a PDF of the given URL.
    ///
    /// Returns the PDF as bytes.
    pub async fn capture_pdf(&self, url: &str) -> Result<Vec<u8>> {
        if !self.pdf_config.enabled {
            anyhow::bail!("PDF generation is disabled");
        }

        self.ensure_browser().await?;

        let browser_guard = self.browser.lock().await;
        let browser = browser_guard.as_ref().context("Browser not initialized")?;

        debug!(url = %url, "Generating PDF");

        // Create a new page (start with about:blank to inject cookies first)
        let page = browser
            .new_page("about:blank")
            .await
            .context("Failed to create new page")?;

        // Inject cookies if configured
        if let Some(ref cookies_path) = self.config.cookies_file_path {
            if cookies_path.exists() {
                match load_cookies_for_url(cookies_path, url).await {
                    Ok(cookies) if !cookies.is_empty() => {
                        match SetCookiesParams::builder().cookies(cookies).build() {
                            Ok(set_cookies) => {
                                if let Err(e) = page.execute(set_cookies).await {
                                    warn!(url = %url, error = %e, "Failed to set cookies for PDF");
                                } else {
                                    debug!(url = %url, "Cookies injected for PDF capture");
                                }
                            }
                            Err(e) => {
                                warn!(url = %url, error = %e, "Failed to build SetCookiesParams for PDF")
                            }
                        }
                    }
                    Ok(_) => debug!(url = %url, "No matching cookies found for PDF"),
                    Err(e) => warn!(url = %url, error = %e, "Failed to load cookies for PDF"),
                }
            }
        }

        // Navigate to the actual URL
        page.goto(url).await.context("Failed to navigate to URL")?;

        // Wait for the page to load
        page.wait_for_navigation()
            .await
            .context("Navigation timeout")?;

        // Give the page a bit more time to render dynamic content
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Generate PDF with configured paper size
        let pdf_params = PrintToPdfParams::builder()
            .paper_width(self.pdf_config.paper_width)
            .paper_height(self.pdf_config.paper_height)
            .print_background(true)
            .build();

        let pdf_data = page
            .pdf(pdf_params)
            .await
            .context("Failed to generate PDF")?;

        // Close the page
        if let Err(e) = page.close().await {
            warn!("Failed to close page: {e}");
        }

        debug!(url = %url, size = pdf_data.len(), "PDF generated");

        Ok(pdf_data)
    }

    /// Generate a PDF and save it to a file.
    pub async fn capture_pdf_to_file(&self, url: &str, output_path: &Path) -> Result<()> {
        let pdf_data = self.capture_pdf(url).await?;
        tokio::fs::write(output_path, &pdf_data)
            .await
            .with_context(|| format!("Failed to write PDF to {}", output_path.display()))?;
        Ok(())
    }

    /// Capture the page as MHTML (single-file web archive).
    ///
    /// MHTML bundles HTML with all resources (CSS, images, etc.) into a single file.
    /// Returns the MHTML content as bytes.
    pub async fn capture_mhtml(&self, url: &str) -> Result<Vec<u8>> {
        if !self.mhtml_config.enabled {
            anyhow::bail!("MHTML generation is disabled");
        }

        self.ensure_browser().await?;

        let browser_guard = self.browser.lock().await;
        let browser = browser_guard.as_ref().context("Browser not initialized")?;

        debug!(url = %url, "Capturing MHTML");

        // Create a new page (start with about:blank to inject cookies first)
        let page = browser
            .new_page("about:blank")
            .await
            .context("Failed to create new page")?;

        // Inject cookies if configured
        if let Some(ref cookies_path) = self.config.cookies_file_path {
            if cookies_path.exists() {
                match load_cookies_for_url(cookies_path, url).await {
                    Ok(cookies) if !cookies.is_empty() => {
                        match SetCookiesParams::builder().cookies(cookies).build() {
                            Ok(set_cookies) => {
                                if let Err(e) = page.execute(set_cookies).await {
                                    warn!(url = %url, error = %e, "Failed to set cookies for MHTML");
                                } else {
                                    debug!(url = %url, "Cookies injected for MHTML capture");
                                }
                            }
                            Err(e) => {
                                warn!(url = %url, error = %e, "Failed to build SetCookiesParams for MHTML")
                            }
                        }
                    }
                    Ok(_) => debug!(url = %url, "No matching cookies found for MHTML"),
                    Err(e) => warn!(url = %url, error = %e, "Failed to load cookies for MHTML"),
                }
            }
        }

        // Navigate to the actual URL
        page.goto(url).await.context("Failed to navigate to URL")?;

        // Wait for the page to load
        page.wait_for_navigation()
            .await
            .context("Navigation timeout")?;

        // Give the page more time to render dynamic content and load resources
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Capture MHTML using CDP Page.captureSnapshot
        let snapshot_params = CaptureSnapshotParams::builder()
            .format(CaptureSnapshotFormat::Mhtml)
            .build();

        let snapshot = page
            .execute(snapshot_params)
            .await
            .context("Failed to capture MHTML snapshot")?;

        let mhtml_data = snapshot.data.clone().into_bytes();

        // Close the page
        if let Err(e) = page.close().await {
            warn!("Failed to close page: {e}");
        }

        debug!(url = %url, size = mhtml_data.len(), "MHTML captured");

        Ok(mhtml_data)
    }

    /// Capture MHTML and save it to a file.
    pub async fn capture_mhtml_to_file(&self, url: &str, output_path: &Path) -> Result<()> {
        let mhtml_data = self.capture_mhtml(url).await?;
        tokio::fs::write(output_path, &mhtml_data)
            .await
            .with_context(|| format!("Failed to write MHTML to {}", output_path.display()))?;
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

async fn clone_chromium_user_data_dir_for_service(
    base_work_dir: &Path,
    source: &Path,
    profile_dir: Option<&str>,
) -> Result<PathBuf> {
    use std::process::Stdio;
    use tokio::process::Command;

    let dest = base_work_dir.join("chromium-screenshot-user-data");
    let _ = tokio::fs::remove_dir_all(&dest).await;
    tokio::fs::create_dir_all(&dest)
        .await
        .context("Failed to create chromium-screenshot-user-data dir")?;

    // Prefer cp -a, but fall back to best-effort copy if permissions are weird.
    let cp_output = Command::new("cp")
        .arg("-a")
        .arg(format!("{}/.", source.display()))
        .arg(&dest)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await;

    match cp_output {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                status = %output.status,
                source = %source.display(),
                stderr = %stderr.trim(),
                "cp -a failed while cloning Chromium profile for screenshots; falling back to best-effort copy"
            );
            copy_dir_best_effort(source, &dest, "chromium profile clone (screenshots)").await?;
        }
        Err(e) => {
            warn!(
                error = %e,
                source = %source.display(),
                "Failed to spawn cp -a for Chromium profile copy (screenshots); falling back to best-effort copy"
            );
            copy_dir_best_effort(source, &dest, "chromium profile clone (screenshots)").await?;
        }
    }

    for name in ["SingletonLock", "SingletonCookie", "SingletonSocket"] {
        let _ = tokio::fs::remove_file(dest.join(name)).await;
    }

    let local_state = dest.join("Local State");
    if !local_state.is_file() {
        anyhow::bail!(
            "Cloned Chromium profile for screenshots is missing 'Local State'. Ensure /app/cookies/chromium-profile is readable by the archiver container (e.g. chmod -R a+rX on the profile)."
        );
    }

    let profile_name = profile_dir.unwrap_or("Default");
    let cookie_db_candidates = [
        dest.join(profile_name).join("Cookies"),
        dest.join(profile_name).join("Network").join("Cookies"),
        dest.join("Default").join("Cookies"),
        dest.join("Default").join("Network").join("Cookies"),
    ];
    let has_cookie_db = cookie_db_candidates.iter().any(|p| p.is_file());
    if !has_cookie_db {
        anyhow::bail!(
            "Cloned Chromium profile for screenshots does not contain a readable Cookies database. Ensure /app/cookies/chromium-profile is readable by the archiver container (e.g. chmod -R a+rX on the profile)."
        );
    }

    Ok(dest)
}

/// Load cookies from a URL for a specific domain.
async fn load_cookies_for_url(cookies_file: &Path, url: &str) -> Result<Vec<CookieParam>> {
    // Parse the URL to get domain
    let parsed_url = Url::parse(url).context("Failed to parse URL")?;
    let domain = parsed_url.host_str().unwrap_or("");

    // Read the cookies file
    let content = tokio::fs::read_to_string(cookies_file)
        .await
        .context("Failed to read cookies file")?;

    let mut cookies = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip comments and empty lines
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Netscape format: domain, include_subdomains, path, secure, expires, name, value
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 7 {
            continue;
        }

        let cookie_domain = fields[0].trim_start_matches('.');
        let path = fields[2];
        let secure = fields[3].to_lowercase() == "true";
        let expires: Option<f64> = fields[4].parse().ok();
        let name = fields[5];
        let value = fields[6];

        // Check if cookie applies to this domain
        let domain_matches = cookie_domain == domain
            || cookie_domain.ends_with(&format!(".{domain}"))
            || domain.ends_with(&format!(".{cookie_domain}"))
            || domain.ends_with(cookie_domain);

        if domain_matches {
            let mut builder = CookieParam::builder()
                .name(name.to_string())
                .value(value.to_string())
                .domain(format!(".{cookie_domain}"))
                .path(path.to_string())
                .secure(secure)
                .http_only(false);

            // Set expiration if valid
            if let Some(exp) = expires {
                if exp > 0.0 {
                    builder = builder.expires(TimeSinceEpoch::new(exp));
                }
            }

            match builder.build() {
                Ok(cookie) => cookies.push(cookie),
                Err(e) => {
                    warn!(
                        name = %name,
                        error = %e,
                        "Failed to build cookie"
                    );
                }
            }
        }
    }

    debug!(
        count = cookies.len(),
        domain = %domain,
        "Loaded cookies for domain"
    );

    Ok(cookies)
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
        assert!(config.cookies_from_browser.is_none());
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

    #[test]
    fn test_default_pdf_config() {
        let config = PdfConfig::default();
        assert!((config.paper_width - DEFAULT_PDF_PAPER_WIDTH).abs() < f64::EPSILON);
        assert!((config.paper_height - DEFAULT_PDF_PAPER_HEIGHT).abs() < f64::EPSILON);
        assert!(!config.enabled);
    }

    #[test]
    fn test_pdf_disabled_by_default() {
        let config = ScreenshotConfig::default();
        let service = ScreenshotService::new(config);
        assert!(!service.is_pdf_enabled());
    }

    #[test]
    fn test_pdf_enabled_with_config() {
        let screenshot_config = ScreenshotConfig::default();
        let pdf_config = PdfConfig {
            enabled: true,
            ..Default::default()
        };
        let service = ScreenshotService::with_pdf_config(screenshot_config, pdf_config);
        assert!(service.is_pdf_enabled());
    }

    #[test]
    fn test_default_mhtml_config() {
        let config = MhtmlConfig::default();
        assert!(!config.enabled);
    }

    #[test]
    fn test_mhtml_disabled_by_default() {
        let config = ScreenshotConfig::default();
        let service = ScreenshotService::new(config);
        assert!(!service.is_mhtml_enabled());
    }

    #[test]
    fn test_mhtml_enabled_with_config() {
        let screenshot_config = ScreenshotConfig::default();
        let pdf_config = PdfConfig::default();
        let mhtml_config = MhtmlConfig { enabled: true };
        let service =
            ScreenshotService::with_all_configs(screenshot_config, pdf_config, mhtml_config);
        assert!(service.is_mhtml_enabled());
    }
}
