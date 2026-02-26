use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use crate::code_message_processor::CodexMessageProcessor;
use crate::external_agent_config_api::ExternalAgentConfigApi;
use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::OutgoingMessageSender;
use code_app_server_protocol::AuthMode;
use code_app_server_protocol::ConfigRequirements;
use code_app_server_protocol::CancelLoginAccountParams;
use code_app_server_protocol::Config as V2Config;
use code_app_server_protocol::ConfigBatchWriteParams;
use code_app_server_protocol::ConfigEdit;
use code_app_server_protocol::ConfigReadParams;
use code_app_server_protocol::ConfigReadResponse;
use code_app_server_protocol::ConfigRequirementsReadResponse;
use code_app_server_protocol::ConfigValueWriteParams;
use code_app_server_protocol::ConfigWriteErrorCode;
use code_app_server_protocol::ConfigWriteResponse;
use code_app_server_protocol::ExternalAgentConfigDetectParams;
use code_app_server_protocol::ExternalAgentConfigImportParams;
use code_app_server_protocol::GetAccountParams;
use code_app_server_protocol::LoginAccountParams;
use code_app_server_protocol::MergeStrategy;
use code_app_server_protocol::ToolsV2;
use code_app_server_protocol::AskForApproval as V2AskForApproval;
use code_app_server_protocol::WriteStatus;
use code_protocol::config_types::Verbosity;
use code_protocol::config_types::WebSearchMode;
use code_core::AuthManager;
use code_core::ConversationManager;
use code_core::config::Config;
use code_core::default_client::get_code_user_agent_with_suffix;
use code_protocol::mcp_protocol::ClientRequest;
use code_protocol::mcp_protocol::ClientRequest::Initialize;
use code_protocol::mcp_protocol::GetUserAgentResponse;
use code_protocol::mcp_protocol::InitializeResponse;
use code_protocol::protocol::SessionSource;
use mcp_types::JSONRPCError;
use mcp_types::JSONRPCErrorError;
use mcp_types::JSONRPCNotification;
use mcp_types::JSONRPCRequest;
use mcp_types::JSONRPCResponse;
use code_utils_absolute_path::AbsolutePathBuf;
use code_utils_json_to_toml::json_to_toml;
use serde_json::json;
use sha1::Digest;
use sha1::Sha1;
use toml::Value as TomlValue;

pub(crate) struct MessageProcessor {
    outgoing: Arc<OutgoingMessageSender>,
    code_message_processor: CodexMessageProcessor,
    external_agent_config_api: ExternalAgentConfigApi,
    base_config: Arc<Config>,
    config_warnings: Arc<Vec<serde_json::Value>>,
    cli_overrides: Vec<(String, TomlValue)>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ConnectionSessionState {
    pub(crate) initialized: bool,
    pub(crate) user_agent_suffix: Option<String>,
    pub(crate) opted_out_notification_methods: HashSet<String>,
}

impl MessageProcessor {
    /// Create a new `MessageProcessor`, retaining a handle to the outgoing
    /// `Sender` so handlers can enqueue messages to be written to the
    /// transport.
    pub(crate) fn new(
        outgoing: Arc<OutgoingMessageSender>,
        code_linux_sandbox_exe: Option<PathBuf>,
        config: Arc<Config>,
        config_warnings: Vec<serde_json::Value>,
        cli_overrides: Vec<(String, TomlValue)>,
    ) -> Self {
        let auth_manager = AuthManager::shared_with_mode_and_originator(
            config.code_home.clone(),
            AuthMode::ApiKey,
            config.responses_originator_header.clone(),
        );
        let conversation_manager = Arc::new(ConversationManager::new(
            auth_manager.clone(),
            SessionSource::Mcp,
        ));
        let config_for_processor = config.clone();
        let code_message_processor = CodexMessageProcessor::new(
            auth_manager,
            conversation_manager,
            outgoing.clone(),
            code_linux_sandbox_exe,
            config_for_processor.clone(),
        );
        let external_agent_config_api =
            ExternalAgentConfigApi::new(config.code_home.clone());

        Self {
            outgoing,
            code_message_processor,
            external_agent_config_api,
            base_config: config_for_processor,
            config_warnings: Arc::new(config_warnings),
            cli_overrides,
        }
    }

