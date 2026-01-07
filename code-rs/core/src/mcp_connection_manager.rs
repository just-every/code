//! Connection manager for Model Context Protocol (MCP) servers.
//!
//! The [`McpConnectionManager`] owns one [`code_rmcp_client::RmcpClient`] per
//! configured server (keyed by the *server name*). It offers convenience
//! helpers to query the available tools across *all* servers and returns them
//! in a single aggregated map using the fully-qualified tool name
//! `"<server><MCP_TOOL_NAME_DELIMITER><tool>"` as the key.

use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsString;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use code_rmcp_client::RmcpClient;
use mcp_types::ClientCapabilities;
use mcp_types::Implementation;
use mcp_types::Tool;

use serde_json::json;
use sha1::Digest;
use sha1::Sha1;
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tracing::info;
use tracing::warn;

use crate::config_types::McpServerConfig;
use crate::config_types::McpServerTransportConfig;

/// Delimiter used to separate the server name from the tool name in a fully
/// qualified tool name.
///
/// OpenAI requires tool names to conform to `^[a-zA-Z0-9_-]+$`, so we must
/// choose a delimiter from this character set.
const MCP_TOOL_NAME_DELIMITER: &str = "__";
const MAX_TOOL_NAME_LENGTH: usize = 64;

/// The Responses API requires tool names to match `^[a-zA-Z0-9_-]+$`.
/// MCP server/tool names are user-controlled, so sanitize the fully-qualified
/// name we expose to the model by replacing any disallowed character with `_`.
fn sanitize_responses_api_tool_name(name: &str) -> String {
    let mut sanitized = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            sanitized.push(c);
        } else {
            sanitized.push('_');
        }
    }

    if sanitized.is_empty() {
        "_".to_string()
    } else {
        sanitized
    }
}

fn sha1_hex(s: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(s.as_bytes());
    let sha1 = hasher.finalize();
    format!("{sha1:x}")
}

/// Append a deterministic SHA1 suffix while keeping the name within the maximum length.
fn append_sha1_suffix(base: &str, raw: &str) -> String {
    let sha1_str = sha1_hex(raw);
    let prefix_len = MAX_TOOL_NAME_LENGTH.saturating_sub(sha1_str.len());
    let prefix = if base.len() > prefix_len {
        &base[..prefix_len]
    } else {
        base
    };
    format!("{prefix}{sha1_str}")
}

/// Default timeout for initializing MCP server & initially listing tools.
const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// Map that holds a startup error for every MCP server that could **not** be
/// spawned successfully.
pub type ClientStartErrors = HashMap<String, anyhow::Error>;

fn qualify_tools(tools: Vec<ToolInfo>) -> HashMap<String, ToolInfo> {
    let mut used_names = HashSet::new();
    let mut seen_raw_names = HashSet::new();
    let mut qualified_tools = HashMap::new();
    for tool in tools {
        let qualified_name_raw = format!(
            "{}{}{}",
            tool.server_name, MCP_TOOL_NAME_DELIMITER, tool.tool_name
        );
        if !seen_raw_names.insert(qualified_name_raw.clone()) {
            warn!("skipping duplicated tool {}", qualified_name_raw);
            continue;
        }

        // Start from a "pretty" name (sanitized), then deterministically disambiguate on
        // collisions by appending a hash of the *raw* (unsanitized) qualified name. This
        // ensures tools like `foo.bar` and `foo_bar` don't collapse to the same key.
        let mut qualified_name = sanitize_responses_api_tool_name(&qualified_name_raw);

        // Enforce length constraints early; use the raw name for the hash input so the
        // output remains stable even when sanitization changes.
        if qualified_name.len() > MAX_TOOL_NAME_LENGTH {
            qualified_name = append_sha1_suffix(&qualified_name, &qualified_name_raw);
        }

        if used_names.contains(&qualified_name) {
            let disambiguated_name = append_sha1_suffix(&qualified_name, &qualified_name_raw);
            if used_names.contains(&disambiguated_name) {
                warn!("skipping duplicated tool {}", disambiguated_name);
                continue;
            }
            qualified_name = disambiguated_name;
        }

        used_names.insert(qualified_name.clone());
        qualified_tools.insert(qualified_name, tool);
    }

    qualified_tools
}

struct ToolInfo {
    server_name: String,
    tool_name: String,
    tool: Tool,
}

struct ManagedClient {
    client: McpClientAdapter,
    startup_timeout: Duration,
    tool_timeout: Option<Duration>,
}

#[derive(Clone)]
enum McpClientAdapter {
    Rmcp(Arc<RmcpClient>),
}

