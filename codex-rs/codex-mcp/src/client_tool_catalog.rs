//! Tool catalog state and synchronization owned by one live MCP client.
//!
//! Reusing the client preserves this state; a replacement client starts a new
//! catalog. Snapshot reads and revision-checked calls keep the locks private;
//! successful refreshes publish after calls using the current catalog finish.
//! Refresh results retain raw tools and eligibility from the same published runtime.
//! Optional live updates are adopted before reading or starting another call.

use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;

use anyhow::Result;
use codex_connectors::ConnectorRuntimeSnapshot;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::sync::RwLockReadGuard;
use tokio::sync::watch;

use crate::tools::ToolInfo;

type ToolCatalogUpdates = watch::Receiver<Option<Arc<ConnectorRuntimeSnapshot<ToolInfo>>>>;

/// The exact Apps catalog returned by an awaited refresh of one published runtime.
pub struct CodexAppsToolSnapshot {
    /// Raw installed tools, including tools hidden or disabled for the model.
    pub tools: Vec<ToolInfo>,
    /// Raw MCP tool names allowed by the same runtime's generic MCP policy.
    /// App-specific policy is applied by the caller.
    pub model_visible_tool_names: HashSet<String>,
}

pub(crate) struct ClientToolCatalog {
    current: RwLock<ToolCatalogSnapshot>,
    /// Serialize fetches without blocking calls against the current catalog.
    refresh_lock: Mutex<()>,
}

pub(crate) struct ToolCatalogSnapshot {
    /// Advances on explicit refresh or adoption of changed live tools.
    pub(crate) revision: u64,
    pub(crate) tools: Vec<ToolInfo>,
    updates: Option<ToolCatalogUpdates>,
}

impl ClientToolCatalog {
    pub(crate) fn new(tools: Vec<ToolInfo>, mut updates: Option<ToolCatalogUpdates>) -> Self {
        let tools = updates
            .as_mut()
            .and_then(|updates| {
                updates
                    .borrow_and_update()
                    .as_ref()
                    .map(|snapshot| snapshot.tools().to_vec())
            })
            .unwrap_or(tools);
        Self {
            current: RwLock::new(ToolCatalogSnapshot {
                revision: 0,
                tools,
                updates,
            }),
            refresh_lock: Mutex::new(()),
        }
    }

    pub(crate) async fn read<R>(&self, read: impl FnOnce(&ToolCatalogSnapshot) -> R) -> R {
        let current = self.read_current().await;
        read(&current)
    }

    async fn read_current(&self) -> RwLockReadGuard<'_, ToolCatalogSnapshot> {
        loop {
            {
                let current = self.current.read().await;
                if !current
                    .updates
                    .as_ref()
                    .is_some_and(|updates| updates.has_changed().unwrap_or(false))
                {
                    return current;
                }
            }
            let mut current = self.current.write().await;
            if let Some(updates) = current.updates.as_mut()
                && updates.has_changed().unwrap_or(false)
            {
                let snapshot = updates.borrow_and_update().clone();
                if let Some(snapshot) = snapshot
                    && current.tools != snapshot.tools()
                {
                    current.tools = snapshot.tools().to_vec();
                    current.revision += 1;
                }
            }
        }
    }

    /// Serialize fetching and publication, leaving the current catalog usable during the fetch.
    /// The publication callback runs alongside the exact-client update under the write lock.
    #[expect(
        clippy::await_holding_invalid_type,
        reason = "refreshes must remain serialized through fetching and catalog publication"
    )]
    pub(crate) async fn refresh<C, R, F, Fut, P>(&self, fetch: F, publish: P) -> Result<R>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(Vec<ToolInfo>, C)>>,
        P: FnOnce(&[ToolInfo], C) -> R,
    {
        let _refresh = self.refresh_lock.lock().await;
        let (tools, context) = fetch().await?;
        let mut current = self.current.write().await;
        let result = publish(&tools, context);
        current.tools = current
            .updates
            .as_mut()
            .and_then(|updates| {
                updates
                    .borrow_and_update()
                    .as_ref()
                    .map(|snapshot| snapshot.tools().to_vec())
            })
            .unwrap_or(tools);
        current.revision += 1;
        Ok(result)
    }

    /// Reject stale calls before preparation and hold catalog authority until execution finishes.
    #[expect(
        clippy::await_holding_invalid_type,
        reason = "catalog publication must wait for call preparation and execution"
    )]
    pub(crate) async fn run_with_revision<R, F, Fut>(
        &self,
        expected_revision: u64,
        run: F,
    ) -> Option<R>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = R>,
    {
        let current = self.read_current().await;
        if current.revision != expected_revision {
            return None;
        }
        let result = run().await;
        drop(current);
        Some(result)
    }
}

/// A binding cache key includes client identity, since new clients start at zero.
#[derive(Clone)]
pub(crate) struct ClientToolCatalogRevision {
    pub(crate) catalog: Arc<ClientToolCatalog>,
    pub(crate) revision: u64,
}

impl PartialEq for ClientToolCatalogRevision {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.catalog, &other.catalog) && self.revision == other.revision
    }
}

impl Eq for ClientToolCatalogRevision {}