    pub(crate) async fn process_request(
        &mut self,
        connection_id: ConnectionId,
        request: JSONRPCRequest,
        session: &mut ConnectionSessionState,
        outbound_initialized: &AtomicBool,
        outbound_opted_out_notification_methods: &RwLock<HashSet<String>>,
    ) {
        let request_id = request.id.clone();

        if self
            .try_process_v2_config_request(request_id.clone(), &request, session.initialized)
            .await
        {
            return;
        }

        if let Ok(request_json) = serde_json::to_value(request)
            && let Ok(code_request) = serde_json::from_value::<ClientRequest>(request_json)
        {
            match code_request {
                // Handle Initialize internally so CodexMessageProcessor does not have to concern
                // itself with per-connection initialization state.
                Initialize { request_id, params } => {
                    if session.initialized {
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: "Already initialized".to_string(),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                        return;
                    }

                    let client_info = params.client_info;
                    let opted_out_notification_methods = params
                        .capabilities
                        .and_then(|capabilities| capabilities.opt_out_notification_methods)
                        .unwrap_or_default();
                    session.opted_out_notification_methods =
                        opted_out_notification_methods.into_iter().collect();
                    session.user_agent_suffix = Some(format!("{}; {}", client_info.name, client_info.version));

                    if let Ok(mut methods) = outbound_opted_out_notification_methods.write() {
                        *methods = session.opted_out_notification_methods.clone();
                    }

                    let user_agent = get_code_user_agent_with_suffix(
                        Some(&self.base_config.responses_originator_header),
                        session.user_agent_suffix.as_deref(),
                    );
                    let response = InitializeResponse { user_agent };
                    self.outgoing.send_response(request_id, response).await;

                    session.initialized = true;
                    outbound_initialized.store(true, Ordering::Release);
                    return;
                }
                ClientRequest::GetUserAgent { request_id, .. } => {
                    if !session.initialized {
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: "Not initialized".to_string(),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                        return;
                    }

                    let response = GetUserAgentResponse {
                        user_agent: get_code_user_agent_with_suffix(
                            Some(&self.base_config.responses_originator_header),
                            session.user_agent_suffix.as_deref(),
                        ),
                    };
                    self.outgoing.send_response(request_id, response).await;
                    return;
                }
                _ => {
                    if !session.initialized {
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: "Not initialized".to_string(),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                        return;
                    }
                }
            }

            self.code_message_processor
                .process_request_for_connection(connection_id, code_request)
                .await;
        } else {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "Invalid request".to_string(),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
        }
    }

    pub(crate) async fn process_notification(&self, notification: JSONRPCNotification) {
        // Currently, we do not expect to receive any notifications from the
        // client, so we just log them.
        tracing::info!("<- notification: {:?}", notification);
    }

    pub(crate) async fn send_initialize_notifications(&self, connection_id: ConnectionId) {
        for params in self.config_warnings.iter().cloned() {
            self.outgoing
                .send_notification_to_connection(
                    connection_id,
                    crate::outgoing_message::OutgoingNotification {
                        method: "configWarning".to_string(),
                        params: Some(params),
                    },
                )
                .await;
        }
    }

    pub(crate) async fn on_connection_closed(&mut self, connection_id: ConnectionId) {
        self.code_message_processor
            .on_connection_closed(connection_id)
            .await;
    }

    /// Handle a standalone JSON-RPC response originating from the peer.
    pub(crate) async fn process_response(
        &mut self,
        connection_id: ConnectionId,
        response: JSONRPCResponse,
    ) {
        tracing::info!("<- response: {:?}", response);
        let JSONRPCResponse { id, result, .. } = response;
        self.outgoing
            .notify_client_response_for_connection(Some(connection_id), id, result)
            .await
    }

