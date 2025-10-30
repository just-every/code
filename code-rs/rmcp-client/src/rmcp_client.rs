use std::collections::HashMap;
use std::error::Error as StdError;
use std::ffi::OsString;
use std::fmt;
use std::future::Future;
use std::io;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use anyhow::anyhow;
use futures::FutureExt;
use mcp_types::CallToolRequestParams;
use mcp_types::CallToolResult;
use mcp_types::InitializeRequestParams;
use mcp_types::InitializeResult;
use mcp_types::ListToolsRequestParams;
use mcp_types::ListToolsResult;
use mcp_types::MCP_SCHEMA_VERSION;
use rmcp::model::CallToolRequestParam;
use rmcp::model::InitializeRequestParam;
use rmcp::model::PaginatedRequestParam;
use rmcp::service::RoleClient;
use rmcp::service::RunningService;
use rmcp::service::{self};
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::child_process::TokioChildProcess;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use reqwest::Error as ReqwestError;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time;
use tracing::info;
use tracing::warn;

use crate::logging_client_handler::LoggingClientHandler;
use crate::utils::convert_call_tool_result;
use crate::utils::convert_to_mcp;
use crate::utils::convert_to_rmcp;
use crate::utils::create_env_for_mcp_server;
use crate::utils::run_with_timeout;

enum PendingTransport {
    ChildProcess(TokioChildProcess),
    StreamableHttp {
        transport: StreamableHttpClientTransport<reqwest::Client>,
        url: String,
        bearer_token: Option<String>,
    },
}

