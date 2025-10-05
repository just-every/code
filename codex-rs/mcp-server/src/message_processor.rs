use std::collections::HashMap;
use std::path::PathBuf;

// TODO: CodexMessageProcessor is private
// use codex_app_server::codex_message_processor::CodexMessageProcessor;
use crate::codex_tool_config::AcpNewSessionToolArgs;
use crate::codex_tool_config::AcpPromptToolArgs;
use crate::codex_tool_config::AcpSetModelToolArgs;
use crate::codex_tool_config::CodexToolCallParam;
use crate::codex_tool_config::CodexToolCallReplyParam;
use crate::codex_tool_config::create_tool_for_acp_new_session;
use crate::codex_tool_config::create_tool_for_acp_prompt;
use crate::codex_tool_config::create_tool_for_acp_set_model;
use crate::codex_tool_config::create_tool_for_codex_tool_call_param;
use crate::codex_tool_config::create_tool_for_codex_tool_call_reply_param;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::{OutgoingMessageSender, OutgoingNotification};
use crate::session_store::SessionMap;
use agent_client_protocol as acp;
use anyhow::Context as _;
use anyhow::anyhow;
use codex_app_server_protocol::ClientRequest;
use codex_protocol::ConversationId;
use codex_protocol::protocol::SessionSource;

use codex_common::model_presets::{ModelPreset, builtin_model_presets};
use codex_core::AuthManager;
use codex_core::ConversationManager;
use codex_core::config_types::{McpServerConfig, McpServerTransportConfig};
// TODO: ClientTools and ReasoningEffort removed from codex_core::config_types
// ClientTools is removed, ReasoningEffort is in codex_protocol::config_types
use codex_app_server_protocol::AuthMode;
use codex_core::config::Config;
use codex_core::default_client::USER_AGENT_SUFFIX;
use codex_core::default_client::get_codex_user_agent;
use codex_core::model_family::{derive_default_model_family, find_family_for_model};
use codex_core::protocol::Op;
use codex_core::protocol::Submission;
use codex_protocol::config_types::ReasoningEffort;
use mcp_types::CallToolRequestParams;
use mcp_types::CallToolResult;
use mcp_types::ClientRequest as McpClientRequest;
use mcp_types::ContentBlock;
use mcp_types::JSONRPCError;
use mcp_types::JSONRPCErrorError;
use mcp_types::JSONRPCNotification;
use mcp_types::JSONRPCRequest;
use mcp_types::JSONRPCResponse;
use mcp_types::ListToolsResult;
use mcp_types::ModelContextProtocolRequest;
use mcp_types::RequestId;
use mcp_types::ServerNotification;
use mcp_types::TextContent;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task;
use uuid::Uuid;

pub(crate) struct MessageProcessor {
    outgoing: Arc<OutgoingMessageSender>,
    initialized: bool,
    codex_linux_sandbox_exe: Option<PathBuf>,
    conversation_manager: Arc<ConversationManager>,
    session_map: SessionMap,
    running_requests_id_to_codex_uuid: Arc<Mutex<HashMap<RequestId, Uuid>>>,
    base_config: Arc<Config>,
    // TODO: CodexMessageProcessor is private
    // codex_message_processor: CodexMessageProcessor,
}

impl MessageProcessor {
    /// Create a new `MessageProcessor`, retaining a handle to the outgoing
    /// `Sender` so handlers can enqueue messages to be written to stdout.
    pub(crate) fn new(
        outgoing: OutgoingMessageSender,
        codex_linux_sandbox_exe: Option<PathBuf>,
        config: Arc<Config>,
    ) -> Self {
        let outgoing = Arc::new(outgoing);
        // TODO: shared_with_mode_and_originator method removed, using new() instead
        // Also responses_originator_header field removed from Config
        let auth_manager = Arc::new(AuthManager::new(
            config.codex_home.clone(),
            true, // enable_codex_api_key_env
        ));
        let conversation_manager = Arc::new(ConversationManager::new(
            auth_manager.clone(),
            SessionSource::Mcp,
        ));
        // TODO: CodexMessageProcessor is private, commenting out
        // let config_for_processor = config.clone();
        // let codex_message_processor = CodexMessageProcessor::new(
        //     auth_manager,
        //     conversation_manager.clone(),
        //     outgoing.clone(),
        //     codex_linux_sandbox_exe.clone(),
        //     config_for_processor.clone(),
        // );
        let session_map: SessionMap = Arc::new(Mutex::new(HashMap::new()));
        Self {
            outgoing,
            initialized: false,
            codex_linux_sandbox_exe,
            conversation_manager,
            session_map,
            running_requests_id_to_codex_uuid: Arc::new(Mutex::new(HashMap::new())),
            base_config: config,
            // TODO: CodexMessageProcessor is private, commenting out
            // codex_message_processor,
        }
    }

