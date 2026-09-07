//! Forwards auth invalidations without credentials. The managed client owns the watcher.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use codex_login::AuthChangeState;
use codex_rmcp_client::RmcpClient;
use rmcp::model::ServerCapabilities;
use serde_json::json;
use tokio::sync::watch;
use tokio_util::task::AbortOnDropHandle;

pub(crate) const CAPABILITY: &str = "codex/auth-change";
const NOTIFICATION: &str = "notifications/codex/authChanged";
const SEND_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) async fn start(
    client: Arc<RmcpClient>,
    capabilities: &ServerCapabilities,
    changes: Option<watch::Receiver<AuthChangeState>>,
) -> Result<Option<Arc<AbortOnDropHandle<()>>>> {
    let Some(mut changes) = changes.filter(|_| {
        capabilities
            .experimental
            .as_ref()
            .is_some_and(|capabilities| capabilities.contains_key(CAPABILITY))
    }) else {
        return Ok(None);
    };

    notify(&client, &mut changes).await?;
    let task = tokio::spawn(async move {
        while changes.changed().await.is_ok() {
            if notify(&client, &mut changes).await.is_err() {
                tracing::warn!("MCP auth invalidation delivery failed; closing connection");
                client.shutdown().await;
                break;
            }
        }
    });
    Ok(Some(Arc::new(AbortOnDropHandle::new(task))))
}

async fn notify(client: &RmcpClient, changes: &mut watch::Receiver<AuthChangeState>) -> Result<()> {
    let state = *changes.borrow_and_update();
    tokio::time::timeout(
        SEND_TIMEOUT,
        client.send_custom_notification(
            NOTIFICATION,
            Some(json!({
                "generation": state.generation,
                "ownerGeneration": state.owner_generation,
            })),
        ),
    )
    .await?
}

#[cfg(test)]
#[path = "auth_changes_tests.rs"]
mod tests;