enum ClientState {
    Connecting {
        transport: Option<PendingTransport>,
    },
    Ready {
        service: Arc<RunningService<RoleClient, LoggingClientHandler>>,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Phase {
    Initialize,
    ListTools,
    CallTool,
}

impl Phase {
    fn as_str(self) -> &'static str {
        match self {
            Phase::Initialize => "initialize",
            Phase::ListTools => "list_tools",
            Phase::CallTool => "call_tool",
        }
    }
}

/// MCP client implemented on top of the official `rmcp` SDK.
/// https://github.com/modelcontextprotocol/rust-sdk
pub struct RmcpClient {
    state: Mutex<ClientState>,
}

impl RmcpClient {
    pub async fn new_stdio_client(
        program: OsString,
        args: Vec<OsString>,
        env: Option<HashMap<String, String>>,
    ) -> io::Result<Self> {
        let program_name = program.to_string_lossy().into_owned();
        let mut command = Command::new(&program);
        command
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .env_clear()
            .envs(create_env_for_mcp_server(env))
            .args(&args);

        let (transport, stderr) = TokioChildProcess::builder(command)
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                loop {
                    match reader.next_line().await {
                        Ok(Some(line)) => {
                            info!("MCP server stderr ({program_name}): {line}");
                        }
                        Ok(None) => break,
                        Err(error) => {
                            warn!("Failed to read MCP server stderr ({program_name}): {error}");
                            break;
                        }
                    }
                }
            });
        }

        Ok(Self {
            state: Mutex::new(ClientState::Connecting {
                transport: Some(PendingTransport::ChildProcess(transport)),
            }),
        })
    }

    pub fn new_streamable_http_client(url: String, bearer_token: Option<String>) -> Result<Self> {
        let transport = build_streamable_http_transport(&url, bearer_token.as_deref());

        Ok(Self {
            state: Mutex::new(ClientState::Connecting {
                transport: Some(PendingTransport::StreamableHttp {
                    transport,
                    url,
                    bearer_token,
                }),
            }),
        })
    }

    /// Perform the initialization handshake with the MCP server.
    /// https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle#initialization
    pub async fn initialize(
        &self,
        params: InitializeRequestParams,
        timeout: Option<Duration>,
    ) -> Result<InitializeResult> {
        let pending_transport = {
            let mut guard = self.state.lock().await;
            match &mut *guard {
                ClientState::Connecting { transport } => transport
                    .take()
                    .ok_or_else(|| anyhow!("client already initializing"))?,
                ClientState::Ready { .. } => return Err(anyhow!("client already initialized")),
            }
        };

        let service = match pending_transport {
            PendingTransport::ChildProcess(transport) => {
                let client_info = convert_to_rmcp::<_, InitializeRequestParam>(params.clone())?;
                let client_handler = LoggingClientHandler::new(client_info);
                let service_future = service::serve_client(client_handler.clone(), transport).boxed();
                await_handshake(service_future, timeout)
                    .await
                    .map_err(|err| annotate_phase_error(Phase::Initialize, err))?
            }
            PendingTransport::StreamableHttp {
                mut transport,
                url,
                bearer_token,
            } => {
                let mut attempt = 0;
                loop {
                    let client_info = convert_to_rmcp::<_, InitializeRequestParam>(params.clone())?;
                    let client_handler = LoggingClientHandler::new(client_info);
                    let service_future = service::serve_client(client_handler.clone(), transport).boxed();
                    match await_handshake(service_future, timeout).await {
                        Ok(service) => break service,
                        Err(err) => {
                            let err = annotate_phase_error(Phase::Initialize, err);
                            if should_retry_initialize(&err, attempt) {
                                attempt += 1;
                                time::sleep(Duration::from_millis(250)).await;
                                transport = build_streamable_http_transport(&url, bearer_token.as_deref());
                                continue;
                            }
                            return Err(err);
                        }
                    }
                }
            }
        };

        let initialize_result_rmcp = service
            .peer()
            .peer_info()
            .ok_or_else(|| annotate_phase_error(Phase::Initialize, anyhow!("handshake succeeded but server info was missing")))?;
        let initialize_result: InitializeResult = convert_to_mcp(initialize_result_rmcp)?;

        if initialize_result.protocol_version != MCP_SCHEMA_VERSION {
            let reported_version = initialize_result.protocol_version.clone();
            return Err(annotate_phase_error(
                Phase::Initialize,
                anyhow!(
                    "MCP server reported protocol version {reported_version}, but this client expects {}. Update either side so both speak the same schema.",
                    MCP_SCHEMA_VERSION
                ),
            ));
        }

        {
            let mut guard = self.state.lock().await;
            *guard = ClientState::Ready {
                service: Arc::new(service),
            };
        }

        Ok(initialize_result)
    }

    pub async fn list_tools(
        &self,
        params: Option<ListToolsRequestParams>,
        timeout: Option<Duration>,
    ) -> Result<ListToolsResult> {
        let service = self.service().await?;
        let rmcp_params = params
            .map(convert_to_rmcp::<_, PaginatedRequestParam>)
            .transpose()?;

        let fut = service.list_tools(rmcp_params);
        let result = run_with_timeout(fut, timeout, "tools/list")
            .await
            .map_err(|err| annotate_phase_error(Phase::ListTools, err))?;
        convert_to_mcp(result)
    }

    pub async fn call_tool(
        &self,
        name: String,
        arguments: Option<serde_json::Value>,
        timeout: Option<Duration>,
    ) -> Result<CallToolResult> {
        let service = self.service().await?;
        let params = CallToolRequestParams { arguments, name };
        let rmcp_params: CallToolRequestParam = convert_to_rmcp(params)?;
        let fut = service.call_tool(rmcp_params);
        let rmcp_result = run_with_timeout(fut, timeout, "tools/call")
            .await
            .map_err(|err| annotate_phase_error(Phase::CallTool, err))?;
        convert_call_tool_result(rmcp_result)
    }

    async fn service(&self) -> Result<Arc<RunningService<RoleClient, LoggingClientHandler>>> {
        let guard = self.state.lock().await;
        match &*guard {
            ClientState::Ready { service } => Ok(Arc::clone(service)),
            ClientState::Connecting { .. } => Err(anyhow!("MCP client not initialized")),
        }
    }

    pub async fn shutdown(&self) {
        if let Ok(service) = self.service().await {
            service.cancellation_token().cancel();
        }
    }
}