    /// Handle an error object received from the peer.
    pub(crate) async fn process_error(&mut self, connection_id: ConnectionId, err: JSONRPCError) {
        tracing::error!("<- error: {:?}", err);
        self.outgoing
            .notify_client_error_for_connection(Some(connection_id), err.id, err.error)
            .await;
    }

    async fn try_process_v2_config_request(
        &self,
        request_id: mcp_types::RequestId,
        request: &JSONRPCRequest,
        session_initialized: bool,
    ) -> bool {
        let is_v2_request = matches!(
            request.method.as_str(),
            "config/read"
                | "configRequirements/read"
                | "config/value/write"
                | "config/batchWrite"
                | "externalAgentConfig/detect"
                | "externalAgentConfig/import"
                | "account/read"
                | "account/login/start"
                | "account/login/cancel"
                | "account/logout"
                | "account/rateLimits/read"
        );
        if is_v2_request && !session_initialized {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "Not initialized".to_string(),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return true;
        }

        match request.method.as_str() {
            "config/read" => {
                let params_value = request.params.clone().unwrap_or_else(|| json!({}));
                let params: ConfigReadParams = match serde_json::from_value(params_value) {
                    Ok(params) => params,
                    Err(err) => {
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("Invalid config/read params: {err}"),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                        return true;
                    }
                };

                let config = match self.load_effective_config(params.cwd.as_deref()) {
                    Ok(config) => config,
                    Err(error) => {
                        self.outgoing.send_error(request_id, error).await;
                        return true;
                    }
                };

                let response = ConfigReadResponse {
                    config: self.v2_config_snapshot_from(&config),
                    origins: HashMap::new(),
                    layers: if params.include_layers {
                        Some(Vec::new())
                    } else {
                        None
                    },
                };
                self.outgoing.send_response(request_id, response).await;
                true
            }
            "configRequirements/read" => {
                let requirements = match code_core::config::load_allowed_approval_policies(
                    &self.base_config.code_home,
                ) {
                    Ok(Some(allowed_approval_policies)) => Some(ConfigRequirements {
                        allowed_approval_policies: Some(
                            allowed_approval_policies
                                .into_iter()
                                .map(map_approval_policy_to_v2)
                                .collect(),
                        ),
                        allowed_sandbox_modes: None,
                        allowed_web_search_modes: None,
                        enforce_residency: None,
                        network: None,
                    }),
                    Ok(None) => None,
                    Err(err) => {
                        let error = JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("Unable to read config requirements: {err}"),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                        return true;
                    }
                };

                let response = ConfigRequirementsReadResponse { requirements };
                self.outgoing.send_response(request_id, response).await;
                true
            }
            "config/value/write" => {
                let params_value = request.params.clone().unwrap_or_else(|| json!({}));
                let params: ConfigValueWriteParams = match serde_json::from_value(params_value) {
                    Ok(params) => params,
                    Err(err) => {
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("Invalid config/value/write params: {err}"),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                        return true;
                    }
                };

                match self.apply_config_value_write(params) {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(error) => self.outgoing.send_error(request_id, error).await,
                }
                true
            }
            "config/batchWrite" => {
                let params_value = request.params.clone().unwrap_or_else(|| json!({}));
                let params: ConfigBatchWriteParams = match serde_json::from_value(params_value) {
                    Ok(params) => params,
                    Err(err) => {
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("Invalid config/batchWrite params: {err}"),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                        return true;
                    }
                };

                match self.apply_config_batch_write(params) {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(error) => self.outgoing.send_error(request_id, error).await,
                }
                true
            }
            "externalAgentConfig/detect" => {
                let params_value = request.params.clone().unwrap_or_else(|| json!({}));
                let params: ExternalAgentConfigDetectParams =
                    match serde_json::from_value(params_value) {
                        Ok(params) => params,
                        Err(err) => {
                            let error = JSONRPCErrorError {
                                code: INVALID_REQUEST_ERROR_CODE,
                                message: format!(
                                    "Invalid externalAgentConfig/detect params: {err}"
                                ),
                                data: None,
                            };
                            self.outgoing.send_error(request_id, error).await;
                            return true;
                        }
                    };

                match self.external_agent_config_api.detect(params).await {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(error) => self.outgoing.send_error(request_id, error).await,
                }
                true
            }
            "externalAgentConfig/import" => {
                let params_value = request.params.clone().unwrap_or_else(|| json!({}));
                let params: ExternalAgentConfigImportParams =
                    match serde_json::from_value(params_value) {
                        Ok(params) => params,
                        Err(err) => {
                            let error = JSONRPCErrorError {
                                code: INVALID_REQUEST_ERROR_CODE,
                                message: format!(
                                    "Invalid externalAgentConfig/import params: {err}"
                                ),
                                data: None,
                            };
                            self.outgoing.send_error(request_id, error).await;
                            return true;
                        }
                    };

                match self.external_agent_config_api.import(params).await {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(error) => self.outgoing.send_error(request_id, error).await,
                }
                true
            }
            "account/read" => {
                let params_value = request.params.clone().unwrap_or_else(|| json!({}));
                let params: GetAccountParams = match serde_json::from_value(params_value) {
                    Ok(params) => params,
                    Err(err) => {
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("Invalid account/read params: {err}"),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                        return true;
                    }
                };

                match self
                    .code_message_processor
                    .get_account_response_v2(params.refresh_token)
                    .await
                {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(error) => self.outgoing.send_error(request_id, error).await,
                }
                true
            }
            "account/login/start" => {
                let params_value = request.params.clone().unwrap_or_else(|| json!({}));
                let params: LoginAccountParams = match serde_json::from_value(params_value) {
                    Ok(params) => params,
                    Err(err) => {
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("Invalid account/login/start params: {err}"),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                        return true;
                    }
                };

                match self.code_message_processor.login_account_v2(params).await {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(error) => self.outgoing.send_error(request_id, error).await,
                }
                true
            }
            "account/login/cancel" => {
                let params_value = request.params.clone().unwrap_or_else(|| json!({}));
                let params: CancelLoginAccountParams = match serde_json::from_value(params_value)
                {
                    Ok(params) => params,
                    Err(err) => {
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("Invalid account/login/cancel params: {err}"),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                        return true;
                    }
                };

                match self.code_message_processor.cancel_login_account_v2(params).await {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(error) => self.outgoing.send_error(request_id, error).await,
                }
                true
            }
            "account/logout" => {
                match self.code_message_processor.logout_account_v2().await {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(error) => self.outgoing.send_error(request_id, error).await,
                }
                true
            }
            "account/rateLimits/read" => {
                match self.code_message_processor.get_account_rate_limits_v2() {
                    Ok(response) => self.outgoing.send_response(request_id, response).await,
                    Err(error) => self.outgoing.send_error(request_id, error).await,
                }
                true
            }
            _ => false,
        }
    }