    pub(crate) async fn process_request(&mut self, request: JSONRPCRequest) {
        // TODO: CodexMessageProcessor is private, commenting out
        // if let Ok(request_json) = serde_json::to_value(request.clone()) {
        //     if let Ok(codex_request) = serde_json::from_value::<ClientRequest>(request_json) {
        //         // If the request is a Codex request, handle it with the Codex
        //         // message processor.
        //         self.codex_message_processor
        //             .process_request(codex_request)
        //             .await;
        //         return;
        //     }
        // }

        tracing::trace!("processing JSON-RPC request: {}", request.method);
        // Hold on to the ID so we can respond.
        let request_id = request.id.clone();

        if request.method == acp::AGENT_METHOD_NAMES.session_new {
            tracing::info!("handling session/new via ACP shim");
            if let Some(params) = request.params.clone() {
                match serde_json::from_value::<AcpNewSessionToolArgs>(params) {
                    Ok(session_params) => {
                        self.handle_session_new(request_id, session_params).await;
                    }
                    Err(err) => {
                        tracing::warn!("Failed to parse session/new params: {err}");
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("invalid session/new params: {err}"),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                    }
                }
            } else {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "session/new requires params".to_string(),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
            return;
        }

        if request.method == acp::AGENT_METHOD_NAMES.session_prompt {
            tracing::info!("handling session/prompt via ACP shim");
            if let Some(params) = request.params.clone() {
                match serde_json::from_value::<AcpPromptToolArgs>(params) {
                    Ok(prompt_params) => {
                        self.handle_session_prompt(request_id, prompt_params).await;
                    }
                    Err(err) => {
                        tracing::warn!("Failed to parse session/prompt params: {err}");
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("invalid session/prompt params: {err}"),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                    }
                }
            } else {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "session/prompt requires params".to_string(),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
            return;
        }

        // TODO: model_select field removed from AGENT_METHOD_NAMES
        if request.method == "acp/model_select" {
            // was: acp::AGENT_METHOD_NAMES.model_select
            tracing::info!("handling session/set_model via ACP shim");
            if let Some(params) = request.params.clone() {
                match serde_json::from_value::<acp::SetSessionModelRequest>(params) {
                    Ok(model_params) => {
                        self.handle_session_set_model(request_id, model_params)
                            .await;
                    }
                    Err(err) => {
                        tracing::warn!("Failed to parse session/set_model params: {err}");
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("invalid session/set_model params: {err}"),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                    }
                }
            } else {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "session/set_model requires params".to_string(),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
            return;
        }

        let mut request = request;

        if request.method == mcp_types::InitializeRequest::METHOD {
            if let Some(params) = request.params.as_mut() {
                if let Some(protocol_version) = params.get_mut("protocolVersion") {
                    if let Some(num) = protocol_version.as_i64() {
                        *protocol_version = serde_json::Value::String(num.to_string());
                    } else if let Some(num) = protocol_version.as_u64() {
                        *protocol_version = serde_json::Value::String(num.to_string());
                    } else if protocol_version.is_null() {
                        *protocol_version = serde_json::Value::String("1".to_string());
                    }
                }

                if let serde_json::Value::Object(map) = params {
                    if !map.contains_key("capabilities") {
                        let capabilities = map
                            .remove("clientCapabilities")
                            .unwrap_or_else(|| serde_json::Value::Object(Default::default()));

                        let mut cap_wrapper = serde_json::Map::new();
                        cap_wrapper.insert("experimental".to_string(), capabilities);
                        map.insert(
                            "capabilities".to_string(),
                            serde_json::Value::Object(cap_wrapper),
                        );
                    }

                    map.entry("clientInfo").or_insert_with(|| {
                        let mut info = serde_json::Map::new();
                        info.insert(
                            "name".to_string(),
                            serde_json::Value::String("unknown-client".into()),
                        );
                        info.insert(
                            "version".to_string(),
                            serde_json::Value::String("0.0.0".into()),
                        );
                        serde_json::Value::Object(info)
                    });
                }
            }
        }

        let client_request = match McpClientRequest::try_from(request) {
            Ok(client_request) => client_request,
            Err(e) => {
                tracing::warn!("Failed to convert request: {e}");
                return;
            }
        };

        // Dispatch to a dedicated handler for each request type.
        match client_request {
            McpClientRequest::InitializeRequest(params) => {
                self.handle_initialize(request_id, params).await;
            }
            McpClientRequest::PingRequest(params) => {
                self.handle_ping(request_id, params).await;
            }
            McpClientRequest::ListResourcesRequest(params) => {
                self.handle_list_resources(params);
            }
            McpClientRequest::ListResourceTemplatesRequest(params) => {
                self.handle_list_resource_templates(params);
            }
            McpClientRequest::ReadResourceRequest(params) => {
                self.handle_read_resource(params);
            }
            McpClientRequest::SubscribeRequest(params) => {
                self.handle_subscribe(params);
            }
            McpClientRequest::UnsubscribeRequest(params) => {
                self.handle_unsubscribe(params);
            }
            McpClientRequest::ListPromptsRequest(params) => {
                self.handle_list_prompts(params);
            }
            McpClientRequest::GetPromptRequest(params) => {
                self.handle_get_prompt(params);
            }
            McpClientRequest::ListToolsRequest(params) => {
                self.handle_list_tools(request_id, params).await;
            }
            McpClientRequest::CallToolRequest(params) => {
                self.handle_call_tool(request_id, params).await;
            }
            McpClientRequest::SetLevelRequest(params) => {
                self.handle_set_level(params);
            }
            McpClientRequest::CompleteRequest(params) => {
                self.handle_complete(params);
            }
        }
    }

    /// Handle a standalone JSON-RPC response originating from the peer.
    pub(crate) async fn process_response(&mut self, response: JSONRPCResponse) {
        tracing::info!("<- response: {:?}", response);
        let JSONRPCResponse { id, result, .. } = response;
        self.outgoing.notify_client_response(id, result).await
    }

    /// Handle a fire-and-forget JSON-RPC notification.
    pub(crate) async fn process_notification(&mut self, notification: JSONRPCNotification) {
        if notification.method == acp::AGENT_METHOD_NAMES.session_cancel {
            tracing::info!("handling session/cancel via ACP shim");
            if let Some(params) = notification.params {
                match serde_json::from_value::<acp::CancelNotification>(params) {
                    Ok(cancel) => {
                        self.handle_session_cancel(cancel).await;
                    }
                    Err(err) => {
                        tracing::warn!("Failed to parse session/cancel params: {err}");
                    }
                }
            } else {
                tracing::warn!("session/cancel notification missing params");
            }
            return;
        }

        let server_notification = match ServerNotification::try_from(notification) {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!("Failed to convert notification: {e}");
                return;
            }
        };

        // Similar to requests, route each notification type to its own stub
        // handler so additional logic can be implemented incrementally.
        match server_notification {
            ServerNotification::CancelledNotification(params) => {
                self.handle_cancelled_notification(params).await;
            }
            ServerNotification::ProgressNotification(params) => {
                self.handle_progress_notification(params);
            }
            ServerNotification::ResourceListChangedNotification(params) => {
                self.handle_resource_list_changed(params);
            }
            ServerNotification::ResourceUpdatedNotification(params) => {
                self.handle_resource_updated(params);
            }
            ServerNotification::PromptListChangedNotification(params) => {
                self.handle_prompt_list_changed(params);
            }
            ServerNotification::ToolListChangedNotification(params) => {
                self.handle_tool_list_changed(params);
            }
            ServerNotification::LoggingMessageNotification(params) => {
                self.handle_logging_message(params);
            }
        }
    }

    /// Handle an error object received from the peer.
    pub(crate) fn process_error(&mut self, err: JSONRPCError) {
        tracing::error!("<- error: {:?}", err);
    }

    async fn handle_initialize(
        &mut self,
        id: RequestId,
        params: <mcp_types::InitializeRequest as ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("initialize -> params: {:?}", params);

        if self.initialized {
            // Already initialised: send JSON-RPC error response.
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "initialize called more than once".to_string(),
                data: None,
            };
            self.outgoing.send_error(id, error).await;
            return;
        }

        let client_info = params.client_info;
        let name = client_info.name;
        let version = client_info.version;
        let user_agent_suffix = format!("{name}; {version}");
        if let Ok(mut suffix) = USER_AGENT_SUFFIX.lock() {
            *suffix = Some(user_agent_suffix);
        }

        self.initialized = true;

        // Build a minimal InitializeResult. Fill with placeholders.
        let server_info = serde_json::json!({
            "name": "code-mcp-server",
            "version": env!("CARGO_PKG_VERSION"),
            "title": "Codex",
            "user_agent": get_codex_user_agent(), // TODO: was get_codex_user_agent_default()
        });

        let agent_capabilities = serde_json::json!({
            "promptCapabilities": {
                "image": true,
                "embeddedContext": true,
                "audio": false
            },
            "mcpCapabilities": {
                "http": false,
                "sse": false
            }
        });

        let auth_methods = serde_json::json!([{
            "id": "code-login",
            "name": "Use Code login",
            "description": "Run `code login` (ChatGPT or API key) before connecting."
        }]);

        let result = serde_json::json!({
            "protocolVersion": 1,
            "serverInfo": server_info,
            "capabilities": {
                "tools": {
                    "listChanged": true
                }
            },
            "agentCapabilities": agent_capabilities,
            "authMethods": auth_methods
        });

        self.outgoing.send_response(id, result).await;
    }

    async fn send_response<T>(&self, id: RequestId, result: T::Result)
    where
        T: ModelContextProtocolRequest,
    {
        self.outgoing.send_response(id, result).await;
    }

    async fn handle_ping(
        &self,
        id: RequestId,
        params: <mcp_types::PingRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("ping -> params: {:?}", params);
        let result = json!({});
        self.send_response::<mcp_types::PingRequest>(id, result)
            .await;
    }

    fn handle_list_resources(
        &self,
        params: <mcp_types::ListResourcesRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("resources/list -> params: {:?}", params);
    }

    fn handle_list_resource_templates(
        &self,
        params:
            <mcp_types::ListResourceTemplatesRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("resources/templates/list -> params: {:?}", params);
    }

    fn handle_read_resource(
        &self,
        params: <mcp_types::ReadResourceRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("resources/read -> params: {:?}", params);
    }

    fn handle_subscribe(
        &self,
        params: <mcp_types::SubscribeRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("resources/subscribe -> params: {:?}", params);
    }

    fn handle_unsubscribe(
        &self,
        params: <mcp_types::UnsubscribeRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("resources/unsubscribe -> params: {:?}", params);
    }

    fn handle_list_prompts(
        &self,
        params: <mcp_types::ListPromptsRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("prompts/list -> params: {:?}", params);
    }

    fn handle_get_prompt(
        &self,
        params: <mcp_types::GetPromptRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("prompts/get -> params: {:?}", params);
    }

    async fn handle_list_tools(
        &self,
        id: RequestId,
        params: <mcp_types::ListToolsRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::trace!("tools/list -> {params:?}");
        let result = ListToolsResult {
            tools: vec![
                create_tool_for_codex_tool_call_param(),
                create_tool_for_codex_tool_call_reply_param(),
                create_tool_for_acp_new_session(),
                create_tool_for_acp_prompt(),
                create_tool_for_acp_set_model(),
            ],
            next_cursor: None,
        };

        self.send_response::<mcp_types::ListToolsRequest>(id, result)
            .await;
    }

    async fn handle_call_tool(
        &self,
        id: RequestId,
        params: <mcp_types::CallToolRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("tools/call -> params: {:?}", params);
        let CallToolRequestParams { name, arguments } = params;

        match name.as_str() {
            "codex" => self.handle_tool_call_codex(id, arguments).await,
            "codex-reply" => {
                self.handle_tool_call_codex_session_reply(id, arguments)
                    .await
            }
            _ if name == acp::AGENT_METHOD_NAMES.session_new => {
                self.handle_tool_call_acp_new_session(id, arguments).await
            }
            _ if name == acp::AGENT_METHOD_NAMES.session_prompt => {
                self.handle_tool_call_acp_prompt(id, arguments).await
            }
            // TODO: model_select field removed from AGENT_METHOD_NAMES
            _ if name == "acp/model_select" => {
                // was: acp::AGENT_METHOD_NAMES.model_select
                self.handle_tool_call_acp_set_model(id, arguments).await
            }
            _ => {
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_string(),
                        text: format!("Unknown tool '{name}'"),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    structured_content: None,
                };
                self.send_response::<mcp_types::CallToolRequest>(id, result)
                    .await;
            }
        }
    }
    async fn handle_tool_call_codex(&self, id: RequestId, arguments: Option<serde_json::Value>) {
        let (initial_prompt, config): (String, Config) = match arguments {
            Some(json_val) => match serde_json::from_value::<CodexToolCallParam>(json_val) {
                Ok(tool_cfg) => match tool_cfg
                    .into_config(self.codex_linux_sandbox_exe.clone())
                    .await
                {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        let result = CallToolResult {
                            content: vec![ContentBlock::TextContent(TextContent {
                                r#type: "text".to_owned(),
                                text: format!(
                                    "Failed to load Codex configuration from overrides: {e}"
                                ),
                                annotations: None,
                            })],
                            is_error: Some(true),
                            structured_content: None,
                        };
                        self.send_response::<mcp_types::CallToolRequest>(id, result)
                            .await;
                        return;
                    }
                },
                Err(e) => {
                    let result = CallToolResult {
                        content: vec![ContentBlock::TextContent(TextContent {
                            r#type: "text".to_owned(),
                            text: format!("Failed to parse configuration for Codex tool: {e}"),
                            annotations: None,
                        })],
                        is_error: Some(true),
                        structured_content: None,
                    };
                    self.send_response::<mcp_types::CallToolRequest>(id, result)
                        .await;
                    return;
                }
            },
            None => {
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_string(),
                        text:
                            "Missing arguments for codex tool-call; the `prompt` field is required."
                                .to_string(),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    structured_content: None,
                };
                self.send_response::<mcp_types::CallToolRequest>(id, result)
                    .await;
                return;
            }
        };

        // Clone outgoing and session map to move into async task.
        let outgoing = self.outgoing.clone();
        let conversation_manager = self.conversation_manager.clone();
        let session_map = self.session_map.clone();
        let running_requests_id_to_codex_uuid = self.running_requests_id_to_codex_uuid.clone();

        // Spawn an async task to handle the Codex session so that we do not
        // block the synchronous message-processing loop.
        task::spawn(async move {
            // Run the Codex session and stream events back to the client.
            crate::codex_tool_runner::run_codex_tool_session(
                id,
                initial_prompt,
                config,
                outgoing,
                session_map,
                conversation_manager,
                running_requests_id_to_codex_uuid,
            )
            .await;
        });
    }

    async fn handle_tool_call_codex_session_reply(
        &self,
        request_id: RequestId,
        arguments: Option<serde_json::Value>,
    ) {
        tracing::info!("tools/call -> params: {:?}", arguments);

        // parse arguments
        let CodexToolCallReplyParam { session_id, prompt } = match arguments {
            Some(json_val) => match serde_json::from_value::<CodexToolCallReplyParam>(json_val) {
                Ok(params) => params,
                Err(e) => {
                    tracing::error!("Failed to parse Codex tool call reply parameters: {e}");
                    let result = CallToolResult {
                        content: vec![ContentBlock::TextContent(TextContent {
                            r#type: "text".to_owned(),
                            text: format!("Failed to parse configuration for Codex tool: {e}"),
                            annotations: None,
                        })],
                        is_error: Some(true),
                        structured_content: None,
                    };
                    self.send_response::<mcp_types::CallToolRequest>(request_id, result)
                        .await;
                    return;
                }
            },
            None => {
                tracing::error!(
                    "Missing arguments for codex-reply tool-call; the `session_id` and `prompt` fields are required."
                );
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_owned(),
                        text: "Missing arguments for codex-reply tool-call; the `session_id` and `prompt` fields are required.".to_owned(),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    structured_content: None,
                };
                self.send_response::<mcp_types::CallToolRequest>(request_id, result)
                    .await;
                return;
            }
        };
        let session_id = match Uuid::parse_str(&session_id) {
            Ok(id) => id,
            Err(e) => {
                tracing::error!("Failed to parse session_id: {e}");
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_owned(),
                        text: format!("Failed to parse session_id: {e}"),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    structured_content: None,
                };
                self.send_response::<mcp_types::CallToolRequest>(request_id, result)
                    .await;
                return;
            }
        };

        let outgoing = self.outgoing.clone();
        let running_requests_id_to_codex_uuid = self.running_requests_id_to_codex_uuid.clone();
        let session_map = self.session_map.clone();

        tokio::spawn(async move {
            let codex = {
                let map = session_map.lock().await;
                map.get(&session_id).map(|entry| entry.conversation.clone())
            };

            let Some(codex) = codex else {
                tracing::warn!("Session not found for session_id: {session_id}");
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_owned(),
                        text: format!("Session not found for session_id: {session_id}"),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    structured_content: None,
                };
                outgoing.send_response(request_id, result).await;
                return;
            };

            crate::codex_tool_runner::run_codex_tool_session_reply(
                codex,
                outgoing,
                request_id,
                prompt,
                running_requests_id_to_codex_uuid,
                session_id,
            )
            .await;
        });
    }

    fn handle_set_level(
        &self,
        params: <mcp_types::SetLevelRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("logging/setLevel -> params: {:?}", params);
    }

    fn handle_complete(
        &self,
        params: <mcp_types::CompleteRequest as mcp_types::ModelContextProtocolRequest>::Params,
    ) {
        tracing::info!("completion/complete -> params: {:?}", params);
    }

    // ---------------------------------------------------------------------
    // Notification handlers
    // ---------------------------------------------------------------------

    async fn handle_cancelled_notification(
        &self,
        params: <mcp_types::CancelledNotification as mcp_types::ModelContextProtocolNotification>::Params,
    ) {
        let request_id = params.request_id;
        // Create a stable string form early for logging and submission id.
        let request_id_string = match &request_id {
            RequestId::String(s) => s.clone(),
            RequestId::Integer(i) => i.to_string(),
        };

        // Obtain the session_id while holding the first lock, then release.
        let session_id = {
            let map_guard = self.running_requests_id_to_codex_uuid.lock().await;
            match map_guard.get(&request_id) {
                Some(id) => *id, // Uuid is Copy
                None => {
                    tracing::warn!("Session not found for request_id: {}", request_id_string);
                    return;
                }
            }
        };
        tracing::info!("session_id: {session_id}");

        // Obtain the Codex conversation from the session map, falling back to the conversation manager.
        let codex_arc = if let Some(entry) = self.session_map.lock().await.get(&session_id) {
            entry.conversation.clone()
        } else {
            match self
                .conversation_manager
                .get_conversation(ConversationId::from(session_id))
                .await
            {
                Ok(c) => c,
                Err(_) => {
                    tracing::warn!("Session not found for session_id: {session_id}");
                    return;
                }
            }
        };

        // Submit interrupt to Codex.
        let err = codex_arc
            .submit_with_id(Submission {
                id: request_id_string,
                op: codex_core::protocol::Op::Interrupt,
            })
            .await;
        if let Err(e) = err {
            tracing::error!("Failed to submit interrupt to Codex: {e}");
            return;
        }
        // unregister the id so we don't keep it in the map
        self.running_requests_id_to_codex_uuid
            .lock()
            .await
            .remove(&request_id);
    }

    fn handle_progress_notification(
        &self,
        params: <mcp_types::ProgressNotification as mcp_types::ModelContextProtocolNotification>::Params,
    ) {
        tracing::info!("notifications/progress -> params: {:?}", params);
    }

    fn handle_resource_list_changed(
        &self,
        params: <mcp_types::ResourceListChangedNotification as mcp_types::ModelContextProtocolNotification>::Params,
    ) {
        tracing::info!(
            "notifications/resources/list_changed -> params: {:?}",
            params
        );
    }

    async fn handle_tool_call_acp_new_session(
        &self,
        request_id: RequestId,
        arguments: Option<serde_json::Value>,
    ) {
        let config = match self.acp_new_session_cfg(arguments).await {
            Ok(cfg) => cfg,
            Err(err) => {
                tracing::warn!("Failed to construct new session config: {}", err);
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_owned(),
                        text: format!("Failed to construct new session config: {err}"),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    structured_content: None,
                };
                self.outgoing.send_response(request_id, result).await;
                return;
            }
        };

        let outgoing = self.outgoing.clone();
        let session_map = self.session_map.clone();
        let conversation_manager = self.conversation_manager.clone();
        let models_state = session_models_from_config(&config);

        task::spawn(async move {
            let Some(session_id) = crate::acp_tool_runner::new_session(
                request_id.clone(),
                config,
                outgoing.clone(),
                session_map,
                conversation_manager,
            )
            .await
            else {
                return;
            };

            let session_id_str = session_id.to_string();
            let response_struct = acp::NewSessionResponse {
                session_id: acp::SessionId(Arc::from(session_id_str.clone())),
                modes: Some(default_session_modes()),
                models: models_state.clone(),
                meta: None,
            };

            let structured = serde_json::to_value(response_struct)
                .unwrap_or_else(|_| json!({ "sessionId": session_id_str }));

            let response = CallToolResult {
                content: vec![],
                is_error: None,
                structured_content: Some(structured),
            };

            outgoing.send_response(request_id, response).await;
        });
    }

    async fn acp_new_session_cfg(
        &self,
        arguments: Option<serde_json::Value>,
    ) -> anyhow::Result<Config> {
        let arguments = arguments.context("Arguments required")?;
        let arguments = serde_json::from_value::<AcpNewSessionToolArgs>(arguments)?;
        let request = serde_json::from_value::<acp::NewSessionRequest>(arguments.request)?;
        self.build_new_session_config(request, None).await // TODO: client_tools parameter removed
    }

    async fn handle_session_new(&self, request_id: RequestId, params: AcpNewSessionToolArgs) {
        let config = match self.session_new_config(params).await {
            Ok(cfg) => cfg,
            Err(err) => {
                tracing::warn!("Failed to prepare session config: {err}");
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("failed to prepare session config: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let outgoing = self.outgoing.clone();
        let session_map = self.session_map.clone();
        let conversation_manager = self.conversation_manager.clone();
        let models_state = session_models_from_config(&config);

        task::spawn(async move {
            let Some(session_id) = crate::acp_tool_runner::new_session(
                request_id.clone(),
                config,
                outgoing.clone(),
                session_map,
                conversation_manager,
            )
            .await
            else {
                return;
            };

            let response = acp::NewSessionResponse {
                session_id: acp::SessionId(Arc::from(session_id.to_string())),
                modes: Some(default_session_modes()),
                models: models_state.clone(),
                meta: None,
            };

            let value = serde_json::to_value(response)
                .unwrap_or_else(|_| json!({ "sessionId": session_id.to_string() }));

            outgoing.send_response(request_id, value).await;
        });
    }

    async fn session_new_config(&self, params: AcpNewSessionToolArgs) -> anyhow::Result<Config> {
        let request = serde_json::from_value::<acp::NewSessionRequest>(params.request)?;
        self.build_new_session_config(request, None).await // TODO: client_tools parameter removed
    }

    async fn build_new_session_config(
        &self,
        request: acp::NewSessionRequest,
        _override_tools: Option<()>, // TODO: ClientTools type removed
    ) -> anyhow::Result<Config> {
        let _mcp_servers = convert_mcp_servers(request.mcp_servers)?;
        // TODO: experimental_client_tools field removed from Config
        // let client_tools = override_tools
        //     .or_else(|| self.base_config.experimental_client_tools.clone());

        let overrides = codex_core::config::ConfigOverrides {
            cwd: Some(request.cwd),
            // TODO: mcp_servers and experimental_client_tools fields removed from ConfigOverrides
            // mcp_servers: Some(mcp_servers),
            // experimental_client_tools: client_tools,
            ..Default::default()
        };

        Ok(Config::load_with_cli_overrides(Default::default(), overrides).await?)
    }

    async fn handle_session_prompt(&self, request_id: RequestId, params: AcpPromptToolArgs) {
        let acp_session_id = params.session_id;
        let session_uuid = match Uuid::parse_str(&acp_session_id.to_string()) {
            Ok(id) => id,
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("invalid session id: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let session_entry = {
            let map = self.session_map.lock().await;
            map.get(&session_uuid).cloned()
        };

        let Some(session_entry) = session_entry else {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: format!("unknown session id: {}", acp_session_id),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        };

        let session = session_entry.conversation.clone();

        let outgoing = self.outgoing.clone();
        let requests_codex_map = self.running_requests_id_to_codex_uuid.clone();
        let prompt_blocks = params.prompt;

        task::spawn(async move {
            requests_codex_map
                .lock()
                .await
                .insert(request_id.clone(), session_uuid);

            let result = crate::acp_tool_runner::prompt(
                acp_session_id.clone(),
                session,
                prompt_blocks,
                outgoing.clone(),
            )
            .await;

            match result {
                Ok(stop_reason) => {
                    let response = acp::PromptResponse {
                        stop_reason,
                        meta: None,
                    };
                    let value = serde_json::to_value(response)
                        .unwrap_or_else(|_| json!({ "stopReason": "end_turn" }));
                    outgoing.send_response(request_id.clone(), value).await;
                }
                Err(err) => {
                    let error = JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: err.to_string(),
                        data: None,
                    };
                    outgoing.send_error(request_id.clone(), error).await;
                }
            }

            requests_codex_map.lock().await.remove(&request_id);
        });
    }

    async fn handle_session_set_model(
        &self,
        request_id: RequestId,
        params: acp::SetSessionModelRequest,
    ) {
        let session_string = params.session_id.to_string();
        let session_uuid = match Uuid::parse_str(&session_string) {
            Ok(id) => id,
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("invalid session id: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        match self
            .update_session_model_meta(session_uuid, params.session_id, params.model_id)
            .await
        {
            Ok(meta) => {
                let response = acp::SetSessionModelResponse { meta };
                self.outgoing.send_response(request_id, response).await;
            }
            Err(err) => {
                self.outgoing.send_error(request_id, err).await;
            }
        }
    }

    async fn update_session_model_meta(
        &self,
        session_uuid: Uuid,
        session_id: acp::SessionId,
        model_id: acp::ModelId,
    ) -> Result<Option<serde_json::Value>, JSONRPCErrorError> {
        let entry = {
            let map = self.session_map.lock().await;
            map.get(&session_uuid).cloned()
        };

        let Some(entry) = entry else {
            return Err(JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: format!("unknown session id: {}", session_id),
                data: None,
            });
        };

        let mut config_guard = entry.config.lock().await;
        let selection = match resolve_model_selection(&model_id, &config_guard) {
            Some(selection) => selection,
            None => {
                let message = format!("unknown model id: {}", model_id.to_string());
                return Err(JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message,
                    data: None,
                });
            }
        };

        let changed = apply_model_selection(&mut config_guard, &selection.model, selection.effort);
        let models_state = session_models_from_config(&config_guard);
        let models_meta_value = models_state
            .as_ref()
            .and_then(|state| serde_json::to_value(state).ok());
        let response_meta = models_meta_value
            .clone()
            .map(|value| json!({ "models": value }));
        let configure_op = if changed {
            Some(configure_session_op_from_config(&config_guard))
        } else {
            None
        };
        let model_name = selection.model.clone();

        drop(config_guard);

        if let Some(op) = configure_op {
            if let Err(err) = entry.conversation.submit(op).await {
                return Err(JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: err.to_string(),
                    data: None,
                });
            }
        }

        if let Some(models_meta) = models_meta_value {
            let notification = acp::SessionNotification {
                session_id: session_id.clone(),
                update: acp::SessionUpdate::AgentMessageChunk {
                    content: acp::ContentBlock::from(format!("Model set to {model_name}")),
                },
                meta: Some(json!({ "models": models_meta })),
            };

            if let Ok(params_value) = serde_json::to_value(notification) {
                self.outgoing
                    .send_notification(OutgoingNotification {
                        method: acp::CLIENT_METHOD_NAMES.session_update.to_string(),
                        params: Some(params_value),
                    })
                    .await;
            }
        }

        Ok(response_meta)
    }

    async fn handle_tool_call_acp_prompt(
        &self,
        request_id: RequestId,
        arguments: Option<serde_json::Value>,
    ) {
        let (session_id, acp_session_id, prompt) = match Self::acp_prompt_arguments(arguments) {
            Ok(cfg) => cfg,
            Err(err) => {
                tracing::warn!("Failed to parse arguments: {}", err);
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_owned(),
                        text: format!("Failed to parse arguments: {err}"),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    structured_content: None,
                };
                self.outgoing.send_response(request_id, result).await;
                return;
            }
        };

        let session = {
            let map = self.session_map.lock().await;
            map.get(&session_id).map(|entry| entry.conversation.clone())
        };

        let Some(session) = session else {
            tracing::warn!("Unknown session id: {}", session_id);
            let result = CallToolResult {
                content: vec![ContentBlock::TextContent(TextContent {
                    r#type: "text".to_owned(),
                    text: format!("Unknown session id: {session_id}"),
                    annotations: None,
                })],
                is_error: Some(true),
                structured_content: None,
            };
            self.outgoing.send_response(request_id, result).await;
            return;
        };

        let outgoing = self.outgoing.clone();
        let requests_codex_map = self.running_requests_id_to_codex_uuid.clone();

        task::spawn(async move {
            requests_codex_map
                .lock()
                .await
                .insert(request_id.clone(), session_id);

            let result =
                crate::acp_tool_runner::prompt(acp_session_id, session, prompt, outgoing.clone())
                    .await;

            let result = match result {
                Ok(stop_reason) => {
                    let structured = serde_json::to_value(acp::PromptResponse {
                        stop_reason,
                        meta: None,
                    })
                    .unwrap_or_else(|_| json!({ "stopReason": "end_turn" }));

                    CallToolResult {
                        content: vec![],
                        is_error: Some(false),
                        structured_content: Some(structured),
                    }
                }
                Err(err) => CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        annotations: None,
                        text: err.to_string(),
                        r#type: "text".to_owned(),
                    })],
                    is_error: Some(true),
                    structured_content: None,
                },
            };

            outgoing.send_response(request_id.clone(), result).await;

            requests_codex_map.lock().await.remove(&request_id);
        });
    }

    async fn handle_tool_call_acp_set_model(
        &self,
        request_id: RequestId,
        arguments: Option<serde_json::Value>,
    ) {
        let args = match arguments.context("arguments required").and_then(|args| {
            serde_json::from_value::<AcpSetModelToolArgs>(args).context("invalid arguments")
        }) {
            Ok(args) => args,
            Err(err) => {
                tracing::warn!("Failed to parse arguments: {err}");
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_owned(),
                        text: format!("Failed to parse arguments: {err}"),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    structured_content: None,
                };
                self.outgoing.send_response(request_id, result).await;
                return;
            }
        };

        let session_uuid = match Uuid::parse_str(&args.session_id.to_string()) {
            Ok(id) => id,
            Err(err) => {
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_owned(),
                        text: format!("Invalid session id: {err}"),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    structured_content: None,
                };
                self.outgoing.send_response(request_id, result).await;
                return;
            }
        };

        match self
            .update_session_model_meta(session_uuid, args.session_id.clone(), args.model_id.clone())
            .await
        {
            Ok(meta) => {
                let structured = serde_json::to_value(acp::SetSessionModelResponse { meta })
                    .unwrap_or_else(|_| json!({}));
                let result = CallToolResult {
                    content: vec![],
                    is_error: Some(false),
                    structured_content: Some(structured),
                };
                self.outgoing.send_response(request_id, result).await;
            }
            Err(err) => {
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_owned(),
                        text: err.message.clone(),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    structured_content: None,
                };
                self.outgoing.send_response(request_id, result).await;
            }
        }
    }

    fn acp_prompt_arguments(
        arguments: Option<serde_json::Value>,
    ) -> anyhow::Result<(Uuid, acp::SessionId, Vec<acp::ContentBlock>)> {
        let arguments = arguments.context("Arguments required")?;
        let arguments = serde_json::from_value::<AcpPromptToolArgs>(arguments)?;

        let session_uuid = Uuid::parse_str(&arguments.session_id.to_string())?;
        Ok((session_uuid, arguments.session_id, arguments.prompt))
    }

    fn handle_resource_updated(
        &self,
        params: <mcp_types::ResourceUpdatedNotification as mcp_types::ModelContextProtocolNotification>::Params,
    ) {
        tracing::info!("notifications/resources/updated -> params: {:?}", params);
    }

    fn handle_prompt_list_changed(
        &self,
        params: <mcp_types::PromptListChangedNotification as mcp_types::ModelContextProtocolNotification>::Params,
    ) {
        tracing::info!("notifications/prompts/list_changed -> params: {:?}", params);
    }

    async fn handle_session_cancel(&self, params: acp::CancelNotification) {
        let session_uuid = match Uuid::parse_str(&params.session_id.to_string()) {
            Ok(uuid) => uuid,
            Err(err) => {
                tracing::warn!("received session/cancel with invalid session id: {err}");
                return;
            }
        };

        let conversation = {
            let map = self.session_map.lock().await;
            map.get(&session_uuid)
                .map(|entry| entry.conversation.clone())
        };

        let Some(conversation) = conversation else {
            tracing::warn!("session/cancel for unknown session: {}", params.session_id);
            return;
        };

        let request_ids: Vec<RequestId> = {
            let map = self.running_requests_id_to_codex_uuid.lock().await;
            map.iter()
                .filter_map(|(request_id, uuid)| {
                    if *uuid == session_uuid {
                        Some(request_id.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        if request_ids.is_empty() {
            if let Err(err) = conversation
                .submit_with_id(Submission {
                    id: Uuid::new_v4().to_string(),
                    op: Op::Interrupt,
                })
                .await
            {
                tracing::error!("failed to interrupt session {}: {err}", params.session_id);
            }
            return;
        }

        for request_id in request_ids {
            let submission_id = request_id_to_string(&request_id);
            if let Err(err) = conversation
                .submit_with_id(Submission {
                    id: submission_id,
                    op: Op::Interrupt,
                })
                .await
            {
                tracing::error!("failed to interrupt in-flight request: {err}");
            }
        }
    }

    fn handle_tool_list_changed(
        &self,
        params: <mcp_types::ToolListChangedNotification as mcp_types::ModelContextProtocolNotification>::Params,
    ) {
        tracing::info!("notifications/tools/list_changed -> params: {:?}", params);
    }

    fn handle_logging_message(
        &self,
        params: <mcp_types::LoggingMessageNotification as mcp_types::ModelContextProtocolNotification>::Params,
    ) {
        tracing::info!("notifications/message -> params: {:?}", params);
    }
}

fn convert_mcp_servers(
    servers: Vec<acp::McpServer>,
) -> anyhow::Result<HashMap<String, McpServerConfig>> {
    let mut map = HashMap::with_capacity(servers.len());
    for server in servers {
        match server {
            acp::McpServer::Stdio {
                name,
                command,
                args,
                env,
            } => {
                let env_map: HashMap<String, String> =
                    env.into_iter().map(|var| (var.name, var.value)).collect();
                let env_map = if env_map.is_empty() {
                    None
                } else {
                    Some(env_map)
                };

                map.insert(
                    name,
                    McpServerConfig {
                        transport: McpServerTransportConfig::Stdio {
                            command: command.display().to_string(),
                            args,
                            env: env_map,
                        },
                        startup_timeout_sec: None,
                        tool_timeout_sec: None,
                    },
                );
            }
            acp::McpServer::Http { name, .. } => {
                return Err(anyhow!(
                    "unsupported MCP transport for server '{}': HTTP servers are not yet supported",
                    name
                ));
            }
            acp::McpServer::Sse { name, .. } => {
                return Err(anyhow!(
                    "unsupported MCP transport for server '{}': SSE servers are not yet supported",
                    name
                ));
            }
        }
    }

    Ok(map)
}

#[derive(Clone)]
struct ModelSelection {
    model: String,
    effort: ReasoningEffort,
}

fn session_models_from_config(config: &Config) -> Option<acp::SessionModelState> {
    let presets = builtin_model_presets(None);
    let mut available_models = Vec::new();
    let mut current_model_id: Option<acp::ModelId> = None;

    for preset in presets.iter() {
        let id = acp::ModelId(Arc::from(preset.id));
        let description = if preset.description.is_empty() {
            None
        } else {
            Some(preset.description.to_string())
        };
        available_models.push(acp::ModelInfo {
            model_id: id.clone(),
            name: preset.label.to_string(),
            description,
            meta: None,
        });

        if preset.model.eq_ignore_ascii_case(&config.model) {
            let preset_effort = preset_effort(preset);
            if current_model_id.is_none() || Some(preset_effort) == config.model_reasoning_effort {
                current_model_id = Some(id.clone());
            }
        }
    }

    if available_models.is_empty() {
        let model_lower = config.model.to_ascii_lowercase();
        let id = acp::ModelId(Arc::from(format!("custom::{model_lower}")));
        available_models.push(acp::ModelInfo {
            model_id: id.clone(),
            name: config.model.clone(),
            description: Some("Configured via CODEX_HOME/config.toml".to_string()),
            meta: None,
        });
        return Some(acp::SessionModelState {
            current_model_id: id,
            available_models,
            meta: None,
        });
    }

    if current_model_id.is_none() {
        let model_lower = config.model.to_ascii_lowercase();
        let id = acp::ModelId(Arc::from(format!("custom::{model_lower}")));
        available_models.push(acp::ModelInfo {
            model_id: id.clone(),
            name: config.model.clone(),
            description: Some("Configured via CODEX_HOME/config.toml".to_string()),
            meta: None,
        });
        current_model_id = Some(id);
    }

    Some(acp::SessionModelState {
        current_model_id: current_model_id.expect("current model id set"),
        available_models,
        meta: None,
    })
}

fn preset_effort(preset: &ModelPreset) -> ReasoningEffort {
    preset
        .effort
        .map(ReasoningEffort::from)
        .unwrap_or(ReasoningEffort::Medium)
}

fn resolve_model_selection(model_id: &acp::ModelId, config: &Config) -> Option<ModelSelection> {
    let requested = model_id.to_string();
    let requested_lower = requested.to_ascii_lowercase();

    for preset in builtin_model_presets(None).iter() {
        if preset.id.eq_ignore_ascii_case(&requested)
            || preset.label.eq_ignore_ascii_case(&requested)
            || preset.model.eq_ignore_ascii_case(&requested)
        {
            return Some(ModelSelection {
                model: preset.model.to_string(),
                effort: preset_effort(preset),
            });
        }
    }

    if let Some(stripped) = requested_lower.strip_prefix("custom::") {
        let effort = if stripped.eq_ignore_ascii_case(&config.model) {
            config
                .model_reasoning_effort
                .unwrap_or(ReasoningEffort::Medium)
        } else {
            ReasoningEffort::Medium
        };
        return Some(ModelSelection {
            model: stripped.to_string(),
            effort,
        });
    }

    if requested_lower == config.model.to_ascii_lowercase() {
        return Some(ModelSelection {
            model: config.model.clone(),
            effort: config
                .model_reasoning_effort
                .unwrap_or(ReasoningEffort::Medium),
        });
    }

    None
}

fn apply_model_selection(config: &mut Config, model: &str, effort: ReasoningEffort) -> bool {
    let mut updated = false;
    if !config.model.eq_ignore_ascii_case(model) {
        config.model = model.to_string();
        config.model_family = find_family_for_model(&config.model)
            .unwrap_or_else(|| derive_default_model_family(&config.model));
        updated = true;
    }

    if config.model_reasoning_effort != Some(effort) {
        config.model_reasoning_effort = Some(effort);
        updated = true;
    }

    updated
}

// TODO: Op::ConfigureSession variant removed
#[allow(dead_code)]
fn configure_session_op_from_config(_config: &Config) -> Op {
    // Op::ConfigureSession variant no longer exists
    // Fields like model_text_verbosity and disable_response_storage also removed from Config
    unimplemented!("Op::ConfigureSession variant removed")
    // Op::ConfigureSession {
    //     provider: config.model_provider.clone(),
    //     model: config.model.clone(),
    //     model_reasoning_effort: config.model_reasoning_effort,
    //     model_reasoning_summary: config.model_reasoning_summary,
    //     model_text_verbosity: config.model_text_verbosity, // REMOVED
    //     user_instructions: config.user_instructions.clone(),
    //     base_instructions: config.base_instructions.clone(),
    //     approval_policy: config.approval_policy.clone(),
    //     sandbox_policy: config.sandbox_policy.clone(),
    //     disable_response_storage: config.disable_response_storage, // REMOVED
    //     notify: config.notify.clone(),
    //     cwd: config.cwd.clone(),
    //     resume_path: None,
    // }
}

fn default_session_modes() -> acp::SessionModeState {
    let mode_id = acp::SessionModeId(Arc::from("default".to_string()));
    let mode = acp::SessionMode {
        id: mode_id.clone(),
        name: "Default".to_string(),
        description: Some("Code prompts before executing tools or applying patches.".to_string()),
        meta: None,
    };

    acp::SessionModeState {
        current_mode_id: mode_id,
        available_modes: vec![mode],
        meta: None,
    }
}

fn request_id_to_string(request_id: &RequestId) -> String {
    match request_id {
        RequestId::String(value) => value.clone(),
        RequestId::Integer(value) => value.to_string(),
    }
}