impl McpClientAdapter {
    async fn new_stdio_client(
        program: OsString,
        args: Vec<OsString>,
        env: Option<HashMap<String, String>>,
        params: mcp_types::InitializeRequestParams,
        startup_timeout: Duration,
    ) -> Result<Self> {
        tracing::debug!(
            "new_stdio_client program: {program:?} args: {args:?} env: {env:?} params: {params:?} startup_timeout: {startup_timeout:?}"
        );
        let client = Arc::new(RmcpClient::new_stdio_client(program, args, env).await?);
        client.initialize(params, Some(startup_timeout)).await?;
        Ok(McpClientAdapter::Rmcp(client))
    }

    async fn new_streamable_http_client(
        url: String,
        bearer_token: Option<String>,
        params: mcp_types::InitializeRequestParams,
        startup_timeout: Duration,
    ) -> Result<Self> {
        let client = Arc::new(RmcpClient::new_streamable_http_client(url, bearer_token)?);
        client.initialize(params, Some(startup_timeout)).await?;
        Ok(McpClientAdapter::Rmcp(client))
    }

    async fn list_tools(
        &self,
        params: Option<mcp_types::ListToolsRequestParams>,
        timeout: Option<Duration>,
    ) -> Result<mcp_types::ListToolsResult> {
        match self {
            McpClientAdapter::Rmcp(client) => client.list_tools(params, timeout).await,
        }
    }

    async fn call_tool(
        &self,
        name: String,
        arguments: Option<serde_json::Value>,
        timeout: Option<Duration>,
    ) -> Result<mcp_types::CallToolResult> {
        match self {
            McpClientAdapter::Rmcp(client) => client.call_tool(name, arguments, timeout).await,
        }
    }

    async fn into_shutdown(self) {
        match self {
            McpClientAdapter::Rmcp(client) => {
                client.shutdown().await;
            }
        }
    }
}

/// A thin wrapper around a set of running [`RmcpClient`] instances.
#[derive(Default)]
pub struct McpConnectionManager {
    /// Server-name -> client instance.
    ///
    /// The server name originates from the keys of the `mcp_servers` map in
    /// the user configuration.
    clients: RwLock<HashMap<String, ManagedClient>>,

    /// Fully qualified tool name -> tool instance.
    tools: HashMap<String, ToolInfo>,
}

impl McpConnectionManager {
    /// Spawn a [`RmcpClient`] for each configured server.
    ///
    /// * `mcp_servers` – Map loaded from the user configuration where *keys*
    ///   are human-readable server identifiers and *values* are the spawn
    ///   instructions.
    ///
    /// Servers that fail to start are reported in `ClientStartErrors`: the
    /// user should be informed about these errors.
    pub async fn new(
        mcp_servers: HashMap<String, McpServerConfig>,
        excluded_tools: HashSet<(String, String)>,
    ) -> Result<(Self, ClientStartErrors)> {
        // Early exit if no servers are configured.
        if mcp_servers.is_empty() {
            return Ok((Self::default(), ClientStartErrors::default()));
        }

        // Launch all configured servers concurrently.
        let mut join_set = JoinSet::new();
        let mut errors = ClientStartErrors::new();

        for (server_name, cfg) in mcp_servers {
            // Validate server name before spawning
            if !is_valid_mcp_server_name(&server_name) {
                let error = anyhow::anyhow!(
                    "invalid server name '{server_name}': must match pattern ^[a-zA-Z0-9_-]+$"
                );
                errors.insert(server_name, error);
                continue;
            }

            let startup_timeout = cfg.startup_timeout_sec.unwrap_or(DEFAULT_STARTUP_TIMEOUT);
            let tool_timeout = cfg.tool_timeout_sec;

            join_set.spawn(async move {
                let McpServerConfig { transport, .. } = cfg;
                let server_name_for_error = server_name.clone();
                let params = mcp_types::InitializeRequestParams {
                    capabilities: ClientCapabilities {
                        experimental: None,
                        roots: None,
                        sampling: None,
                        // https://modelcontextprotocol.io/specification/2025-06-18/client/elicitation#capabilities
                        // indicates this should be an empty object.
                        elicitation: Some(json!({})),
                    },
                    client_info: Implementation {
                        name: "codex-mcp-client".to_owned(),
                        version: env!("CARGO_PKG_VERSION").to_owned(),
                        title: Some("Codex".into()),
                        // This field is used by Codex when it is an MCP
                        // server: it should not be used when Codex is
                        // an MCP client.
                        user_agent: None,
                    },
                    protocol_version: mcp_types::MCP_SCHEMA_VERSION.to_owned(),
                };

                let client = match transport {
                    McpServerTransportConfig::Stdio { command, args, env } => {
                        let command_for_error = command.clone();
                        let args_for_error = args.clone();
                        let command_os: OsString = command.into();
                        let args_os: Vec<OsString> = args.into_iter().map(Into::into).collect();
                        McpClientAdapter::new_stdio_client(
                            command_os,
                            args_os,
                            env,
                            params.clone(),
                            startup_timeout,
                        )
                        .await
                        .with_context(|| {
                            if args_for_error.is_empty() {
                                format!(
                                    "failed to spawn MCP server `{}` using command `{}`",
                                    server_name_for_error, command_for_error
                                )
                            } else {
                                format!(
                                    "failed to spawn MCP server `{}` using command `{}` with args {:?}",
                                    server_name_for_error, command_for_error, args_for_error
                                )
                            }
                        })
                    }
                    McpServerTransportConfig::StreamableHttp { url, bearer_token } => {
                        McpClientAdapter::new_streamable_http_client(
                            url,
                            bearer_token,
                            params,
                            startup_timeout,
                        )
                        .await
                    }
                }
                .map(|c| (c, startup_timeout));

                ((server_name, tool_timeout), client)
            });
        }

        let mut clients: HashMap<String, ManagedClient> = HashMap::with_capacity(join_set.len());

        while let Some(res) = join_set.join_next().await {
            let ((server_name, tool_timeout), client_res) = match res {
                Ok(result) => result,
                Err(e) => {
                    warn!("Task panic when starting MCP server: {e:#}");
                    continue;
                }
            };

            match client_res {
                Ok((client, startup_timeout)) => {
                    clients.insert(
                        server_name,
                        ManagedClient {
                            client,
                            startup_timeout,
                            tool_timeout,
                        },
                    );
                }
                Err(e) => {
                    errors.insert(server_name, e);
                }
            }
        }

        let all_tools = list_all_tools(&clients, &excluded_tools, &mut errors).await;

        let tools = qualify_tools(all_tools);

        Ok((Self {
            clients: RwLock::new(clients),
            tools,
        }, errors))
    }

