use crate::BrowserError;
use crate::Result;
use crate::config::BrowserConfig;
use crate::config::WaitStrategy;
use crate::page::Page;
use chromiumoxide::Browser;
use chromiumoxide::BrowserConfig as CdpConfig;
use chromiumoxide::browser::HeadlessMode;
use chromiumoxide::cdp::browser_protocol::emulation;
use chromiumoxide::cdp::browser_protocol::network;
use fs2::FileExt;
use futures::StreamExt;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::time::Duration;
use tokio::time::Instant;
use tokio::time::sleep;
use tracing::debug;
use tracing::info;
use tracing::warn;
use crate::global;

#[derive(Deserialize)]
struct JsonVersion {
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: String,
}

static INTERNAL_BROWSER_LAUNCH_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

struct BrowserLaunchLockFile {
    file: std::fs::File,
}

impl BrowserLaunchLockFile {
    async fn acquire(timeout: Duration) -> Result<Self> {
        let lock_path = std::env::temp_dir().join("code-browser-launch.lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)?;

        let start = Instant::now();
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(Self { file }),
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    if start.elapsed() >= timeout {
                        return Err(BrowserError::CdpError(format!(
                            "Timed out waiting for browser launch lock at {} (another Code instance may be launching a browser).",
                            lock_path.display()
                        )));
                    }
                    sleep(Duration::from_millis(50)).await;
                }
                Err(err) => return Err(BrowserError::IoError(err)),
            }
        }
    }
}

impl Drop for BrowserLaunchLockFile {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn is_temporary_internal_launch_error_message(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    // macOS: EAGAIN = os error 35
    // Linux: ENOMEM = os error 12
    // Common: "Too many open files" (EMFILE)
    message.contains("resource temporarily unavailable")
        || message.contains("temporarily unavailable")
        || message.contains("os error 35")
        || message.contains("eagain")
        || message.contains("os error 12")
        || message.contains("cannot allocate memory")
        || message.contains("too many open files")
        || message.contains("os error 24")
}

fn chrome_logging_enabled() -> bool {
    env_truthy("CODE_SUBAGENT_DEBUG") || env_truthy("CODEX_BROWSER_LOG")
}

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn resolve_chrome_log_path() -> Option<PathBuf> {
    if !chrome_logging_enabled() {
        return None;
    }

    if let Ok(path) = std::env::var("CODEX_BROWSER_LOG_PATH") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    let base = if let Ok(home) = std::env::var("CODE_HOME").or_else(|_| std::env::var("CODEX_HOME")) {
        PathBuf::from(home).join("debug_logs")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".code").join("debug_logs")
    } else {
        return Some(std::env::temp_dir().join("code-chrome.log"));
    };

    let path = base.join("code-chrome.log");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    Some(path)
}

const HANDLER_ERROR_LIMIT: u32 = 3;

fn should_restart_handler(consecutive_errors: u32) -> bool {
    consecutive_errors >= HANDLER_ERROR_LIMIT
}

fn should_stop_handler<E: std::fmt::Display>(
    label: &'static str,
    result: std::result::Result<(), E>,
    consecutive_errors: &mut u32,
) -> bool {
    match result {
        Ok(()) => {
            *consecutive_errors = 0;
            false
        }
        Err(err) => {
            let message = err.to_string();
            let message_lower = message.to_ascii_lowercase();
            // Chromiumoxide surfaces "oneshot canceled" when a request future is dropped
            // (e.g., due to our own timeouts). These are expected and should not
            // be treated as connection failures.
            if message_lower.contains("oneshot canceled") || message_lower.contains("oneshot cancelled") {
                *consecutive_errors = 0;
                debug!("{label} event handler error ignored: {message}");
                return false;
            }
            *consecutive_errors = consecutive_errors.saturating_add(1);
            let count = *consecutive_errors;
            if count <= HANDLER_ERROR_LIMIT {
                debug!("{label} event handler error: {err} (count: {count})");
            }
            if should_restart_handler(count) {
                warn!("{label} event handler errors exceeded limit; restarting browser connection");
                return true;
            }
            false
        }
    }
}

async fn discover_ws_via_host_port(host: &str, port: u16) -> Result<String> {
    let url = format!("http://{}:{}/json/version", host, port);
    debug!("Requesting Chrome version info from: {}", url);

    let client_start = tokio::time::Instant::now();
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5)) // Allow Chrome time to bring up /json/version on fresh launch
        .build()
        .map_err(|e| BrowserError::CdpError(format!("Failed to build HTTP client: {}", e)))?;
    debug!("HTTP client created in {:?}", client_start.elapsed());

    let req_start = tokio::time::Instant::now();
    let resp = client.get(&url).send().await.map_err(|e| {
        BrowserError::CdpError(format!("Failed to connect to Chrome debug port: {}", e))
    })?;
    debug!(
        "HTTP request completed in {:?}, status: {}",
        req_start.elapsed(),
        resp.status()
    );

    if !resp.status().is_success() {
        return Err(BrowserError::CdpError(format!(
            "Chrome /json/version returned {}",
            resp.status()
        )));
    }

    let parse_start = tokio::time::Instant::now();
    let body: JsonVersion = resp.json().await.map_err(|e| {
        BrowserError::CdpError(format!("Failed to parse Chrome debug response: {}", e))
    })?;
    debug!("Response parsed in {:?}", parse_start.elapsed());

    Ok(body.web_socket_debugger_url)
}

/// Scan for Chrome processes with debug ports and verify accessibility
async fn scan_for_chrome_debug_port() -> Option<u16> {
    use std::process::Command;

    // Use ps to find Chrome processes with remote-debugging-port
    let output = Command::new("ps").args(&["aux"]).output().ok()?;

    let ps_output = String::from_utf8_lossy(&output.stdout);

    // Find all Chrome processes with debug ports
    let mut found_ports = Vec::new();
    for line in ps_output.lines() {
        // Look for Chrome/Chromium processes with remote-debugging-port
        if (line.contains("chrome") || line.contains("Chrome") || line.contains("chromium"))
            && line.contains("--remote-debugging-port=")
        {
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

    info!(
        "Found {} Chrome process(es) with debug ports: {:?}",
        found_ports.len(),
        found_ports
    );

    // Test each found port to see if it's accessible (test in parallel for speed)
    if found_ports.is_empty() {
        return None;
    }

    debug!("Testing {} port(s) for accessibility...", found_ports.len());
    let test_start = tokio::time::Instant::now();

    // Create futures for testing all ports in parallel
    let mut port_tests = Vec::new();
    for port in found_ports {
        let test_future = async move {
            let url = format!("http://127.0.0.1:{}/json/version", port);
            let client = Client::builder()
                .no_proxy()
                .timeout(Duration::from_millis(200)) // Shorter timeout for parallel tests
                .build()
                .ok()?;

            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    debug!("Chrome port {} is accessible", port);
                    Some(port)
                }
                Ok(resp) => {
                    debug!("Chrome port {} returned status: {}", port, resp.status());
                    None
                }
                Err(_) => {
                    debug!("Could not connect to Chrome port {}", port);
                    None
                }
            }
        };
        port_tests.push(test_future);
    }

    // Test all ports in parallel and return the first accessible one
    let results = futures::future::join_all(port_tests).await;
    debug!(
        "Port accessibility tests completed in {:?}",
        test_start.elapsed()
    );

    for port in results.into_iter().flatten() {
        info!("Verified Chrome debug port at {} is accessible", port);
        return Some(port);
    }

    warn!("No accessible Chrome debug ports found");
    None
}

pub struct BrowserManager {
    pub config: Arc<RwLock<BrowserConfig>>,
    browser: Arc<Mutex<Option<Browser>>>,
    page: Arc<Mutex<Option<Arc<Page>>>>,
    // Dedicated background page for screenshots to prevent focus stealing
    background_page: Arc<Mutex<Option<Arc<Page>>>>,
    last_activity: Arc<Mutex<Instant>>,
    idle_monitor_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    event_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    assets: Arc<Mutex<Option<Arc<crate::assets::AssetManager>>>>,
    user_data_dir: Arc<Mutex<Option<String>>>,
    cleanup_profile_on_drop: Arc<Mutex<bool>>,
    navigation_callback: Arc<tokio::sync::RwLock<Option<Box<dyn Fn(String) + Send + Sync>>>>,
    navigation_monitor_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    viewport_monitor_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Gate to temporarily disable all automatic viewport corrections (post-initial set)
    auto_viewport_correction_enabled: Arc<tokio::sync::RwLock<bool>>,
    /// Track last applied device metrics to avoid redundant overrides
    last_metrics_applied: Arc<Mutex<Option<(i64, i64, f64, bool, std::time::Instant)>>>,
}

#[derive(Debug)]
struct PageDebugInfo {
    target_id: String,
    session_id: String,
    opener_id: Option<String>,
    cached_url: Option<String>,
    live_url: Option<String>,
}

#[derive(Debug)]
struct TargetSnapshot {
    total: usize,
    sample: Vec<String>,
    truncated: bool,
}

impl BrowserManager {
    const SCREENSHOT_TTL_MS: u64 = 86_400_000; // 24 hours

