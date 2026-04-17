use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use code_app_server_protocol::Account as V2Account;
use code_app_server_protocol::CancelLoginAccountParams;
use code_app_server_protocol::CancelLoginAccountResponse;
use code_app_server_protocol::CancelLoginAccountStatus;
use code_app_server_protocol::GetAccountRateLimitsResponse;
use code_app_server_protocol::GetAccountResponse;
use code_app_server_protocol::LoginAccountParams;
use code_app_server_protocol::LoginAccountResponse;
use code_app_server_protocol::LogoutAccountResponse;
use code_app_server_protocol::Model as V2Model;
use code_app_server_protocol::ModelListParams;
use code_app_server_protocol::ModelListResponse;
use code_app_server_protocol::ModelUpgradeInfo;
use code_app_server_protocol::ReasoningEffortOption;
use code_app_server_protocol::ReviewStartParams;
use code_app_server_protocol::ReviewStartResponse;
use code_app_server_protocol::ReviewTarget as V2ReviewTarget;
use code_app_server_protocol::Thread;
use code_app_server_protocol::ThreadItem;
use code_app_server_protocol::ThreadResumeParams;
use code_app_server_protocol::ThreadResumeResponse;
use code_app_server_protocol::Turn;
use code_app_server_protocol::TurnStatus;
use code_app_server_protocol::UserInput as V2UserInput;
use code_app_server_protocol::ToolRequestUserInputOption;
use code_app_server_protocol::ToolRequestUserInputParams;
use code_app_server_protocol::ToolRequestUserInputQuestion;
use code_app_server_protocol::ToolRequestUserInputResponse;
use code_common::model_presets::all_model_presets;
use code_common::model_presets::model_preset_available_for_auth;
use code_app_server_protocol::AuthMode;
use code_core::AuthManager;
use code_core::CodexConversation;
use code_core::ConversationManager;
use code_core::NewConversation;
use code_core::RolloutRecorder;
use code_core::SessionCatalog;
use code_core::Cursor;
use code_core::config::Config;
use code_core::config::ConfigOverrides;
use code_core::config::ConfigToml;
use code_core::config_edit::{CONFIG_KEY_EFFORT, CONFIG_KEY_MODEL};
use code_core::exec;
use code_core::exec_env;
use code_core::get_platform_sandbox;
use code_core::git_info::git_diff_to_remote;
use code_core::protocol::ApplyPatchApprovalRequestEvent;
use code_core::protocol::Event;
use code_core::protocol::EventMsg;
use code_core::protocol::ExecApprovalRequestEvent;
use code_protocol::mcp_protocol::FuzzyFileSearchParams;
use code_protocol::mcp_protocol::FuzzyFileSearchResponse;
use code_protocol::protocol::ReviewDecision;
use code_protocol::protocol::ReviewTarget as CoreReviewTarget;
use mcp_types::JSONRPCErrorError;
use mcp_types::RequestId;
use code_login::CLIENT_ID;
use code_login::ServerOptions;
use code_login::ShutdownHandle;
use code_login::run_login_server;
use tokio::sync::Mutex;
use tokio::sync::oneshot;
use tokio::time::Duration;
use tokio::time::timeout;
use tracing::error;
use uuid::Uuid;
use chrono::DateTime;

use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use code_utils_json_to_toml::json_to_toml;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::OutgoingMessageSender;
use crate::outgoing_message::OutgoingNotification;
use crate::fuzzy_file_search::run_fuzzy_file_search;
use code_protocol::protocol::TurnAbortReason;
use code_protocol::dynamic_tools::DynamicToolResponse as CoreDynamicToolResponse;
use code_core::protocol::InputItem as CoreInputItem;
use code_core::protocol::Op;
use code_core::protocol as core_protocol;
use code_protocol::mcp_protocol::APPLY_PATCH_APPROVAL_METHOD;
use code_protocol::mcp_protocol::AddConversationListenerParams;
use code_protocol::mcp_protocol::AddConversationSubscriptionResponse;
use code_protocol::mcp_protocol::ApplyPatchApprovalParams;
use code_protocol::mcp_protocol::ApplyPatchApprovalResponse;
use code_protocol::mcp_protocol::ClientRequest;
use code_protocol::mcp_protocol::ConversationId;
use code_protocol::mcp_protocol::DynamicToolCallParams;
use code_protocol::mcp_protocol::DynamicToolCallResponse;
use code_protocol::mcp_protocol::EXEC_COMMAND_APPROVAL_METHOD;
use code_protocol::mcp_protocol::DYNAMIC_TOOL_CALL_METHOD;
use code_protocol::request_user_input::RequestUserInputAnswer;
use code_protocol::request_user_input::RequestUserInputResponse;
use code_protocol::mcp_protocol::ExecCommandApprovalParams;
use code_protocol::mcp_protocol::ExecCommandApprovalResponse;
use code_protocol::mcp_protocol::InputItem as WireInputItem;
use code_protocol::mcp_protocol::InterruptConversationParams;
use code_protocol::mcp_protocol::InterruptConversationResponse;
// Unused login-related and diff param imports removed
use code_protocol::mcp_protocol::GitDiffToRemoteResponse;
use code_protocol::mcp_protocol::GetAuthStatusParams;
use code_protocol::mcp_protocol::GetAuthStatusResponse;
use code_protocol::mcp_protocol::GetUserAgentResponse;
use code_protocol::mcp_protocol::GetUserSavedConfigResponse;
use code_protocol::mcp_protocol::ListConversationsParams;
use code_protocol::mcp_protocol::ListConversationsResponse;
use code_protocol::mcp_protocol::LoginApiKeyParams;
use code_protocol::mcp_protocol::LoginApiKeyResponse;
use code_protocol::mcp_protocol::NewConversationParams;
use code_protocol::mcp_protocol::NewConversationResponse;
use code_protocol::mcp_protocol::ResumeConversationParams;
use code_protocol::mcp_protocol::ResumeConversationResponse;
use code_protocol::mcp_protocol::ArchiveConversationParams;
use code_protocol::mcp_protocol::ArchiveConversationResponse;
use code_protocol::mcp_protocol::RemoveConversationListenerParams;
use code_protocol::mcp_protocol::RemoveConversationSubscriptionResponse;
use code_protocol::mcp_protocol::SetDefaultModelParams;
use code_protocol::mcp_protocol::SetDefaultModelResponse;
use code_protocol::mcp_protocol::SendUserMessageParams;
use code_protocol::mcp_protocol::SendUserMessageResponse;
use code_protocol::mcp_protocol::SendUserTurnParams;
use code_protocol::mcp_protocol::SendUserTurnResponse;
use code_protocol::mcp_protocol::UserInfoResponse;
use code_protocol::mcp_protocol::ExecOneOffCommandParams;
use code_protocol::mcp_protocol::ExecArbitraryCommandResponse;
use code_protocol::mcp_protocol::ConversationSummary;
use code_protocol::mcp_protocol::UserSavedConfig;
use code_protocol::mcp_protocol::Profile;
use code_protocol::mcp_protocol::SandboxSettings;
use code_protocol::mcp_protocol::Tools;
use code_protocol::mcp_protocol::LoginChatGptResponse;
use code_protocol::mcp_protocol::CancelLoginChatGptParams;
use code_protocol::mcp_protocol::CancelLoginChatGptResponse;
use code_protocol::mcp_protocol::LogoutChatGptResponse;
use code_protocol::account::PlanType;
use code_protocol::protocol::RateLimitSnapshot as CoreRateLimitSnapshot;
use code_protocol::protocol::RateLimitWindow as CoreRateLimitWindow;

// Removed deprecated ChatGPT login support scaffolding

const TOOL_REQUEST_USER_INPUT_METHOD: &str = "item/tool/requestUserInput";

struct ConversationListenerRegistration {
    owner_connection_id: ConnectionId,
    cancel_tx: oneshot::Sender<()>,
}

struct ActiveLogin {
    login_id: Uuid,
    shutdown_handle: ShutdownHandle,
}

