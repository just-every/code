use crate::error_code::OVERLOADED_ERROR_CODE;
use crate::message_processor::ConnectionSessionState;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::OutgoingEnvelope;
use crate::outgoing_message::OutgoingError;
use crate::outgoing_message::OutgoingMessage;
use mcp_types::JSONRPCErrorError;
use mcp_types::JSONRPCMessage;
use futures::SinkExt;
use futures::StreamExt;
use owo_colors::OwoColorize;
use owo_colors::Stream;
use owo_colors::Style;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::io::{self};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message as WebSocketMessage;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

/// Size of the bounded channels used to communicate between tasks.
pub(crate) const CHANNEL_CAPACITY: usize = 128;

fn colorize(text: &str, style: Style) -> String {
    text.if_supports_color(Stream::Stderr, |value| value.style(style))
        .to_string()
}

#[allow(clippy::print_stderr)]
fn print_websocket_startup_banner(addr: SocketAddr) {
    let title = colorize("code app-server (WebSockets)", Style::new().bold().cyan());
    let listening_label = colorize("listening on:", Style::new().dimmed());
    let listen_url = colorize(&format!("ws://{addr}"), Style::new().green());
    let note_label = colorize("note:", Style::new().dimmed());
    eprintln!("{title}");
    eprintln!("  {listening_label} {listen_url}");
    if addr.ip().is_loopback() {
        eprintln!(
            "  {note_label} binds localhost only (use SSH port-forwarding for remote access)"
        );
    } else {
        eprintln!(
            "  {note_label} this is a raw WS server; consider running behind TLS/auth for real remote use"
        );
    }
}

#[allow(clippy::print_stderr)]
fn print_websocket_connection(peer_addr: SocketAddr) {
    let connected_label = colorize("websocket client connected from", Style::new().dimmed());
    eprintln!("{connected_label} {peer_addr}");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppServerTransport {
    Stdio,
    WebSocket { bind_address: SocketAddr },
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum AppServerTransportParseError {
    UnsupportedListenUrl(String),
    InvalidWebSocketListenUrl(String),
}

impl std::fmt::Display for AppServerTransportParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppServerTransportParseError::UnsupportedListenUrl(listen_url) => write!(
                f,
                "unsupported --listen URL `{listen_url}`; expected `stdio://` or `ws://IP:PORT`"
            ),
            AppServerTransportParseError::InvalidWebSocketListenUrl(listen_url) => write!(
                f,
                "invalid websocket --listen URL `{listen_url}`; expected `ws://IP:PORT`"
            ),
        }
    }
}

impl std::error::Error for AppServerTransportParseError {}

impl AppServerTransport {
    pub const DEFAULT_LISTEN_URL: &'static str = "stdio://";

    pub fn from_listen_url(listen_url: &str) -> Result<Self, AppServerTransportParseError> {
        if listen_url == Self::DEFAULT_LISTEN_URL {
            return Ok(Self::Stdio);
        }

        if let Some(socket_addr) = listen_url.strip_prefix("ws://") {
            let bind_address = socket_addr.parse::<SocketAddr>().map_err(|_| {
                AppServerTransportParseError::InvalidWebSocketListenUrl(listen_url.to_string())
            })?;
            return Ok(Self::WebSocket { bind_address });
        }

        Err(AppServerTransportParseError::UnsupportedListenUrl(
            listen_url.to_string(),
        ))
    }
}

impl FromStr for AppServerTransport {
    type Err = AppServerTransportParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_listen_url(s)
    }
}

#[derive(Debug)]
pub(crate) enum TransportEvent {
    ConnectionOpened {
        connection_id: ConnectionId,
        writer: mpsc::Sender<OutgoingMessage>,
        disconnect_notify: Option<Arc<Notify>>,
    },
    ConnectionClosed {
        connection_id: ConnectionId,
    },
    IncomingMessage {
        connection_id: ConnectionId,
        message: JSONRPCMessage,
    },
}