    fn load_effective_config(&self, cwd: Option<&str>) -> Result<Config, JSONRPCErrorError> {
        let mut overrides = code_core::config::ConfigOverrides::default();
        overrides.code_linux_sandbox_exe = self.base_config.code_linux_sandbox_exe.clone();
        overrides.cwd = cwd.map(PathBuf::from);

        Config::load_with_cli_overrides(self.cli_overrides.clone(), overrides).map_err(|err| {
            JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("Unable to load effective config: {err}"),
                data: None,
            }
        })
    }

    fn v2_config_snapshot_from(&self, config: &Config) -> V2Config {
        V2Config {
            model: Some(config.model.clone()),
            review_model: Some(config.review_model.clone()),
            model_context_window: config
                .model_context_window
                .and_then(|value| i64::try_from(value).ok()),
            model_auto_compact_token_limit: config.model_auto_compact_token_limit,
            model_provider: Some(config.model_provider_id.clone()),
            approval_policy: Some(match config.approval_policy {
                code_core::protocol::AskForApproval::UnlessTrusted => {
                    V2AskForApproval::UnlessTrusted
                }
                code_core::protocol::AskForApproval::OnFailure => V2AskForApproval::OnFailure,
                code_core::protocol::AskForApproval::OnRequest => V2AskForApproval::OnRequest,
                code_core::protocol::AskForApproval::Reject(config) => V2AskForApproval::Reject {
                    sandbox_approval: config.sandbox_approval,
                    rules: config.rules,
                    mcp_elicitations: config.mcp_elicitations,
                },
                code_core::protocol::AskForApproval::Never => V2AskForApproval::Never,
            }),
            sandbox_mode: None,
            sandbox_workspace_write: None,
            forced_chatgpt_workspace_id: None,
            forced_login_method: None,
            web_search: Some(if config.tools_web_search_request {
                WebSearchMode::Cached
            } else {
                WebSearchMode::Disabled
            }),
            tools: Some(ToolsV2 {
                web_search: Some(config.tools_web_search_request),
                view_image: Some(config.include_view_image_tool),
            }),
            profile: config.active_profile.clone(),
            profiles: HashMap::new(),
            instructions: config.base_instructions.clone(),
            developer_instructions: None,
            compact_prompt: config.compact_prompt_override.clone(),
            model_reasoning_effort: None,
            model_reasoning_summary: None,
            model_verbosity: Some(match config.model_text_verbosity {
                code_core::config_types::TextVerbosity::Low => Verbosity::Low,
                code_core::config_types::TextVerbosity::Medium => Verbosity::Medium,
                code_core::config_types::TextVerbosity::High => Verbosity::High,
            }),
            analytics: None,
            apps: None,
            additional: HashMap::new(),
        }
    }