/// Handles JSON-RPC messages for Codex conversations.
pub struct CodexMessageProcessor {
    auth_manager: Arc<AuthManager>,
    conversation_manager: Arc<ConversationManager>,
    outgoing: Arc<OutgoingMessageSender>,
    code_linux_sandbox_exe: Option<PathBuf>,
    config: Arc<Config>,
    conversation_listeners: HashMap<Uuid, ConversationListenerRegistration>,
    active_login: Arc<Mutex<Option<ActiveLogin>>>,
    // Queue of pending interrupt requests per conversation. We reply when TurnAborted arrives.
    pending_interrupts: Arc<Mutex<HashMap<Uuid, Vec<RequestId>>>>,
    conversation_configs: Arc<Mutex<HashMap<ConversationId, Config>>>,
    resumed_conversation_aliases: Arc<Mutex<HashMap<ConversationId, ConversationId>>>,
    #[allow(dead_code)]
    pending_fuzzy_searches: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl CodexMessageProcessor {
    pub fn new(
        auth_manager: Arc<AuthManager>,
        conversation_manager: Arc<ConversationManager>,
        outgoing: Arc<OutgoingMessageSender>,
        code_linux_sandbox_exe: Option<PathBuf>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            auth_manager,
            conversation_manager,
            outgoing,
            code_linux_sandbox_exe,
            config,
            conversation_listeners: HashMap::new(),
            active_login: Arc::new(Mutex::new(None)),
            pending_interrupts: Arc::new(Mutex::new(HashMap::new())),
            conversation_configs: Arc::new(Mutex::new(HashMap::new())),
            resumed_conversation_aliases: Arc::new(Mutex::new(HashMap::new())),
            pending_fuzzy_searches: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn resolve_conversation_id_alias(
        &self,
        conversation_id: ConversationId,
    ) -> ConversationId {
        self.resumed_conversation_aliases
            .lock()
            .await
            .get(&conversation_id)
            .copied()
            .unwrap_or(conversation_id)
    }

    async fn conversation_config(&self, conversation_id: ConversationId) -> Option<Config> {
        self.conversation_configs
            .lock()
            .await
            .get(&conversation_id)
            .cloned()
    }

    async fn remember_conversation_config(&self, conversation_id: ConversationId, config: &Config) {
        self.conversation_configs
            .lock()
            .await
            .insert(conversation_id, config.clone());
    }

    pub async fn process_request(&mut self, request: ClientRequest) {
        self.process_request_for_connection(ConnectionId(0), request)
            .await;
    }

    pub(crate) async fn process_request_for_connection(
        &mut self,
        connection_id: ConnectionId,
        request: ClientRequest,
    ) {
        match request {
            ClientRequest::Initialize { request_id, .. } => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "already initialized".to_string(),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
            ClientRequest::NewConversation { request_id, params } => {
                // Do not tokio::spawn() to process new_conversation()
                // asynchronously because we need to ensure the conversation is
                // created before processing any subsequent messages.
                self.process_new_conversation(request_id, params).await;
            }
            ClientRequest::ListConversations { request_id, params } => {
                self.list_conversations(request_id, params).await;
            }
            ClientRequest::ResumeConversation { request_id, params } => {
                self.resume_conversation(request_id, params).await;
            }
            ClientRequest::ArchiveConversation { request_id, params } => {
                self.archive_conversation(request_id, params).await;
            }
            ClientRequest::SendUserMessage { request_id, params } => {
                self.send_user_message(request_id, params).await;
            }
            ClientRequest::InterruptConversation { request_id, params } => {
                self.interrupt_conversation(request_id, params).await;
            }
            ClientRequest::AddConversationListener { request_id, params } => {
                self.add_conversation_listener(connection_id, request_id, params)
                    .await;
            }
            ClientRequest::RemoveConversationListener { request_id, params } => {
                self.remove_conversation_listener(connection_id, request_id, params)
                    .await;
            }
            ClientRequest::SendUserTurn { request_id, params } => {
                self.send_user_turn_compat(request_id, params).await;
            }
            ClientRequest::FuzzyFileSearch { request_id, params } => {
                self.fuzzy_file_search(request_id, params).await;
            }
            ClientRequest::LoginChatGpt { request_id, .. } => {
                self.login_chatgpt_v1(request_id).await;
            }
            ClientRequest::LoginApiKey { request_id, params } => {
                self.login_api_key(request_id, params).await;
            }
            ClientRequest::CancelLoginChatGpt { request_id, params } => {
                self.cancel_login_chatgpt_v1(request_id, params).await;
            }
            ClientRequest::LogoutChatGpt { request_id, .. } => {
                self.logout_chatgpt_v1(request_id).await;
            }
            ClientRequest::GetAuthStatus { request_id, params } => {
                self.get_auth_status(request_id, params).await;
            }
            ClientRequest::GetUserSavedConfig { request_id, .. } => {
                self.get_user_saved_config(request_id).await;
            }
            ClientRequest::SetDefaultModel { request_id, params } => {
                self.set_default_model(request_id, params).await;
            }
            ClientRequest::GetUserAgent { request_id, .. } => {
                self.get_user_agent(request_id).await;
            }
            ClientRequest::UserInfo { request_id, .. } => {
                self.user_info(request_id).await;
            }
            ClientRequest::GitDiffToRemote { request_id, params } => {
                self.git_diff_to_origin(request_id, params.cwd).await;
            }
            ClientRequest::ExecOneOffCommand { request_id, params } => {
                self.exec_one_off_command(request_id, params).await;
            }
        }
    }

    pub(crate) async fn on_connection_closed(&mut self, connection_id: ConnectionId) {
        let subscription_ids: Vec<Uuid> = self
            .conversation_listeners
            .iter()
            .filter_map(|(subscription_id, registration)| {
                if registration.owner_connection_id == connection_id {
                    Some(*subscription_id)
                } else {
                    None
                }
            })
            .collect();

        for subscription_id in subscription_ids {
            if let Some(registration) = self.conversation_listeners.remove(&subscription_id) {
                let _ = registration.cancel_tx.send(());
            }
        }
    }

    pub(crate) async fn get_account_response_v2(
        &self,
        refresh_token: bool,
    ) -> Result<GetAccountResponse, JSONRPCErrorError> {
        let requires_openai_auth = self.config.model_provider.requires_openai_auth;

        self.refresh_token_if_requested(refresh_token).await;

        if !requires_openai_auth {
            return Ok(GetAccountResponse {
                account: None,
                requires_openai_auth,
            });
        }

        let account = match self.auth_manager.auth() {
            Some(auth) if auth.mode == code_app_server_protocol::AuthMode::ApiKey => {
                Some(V2Account::ApiKey {})
            }
            Some(auth) if auth.mode.is_chatgpt() => {
                let email = auth
                    .get_token_data()
                    .await
                    .ok()
                    .and_then(|token_data| token_data.id_token.email);
                let plan_type = parse_plan_type(auth.get_plan_type());

                match email {
                    Some(email) => Some(V2Account::Chatgpt { email, plan_type }),
                    None => {
                        return Err(JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: "email is required for chatgpt authentication".to_string(),
                            data: None,
                        });
                    }
                }
            }
            _ => None,
        };

        Ok(GetAccountResponse {
            account,
            requires_openai_auth,
        })
    }

    async fn refresh_token_if_requested(&self, refresh_token: bool) {
        if !refresh_token {
            return;
        }

        if self
            .auth_manager
            .auth()
            .as_ref()
            .is_some_and(|auth| auth.mode == code_app_server_protocol::AuthMode::ChatgptAuthTokens)
        {
            return;
        }

        let _ = self.auth_manager.refresh_token_classified().await;
    }

    pub(crate) async fn login_account_v2(
        &self,
        params: LoginAccountParams,
    ) -> Result<LoginAccountResponse, JSONRPCErrorError> {
        match params {
            LoginAccountParams::ApiKey { api_key } => {
                let api_key = api_key.trim();
                if api_key.is_empty() {
                    return Err(JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: "apiKey is required".to_string(),
                        data: None,
                    });
                }

                if let Err(err) = code_core::auth::login_with_api_key(&self.config.code_home, api_key) {
                    return Err(JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: format!("failed to persist api key: {err}"),
                        data: None,
                    });
                }

                self.auth_manager.reload();
                Ok(LoginAccountResponse::ApiKey {})
            }
            LoginAccountParams::Chatgpt => self.start_chatgpt_login_v2().await,
            LoginAccountParams::ChatgptAuthTokens {
                access_token,
                chatgpt_account_id,
                chatgpt_plan_type,
            } => {
                code_core::auth::login_with_chatgpt_auth_tokens(
                    &self.config.code_home,
                    &access_token,
                    &chatgpt_account_id,
                    chatgpt_plan_type.as_deref(),
                )
                .map_err(|err| JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("failed to persist chatgpt auth tokens: {err}"),
                    data: None,
                })?;

                self.auth_manager.reload();
                Ok(LoginAccountResponse::ChatgptAuthTokens {})
            }
        }
    }

    async fn start_chatgpt_login_v2(&self) -> Result<LoginAccountResponse, JSONRPCErrorError> {
        let mut options = ServerOptions::new(
            self.config.code_home.clone(),
            CLIENT_ID.to_string(),
            self.config.responses_originator_header.clone(),
        );
        options.open_browser = false;

        let server = run_login_server(options).map_err(|err| JSONRPCErrorError {
            code: INTERNAL_ERROR_CODE,
            message: format!("failed to start login server: {err}"),
            data: None,
        })?;

        let login_id = Uuid::new_v4();
        let auth_url = server.auth_url.clone();
        let shutdown_handle = server.cancel_handle();

        {
            let mut active_login = self.active_login.lock().await;
            if let Some(existing) = active_login.take() {
                existing.shutdown_handle.shutdown();
            }
            *active_login = Some(ActiveLogin {
                login_id,
                shutdown_handle: shutdown_handle.clone(),
            });
        }

        let active_login = Arc::clone(&self.active_login);
        let auth_manager = Arc::clone(&self.auth_manager);
        tokio::spawn(async move {
            let login_result = timeout(Duration::from_secs(300), server.block_until_done()).await;
            match login_result {
                Ok(Ok(())) => {
                    auth_manager.reload();
                }
                Ok(Err(err)) => {
                    tracing::warn!("chatgpt login failed: {err}");
                }
                Err(_elapsed) => {
                    shutdown_handle.shutdown();
                }
            }

            let mut active_login = active_login.lock().await;
            if active_login.as_ref().map(|entry| entry.login_id) == Some(login_id) {
                *active_login = None;
            }
        });

        Ok(LoginAccountResponse::Chatgpt {
            login_id: login_id.to_string(),
            auth_url,
        })
    }

    pub(crate) async fn cancel_login_account_v2(
        &self,
        params: CancelLoginAccountParams,
    ) -> Result<CancelLoginAccountResponse, JSONRPCErrorError> {
        let login_id = Uuid::parse_str(&params.login_id).map_err(|_| JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message: format!("invalid login id: {}", params.login_id),
            data: None,
        })?;

        let status = self.cancel_active_login(login_id).await;
        Ok(CancelLoginAccountResponse { status })
    }

    async fn cancel_active_login(&self, login_id: Uuid) -> CancelLoginAccountStatus {
        let mut active_login = self.active_login.lock().await;
        if active_login.as_ref().map(|entry| entry.login_id) == Some(login_id) {
            if let Some(existing) = active_login.take() {
                existing.shutdown_handle.shutdown();
            }
            CancelLoginAccountStatus::Canceled
        } else {
            CancelLoginAccountStatus::NotFound
        }
    }

    pub(crate) async fn logout_account_v2(&self) -> Result<LogoutAccountResponse, JSONRPCErrorError> {
        {
            let mut active_login = self.active_login.lock().await;
            if let Some(existing) = active_login.take() {
                existing.shutdown_handle.shutdown();
            }
        }

        self.auth_manager.logout().map_err(|err| JSONRPCErrorError {
            code: INTERNAL_ERROR_CODE,
            message: format!("logout failed: {err}"),
            data: None,
        })?;
        Ok(LogoutAccountResponse {})
    }

    pub(crate) fn get_account_rate_limits_v2(
        &self,
    ) -> Result<GetAccountRateLimitsResponse, JSONRPCErrorError> {
        let Some(auth) = self.auth_manager.auth() else {
            return Err(JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "account authentication required to read rate limits".to_string(),
                data: None,
            });
        };

        if !auth.mode.is_chatgpt() {
            return Err(JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "chatgpt authentication required to read rate limits".to_string(),
                data: None,
            });
        }

        let snapshots = code_core::account_usage::list_rate_limit_snapshots(&self.config.code_home)
            .map_err(|err| JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to read rate limit snapshots: {err}"),
                data: None,
            })?;
        let selected = select_rate_limit_snapshot(auth.get_account_id(), snapshots).ok_or_else(|| {
            JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "no rate limit snapshot available".to_string(),
                data: None,
            }
        })?;

        let snapshot = selected.snapshot.clone().ok_or_else(|| JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message: "no rate limit snapshot available".to_string(),
            data: None,
        })?;

        let plan_type = selected.plan.clone().map(|value| parse_plan_type(Some(value)));
        let rate_limits = rate_limit_snapshot_from_event(&snapshot, plan_type);
        let mut rate_limits_by_limit_id = HashMap::new();
        rate_limits_by_limit_id.insert(selected.account_id, rate_limits.clone().into());

        Ok(GetAccountRateLimitsResponse {
            rate_limits: rate_limits.into(),
            rate_limits_by_limit_id: Some(rate_limits_by_limit_id),
        })
    }

    async fn process_new_conversation(&self, request_id: RequestId, params: NewConversationParams) {
        let config = match derive_config_from_params(
            params,
            None,
            self.code_linux_sandbox_exe.clone(),
        ) {
            Ok(config) => config,
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("error deriving config: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        match self
            .conversation_manager
            .new_conversation(config.clone())
            .await
        {
            Ok(NewConversation {
                conversation_id,
                session_configured,
                ..
            }) => {
                self.remember_conversation_config(conversation_id, &config).await;
                let response = NewConversationResponse {
                    conversation_id,
                    model: session_configured.model,
                    reasoning_effort: None,
                    // We do not expose the underlying rollout file path in this fork; provide the sessions root.
                    rollout_path: self.config.code_home.join("sessions"),
                };
                self.outgoing.send_response(request_id, response).await;
            }
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("error creating conversation: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn send_user_message(&self, request_id: RequestId, params: SendUserMessageParams) {
        let SendUserMessageParams {
            conversation_id,
            items,
        } = params;
        let conversation_id = self.resolve_conversation_id_alias(conversation_id).await;
        let Ok(conversation) = self
            .conversation_manager
            .get_conversation(conversation_id)
            .await
        else {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: format!("conversation not found: {conversation_id}"),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        };

        let mapped_items: Vec<CoreInputItem> = items
            .into_iter()
            .map(|item| match item {
                WireInputItem::Text { text } => CoreInputItem::Text { text },
                WireInputItem::Image { image_url } => CoreInputItem::Image { image_url },
                WireInputItem::LocalImage { path } => CoreInputItem::LocalImage { path },
            })
            .collect();

        // Submit user input to the conversation.
        let _ = conversation
            .submit(Op::UserInput {
                items: mapped_items,
                final_output_json_schema: None,
            })
            .await;

        // Acknowledge with an empty result.
        self.outgoing
            .send_response(request_id, SendUserMessageResponse {})
            .await;
    }

    #[allow(dead_code)]
    async fn send_user_turn(&self, request_id: RequestId, params: SendUserTurnParams) {
        let SendUserTurnParams {
            conversation_id,
            items,
            cwd: _,
            approval_policy: _,
            sandbox_policy: _,
            model: _,
            effort: _,
            summary: _,
        } = params;

        let Ok(conversation) = self
            .conversation_manager
            .get_conversation(conversation_id)
            .await
        else {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: format!("conversation not found: {conversation_id}"),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        };

        let mapped_items: Vec<CoreInputItem> = items
            .into_iter()
            .map(|item| match item {
                WireInputItem::Text { text } => CoreInputItem::Text { text },
                WireInputItem::Image { image_url } => CoreInputItem::Image { image_url },
                WireInputItem::LocalImage { path } => CoreInputItem::LocalImage { path },
            })
            .collect();

        // Core protocol compatibility: older cores do not support per-turn overrides.
        // Submit only the user input items.
        let _ = conversation
            .submit(Op::UserInput {
                items: mapped_items,
                final_output_json_schema: None,
            })
            .await;

        self.outgoing
            .send_response(request_id, SendUserTurnResponse {})
            .await;
    }

    async fn interrupt_conversation(
        &mut self,
        request_id: RequestId,
        params: InterruptConversationParams,
    ) {
        let InterruptConversationParams { conversation_id } = params;
        let conversation_id = self.resolve_conversation_id_alias(conversation_id).await;
        let Ok(conversation) = self
            .conversation_manager
            .get_conversation(conversation_id)
            .await
        else {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: format!("conversation not found: {conversation_id}"),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        };

        // Submit the interrupt and respond immediately (core does not emit a dedicated event).
        let _ = conversation.submit(Op::Interrupt).await;
        let response = InterruptConversationResponse { abort_reason: TurnAbortReason::Interrupted };
        self.outgoing.send_response(request_id, response).await;
    }

    async fn add_conversation_listener(
        &mut self,
        owner_connection_id: ConnectionId,
        request_id: RequestId,
        params: AddConversationListenerParams,
    ) {
        let AddConversationListenerParams { conversation_id } = params;
        let conversation_id = self.resolve_conversation_id_alias(conversation_id).await;
        let Ok(conversation) = self
            .conversation_manager
            .get_conversation(conversation_id)
            .await
        else {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: format!("conversation not found: {conversation_id}"),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        };

        let subscription_id = Uuid::new_v4();
        let (cancel_tx, mut cancel_rx) = oneshot::channel();
        self.conversation_listeners.insert(
            subscription_id,
            ConversationListenerRegistration {
                owner_connection_id,
                cancel_tx,
            },
        );
        let outgoing_for_task = self.outgoing.clone();
        let pending_interrupts = self.pending_interrupts.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut cancel_rx => {
                        // User has unsubscribed, so exit this task.
                        break;
                    }
                    event = conversation.next_event() => {
                        let event = match event {
                            Ok(event) => event,
                            Err(err) => {
                                tracing::warn!("conversation.next_event() failed with: {err}");
                                break;
                            }
                        };

                        // For now, we send a notification for every event,
                        // JSON-serializing the `Event` as-is, but we will move
                        // to creating a special enum for notifications with a
                        // stable wire format.
                        let method = format!("codex/event/{}", event.msg);
                        let mut params = match serde_json::to_value(event.clone()) {
                            Ok(serde_json::Value::Object(map)) => map,
                            Ok(_) => {
                                tracing::error!("event did not serialize to an object");
                                continue;
                            }
                            Err(err) => {
                                tracing::error!("failed to serialize event: {err}");
                                continue;
                            }
                        };
                        params.insert("conversationId".to_string(), conversation_id.to_string().into());

                        outgoing_for_task
                            .send_notification_to_connection(
                                owner_connection_id,
                                OutgoingNotification {
                                    method,
                                    params: Some(params.into()),
                                },
                            )
                            .await;

                        apply_bespoke_event_handling(
                            event.clone(),
                            conversation_id,
                            owner_connection_id,
                            conversation.clone(),
                            outgoing_for_task.clone(),
                            pending_interrupts.clone(),
                        )
                        .await;
                    }
                }
            }
        });
        let response = AddConversationSubscriptionResponse { subscription_id };
        self.outgoing.send_response(request_id, response).await;
    }

    async fn remove_conversation_listener(
        &mut self,
        requester_connection_id: ConnectionId,
        request_id: RequestId,
        params: RemoveConversationListenerParams,
    ) {
        let RemoveConversationListenerParams { subscription_id } = params;
        match self.conversation_listeners.remove(&subscription_id) {
            Some(registration) => {
                if registration.owner_connection_id != requester_connection_id {
                    // Keep ownership scoped to the client that created the listener.
                    self.conversation_listeners
                        .insert(subscription_id, registration);
                    let error = JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: format!("subscription not found: {subscription_id}"),
                        data: None,
                    };
                    self.outgoing.send_error(request_id, error).await;
                    return;
                }

                // Signal the spawned task to exit and acknowledge.
                let _ = registration.cancel_tx.send(());
                let response = RemoveConversationSubscriptionResponse {};
                self.outgoing.send_response(request_id, response).await;
            }
            None => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("subscription not found: {subscription_id}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn list_conversations(&self, request_id: RequestId, params: ListConversationsParams) {
        let page_size = params.page_size.unwrap_or(50).min(200);
        let cursor: Option<Cursor> = match params.cursor {
            Some(cursor) => match serde_json::from_value::<Cursor>(serde_json::Value::String(cursor)) {
                Ok(cursor) => Some(cursor),
                Err(_) => {
                    let error = JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: "invalid cursor".to_string(),
                        data: None,
                    };
                    self.outgoing.send_error(request_id, error).await;
                    return;
                }
            },
            None => None,
        };

        let page = match RolloutRecorder::list_conversations(
            &self.config.code_home,
            page_size,
            cursor.as_ref(),
            &[],
        )
        .await
        {
            Ok(page) => page,
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("failed to list conversations: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let mut out = Vec::new();
        for item in page.items {
            let conversation_id = match conversation_id_from_rollout_path(&item.path) {
                Some(id) => id,
                None => continue,
            };
            let preview = snippet_from_rollout_tail(&item.tail).unwrap_or_default();
            out.push(ConversationSummary {
                conversation_id,
                path: item.path,
                preview,
                timestamp: item.created_at,
            });
        }

        let next_cursor = page.next_cursor.and_then(|cursor| {
            serde_json::to_value(cursor)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        });

        self.outgoing
            .send_response(
                request_id,
                ListConversationsResponse {
                    items: out,
                    next_cursor,
                },
            )
            .await;
    }

    async fn resume_conversation(&self, request_id: RequestId, params: ResumeConversationParams) {
        let overrides = params.overrides.unwrap_or_default();
        let config = match derive_config_from_params(
            overrides,
            None,
            self.code_linux_sandbox_exe.clone(),
        ) {
            Ok(config) => config,
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("error deriving config: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        match self
            .conversation_manager
            .resume_conversation_from_rollout(
                config.clone(),
                params.path,
                Arc::clone(&self.auth_manager),
            )
            .await
        {
            Ok(NewConversation {
                conversation_id,
                session_configured,
                ..
            }) => {
                self.remember_conversation_config(conversation_id, &config).await;
                self.outgoing
                    .send_response(
                        request_id,
                        ResumeConversationResponse {
                            conversation_id,
                            model: session_configured.model,
                            initial_messages: None,
                        },
                    )
                    .await;
            }
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("error resuming conversation: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    pub(crate) async fn thread_resume_v2(
        &self,
        request_id: RequestId,
        params: ThreadResumeParams,
    ) {
        if params.history.is_some() {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "thread/resume.history is not supported by the Every Code app-server"
                    .to_string(),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        let thread_id = params.thread_id.clone();
        let explicit_path = params.path.clone();
        let catalog_thread_id = match ConversationId::from_string(&thread_id) {
            Ok(conversation_id) => self
                .resolve_conversation_id_alias(conversation_id)
                .await
                .to_string(),
            Err(_) => thread_id.clone(),
        };
        let catalog = SessionCatalog::new(self.config.code_home.clone());

        let catalog_entry = if thread_resume_should_lookup_catalog(explicit_path.as_deref()) {
            match catalog.find_by_id(&catalog_thread_id).await {
                Ok(entry) => entry,
                Err(err) => {
                    let error = JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: format!("failed to resolve thread: {err}"),
                        data: None,
                    };
                    self.outgoing.send_error(request_id, error).await;
                    return;
                }
            }
        } else {
            None
        };

        let rollout_path = match thread_resume_rollout_path(
            explicit_path.clone(),
            catalog_entry
                .as_ref()
                .map(|entry| catalog.entry_rollout_path(entry)),
        ) {
            Some(path) => path,
            None => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "thread not found".to_string(),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let overrides = NewConversationParams {
            model: params.model.clone(),
            profile: None,
            cwd: params.cwd.clone(),
            approval_policy: params
                .approval_policy
                .clone()
                .map(|approval_policy| approval_policy.to_core()),
            sandbox: params.sandbox.map(|sandbox| sandbox.to_core()),
            config: params.config.clone(),
            base_instructions: params.base_instructions.clone(),
            include_plan_tool: None,
            dynamic_tools: None,
            include_apply_patch_tool: None,
        };
        let config = match derive_config_from_params(
            overrides,
            params.model_provider.clone(),
            self.code_linux_sandbox_exe.clone(),
        ) {
            Ok(config) => config,
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("error deriving config: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        match self
            .conversation_manager
            .resume_conversation_from_rollout(
                config.clone(),
                rollout_path.clone(),
                Arc::clone(&self.auth_manager),
            )
            .await
        {
            Ok(NewConversation {
                conversation_id,
                session_configured: _,
                ..
            }) => {
                self.remember_conversation_config(conversation_id, &config).await;
                let canonical_thread_id =
                    thread_resume_canonical_thread_id(conversation_id, &rollout_path, catalog_entry.as_ref());
                let thread = thread_resume_response_thread(
                    &canonical_thread_id,
                    catalog_entry.as_ref(),
                    &config,
                    rollout_path,
                );
                self.outgoing
                    .send_response(
                        request_id,
                        ThreadResumeResponse {
                            thread,
                            model: config.model.clone(),
                            model_provider: config.model_provider_id.clone(),
                            cwd: config.cwd.clone(),
                            approval_policy: map_ask_for_approval_to_wire(config.approval_policy).into(),
                            sandbox: map_sandbox_policy_to_wire(config.sandbox_policy.clone()).into(),
                            reasoning_effort: Some(config.model_reasoning_effort.into()),
                        },
                    )
                    .await;

                if let Ok(requested_conversation_id) = ConversationId::from_string(&thread_id)
                    && thread_resume_should_record_alias(
                        explicit_path.as_deref(),
                        &requested_conversation_id,
                        &conversation_id,
                    )
                {
                    self.resumed_conversation_aliases
                        .lock()
                        .await
                        .insert(requested_conversation_id, conversation_id);
                }
            }
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("error resuming thread: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    pub(crate) async fn model_list_v2(&self, request_id: RequestId, params: ModelListParams) {
        let ModelListParams {
            limit,
            cursor,
            include_hidden,
        } = params;
        let (auth_mode, supports_pro_only_models) =
            model_picker_auth_state(&self.config, &self.auth_manager);
        let mut models: Vec<V2Model> = all_model_presets()
            .iter()
            .filter(|preset| {
                model_preset_available_for_auth(preset, auth_mode, supports_pro_only_models)
            })
            .filter(|preset| include_hidden.unwrap_or(false) || preset.show_in_picker)
            .cloned()
            .map(model_preset_to_v2_model)
            .collect();

        mark_default_model(&mut models);

        let total = models.len();
        if total == 0 {
            self.outgoing
                .send_response(
                    request_id,
                    ModelListResponse {
                        data: Vec::new(),
                        next_cursor: None,
                    },
                )
                .await;
            return;
        }

        let effective_limit = limit.unwrap_or(total as u32).max(1) as usize;
        let effective_limit = effective_limit.min(total);
        let start = match cursor {
            Some(cursor) => match cursor.parse::<usize>() {
                Ok(index) => index,
                Err(_) => {
                    self.outgoing
                        .send_error(
                            request_id,
                            JSONRPCErrorError {
                                code: INVALID_REQUEST_ERROR_CODE,
                                message: format!("invalid cursor: {cursor}"),
                                data: None,
                            },
                        )
                        .await;
                    return;
                }
            },
            None => 0,
        };

        if start > total {
            self.outgoing
                .send_error(
                    request_id,
                    JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: format!("cursor {start} exceeds total models {total}"),
                        data: None,
                    },
                )
                .await;
            return;
        }

        let end = start.saturating_add(effective_limit).min(total);
        let next_cursor = if end < total {
            Some(end.to_string())
        } else {
            None
        };

        self.outgoing
            .send_response(
                request_id,
                ModelListResponse {
                    data: models[start..end].to_vec(),
                    next_cursor,
                },
            )
            .await;
    }

    pub(crate) async fn review_start_v2(&self, request_id: RequestId, params: ReviewStartParams) {
        let ReviewStartParams {
            thread_id,
            target,
            delivery,
        } = params;
        let resolved_thread_id = match ConversationId::from_string(&thread_id) {
            Ok(conversation_id) => self.resolve_conversation_id_alias(conversation_id).await,
            Err(_) => {
                self.outgoing
                    .send_error(
                        request_id,
                        JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: format!("invalid thread id: {thread_id}"),
                            data: None,
                        },
                    )
                    .await;
                return;
            }
        };

        let (review_request, display_text) = match review_request_from_target(target) {
            Ok(value) => value,
            Err(err) => {
                self.outgoing.send_error(request_id, err).await;
                return;
            }
        };

        let delivery = delivery.unwrap_or(code_app_server_protocol::ReviewDelivery::Inline);
        match delivery {
            code_app_server_protocol::ReviewDelivery::Inline => {
                let conversation = match self
                    .conversation_manager
                    .get_conversation(resolved_thread_id)
                    .await
                {
                    Ok(conversation) => conversation,
                    Err(_) => {
                        self.outgoing
                            .send_error(
                                request_id,
                                JSONRPCErrorError {
                                    code: INVALID_REQUEST_ERROR_CODE,
                                    message: format!("thread not found: {thread_id}"),
                                    data: None,
                                },
                            )
                            .await;
                        return;
                    }
                };

                let turn_id = match conversation.submit(Op::Review { review_request }).await {
                    Ok(turn_id) => turn_id,
                    Err(err) => {
                        self.outgoing
                            .send_error(
                                request_id,
                                JSONRPCErrorError {
                                    code: INTERNAL_ERROR_CODE,
                                    message: format!("failed to start review: {err}"),
                                    data: None,
                                },
                            )
                            .await;
                        return;
                    }
                };

                self.outgoing
                    .send_response(
                        request_id,
                        ReviewStartResponse {
                            turn: build_review_turn(turn_id, &display_text),
                            review_thread_id: thread_id,
                        },
                    )
                    .await;
            }
            code_app_server_protocol::ReviewDelivery::Detached => {
                let source_config = self.conversation_config(resolved_thread_id).await;
                let catalog = SessionCatalog::new(self.config.code_home.clone());
                let catalog_entry = if source_config.is_none() {
                    match catalog.find_by_id(&resolved_thread_id.to_string()).await {
                        Ok(entry) => entry,
                        Err(err) => {
                            self.outgoing
                                .send_error(
                                    request_id,
                                    JSONRPCErrorError {
                                        code: INTERNAL_ERROR_CODE,
                                        message: format!(
                                            "failed to resolve detached review thread: {err}"
                                        ),
                                        data: None,
                                    },
                                )
                                .await;
                            return;
                        }
                    }
                } else {
                    None
                };
                let source_thread_exists = source_config.is_some()
                    || catalog_entry.is_some()
                    || self
                        .conversation_manager
                        .get_conversation(resolved_thread_id)
                        .await
                        .is_ok();
                if !source_thread_exists {
                    self.outgoing
                        .send_error(
                            request_id,
                            JSONRPCErrorError {
                                code: INVALID_REQUEST_ERROR_CODE,
                                message: format!("thread not found: {thread_id}"),
                                data: None,
                            },
                        )
                        .await;
                    return;
                }

                let mut config = source_config.clone().unwrap_or_else(|| (*self.config).clone());
                if let Some(entry) = catalog_entry {
                    config.cwd = entry.cwd_real;
                }

                let NewConversation {
                    conversation_id,
                    ..
                } = match self.conversation_manager.new_conversation(config.clone()).await {
                    Ok(conversation) => conversation,
                    Err(err) => {
                        self.outgoing
                            .send_error(
                                request_id,
                                JSONRPCErrorError {
                                    code: INTERNAL_ERROR_CODE,
                                    message: format!("failed to create detached review thread: {err}"),
                                    data: None,
                                },
                            )
                            .await;
                        return;
                    }
                };
                self.remember_conversation_config(conversation_id, &config).await;
                let conversation = match self
                    .conversation_manager
                    .get_conversation(conversation_id)
                    .await
                {
                    Ok(conversation) => conversation,
                    Err(err) => {
                        self.outgoing
                            .send_error(
                                request_id,
                                JSONRPCErrorError {
                                    code: INTERNAL_ERROR_CODE,
                                    message: format!("failed to load detached review thread: {err}"),
                                    data: None,
                                },
                            )
                            .await;
                        return;
                    }
                };
                let turn_id = match conversation.submit(Op::Review { review_request }).await {
                    Ok(turn_id) => turn_id,
                    Err(err) => {
                        self.outgoing
                            .send_error(
                                request_id,
                                JSONRPCErrorError {
                                    code: INTERNAL_ERROR_CODE,
                                    message: format!("failed to start detached review: {err}"),
                                    data: None,
                                },
                            )
                            .await;
                        return;
                    }
                };

                self.outgoing
                    .send_response(
                        request_id,
                        ReviewStartResponse {
                            turn: build_review_turn(turn_id, &display_text),
                            review_thread_id: conversation_id.to_string(),
                        },
                    )
                    .await;
            }
        }
    }

    async fn archive_conversation(
        &self,
        request_id: RequestId,
        params: ArchiveConversationParams,
    ) {
        let ArchiveConversationParams {
            conversation_id,
            rollout_path,
        } = params;
        let conversation_id = self.resolve_conversation_id_alias(conversation_id).await;

        if self
            .conversation_manager
            .get_conversation(conversation_id)
            .await
            .is_ok()
        {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "cannot archive an active conversation".to_string(),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        let catalog = code_core::SessionCatalog::new(self.config.code_home.clone());
        match catalog
            .archive_conversation(uuid::Uuid::from(conversation_id), &rollout_path)
            .await
        {
            Ok(true) => {
                self.outgoing
                    .send_response(request_id, ArchiveConversationResponse {})
                    .await;
            }
            Ok(false) => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "conversation not found".to_string(),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("failed to archive conversation: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn login_chatgpt_v1(&self, request_id: RequestId) {
        match self.start_chatgpt_login_v2().await {
            Ok(LoginAccountResponse::Chatgpt { login_id, auth_url }) => {
                let login_id = match Uuid::parse_str(&login_id) {
                    Ok(login_id) => login_id,
                    Err(err) => {
                        let error = JSONRPCErrorError {
                            code: INTERNAL_ERROR_CODE,
                            message: format!("invalid login id generated by server: {err}"),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                        return;
                    }
                };

                self.outgoing
                    .send_response(request_id, LoginChatGptResponse { login_id, auth_url })
                    .await;
            }
            Ok(_) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: "unexpected login response type".to_string(),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn cancel_login_chatgpt_v1(
        &self,
        request_id: RequestId,
        params: CancelLoginChatGptParams,
    ) {
        let status = self.cancel_active_login(params.login_id).await;
        match status {
            CancelLoginAccountStatus::Canceled => {
                self.outgoing
                    .send_response(request_id, CancelLoginChatGptResponse {})
                    .await;
            }
            CancelLoginAccountStatus::NotFound => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("login id not found: {}", params.login_id),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn logout_chatgpt_v1(&self, request_id: RequestId) {
        match self.logout_account_v2().await {
            Ok(_) => {
                self.outgoing
                    .send_response(request_id, LogoutChatGptResponse {})
                    .await;
            }
            Err(error) => {
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn login_api_key(&self, request_id: RequestId, params: LoginApiKeyParams) {
        let api_key = params.api_key.trim();
        if api_key.is_empty() {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "api_key is required".to_string(),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        if let Err(err) = code_core::auth::login_with_api_key(&self.config.code_home, api_key) {
            let error = JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to persist api key: {err}"),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        self.auth_manager.reload();
        self.outgoing
            .send_response(request_id, LoginApiKeyResponse {})
            .await;
    }

    async fn get_auth_status(&self, request_id: RequestId, params: GetAuthStatusParams) {
        let requires_openai_auth = self.config.model_provider.requires_openai_auth;
        let include_token = params.include_token.unwrap_or(false);

        self.refresh_token_if_requested(params.refresh_token.unwrap_or(false))
            .await;

        let auth = self.auth_manager.auth();
        let mut auth_method = auth.as_ref().map(|a| map_auth_mode_to_wire(a.mode));
        let mut auth_token = None;

        if !requires_openai_auth {
            auth_method = None;
        } else if include_token {
            if let Some(auth) = auth.as_ref() {
                let permanent_refresh_failure =
                    self.auth_manager.refresh_failure_for_auth(auth).is_some();
                if !permanent_refresh_failure && let Ok(token) = auth.get_token().await {
                    if !token.trim().is_empty() {
                        auth_token = Some(token);
                    }
                }
            }
        }

        self.outgoing
            .send_response(
                request_id,
                GetAuthStatusResponse {
                    auth_method,
                    auth_token,
                    requires_openai_auth: Some(requires_openai_auth),
                },
            )
            .await;
    }

    async fn get_user_saved_config(&self, request_id: RequestId) {
        let cfg: ConfigToml = match code_core::config::load_config_as_toml_with_cli_overrides(
            &self.config.code_home,
            Vec::new(),
        ) {
            Ok(cfg) => cfg,
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("failed to load config: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
                return;
            }
        };

        let config = UserSavedConfig {
            approval_policy: cfg.approval_policy.map(map_ask_for_approval_to_wire),
            sandbox_mode: cfg.sandbox_mode,
            sandbox_settings: cfg.sandbox_workspace_write.as_ref().map(|s| SandboxSettings {
                writable_roots: s.writable_roots.clone(),
                network_access: Some(s.network_access),
                exclude_tmpdir_env_var: Some(s.exclude_tmpdir_env_var),
                exclude_slash_tmp: Some(s.exclude_slash_tmp),
            }),
            model: cfg.model,
            model_reasoning_effort: cfg
                .model_reasoning_effort
                .map(map_reasoning_effort_to_wire),
            model_reasoning_summary: cfg
                .model_reasoning_summary
                .map(map_reasoning_summary_to_wire),
            model_verbosity: cfg.model_text_verbosity.map(map_verbosity_to_wire),
            tools: cfg.tools.map(|t| Tools {
                web_search: t.web_search,
                view_image: t.view_image,
            }),
            profile: cfg.profile,
            profiles: cfg
                .profiles
                .into_iter()
                .map(|(name, profile)| {
                    (
                        name,
                        Profile {
                            model: profile.model,
                            model_provider: profile.model_provider,
                            approval_policy: profile
                                .approval_policy
                                .map(map_ask_for_approval_to_wire),
                            model_reasoning_effort: profile
                                .model_reasoning_effort
                                .map(map_reasoning_effort_to_wire),
                            model_reasoning_summary: profile
                                .model_reasoning_summary
                                .map(map_reasoning_summary_to_wire),
                            model_verbosity: profile
                                .model_text_verbosity
                                .map(map_verbosity_to_wire),
                            chatgpt_base_url: profile.chatgpt_base_url,
                        },
                    )
                })
                .collect(),
        };

        self.outgoing
            .send_response(request_id, GetUserSavedConfigResponse { config })
            .await;
    }

    async fn set_default_model(&self, request_id: RequestId, params: SetDefaultModelParams) {
        let effort_value = params.reasoning_effort.map(|effort| match effort {
            code_protocol::config_types::ReasoningEffort::None => "minimal".to_string(),
            _ => effort.to_string(),
        });
        let model_value = params.model;

        let effort_ref = effort_value.as_deref();
        let model_ref = model_value.as_deref();

        let overrides = [
            (&[CONFIG_KEY_MODEL][..], model_ref),
            (&[CONFIG_KEY_EFFORT][..], effort_ref),
        ];

        if let Err(err) = code_core::config_edit::persist_overrides_and_clear_if_none(
            &self.config.code_home,
            self.config.active_profile.as_deref(),
            &overrides,
        )
        .await
        {
            let error = JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to persist config: {err}"),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        self.outgoing
            .send_response(request_id, SetDefaultModelResponse {})
            .await;
    }

    async fn get_user_agent(&self, request_id: RequestId) {
        let originator = self.config.responses_originator_header.trim();
        let user_agent = code_core::default_client::get_code_user_agent(
            (!originator.is_empty()).then_some(originator),
        );
        self.outgoing
            .send_response(request_id, GetUserAgentResponse { user_agent })
            .await;
    }

    async fn user_info(&self, request_id: RequestId) {
        let mut alleged_user_email = None;
        if let Some(auth) = self.auth_manager.auth() {
            if auth.mode.is_chatgpt() {
                alleged_user_email = auth
                    .get_token_data()
                    .await
                    .ok()
                    .and_then(|t| t.id_token.email);
            }
        }
        self.outgoing
            .send_response(request_id, UserInfoResponse { alleged_user_email })
            .await;
    }

    async fn exec_one_off_command(&self, request_id: RequestId, params: ExecOneOffCommandParams) {
        if params.command.is_empty() {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "command is required".to_string(),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        if params.sandbox_policy.is_some() {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "sandbox_policy override is not supported".to_string(),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        }

        let cwd = params.cwd.unwrap_or_else(|| self.config.cwd.clone());
        let env = exec_env::create_env(&self.config.shell_environment_policy);

        let exec_params = exec::ExecParams {
            command: params.command,
            cwd,
            timeout_ms: params.timeout_ms,
            env,
            with_escalated_permissions: None,
            justification: None,
        };
        let sandbox_type = get_platform_sandbox().unwrap_or(exec::SandboxType::None);

        match exec::process_exec_tool_call(
            exec_params,
            sandbox_type,
            &self.config.sandbox_policy,
            self.config.cwd.as_path(),
            &self.config.code_linux_sandbox_exe,
            None,
        )
        .await
        {
            Ok(output) => {
                let exec::ExecToolCallOutput {
                    exit_code,
                    stdout,
                    stderr,
                    ..
                } = output;
                self.outgoing
                    .send_response(
                        request_id,
                        ExecArbitraryCommandResponse {
                            exit_code,
                            stdout: stdout.text,
                            stderr: stderr.text,
                        },
                    )
                    .await;
            }
            Err(err) => {
                let error = JSONRPCErrorError {
                    code: INTERNAL_ERROR_CODE,
                    message: format!("exec failed: {err}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    async fn git_diff_to_origin(&self, request_id: RequestId, cwd: PathBuf) {
        let diff = git_diff_to_remote(&cwd).await;
        match diff {
            Some(value) => {
                let response = GitDiffToRemoteResponse {
                    sha: code_protocol::mcp_protocol::GitSha::new(&value.sha.0),
                    diff: value.diff,
                };
                self.outgoing.send_response(request_id, response).await;
            }
            None => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!("failed to compute git diff to remote for cwd: {cwd:?}"),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
        }
    }

    #[allow(dead_code)]
    async fn fuzzy_file_search(&mut self, request_id: RequestId, params: FuzzyFileSearchParams) {
        let FuzzyFileSearchParams {
            query,
            roots,
            cancellation_token,
        } = params;

        let cancel_flag = match cancellation_token.clone() {
            Some(token) => {
                let mut pending_fuzzy_searches = self.pending_fuzzy_searches.lock().await;
                // if a cancellation_token is provided and a pending_request exists for
                // that token, cancel it
                if let Some(existing) = pending_fuzzy_searches.get(&token) {
                    existing.store(true, Ordering::Relaxed);
                }
                let flag = Arc::new(AtomicBool::new(false));
                pending_fuzzy_searches.insert(token.clone(), flag.clone());
                flag
            }
            None => Arc::new(AtomicBool::new(false)),
        };

        let results = match query.as_str() {
            "" => vec![],
            _ => run_fuzzy_file_search(query, roots, cancel_flag.clone()).await,
        };

        if let Some(token) = cancellation_token {
            let mut pending_fuzzy_searches = self.pending_fuzzy_searches.lock().await;
            if let Some(current_flag) = pending_fuzzy_searches.get(&token)
                && Arc::ptr_eq(current_flag, &cancel_flag)
            {
                pending_fuzzy_searches.remove(&token);
            }
        }

        let response = FuzzyFileSearchResponse { files: results };
        self.outgoing.send_response(request_id, response).await;
    }
}

fn parse_rfc3339_timestamp_seconds(value: &str) -> i64 {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.timestamp())
        .unwrap_or_default()
}

fn thread_resume_rollout_path(
    explicit_path: Option<PathBuf>,
    catalog_path: Option<PathBuf>,
) -> Option<PathBuf> {
    explicit_path.or(catalog_path)
}

fn thread_resume_should_lookup_catalog(explicit_path: Option<&std::path::Path>) -> bool {
    explicit_path.is_none()
}

fn thread_resume_should_record_alias(
    explicit_path: Option<&std::path::Path>,
    requested_conversation_id: &ConversationId,
    resumed_conversation_id: &ConversationId,
) -> bool {
    explicit_path.is_none() && requested_conversation_id != resumed_conversation_id
}

fn thread_resume_canonical_thread_id(
    resumed_conversation_id: ConversationId,
    rollout_path: &std::path::Path,
    entry: Option<&code_core::SessionIndexEntry>,
) -> String {
    conversation_id_from_rollout_path(rollout_path)
        .map(|conversation_id| conversation_id.to_string())
        .or_else(|| entry.map(|item| item.session_id.to_string()))
        .unwrap_or_else(|| resumed_conversation_id.to_string())
}

fn thread_resume_response_thread(
    thread_id: &str,
    entry: Option<&code_core::SessionIndexEntry>,
    config: &Config,
    rollout_path: PathBuf,
) -> Thread {
    let created_at = entry
        .map(|item| parse_rfc3339_timestamp_seconds(&item.created_at))
        .unwrap_or_default();
    let updated_at = entry
        .map(|item| parse_rfc3339_timestamp_seconds(&item.last_event_at))
        .unwrap_or(created_at);

    Thread {
        id: thread_id.to_string(),
        preview: entry
            .and_then(|item| item.last_user_snippet.clone())
            .unwrap_or_default(),
        model_provider: entry
            .and_then(|item| item.model_provider.clone())
            .unwrap_or_else(|| config.model_provider_id.clone()),
        created_at,
        updated_at,
        path: Some(rollout_path),
        cwd: entry
            .map(|item| item.cwd_real.clone())
            .unwrap_or_else(|| config.cwd.clone()),
        cli_version: env!("CARGO_PKG_VERSION").to_string(),
        source: entry
            .map(|item| item.session_source.clone().into())
            .unwrap_or(code_app_server_protocol::SessionSource::AppServer),
        git_info: entry.map(|item| code_app_server_protocol::GitInfo {
            sha: None,
            branch: item.git_branch.clone(),
            origin_url: None,
        }),
        turns: Vec::new(),
    }
}

fn model_picker_auth_state(
    config: &Config,
    auth_manager: &AuthManager,
) -> (Option<AuthMode>, bool) {
    let preferred_auth_mode = if config.using_chatgpt_auth {
        AuthMode::Chatgpt
    } else {
        AuthMode::ApiKey
    };
    let auth_mode = auth_manager
        .auth()
        .map(|auth| auth.mode)
        .or(Some(preferred_auth_mode));
    let supports_pro_only_models = auth_manager.supports_pro_only_models();
    (auth_mode, supports_pro_only_models)
}

fn model_preset_to_v2_model(preset: code_common::model_presets::ModelPreset) -> V2Model {
    V2Model {
        id: preset.id.clone(),
        model: preset.model,
        upgrade: preset.upgrade.as_ref().map(|upgrade| upgrade.id.clone()),
        upgrade_info: preset.upgrade.map(|upgrade| ModelUpgradeInfo {
            model: upgrade.id,
            upgrade_copy: None,
            model_link: None,
            migration_markdown: None,
        }),
        availability_nux: None,
        display_name: preset.display_name,
        description: preset.description,
        hidden: !preset.show_in_picker,
        supported_reasoning_efforts: preset
            .supported_reasoning_efforts
            .into_iter()
            .map(|preset| ReasoningEffortOption {
                reasoning_effort: preset.effort.into(),
                description: preset.description,
            })
            .collect(),
        default_reasoning_effort: preset.default_reasoning_effort.into(),
        input_modalities: code_protocol::openai_models::default_input_modalities(),
        supports_personality: false,
        is_default: preset.is_default,
    }
}

fn mark_default_model(models: &mut [V2Model]) {
    for model in models.iter_mut() {
        model.is_default = false;
    }
    if let Some(model) = models.iter_mut().find(|model| !model.hidden) {
        model.is_default = true;
    } else if let Some(model) = models.first_mut() {
        model.is_default = true;
    }
}

fn review_request_from_target(
    target: V2ReviewTarget,
) -> Result<(core_protocol::ReviewRequest, String), JSONRPCErrorError> {
    fn invalid_request(message: String) -> JSONRPCErrorError {
        JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message,
            data: None,
        }
    }

    let (target, prompt, hint) = match target {
        V2ReviewTarget::UncommittedChanges => (
            CoreReviewTarget::UncommittedChanges,
            "Review the current workspace changes and highlight bugs, regressions, risky patterns, and missing tests before merge.".to_string(),
            "current workspace changes".to_string(),
        ),
        V2ReviewTarget::BaseBranch { branch } => {
            let branch = branch.trim().to_string();
            if branch.is_empty() {
                return Err(invalid_request("branch must not be empty".to_string()));
            }
            (
                CoreReviewTarget::BaseBranch {
                    branch: branch.clone(),
                },
                format!(
                    "Review the changes between the current branch and base branch {branch} and highlight bugs, regressions, risky patterns, and missing tests before merge."
                ),
                format!("base branch {branch}"),
            )
        }
        V2ReviewTarget::Commit { sha, title } => {
            let sha = sha.trim().to_string();
            if sha.is_empty() {
                return Err(invalid_request("sha must not be empty".to_string()));
            }
            let title = title.map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
            let short_sha: String = sha.chars().take(12).collect();
            let prompt = match title.as_deref() {
                Some(title) => format!(
                    "Review the changes introduced by commit {sha} ({title}) and highlight bugs, regressions, risky patterns, and missing tests before merge."
                ),
                None => format!(
                    "Review the changes introduced by commit {sha} and highlight bugs, regressions, risky patterns, and missing tests before merge."
                ),
            };
            (
                CoreReviewTarget::Commit { sha, title },
                prompt,
                format!("commit {short_sha}"),
            )
        }
        V2ReviewTarget::Custom { instructions } => {
            let instructions = instructions.trim().to_string();
            if instructions.is_empty() {
                return Err(invalid_request("instructions must not be empty".to_string()));
            }
            (
                CoreReviewTarget::Custom {
                    instructions: instructions.clone(),
                },
                instructions.clone(),
                instructions,
            )
        }
    };

    Ok((
        core_protocol::ReviewRequest {
            target,
            user_facing_hint: Some(hint.clone()),
            prompt,
        },
        hint,
    ))
}

fn build_review_turn(turn_id: String, display_text: &str) -> Turn {
    let items = if display_text.is_empty() {
        Vec::new()
    } else {
        vec![ThreadItem::UserMessage {
            id: turn_id.clone(),
            content: vec![V2UserInput::Text {
                text: display_text.to_string(),
                text_elements: Vec::new(),
            }],
        }]
    };

    Turn {
        id: turn_id,
        items,
        error: None,
        status: TurnStatus::InProgress,
    }
}

impl CodexMessageProcessor {
    // Minimal compatibility layer: translate SendUserTurn into our current
    // flow by submitting only the user items. We intentionally do not attempt
    // per‑turn reconfiguration here (model, cwd, approval, sandbox) to avoid
    // destabilizing the session. This preserves behavior and acks the request
    // so clients using the new method continue to function.
    async fn send_user_turn_compat(
        &self,
        request_id: RequestId,
        params: SendUserTurnParams,
    ) {
        let SendUserTurnParams {
            conversation_id,
            items,
            ..
        } = params;
        let conversation_id = self.resolve_conversation_id_alias(conversation_id).await;

        let Ok(conversation) = self
            .conversation_manager
            .get_conversation(conversation_id)
            .await
        else {
            let error = JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: format!("conversation not found: {conversation_id}"),
                data: None,
            };
            self.outgoing.send_error(request_id, error).await;
            return;
        };

        // Map wire input items into core protocol items.
        let mapped_items: Vec<CoreInputItem> = items
            .into_iter()
            .map(|item| match item {
                WireInputItem::Text { text } => CoreInputItem::Text { text },
                WireInputItem::Image { image_url } => CoreInputItem::Image { image_url },
                WireInputItem::LocalImage { path } => CoreInputItem::LocalImage { path },
            })
            .collect();

        // Submit user input to the conversation.
        let _ = conversation
            .submit(Op::UserInput {
                items: mapped_items,
                final_output_json_schema: None,
            })
            .await;

        // Acknowledge.
        self.outgoing.send_response(request_id, SendUserTurnResponse {}).await;
    }
}

async fn apply_bespoke_event_handling(
    event: Event,
    conversation_id: ConversationId,
    owner_connection_id: ConnectionId,
    conversation: Arc<CodexConversation>,
    outgoing: Arc<OutgoingMessageSender>,
    _pending_interrupts: Arc<Mutex<HashMap<Uuid, Vec<RequestId>>>>,
) {
    let Event { id: _event_id, msg, .. } = event;
    match msg {
        EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
            call_id,
            changes,
            reason,
            grant_root,
        }) => {
            // Map core FileChange to wire FileChange
            let file_changes: HashMap<PathBuf, code_protocol::protocol::FileChange> = changes
                .into_iter()
                .map(|(p, c)| {
                    let mapped = match c {
                        code_core::protocol::FileChange::Add { content } => {
                            code_protocol::protocol::FileChange::Add { content }
                        }
                        code_core::protocol::FileChange::Delete => {
                            code_protocol::protocol::FileChange::Delete { content: String::new() }
                        }
                        code_core::protocol::FileChange::Update {
                            unified_diff,
                            move_path,
                            original_content: _,
                            new_content: _,
                        } => {
                            code_protocol::protocol::FileChange::Update {
                                unified_diff,
                                move_path,
                            }
                        }
                    };
                    (p, mapped)
                })
                .collect();

            let params = ApplyPatchApprovalParams {
                conversation_id,
                call_id: call_id.clone(),
                file_changes,
                reason,
                grant_root,
            };
            let value = serde_json::to_value(&params).unwrap_or_default();
            let rx = outgoing
                .send_request_to_connection(owner_connection_id, APPLY_PATCH_APPROVAL_METHOD, Some(value))
                .await;
            // TODO(mbolin): Enforce a timeout so this task does not live indefinitely?
            let approval_id = call_id.clone(); // correlate by call_id, not event_id
            tokio::spawn(async move {
                on_patch_approval_response(approval_id, rx, conversation).await;
            });
        }
        EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
            call_id,
            approval_id,
            turn_id,
            command,
            cwd,
            reason,
            network_approval_context: _,
            additional_permissions,
        }) => {
            let effective_approval_id = approval_id.clone().unwrap_or_else(|| call_id.clone());
            let params = ExecCommandApprovalParams {
                conversation_id,
                call_id: call_id.clone(),
                approval_id,
                command,
                cwd,
                reason,
                additional_permissions,
            };
            let value = serde_json::to_value(&params).unwrap_or_default();
            let rx = outgoing
                .send_request_to_connection(owner_connection_id, EXEC_COMMAND_APPROVAL_METHOD, Some(value))
                .await;

            // TODO(mbolin): Enforce a timeout so this task does not live indefinitely?
            let approval_id = effective_approval_id; // correlate by approval_id/call_id, not event_id
            tokio::spawn(async move {
                on_exec_approval_response(approval_id, Some(turn_id), rx, conversation).await;
            });
        }
        EventMsg::DynamicToolCallRequest(request) => {
            let call_id = request.call_id;
            let params = DynamicToolCallParams {
                conversation_id,
                turn_id: request.turn_id,
                call_id: call_id.clone(),
                tool: request.tool,
                arguments: request.arguments,
            };
            let value = serde_json::to_value(&params).unwrap_or_default();
            let rx = outgoing
                .send_request_to_connection(owner_connection_id, DYNAMIC_TOOL_CALL_METHOD, Some(value))
                .await;

            tokio::spawn(async move {
                on_dynamic_tool_call_response(call_id, rx, conversation).await;
            });
        }
        EventMsg::RequestUserInput(request) => {
            let request_turn_id = request.turn_id;
            let params = ToolRequestUserInputParams {
                thread_id: conversation_id.to_string(),
                turn_id: request_turn_id.clone(),
                item_id: request.call_id,
                questions: request
                    .questions
                    .into_iter()
                    .map(|question| ToolRequestUserInputQuestion {
                        id: question.id,
                        header: question.header,
                        question: question.question,
                        is_other: question.is_other,
                        is_secret: question.is_secret,
                        options: question.options.map(|options| {
                            options
                                .into_iter()
                                .map(|option| ToolRequestUserInputOption {
                                    label: option.label,
                                    description: option.description,
                                })
                                .collect()
                        }),
                    })
                    .collect(),
            };
            let value = serde_json::to_value(&params).unwrap_or_default();
            let rx = outgoing
                .send_request_to_connection(
                    owner_connection_id,
                    TOOL_REQUEST_USER_INPUT_METHOD,
                    Some(value),
                )
                .await;

            tokio::spawn(async move {
                on_request_user_input_response(request_turn_id, rx, conversation).await;
            });
        }
        // No special handling needed for interrupts; responses are sent immediately.

        _ => {}
    }
}

fn derive_config_from_params(
    params: NewConversationParams,
    model_provider: Option<String>,
    code_linux_sandbox_exe: Option<PathBuf>,
) -> std::io::Result<Config> {
    let NewConversationParams {
        model,
        profile,
        cwd,
        approval_policy,
        sandbox: sandbox_mode,
        config: cli_overrides,
        base_instructions,
        include_plan_tool,
        dynamic_tools,
        ..
    } = params;
    let overrides = ConfigOverrides {
        model,
        review_model: None,
        config_profile: profile,
        cwd: cwd.map(PathBuf::from),
        approval_policy: approval_policy.map(map_ask_for_approval_from_wire),
        sandbox_mode,
        model_provider,
        code_linux_sandbox_exe,
        base_instructions,
        include_plan_tool,
        include_apply_patch_tool: None,
        include_view_image_tool: None,
        disable_response_storage: None,
        show_raw_agent_reasoning: None,
        debug: None,
        tools_web_search_request: None,
        mcp_servers: None,
        experimental_client_tools: None,
        dynamic_tools,
        compact_prompt_override: None,
        compact_prompt_override_file: None,
    };

    let cli_overrides = cli_overrides
        .unwrap_or_default()
        .into_iter()
        .map(|(k, v)| (k, json_to_toml(v)))
        .collect();

    Config::load_with_cli_overrides(cli_overrides, overrides)
}

async fn on_patch_approval_response(
    approval_id: String,
    receiver: tokio::sync::oneshot::Receiver<mcp_types::Result>,
    codex: Arc<CodexConversation>,
) {
    let response = receiver.await;
    let value = match response {
        Ok(value) => value,
        Err(err) => {
            error!("request failed: {err:?}");
            if let Err(submit_err) = codex
                .submit(Op::PatchApproval {
                    id: approval_id.clone(),
                    decision: core_protocol::ReviewDecision::Denied,
                })
                .await
            {
                error!("failed to submit denied PatchApproval after request failure: {submit_err}");
            }
            return;
        }
    };

    let response =
        serde_json::from_value::<ApplyPatchApprovalResponse>(value).unwrap_or_else(|err| {
            error!("failed to deserialize ApplyPatchApprovalResponse: {err}");
            ApplyPatchApprovalResponse {
                decision: ReviewDecision::Denied,
            }
        });

    if let Err(err) = codex
        .submit(Op::PatchApproval {
            id: approval_id,
            decision: map_review_decision_from_wire(response.decision),
        })
        .await
    {
        error!("failed to submit PatchApproval: {err}");
    }
}

async fn on_dynamic_tool_call_response(
    call_id: String,
    receiver: tokio::sync::oneshot::Receiver<mcp_types::Result>,
    conversation: Arc<CodexConversation>,
) {
    let response = receiver.await;
    let value = match response {
        Ok(value) => value,
        Err(err) => {
            error!("request failed: {err:?}");
            let fallback = CoreDynamicToolResponse {
                content_items: vec![code_protocol::dynamic_tools::DynamicToolCallOutputContentItem::InputText {
                    text: "dynamic tool request failed".to_string(),
                }],
                success: false,
            };
            if let Err(err) = conversation
                .submit(Op::DynamicToolResponse {
                    id: call_id.clone(),
                    response: fallback,
                })
                .await
            {
                error!("failed to submit DynamicToolResponse: {err}");
            }
            return;
        }
    };

    let response = serde_json::from_value::<DynamicToolCallResponse>(value).unwrap_or_else(|err| {
        error!("failed to deserialize DynamicToolCallResponse: {err}");
        DynamicToolCallResponse {
            output: "dynamic tool response was invalid".to_string(),
            success: false,
        }
    });

    let response = CoreDynamicToolResponse {
        content_items: vec![
            code_protocol::dynamic_tools::DynamicToolCallOutputContentItem::InputText {
                text: response.output,
            },
        ],
        success: response.success,
    };
    if let Err(err) = conversation
        .submit(Op::DynamicToolResponse {
            id: call_id,
            response,
        })
        .await
    {
        error!("failed to submit DynamicToolResponse: {err}");
    }
}

async fn on_request_user_input_response(
    turn_id: String,
    receiver: tokio::sync::oneshot::Receiver<mcp_types::Result>,
    conversation: Arc<CodexConversation>,
) {
    let response = receiver.await;
    let value = match response {
        Ok(value) => value,
        Err(err) => {
            error!("request failed: {err:?}");
            let empty = RequestUserInputResponse {
                answers: HashMap::new(),
            };
            if let Err(err) = conversation
                .submit(Op::UserInputAnswer {
                    id: turn_id,
                    response: empty,
                })
                .await
            {
                error!("failed to submit UserInputAnswer: {err}");
            }
            return;
        }
    };

    let response =
        serde_json::from_value::<ToolRequestUserInputResponse>(value).unwrap_or_else(|err| {
            error!("failed to deserialize ToolRequestUserInputResponse: {err}");
            ToolRequestUserInputResponse {
                answers: HashMap::new(),
            }
        });

    let response = map_tool_request_user_input_response(response);
    if let Err(err) = conversation
        .submit(Op::UserInputAnswer {
            id: turn_id,
            response,
        })
        .await
    {
        error!("failed to submit UserInputAnswer: {err}");
    }
}

fn map_tool_request_user_input_response(
    response: ToolRequestUserInputResponse,
) -> RequestUserInputResponse {
    RequestUserInputResponse {
        answers: response
            .answers
            .into_iter()
            .map(|(id, answer)| {
                (
                    id,
                    RequestUserInputAnswer {
                        answers: answer.answers,
                    },
                )
            })
            .collect(),
    }
}

async fn on_exec_approval_response(
    approval_id: String,
    approval_turn_id: Option<String>,
    receiver: tokio::sync::oneshot::Receiver<mcp_types::Result>,
    conversation: Arc<CodexConversation>,
) {
    let response = receiver.await;
    let value = match response {
        Ok(value) => value,
        Err(err) => {
            tracing::error!("request failed: {err:?}");
            // When the owning connection disconnects, callbacks are dropped.
            // Submit a conservative deny so the run can progress.
            if let Err(submit_err) = conversation
                .submit(Op::ExecApproval {
                    id: approval_id.clone(),
                    turn_id: approval_turn_id.clone(),
                    decision: core_protocol::ReviewDecision::Denied,
                })
                .await
            {
                error!("failed to submit denied ExecApproval after request failure: {submit_err}");
            }
            return;
        }
    };

    // Try to deserialize `value` and then make the appropriate call to `codex`.
    let response =
        serde_json::from_value::<ExecCommandApprovalResponse>(value).unwrap_or_else(|err| {
            error!("failed to deserialize ExecCommandApprovalResponse: {err}");
            // If we cannot deserialize the response, we deny the request to be
            // conservative.
            ExecCommandApprovalResponse {
                decision: ReviewDecision::Denied,
            }
        });

    if let Err(err) = conversation
        .submit(Op::ExecApproval {
            id: approval_id,
            turn_id: approval_turn_id,
            decision: map_review_decision_from_wire(response.decision),
        })
        .await
    {
        error!("failed to submit ExecApproval: {err}");
    }
}

fn map_review_decision_from_wire(d: code_protocol::protocol::ReviewDecision) -> core_protocol::ReviewDecision {
    match d {
        code_protocol::protocol::ReviewDecision::Approved => core_protocol::ReviewDecision::Approved,
        code_protocol::protocol::ReviewDecision::ApprovedExecpolicyAmendment { .. } => {
            core_protocol::ReviewDecision::Approved
        }
        code_protocol::protocol::ReviewDecision::ApprovedForSession => core_protocol::ReviewDecision::ApprovedForSession,
        code_protocol::protocol::ReviewDecision::Denied => core_protocol::ReviewDecision::Denied,
        code_protocol::protocol::ReviewDecision::Abort => core_protocol::ReviewDecision::Abort,
    }
}

trait IntoWireAuthMode {
    fn into_wire(self) -> code_protocol::mcp_protocol::AuthMode;
}

impl IntoWireAuthMode for code_app_server_protocol::AuthMode {
    fn into_wire(self) -> code_protocol::mcp_protocol::AuthMode {
        match self {
            code_app_server_protocol::AuthMode::ApiKey => {
                code_protocol::mcp_protocol::AuthMode::ApiKey
            }
            code_app_server_protocol::AuthMode::Chatgpt => {
                code_protocol::mcp_protocol::AuthMode::ChatGPT
            }
            code_app_server_protocol::AuthMode::ChatgptAuthTokens => {
                code_protocol::mcp_protocol::AuthMode::ChatgptAuthTokens
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_app_server_protocol::AuthMode;
    use code_app_server_protocol::ReviewDelivery;
    use code_app_server_protocol::ReviewTarget;
    use code_core::auth::CodexAuth;
    use code_core::auth::RefreshTokenError;
    use code_core::config::ConfigOverrides;
    use code_core::SessionIndexEntry;
    use code_protocol::mcp_protocol::RemoveConversationListenerParams;
    use code_protocol::protocol::SessionSource;
    use mcp_types::RequestId;
    use serde_json::from_value;
    use tokio::sync::mpsc;

    fn make_processor_for_tests() -> (CodexMessageProcessor, mpsc::UnboundedReceiver<crate::outgoing_message::OutgoingMessage>) {
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();
        let outgoing = Arc::new(OutgoingMessageSender::new(outgoing_tx));
        let config = Arc::new(
            Config::load_with_cli_overrides(Vec::new(), ConfigOverrides::default())
                .expect("load default config"),
        );
        let auth_manager = AuthManager::shared_with_mode_and_originator(
            config.code_home.clone(),
            AuthMode::ApiKey,
            config.responses_originator_header.clone(),
        );
        let conversation_manager = Arc::new(ConversationManager::new(
            auth_manager.clone(),
            SessionSource::Mcp,
        ));

        (
            CodexMessageProcessor::new(
                auth_manager,
                conversation_manager,
                outgoing,
                None,
                config,
            ),
            outgoing_rx,
        )
    }

    fn make_processor_with_auth_for_tests(
        auth_manager: Arc<AuthManager>,
    ) -> (
        CodexMessageProcessor,
        mpsc::UnboundedReceiver<crate::outgoing_message::OutgoingMessage>,
    ) {
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();
        let outgoing = Arc::new(OutgoingMessageSender::new(outgoing_tx));
        let config = Arc::new(
            Config::load_with_cli_overrides(Vec::new(), ConfigOverrides::default())
                .expect("load default config"),
        );
        let conversation_manager = Arc::new(ConversationManager::new(
            auth_manager.clone(),
            SessionSource::Mcp,
        ));

        (
            CodexMessageProcessor::new(
                auth_manager,
                conversation_manager,
                outgoing,
                None,
                config,
            ),
            outgoing_rx,
        )
    }

    async fn expect_error_message(
        outgoing_rx: &mut mpsc::UnboundedReceiver<crate::outgoing_message::OutgoingMessage>,
    ) -> String {
        let message = outgoing_rx.recv().await.expect("error response should be sent");
        match message {
            crate::outgoing_message::OutgoingMessage::Error(err) => err.error.message,
            other => panic!("expected error response, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn remove_conversation_listener_enforces_owner_connection() {
        let (mut processor, mut outgoing_rx) = make_processor_for_tests();

        let subscription_id = Uuid::new_v4();
        let (cancel_tx, mut cancel_rx) = oneshot::channel();
        processor.conversation_listeners.insert(
            subscription_id,
            ConversationListenerRegistration {
                owner_connection_id: ConnectionId(1),
                cancel_tx,
            },
        );

        processor
            .remove_conversation_listener(
                ConnectionId(2),
                RequestId::Integer(10),
                RemoveConversationListenerParams { subscription_id },
            )
            .await;

        let message = outgoing_rx
            .recv()
            .await
            .expect("error response should be sent");
        match message {
            crate::outgoing_message::OutgoingMessage::Error(err) => {
                assert_eq!(err.id, RequestId::Integer(10));
                assert!(err.error.message.contains("subscription not found"));
            }
            _ => panic!("expected error response"),
        }

        assert!(
            processor.conversation_listeners.contains_key(&subscription_id),
            "listener should remain registered for original owner"
        );

        processor
            .remove_conversation_listener(
                ConnectionId(1),
                RequestId::Integer(11),
                RemoveConversationListenerParams { subscription_id },
            )
            .await;

        let message = outgoing_rx
            .recv()
            .await
            .expect("success response should be sent");
        match message {
            crate::outgoing_message::OutgoingMessage::Response(response) => {
                assert_eq!(response.id, RequestId::Integer(11));
            }
            _ => panic!("expected success response"),
        }

        assert!(
            processor.conversation_listeners.get(&subscription_id).is_none(),
            "listener should be removed by owner"
        );
        assert_eq!(cancel_rx.try_recv(), Ok(()));
    }

    #[tokio::test]
    async fn get_auth_status_omits_token_after_permanent_refresh_failure() {
        let auth = CodexAuth::create_dummy_chatgpt_auth_for_testing();
        let auth_manager = AuthManager::from_auth_for_testing(auth.clone());
        auth_manager.seed_refresh_failure_for_testing(
            &auth,
            RefreshTokenError::permanent("refresh token already used"),
        );

        let (processor, mut outgoing_rx) = make_processor_with_auth_for_tests(auth_manager);
        processor
            .get_auth_status(
                RequestId::Integer(42),
                GetAuthStatusParams {
                    include_token: Some(true),
                    refresh_token: Some(false),
                },
            )
            .await;

        let message = outgoing_rx
            .recv()
            .await
            .expect("auth status response should be sent");
        let response = match message {
            crate::outgoing_message::OutgoingMessage::Response(response) => response,
            _ => panic!("expected response message"),
        };
        let status: GetAuthStatusResponse =
            from_value(response.result).expect("valid getAuthStatus payload");
        assert_eq!(
            status,
            GetAuthStatusResponse {
                auth_method: Some(code_protocol::mcp_protocol::AuthMode::ChatGPT),
                auth_token: None,
                requires_openai_auth: Some(true),
            }
        );
    }

    #[tokio::test]
    async fn thread_resume_v2_rejects_history_even_with_path() {
        let (processor, mut outgoing_rx) = make_processor_for_tests();

        processor
            .thread_resume_v2(
                RequestId::Integer(7),
                ThreadResumeParams {
                    thread_id: Uuid::new_v4().to_string(),
                    history: Some(Vec::new()),
                    path: Some(std::path::PathBuf::from("/tmp/rollout.jsonl")),
                    model: None,
                    model_provider: None,
                    cwd: None,
                    approval_policy: None,
                    sandbox: None,
                    config: None,
                    base_instructions: None,
                    developer_instructions: None,
                    personality: None,
                },
            )
            .await;

        let message = expect_error_message(&mut outgoing_rx).await;
        assert_eq!(
            message,
            "thread/resume.history is not supported by the Every Code app-server"
        );
    }

    #[tokio::test]
    async fn review_start_v2_detached_rejects_unknown_source_thread() {
        let (processor, mut outgoing_rx) = make_processor_for_tests();
        let unknown_thread_id = Uuid::new_v4().to_string();

        processor
            .review_start_v2(
                RequestId::Integer(8),
                ReviewStartParams {
                    thread_id: unknown_thread_id.clone(),
                    target: ReviewTarget::UncommittedChanges,
                    delivery: Some(ReviewDelivery::Detached),
                },
            )
            .await;

        let message = expect_error_message(&mut outgoing_rx).await;
        assert_eq!(message, format!("thread not found: {unknown_thread_id}"));
    }

    #[test]
    fn parse_plan_type_is_case_insensitive() {
        assert_eq!(parse_plan_type(Some("Pro".to_string())), PlanType::Pro);
        assert_eq!(
            parse_plan_type(Some("BUSINESS".to_string())),
            PlanType::Business
        );
        assert_eq!(parse_plan_type(Some("mystery".to_string())), PlanType::Unknown);
        assert_eq!(parse_plan_type(None), PlanType::Unknown);
    }

    #[test]
    fn select_rate_limit_snapshot_prefers_matching_account() {
        let snapshots = vec![
            code_core::account_usage::StoredRateLimitSnapshot {
                account_id: "acct-a".to_string(),
                plan: Some("pro".to_string()),
                snapshot: None,
                observed_at: None,
                primary_next_reset_at: None,
                secondary_next_reset_at: None,
                last_usage_limit_hit_at: None,
            },
            code_core::account_usage::StoredRateLimitSnapshot {
                account_id: "acct-b".to_string(),
                plan: Some("plus".to_string()),
                snapshot: None,
                observed_at: None,
                primary_next_reset_at: None,
                secondary_next_reset_at: None,
                last_usage_limit_hit_at: None,
            },
        ];

        let selected = select_rate_limit_snapshot(Some("acct-b".to_string()), snapshots)
            .expect("snapshot should be selected");
        assert_eq!(selected.account_id, "acct-b");
    }

    #[test]
    fn rate_limit_snapshot_from_event_maps_windows() {
        let event = code_core::protocol::RateLimitSnapshotEvent {
            primary_used_percent: 11.0,
            secondary_used_percent: 22.0,
            primary_to_secondary_ratio_percent: 50.0,
            primary_window_minutes: 60,
            secondary_window_minutes: 1440,
            primary_reset_after_seconds: Some(12),
            secondary_reset_after_seconds: Some(34),
        };

        let snapshot = rate_limit_snapshot_from_event(&event, Some(PlanType::Pro));
        assert_eq!(snapshot.plan_type, Some(PlanType::Pro));
        assert_eq!(
            snapshot.primary.as_ref().and_then(|window| window.window_minutes),
            Some(60)
        );
        assert_eq!(
            snapshot
                .secondary
                .as_ref()
                .and_then(|window| window.window_minutes),
            Some(1440)
        );
    }

    #[test]
    fn map_tool_request_user_input_response_preserves_answers() {
        let response = ToolRequestUserInputResponse {
            answers: std::collections::HashMap::from([(
                "question_id".to_string(),
                code_app_server_protocol::ToolRequestUserInputAnswer {
                    answers: vec!["selected".to_string()],
                },
            )]),
        };

        let mapped = map_tool_request_user_input_response(response);
        assert_eq!(
            mapped
                .answers
                .get("question_id")
                .expect("question_id should exist")
                .answers,
            vec!["selected".to_string()]
        );
    }

    #[test]
    fn thread_resume_response_thread_uses_catalog_metadata() {
        let config =
            Config::load_with_cli_overrides(Vec::new(), ConfigOverrides::default())
                .expect("load default config");
        let entry = SessionIndexEntry {
            session_id: Uuid::new_v4(),
            rollout_path: std::path::PathBuf::from("sessions/test.jsonl"),
            snapshot_path: None,
            created_at: "2026-04-03T10:00:00.000Z".to_string(),
            last_event_at: "2026-04-03T10:05:00.000Z".to_string(),
            cwd_real: std::path::PathBuf::from("/tmp/test-thread"),
            cwd_display: "/tmp/test-thread".to_string(),
            git_project_root: None,
            git_branch: Some("main".to_string()),
            model_provider: Some("openai".to_string()),
            session_source: SessionSource::Mcp,
            message_count: 3,
            user_message_count: 1,
            last_user_snippet: Some("resume me".to_string()),
            nickname: None,
            sync_origin_device: None,
            sync_version: 0,
            archived: false,
            deleted: false,
        };

        let thread = thread_resume_response_thread(
            &entry.session_id.to_string(),
            Some(&entry),
            &config,
            std::path::PathBuf::from("/tmp/test.jsonl"),
        );

        assert_eq!(thread.id, entry.session_id.to_string());
        assert_eq!(thread.preview, "resume me");
        assert_eq!(thread.model_provider, "openai");
        assert_eq!(thread.cwd, std::path::PathBuf::from("/tmp/test-thread"));
        assert_eq!(thread.path, Some(std::path::PathBuf::from("/tmp/test.jsonl")));
        assert_eq!(thread.source, code_app_server_protocol::SessionSource::AppServer);
        assert_eq!(thread.git_info.and_then(|info| info.branch), Some("main".to_string()));
        assert_eq!(thread.created_at, 1_775_210_400);
        assert_eq!(thread.updated_at, 1_775_210_700);
    }

    #[test]
    fn thread_resume_rollout_path_prefers_explicit_path() {
        let explicit_path = std::path::PathBuf::from("/tmp/explicit.jsonl");
        let catalog_path = std::path::PathBuf::from("/tmp/catalog.jsonl");

        let rollout_path =
            thread_resume_rollout_path(Some(explicit_path.clone()), Some(catalog_path));

        assert_eq!(rollout_path, Some(explicit_path));
    }

    #[test]
    fn thread_resume_skips_catalog_lookup_when_path_is_explicit() {
        assert!(!thread_resume_should_lookup_catalog(Some(std::path::Path::new(
            "/tmp/explicit.jsonl",
        ))));
        assert!(thread_resume_should_lookup_catalog(None));
    }

    #[test]
    fn thread_resume_does_not_record_alias_for_explicit_path() {
        let requested_conversation_id = ConversationId::from_string(
            "11111111-1111-4111-8111-111111111111",
        )
        .expect("valid uuid");
        let resumed_conversation_id = ConversationId::from_string(
            "22222222-2222-4222-8222-222222222222",
        )
        .expect("valid uuid");

        assert!(!thread_resume_should_record_alias(
            Some(std::path::Path::new("/tmp/explicit.jsonl")),
            &requested_conversation_id,
            &resumed_conversation_id,
        ));
    }

    #[test]
    fn thread_resume_records_alias_for_thread_id_resume() {
        let requested_conversation_id = ConversationId::from_string(
            "11111111-1111-4111-8111-111111111111",
        )
        .expect("valid uuid");
        let resumed_conversation_id = ConversationId::from_string(
            "22222222-2222-4222-8222-222222222222",
        )
        .expect("valid uuid");

        assert!(thread_resume_should_record_alias(
            None,
            &requested_conversation_id,
            &resumed_conversation_id,
        ));
        assert!(!thread_resume_should_record_alias(
            None,
            &resumed_conversation_id,
            &resumed_conversation_id,
        ));
    }

    #[test]
    fn thread_resume_canonical_thread_id_prefers_rollout_path() {
        let resumed_conversation_id = ConversationId::from_string(
            "33333333-3333-4333-8333-333333333333",
        )
        .expect("valid uuid");
        let entry = SessionIndexEntry {
            session_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111")
                .expect("valid uuid"),
            rollout_path: std::path::PathBuf::from("sessions/wrong.jsonl"),
            snapshot_path: None,
            created_at: "2026-04-03T10:00:00.000Z".to_string(),
            last_event_at: "2026-04-03T10:05:00.000Z".to_string(),
            cwd_real: std::path::PathBuf::from("/tmp/test-thread"),
            cwd_display: "/tmp/test-thread".to_string(),
            git_project_root: None,
            git_branch: Some("main".to_string()),
            model_provider: Some("openai".to_string()),
            session_source: SessionSource::Mcp,
            message_count: 3,
            user_message_count: 1,
            last_user_snippet: Some("resume me".to_string()),
            nickname: None,
            sync_origin_device: None,
            sync_version: 0,
            archived: false,
            deleted: false,
        };

        let canonical_thread_id = thread_resume_canonical_thread_id(
            resumed_conversation_id,
            std::path::Path::new(
                "/tmp/rollout-2026-04-03T09-10-00Z-22222222-2222-4222-8222-222222222222.jsonl",
            ),
            Some(&entry),
        );

        assert_eq!(canonical_thread_id, "22222222-2222-4222-8222-222222222222");
    }

    #[test]
    fn thread_resume_canonical_thread_id_falls_back_to_resumed_conversation() {
        let resumed_conversation_id = ConversationId::from_string(
            "33333333-3333-4333-8333-333333333333",
        )
        .expect("valid uuid");

        let canonical_thread_id = thread_resume_canonical_thread_id(
            resumed_conversation_id,
            std::path::Path::new("/tmp/rollout-without-uuid.jsonl"),
            None,
        );

        assert_eq!(canonical_thread_id, "33333333-3333-4333-8333-333333333333");
    }

    #[test]
    fn derive_config_from_params_applies_model_provider_override() {
        let params = NewConversationParams {
            model: None,
            profile: None,
            cwd: None,
            approval_policy: None,
            sandbox: None,
            config: None,
            base_instructions: None,
            include_plan_tool: None,
            include_apply_patch_tool: None,
            dynamic_tools: None,
        };

        let config = derive_config_from_params(params, Some("oss".to_string()), None)
            .expect("derive config with model provider override");

        assert_eq!(config.model_provider_id, "oss");
    }

    #[test]
    fn mark_default_model_prefers_visible_models() {
        let mut models = vec![
            V2Model {
                id: "hidden".to_string(),
                model: "hidden".to_string(),
                upgrade: None,
                upgrade_info: None,
                availability_nux: None,
                display_name: "Hidden".to_string(),
                description: String::new(),
                hidden: true,
                supported_reasoning_efforts: Vec::new(),
                default_reasoning_effort: code_protocol::config_types::ReasoningEffort::Minimal,
                input_modalities: code_protocol::openai_models::default_input_modalities(),
                supports_personality: false,
                is_default: true,
            },
            V2Model {
                id: "visible".to_string(),
                model: "visible".to_string(),
                upgrade: None,
                upgrade_info: None,
                availability_nux: None,
                display_name: "Visible".to_string(),
                description: String::new(),
                hidden: false,
                supported_reasoning_efforts: Vec::new(),
                default_reasoning_effort: code_protocol::config_types::ReasoningEffort::Minimal,
                input_modalities: code_protocol::openai_models::default_input_modalities(),
                supports_personality: false,
                is_default: false,
            },
        ];

        mark_default_model(&mut models);

        assert!(!models[0].is_default);
        assert!(models[1].is_default);
    }

    #[test]
    fn review_request_from_target_builds_workspace_prompt() {
        let (request, display_text) = review_request_from_target(V2ReviewTarget::UncommittedChanges)
            .expect("workspace review target should be accepted");

        assert_eq!(display_text, "current workspace changes");
        assert_eq!(
            request.user_facing_hint.as_deref(),
            Some("current workspace changes")
        );
        assert!(request.prompt.contains("current workspace changes"));
    }

    #[test]
    fn review_request_from_target_rejects_empty_custom_instructions() {
        let error = review_request_from_target(V2ReviewTarget::Custom {
            instructions: "   ".to_string(),
        })
        .expect_err("empty instructions should be rejected");

        assert_eq!(error.message, "instructions must not be empty");
    }

    #[test]
    fn conversation_id_from_rollout_path_parses_hyphenated_uuid_suffix() {
        let conversation_id = conversation_id_from_rollout_path(std::path::Path::new(
            "/tmp/rollout-2026-04-03T09-10-00Z-22222222-2222-4222-8222-222222222222.jsonl",
        ))
        .expect("conversation id should parse from rollout path");

        assert_eq!(
            conversation_id.to_string(),
            "22222222-2222-4222-8222-222222222222"
        );
    }
}

impl IntoWireAuthMode for code_protocol::mcp_protocol::AuthMode {
    fn into_wire(self) -> code_protocol::mcp_protocol::AuthMode {
        self
    }
}

fn map_auth_mode_to_wire<M: IntoWireAuthMode>(mode: M) -> code_protocol::mcp_protocol::AuthMode {
    mode.into_wire()
}

fn map_ask_for_approval_from_wire(a: code_protocol::protocol::AskForApproval) -> core_protocol::AskForApproval {
    match a {
        code_protocol::protocol::AskForApproval::UnlessTrusted => core_protocol::AskForApproval::UnlessTrusted,
        code_protocol::protocol::AskForApproval::OnFailure => core_protocol::AskForApproval::OnFailure,
        code_protocol::protocol::AskForApproval::OnRequest => core_protocol::AskForApproval::OnRequest,
        code_protocol::protocol::AskForApproval::Reject(config) => {
            core_protocol::AskForApproval::Reject(core_protocol::RejectConfig {
                sandbox_approval: config.sandbox_approval,
                rules: config.rules,
                skill_approval: config.skill_approval,
                request_permissions: config.request_permissions,
                mcp_elicitations: config.mcp_elicitations,
            })
        }
        code_protocol::protocol::AskForApproval::Never => core_protocol::AskForApproval::Never,
    }
}

fn map_ask_for_approval_to_wire(a: core_protocol::AskForApproval) -> code_protocol::protocol::AskForApproval {
    match a {
        core_protocol::AskForApproval::UnlessTrusted => code_protocol::protocol::AskForApproval::UnlessTrusted,
        core_protocol::AskForApproval::OnFailure => code_protocol::protocol::AskForApproval::OnFailure,
        core_protocol::AskForApproval::OnRequest => code_protocol::protocol::AskForApproval::OnRequest,
        core_protocol::AskForApproval::Reject(config) => {
            code_protocol::protocol::AskForApproval::Reject(code_protocol::protocol::RejectConfig {
                sandbox_approval: config.sandbox_approval,
                rules: config.rules,
                skill_approval: config.skill_approval,
                request_permissions: config.request_permissions,
                mcp_elicitations: config.mcp_elicitations,
            })
        }
        core_protocol::AskForApproval::Never => code_protocol::protocol::AskForApproval::Never,
    }
}

fn map_sandbox_policy_to_wire(
    policy: core_protocol::SandboxPolicy,
) -> code_protocol::protocol::SandboxPolicy {
    match policy {
        core_protocol::SandboxPolicy::DangerFullAccess => {
            code_protocol::protocol::SandboxPolicy::DangerFullAccess
        }
        core_protocol::SandboxPolicy::ReadOnly => code_protocol::protocol::SandboxPolicy::ReadOnly,
        core_protocol::SandboxPolicy::WorkspaceWrite {
            writable_roots,
            network_access,
            exclude_tmpdir_env_var,
            exclude_slash_tmp,
            allow_git_writes,
        } => code_protocol::protocol::SandboxPolicy::WorkspaceWrite {
            writable_roots: writable_roots
                .into_iter()
                .filter_map(|path| {
                    code_utils_absolute_path::AbsolutePathBuf::from_absolute_path(path).ok()
                })
                .collect(),
            network_access,
            exclude_tmpdir_env_var,
            exclude_slash_tmp,
            allow_git_writes,
        },
    }
}

fn map_reasoning_effort_to_wire(
    effort: code_core::config_types::ReasoningEffort,
) -> code_protocol::config_types::ReasoningEffort {
    match effort {
        code_core::config_types::ReasoningEffort::Minimal => {
            code_protocol::config_types::ReasoningEffort::Minimal
        }
        code_core::config_types::ReasoningEffort::Low => code_protocol::config_types::ReasoningEffort::Low,
        code_core::config_types::ReasoningEffort::Medium => {
            code_protocol::config_types::ReasoningEffort::Medium
        }
        code_core::config_types::ReasoningEffort::High => code_protocol::config_types::ReasoningEffort::High,
        code_core::config_types::ReasoningEffort::XHigh => {
            code_protocol::config_types::ReasoningEffort::XHigh
        }
        code_core::config_types::ReasoningEffort::None => {
            code_protocol::config_types::ReasoningEffort::Minimal
        }
    }
}

fn map_reasoning_summary_to_wire(
    summary: code_core::config_types::ReasoningSummary,
) -> code_protocol::config_types::ReasoningSummary {
    match summary {
        code_core::config_types::ReasoningSummary::Auto => code_protocol::config_types::ReasoningSummary::Auto,
        code_core::config_types::ReasoningSummary::Concise => {
            code_protocol::config_types::ReasoningSummary::Concise
        }
        code_core::config_types::ReasoningSummary::Detailed => {
            code_protocol::config_types::ReasoningSummary::Detailed
        }
        code_core::config_types::ReasoningSummary::None => code_protocol::config_types::ReasoningSummary::None,
    }
}

fn map_verbosity_to_wire(
    verbosity: code_core::config_types::TextVerbosity,
) -> code_protocol::config_types::Verbosity {
    match verbosity {
        code_core::config_types::TextVerbosity::Low => code_protocol::config_types::Verbosity::Low,
        code_core::config_types::TextVerbosity::Medium => {
            code_protocol::config_types::Verbosity::Medium
        }
        code_core::config_types::TextVerbosity::High => code_protocol::config_types::Verbosity::High,
    }
}

fn parse_plan_type(plan: Option<String>) -> PlanType {
    let Some(plan) = plan else {
        return PlanType::Unknown;
    };

    match plan.trim().to_ascii_lowercase().as_str() {
        "free" => PlanType::Free,
        "go" => PlanType::Go,
        "plus" => PlanType::Plus,
        "pro" => PlanType::Pro,
        "team" => PlanType::Team,
        "business" => PlanType::Business,
        "enterprise" => PlanType::Enterprise,
        "edu" => PlanType::Edu,
        _ => PlanType::Unknown,
    }
}

fn select_rate_limit_snapshot(
    account_id: Option<String>,
    snapshots: Vec<code_core::account_usage::StoredRateLimitSnapshot>,
) -> Option<code_core::account_usage::StoredRateLimitSnapshot> {
    if snapshots.is_empty() {
        return None;
    }

    if let Some(account_id) = account_id
        && let Some(snapshot) = snapshots
            .iter()
            .find(|snapshot| snapshot.account_id == account_id)
    {
        return Some(snapshot.clone());
    }

    snapshots.into_iter().next()
}

fn rate_limit_snapshot_from_event(
    snapshot: &code_core::protocol::RateLimitSnapshotEvent,
    plan_type: Option<PlanType>,
) -> CoreRateLimitSnapshot {
    let primary = CoreRateLimitWindow {
        used_percent: snapshot.primary_used_percent,
        window_minutes: Some(snapshot.primary_window_minutes),
        resets_in_seconds: snapshot.primary_reset_after_seconds,
        resets_at: None,
    };
    let secondary = CoreRateLimitWindow {
        used_percent: snapshot.secondary_used_percent,
        window_minutes: Some(snapshot.secondary_window_minutes),
        resets_in_seconds: snapshot.secondary_reset_after_seconds,
        resets_at: None,
    };

    CoreRateLimitSnapshot {
        limit_id: None,
        limit_name: None,
        primary: Some(primary),
        secondary: Some(secondary),
        credits: None,
        plan_type,
    }
}

fn conversation_id_from_rollout_path(path: &std::path::Path) -> Option<ConversationId> {
    let stem = path.file_stem()?.to_str()?;

    stem.match_indices('-')
        .rev()
        .find_map(|(index, _)| ConversationId::from_string(&stem[index + 1..]).ok())
}

fn snippet_from_rollout_tail(tail: &[serde_json::Value]) -> Option<String> {
    for value in tail.iter().rev() {
        let item = match serde_json::from_value::<code_protocol::protocol::RolloutItem>(value.clone()) {
            Ok(item) => item,
            Err(_) => continue,
        };
        if let code_protocol::protocol::RolloutItem::ResponseItem(
            code_protocol::models::ResponseItem::Message { role, content, .. },
        ) = item
            && role.eq_ignore_ascii_case("user")
        {
            if let Some(snippet) = snippet_from_content(&content)
                && !snippet.starts_with("== System Status ==")
            {
                return Some(snippet);
            }
        }
    }
    None
}

fn snippet_from_content(content: &[code_protocol::models::ContentItem]) -> Option<String> {
    content.iter().find_map(|item| match item {
        code_protocol::models::ContentItem::InputText { text }
        | code_protocol::models::ContentItem::OutputText { text } => {
            if text.trim().is_empty() {
                None
            } else {
                Some(text.chars().take(100).collect())
            }
        }
        _ => None,
    })
}

// Unused legacy mappers removed to avoid warnings.
