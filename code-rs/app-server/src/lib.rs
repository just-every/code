#![deny(clippy::print_stdout, clippy::print_stderr)]

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;

use code_common::CliConfigOverrides;
use code_core::config::Config;
use code_core::config::ConfigOverrides;
use mcp_types::JSONRPCMessage;
use mcp_types::RequestId;
use tokio::sync::mpsc;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::Duration;
use tokio::time::sleep;
use tracing::info;
use tracing::warn;
use tracing_subscriber::EnvFilter;
use serde_json::json;

use crate::message_processor::MessageProcessor;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::OutgoingEnvelope;
use crate::outgoing_message::OutgoingMessage;
use crate::outgoing_message::OutgoingMessageSender;
use crate::transport::CHANNEL_CAPACITY;
use crate::transport::ConnectionState;
use crate::transport::OutboundConnectionState;
use crate::transport::TransportEvent;
use crate::transport::route_outgoing_envelope;
use crate::transport::start_stdio_connection;
use crate::transport::start_websocket_acceptor;

pub mod code_message_processor;
mod error_code;
mod external_agent_config_api;
mod fuzzy_file_search;
mod message_processor;
pub mod outgoing_message;
mod transport;

pub use crate::transport::AppServerTransport;

const INTERNAL_REQUEST_ID_PREFIX: &str = "__code_internal_request__";

/// Control-plane messages from the processor side to the outbound router task.
enum OutboundControlEvent {
    Opened {
        connection_id: ConnectionId,
        writer: mpsc::Sender<OutgoingMessage>,
        initialized: Arc<AtomicBool>,
        opted_out_notification_methods: Arc<RwLock<HashSet<String>>>,
        disconnect_notify: Option<Arc<Notify>>,
    },
    Closed {
        connection_id: ConnectionId,
    },
}

#[derive(Clone, Debug)]
struct RequestRoute {
    connection_id: ConnectionId,
    original_request_id: RequestId,
}

pub async fn run_main(
    code_linux_sandbox_exe: Option<PathBuf>,
    cli_config_overrides: CliConfigOverrides,
) -> IoResult<()> {
    run_main_with_transport(
        code_linux_sandbox_exe,
        cli_config_overrides,
        AppServerTransport::Stdio,
    )
    .await
}

