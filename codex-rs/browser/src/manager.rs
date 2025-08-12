use crate::{config::BrowserConfig, page::Page, BrowserError, Result};
use chromiumoxide::{Browser, BrowserConfig as CdpConfig};
use chromiumoxide::cdp::browser_protocol::{emulation, network};
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, info, warn};

#[derive(Deserialize)]
struct JsonVersion {
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: String,
}

async fn discover_ws_via_port(port: u16) -> Result<String> {
    let url = format!("http://127.0.0.1:{}/json/version", port);
    let resp = Client::new().get(&url).send().await
        .map_err(|e| BrowserError::CdpError(format!("Failed to connect to Chrome debug port: {}", e)))?;
    
    if !resp.status().is_success() {
        return Err(BrowserError::CdpError(format!("Chrome /json/version returned {}", resp.status())));
    }
    
    let body: JsonVersion = resp.json().await
        .map_err(|e| BrowserError::CdpError(format!("Failed to parse Chrome debug response: {}", e)))?;
    
    Ok(body.web_socket_debugger_url)
}

/// Scan for Chrome processes with debug ports and verify accessibility
async fn scan_for_chrome_debug_port() -> Option<u16> {
    use std::process::Command;
    
    // Use ps to find Chrome processes with remote-debugging-port
    let output = Command::new("ps")
        .args(&["aux"])
        .output()
        .ok()?;
    
    let ps_output = String::from_utf8_lossy(&output.stdout);
    
    // Find all Chrome processes with debug ports
    let mut found_ports = Vec::new();
    for line in ps_output.lines() {
        // Look for Chrome/Chromium processes with remote-debugging-port
        if (line.contains("chrome") || line.contains("Chrome") || line.contains("chromium")) 
            && line.contains("--remote-debugging-port=") {
            
            // Extract the port number
            if let Some(port_str) = line.split("--remote-debugging-port=").nth(1) {
                // Take everything up to the next space or end of line
                let port_str = port_str.split_whitespace().next().unwrap_or(port_str);
                
                // Parse the port number
                if let Ok(port) = port_str.parse::<u16>() {
                    // Skip port 0 (means random port, not accessible)
                    if port > 0 {
                        found_ports.push(port);
                    }
                }
            }
        }
    }
    
    // Remove duplicates
    found_ports.sort_unstable();
    found_ports.dedup();
    
    info!("Found {} Chrome process(es) with debug ports: {:?}", found_ports.len(), found_ports);
    
    // Test each found port to see if it's accessible
    for port in found_ports {
        let url = format!("http://127.0.0.1:{}/json/version", port);
        let client = Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .ok()?;
            
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                info!("Verified Chrome debug port at {} is accessible", port);
                return Some(port);
            } else {
                debug!("Chrome port {} returned status: {}", port, resp.status());
            }
        } else {
            debug!("Could not connect to Chrome port {}", port);
        }
    }
    
    warn!("No accessible Chrome debug ports found");
    None
}

pub struct BrowserManager {
    pub config: Arc<RwLock<BrowserConfig>>,
    browser: Arc<Mutex<Option<Browser>>>,
    page: Arc<Mutex<Option<Arc<Page>>>>,
    last_activity: Arc<Mutex<Instant>>,
    idle_monitor_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    assets: Arc<Mutex<Option<Arc<crate::assets::AssetManager>>>>,
    user_data_dir: Arc<Mutex<Option<String>>>,
    cleanup_profile_on_drop: Arc<Mutex<bool>>,
}