    /// Returns a single map that contains **all** tools. Each key is the
    /// fully-qualified name for the tool.
    pub fn list_all_tools(&self) -> HashMap<String, Tool> {
        self.tools
            .iter()
            .map(|(name, tool)| (name.clone(), tool.tool.clone()))
            .collect()
    }

    /// Invoke the tool indicated by the (server, tool) pair.
    pub async fn call_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Option<serde_json::Value>,
        timeout_override: Option<Duration>,
    ) -> Result<mcp_types::CallToolResult> {
        let (client, timeout) = {
            let clients = self.clients.read().await;
            let managed = clients
                .get(server)
                .ok_or_else(|| anyhow!("unknown MCP server '{server}'"))?;
            let timeout = timeout_override.or(managed.tool_timeout);
            (managed.client.clone(), timeout)
        };

        client
            .call_tool(tool.to_string(), arguments, timeout)
            .await
            .with_context(|| format!("tool call failed for `{server}/{tool}`"))
    }

    pub fn parse_tool_name(&self, tool_name: &str) -> Option<(String, String)> {
        self.tools
            .get(tool_name)
            .map(|tool| (tool.server_name.clone(), tool.tool_name.clone()))
    }

    pub async fn shutdown_all(&self) {
        let mut clients = self.clients.write().await;
        let drained: Vec<ManagedClient> = clients.drain().map(|(_, managed)| managed).collect();
        drop(clients);

        for managed in drained {
            managed.shutdown().await;
        }
    }

}

impl ManagedClient {
    async fn shutdown(self) {
        self.client.into_shutdown().await;
    }
}

/// Query every server for its available tools and return a single map that
/// contains **all** tools. Each key is the fully-qualified name for the tool.
async fn list_all_tools(
    clients: &HashMap<String, ManagedClient>,
    excluded_tools: &HashSet<(String, String)>,
    errors: &mut ClientStartErrors,
) -> Vec<ToolInfo> {
    let mut join_set = JoinSet::new();

    // Spawn one task per server so we can query them concurrently. This
    // keeps the overall latency roughly at the slowest server instead of
    // the cumulative latency.
    for (server_name, managed_client) in clients {
        let server_name_cloned = server_name.clone();
        let client_clone = managed_client.client.clone();
        let startup_timeout = managed_client.startup_timeout;
        join_set.spawn(async move {
            let res = client_clone.list_tools(None, Some(startup_timeout)).await;
            (server_name_cloned, res)
        });
    }

    let mut aggregated: Vec<ToolInfo> = Vec::with_capacity(join_set.len());

    while let Some(join_res) = join_set.join_next().await {
        let (server_name, list_result) = if let Ok(result) = join_res {
            result
        } else {
            warn!("Task panic when listing tools for MCP server: {join_res:#?}");
            continue;
        };

        match list_result {
            Ok(result) => {
                for tool in result.tools {
                    if excluded_tools.contains(&(server_name.clone(), tool.name.clone())) {
                        continue;
                    }
                    let tool_info = ToolInfo {
                        server_name: server_name.clone(),
                        tool_name: tool.name.clone(),
                        tool,
                    };
                    aggregated.push(tool_info);
                }
            }
            Err(err) => {
                warn!(
                    "Failed to list tools for MCP server '{server_name}': {err:#?}"
                );
                errors.insert(server_name, err.into());
            }
        }
    }

    info!(
        "aggregated {} tools from {} servers",
        aggregated.len(),
        clients.len()
    );

    aggregated
}