    pub fn new(config: BrowserConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            browser: Arc::new(Mutex::new(None)),
            page: Arc::new(Mutex::new(None)),
            background_page: Arc::new(Mutex::new(None)),
            last_activity: Arc::new(Mutex::new(Instant::now())),
            idle_monitor_handle: Arc::new(Mutex::new(None)),
            event_task: Arc::new(Mutex::new(None)),
            assets: Arc::new(Mutex::new(None)),
            user_data_dir: Arc::new(Mutex::new(None)),
            cleanup_profile_on_drop: Arc::new(Mutex::new(false)),
            navigation_callback: Arc::new(tokio::sync::RwLock::new(None)),
            navigation_monitor_handle: Arc::new(Mutex::new(None)),
            viewport_monitor_handle: Arc::new(Mutex::new(None)),
            auto_viewport_correction_enabled: Arc::new(tokio::sync::RwLock::new(true)),
            last_metrics_applied: Arc::new(Mutex::new(None)),
        }
    }

    /// Try to connect to Chrome via CDP only - no fallback to internal browser
    pub async fn connect_to_chrome_only(&self) -> Result<()> {
        tracing::info!("[cdp/bm] connect_to_chrome_only: begin");
        // Quick check without holding the lock during IO
        if self.browser.lock().await.is_some() {
            tracing::info!("[cdp/bm] already connected; early return");
            return Ok(());
        }

        let config = self.config.read().await.clone();
        tracing::info!(
            "[cdp/bm] config: connect_host={:?}, connect_port={:?}, connect_ws={:?}",
            config.connect_host, config.connect_port, config.connect_ws
        );

        // If a WebSocket is configured explicitly, try that first
        if let Some(ws) = config.connect_ws.clone() {
            info!("[cdp/bm] Connecting to Chrome via configured WebSocket: {}", ws);
            let attempt_timeout = Duration::from_millis(config.connect_attempt_timeout_ms);
            let attempts = std::cmp::max(1, config.connect_attempts as i32);
            let mut last_err: Option<String> = None;

            for attempt in 1..=attempts {
                info!(
                    "[cdp/bm] WS connect attempt {}/{} (timeout={}ms)",
                    attempt,
                    attempts,
                    attempt_timeout.as_millis()
                );
                let ws_clone = ws.clone();
                let handle = tokio::spawn(async move { Browser::connect(ws_clone).await });
                match tokio::time::timeout(attempt_timeout, handle).await {
                    Ok(Ok(Ok((browser, mut handler)))) => {
                        info!("[cdp/bm] WS connect attempt {} succeeded", attempt);

                        // Start event handler loop
                let browser_arc = self.browser.clone();
                let page_arc = self.page.clone();
                let background_page_arc = self.background_page.clone();
                let task = tokio::spawn(async move {
                    let mut consecutive_errors = 0u32;
                    while let Some(result) = handler.next().await {
                        if should_stop_handler("[cdp/bm]", result, &mut consecutive_errors) {
                            break;
                        }
                    }
                    warn!("[cdp/bm] event handler ended; clearing browser state so it can restart");
                    *browser_arc.lock().await = None;
                    *page_arc.lock().await = None;
                    *background_page_arc.lock().await = None;
                });
                *self.event_task.lock().await = Some(task);
                        {
                            let mut guard = self.browser.lock().await;
                            *guard = Some(browser);
                        }
                        *self.cleanup_profile_on_drop.lock().await = false;

                        // Fire-and-forget targets warmup
                        {
                            let browser_arc = self.browser.clone();
                            tokio::spawn(async move {
                                if let Some(browser) = browser_arc.lock().await.as_mut() {
                                    let _ = tokio::time::timeout(Duration::from_millis(100), browser.fetch_targets()).await;
                                }
                            });
                        }

                        self.start_idle_monitor().await;
                        self.update_activity().await;
                        // Cache last connection (ws only)
                        global::set_last_connection(None, Some(ws.clone())).await;
                        return Ok(());
                    }
                    Ok(Ok(Err(e))) => {
                        let msg = format!("CDP WebSocket connect failed: {}", e);
                        warn!("[cdp/bm] {}", msg);
                        last_err = Some(msg);
                    }
                    Ok(Err(join_err)) => {
                        let msg = format!("Join error during connect attempt: {}", join_err);
                        warn!("[cdp/bm] {}", msg);
                        last_err = Some(msg);
                    }
                    Err(_) => {
                        warn!(
                            "[cdp/bm] WS connect attempt {} timed out after {}ms; aborting attempt",
                            attempt,
                            attempt_timeout.as_millis()
                        );
                    }
                }
                sleep(Duration::from_millis(200)).await;
            }

            let base = "CDP WebSocket connect failed after all attempts".to_string();
            let msg = if let Some(e) = last_err { format!("{}: {}", base, e) } else { base };
            return Err(BrowserError::CdpError(msg));
        }

        // Only try CDP connection via port, no fallback
        if let Some(port) = config.connect_port {
            let host = config.connect_host.as_deref().unwrap_or("127.0.0.1");
            let actual_port = if port == 0 {
                info!("Auto-scanning for Chrome debug ports...");
                let start = tokio::time::Instant::now();
                let result = scan_for_chrome_debug_port().await.unwrap_or(0);
                info!(
                    "Auto-scan completed in {:?}, found port: {}",
                    start.elapsed(),
                    result
                );
                result
            } else {
                info!("[cdp/bm] Using specified Chrome debug port: {}", port);
                port
            };

            if actual_port > 0 {
                info!("[cdp/bm] Discovering Chrome WebSocket URL via {}:{}...", host, actual_port);
                // Retry discovery for up to 15s to allow a freshly launched Chrome to initialize
                let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
                let ws = loop {
                    let discover_start = tokio::time::Instant::now();
                    match discover_ws_via_host_port(host, actual_port).await {
                        Ok(ws) => {
                            info!("[cdp/bm] WS discovered in {:?}: {}", discover_start.elapsed(), ws);
                            break ws;
                        }
                        Err(e) => {
                            if tokio::time::Instant::now() >= deadline {
                                return Err(BrowserError::CdpError(format!(
                                    "Failed to discover Chrome WebSocket on port {} within 15s: {}",
                                    actual_port, e
                                )));
                            }
                            tokio::time::sleep(Duration::from_millis(300)).await;
                        }
                    }
                };

                info!("[cdp/bm] Connecting to Chrome via WebSocket...");
                let connect_start = tokio::time::Instant::now();

                // Enforce per-attempt timeouts via spawned task to avoid hangs
                let attempt_timeout = Duration::from_millis(config.connect_attempt_timeout_ms);
                let attempts = std::cmp::max(1, config.connect_attempts as i32);
                let mut last_err: Option<String> = None;

                for attempt in 1..=attempts {
                    info!(
                        "[cdp/bm] WS connect attempt {}/{} (timeout={}ms)",
                        attempt,
                        attempts,
                        attempt_timeout.as_millis()
                    );

                    let ws_clone = ws.clone();
                    let handle = tokio::spawn(async move { Browser::connect(ws_clone).await });

                    match tokio::time::timeout(attempt_timeout, handle).await {
                        Ok(Ok(Ok((browser, mut handler)))) => {
                            info!("[cdp/bm] WS connect attempt {} succeeded", attempt);
                            info!("[cdp/bm] Connected to Chrome in {:?}", connect_start.elapsed());

                            // Start event handler loop
                            let browser_arc = self.browser.clone();
                            let page_arc = self.page.clone();
                            let background_page_arc = self.background_page.clone();
                            let task = tokio::spawn(async move {
                                let mut consecutive_errors = 0u32;
                                while let Some(result) = handler.next().await {
                                    if should_stop_handler("[cdp/bm]", result, &mut consecutive_errors) {
                                        break;
                                    }
                                }
                                warn!("[cdp/bm] event handler ended; clearing browser state so it can restart");
                                *browser_arc.lock().await = None;
                                *page_arc.lock().await = None;
                                *background_page_arc.lock().await = None;
                            });
                            *self.event_task.lock().await = Some(task);

                            // Install browser
                            {
                                let mut guard = self.browser.lock().await;
                                *guard = Some(browser);
                            }
                            *self.cleanup_profile_on_drop.lock().await = false;

                            // Fire-and-forget targets warmup after browser is installed
                            {
                                let browser_arc = self.browser.clone();
                                tokio::spawn(async move {
                                    if let Some(browser) = browser_arc.lock().await.as_mut() {
                                        let _ = tokio::time::timeout(Duration::from_millis(100), browser.fetch_targets()).await;
                                    }
                                });
                            }

                            self.start_idle_monitor().await;
                            self.update_activity().await;
                            // Update last connection cache
                            global::set_last_connection(Some(actual_port), Some(ws.clone())).await;
                            return Ok(());
                        }
                        Ok(Ok(Err(e))) => {
                            let msg = format!("CDP WebSocket connect failed: {}", e);
                            warn!("[cdp/bm] {}", msg);
                            last_err = Some(msg);
                        }
                        Ok(Err(join_err)) => {
                            let msg = format!("Join error during connect attempt: {}", join_err);
                            warn!("[cdp/bm] {}", msg);
                            last_err = Some(msg);
                        }
                        Err(_) => {
                            warn!(
                                "[cdp/bm] WS connect attempt {} timed out after {}ms; aborting attempt",
                                attempt,
                                attempt_timeout.as_millis()
                            );
                            // Best-effort abort; if connect is internally blocking, it may keep a worker thread busy,
                            // but our caller remains responsive and we can retry.
                            // We cannot await the handle here without risking another stall.
                        }
                    }

                    // Small backoff between attempts
                    sleep(Duration::from_millis(200)).await;
                }

                let base = "CDP WebSocket connect failed after all attempts".to_string();
                let msg = if let Some(e) = last_err { format!("{}: {}", base, e) } else { base };
                return Err(BrowserError::CdpError(msg));
            } else {
                return Err(BrowserError::CdpError(
                    "No Chrome instance found with debug port".to_string(),
                ));
            }
        } else {
            return Err(BrowserError::CdpError(
                "No CDP port configured for Chrome connection".to_string(),
            ));
        }
    }

    pub async fn start(&self) -> Result<()> {
        if self.browser.lock().await.is_some() {
            return Ok(());
        }

        let config = self.config.read().await.clone();

        // 1) Attach to a live Chrome, if requested
        if let Some(ws) = config.connect_ws.clone() {
            info!("Connecting to Chrome via WebSocket: {}", ws);
            // Use the same guarded connect strategy as connect_to_chrome_only
            let attempt_timeout = Duration::from_millis(config.connect_attempt_timeout_ms);
            let attempts = std::cmp::max(1, config.connect_attempts as i32);
            let mut last_err: Option<String> = None;

            for attempt in 1..=attempts {
                info!(
                    "[cdp/bm] WS connect attempt {}/{} (timeout={}ms)",
                    attempt,
                    attempts,
                    attempt_timeout.as_millis()
                );
                let ws_clone = ws.clone();
                let handle = tokio::spawn(async move { Browser::connect(ws_clone).await });
            match tokio::time::timeout(attempt_timeout, handle).await {
                Ok(Ok(Ok((browser, mut handler)))) => {
                    info!("[cdp/bm] WS connect attempt {} succeeded", attempt);
                    // Start event handler loop
                    let browser_arc = self.browser.clone();
                    let page_arc = self.page.clone();
                    let background_page_arc = self.background_page.clone();
                    let task = tokio::spawn(async move {
                        let mut consecutive_errors = 0u32;
                        while let Some(result) = handler.next().await {
                            if should_stop_handler("[cdp/bm]", result, &mut consecutive_errors) {
                                break;
                            }
                        }
                        warn!("[cdp/bm] event handler ended; clearing browser state so it can restart");
                        *browser_arc.lock().await = None;
                        *page_arc.lock().await = None;
                        *background_page_arc.lock().await = None;
                    });
                    *self.event_task.lock().await = Some(task);
                    {
                        let mut guard = self.browser.lock().await;
                        *guard = Some(browser);
                    }
                    *self.cleanup_profile_on_drop.lock().await = false;

                    // Fire-and-forget targets warmup after browser is installed
                    {
                        let browser_arc = self.browser.clone();
                        tokio::spawn(async move {
                            if let Some(browser) = browser_arc.lock().await.as_mut() {
                                let _ = tokio::time::timeout(Duration::from_millis(100), browser.fetch_targets()).await;
                            }
                        });
                    }
                    self.start_idle_monitor().await;
                    self.update_activity().await;
                    return Ok(());
                }
                    Ok(Ok(Err(e))) => {
                        let msg = format!("CDP WebSocket connect failed: {}", e);
                        warn!("[cdp/bm] {}", msg);
                        last_err = Some(msg);
                    }
                    Ok(Err(join_err)) => {
                        let msg = format!("Join error during connect attempt: {}", join_err);
                        warn!("[cdp/bm] {}", msg);
                        last_err = Some(msg);
                    }
                    Err(_) => {
                        warn!(
                            "[cdp/bm] WS connect attempt {} timed out after {}ms; aborting attempt",
                            attempt,
                            attempt_timeout.as_millis()
                        );
                    }
                }
                sleep(Duration::from_millis(200)).await;
            }

            let base = "CDP WebSocket connect failed after all attempts".to_string();
            let msg = if let Some(e) = last_err { format!("{}: {}", base, e) } else { base };
            return Err(BrowserError::CdpError(msg));
        }

        if let Some(port) = config.connect_port {
            let host = config.connect_host.as_deref().unwrap_or("127.0.0.1");
            let actual_port = if port == 0 {
                info!("Auto-scanning for Chrome debug ports...");
                let start = tokio::time::Instant::now();
                let result = scan_for_chrome_debug_port().await.unwrap_or(0);
                info!(
                    "Auto-scan completed in {:?}, found port: {}",
                    start.elapsed(),
                    result
                );
                result
            } else {
                info!("Using specified Chrome debug port: {}", port);
                port
            };

            if actual_port > 0 {
                info!("Step 1: Discovering Chrome WebSocket URL via {}:{}...", host, actual_port);
                let ws = loop {
                    let discover_start = tokio::time::Instant::now();
                    match discover_ws_via_host_port(host, actual_port).await {
                        Ok(ws) => {
                            info!(
                                "Step 2: WebSocket URL discovered in {:?}: {}",
                                discover_start.elapsed(),
                                ws
                            );
                            break ws;
                        }
                        Err(e) => {
                            if tokio::time::Instant::now() - discover_start > Duration::from_secs(15) {
                                return Err(BrowserError::CdpError(format!(
                                    "Failed to discover Chrome WebSocket on port {} within 15s: {}",
                                    actual_port, e
                                )));
                            }
                            tokio::time::sleep(Duration::from_millis(300)).await;
                        }
                    }
                };

                info!("Step 3: Connecting to Chrome via WebSocket...");
                let connect_start = tokio::time::Instant::now();
                // Use guarded connect strategy with retries
                let attempt_timeout = Duration::from_millis(config.connect_attempt_timeout_ms);
                let attempts = std::cmp::max(1, config.connect_attempts as i32);
                let mut last_err: Option<String> = None;

                for attempt in 1..=attempts {
                    info!(
                        "[cdp/bm] WS connect attempt {}/{} (timeout={}ms)",
                        attempt,
                        attempts,
                        attempt_timeout.as_millis()
                    );
                    let ws_clone = ws.clone();
                    let handle = tokio::spawn(async move { Browser::connect(ws_clone).await });
                    match tokio::time::timeout(attempt_timeout, handle).await {
                        Ok(Ok(Ok((browser, mut handler)))) => {
                            info!("[cdp/bm] WS connect attempt {} succeeded", attempt);
                            info!(
                                "Step 4: Connected to Chrome in {:?}",
                                connect_start.elapsed()
                            );

                            // Start event handler
                            let browser_arc = self.browser.clone();
                            let page_arc = self.page.clone();
                            let background_page_arc = self.background_page.clone();
                            let task = tokio::spawn(async move {
                                let mut consecutive_errors = 0u32;
                                while let Some(result) = handler.next().await {
                                    if should_stop_handler("[cdp/bm]", result, &mut consecutive_errors) {
                                        break;
                                    }
                                }
                                warn!("[cdp/bm] event handler ended; clearing browser state so it can restart");
                                *browser_arc.lock().await = None;
                                *page_arc.lock().await = None;
                                *background_page_arc.lock().await = None;
                            });
                            *self.event_task.lock().await = Some(task);
                            {
                                let mut guard = self.browser.lock().await;
                                *guard = Some(browser);
                            }
                            *self.cleanup_profile_on_drop.lock().await = false;

                            // Fire-and-forget targets warmup after browser is installed
                            {
                                let browser_arc = self.browser.clone();
                                tokio::spawn(async move {
                                    if let Some(browser) = browser_arc.lock().await.as_mut() {
                                        let _ = tokio::time::timeout(Duration::from_millis(100), browser.fetch_targets()).await;
                                    }
                                });
                            }

                            info!("Step 5: Starting idle monitor...");
                            self.start_idle_monitor().await;
                            self.update_activity().await;
                            info!("Step 6: Chrome connection complete!");
                            return Ok(());
                        }
                        Ok(Ok(Err(e))) => {
                            let msg = format!("CDP WebSocket connect failed: {}", e);
                            warn!("[cdp/bm] {}", msg);
                            last_err = Some(msg);
                        }
                        Ok(Err(join_err)) => {
                            let msg = format!("Join error during connect attempt: {}", join_err);
                            warn!("[cdp/bm] {}", msg);
                            last_err = Some(msg);
                        }
                        Err(_) => {
                            warn!(
                                "[cdp/bm] WS connect attempt {} timed out after {}ms; aborting attempt",
                                attempt,
                                attempt_timeout.as_millis()
                            );
                        }
                    }
                    sleep(Duration::from_millis(200)).await;
                }

                let base = "CDP WebSocket connect failed after all attempts".to_string();
                let msg = if let Some(e) = last_err { format!("{}: {}", base, e) } else { base };
                return Err(BrowserError::CdpError(msg));
            }
        }

        // 2) Launch a browser
        info!("Launching new browser instance");
        // Prevent redundant browser launches within the same process.
        let _launch_guard = INTERNAL_BROWSER_LAUNCH_GUARD.lock().await;
        // Also serialize launches across Code processes; concurrent Chromium spawns
        // are a common source of transient EAGAIN/ENOMEM failures on macOS.
        let _launch_lock = BrowserLaunchLockFile::acquire(Duration::from_secs(30)).await?;

        const INTERNAL_LAUNCH_RETRY_DELAYS_MS: [u64; 5] = [50, 200, 500, 1000, 2000];
        let max_attempts = INTERNAL_LAUNCH_RETRY_DELAYS_MS.len() + 1;

        // Add browser launch flags (keep minimal set for screenshot functionality)
        let log_file = resolve_chrome_log_path();

        let (browser, mut handler, user_data_path) = {
            let mut attempt = 1usize;
            loop {
                // Profile dir
                let (user_data_path, is_temp_profile) = if let Some(dir) = &config.user_data_dir {
                    (dir.to_string_lossy().to_string(), false)
                } else {
                    let pid = std::process::id();
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();
                    let temp_path = format!("/tmp/code-browser-{pid}-{timestamp}-{attempt}");
                    if tokio::fs::metadata(&temp_path).await.is_ok() {
                        let _ = tokio::fs::remove_dir_all(&temp_path).await;
                    }
                    (temp_path, true)
                };

                let mut builder = CdpConfig::builder().user_data_dir(&user_data_path);

                // Set headless mode based on config (keep original approach for stability)
                if config.headless {
                    builder = builder.headless_mode(HeadlessMode::New);
                }

                // Configure viewport (revert to original approach for screenshot stability)
                builder = builder.window_size(config.viewport.width, config.viewport.height);

                builder = builder
                    .arg("--disable-blink-features=AutomationControlled")
                    .arg("--no-first-run")
                    .arg("--no-default-browser-check")
                    .arg("--disable-component-extensions-with-background-pages")
                    .arg("--disable-background-networking")
                    .arg("--silent-debugger-extension-api")
                    .arg("--remote-allow-origins=*")
                    .arg("--disable-features=ChromeWhatsNewUI,TriggerFirstRunUI")
                    // Disable timeout for slow networks/pages
                    .arg("--disable-hang-monitor")
                    .arg("--disable-background-timer-throttling")
                    // Suppress console output
                    .arg("--silent-launch")
                    // Set a longer timeout for CDP requests (60 seconds instead of default 30)
                    .request_timeout(Duration::from_secs(60));

                if let Some(ref log_file) = log_file {
                    builder = builder
                        .arg("--enable-logging")
                        .arg("--log-level=1")
                        .arg(format!("--log-file={}", log_file.display()));
                }

                let browser_config = builder
                    .build()
                    .map_err(|e| BrowserError::CdpError(e.to_string()))?;

                match Browser::launch(browser_config).await {
                    Ok((browser, handler)) => break (browser, handler, user_data_path),
                    Err(e) => {
                        let message = e.to_string();

                        let is_temporary = is_temporary_internal_launch_error_message(&message);
                        if is_temp_profile {
                            let _ = tokio::fs::remove_dir_all(&user_data_path).await;
                        }

                        if is_temporary && attempt < max_attempts {
                            let delay_ms = INTERNAL_LAUNCH_RETRY_DELAYS_MS[attempt - 1];
                            warn!(
                                error = %message,
                                attempt,
                                delay_ms,
                                "Internal browser launch failed with transient error; retrying"
                            );
                            sleep(Duration::from_millis(delay_ms)).await;
                            attempt += 1;
                            continue;
                        }

                        #[cfg(target_os = "macos")]
                        let hint = "Ensure Google Chrome or Chromium is installed and runnable (e.g., /Applications/Google Chrome.app).";
                        #[cfg(target_os = "linux")]
                        let hint = "Ensure google-chrome or chromium is installed and available on PATH.";
                        #[cfg(target_os = "windows")]
                        let hint = "Ensure Chrome is installed and chrome.exe is available (typically in Program Files).";
                        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
                        let hint = "Ensure Chrome/Chromium is installed and available on PATH.";

                        let log_note = log_file
                            .as_ref()
                            .map(|path| format!(" Chrome log: {}", path.display()))
                            .unwrap_or_default();
                        return Err(BrowserError::CdpError(format!(
                            "Failed to launch internal browser: {message}. Hint: {hint}.{log_note}"
                        )));
                    }
                }
            }
        };
        // Optionally: browser.fetch_targets().await.ok();

        let browser_arc = self.browser.clone();
        let page_arc = self.page.clone();
        let background_page_arc = self.background_page.clone();
        let task = tokio::spawn(async move {
            let mut consecutive_errors = 0u32;
            while let Some(result) = handler.next().await {
                if should_stop_handler("[cdp/bm]", result, &mut consecutive_errors) {
                    break;
                }
            }
            warn!("[cdp/bm] event handler ended; clearing browser state so it can restart");
            *browser_arc.lock().await = None;
            *page_arc.lock().await = None;
            *background_page_arc.lock().await = None;
        });
        *self.event_task.lock().await = Some(task);

        {
            let mut guard = self.browser.lock().await;
            *guard = Some(browser);
        }
        *self.user_data_dir.lock().await = Some(user_data_path.clone());

        let should_cleanup = config.user_data_dir.is_none() || !config.persist_profile;
        *self.cleanup_profile_on_drop.lock().await = should_cleanup;

        self.start_idle_monitor().await;
        self.update_activity().await;
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.stop_idle_monitor().await;

        // stop event handler task cleanly
        if let Some(task) = self.event_task.lock().await.take() {
            task.abort();
        }

        self.stop_navigation_monitor().await;

        let mut page_guard = self.page.lock().await;
        *page_guard = None;

        // Also cleanup the background page
        let mut background_page_guard = self.background_page.lock().await;
        *background_page_guard = None;

        let config = self.config.read().await;
        let is_external_chrome = config.connect_port.is_some() || config.connect_ws.is_some();
        drop(config);

        let mut browser_guard = self.browser.lock().await;
        if let Some(mut browser) = browser_guard.take() {
            if is_external_chrome {
                info!("Disconnecting from external Chrome (not closing it)");
                // Just drop the connection, don't close the browser
            } else {
                info!("Stopping browser we launched");
                browser.close().await?;
            }
        }

        // When cleaning profiles, respect the flag everywhere:
        let should_cleanup = *self.cleanup_profile_on_drop.lock().await;
        if should_cleanup {
            let mut user_data_guard = self.user_data_dir.lock().await;
            if let Some(user_data_path) = user_data_guard.take() {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let _ = tokio::fs::remove_dir_all(&user_data_path).await;
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

        Ok(())
    }

    pub async fn get_or_create_page(&self) -> Result<Arc<Page>> {
        let overall_start = Instant::now();
        info!("[bm] get_or_create_page: begin");
        self.ensure_browser().await?;
        info!("[bm] get_or_create_page: ensure_browser in {:?}", overall_start.elapsed());
        self.update_activity().await;

        let mut page_guard = self.page.lock().await;
        if let Some(page) = page_guard.as_ref() {
            // Verify the page is still responsive
            let check_result =
                tokio::time::timeout(Duration::from_secs(2), page.get_current_url()).await;

            match check_result {
                Ok(Ok(_)) => {
                    // Page is responsive
                    info!(
                        "[bm] get_or_create_page: reused responsive page in {:?}",
                        overall_start.elapsed()
                    );
                    return Ok(Arc::clone(page));
                }
                Ok(Err(e)) => {
                    warn!("Existing page returned error: {}, will create new page", e);
                    *page_guard = None;
                }
                Err(_) => {
                    // Timeout checking URL; prefer to reuse instead of re-applying overrides repeatedly
                    warn!("Existing page timed out checking URL; reusing current page to avoid churn");
                    return Ok(Arc::clone(page));
                }
            }
        }

        let browser_guard = self.browser.lock().await;
        let browser = browser_guard.as_ref().ok_or(BrowserError::NotInitialized)?;

        let config = self.config.read().await;

        // If we're connected to an existing Chrome (via connect_port or connect_ws),
        // try to use the current active tab instead of creating a new one
        let cdp_page = if config.connect_port.is_some() || config.connect_ws.is_some() {
            info!("[bm] get_or_create_page: selecting an existing tab");
            // Try to get existing pages
            let mut pages = browser.pages().await?;
            if pages.is_empty() {
                // brief retry loop to allow targets to populate
                for _ in 0..10 {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    pages = browser.pages().await?;
                    if !pages.is_empty() { break; }
                }
            }

            if !pages.is_empty() {
                // Try to find the active/visible tab
                // We'll check each page to see if it's visible/focused
                let mut active_page = None;                // focused && visible
                let mut first_visible: Option<chromiumoxide::page::Page> = None; // visible
                let mut last_allowed: Option<chromiumoxide::page::Page> = None;  // allowed regardless of visibility

                // Helper: determine if a URL is controllable (we can inject/evaluate)
                let is_allowed = |u: &str| {
                    let lu = u.to_lowercase();
                    if lu.starts_with("chrome://")
                        || lu.starts_with("devtools://")
                        || lu.starts_with("edge://")
                        || lu.starts_with("chrome-extension://")
                        || lu.starts_with("brave://")
                        || lu.starts_with("vivaldi://")
                        || lu.starts_with("opera://")
                    {
                        return false;
                    }
                    // Allow http/https/file/about:blank
                    lu.starts_with("http://")
                        || lu.starts_with("https://")
                        || lu.starts_with("file://")
                        || lu == "about:blank"
                };

                for page in &pages {
                    // Quick URL check first to skip uninjectable pages
                    let url = match tokio::time::timeout(Duration::from_millis(200), page.url()).await {
                        Ok(Ok(Some(u))) => u,
                        _ => "unknown".to_string(),
                    };
                    if !is_allowed(&url) {
                        debug!("Skipping uncontrollable tab: {}", url);
                        continue;
                    } else {
                        last_allowed = Some(page.clone());
                    }
                    // Evaluate visibility/focus of the tab. We avoid focus listeners since they won't fire when attaching.
                    let eval = page.evaluate(
                        "(() => {\n"
                        .to_string()
                        + "  return {\n"
                        + "    visible: document.visibilityState === 'visible',\n"
                        + "    focused: (document.hasFocus && document.hasFocus()) || false,\n"
                        + "    url: String(window.location.href || '')\n"
                        + "  };\n"
                        + "})()",
                    );
                    // Guard against hung targets by timing out quickly
                    let is_visible = tokio::time::timeout(Duration::from_millis(300), eval).await;

                    if let Ok(Ok(result)) = is_visible {
                        if let Ok(obj) = result.into_value::<serde_json::Value>() {
                            let visible = obj
                                .get("visible")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            let focused = obj
                                .get("focused")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            let url = obj.get("url").and_then(|v| v.as_str()).unwrap_or("unknown");

                            debug!("Tab check - URL: {}, Visible: {}, Focused: {}", url, visible, focused);

                            // Selection heuristic (revised to avoid minimized windows):
                            // 1) Focused AND visible wins immediately.
                            // 2) Otherwise, remember the first visible tab.
                            // 3) Otherwise, fallback to the last allowed tab.
                            if focused && visible {
                                info!("Found focused & visible tab: {}", url);
                                active_page = Some(page.clone());
                                break;
                            } else if focused && !visible {
                                info!("Focused but not visible (likely minimized): skipping {}", url);
                            } else if visible && first_visible.is_none() {
                                info!("Found visible tab: {}", url);
                                first_visible = Some(page.clone());
                            }
                        } else {
                            debug!("Tab visibility check returned non-JSON; skipping");
                        }
                    } else {
                        debug!("Tab visibility check timed out or failed; skipping unresponsive tab");
                    }
                }

                // Use focused & visible if found, else first visible, else last allowed
                if let Some(page) = active_page {
                    info!("Using active/visible Chrome tab");
                    page
                } else if let Some(page) = first_visible {
                    info!("Using first visible Chrome tab");
                    page
                } else {
                    if let Some(p) = last_allowed {
                        info!("No active tab found, using last allowed tab");
                        p
                    } else {
                        // No allowed pages at all, create an about:blank tab
                        warn!("No controllable tabs found; creating about:blank");
                        browser.new_page("about:blank").await?
                    }
                }
            } else {
                // No existing tabs found. Do NOT create a new tab for external Chrome if avoidable.
                info!("No existing tabs found; waiting briefly for targets");
                tokio::time::sleep(Duration::from_millis(200)).await;
                let mut pages2 = browser.pages().await?;
                if !pages2.is_empty() {
                    pages2.pop().unwrap()
                } else {
                    // As a last resort, still create a tab, but log it clearly
                    warn!("Creating a new about:blank tab because none were available");
                    browser.new_page("about:blank").await?
                }
            }
        } else {
            // We launched Chrome ourselves, create a new page
            info!("[bm] get_or_create_page: creating new about:blank tab");
            browser.new_page("about:blank").await?
        };

        // Apply page overrides (UA, locale, timezone, viewport, etc.)
        let overrides_start = Instant::now();
        self.apply_page_overrides(&cdp_page).await?;
        info!("[bm] get_or_create_page: overrides in {:?}", overrides_start.elapsed());

        let page = Arc::new(Page::new(cdp_page, config.clone()));
        *page_guard = Some(Arc::clone(&page));

        // Inject the virtual cursor when page is created
        debug!("Injecting virtual cursor for new page");
        if let Err(e) = page.inject_virtual_cursor().await {
            warn!("Failed to inject virtual cursor on page creation: {}", e);
            // Continue even if cursor injection fails
        }

        // Ensure console capture is installed immediately for the current document.
        // Without this, connecting to an already-loaded tab would only register
        // the bootstrap for future documents, and an initial Browser Console read
        // would return no logs. This eagerly hooks console methods now.
        let console_hook = r#"(function(){
            try {
                if (!window.__code_console_logs) {
                    window.__code_console_logs = [];
                    const push = (level, message) => {
                        try {
                            window.__code_console_logs.push({ timestamp: new Date().toISOString(), level, message });
                            if (window.__code_console_logs.length > 2000) window.__code_console_logs.shift();
                        } catch (_) {}
                    };

                    ['log','warn','error','info','debug'].forEach(function(method) {
                        try {
                            const orig = console[method];
                            console[method] = function() {
                                try {
                                    var args = Array.prototype.slice.call(arguments);
                                    var msg = args.map(function(a) {
                                        try {
                                            if (a && typeof a === 'object') return JSON.stringify(a);
                                            return String(a);
                                        } catch (_) { return String(a); }
                                    }).join(' ');
                                    push(method, msg);
                                } catch(_) {}
                                if (orig) return orig.apply(console, arguments);
                            };
                        } catch(_) {}
                    });

                    window.addEventListener('error', function(e) {
                        try {
                            var msg = e && e.message ? e.message : 'Script error';
                            var stack = e && e.error && e.error.stack ? ('\n' + e.error.stack) : '';
                            push('exception', msg + stack);
                        } catch(_) {}
                    });
                    window.addEventListener('unhandledrejection', function(e) {
                        try {
                            var reason = e && e.reason;
                            if (reason && typeof reason === 'object') { try { reason = JSON.stringify(reason); } catch(_) {} }
                            push('unhandledrejection', String(reason));
                        } catch(_) {}
                    });
                }
                return true;
            } catch (_) { return false; }
        })()"#;
        if let Err(e) = page.inject_js(console_hook).await {
            warn!("Failed to install console capture on page creation: {}", e);
        }

        // Start navigation monitoring for this page
        self.start_navigation_monitor(Arc::clone(&page)).await;
        // Start viewport monitor (low-frequency, non-invasive)
        self.start_viewport_monitor(Arc::clone(&page)).await;
        // TEMP: disable auto-corrections post-initial set to validate no unintended resizes
        // This affects both external and internal; explicit browser.setViewport still works
        self.set_auto_viewport_correction(false).await;
        info!(
            "[bm] get_or_create_page: complete in {:?}",
            overall_start.elapsed()
        );

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

    /// Returns how long the browser has been idle when it exceeds the configured timeout.
    /// Used to decide whether we should avoid taking fresh screenshots (which would reset
    /// the idle timer) until the user interacts with browser_* tools again.
    pub async fn idle_elapsed_past_timeout(&self) -> Option<(Duration, Duration)> {
        let idle_timeout = Duration::from_millis(self.config.read().await.idle_timeout_ms);
        let last = *self.last_activity.lock().await;
        let elapsed = last.elapsed();
        if elapsed > idle_timeout {
            Some((elapsed, idle_timeout))
        } else {
            None
        }
    }

    /// Get a description of the browser connection type
    pub async fn get_browser_type(&self) -> String {
        let config = self.config.read().await;
        if config.connect_ws.is_some() || config.connect_port.is_some() {
            "CDP-connected to user's Chrome browser".to_string()
        } else if config.headless {
            "internal headless Chrome browser".to_string()
        } else {
            "internal Chrome browser (headed mode)".to_string()
        }
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
            // Avoid viewport manipulation for external CDP connections to prevent focus/flicker
            let is_external = config.connect_port.is_some() || config.connect_ws.is_some();
            if !is_external {
                page.update_viewport(config.viewport.clone()).await?;
            }
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

    /// Apply environment overrides on page creation.
    /// - For external CDP connections: set viewport once on connect; skip humanization (UA, locale, etc.).
    /// - For internal (launched) Chrome: apply humanization; skip viewport here (kept minimal).
    pub async fn apply_page_overrides(&self, page: &chromiumoxide::Page) -> Result<()> {
        let config = self.config.read().await;
        let is_external = config.connect_port.is_some() || config.connect_ws.is_some();

        // Always enable Network domain once
        page.execute(network::EnableParams::default()).await?;

        if is_external {
            // External Chrome: set viewport once on connection; skip humanization.
            let w = config.viewport.width as i64;
            let h = config.viewport.height as i64;
            let dpr = config.viewport.device_scale_factor as f64;
            let mob = config.viewport.mobile;

            // Skip redundant overrides within a short window to prevent flash
            {
                let guard = self.last_metrics_applied.lock().await;
                if let Some((lw, lh, ldpr, lmob, ts)) = *guard {
                    let same = lw == w && lh == h && (ldpr - dpr).abs() < 0.001 && lmob == mob;
                    let recent = ts.elapsed() < std::time::Duration::from_secs(30);
                    if same && recent {
                        debug!("Skipping redundant device metrics override (external, recent)");
                        return Ok(());
                    }
                }
            }

            let viewport_params = emulation::SetDeviceMetricsOverrideParams::builder()
                .width(w)
                .height(h)
                .device_scale_factor(dpr)
                .mobile(mob)
                .build()
                .map_err(BrowserError::CdpError)?;
            info!("Applying external device metrics override: {}x{} @ {} (mobile={})", w, h, dpr, mob);
            page.execute(viewport_params).await?;
            let mut guard = self.last_metrics_applied.lock().await;
            *guard = Some((w, h, dpr, mob, std::time::Instant::now()));
        } else {
            // Internal (launched) Chrome: apply human settings; avoid CDP viewport override here
            if let Some(ua) = &config.user_agent {
                let mut b = network::SetUserAgentOverrideParams::builder().user_agent(ua);
                if let Some(al) = &config.accept_language {
                    b = b.accept_language(al);
                }
                page.execute(b.build().map_err(BrowserError::CdpError)?)
                    .await?;
            } else if let Some(al) = &config.accept_language {
                let mut headers_map = std::collections::HashMap::new();
                headers_map.insert(
                    "Accept-Language".to_string(),
                    serde_json::Value::String(al.clone()),
                );
                let headers = network::Headers::new(serde_json::Value::Object(
                    headers_map.into_iter().map(|(k, v)| (k, v)).collect(),
                ));
                let p = network::SetExtraHttpHeadersParams::builder()
                    .headers(headers)
                    .build()
                    .map_err(BrowserError::CdpError)?;
                page.execute(p).await?;
            }

            if let Some(tz) = &config.timezone {
                page.execute(emulation::SetTimezoneOverrideParams {
                    timezone_id: tz.clone(),
                })
                .await?;
            }
            if let Some(locale) = &config.locale {
                let p = emulation::SetLocaleOverrideParams::builder()
                    .locale(locale)
                    .build();
                page.execute(p).await?;
            }
        }

        Ok(())
    }

    async fn ensure_browser(&self) -> Result<()> {
        let mut browser_guard = self.browser.lock().await;

        // Check if we have a browser instance
        if let Some(browser) = browser_guard.as_ref() {
            // Try to verify it's still connected with a simple operation
            let check_result =
                tokio::time::timeout(Duration::from_secs(2), browser.version()).await;

            match check_result {
                Ok(Ok(_)) => {
                    // Browser is responsive
                    return Ok(());
                }
                Ok(Err(e)) => {
                    warn!("Browser check failed: {}, will restart", e);
                    *browser_guard = None;
                }
                Err(_) => {
                    warn!("Browser check timed out, likely disconnected. Will restart");
                    *browser_guard = None;
                }
            }
        }

        // Need to start or restart the browser
        drop(browser_guard);
        info!("Starting/restarting browser connection...");
        self.start().await?;
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
        let cfg = self
            .config
            .try_read()
            .map(|c| {
                let enabled = c.enabled;
                let viewport_width = c.viewport.width;
                let viewport_height = c.viewport.height;
                let fullpage = c.fullpage;
                (enabled, viewport_width, viewport_height, fullpage)
            })
            .unwrap_or((false, 1024, 768, false));

        let browser_active = self
            .browser
            .try_lock()
            .map(|b| b.is_some())
            .unwrap_or(false);

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
        let is_external_chrome = config.connect_port.is_some() || config.connect_ws.is_some();
        let should_cleanup = *self.cleanup_profile_on_drop.lock().await; // <-- respect this
        drop(config);

        if is_external_chrome {
            info!("Skipping idle monitor for external Chrome connection");
            return;
        }

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
                    if should_cleanup {
                        if let Some(user_data_path) = user_data_dir.lock().await.take() {
                            let _ = tokio::fs::remove_dir_all(&user_data_path).await;
                        }
                    }
                    break;
                }
            }
        });

        *self.idle_monitor_handle.lock().await = Some(handle);
    }

    async fn stop_idle_monitor(&self) {
        let mut handle_guard = self.idle_monitor_handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }
    }

    pub async fn goto(&self, url: &str) -> Result<crate::page::GotoResult> {
        const MAX_RECOVERY_ATTEMPTS: usize = 2; // number of retries after the initial attempt
        let mut recovery_attempts = 0usize;

        loop {
            match self.goto_once(url).await {
                Ok(result) => {
                    if recovery_attempts > 0 {
                        info!(
                            "Browser navigation succeeded after {} recovery attempt(s)",
                            recovery_attempts
                        );
                    }
                    return Ok(result);
                }
                Err(err) => {
                    let should_retry =
                        recovery_attempts < MAX_RECOVERY_ATTEMPTS
                            && self.should_retry_after_goto_error(&err).await;

                    self
                        .log_navigation_failure(url, &err, recovery_attempts, should_retry)
                        .await;

                    if !should_retry {
                        return Err(err);
                    }

                    warn!(
                        error = %err,
                        recovery_attempt = recovery_attempts + 1,
                        "Browser navigation failed; restarting browser before retry"
                    );

                    if let Err(stop_err) = self.stop().await {
                        warn!("Failed to stop browser during recovery: {}", stop_err);
                    }

                    tokio::time::sleep(Duration::from_millis(400)).await;
                    recovery_attempts += 1;
                }
            }
        }
    }

    async fn log_navigation_failure(
        &self,
        url: &str,
        err: &BrowserError,
        recovery_attempt: usize,
        will_retry: bool,
    ) {
        let error_string = err.to_string();
        let config = self.config.read().await.clone();
        let is_external = config.connect_port.is_some() || config.connect_ws.is_some();
        let browser_active = self.browser.lock().await.is_some();
        let wait_desc = match &config.wait {
            WaitStrategy::Event(event) => format!("event:{event}"),
            WaitStrategy::Delay { delay_ms } => format!("delay:{delay_ms}ms"),
        };

        warn!(
            url = %url,
            error = %error_string,
            recovery_attempt,
            will_retry,
            browser_active,
            is_external,
            headless = config.headless,
            connect_port = ?config.connect_port,
            connect_ws = ?config.connect_ws,
            viewport_width = config.viewport.width,
            viewport_height = config.viewport.height,
            viewport_dpr = config.viewport.device_scale_factor,
            viewport_mobile = config.viewport.mobile,
            wait = %wait_desc,
            "Browser navigation failed"
        );

        let page_snapshot = self.page.lock().await.clone();
        if let Some(page) = page_snapshot.as_ref() {
            let page_debug = Self::collect_page_debug_info(page).await;
            warn!(
                target_id = %page_debug.target_id,
                session_id = %page_debug.session_id,
                opener_id = ?page_debug.opener_id,
                cached_url = ?page_debug.cached_url,
                live_url = ?page_debug.live_url,
                "Browser page debug context"
            );
        } else {
            warn!("Browser navigation failed without an active page");
        }

        if let Some(snapshot) = self.collect_target_snapshot().await {
            warn!(
                targets_total = snapshot.total,
                targets_truncated = snapshot.truncated,
                targets_sample = ?snapshot.sample,
                "Browser target snapshot"
            );
        }
    }

    async fn collect_page_debug_info(page: &Page) -> PageDebugInfo {
        let cached_url = page.get_url().await.ok();
        let live_url = match tokio::time::timeout(Duration::from_millis(600), page.get_current_url()).await {
            Ok(Ok(url)) => Some(url),
            Ok(Err(err)) => {
                debug!(error = %err, "Navigation telemetry failed to read live URL");
                None
            }
            Err(_) => {
                debug!("Navigation telemetry timed out reading live URL");
                None
            }
        };

        PageDebugInfo {
            target_id: page.target_id_debug(),
            session_id: page.session_id_debug(),
            opener_id: page.opener_id_debug(),
            cached_url,
            live_url,
        }
    }

    async fn collect_target_snapshot(&self) -> Option<TargetSnapshot> {
        let mut browser_guard = self.browser.lock().await;
        let browser = browser_guard.as_mut()?;
        let fetch = tokio::time::timeout(Duration::from_millis(1200), browser.fetch_targets()).await;
        let targets = match fetch {
            Ok(Ok(targets)) => targets,
            Ok(Err(err)) => {
                warn!(error = %err, "Failed to fetch CDP targets for navigation telemetry");
                return None;
            }
            Err(_) => {
                warn!("Timed out fetching CDP targets for navigation telemetry");
                return None;
            }
        };

        let total = targets.len();
        let mut sample = Vec::new();
        for (index, target) in targets.iter().take(12).enumerate() {
            let target_id = &target.target_id;
            let target_type = &target.r#type;
            let subtype = target.subtype.as_deref().unwrap_or("-");
            let opener = target
                .opener_id
                .as_ref()
                .map(|opener_id| format!("{opener_id:?}"))
                .unwrap_or_else(|| "-".to_string());
            let url = &target.url;
            let title = &target.title;
            let attached = target.attached;
            sample.push(format!(
                "#{index} id={target_id:?} type={target_type} subtype={subtype} attached={attached} opener={opener} url={url} title={title}",
                index = index + 1,
                target_id = target_id,
                target_type = target_type,
                subtype = subtype,
                attached = attached,
                opener = opener,
                url = url,
                title = title
            ));
        }

        Some(TargetSnapshot {
            total,
            truncated: total > sample.len(),
            sample,
        })
    }

    async fn should_retry_after_goto_error(&self, err: &BrowserError) -> bool {
        let is_internal = {
            let cfg = self.config.read().await;
            cfg.connect_port.is_none() && cfg.connect_ws.is_none()
        };

        if !is_internal {
            return false;
        }

        match err {
            BrowserError::NotInitialized => true,
            BrowserError::IoError(e) => matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
            ),
            BrowserError::CdpError(msg) => {
                let msg_lower = msg.to_ascii_lowercase();
                const RECOVERABLE_SUBSTRINGS: &[&str] = &[
                    "connection closed",
                    "browser closed",
                    "target crashed",
                    "context destroyed",
                    "no such session",
                    "disconnected",
                    "transport",
                    "timeout",
                    "timed out",
                    "oneshot canceled",
                    "oneshot cancelled",
                    "resource temporarily unavailable",
                    "temporarily unavailable",
                    "eagain",
                ];

                RECOVERABLE_SUBSTRINGS
                    .iter()
                    .any(|needle| msg_lower.contains(needle))
            }
            _ => false,
        }
    }

    async fn goto_once(&self, url: &str) -> Result<crate::page::GotoResult> {
        // Get or create page
        let page = self.get_or_create_page().await?;

        let nav_start = std::time::Instant::now();
        info!("Navigating to URL: {}", url);
        let config = self.config.read().await;
        let result = page.goto(url, Some(config.wait.clone())).await?;
        info!(
            "Navigation complete to: {} in {:?}",
            result.url,
            nav_start.elapsed()
        );

        // Manually trigger navigation callback for immediate response
        if let Some(ref callback) = *self.navigation_callback.read().await {
            debug!("Manually triggering navigation callback after goto");
            callback(result.url.clone());
        }

        self.update_activity().await;
        Ok(result)
    }

    pub async fn capture_screenshot_with_url(
        &self,
    ) -> Result<(Vec<std::path::PathBuf>, Option<String>)> {
        let (paths, url) = self.capture_screenshot_internal().await?;
        Ok((paths, Some(url)))
    }

    pub async fn capture_screenshot(&self) -> Result<Vec<std::path::PathBuf>> {
        let (paths, _) = self.capture_screenshot_internal().await?;
        Ok(paths)
    }

    async fn capture_screenshot_internal(&self) -> Result<(Vec<std::path::PathBuf>, String)> {
        // Always capture from the active page; do not create background tabs.
        self.capture_screenshot_regular().await
    }

    /// Capture screenshot using regular strategy (launched Chrome)
    async fn capture_screenshot_regular(&self) -> Result<(Vec<std::path::PathBuf>, String)> {
        // For launched Chrome, use the regular approach since it's already isolated
        let page = self.get_or_create_page().await?;

        // Viewport correction is handled inside Page::screenshot for all connections

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
                segments_max: Some(config.segments_max),
            }
        } else {
            crate::page::ScreenshotMode::Viewport
        };

        // Get current URL with timeout
        let current_url =
            match tokio::time::timeout(Duration::from_secs(3), page.get_current_url()).await {
                Ok(Ok(url)) => url,
                Ok(Err(_)) | Err(_) => {
                    warn!("Failed to get current URL, using default");
                    "about:blank".to_string()
                }
            };

        // Capture screenshots with timeout
        let screenshot_result = tokio::time::timeout(
            Duration::from_secs(15), // Allow up to 15 seconds for screenshot
            page.screenshot(mode),
        )
        .await;

        let screenshots = match screenshot_result {
            Ok(Ok(shots)) => shots,
            Ok(Err(e)) => {
                return Err(BrowserError::ScreenshotError(format!(
                    "Screenshot capture failed: {}",
                    e
                )));
            }
            Err(_) => {
                return Err(BrowserError::ScreenshotError(
                    "Screenshot capture timed out after 15 seconds".to_string(),
                ));
            }
        };

        // Store screenshots and get paths
        let mut paths = Vec::new();
        for screenshot in screenshots {
            let image_ref = assets
                .store_screenshot(
                    &screenshot.data,
                    screenshot.format,
                    screenshot.width,
                    screenshot.height,
                    Self::SCREENSHOT_TTL_MS,
                )
                .await?;
            paths.push(std::path::PathBuf::from(image_ref.path));
        }

        self.update_activity().await;
        Ok((paths, current_url))
    }

    pub async fn close(&self) -> Result<()> {
        // Just delegate to stop() which handles cleanup properly
        self.stop().await
    }

    /// Move the mouse to the specified coordinates
    pub async fn move_mouse(&self, x: f64, y: f64) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.move_mouse(x, y).await
    }

    /// Move the mouse by relative offset from current position
    pub async fn move_mouse_relative(&self, dx: f64, dy: f64) -> Result<(f64, f64)> {
        let page = self.get_or_create_page().await?;
        page.move_mouse_relative(dx, dy).await
    }

    /// Click at the specified coordinates
    pub async fn click(&self, x: f64, y: f64) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.click(x, y).await
    }

    /// Click at the current mouse position
    pub async fn click_at_current(&self) -> Result<(f64, f64)> {
        let page = self.get_or_create_page().await?;
        page.click_at_current().await
    }
    
    /// Perform mouse down at the current position
    pub async fn mouse_down_at_current(&self) -> Result<(f64, f64)> {
        let page = self.get_or_create_page().await?;
        page.mouse_down_at_current().await
    }
    
    /// Perform mouse up at the current position
    pub async fn mouse_up_at_current(&self) -> Result<(f64, f64)> {
        let page = self.get_or_create_page().await?;
        page.mouse_up_at_current().await
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

    /// Scroll the page by the given delta in pixels
    pub async fn scroll_by(&self, dx: f64, dy: f64) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.scroll_by(dx, dy).await
    }

    /// Navigate browser history backward one entry
    pub async fn history_back(&self) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.go_back().await
    }

    /// Navigate browser history forward one entry
    pub async fn history_forward(&self) -> Result<()> {
        let page = self.get_or_create_page().await?;
        page.go_forward().await
    }

    /// Capture console logs from the browser, including errors and unhandled rejections
    pub async fn get_console_logs(&self, lines: Option<usize>) -> Result<serde_json::Value> {
        let page = self.get_or_create_page().await?;

        // 1) Prefer CDP-captured buffer (event-based). If we have entries, return them.
        let cdp_logs = page.get_console_logs_tail(lines).await;
        if cdp_logs.as_array().map(|a| !a.is_empty()).unwrap_or(false) {
            return Ok(cdp_logs);
        }

        // 2) Fallback to JS-installed hook (ensures capture on pages where events are unavailable).
        let requested = lines.unwrap_or(0);
        let script = format!(
            r#"(function() {{
                try {{
                    if (!window.__code_console_logs) {{
                        window.__code_console_logs = [];
                        const push = (level, message) => {{
                            try {{
                                window.__code_console_logs.push({{ timestamp: new Date().toISOString(), level, message }});
                                if (window.__code_console_logs.length > 2000) window.__code_console_logs.shift();
                            }} catch (_) {{}}
                        }};

                        ['log','warn','error','info','debug'].forEach(function(method) {{
                            try {{
                                const orig = console[method];
                                console[method] = function() {{
                                    try {{
                                        var args = Array.prototype.slice.call(arguments);
                                        var msg = args.map(function(a) {{
                                            try {{ if (a && typeof a === 'object') return JSON.stringify(a); return String(a); }}
                                            catch (_) {{ return String(a); }}
                                        }}).join(' ');
                                        push(method, msg);
                                    }} catch(_) {{}}
                                    if (orig) return orig.apply(console, arguments);
                                }};
                            }} catch(_) {{}}
                        }});

                        window.addEventListener('error', function(e) {{
                            try {{
                                var msg = e && e.message ? e.message : 'Script error';
                                var stack = e && e.error && e.error.stack ? ('\n' + e.error.stack) : '';
                                push('exception', msg + stack);
                            }} catch(_) {{}}
                        }});
                        window.addEventListener('unhandledrejection', function(e) {{
                            try {{
                                var reason = e && e.reason;
                                if (reason && typeof reason === 'object') {{ try {{ reason = JSON.stringify(reason); }} catch(_) {{}} }}
                                push('unhandledrejection', String(reason));
                            }} catch(_) {{}}
                        }});
                    }}

                    var logs = window.__code_console_logs || [];
                    var n = {requested};
                    return (n && n > 0) ? logs.slice(-n) : logs;
                }} catch (err) {{
                    return [{{ timestamp: new Date().toISOString(), level: 'error', message: 'capture failed: ' + (err && err.message ? err.message : String(err)) }}];
                }}
            }})()"#
        );

        page.inject_js(&script).await
    }

    /// Execute an arbitrary CDP command against the active page session
    pub async fn execute_cdp(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value> {
        let page = self.get_or_create_page().await?;
        page.execute_cdp_raw(method, params).await
    }

    /// Execute an arbitrary CDP command at the browser (no session) scope
    pub async fn execute_cdp_browser(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value> {
        // Ensure a browser is connected
        self.ensure_browser().await?;
        let browser_guard = self.browser.lock().await;
        let browser = browser_guard
            .as_ref()
            .ok_or_else(|| BrowserError::CdpError("Browser not available".to_string()))?;

        // Local raw command type (serialize only params)
        #[derive(Debug, Clone)]
        struct RawCdpCommandBrowser {
            method: String,
            params: Value,
        }
        impl serde::Serialize for RawCdpCommandBrowser {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                self.params.serialize(serializer)
            }
        }
        impl chromiumoxide_types::Method for RawCdpCommandBrowser {
            fn identifier(&self) -> chromiumoxide_types::MethodId {
                self.method.clone().into()
            }
        }
        impl chromiumoxide_types::Command for RawCdpCommandBrowser {
            type Response = Value;
        }

        let cmd = RawCdpCommandBrowser {
            method: method.to_string(),
            params,
        };
        let resp = browser.execute(cmd).await?;
        Ok(resp.result)
    }

    /// Clean up injected artifacts and restore viewport/state where possible.
    /// This does not close the browser; it is safe to call when connected.
    pub async fn cleanup(&self) -> Result<()> {
        // Hide any overlay highlight
        let _ = self.execute_cdp("Overlay.hideHighlight", serde_json::json!({})).await;

        // Reset device metrics override (best-effort)
        let _ = self
            .execute_cdp("Emulation.clearDeviceMetricsOverride", serde_json::json!({}))
            .await;

        // Remove virtual cursor and related overlays if present
        let page = self.get_or_create_page().await?;
        let cleanup_js = r#"
            (function(){
                try { if (window.__vc && typeof window.__vc.destroy === 'function') window.__vc.destroy(); } catch(_) {}
                try { if (window.__code_console_logs) delete window.__code_console_logs; } catch(_) {}
                return true;
            })()
        "#;
        let _ = page.inject_js(cleanup_js).await;
        Ok(())
    }

    /// Get the current cursor position
    pub async fn get_cursor_position(&self) -> Result<(f64, f64)> {
        let page = self.get_or_create_page().await?;
        page.get_cursor_position().await
    }

    /// Get the current viewport dimensions
    pub async fn get_viewport_size(&self) -> (u32, u32) {
        let config = self.config.read().await;
        (config.viewport.width, config.viewport.height)
    }

    /// Set a callback to be called when navigation occurs
    pub async fn set_navigation_callback<F>(&self, callback: F)
    where
        F: Fn(String) + Send + Sync + 'static,
    {
        let mut callback_guard = self.navigation_callback.write().await;
        *callback_guard = Some(Box::new(callback));
    }

    /// Start monitoring for page navigation changes
    async fn start_navigation_monitor(&self, page: Arc<Page>) {
        // Stop any existing monitor
        self.stop_navigation_monitor().await;

        let navigation_callback = Arc::clone(&self.navigation_callback);
        let page_target_id = page.target_id_debug();
        let page_session_id = page.session_id_debug();
        let page_weak = Arc::downgrade(&page);

        let assets_arc = Arc::clone(&self.assets);
        let config_arc = Arc::clone(&self.config);
        let handle = tokio::spawn(async move {
            let mut last_url = String::new();
            let mut last_seq: u64 = 0;
            let mut _check_count = 0; // reserved for future periodic checks

            debug!(
                target_id = %page_target_id,
                session_id = %page_session_id,
                "Starting navigation monitor"
            );

            loop {
                // Check if page is still alive
                let page = match page_weak.upgrade() {
                    Some(p) => p,
                    None => {
                        debug!(
                            target_id = %page_target_id,
                            session_id = %page_session_id,
                            "Page dropped, stopping navigation monitor"
                        );
                        break;
                    }
                };

                // Get current URL
                if let Ok(current_url) = page.get_current_url().await {
                    // Check if URL changed (ignore about:blank)
                    if current_url != last_url && current_url != "about:blank" {
                        info!(
                            "Navigation detected: {} -> {}",
                            if last_url.is_empty() {
                                "initial"
                            } else {
                                &last_url
                            },
                            current_url
                        );
                        last_url = current_url.clone();

                        // Call the callback if set (immediate)
                        if let Some(ref callback) = *navigation_callback.read().await {
                            debug!("Triggering navigation callback for URL: {}", current_url);
                            callback(current_url.clone());
                        }

                        // Schedule a delayed callback for fully loaded page
                        let navigation_callback_delayed = Arc::clone(&navigation_callback);
                        let current_url_delayed = current_url.clone();
                        tokio::spawn(async move {
                            // Wait for page to fully load
                            tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

                            // Call the callback again with a marker that it's fully loaded
                            if let Some(ref callback) = *navigation_callback_delayed.read().await {
                                info!("Page fully loaded callback for: {}", current_url_delayed);
                                callback(current_url_delayed);
                            }
                        });
                    }
                }

                // Listen for SPA changes via codex:locationchange (only when attached to external Chrome)
                let cfg_now = config_arc.read().await.clone();
                if cfg_now.connect_port.is_some() || cfg_now.connect_ws.is_some() {
                    // Install listener once and poll sequence counter
                    let listener_script = r#"
                        (function(){
                          try {
                            if (!window.__code_nav_listening) {
                              window.__code_nav_listening = true;
                              window.__code_nav_seq = 0;
                              window.__code_nav_url = String(location.href || '');
                              window.addEventListener('codex:locationchange', function(){
                                window.__code_nav_seq += 1;
                                window.__code_nav_url = String(location.href || '');
                              }, { capture: true });
                            }
                            return { seq: Number(window.__code_nav_seq||0), url: String(window.__code_nav_url||location.href) };
                          } catch (e) { return { seq: 0, url: String(location.href||'') }; }
                        })()
                    "#;

                    if let Ok(result) = page.execute_javascript(listener_script).await {
                        let seq = result.get("seq").and_then(|v| v.as_u64()).unwrap_or(0);
                        let url = result
                            .get("url")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        if seq > last_seq {
                            info!("SPA locationchange detected: {} (seq {} -> {})", url, last_seq, seq);
                            last_seq = seq;

                            // Fire callback
                            if let Some(ref callback) = *navigation_callback.read().await {
                                callback(url.clone());
                            }

                            // Capture a screenshot asynchronously
                            let assets_arc2 = Arc::clone(&assets_arc);
                            let config_arc2 = Arc::clone(&config_arc);
                            let page_for_shot = Arc::clone(&page);
                            tokio::spawn(async move {
                                // Initialize assets manager if needed
                                if assets_arc2.lock().await.is_none() {
                                    if let Ok(am) = crate::assets::AssetManager::new().await {
                                        *assets_arc2.lock().await = Some(Arc::new(am));
                                    }
                                }
                                let assets_opt = assets_arc2.lock().await.clone();
                                drop(assets_arc2);
                                if let Some(assets) = assets_opt {
                                    let cfg = config_arc2.read().await.clone();
                                    let mode = if cfg.fullpage {
                                        crate::page::ScreenshotMode::FullPage { segments_max: Some(cfg.segments_max) }
                                    } else { crate::page::ScreenshotMode::Viewport };
                                    // small delay to allow SPA content to render
                                    tokio::time::sleep(Duration::from_millis(400)).await;
                                    if let Ok(shots) = page_for_shot.screenshot(mode).await {
                                        for s in shots {
                                            let _ = assets
                                                .store_screenshot(
                                                    &s.data,
                                                    s.format,
                                                    s.width,
                                                    s.height,
                                                    Self::SCREENSHOT_TTL_MS,
                                                )
                                                .await;
                                        }
                                    }
                                }
                            });
                        }
                    }
                }

                // periodic counter disabled; listener-based SPA detection in place

                // Check every 500ms
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        });

        let mut handle_guard = self.navigation_monitor_handle.lock().await;
        *handle_guard = Some(handle);
    }

    /// Stop navigation monitoring
    async fn stop_navigation_monitor(&self) {
        let mut handle_guard = self.navigation_monitor_handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }
    }

    /// Start a low-frequency viewport monitor that checks for drift without forcing resyncs.
    /// Applies the same logic to internal and external: only correct after two consecutive
    /// mismatches and at most once per minute to avoid jank. Logs when throttled.
    async fn start_viewport_monitor(&self, page: Arc<Page>) {
        // Stop any existing monitor first
        self.stop_viewport_monitor().await;

        let config_arc = Arc::clone(&self.config);
        let correction_enabled = Arc::clone(&self.auto_viewport_correction_enabled);
        let handle = tokio::spawn(async move {
            let mut consecutive_mismatch = 0u32;
            let mut last_warn: Option<std::time::Instant> = None;
            let mut last_correction: Option<std::time::Instant> = None;
            let check_interval = std::time::Duration::from_secs(60);
            let warn_interval = std::time::Duration::from_secs(300);
            let min_correction_interval = std::time::Duration::from_secs(60);

            loop {
                tokio::time::sleep(check_interval).await;

                // Snapshot expected config
                let cfg = config_arc.read().await.clone();
                let is_external = cfg.connect_port.is_some() || cfg.connect_ws.is_some();
                let expected_w = cfg.viewport.width as f64;
                let expected_h = cfg.viewport.height as f64;
                let expected_dpr = cfg.viewport.device_scale_factor as f64;

                // Probe current viewport via JS (cheap and non-invasive)
                let probe_js = r#"(() => ({
                    w: (document.documentElement.clientWidth|0),
                    h: (document.documentElement.clientHeight|0),
                    dpr: (window.devicePixelRatio||1)
                }))()"#;

                if let Ok(val) = page.inject_js(probe_js).await {
                    let cw = val.get("w").and_then(|v| v.as_u64()).unwrap_or(0) as f64;
                    let ch = val.get("h").and_then(|v| v.as_u64()).unwrap_or(0) as f64;
                    let cdpr = val.get("dpr").and_then(|v| v.as_f64()).unwrap_or(1.0);

                    let w_ok = (cw - expected_w).abs() <= 5.0;
                    let h_ok = (ch - expected_h).abs() <= 5.0;
                    let dpr_ok = (cdpr - expected_dpr).abs() <= 0.05;
                    let mismatch = !(w_ok && h_ok && dpr_ok);

                    if mismatch {
                        consecutive_mismatch += 1;
                        let now = std::time::Instant::now();
                        let can_correct = last_correction
                            .map(|t| now.duration_since(t) >= min_correction_interval)
                            .unwrap_or(true);

                        // Check gate: allow disabling auto-corrections at runtime
                        let enabled = *correction_enabled.read().await;
                        if consecutive_mismatch >= 2 && can_correct && enabled {
                            info!(
                                "Correcting viewport: {}x{}@{} -> {}x{}@{} (external={})",
                                cw, ch, cdpr, expected_w, expected_h, expected_dpr, is_external
                            );
                            let _ = page
                                .set_viewport(crate::page::SetViewportParams {
                                    width: cfg.viewport.width,
                                    height: cfg.viewport.height,
                                    device_scale_factor: Some(cfg.viewport.device_scale_factor),
                                    mobile: Some(cfg.viewport.mobile),
                                })
                                .await;
                            last_correction = Some(now);
                            consecutive_mismatch = 0;
                        } else {
                            // Throttled: log at most every 5 minutes
                            let should_warn = last_warn
                                .map(|t| now.duration_since(t) >= warn_interval)
                                .unwrap_or(true);
                            if should_warn {
                                warn!(
                                    "Viewport drift detected (throttled): {}x{}@{} vs expected {}x{}@{} (external={}, can_correct={})",
                                    cw, ch, cdpr, expected_w, expected_h, expected_dpr, is_external, can_correct
                                );
                                last_warn = Some(now);
                            }
                        }
                    } else {
                        consecutive_mismatch = 0;
                    }
                }
            }
        });

        *self.viewport_monitor_handle.lock().await = Some(handle);
    }

    async fn stop_viewport_monitor(&self) {
        let mut handle_guard = self.viewport_monitor_handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            handle.abort();
        }
    }

    /// Temporarily enable/disable automatic viewport correction (monitor-driven)
    pub async fn set_auto_viewport_correction(&self, enabled: bool) {
        let mut guard = self.auto_viewport_correction_enabled.write().await;
        *guard = enabled;
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

#[cfg(test)]
mod tests {
    use super::discover_ws_via_host_port;
    use super::should_restart_handler;
    use super::should_stop_handler;
    use std::io::Read;
    use std::io::Write;
    use std::net::TcpListener;
    use std::process::Command;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::thread;
    use std::time::Duration;
    use std::time::Instant;

    #[derive(Debug)]
    struct TestError(&'static str);

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }

    #[test]
    fn handler_restarts_after_repeated_errors() {
        assert!(!should_restart_handler(0));
        assert!(!should_restart_handler(1));
        assert!(!should_restart_handler(2));
        assert!(should_restart_handler(3));
    }

    #[test]
    fn handler_ignores_oneshot_cancellations() {
        let mut consecutive_errors = 0u32;
        for _ in 0..10 {
            let should_stop = should_stop_handler(
                "[test]",
                Err(TestError("oneshot canceled")),
                &mut consecutive_errors,
            );
            assert!(!should_stop);
            assert_eq!(consecutive_errors, 0);
        }
    }

    const TEST_PROXY_WS_URL: &str = "ws://proxy.invalid/devtools/browser/proxy";
    const TEST_TARGET_WS_URL: &str = "ws://target.invalid/devtools/browser/target";

    fn spawn_json_version_server(
        ws_url: &str,
    ) -> (u16, Arc<AtomicBool>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind server");
        listener
            .set_nonblocking(true)
            .expect("set non-blocking");
        let port = listener.local_addr().expect("server addr").port();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let ws_url = ws_url.to_string();

        let handle = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !stop_thread.load(Ordering::Relaxed) && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buffer = [0u8; 1024];
                        let _ = stream.read(&mut buffer);

                        let body = format!(r#"{{"webSocketDebuggerUrl":"{ws_url}"}}"#);
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        (port, stop, handle)
    }

    #[test]
    fn cdp_discovery_ignores_proxy_env_vars() {
        let (proxy_port, proxy_stop, proxy_handle) =
            spawn_json_version_server(TEST_PROXY_WS_URL);
        let (target_port, target_stop, target_handle) =
            spawn_json_version_server(TEST_TARGET_WS_URL);

        let exe = std::env::current_exe().expect("current exe");
        let proxy_url = format!("http://127.0.0.1:{proxy_port}");

        let output = Command::new(exe)
            .arg("--exact")
            .arg("manager::tests::cdp_discovery_ignores_proxy_env_vars_child")
            .arg("--ignored")
            .arg("--nocapture")
            .env("CODE_BROWSER_TEST_TARGET_PORT", target_port.to_string())
            .env("HTTP_PROXY", &proxy_url)
            .env("HTTPS_PROXY", &proxy_url)
            .env("ALL_PROXY", &proxy_url)
            .env("http_proxy", &proxy_url)
            .env("https_proxy", &proxy_url)
            .env("all_proxy", &proxy_url)
            .env("NO_PROXY", "")
            .env("no_proxy", "")
            .output()
            .expect("spawn child test");

        proxy_stop.store(true, Ordering::Relaxed);
        target_stop.store(true, Ordering::Relaxed);
        let _ = proxy_handle.join();
        let _ = target_handle.join();

        if !output.status.success() {
            panic!(
                "child test failed: status={:?}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }
    }

    #[ignore]
    #[tokio::test]
    async fn cdp_discovery_ignores_proxy_env_vars_child() {
        let target_port: u16 = std::env::var("CODE_BROWSER_TEST_TARGET_PORT")
            .expect("CODE_BROWSER_TEST_TARGET_PORT")
            .parse()
            .expect("valid port");

        let url = format!("http://127.0.0.1:{target_port}/json/version");
        let default_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .expect("build default client");
        let resp = default_client
            .get(&url)
            .send()
            .await
            .expect("default request");

        let proxy_version: super::JsonVersion =
            resp.json().await.expect("parse proxy json");
        assert_eq!(proxy_version.web_socket_debugger_url, TEST_PROXY_WS_URL);

        let discovered = discover_ws_via_host_port("127.0.0.1", target_port)
            .await
            .expect("discover ws url");
        assert_eq!(discovered, TEST_TARGET_WS_URL);
    }

}