pub(crate) struct ConnectionState {
    pub(crate) outbound_initialized: Arc<AtomicBool>,
    pub(crate) outbound_opted_out_notification_methods: Arc<RwLock<HashSet<String>>>,
    pub(crate) session: ConnectionSessionState,
}

impl ConnectionState {
    pub(crate) fn new(
        outbound_initialized: Arc<AtomicBool>,
        outbound_opted_out_notification_methods: Arc<RwLock<HashSet<String>>>,
    ) -> Self {
        Self {
            outbound_initialized,
            outbound_opted_out_notification_methods,
            session: ConnectionSessionState::default(),
        }
    }
}

pub(crate) struct OutboundConnectionState {
    pub(crate) initialized: Arc<AtomicBool>,
    pub(crate) opted_out_notification_methods: Arc<RwLock<HashSet<String>>>,
    pub(crate) writer: mpsc::Sender<OutgoingMessage>,
    pub(crate) disconnect_notify: Option<Arc<Notify>>,
}

impl OutboundConnectionState {
    pub(crate) fn new(
        writer: mpsc::Sender<OutgoingMessage>,
        initialized: Arc<AtomicBool>,
        opted_out_notification_methods: Arc<RwLock<HashSet<String>>>,
        disconnect_notify: Option<Arc<Notify>>,
    ) -> Self {
        Self {
            initialized,
            opted_out_notification_methods,
            writer,
            disconnect_notify,
        }
    }
}

pub(crate) async fn start_stdio_connection(
    transport_event_tx: mpsc::Sender<TransportEvent>,
    stdio_handles: &mut Vec<JoinHandle<()>>,
) -> IoResult<()> {
    let connection_id = ConnectionId(0);
    let (writer_tx, mut writer_rx) = mpsc::channel::<OutgoingMessage>(CHANNEL_CAPACITY);
    let writer_tx_for_reader = writer_tx.clone();
    transport_event_tx
        .send(TransportEvent::ConnectionOpened {
            connection_id,
            writer: writer_tx,
            disconnect_notify: None,
        })
        .await
        .map_err(|_| std::io::Error::new(ErrorKind::BrokenPipe, "processor unavailable"))?;

    let transport_event_tx_for_reader = transport_event_tx.clone();
    stdio_handles.push(tokio::spawn(async move {
        let stdin = io::stdin();
        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if !forward_incoming_message(
                        &transport_event_tx_for_reader,
                        &writer_tx_for_reader,
                        connection_id,
                        &line,
                    )
                    .await
                    {
                        break;
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    error!("Failed reading stdin: {err}");
                    break;
                }
            }
        }

        let _ = transport_event_tx_for_reader
            .send(TransportEvent::ConnectionClosed { connection_id })
            .await;
        debug!("stdin reader finished (EOF)");
    }));

    stdio_handles.push(tokio::spawn(async move {
        let mut stdout = io::stdout();
        while let Some(outgoing_message) = writer_rx.recv().await {
            let Some(mut json) = serialize_outgoing_message(outgoing_message) else {
                continue;
            };
            json.push('\n');
            if let Err(err) = stdout.write_all(json.as_bytes()).await {
                error!("Failed to write to stdout: {err}");
                break;
            }
        }
        info!("stdout writer exited (channel closed)");
    }));

    Ok(())
}

pub(crate) async fn start_websocket_acceptor(
    bind_address: SocketAddr,
    transport_event_tx: mpsc::Sender<TransportEvent>,
) -> IoResult<JoinHandle<()>> {
    let listener = TcpListener::bind(bind_address).await?;
    let local_addr = listener.local_addr()?;
    print_websocket_startup_banner(local_addr);
    info!("app-server websocket listening on ws://{local_addr}");

    let connection_counter = Arc::new(AtomicU64::new(1));
    Ok(tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    print_websocket_connection(peer_addr);
                    let connection_id =
                        ConnectionId(connection_counter.fetch_add(1, Ordering::Relaxed));
                    let transport_event_tx_for_connection = transport_event_tx.clone();
                    tokio::spawn(async move {
                        run_websocket_connection(
                            connection_id,
                            stream,
                            transport_event_tx_for_connection,
                        )
                        .await;
                    });
                }
                Err(err) => {
                    error!("failed to accept websocket connection: {err}");
                }
            }
        }
    }))
}

