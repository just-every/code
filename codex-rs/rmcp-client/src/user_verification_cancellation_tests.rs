//! Ensures MCP service cancellation releases the pending verification route and tool timeout pause.

use super::ElicitationClientService;
use super::ElicitationPauseState;
use super::ElicitationResponse;
use codex_protocol::mcp::OPENAI_ELICITATION_EXTENSION_ID;
use rmcp::RoleServer;
use rmcp::model::CancelledNotification;
use rmcp::model::CancelledNotificationParam;
use rmcp::model::ClientInfo;
use rmcp::model::ClientJsonRpcMessage;
use rmcp::model::CustomRequest;
use rmcp::model::ElicitationAction;
use rmcp::model::RequestId;
use rmcp::model::ServerJsonRpcMessage;
use rmcp::model::ServerNotification;
use rmcp::model::ServerRequest;
use rmcp::service::serve_directly;
use rmcp::transport::IntoTransport;
use rmcp::transport::Transport;
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::timeout;

#[tokio::test]
async fn user_verification_service_cancellation_drops_pending_response() -> anyhow::Result<()> {
    let pause_state = ElicitationPauseState::new();
    let mut paused = pause_state.subscribe();
    let (route_tx, mut route_rx) = mpsc::unbounded_channel();
    let mut client_info = ClientInfo::default();
    client_info.capabilities.extensions = Some(
        [(
            OPENAI_ELICITATION_EXTENSION_ID.to_string(),
            serde_json::Map::from_iter([("userVerification".to_string(), json!({}))]),
        )]
        .into_iter()
        .collect(),
    );
    let service = ElicitationClientService::new(
        client_info,
        Box::new(move |_, _| {
            let (response_tx, response_rx) = oneshot::channel();
            route_tx
                .send(response_tx)
                .expect("observe pending verification");
            Box::pin(async move { Ok(response_rx.await?) })
        }),
        pause_state,
    );
    let (client_transport, server_transport) = tokio::io::duplex(/*max_buf_size*/ 4096);
    let client = serve_directly(service, client_transport, /*peer_info*/ None);
    let mut server = IntoTransport::<RoleServer, _, _>::into_transport(server_transport);
    server
        .send(ServerJsonRpcMessage::request(
            ServerRequest::CustomRequest(CustomRequest::new(
                "openai/elicitation/create",
                Some(json!({
                    "mode": "openai/userVerification",
                    "title": "Approve",
                    "description": "",
                    "challenge": "AQID",
                })),
            )),
            RequestId::Number(1),
        ))
        .await?;
    let mut response_tx = timeout(Duration::from_secs(/*secs*/ 5), route_rx.recv())
        .await?
        .expect("verification was routed to the UI");
    assert!(*paused.borrow());

    client.cancel().await?;

    timeout(Duration::from_secs(/*secs*/ 5), response_tx.closed()).await?;
    timeout(
        Duration::from_secs(/*secs*/ 5),
        paused.wait_for(|paused| !*paused),
    )
    .await??;
    Ok(())
}

#[tokio::test]
async fn cancelling_one_verification_leaves_the_mcp_connection_and_other_requests_alive()
-> anyhow::Result<()> {
    let pause_state = ElicitationPauseState::new();
    let mut paused = pause_state.subscribe();
    let (route_tx, mut route_rx) = mpsc::unbounded_channel();
    let mut client_info = ClientInfo::default();
    client_info.capabilities.extensions = Some(
        [(
            OPENAI_ELICITATION_EXTENSION_ID.to_string(),
            serde_json::Map::from_iter([("userVerification".to_string(), json!({}))]),
        )]
        .into_iter()
        .collect(),
    );
    let service = ElicitationClientService::new(
        client_info,
        Box::new(move |id, _| {
            let (response_tx, response_rx) = oneshot::channel();
            route_tx
                .send((id, response_tx))
                .expect("observe verification");
            Box::pin(async move { Ok(response_rx.await?) })
        }),
        pause_state,
    );
    let (client_transport, server_transport) = tokio::io::duplex(/*max_buf_size*/ 4096);
    let client = serve_directly(service, client_transport, /*peer_info*/ None);
    let mut server = IntoTransport::<RoleServer, _, _>::into_transport(server_transport);

    for id in [1, 2] {
        server
            .send(ServerJsonRpcMessage::request(
                ServerRequest::CustomRequest(CustomRequest::new(
                    "openai/elicitation/create",
                    Some(json!({
                        "mode": "openai/userVerification",
                        "title": "Approve",
                        "description": "",
                        "challenge": "AQID",
                    })),
                )),
                RequestId::Number(id),
            ))
            .await?;
    }
    let (first_id, mut first) = timeout(Duration::from_secs(/*secs*/ 5), route_rx.recv())
        .await?
        .expect("first verification reached UI");
    let (second_id, second) = timeout(Duration::from_secs(/*secs*/ 5), route_rx.recv())
        .await?
        .expect("second verification reached UI");
    assert_ne!(first_id, second_id);
    assert!(*paused.borrow());

    server
        .send(ServerJsonRpcMessage::notification(
            ServerNotification::CancelledNotification(CancelledNotification::new(
                CancelledNotificationParam::new(Some(first_id.clone()), None),
            )),
        ))
        .await?;
    timeout(Duration::from_secs(/*secs*/ 5), first.closed()).await?;
    assert!(!second.is_closed());
    assert!(*paused.borrow());
    let cancel = timeout(Duration::from_secs(/*secs*/ 5), server.receive())
        .await?
        .expect("cancelled verification still sends a response");
    let ClientJsonRpcMessage::Response(cancel) = cancel else {
        anyhow::bail!("expected a cancellation response");
    };
    assert_eq!(cancel.id, first_id);
    assert_eq!(serde_json::to_value(cancel.result)?["action"], "cancel");

    for request_id in [first_id, RequestId::Number(999)] {
        server
            .send(ServerJsonRpcMessage::notification(
                ServerNotification::CancelledNotification(CancelledNotification::new(
                    CancelledNotificationParam::new(Some(request_id), None),
                )),
            ))
            .await?;
    }
    assert!(!second.is_closed());

    second
        .send(ElicitationResponse {
            action: ElicitationAction::Accept,
            content: Some(json!({"credentialId": "AQID", "signature": "BAUG"})),
            meta: None,
        })
        .expect("second verification remains pending");
    let accepted = timeout(Duration::from_secs(/*secs*/ 5), server.receive())
        .await?
        .expect("second request survived cancellation");
    let ClientJsonRpcMessage::Response(accepted) = accepted else {
        anyhow::bail!("expected the second verification response");
    };
    assert_eq!(accepted.id, second_id);
    assert_eq!(serde_json::to_value(accepted.result)?["action"], "accept");
    timeout(
        Duration::from_secs(/*secs*/ 5),
        paused.wait_for(|paused| !*paused),
    )
    .await??;
    client.cancel().await?;
    Ok(())
}
