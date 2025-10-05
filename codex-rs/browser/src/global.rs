use crate::config::BrowserConfig;
use crate::manager::BrowserManager;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Global browser manager instance shared between TUI and Session
static GLOBAL_BROWSER_MANAGER: Lazy<Arc<RwLock<Option<Arc<BrowserManager>>>>> =
    Lazy::new(|| Arc::new(RwLock::new(None)));

/// Cache of the last successful external Chrome connection (port/ws)
static LAST_CONNECTION: Lazy<Arc<RwLock<(Option<u16>, Option<String>)>>> =
    Lazy::new(|| Arc::new(RwLock::new((None, None))));

/// Get or create the global browser manager
pub async fn get_or_create_browser_manager() -> Arc<BrowserManager> {
    // Fast path: try read lock to avoid contending on writer when already initialized
    if let Some(existing) = GLOBAL_BROWSER_MANAGER.read().await.as_ref().cloned() {
        return existing;
    }

    // Slow path: acquire write lock and initialize if still empty
    let mut w = GLOBAL_BROWSER_MANAGER.write().await;
    if let Some(existing) = w.as_ref() {
        return existing.clone();
    }
    let config = BrowserConfig::default();
    let manager = Arc::new(BrowserManager::new(config));
    *w = Some(manager.clone());
    manager
}

/// Get the global browser manager if it exists
pub async fn get_browser_manager() -> Option<Arc<BrowserManager>> {
    GLOBAL_BROWSER_MANAGER.read().await.as_ref().cloned()
}

/// Clear the global browser manager
pub async fn clear_browser_manager() {
    *GLOBAL_BROWSER_MANAGER.write().await = None;
}

/// Set the global browser manager configuration (used by TUI to sync with global state)
pub async fn set_global_browser_manager(manager: Arc<BrowserManager>) {
    let mut guard = GLOBAL_BROWSER_MANAGER.write().await;
    *guard = Some(manager);
    tracing::info!("Global browser manager set");
}

/// Get the last known external Chrome connection (port, ws)
pub async fn get_last_connection() -> (Option<u16>, Option<String>) {
    let (port, ws) = LAST_CONNECTION.read().await.clone();
    (port, ws)
}

/// Update the last known external Chrome connection (port, ws)
pub async fn set_last_connection(port: Option<u16>, ws: Option<String>) {
    let mut guard = LAST_CONNECTION.write().await;
    // Clone ws for logging to avoid use after move
    let ws_for_log = ws.clone();
    *guard = (port, ws);
    tracing::debug!(
        "Updated last Chrome connection cache: port={:?}, ws={:?}",
        port,
        ws_for_log
    );
}