    fn apply_config_value_write(
        &self,
        params: ConfigValueWriteParams,
    ) -> Result<ConfigWriteResponse, JSONRPCErrorError> {
        let ConfigValueWriteParams {
            key_path,
            value,
            merge_strategy,
            file_path,
            expected_version,
        } = params;
        self.apply_config_edits(
            vec![ConfigEdit {
                key_path,
                value,
                merge_strategy,
            }],
            file_path,
            expected_version,
        )
    }

    fn apply_config_batch_write(
        &self,
        params: ConfigBatchWriteParams,
    ) -> Result<ConfigWriteResponse, JSONRPCErrorError> {
        self.apply_config_edits(params.edits, params.file_path, params.expected_version)
    }

    fn apply_config_edits(
        &self,
        edits: Vec<ConfigEdit>,
        file_path: Option<String>,
        expected_version: Option<String>,
    ) -> Result<ConfigWriteResponse, JSONRPCErrorError> {
        let allowed_file_path = self.base_config.code_home.join("config.toml");
        let file_path = self.resolve_config_file_path(file_path, &allowed_file_path)?;
        let current_contents = match std::fs::read_to_string(&file_path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                return Err(config_write_error(
                    ConfigWriteErrorCode::ConfigValidationError,
                    format!("Unable to read config file: {err}"),
                ));
            }
        };
        let current_version = config_version(&current_contents);
        if let Some(expected_version) = expected_version
            && expected_version != current_version
        {
            return Err(config_write_error(
                ConfigWriteErrorCode::ConfigVersionConflict,
                "Config version conflict",
            ));
        }

        let mut root = if current_contents.trim().is_empty() {
            TomlValue::Table(Default::default())
        } else {
            current_contents.parse::<TomlValue>().map_err(|err| {
                config_write_error(
                    ConfigWriteErrorCode::ConfigValidationError,
                    format!("Invalid TOML in config file: {err}"),
                )
            })?
        };