async fn run_websocket_connection(
    connection_id: ConnectionId,
    stream: TcpStream,
    transport_event_tx: mpsc::Sender<TransportEvent>,
) {
    let websocket_stream = match accept_async(stream).await {
        Ok(stream) => stream,
        Err(err) => {
            warn!("failed to complete websocket handshake: {err}");
            return;
        }
    };

    let (writer_tx, mut writer_rx) = mpsc::channel::<OutgoingMessage>(CHANNEL_CAPACITY);
    let writer_tx_for_reader = writer_tx.clone();
    let disconnect_notify = Arc::new(Notify::new());
    if transport_event_tx
        .send(TransportEvent::ConnectionOpened {
            connection_id,
            writer: writer_tx,
            disconnect_notify: Some(Arc::clone(&disconnect_notify)),
        })
        .await
        .is_err()
    {
        return;
    }

    let (mut websocket_writer, mut websocket_reader) = websocket_stream.split();
    loop {
        tokio::select! {
            _ = disconnect_notify.notified() => {
                break;
            }
            outgoing_message = writer_rx.recv() => {
                let Some(outgoing_message) = outgoing_message else {
                    break;
                };
                let Some(json) = serialize_outgoing_message(outgoing_message) else {
                    continue;
                };
                let send = websocket_writer.send(WebSocketMessage::Text(json.into()));
                tokio::pin!(send);
                let send_result = tokio::select! {
                    result = &mut send => Some(result),
                    _ = disconnect_notify.notified() => None,
                };

                if !matches!(send_result, Some(Ok(()))) {
                    break;
                }
            }
            incoming_message = websocket_reader.next() => {
                match incoming_message {
                    Some(Ok(WebSocketMessage::Text(text))) => {
                        if !forward_incoming_message(
                            &transport_event_tx,
                            &writer_tx_for_reader,
                            connection_id,
                            &text,
                        )
                        .await
                        {
                            break;
                        }
                    }
                    Some(Ok(WebSocketMessage::Ping(payload))) => {
                        let send_pong = websocket_writer.send(WebSocketMessage::Pong(payload));
                        tokio::pin!(send_pong);
                        let send_pong_result = tokio::select! {
                            result = &mut send_pong => Some(result),
                            _ = disconnect_notify.notified() => None,
                        };

                        if !matches!(send_pong_result, Some(Ok(()))) {
                            break;
                        }
                    }
                    Some(Ok(WebSocketMessage::Pong(_))) => {}
                    Some(Ok(WebSocketMessage::Close(_))) | None => break,
                    Some(Ok(WebSocketMessage::Binary(_))) => {
                        warn!("dropping unsupported binary websocket message");
                    }
                    Some(Ok(WebSocketMessage::Frame(_))) => {}
                    Some(Err(err)) => {
                        warn!("websocket receive error: {err}");
                        break;
                    }
                }
            }
        }
    }

    let _ = transport_event_tx
        .send(TransportEvent::ConnectionClosed { connection_id })
        .await;
}

async fn forward_incoming_message(
    transport_event_tx: &mpsc::Sender<TransportEvent>,
    writer: &mpsc::Sender<OutgoingMessage>,
    connection_id: ConnectionId,
    payload: &str,
) -> bool {
    match serde_json::from_str::<JSONRPCMessage>(payload) {
        Ok(message) => {
            enqueue_incoming_message(transport_event_tx, writer, connection_id, message).await
        }
        Err(err) => {
            error!("Failed to deserialize JSONRPCMessage: {err}");
            true
        }
    }
}

