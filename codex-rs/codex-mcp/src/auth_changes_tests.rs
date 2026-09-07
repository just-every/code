use super::*;
use codex_rmcp_client::InProcessTransportFactory;
use futures::FutureExt;
use futures::future::BoxFuture;
use pretty_assertions::assert_eq;
use rmcp::ServiceExt;
use rmcp::model::ClientCapabilities;
use rmcp::model::CustomNotification;
use rmcp::model::Implementation;
use rmcp::model::InitializeRequestParams;
use tokio::sync::mpsc;
use tokio::time::timeout;

#[derive(Clone)]
struct NotificationServer(mpsc::Sender<serde_json::Value>);

impl rmcp::ServerHandler for NotificationServer {
    async fn on_custom_notification(
        &self,
        notification: CustomNotification,
        _context: rmcp::service::NotificationContext<rmcp::RoleServer>,
    ) {
        self.0.send(json!(notification)).await.unwrap();
    }
}

impl InProcessTransportFactory for NotificationServer {
    fn open(&self) -> BoxFuture<'static, std::io::Result<tokio::io::DuplexStream>> {
        let server = self.clone();
        async move {
            let (client, transport) = tokio::io::duplex(/*max_buf_size*/ 4096);
            tokio::spawn(async move {
                let service = server.serve(transport).await.unwrap();
                service.waiting().await.unwrap();
            });
            Ok(client)
        }
        .boxed()
    }
}

#[tokio::test]
async fn auth_notifications_require_opt_in_and_follow_client_lifetime() -> Result<()> {
    let (notifications, mut received) = mpsc::channel(/*buffer*/ 8);
    let client = Arc::new(
        RmcpClient::new_in_process_client(Arc::new(NotificationServer(notifications))).await?,
    );
    client
        .initialize(
            InitializeRequestParams::new(
                ClientCapabilities::default(),
                Implementation::new("test", "1"),
            ),
            Some(SEND_TIMEOUT),
            Box::new(|_, _| async { anyhow::bail!("unexpected elicitation") }.boxed()),
        )
        .await?;
    let (changes, receiver) = watch::channel(AuthChangeState::default());
    let mut capabilities = ServerCapabilities::default();
    assert!(
        start(Arc::clone(&client), &capabilities, Some(receiver.clone()))
            .await?
            .is_none()
    );
    capabilities.experimental = Some([(CAPABILITY.to_string(), Default::default())].into());
    assert!(
        start(Arc::clone(&client), &capabilities, /*changes*/ None)
            .await?
            .is_none()
    );
    assert_eq!(received.try_recv(), Err(mpsc::error::TryRecvError::Empty));
    let watcher = start(Arc::clone(&client), &capabilities, Some(receiver))
        .await?
        .unwrap();
    assert_eq!(
        timeout(SEND_TIMEOUT, received.recv()).await?,
        Some(
            json!({"method": NOTIFICATION, "params": {"_meta": {}, "generation": 0, "ownerGeneration": 0}})
        ),
    );
    changes.send_modify(|state| state.generation += 1);
    assert_eq!(
        timeout(SEND_TIMEOUT, received.recv()).await?,
        Some(
            json!({"method": NOTIFICATION, "params": {"_meta": {}, "generation": 1, "ownerGeneration": 0}})
        ),
    );
    for _ in 0..2 {
        changes.send_modify(|state| {
            state.generation += 1;
            state.owner_generation += 1;
        });
    }
    assert_eq!(
        timeout(SEND_TIMEOUT, received.recv()).await?,
        Some(
            json!({"method": NOTIFICATION, "params": {"_meta": {}, "generation": 3, "ownerGeneration": 2}})
        ),
    );
    drop(watcher);
    timeout(SEND_TIMEOUT, changes.closed()).await?;
    client.shutdown().await;
    Ok(())
}
