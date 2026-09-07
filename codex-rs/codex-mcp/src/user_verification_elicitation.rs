//! Routes device verification directly to the app, outside automated approval policy.

use super::*;

pub(super) async fn route(
    router: ElicitationRequestRouter,
    events: Option<Sender<Event>>,
    authority: Arc<StdMutex<Option<ElicitationAuthority>>>,
    server_name: String,
    request: ElicitationRequest,
) -> Result<ElicitationResponse> {
    let authority = authority
        .lock()
        .ok()
        .and_then(|authority| authority.clone());
    let plugin_service = authority.as_ref().is_some_and(|authority| {
        authority
            .config
            .mcp_server_catalog
            .server(&server_name)
            .is_some_and(|server| {
                server
                    .source()
                    .is_host_owned_apps(&server_name, server.config())
            })
    });
    let Some(events) = events.filter(|_| plugin_service && !router.auto_deny()) else {
        return Ok(ElicitationResponse {
            action: ElicitationAction::Cancel,
            content: None,
            meta: None,
        });
    };
    let id = format!(
        "codex-mcp-elicitation-{}",
        NEXT_ELICITATION_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
    );
    let key = (server_name.clone(), RequestId::String(id.clone().into()));
    let (response, receiver) = oneshot::channel();
    router
        .requests
        .lock()
        .map_err(|_| anyhow!("elicitation request router unavailable"))?
        .insert(key.clone(), response);
    let _pending = PendingElicitationRequest { router, key };
    let _active = authority
        .as_ref()
        .and_then(|authority| authority.lifecycle.as_ref())
        .map(ElicitationLifecycle::start);
    events
        .send(Event {
            id: "mcp_elicitation_request".to_string(),
            msg: EventMsg::ElicitationRequest(ElicitationRequestEvent {
                turn_id: None,
                server_name,
                id: ProtocolRequestId::String(id),
                request,
            }),
        })
        .await
        .context("failed to deliver user-verification request")?;
    receiver
        .await
        .context("user-verification response channel closed")
}