        for edit in edits {
            apply_toml_edit(
                &mut root,
                edit.key_path.as_str(),
                json_to_toml(edit.value),
                edit.merge_strategy,
            )?;
        }

        let serialized = toml::to_string_pretty(&root).map_err(|err| {
            config_write_error(
                ConfigWriteErrorCode::ConfigValidationError,
                format!("Unable to serialize config TOML: {err}"),
            )
        })?;

        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                config_write_error(
                    ConfigWriteErrorCode::UserLayerNotFound,
                    format!("Unable to create config directory: {err}"),
                )
            })?;
        }

        std::fs::write(&file_path, serialized.as_bytes()).map_err(|err| {
            config_write_error(
                ConfigWriteErrorCode::ConfigValidationError,
                format!("Unable to write config file: {err}"),
            )
        })?;

        let absolute_file_path = to_absolute_path_buf(&file_path).map_err(|err| {
            config_write_error(
                ConfigWriteErrorCode::ConfigValidationError,
                format!("Unable to resolve config file path: {err}"),
            )
        })?;

        Ok(ConfigWriteResponse {
            status: WriteStatus::Ok,
            version: config_version(serialized.as_str()),
            file_path: absolute_file_path,
            overridden_metadata: None,
        })
    }

    fn resolve_config_file_path(
        &self,
        file_path: Option<String>,
        allowed_file_path: &Path,
    ) -> Result<PathBuf, JSONRPCErrorError> {
        let path = match file_path {
            Some(path) => {
                let path = PathBuf::from(path);
                if !path.is_absolute() {
                    return Err(config_write_error(
                        ConfigWriteErrorCode::ConfigValidationError,
                        "filePath must be an absolute path",
                    ));
                }
                if !paths_match(allowed_file_path, &path) {
                    return Err(config_write_error(
                        ConfigWriteErrorCode::ConfigLayerReadonly,
                        "Only writes to the user config are allowed",
                    ));
                }
                path
            }
            None => allowed_file_path.to_path_buf(),
        };

        Ok(path)
    }
}

fn paths_match(expected: &Path, provided: &Path) -> bool {
    let expected = expected.canonicalize().unwrap_or_else(|_| expected.to_path_buf());
    let provided = provided.canonicalize().unwrap_or_else(|_| provided.to_path_buf());
    expected == provided
}

fn map_approval_policy_to_v2(
    policy: code_core::protocol::AskForApproval,
) -> V2AskForApproval {
    match policy {
        code_core::protocol::AskForApproval::UnlessTrusted => V2AskForApproval::UnlessTrusted,
        code_core::protocol::AskForApproval::OnFailure => V2AskForApproval::OnFailure,
        code_core::protocol::AskForApproval::OnRequest => V2AskForApproval::OnRequest,
        code_core::protocol::AskForApproval::Reject(config) => V2AskForApproval::Reject {
            sandbox_approval: config.sandbox_approval,
            rules: config.rules,
            mcp_elicitations: config.mcp_elicitations,
        },
        code_core::protocol::AskForApproval::Never => V2AskForApproval::Never,
    }
}

fn apply_toml_edit(
    root: &mut TomlValue,
    key_path: &str,
    value: TomlValue,
    merge_strategy: MergeStrategy,
) -> Result<(), JSONRPCErrorError> {
    match merge_strategy {
        MergeStrategy::Replace => set_toml_path(root, key_path, value),
        MergeStrategy::Upsert => upsert_toml_path(root, key_path, value),
    }
}

fn set_toml_path(root: &mut TomlValue, key_path: &str, value: TomlValue) -> Result<(), JSONRPCErrorError> {
    let segments: Vec<&str> = key_path.split('.').filter(|segment| !segment.is_empty()).collect();
    if segments.is_empty() {
        return Err(config_write_error(
            ConfigWriteErrorCode::ConfigPathNotFound,
            "Config key path must not be empty",
        ));
    }

    let mut current = root;
    for segment in &segments[..segments.len() - 1] {
        if !current.is_table() {
            *current = TomlValue::Table(Default::default());
        }
        let table = current
            .as_table_mut()
            .expect("table should exist after conversion");
        current = table
            .entry((*segment).to_string())
            .or_insert_with(|| TomlValue::Table(Default::default()));
    }

    if !current.is_table() {
        *current = TomlValue::Table(Default::default());
    }
    let table = current
        .as_table_mut()
        .expect("table should exist after conversion");
    table.insert(
        segments
            .last()
            .expect("segments cannot be empty")
            .to_string(),
        value,
    );

    Ok(())
}