pub async fn run_main_with_transport(
    code_linux_sandbox_exe: Option<PathBuf>,
    cli_config_overrides: CliConfigOverrides,
    transport: AppServerTransport,
) -> IoResult<()> {
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    let (transport_event_tx, mut transport_event_rx) =
        mpsc::channel::<TransportEvent>(CHANNEL_CAPACITY);
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<OutgoingEnvelope>(CHANNEL_CAPACITY);
    let (outbound_control_tx, mut outbound_control_rx) =
        mpsc::channel::<OutboundControlEvent>(CHANNEL_CAPACITY);

    let mut stdio_handles = Vec::<JoinHandle<()>>::new();
    let mut websocket_accept_handle = None;
    match transport {
        AppServerTransport::Stdio => {
            start_stdio_connection(transport_event_tx.clone(), &mut stdio_handles).await?;
        }
        AppServerTransport::WebSocket { bind_address } => {
            websocket_accept_handle =
                Some(start_websocket_acceptor(bind_address, transport_event_tx.clone()).await?);
        }
    }
    let shutdown_when_no_connections = matches!(transport, AppServerTransport::Stdio);

    // Parse CLI overrides once and derive the base Config eagerly so later
    // components do not need to work with raw TOML values.
    let cli_kv_overrides = cli_config_overrides.parse_overrides().map_err(|e| {
        std::io::Error::new(
            ErrorKind::InvalidInput,
            format!("error parsing -c overrides: {e}"),
        )
    })?;
    let mut config_overrides = ConfigOverrides::default();
    config_overrides.code_linux_sandbox_exe = code_linux_sandbox_exe.clone();
    let mut config_warnings = Vec::<serde_json::Value>::new();

    let config = match Config::load_with_cli_overrides(cli_kv_overrides.clone(), config_overrides.clone()) {
        Ok(config) => config,
        Err(err) => {
            config_warnings.push(json!({
                "summary": "Invalid configuration; using defaults.",
                "details": err.to_string(),
                "path": serde_json::Value::Null,
                "range": serde_json::Value::Null,
            }));
            Config::load_default_with_cli_overrides(cli_kv_overrides.clone(), config_overrides)
            .map_err(|fallback_err| {
                std::io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "error loading default config after config error: {fallback_err}"
                    ),
                )
            })?
        }
    };

    let request_routes = Arc::new(tokio::sync::Mutex::new(HashMap::<RequestId, RequestRoute>::new()));
    let request_routes_for_outbound = Arc::clone(&request_routes);
    let transport_event_tx_for_outbound = transport_event_tx.clone();
    let outbound_handle = tokio::spawn(async move {
        let mut outbound_connections = HashMap::<ConnectionId, OutboundConnectionState>::new();
        let mut pending_closed_connections = VecDeque::<ConnectionId>::new();
        loop {
            tokio::select! {
                biased;
                event = outbound_control_rx.recv() => {
                    let Some(event) = event else {
                        break;
                    };
                    match event {
                        OutboundControlEvent::Opened {
                            connection_id,
                            writer,
                            initialized,
                            opted_out_notification_methods,
                            disconnect_notify,
                        } => {
                            outbound_connections.insert(
                                connection_id,
                                OutboundConnectionState::new(
                                    writer,
                                    initialized,
                                    opted_out_notification_methods,
                                    disconnect_notify,
                                ),
                            );
                        }
                        OutboundControlEvent::Closed { connection_id } => {
                            outbound_connections.remove(&connection_id);
                        }
                    }
                }
                envelope = outgoing_rx.recv() => {
                    let Some(envelope) = envelope else {
                        break;
                    };
                    let Some(envelope) =
                        rewrite_response_routing(envelope, &request_routes_for_outbound).await
                    else {
                        continue;
                    };
                    let disconnected_connections =
                        route_outgoing_envelope(&mut outbound_connections, envelope).await;
                    pending_closed_connections.extend(disconnected_connections);
                }
            }

            while let Some(connection_id) = pending_closed_connections.front().copied() {
                match transport_event_tx_for_outbound
                    .try_send(TransportEvent::ConnectionClosed { connection_id })
                {
                    Ok(()) => {
                        pending_closed_connections.pop_front();
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        break;
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        return;
                    }
                }
            }
        }
        info!("outbound router task exited (channel closed)");
    });

    let processor_handle = tokio::spawn({
        let outgoing_message_sender =
            Arc::new(OutgoingMessageSender::new_with_routed_sender(outgoing_tx));
        let outbound_control_tx = outbound_control_tx;
        let request_routes = Arc::clone(&request_routes);
        let mut processor = MessageProcessor::new(
            Arc::clone(&outgoing_message_sender),
            code_linux_sandbox_exe,
            Arc::new(config),
            config_warnings,
            cli_kv_overrides,
        );
        let mut connections = HashMap::<ConnectionId, ConnectionState>::new();
        let mut next_internal_request_ordinal = 0_u64;
        async move {
            loop {
                let Some(event) = transport_event_rx.recv().await else {
                    break;
                };
                match event {
                    TransportEvent::ConnectionOpened {
                        connection_id,
                        writer,
                        disconnect_notify,
                    } => {
                        let outbound_initialized = Arc::new(AtomicBool::new(false));
                        let outbound_opted_out_notification_methods =
                            Arc::new(RwLock::new(HashSet::new()));
                        if outbound_control_tx
                            .send(OutboundControlEvent::Opened {
                                connection_id,
                                writer,
                                initialized: Arc::clone(&outbound_initialized),
                                opted_out_notification_methods: Arc::clone(
                                    &outbound_opted_out_notification_methods,
                                ),
                                disconnect_notify,
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                        connections.insert(
                            connection_id,
                            ConnectionState::new(
                                outbound_initialized,
                                outbound_opted_out_notification_methods,
                            ),
                        );
                    }
                    TransportEvent::ConnectionClosed { connection_id } => {
                        if shutdown_when_no_connections {
                            // Stdio clients can close stdin after sending requests while still
                            // expecting pending responses on stdout.
                            outgoing_message_sender
                                .clear_callbacks_for_connection(connection_id)
                                .await;
                            processor.on_connection_closed(connection_id).await;
                            wait_for_request_routes_for_connection(
                                &request_routes,
                                connection_id,
                            )
                            .await;
                        }

                        if outbound_control_tx
                            .send(OutboundControlEvent::Closed { connection_id })
                            .await
                            .is_err()
                        {
                            break;
                        }
                        connections.remove(&connection_id);
                        remove_request_routes_for_connection(&request_routes, connection_id).await;
                        if !shutdown_when_no_connections {
                            outgoing_message_sender
                                .clear_callbacks_for_connection(connection_id)
                                .await;
                            processor.on_connection_closed(connection_id).await;
                        }

                        if shutdown_when_no_connections && connections.is_empty() {
                            break;
                        }
                    }
                    TransportEvent::IncomingMessage {
                        connection_id,
                        message,
                    } => match message {
                        JSONRPCMessage::Request(mut request) => {
                            let Some(connection_state) = connections.get_mut(&connection_id) else {
                                warn!("dropping request from unknown connection: {:?}", connection_id);
                                continue;
                            };

                            let original_request_id = request.id.clone();
                            let internal_request_id = RequestId::String(format!(
                                "{INTERNAL_REQUEST_ID_PREFIX}{}:{next_internal_request_ordinal}",
                                connection_id.0
                            ));
                            next_internal_request_ordinal += 1;
                            request.id = internal_request_id.clone();
                            {
                                let mut request_routes = request_routes.lock().await;
                                request_routes.insert(
                                    internal_request_id,
                                    RequestRoute {
                                        connection_id,
                                        original_request_id,
                                    },
                                );
                            }

                            let was_initialized = connection_state.session.initialized;
                            processor
                                .process_request(
                                    connection_id,
                                    request,
                                    &mut connection_state.session,
                                    &connection_state.outbound_initialized,
                                    &connection_state.outbound_opted_out_notification_methods,
                                )
                                .await;
                            if !was_initialized && connection_state.session.initialized {
                                processor.send_initialize_notifications(connection_id).await;
                            }
                        }
                        JSONRPCMessage::Response(response) => {
                            processor.process_response(connection_id, response).await;
                        }
                        JSONRPCMessage::Notification(notification) => {
                            processor.process_notification(notification).await;
                        }
                        JSONRPCMessage::Error(err) => {
                            processor.process_error(connection_id, err).await;
                        }
                    },
                }
            }

            info!("processor task exited (channel closed)");
        }
    });

    drop(transport_event_tx);

    let _ = processor_handle.await;
    let _ = outbound_handle.await;

    if let Some(handle) = websocket_accept_handle {
        handle.abort();
    }

    for handle in stdio_handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn rewrite_response_routing(
    envelope: OutgoingEnvelope,
    request_routes: &Arc<tokio::sync::Mutex<HashMap<RequestId, RequestRoute>>>,
) -> Option<OutgoingEnvelope> {
    match envelope {
        OutgoingEnvelope::Broadcast {
            message: OutgoingMessage::Response(mut response),
        } => {
            let route = {
                let mut request_routes = request_routes.lock().await;
                request_routes.remove(&response.id)
            };
            if let Some(route) = route {
                response.id = route.original_request_id;
                return Some(OutgoingEnvelope::ToConnection {
                    connection_id: route.connection_id,
                    message: OutgoingMessage::Response(response),
                });
            }

            if is_internal_request_id(&response.id) {
                warn!(
                    "dropping response for disconnected request route: {:?}",
                    response.id
                );
                return None;
            }

            Some(OutgoingEnvelope::Broadcast {
                message: OutgoingMessage::Response(response),
            })
        }
        OutgoingEnvelope::Broadcast {
            message: OutgoingMessage::Error(mut outgoing_error),
        } => {
            let route = {
                let mut request_routes = request_routes.lock().await;
                request_routes.remove(&outgoing_error.id)
            };
            if let Some(route) = route {
                outgoing_error.id = route.original_request_id;
                return Some(OutgoingEnvelope::ToConnection {
                    connection_id: route.connection_id,
                    message: OutgoingMessage::Error(outgoing_error),
                });
            }

            if is_internal_request_id(&outgoing_error.id) {
                warn!(
                    "dropping error for disconnected request route: {:?}",
                    outgoing_error.id
                );
                return None;
            }

            Some(OutgoingEnvelope::Broadcast {
                message: OutgoingMessage::Error(outgoing_error),
            })
        }
        _ => Some(envelope),
    }
}

fn is_internal_request_id(request_id: &RequestId) -> bool {
    matches!(request_id, RequestId::String(value) if value.starts_with(INTERNAL_REQUEST_ID_PREFIX))
}

async fn remove_request_routes_for_connection(
    request_routes: &Arc<tokio::sync::Mutex<HashMap<RequestId, RequestRoute>>>,
    connection_id: ConnectionId,
) {
    let mut request_routes = request_routes.lock().await;
    request_routes.retain(|_, route| route.connection_id != connection_id);
}

async fn wait_for_request_routes_for_connection(
    request_routes: &Arc<tokio::sync::Mutex<HashMap<RequestId, RequestRoute>>>,
    connection_id: ConnectionId,
) {
    loop {
        let has_pending_requests = {
            let request_routes = request_routes.lock().await;
            request_routes
                .values()
                .any(|route| route.connection_id == connection_id)
        };

        if !has_pending_requests {
            return;
        }

        sleep(Duration::from_millis(10)).await;
    }
}