impl BrowserManager {
    pub fn new(config: BrowserConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            browser: Arc::new(Mutex::new(None)),
            page: Arc::new(Mutex::new(None)),
            last_activity: Arc::new(Mutex::new(Instant::now())),
            idle_monitor_handle: Arc::new(Mutex::new(None)),
            assets: Arc::new(Mutex::new(None)),
            user_data_dir: Arc::new(Mutex::new(None)),
            cleanup_profile_on_drop: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn start(&self) -> Result<()> {
        let mut browser_guard = self.browser.lock().await;
        if browser_guard.is_some() {
            return Ok(());
        }

        let config = self.config.read().await.clone();
        
        // 1) Attach to a live Chrome, if requested
        if let Some(ws) = config.connect_ws.clone() {
            info!("Connecting to Chrome via WebSocket: {}", ws);
            let (browser, mut handler) = Browser::connect(ws).await?;
            tokio::spawn(async move {
                while let Some(_evt) = handler.next().await {}
            });
            *browser_guard = Some(browser);
            *self.cleanup_profile_on_drop.lock().await = false;
            
            self.start_idle_monitor().await;
            self.update_activity().await;
            return Ok(());
        }

        if let Some(port) = config.connect_port {
            // If port is 0, auto-scan for Chrome debug ports
            let actual_port = if port == 0 {
                info!("Auto-scanning for Chrome debug ports...");
                match scan_for_chrome_debug_port().await {
                    Some(found_port) => {
                        info!("Auto-detected Chrome on port {}", found_port);
                        found_port
                    }
                    None => {
                        warn!("No Chrome debug ports found during auto-scan. Will launch new instance.");
                        0  // Signal to fall through to launch
                    }
                }
            } else {
                port
            };
            
            if actual_port > 0 {
                info!("Discovering Chrome via debug port: {}", actual_port);
                match discover_ws_via_port(actual_port).await {
                    Ok(ws) => {
                        info!("Connecting to Chrome via discovered WebSocket: {}", ws);
                        let (browser, mut handler) = Browser::connect(ws).await?;
                        tokio::spawn(async move {
                            while let Some(_evt) = handler.next().await {}
                        });
                        *browser_guard = Some(browser);
                        *self.cleanup_profile_on_drop.lock().await = false;
                        
                        self.start_idle_monitor().await;
                        self.update_activity().await;
                        return Ok(());
                    }
                    Err(e) => {
                        warn!("Failed to connect to Chrome on port {}: {}. Will launch new instance.", actual_port, e);
                        // Fall through to launch
                    }
                }
            }
        }

        // 2) Otherwise: launch a browser
        info!("Launching new browser instance");
        
        let mut builder = CdpConfig::builder();
        
        // Use persistent profile if specified, otherwise temp
        let user_data_path = if let Some(dir) = &config.user_data_dir {
            builder = builder.user_data_dir(dir.clone());
            dir.to_string_lossy().to_string()
        } else {
            // Create temp profile
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let temp_path = format!("/tmp/coder-browser-{}-{}", std::process::id(), timestamp);
            
            // Ensure the directory doesn't exist before starting
            if tokio::fs::metadata(&temp_path).await.is_ok() {
                if let Err(e) = tokio::fs::remove_dir_all(&temp_path).await {
                    warn!("Failed to cleanup existing browser directory {}: {}", temp_path, e);
                }
            }
            
            builder = builder.user_data_dir(&temp_path);
            temp_path
        };
        
        // Configure viewport
        builder = builder
            .window_size(config.viewport.width, config.viewport.height);
        
        // Set headless mode based on config
        if config.headless {
            builder = builder.headless_mode(chromiumoxide::browser::HeadlessMode::New);
        }
        
        // Add less automation-screamy flags
        builder = builder
            .arg("--disable-blink-features=AutomationControlled")
            .arg("--disable-features=VizDisplayCompositor");
        
        let browser_config = builder.build()
            .map_err(|e| BrowserError::CdpError(e.to_string()))?;
            
        let (browser, mut handler) = Browser::launch(browser_config).await?;
        
        tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                debug!("Browser event: {:?}", event);
            }
        });

        *browser_guard = Some(browser);
        
        // Store the user data directory path for cleanup
        {
            let mut user_data_guard = self.user_data_dir.lock().await;
            *user_data_guard = Some(user_data_path.clone());
        }
        
        // Determine if we should cleanup on drop
        let should_cleanup = config.user_data_dir.is_none() || !config.persist_profile;
        *self.cleanup_profile_on_drop.lock().await = should_cleanup;
        
        self.start_idle_monitor().await;
        self.update_activity().await;
        
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.stop_idle_monitor().await;
        
        let mut page_guard = self.page.lock().await;
        *page_guard = None;
        
        let mut browser_guard = self.browser.lock().await;
        if let Some(mut browser) = browser_guard.take() {
            info!("Stopping browser");
            browser.close().await?;
        }
        
        // Only cleanup user data directory if we should
        let should_cleanup = *self.cleanup_profile_on_drop.lock().await;
        if should_cleanup {
            let mut user_data_guard = self.user_data_dir.lock().await;
            if let Some(user_data_path) = user_data_guard.take() {
                // Give Chrome a moment to fully release the profile
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                
                if let Err(e) = tokio::fs::remove_dir_all(&user_data_path).await {
                    warn!("Failed to cleanup browser user data directory {}: {}", user_data_path, e);
                    // Try a more aggressive cleanup on macOS
                    #[cfg(target_os = "macos")]
                    {
                        let _ = tokio::process::Command::new("rm")
                            .arg("-rf")
                            .arg(&user_data_path)
                            .output()
                            .await;
                    }
                }
            }
        }
        
        Ok(())
    }

    pub async fn get_or_create_page(&self) -> Result<Arc<Page>> {
        self.ensure_browser().await?;
        self.update_activity().await;

        let mut page_guard = self.page.lock().await;
        if let Some(page) = page_guard.as_ref() {
            return Ok(Arc::clone(page));
        }

        let browser_guard = self.browser.lock().await;
        let browser = browser_guard
            .as_ref()
            .ok_or(BrowserError::NotInitialized)?;

        let cdp_page = browser.new_page("about:blank").await?;
        
        // Apply page overrides (UA, locale, timezone, viewport, etc.)
        self.apply_page_overrides(&cdp_page).await?;
        
        let config = self.config.read().await;
        let page = Arc::new(Page::new(cdp_page, config.clone()));
        *page_guard = Some(Arc::clone(&page));

        Ok(page)
    }

    pub async fn close_page(&self) -> Result<()> {
        let mut page_guard = self.page.lock().await;
        if let Some(page) = page_guard.take() {
            page.close().await?;
        }
        Ok(())
    }

    pub async fn is_enabled(&self) -> bool {
        self.config.read().await.enabled
    }
    
    pub fn is_enabled_sync(&self) -> bool {
        self.config.try_read().map(|c| c.enabled).unwrap_or(false)
    }

    pub async fn set_enabled(&self, enabled: bool) -> Result<()> {
        let mut config = self.config.write().await;
        config.enabled = enabled;
        
        if enabled {
            self.start().await?;
        } else {
            self.stop().await?;
        }
        
        Ok(())
    }

    pub async fn update_config(&self, updates: impl FnOnce(&mut BrowserConfig)) -> Result<()> {
        let mut config = self.config.write().await;
        updates(&mut config);
        
        if let Some(page) = self.page.lock().await.as_ref() {
            page.update_viewport(config.viewport.clone()).await?;
        }
        
        Ok(())
    }

    pub async fn get_config(&self) -> BrowserConfig {
        self.config.read().await.clone()
    }

    pub async fn get_current_url(&self) -> Option<String> {
        let page_guard = self.page.lock().await;
        if let Some(page) = page_guard.as_ref() {
            page.get_current_url().await.ok()
        } else {
            None
        }
    }

    pub async fn get_status(&self) -> BrowserStatus {
        let config = self.config.read().await;
        let browser_active = self.browser.lock().await.is_some();
        let current_url = self.get_current_url().await;

        BrowserStatus {
            enabled: config.enabled,
            browser_active,
            current_url,
            viewport: config.viewport.clone(),
            fullpage: config.fullpage,
        }
    }

    /// Apply "human" environment: UA / Accept-Language / Timezone / Locale / DPR+Viewport
    pub async fn apply_page_overrides(
        &self,
        page: &chromiumoxide::Page,
    ) -> Result<()> {
        let config = self.config.read().await;
        
        // Enable Network domain before setting headers
        page.execute(network::EnableParams::default()).await?;

        // SetUserAgentOverrideParams requires user_agent to be set
        // Only call it if we have a user_agent (accept_language is optional)
        if let Some(ua) = &config.user_agent {
            let mut params_builder = network::SetUserAgentOverrideParams::builder()
                .user_agent(ua);
            
            if let Some(al) = &config.accept_language {
                params_builder = params_builder.accept_language(al);
            }
            
            let params = params_builder
                .build()
                .map_err(|e| BrowserError::CdpError(e))?;
            page.execute(params).await?;
        }

        if let Some(tz) = &config.timezone {
            page.execute(emulation::SetTimezoneOverrideParams {
                timezone_id: tz.clone(),
            }).await?;
        }

        if let Some(locale) = &config.locale {
            let params = emulation::SetLocaleOverrideParams::builder()
                .locale(locale)
                .build();
            page.execute(params).await?;
        }

        let params = emulation::SetDeviceMetricsOverrideParams::builder()
            .width(config.viewport.width as i64)
            .height(config.viewport.height as i64)
            .device_scale_factor(config.viewport.device_scale_factor)
            .mobile(config.viewport.mobile)
            .build()
            .map_err(|e| BrowserError::CdpError(e))?;
        page.execute(params).await?;

        Ok(())
    }

    async fn ensure_browser(&self) -> Result<()> {
        let browser_guard = self.browser.lock().await;
        if browser_guard.is_none() {
            drop(browser_guard);
            self.start().await?;
        }
        Ok(())
    }

    async fn update_activity(&self) {
        let mut last_activity = self.last_activity.lock().await;
        *last_activity = Instant::now();
    }

    pub fn set_enabled_sync(&self, enabled: bool) {
        // Try to set immediately if possible, otherwise spawn a task
        if let Ok(mut cfg) = self.config.try_write() {
            cfg.enabled = enabled;
        } else {
            let config = self.config.clone();
            tokio::spawn(async move {
                let mut cfg = config.write().await;
                cfg.enabled = enabled;
            });
        }
    }

    pub fn set_fullpage_sync(&self, fullpage: bool) {
        if let Ok(mut cfg) = self.config.try_write() {
            cfg.fullpage = fullpage;
        } else {
            let config = self.config.clone();
            tokio::spawn(async move {
                let mut cfg = config.write().await;
                cfg.fullpage = fullpage;
            });
        }
    }

    pub fn set_viewport_sync(&self, width: u32, height: u32) {
        if let Ok(mut cfg) = self.config.try_write() {
            cfg.viewport.width = width;
            cfg.viewport.height = height;
        } else {
            let config = self.config.clone();
            tokio::spawn(async move {
                let mut cfg = config.write().await;
                cfg.viewport.width = width;
                cfg.viewport.height = height;
            });
        }
    }

    pub fn set_segments_max_sync(&self, segments_max: usize) {
        if let Ok(mut cfg) = self.config.try_write() {
            cfg.segments_max = segments_max;
        } else {
            let config = self.config.clone();
            tokio::spawn(async move {
                let mut cfg = config.write().await;
                cfg.segments_max = segments_max;
            });
        }
    }

    pub fn get_status_sync(&self) -> String {
        // Use try operations to avoid blocking - return cached/default values if locks are held
        let cfg = self.config.try_read().map(|c| {
            let enabled = c.enabled;
            let viewport_width = c.viewport.width;
            let viewport_height = c.viewport.height;
            let fullpage = c.fullpage;
            (enabled, viewport_width, viewport_height, fullpage)
        }).unwrap_or((false, 1024, 768, false));
        
        let browser_active = self.browser.try_lock().map(|b| b.is_some()).unwrap_or(false);
        
        let mode = if cfg.0 { "enabled" } else { "disabled" };
        let fullpage = if cfg.3 { "on" } else { "off" };
        
        let mut status = format!(
            "Browser status:\n• Mode: {}\n• Viewport: {}×{}\n• Full-page: {}",
            mode, cfg.1, cfg.2, fullpage
        );
        
        if browser_active {
            status.push_str("\n• Browser: active");
        }
        
        status
    }

    async fn start_idle_monitor(&self) {
        let config = self.config.read().await;
        let idle_timeout = Duration::from_millis(config.idle_timeout_ms);
        drop(config);

        let browser = Arc::clone(&self.browser);
        let last_activity = Arc::clone(&self.last_activity);
        let user_data_dir = Arc::clone(&self.user_data_dir);

        let handle = tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(10)).await;
                
                let last = *last_activity.lock().await;
                if last.elapsed() > idle_timeout {
                    warn!("Browser idle timeout reached, closing");
                    let mut browser_guard = browser.lock().await;
                    if let Some(mut browser) = browser_guard.take() {
                        let _ = browser.close().await;
                    }
                    
                    // Cleanup user data directory on idle timeout
                    let mut user_data_guard = user_data_dir.lock().await;
                    if let Some(user_data_path) = user_data_guard.take() {
                        if let Err(e) = tokio::fs::remove_dir_all(&user_data_path).await {
                            warn!("Failed to cleanup browser user data directory {}: {}", user_data_path, e);
                        }
                    }
                    
                    break;
                }
            }
        });

        let mut handle_guard = self.idle_monitor_handle.lock().await;
        *handle_guard = Some(handle);
    }

    async fn stop_idle_monitor(&self) {
        let mut handle_guard = self.idle_monitor_handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }
    }

    pub async fn goto(&self, url: &str) -> Result<crate::page::GotoResult> {
        // Get or create page
        let page = self.get_or_create_page().await?;
        
        let config = self.config.read().await;
        let result = page.goto(url, Some(config.wait.clone())).await?;
        
        self.update_activity().await;
        Ok(result)
    }

    pub async fn capture_screenshot_with_url(&self) -> Result<(Vec<std::path::PathBuf>, Option<String>)> {
        let (paths, url) = self.capture_screenshot_internal().await?;
        Ok((paths, Some(url)))
    }

    pub async fn capture_screenshot(&self) -> Result<Vec<std::path::PathBuf>> {
        let (paths, _) = self.capture_screenshot_internal().await?;
        Ok(paths)
    }

    async fn capture_screenshot_internal(&self) -> Result<(Vec<std::path::PathBuf>, String)> {
        // Ensure we have a browser and page
        let page = self.get_or_create_page().await?;
        
        // Initialize assets manager if needed
        let mut assets_guard = self.assets.lock().await;
        if assets_guard.is_none() {
            *assets_guard = Some(Arc::new(crate::assets::AssetManager::new().await?));
        }
        let assets = assets_guard.as_ref().unwrap().clone();
        drop(assets_guard);
        
        // Get current config
        let config = self.config.read().await;
        
        // Determine screenshot mode
        let mode = if config.fullpage {
            crate::page::ScreenshotMode::FullPage { 
                segments_max: Some(config.segments_max) 
            }
        } else {
            crate::page::ScreenshotMode::Viewport
        };
        
        // Get current URL directly from the browser (not cached)
        let current_url = page.get_current_url().await.unwrap_or_else(|_| "about:blank".to_string());
        
        // Capture screenshots
        let screenshots = page.screenshot(mode).await?;
        
        // Store screenshots and get paths
        let mut paths = Vec::new();
        for screenshot in screenshots {
            let image_ref = assets.store_screenshot(
                &screenshot.data,
                screenshot.format,
                screenshot.width,
                screenshot.height,
                300000, // 5 minute TTL
            ).await?;
            paths.push(std::path::PathBuf::from(image_ref.path));
        }
        
        self.update_activity().await;
        Ok((paths, current_url))
    }

    pub async fn close(&self) -> Result<()> {
        // Just delegate to stop() which handles cleanup properly
        self.stop().await
    }

    /// Click at the specified coordinates
    pub async fn click(&self, x: f64, y: f64) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.click(x, y).await
    }

    /// Type text into the currently focused element
    pub async fn type_text(&self, text: &str) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.type_text(text).await
    }

    /// Press a key (e.g., "Enter", "Tab", "Escape", "ArrowDown")
    pub async fn press_key(&self, key: &str) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.press_key(key).await
    }

    /// Execute JavaScript code with enhanced return value handling
    pub async fn execute_javascript(&self, code: &str) -> Result<serde_json::Value> {
        let page = self.get_or_create_page().await?;
        page.execute_javascript(code).await
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BrowserStatus {
    pub enabled: bool,
    pub browser_active: bool,
    pub current_url: Option<String>,
    pub viewport: crate::config::ViewportConfig,
    pub fullpage: bool,
}