async fn enqueue_incoming_message(
    transport_event_tx: &mpsc::Sender<TransportEvent>,
    writer: &mpsc::Sender<OutgoingMessage>,
    connection_id: ConnectionId,
    message: JSONRPCMessage,
) -> bool {
    let event = TransportEvent::IncomingMessage {
        connection_id,
        message,
    };
    match transport_event_tx.try_send(event) {
        Ok(()) => true,
        Err(mpsc::error::TrySendError::Closed(_)) => false,
        Err(mpsc::error::TrySendError::Full(TransportEvent::IncomingMessage {
            connection_id,
            message: JSONRPCMessage::Request(request),
        })) => {
            let overload_error = OutgoingMessage::Error(OutgoingError {
                id: request.id,
                error: JSONRPCErrorError {
                    code: OVERLOADED_ERROR_CODE,
                    message: "Server overloaded; retry later.".to_string(),
                    data: None,
                },
            });
            match writer.try_send(overload_error) {
                Ok(()) => true,
                Err(mpsc::error::TrySendError::Closed(_)) => false,
                Err(mpsc::error::TrySendError::Full(_overload_error)) => {
                    warn!(
                        "dropping overload response for connection {:?}: outbound queue is full",
                        connection_id
                    );
                    true
                }
            }
        }
        Err(mpsc::error::TrySendError::Full(event)) => transport_event_tx.send(event).await.is_ok(),
    }
}

fn serialize_outgoing_message(outgoing_message: OutgoingMessage) -> Option<String> {
    let jsonrpc: JSONRPCMessage = outgoing_message.into();
    match serde_json::to_string(&jsonrpc) {
        Ok(json) => Some(json),
        Err(err) => {
            error!("Failed to serialize JSONRPCMessage: {err}");
            None
        }
    }
}

fn should_skip_notification_for_connection(
    connection_state: &OutboundConnectionState,
    message: &OutgoingMessage,
) -> bool {
    let Ok(opted_out_notification_methods) = connection_state.opted_out_notification_methods.read()
    else {
        warn!("failed to read outbound opted-out notifications");
        return false;
    };
    match message {
        OutgoingMessage::Notification(notification) => {
            opted_out_notification_methods.contains(notification.method.as_str())
        }
        _ => false,
    }
}

pub(crate) async fn route_outgoing_envelope(
    connections: &mut HashMap<ConnectionId, OutboundConnectionState>,
    envelope: OutgoingEnvelope,
) -> Vec<ConnectionId> {
    let mut disconnected = Vec::new();
    match envelope {
        OutgoingEnvelope::ToConnection {
            connection_id,
            message,
        } => {
            let Some(connection_state) = connections.get(&connection_id) else {
                warn!(
                    "dropping message for disconnected connection: {:?}",
                    connection_id
                );
                return disconnected;
            };
            if should_skip_notification_for_connection(connection_state, &message) {
                return disconnected;
            }
            if is_connection_write_failed(connection_id, connection_state, message).await {
                connections.remove(&connection_id);
                disconnected.push(connection_id);
            }
        }
        OutgoingEnvelope::Broadcast { message } => {
            let target_connections: Vec<ConnectionId> = connections
                .iter()
                .filter_map(|(connection_id, connection_state)| {
                    if connection_state.initialized.load(Ordering::Acquire)
                        && !should_skip_notification_for_connection(connection_state, &message)
                    {
                        Some(*connection_id)
                    } else {
                        None
                    }
                })
                .collect();

            for connection_id in target_connections {
                let Some(connection_state) = connections.get(&connection_id) else {
                    continue;
                };
                if is_connection_write_failed(connection_id, connection_state, message.clone()).await {
                    connections.remove(&connection_id);
                    disconnected.push(connection_id);
                }
            }
        }
    }
    disconnected
}