fn upsert_toml_path(
    root: &mut TomlValue,
    key_path: &str,
    value: TomlValue,
) -> Result<(), JSONRPCErrorError> {
    let segments: Vec<&str> = key_path.split('.').filter(|segment| !segment.is_empty()).collect();
    if segments.is_empty() {
        return Err(config_write_error(
            ConfigWriteErrorCode::ConfigPathNotFound,
            "Config key path must not be empty",
        ));
    }

    let mut current = root;
    for segment in &segments[..segments.len() - 1] {
        if !current.is_table() {
            *current = TomlValue::Table(Default::default());
        }
        let table = current
            .as_table_mut()
            .expect("table should exist after conversion");
        current = table
            .entry((*segment).to_string())
            .or_insert_with(|| TomlValue::Table(Default::default()));
    }

    if !current.is_table() {
        *current = TomlValue::Table(Default::default());
    }

    let table = current
        .as_table_mut()
        .expect("table should exist after conversion");
    let key = segments
        .last()
        .expect("segments cannot be empty")
        .to_string();
    if let Some(existing) = table.get_mut(&key) {
        merge_toml_values(existing, value);
    } else {
        table.insert(key, value);
    }
    Ok(())
}

fn merge_toml_values(target: &mut TomlValue, incoming: TomlValue) {
    match (target, incoming) {
        (TomlValue::Table(target_table), TomlValue::Table(incoming_table)) => {
            for (key, incoming_value) in incoming_table {
                if let Some(existing) = target_table.get_mut(&key) {
                    merge_toml_values(existing, incoming_value);
                } else {
                    target_table.insert(key, incoming_value);
                }
            }
        }
        (target_value, incoming_value) => {
            *target_value = incoming_value;
        }
    }
}