fn is_valid_mcp_server_name(server_name: &str) -> bool {
    !server_name.is_empty()
        && server_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcp_types::ToolInputSchema;

    fn create_test_tool(server_name: &str, tool_name: &str) -> ToolInfo {
        ToolInfo {
            server_name: server_name.to_string(),
            tool_name: tool_name.to_string(),
            tool: Tool {
                annotations: None,
                description: Some(format!("Test tool: {tool_name}")),
                input_schema: ToolInputSchema {
                    properties: None,
                    required: None,
                    r#type: "object".to_string(),
                },
                name: tool_name.to_string(),
                output_schema: None,
                title: None,
            },
        }
    }

    #[test]
    fn test_qualify_tools_short_non_duplicated_names() {
        let tools = vec![
            create_test_tool("server1", "tool1"),
            create_test_tool("server1", "tool2"),
        ];

        let qualified_tools = qualify_tools(tools);

        assert_eq!(qualified_tools.len(), 2);
        assert!(qualified_tools.contains_key("server1__tool1"));
        assert!(qualified_tools.contains_key("server1__tool2"));
    }

    #[test]
    fn test_qualify_tools_duplicated_names_skipped() {
        let tools = vec![
            create_test_tool("server1", "duplicate_tool"),
            create_test_tool("server1", "duplicate_tool"),
        ];

        let qualified_tools = qualify_tools(tools);

        // Only the first tool should remain, the second is skipped
        assert_eq!(qualified_tools.len(), 1);
        assert!(qualified_tools.contains_key("server1__duplicate_tool"));
    }

    #[test]
    fn test_qualify_tools_long_names_same_server() {
        let server_name = "my_server";

        let tools = vec![
            create_test_tool(
                server_name,
                "extremely_lengthy_function_name_that_absolutely_surpasses_all_reasonable_limits",
            ),
            create_test_tool(
                server_name,
                "yet_another_extremely_lengthy_function_name_that_absolutely_surpasses_all_reasonable_limits",
            ),
        ];

        let qualified_tools = qualify_tools(tools);

        assert_eq!(qualified_tools.len(), 2);

        let mut keys: Vec<_> = qualified_tools.keys().cloned().collect();
        keys.sort();

        assert_eq!(keys[0].len(), 64);
        assert_eq!(
            keys[0],
            "my_server__extremely_lena02e507efc5a9de88637e436690364fd4219e4ef"
        );

        assert_eq!(keys[1].len(), 64);
        assert_eq!(
            keys[1],
            "my_server__yet_another_e1c3987bd9c50b826cbe1687966f79f0c602d19ca"
        );
    }

    #[test]
    fn test_qualify_tools_sanitizes_invalid_characters() {
        let tools = vec![create_test_tool("server.one", "tool.two")];

        let qualified_tools = qualify_tools(tools);

        assert_eq!(qualified_tools.len(), 1);
        let (qualified_name, tool) = qualified_tools.into_iter().next().expect("one tool");
        assert_eq!(qualified_name, "server_one__tool_two");

        // The key is sanitized for OpenAI, but we keep original parts for the actual MCP call.
        assert_eq!(tool.server_name, "server.one");
        assert_eq!(tool.tool_name, "tool.two");

        assert!(
            qualified_name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "qualified name must be Responses API compatible: {qualified_name:?}"
        );
    }

    #[tokio::test]
    async fn stdio_spawn_error_mentions_server_and_command() {
        let mut servers = HashMap::new();
        servers.insert(
            "context7-mcp".to_string(),
            McpServerConfig {
                transport: McpServerTransportConfig::Stdio {
                    command: "nonexistent-cmd".to_string(),
                    args: Vec::new(),
                    env: None,
                },
                startup_timeout_sec: None,
                tool_timeout_sec: None,
            },
        );

        let (_manager, errors) = McpConnectionManager::new(servers, HashSet::new())
            .await
            .expect("manager creation should succeed even when servers fail");

        let err = errors
            .get("context7-mcp")
            .expect("missing executable should be reported under server name");
        let msg = format!("{err:#}");

        assert!(msg.contains("context7-mcp"), "error should mention the server name");
        assert!(
            msg.contains("nonexistent-cmd"),
            "error should include the missing command, got: {msg}"
        );
    }
}
