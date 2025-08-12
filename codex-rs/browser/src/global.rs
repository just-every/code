use crate::manager::BrowserManager;
use crate::config::BrowserConfig;
use std::sync::Arc;
use once_cell::sync::Lazy;
use tokio::sync::RwLock;

/// Global browser manager instance shared between TUI and Session
static GLOBAL_BROWSER_MANAGER: Lazy<Arc<RwLock<Option<Arc<BrowserManager>>>>> = 
    Lazy::new(|| Arc::new(RwLock::new(None)));

/// Get or create the global browser manager
pub async fn get_or_create_browser_manager() -> Arc<BrowserManager> {
    let mut guard = GLOBAL_BROWSER_MANAGER.write().await;
    if let Some(manager) = guard.as_ref() {
        manager.clone()
    } else {
        let mut config = BrowserConfig::default();
        config.enabled = true;
        let manager = Arc::new(BrowserManager::new(config));
        manager.set_enabled_sync(true);
        *guard = Some(manager.clone());
        manager
    }
}

/// Get the global browser manager if it exists
pub async fn get_browser_manager() -> Option<Arc<BrowserManager>> {
    GLOBAL_BROWSER_MANAGER.read().await.clone()
}

/// Clear the global browser manager
pub async fn clear_browser_manager() {
    *GLOBAL_BROWSER_MANAGER.write().await = None;
}

/// Set the global browser manager configuration (used by TUI to sync with global state)
pub async fn set_global_browser_manager(manager: Arc<BrowserManager>) {
    let mut guard = GLOBAL_BROWSER_MANAGER.write().await;
    *guard = Some(manager);
}