async fn await_handshake<F, E>(
    future: F,
    timeout: Option<Duration>,
) -> Result<RunningService<RoleClient, LoggingClientHandler>>
where
    F: Future<
        Output = Result<
            RunningService<RoleClient, LoggingClientHandler>,
            E,
        >,
    >,
    E: Into<anyhow::Error>,
{
    if let Some(duration) = timeout {
        match time::timeout(duration, future).await {
            Ok(Ok(service)) => Ok(service),
            Ok(Err(err)) => Err(handshake_failed_error(err)),
            Err(_) => Err(handshake_timeout_error(duration)),
        }
    } else {
        future.await.map_err(handshake_failed_error)
    }
}

fn annotate_phase_error(phase: Phase, err: anyhow::Error) -> anyhow::Error {
    err.context(format!("phase={}", phase.as_str()))
}

fn should_retry_initialize(err: &anyhow::Error, attempt: usize) -> bool {
    if attempt != 0 {
        return false;
    }

    for source in err.chain() {
        if let Some(reqwest_err) = source.downcast_ref::<ReqwestError>() {
            if reqwest_err.is_timeout() || reqwest_err.is_connect() {
                return true;
            }
        }

        if let Some(io_err) = source.downcast_ref::<io::Error>() {
            if matches!(
                io_err.kind(),
                io::ErrorKind::TimedOut | io::ErrorKind::ConnectionRefused
            ) {
                return true;
            }
        }
    }

    false
}

fn build_streamable_http_transport(
    url: &str,
    bearer_token: Option<&str>,
) -> StreamableHttpClientTransport<reqwest::Client> {
    let mut config = StreamableHttpClientTransportConfig::with_uri(url.to_string());
    if let Some(token) = bearer_token {
        config = config.auth_header(format!("Bearer {token}"));
    }
    StreamableHttpClientTransport::from_config(config)
}

fn handshake_failed_error(err: impl Into<anyhow::Error>) -> anyhow::Error {
    let err = err.into();
    anyhow!(
        "handshaking with MCP server failed: {err} (this client supports MCP schema version {MCP_SCHEMA_VERSION})"
    )
}

fn handshake_timeout_error(duration: Duration) -> anyhow::Error {
    anyhow!(HandshakeTimeoutError(duration))
}

#[derive(Debug)]
struct HandshakeTimeoutError(Duration);

impl fmt::Display for HandshakeTimeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "timed out awaiting MCP handshake after {:?}",
            self.0
        )
    }
}

impl StdError for HandshakeTimeoutError {}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn mcp_schema_version_is_well_formed() {
        assert!(!MCP_SCHEMA_VERSION.is_empty());
        let parts: Vec<&str> = MCP_SCHEMA_VERSION.split('-').collect();
        assert_eq!(
            parts.len(),
            3,
            "MCP_SCHEMA_VERSION should be in YYYY-MM-DD format"
        );
        assert!(parts.iter().all(|segment| !segment.trim().is_empty()));
    }

    #[test]
    fn annotate_phase_error_adds_phase_label() {
        let err = annotate_phase_error(Phase::ListTools, anyhow!("boom"));
        let message = err.to_string();
        assert_eq!(message, "phase=list_tools");
        let sources: Vec<String> = err.chain().map(|source| source.to_string()).collect();
        assert!(sources.iter().any(|s| s.contains("boom")), "sources: {sources:?}");
    }

    #[test]
    fn should_retry_initialize_detects_transient_errors() {
        let timeout_err = anyhow!(io::Error::new(io::ErrorKind::TimedOut, "timed out"));
        assert!(should_retry_initialize(&timeout_err, 0));
        assert!(!should_retry_initialize(&timeout_err, 1));

        let mismatch_err = anyhow!("protocol mismatch");
        assert!(!should_retry_initialize(&mismatch_err, 0));
    }
}