fn config_version(contents: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(contents.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn to_absolute_path_buf(path: &Path) -> std::io::Result<AbsolutePathBuf> {
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    absolute_path
        .try_into()
        .map_err(std::io::Error::other)
}

fn config_write_error(code: ConfigWriteErrorCode, message: impl Into<String>) -> JSONRPCErrorError {
    JSONRPCErrorError {
        code: INVALID_REQUEST_ERROR_CODE,
        message: message.into(),
        data: Some(json!({
            "config_write_error_code": code,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outgoing_message::OutgoingEnvelope;
    use crate::outgoing_message::OutgoingMessage;
    use mcp_types::JSONRPC_VERSION;
    use mcp_types::RequestId;
    use serde_json::json;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    #[tokio::test]
    async fn initialize_applies_opt_out_notification_methods_per_connection() {
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<OutgoingEnvelope>(8);
        let outgoing = Arc::new(OutgoingMessageSender::new_with_routed_sender(outgoing_tx));
        let config = Arc::new(
            Config::load_with_cli_overrides(Vec::new(), code_core::config::ConfigOverrides::default())
                .expect("load default config"),
        );
        let mut processor = MessageProcessor::new(outgoing, None, config, Vec::new(), Vec::new());
        let mut session = ConnectionSessionState::default();
        let outbound_initialized = AtomicBool::new(false);
        let outbound_opted_out_notification_methods = RwLock::new(HashSet::new());

        let request = JSONRPCRequest {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: RequestId::Integer(1),
            method: "initialize".to_string(),
            params: Some(json!({
                "clientInfo": {
                    "name": "client-a",
                    "version": "1.0.0"
                },
                "capabilities": {
                    "experimentalApi": false,
                    "optOutNotificationMethods": ["configWarning", "codex/event/session_configured"]
                }
            })),
        };

        processor
            .process_request(
                ConnectionId(42),
                request,
                &mut session,
                &outbound_initialized,
                &outbound_opted_out_notification_methods,
            )
            .await;

        assert!(session.initialized, "session should be initialized");
        assert!(
            outbound_initialized.load(Ordering::Acquire),
            "outbound initialized flag should be set"
        );

        let opted_out = outbound_opted_out_notification_methods
            .read()
            .expect("read lock");
        assert!(opted_out.contains("configWarning"));
        assert!(opted_out.contains("codex/event/session_configured"));

        // Drain initialize response envelope to ensure processing completed.
        let envelope = outgoing_rx.recv().await.expect("initialize response envelope");
        match envelope {
            OutgoingEnvelope::Broadcast { .. } => {}
            _ => panic!("expected initialize response to be emitted"),
        }
    }

    #[tokio::test]
    async fn v2_requests_require_initialize() {
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<OutgoingEnvelope>(8);
        let outgoing = Arc::new(OutgoingMessageSender::new_with_routed_sender(outgoing_tx));
        let config = Arc::new(
            Config::load_with_cli_overrides(Vec::new(), code_core::config::ConfigOverrides::default())
                .expect("load default config"),
        );
        let mut processor = MessageProcessor::new(outgoing, None, config, Vec::new(), Vec::new());
        let mut session = ConnectionSessionState::default();
        let outbound_initialized = AtomicBool::new(false);
        let outbound_opted_out_notification_methods = RwLock::new(HashSet::new());

        let request = JSONRPCRequest {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: RequestId::Integer(7),
            method: "config/read".to_string(),
            params: Some(json!({
                "includeLayers": false,
            })),
        };

        processor
            .process_request(
                ConnectionId(42),
                request,
                &mut session,
                &outbound_initialized,
                &outbound_opted_out_notification_methods,
            )
            .await;

        let envelope = outgoing_rx
            .recv()
            .await
            .expect("expected not-initialized error");
        match envelope {
            OutgoingEnvelope::Broadcast {
                message: OutgoingMessage::Error(error),
            } => {
                assert_eq!(error.id, RequestId::Integer(7));
                assert_eq!(error.error.message, "Not initialized");
            }
            _ => panic!("expected broadcast error response"),
        }
    }

    #[test]
    fn config_write_rejects_unreadable_existing_path() {
        let (outgoing_tx, _outgoing_rx) = mpsc::channel::<OutgoingEnvelope>(8);
        let outgoing = Arc::new(OutgoingMessageSender::new_with_routed_sender(outgoing_tx));

        let mut config =
            Config::load_with_cli_overrides(Vec::new(), code_core::config::ConfigOverrides::default())
                .expect("load default config");
        let temp_code_home = std::env::temp_dir().join(format!(
            "code-app-server-message-processor-{}",
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&temp_code_home).expect("create temp code home");
        let config_toml_path = temp_code_home.join("config.toml");
        std::fs::create_dir_all(&config_toml_path).expect("create unreadable config path");
        config.code_home = temp_code_home.clone();

        let processor = MessageProcessor::new(
            outgoing,
            None,
            Arc::new(config),
            Vec::new(),
            Vec::new(),
        );
        let result = processor.apply_config_value_write(ConfigValueWriteParams {
            key_path: "model".to_string(),
            value: json!("o3"),
            merge_strategy: MergeStrategy::Replace,
            file_path: None,
            expected_version: None,
        });

        let err = result.expect_err("write should fail when reading config path fails");
        assert!(err.message.contains("Unable to read config file"));
        assert_eq!(
            err.data,
            Some(json!({
                "config_write_error_code": ConfigWriteErrorCode::ConfigValidationError,
            }))
        );

        let _ = std::fs::remove_dir_all(temp_code_home);
    }
}