async fn is_connection_write_failed(
    connection_id: ConnectionId,
    connection_state: &OutboundConnectionState,
    message: OutgoingMessage,
) -> bool {
    match connection_state.writer.try_send(message) {
        Ok(()) => false,
        Err(mpsc::error::TrySendError::Closed(_)) => true,
        Err(mpsc::error::TrySendError::Full(message)) => {
            if let Some(disconnect_notify) = &connection_state.disconnect_notify {
                // For websocket clients, prevent a single slow peer from stalling all outbound traffic.
                warn!(
                    "disconnecting slow connection {:?}: outbound queue is full",
                    connection_id
                );
                disconnect_notify.notify_one();
                true
            } else {
                // Preserve stdio behavior: apply backpressure rather than disconnecting.
                connection_state.writer.send(message).await.is_err()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::time::sleep;
    use tokio::time::Duration;
    use tokio::time::timeout;

    #[test]
    fn app_server_transport_parses_stdio_listen_url() {
        let transport = AppServerTransport::from_listen_url(AppServerTransport::DEFAULT_LISTEN_URL)
            .expect("stdio listen URL should parse");
        assert_eq!(transport, AppServerTransport::Stdio);
    }

    #[test]
    fn app_server_transport_parses_websocket_listen_url() {
        let transport = AppServerTransport::from_listen_url("ws://127.0.0.1:1234")
            .expect("websocket listen URL should parse");
        assert_eq!(
            transport,
            AppServerTransport::WebSocket {
                bind_address: "127.0.0.1:1234".parse().expect("valid socket address"),
            }
        );
    }

    #[test]
    fn app_server_transport_rejects_invalid_websocket_listen_url() {
        let err = AppServerTransport::from_listen_url("ws://localhost:1234")
            .expect_err("hostname bind address should be rejected");
        assert_eq!(
            err.to_string(),
            "invalid websocket --listen URL `ws://localhost:1234`; expected `ws://IP:PORT`"
        );
    }

    #[test]
    fn app_server_transport_rejects_unsupported_listen_url() {
        let err = AppServerTransport::from_listen_url("http://127.0.0.1:1234")
            .expect_err("unsupported scheme should fail");
        assert_eq!(
            err.to_string(),
            "unsupported --listen URL `http://127.0.0.1:1234`; expected `stdio://` or `ws://IP:PORT`"
        );
    }

    #[tokio::test]
    async fn enqueue_incoming_request_returns_overload_error_when_queue_is_full() {
        let connection_id = ConnectionId(42);
        let (transport_event_tx, mut transport_event_rx) = mpsc::channel(1);
        let (writer_tx, mut writer_rx) = mpsc::channel(1);

        let first_message = JSONRPCMessage::Notification(mcp_types::JSONRPCNotification {
            jsonrpc: mcp_types::JSONRPC_VERSION.to_string(),
            method: "initialized".to_string(),
            params: None,
        });
        transport_event_tx
            .send(TransportEvent::IncomingMessage {
                connection_id,
                message: first_message.clone(),
            })
            .await
            .expect("queue should accept first message");

        let request = JSONRPCMessage::Request(mcp_types::JSONRPCRequest {
            jsonrpc: mcp_types::JSONRPC_VERSION.to_string(),
            id: mcp_types::RequestId::Integer(7),
            method: "config/read".to_string(),
            params: Some(json!({ "includeLayers": false })),
        });
        assert!(
            enqueue_incoming_message(&transport_event_tx, &writer_tx, connection_id, request).await
        );

        let queued_event = transport_event_rx
            .recv()
            .await
            .expect("first event should stay queued");
        match queued_event {
            TransportEvent::IncomingMessage {
                connection_id: queued_connection_id,
                message,
            } => {
                assert_eq!(queued_connection_id, connection_id);
                assert_eq!(message, first_message);
            }
            _ => panic!("expected queued incoming message"),
        }

        let overload = writer_rx
            .recv()
            .await
            .expect("request should receive overload error");
        let overload_json =
            serde_json::to_value::<JSONRPCMessage>(overload.into()).expect("serialize overload error");
        assert_eq!(
            overload_json,
            json!({
                "jsonrpc": mcp_types::JSONRPC_VERSION,
                "id": 7,
                "error": {
                    "code": OVERLOADED_ERROR_CODE,
                    "message": "Server overloaded; retry later."
                }
            })
        );
    }

    #[tokio::test]
    async fn enqueue_incoming_response_waits_instead_of_dropping_when_queue_is_full() {
        let connection_id = ConnectionId(42);
        let (transport_event_tx, mut transport_event_rx) = mpsc::channel(1);
        let (writer_tx, _writer_rx) = mpsc::channel(1);

        let first_message = JSONRPCMessage::Notification(mcp_types::JSONRPCNotification {
            jsonrpc: mcp_types::JSONRPC_VERSION.to_string(),
            method: "initialized".to_string(),
            params: None,
        });
        transport_event_tx
            .send(TransportEvent::IncomingMessage {
                connection_id,
                message: first_message.clone(),
            })
            .await
            .expect("queue should accept first message");

        let response = JSONRPCMessage::Response(mcp_types::JSONRPCResponse {
            jsonrpc: mcp_types::JSONRPC_VERSION.to_string(),
            id: mcp_types::RequestId::Integer(7),
            result: json!({"ok": true}),
        });
        let transport_event_tx_for_enqueue = transport_event_tx.clone();
        let writer_tx_for_enqueue = writer_tx.clone();
        let enqueue_handle = tokio::spawn(async move {
            enqueue_incoming_message(
                &transport_event_tx_for_enqueue,
                &writer_tx_for_enqueue,
                connection_id,
                response,
            )
            .await
        });

        let queued_event = transport_event_rx
            .recv()
            .await
            .expect("first event should be dequeued");
        match queued_event {
            TransportEvent::IncomingMessage {
                connection_id: queued_connection_id,
                message,
            } => {
                assert_eq!(queued_connection_id, connection_id);
                assert_eq!(message, first_message);
            }
            _ => panic!("expected queued incoming message"),
        }

        let enqueue_result = enqueue_handle.await.expect("enqueue task should not panic");
        assert!(enqueue_result);

        let forwarded_event = transport_event_rx
            .recv()
            .await
            .expect("response should be forwarded instead of dropped");
        match forwarded_event {
            TransportEvent::IncomingMessage {
                connection_id: queued_connection_id,
                message:
                    JSONRPCMessage::Response(mcp_types::JSONRPCResponse { id, result, .. }),
            } => {
                assert_eq!(queued_connection_id, connection_id);
                assert_eq!(id, mcp_types::RequestId::Integer(7));
                assert_eq!(result, json!({"ok": true}));
            }
            _ => panic!("expected forwarded response message"),
        }
    }

    #[tokio::test]
    async fn enqueue_incoming_request_does_not_block_when_writer_queue_is_full() {
        let connection_id = ConnectionId(42);
        let (transport_event_tx, _transport_event_rx) = mpsc::channel(1);
        let (writer_tx, mut writer_rx) = mpsc::channel(1);

        transport_event_tx
            .send(TransportEvent::IncomingMessage {
                connection_id,
                message: JSONRPCMessage::Notification(mcp_types::JSONRPCNotification {
                    jsonrpc: mcp_types::JSONRPC_VERSION.to_string(),
                    method: "initialized".to_string(),
                    params: None,
                }),
            })
            .await
            .expect("transport queue should accept first message");

        writer_tx
            .send(OutgoingMessage::Notification(
                crate::outgoing_message::OutgoingNotification {
                    method: "queued".to_string(),
                    params: None,
                },
            ))
            .await
            .expect("writer queue should accept first message");

        let request = JSONRPCMessage::Request(mcp_types::JSONRPCRequest {
            jsonrpc: mcp_types::JSONRPC_VERSION.to_string(),
            id: mcp_types::RequestId::Integer(7),
            method: "config/read".to_string(),
            params: Some(json!({ "includeLayers": false })),
        });

        let enqueue_result = timeout(
            Duration::from_millis(100),
            enqueue_incoming_message(&transport_event_tx, &writer_tx, connection_id, request),
        )
        .await
        .expect("enqueue should not block while writer queue is full");
        assert!(enqueue_result);

        let queued_outgoing = writer_rx
            .recv()
            .await
            .expect("writer queue should still contain original message");
        let queued_json =
            serde_json::to_value::<JSONRPCMessage>(queued_outgoing.into()).expect("serialize queued message");
        assert_eq!(queued_json, json!({ "jsonrpc": "2.0", "method": "queued" }));
    }

    #[tokio::test]
    async fn routed_notification_respects_opt_out_on_target_connection() {
        let connection_id = ConnectionId(7);
        let (writer_tx, mut writer_rx) = mpsc::channel(1);
        let mut connections = HashMap::new();
        let initialized = Arc::new(AtomicBool::new(true));
        let opted_out = Arc::new(RwLock::new(HashSet::from(["configWarning".to_string()])));
        connections.insert(
            connection_id,
            OutboundConnectionState::new(writer_tx, initialized, opted_out, None),
        );

        let envelope = OutgoingEnvelope::ToConnection {
            connection_id,
            message: OutgoingMessage::Notification(crate::outgoing_message::OutgoingNotification {
                method: "configWarning".to_string(),
                params: Some(json!({ "summary": "warning" })),
            }),
        };

        let disconnected = route_outgoing_envelope(&mut connections, envelope).await;
        assert!(disconnected.is_empty(), "connection should remain active");

        assert!(
            timeout(Duration::from_millis(25), writer_rx.recv())
                .await
                .is_err(),
            "notification should be suppressed by opt-out"
        );
    }

    #[tokio::test]
    async fn slow_connection_does_not_block_other_clients() {
        let (slow_writer_tx, mut slow_writer_rx) = mpsc::channel::<OutgoingMessage>(1);
        let (fast_writer_tx, mut fast_writer_rx) = mpsc::channel::<OutgoingMessage>(1);
        let slow_disconnect_notify = Arc::new(Notify::new());

        // Fill the slow client's queue first so later writes would block if awaited.
        slow_writer_tx
            .try_send(OutgoingMessage::Notification(
                crate::outgoing_message::OutgoingNotification {
                    method: "prefill".to_string(),
                    params: None,
                },
            ))
            .expect("slow queue should accept prefill");

        let mut connections = HashMap::new();
        connections.insert(
            ConnectionId(1),
            OutboundConnectionState::new(
                slow_writer_tx,
                Arc::new(AtomicBool::new(true)),
                Arc::new(RwLock::new(HashSet::new())),
                Some(slow_disconnect_notify),
            ),
        );
        connections.insert(
            ConnectionId(2),
            OutboundConnectionState::new(
                fast_writer_tx,
                Arc::new(AtomicBool::new(true)),
                Arc::new(RwLock::new(HashSet::new())),
                None,
            ),
        );

        let envelope = OutgoingEnvelope::Broadcast {
            message: OutgoingMessage::Notification(crate::outgoing_message::OutgoingNotification {
                method: "codex/event/item_started".to_string(),
                params: Some(json!({ "ok": true })),
            }),
        };

        let disconnected = timeout(
            Duration::from_millis(50),
            route_outgoing_envelope(&mut connections, envelope),
        )
        .await
        .expect("routing should finish promptly even when one client is slow");

        assert_eq!(disconnected, vec![ConnectionId(1)]);
        assert!(connections.contains_key(&ConnectionId(2)));

        let fast_outgoing = fast_writer_rx
            .recv()
            .await
            .expect("fast connection should still receive the broadcast");
        let OutgoingMessage::Notification(notification) = fast_outgoing else {
            panic!("expected broadcast notification for fast connection");
        };
        assert_eq!(notification.method, "codex/event/item_started");

        let slow_prefill = slow_writer_rx
            .recv()
            .await
            .expect("slow queue should only contain prefilled message");
        let OutgoingMessage::Notification(notification) = slow_prefill else {
            panic!("expected prefilled notification");
        };
        assert_eq!(notification.method, "prefill");
    }

    #[tokio::test]
    async fn slow_connection_notifies_disconnect_signal() {
        let (slow_writer_tx, _slow_writer_rx) = mpsc::channel::<OutgoingMessage>(1);
        slow_writer_tx
            .try_send(OutgoingMessage::Notification(
                crate::outgoing_message::OutgoingNotification {
                    method: "prefill".to_string(),
                    params: None,
                },
            ))
            .expect("slow queue should accept prefill");

        let disconnect_notify = Arc::new(Notify::new());
        let mut connections = HashMap::new();
        connections.insert(
            ConnectionId(9),
            OutboundConnectionState::new(
                slow_writer_tx,
                Arc::new(AtomicBool::new(true)),
                Arc::new(RwLock::new(HashSet::new())),
                Some(Arc::clone(&disconnect_notify)),
            ),
        );

        let envelope = OutgoingEnvelope::ToConnection {
            connection_id: ConnectionId(9),
            message: OutgoingMessage::Notification(crate::outgoing_message::OutgoingNotification {
                method: "codex/event/item_started".to_string(),
                params: Some(json!({ "ok": true })),
            }),
        };

        let notified = disconnect_notify.notified();
        let disconnected = route_outgoing_envelope(&mut connections, envelope).await;
        assert_eq!(disconnected, vec![ConnectionId(9)]);

        timeout(Duration::from_millis(50), notified)
            .await
            .expect("disconnect notification should fire");
    }

    #[tokio::test]
    async fn stdio_full_queue_backpressures_instead_of_disconnect() {
        let (writer_tx, mut writer_rx) = mpsc::channel::<OutgoingMessage>(1);
        writer_tx
            .try_send(OutgoingMessage::Notification(
                crate::outgoing_message::OutgoingNotification {
                    method: "prefill".to_string(),
                    params: None,
                },
            ))
            .expect("queue should accept prefill");

        let mut connections = HashMap::new();
        connections.insert(
            ConnectionId(11),
            OutboundConnectionState::new(
                writer_tx,
                Arc::new(AtomicBool::new(true)),
                Arc::new(RwLock::new(HashSet::new())),
                None,
            ),
        );

        let route_task = tokio::spawn(async move {
            route_outgoing_envelope(
                &mut connections,
                OutgoingEnvelope::ToConnection {
                    connection_id: ConnectionId(11),
                    message: OutgoingMessage::Notification(
                        crate::outgoing_message::OutgoingNotification {
                            method: "second".to_string(),
                            params: None,
                        },
                    ),
                },
            )
            .await
        });

        sleep(Duration::from_millis(20)).await;
        assert!(
            !route_task.is_finished(),
            "stdio routing should backpressure while queue is full"
        );

        let prefill = writer_rx.recv().await.expect("prefill should be queued");
        let OutgoingMessage::Notification(prefill_notification) = prefill else {
            panic!("expected prefill notification");
        };
        assert_eq!(prefill_notification.method, "prefill");

        let disconnected = route_task.await.expect("route task should complete");
        assert!(
            disconnected.is_empty(),
            "stdio connection should not be disconnected on backpressure"
        );

        let second = writer_rx.recv().await.expect("second notification should enqueue");
        let OutgoingMessage::Notification(second_notification) = second else {
            panic!("expected second notification");
        };
        assert_eq!(second_notification.method, "second");
    }
}
