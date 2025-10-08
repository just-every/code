use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use codex_core::AuthManager;
use codex_core::CodexConversation;
use codex_core::ConversationManager;
use codex_core::NewConversation;
use codex_core::config::Config;
use codex_core::config::ConfigOverrides;
use codex_core::git_info::git_diff_to_remote;
use codex_core::protocol::ApplyPatchApprovalRequestEvent;
use codex_core::protocol::Event;
use codex_core::protocol::EventMsg;
use codex_core::protocol::ExecApprovalRequestEvent;
use codex_protocol::protocol::ReviewDecision;
use mcp_types::JSONRPCErrorError;
use mcp_types::RequestId;
use tokio::sync::Mutex;
use tokio::sync::oneshot;
use tracing::error;
use uuid::Uuid;

use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::json_to_toml::json_to_toml;
use crate::outgoing_message::OutgoingMessageSender;
use crate::outgoing_message::OutgoingNotification;
use codex_core::protocol as core_protocol;
use codex_core::protocol::InputItem as CoreInputItem;
use codex_core::protocol::Op;
use codex_protocol::mcp_protocol::APPLY_PATCH_APPROVAL_METHOD;
use codex_protocol::mcp_protocol::AddConversationListenerParams;
use codex_protocol::mcp_protocol::AddConversationSubscriptionResponse;
use codex_protocol::mcp_protocol::ApplyPatchApprovalParams;
use codex_protocol::mcp_protocol::ApplyPatchApprovalResponse;
use codex_protocol::mcp_protocol::ClientRequest;
use codex_protocol::mcp_protocol::ConversationId;
use codex_protocol::mcp_protocol::EXEC_COMMAND_APPROVAL_METHOD;
use codex_protocol::mcp_protocol::ExecCommandApprovalParams;
use codex_protocol::mcp_protocol::ExecCommandApprovalResponse;
use codex_protocol::mcp_protocol::InputItem as WireInputItem;
use codex_protocol::mcp_protocol::InterruptConversationParams;
use codex_protocol::mcp_protocol::InterruptConversationResponse;
use codex_protocol::protocol::TurnAbortReason;
// Unused login-related and diff param imports removed
use codex_protocol::mcp_protocol::GitDiffToRemoteResponse;
use codex_protocol::mcp_protocol::NewConversationParams;
use codex_protocol::mcp_protocol::NewConversationResponse;
use codex_protocol::mcp_protocol::RemoveConversationListenerParams;
use codex_protocol::mcp_protocol::RemoveConversationSubscriptionResponse;
use codex_protocol::mcp_protocol::SendUserMessageParams;
use codex_protocol::mcp_protocol::SendUserMessageResponse;
use codex_protocol::mcp_protocol::SendUserTurnParams;
use codex_protocol::mcp_protocol::SendUserTurnResponse;

// Removed deprecated ChatGPT login support scaffolding

/// Handles JSON-RPC messages for Codex conversations.
pub(crate) struct CodexMessageProcessor {
    _auth_manager: Arc<AuthManager>,
    conversation_manager: Arc<ConversationManager>,
    outgoing: Arc<OutgoingMessageSender>,
    codex_linux_sandbox_exe: Option<PathBuf>,
    _config: Arc<Config>,
    conversation_listeners: HashMap<Uuid, oneshot::Sender<()>>,
    // Queue of pending interrupt requests per conversation. We reply when TurnAborted arrives.
    pending_interrupts: Arc<Mutex<HashMap<Uuid, Vec<RequestId>>>>,
}

impl CodexMessageProcessor {
    pub fn new(
        auth_manager: Arc<AuthManager>,
        conversation_manager: Arc<ConversationManager>,
        outgoing: Arc<OutgoingMessageSender>,
        codex_linux_sandbox_exe: Option<PathBuf>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            _auth_manager: auth_manager,
            conversation_manager,
            outgoing,
            codex_linux_sandbox_exe,
            _config: config,
            conversation_listeners: HashMap::new(),
            pending_interrupts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn process_request(&mut self, request: ClientRequest) {
        match request {
            ClientRequest::NewConversation { request_id, params } => {
                // Do not tokio::spawn() to process new_conversation()
                // asynchronously because we need to ensure the conversation is
                // created before processing any subsequent messages.
                self.process_new_conversation(request_id, params).await;
            }
            ClientRequest::SendUserMessage { request_id, params } => {
                self.send_user_message(request_id, params).await;
            }
            ClientRequest::InterruptConversation { request_id, params } => {
                self.interrupt_conversation(request_id, params).await;
            }
            ClientRequest::AddConversationListener { request_id, params } => {
                self.add_conversation_listener(request_id, params).await;
            }
            ClientRequest::RemoveConversationListener { request_id, params } => {
                self.remove_conversation_listener(request_id, params).await;
            }
            ClientRequest::SendUserTurn { request_id, params } => {
                self.send_user_turn_compat(request_id, params).await;
            }
            ClientRequest::LoginChatGpt { request_id } => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "login is not supported by this server".to_string(),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
            ClientRequest::CancelLoginChatGpt { request_id, .. } => {
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "cancel login is not supported by this server".to_string(),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
            ClientRequest::LogoutChatGpt { request_id } => {
                // Not supported by this server implementation
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "logout is not supported by this server".to_string(),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
            ClientRequest::GetAuthStatus { request_id, .. } => {
                // Not supported by this server implementation
                let error = JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "auth status is not supported by this server".to_string(),
                    data: None,
                };
                self.outgoing.send_error(request_id, error).await;
            }
            ClientRequest::GitDiffToRemote { request_id, params } => {
                self.git_diff_to_origin(request_id, params.cwd).await;
            }
            _ => { /* ignore unsupported methods */ }
        }
    }

    // Upstream added utility endpoints (user info, set default model, one-off exec).
    // Our fork does not expose them via this server; omit to preserve behavior.
    async fn process_new_conversation(&self, request_id: RequestId, params: NewConversationParams) {
        let config = match derive_config_from_params(params, self.codex_linux_sandbox_exe.clone()) {
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

        match self.conversation_manager.new_conversation(config).await {
            Ok(conversation_id) => {
                let NewConversation {
                    conversation_id,
                    session_configured,
                    ..
                } = conversation_id;
                let response = NewConversationResponse {
                    conversation_id,
                    model: session_configured.model,
                    reasoning_effort: None,
                    // We do not expose the underlying rollout file path in this fork; provide the sessions root.
                    rollout_path: self._config.codex_home.join("sessions"),
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
        let response = InterruptConversationResponse {
            abort_reason: TurnAbortReason::Interrupted,
        };
        self.outgoing.send_response(request_id, response).await;
    }

    async fn add_conversation_listener(
        &mut self,
        request_id: RequestId,
        params: AddConversationListenerParams,
    ) {
        let AddConversationListenerParams { conversation_id } = params;
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
        self.conversation_listeners
            .insert(subscription_id, cancel_tx);
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

                        outgoing_for_task.send_notification(OutgoingNotification {
                            method,
                            params: Some(params.into()),
                        })
                        .await;

                        apply_bespoke_event_handling(event.clone(), conversation_id, conversation.clone(), outgoing_for_task.clone(), pending_interrupts.clone()).await;
                    }
                }
            }
        });
        let response = AddConversationSubscriptionResponse { subscription_id };
        self.outgoing.send_response(request_id, response).await;
    }

    async fn remove_conversation_listener(
        &mut self,
        request_id: RequestId,
        params: RemoveConversationListenerParams,
    ) {
        let RemoveConversationListenerParams { subscription_id } = params;
        match self.conversation_listeners.remove(&subscription_id) {
            Some(sender) => {
                // Signal the spawned task to exit and acknowledge.
                let _ = sender.send(());
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

    async fn git_diff_to_origin(&self, request_id: RequestId, cwd: PathBuf) {
        let diff = git_diff_to_remote(&cwd).await;
        match diff {
            Some(value) => {
                let response = GitDiffToRemoteResponse {
                    sha: value.sha,
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
}

impl CodexMessageProcessor {
    // Minimal compatibility layer: translate SendUserTurn into our current
    // flow by submitting only the user items. We intentionally do not attempt
    // per‑turn reconfiguration here (model, cwd, approval, sandbox) to avoid
    // destabilizing the session. This preserves behavior and acks the request
    // so clients using the new method continue to function.
    async fn send_user_turn_compat(&self, request_id: RequestId, params: SendUserTurnParams) {
        let SendUserTurnParams {
            conversation_id,
            items,
            ..
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
            })
            .await;

        // Acknowledge.
        self.outgoing
            .send_response(request_id, SendUserTurnResponse {})
            .await;
    }
}

async fn apply_bespoke_event_handling(
    event: Event,
    conversation_id: ConversationId,
    conversation: Arc<CodexConversation>,
    outgoing: Arc<OutgoingMessageSender>,
    _pending_interrupts: Arc<Mutex<HashMap<Uuid, Vec<RequestId>>>>,
) {
    let Event {
        id: _event_id, msg, ..
    } = event;
    match msg {
        EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
            call_id,
            changes,
            reason,
            grant_root,
        }) => {
            // Map core FileChange to wire FileChange
            let file_changes: HashMap<PathBuf, codex_protocol::protocol::FileChange> = changes
                .into_iter()
                .map(|(p, c)| {
                    let mapped = match c {
                        codex_core::protocol::FileChange::Add { content } => {
                            codex_protocol::protocol::FileChange::Add { content }
                        }
                        codex_core::protocol::FileChange::Delete => {
                            codex_protocol::protocol::FileChange::Delete {
                                content: String::new(),
                            }
                        }
                        codex_core::protocol::FileChange::Update {
                            unified_diff,
                            move_path,
                            original_content,
                            new_content,
                        } => codex_protocol::protocol::FileChange::Update {
                            unified_diff,
                            move_path,
                            original_content,
                            new_content,
                        },
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
                .send_request(APPLY_PATCH_APPROVAL_METHOD, Some(value))
                .await;
            // TODO(mbolin): Enforce a timeout so this task does not live indefinitely?
            let approval_id = call_id.clone(); // correlate by call_id, not event_id
            tokio::spawn(async move {
                on_patch_approval_response(approval_id, rx, conversation).await;
            });
        }
        EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
            call_id,
            command,
            cwd,
            reason,
        }) => {
            let params = ExecCommandApprovalParams {
                conversation_id,
                call_id: call_id.clone(),
                command,
                cwd,
                reason,
            };
            let value = serde_json::to_value(&params).unwrap_or_default();
            let rx = outgoing
                .send_request(EXEC_COMMAND_APPROVAL_METHOD, Some(value))
                .await;

            // TODO(mbolin): Enforce a timeout so this task does not live indefinitely?
            let approval_id = call_id.clone(); // correlate by call_id, not event_id
            tokio::spawn(async move {
                on_exec_approval_response(approval_id, rx, conversation).await;
            });
        }
        // No special handling needed for interrupts; responses are sent immediately.
        _ => {}
    }
}

fn derive_config_from_params(
    params: NewConversationParams,
    codex_linux_sandbox_exe: Option<PathBuf>,
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
        ..
    } = params;
    let overrides = ConfigOverrides {
        model,
        review_model: None,
        config_profile: profile,
        cwd: cwd.map(PathBuf::from),
        approval_policy: approval_policy.map(map_ask_for_approval_from_wire),
        sandbox_mode,
        model_provider: None,
        codex_linux_sandbox_exe,
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

async fn on_exec_approval_response(
    approval_id: String,
    receiver: tokio::sync::oneshot::Receiver<mcp_types::Result>,
    conversation: Arc<CodexConversation>,
) {
    let response = receiver.await;
    let value = match response {
        Ok(value) => value,
        Err(err) => {
            tracing::error!("request failed: {err:?}");
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
            decision: map_review_decision_from_wire(response.decision),
        })
        .await
    {
        error!("failed to submit ExecApproval: {err}");
    }
}

fn map_review_decision_from_wire(
    d: codex_protocol::protocol::ReviewDecision,
) -> core_protocol::ReviewDecision {
    match d {
        codex_protocol::protocol::ReviewDecision::Approved => {
            core_protocol::ReviewDecision::Approved
        }
        codex_protocol::protocol::ReviewDecision::ApprovedForSession => {
            core_protocol::ReviewDecision::ApprovedForSession
        }
        codex_protocol::protocol::ReviewDecision::Denied => core_protocol::ReviewDecision::Denied,
        codex_protocol::protocol::ReviewDecision::Abort => core_protocol::ReviewDecision::Abort,
    }
}

fn map_ask_for_approval_from_wire(
    a: codex_protocol::protocol::AskForApproval,
) -> core_protocol::AskForApproval {
    match a {
        codex_protocol::protocol::AskForApproval::UnlessTrusted => {
            core_protocol::AskForApproval::UnlessTrusted
        }
        codex_protocol::protocol::AskForApproval::OnFailure => {
            core_protocol::AskForApproval::OnFailure
        }
        codex_protocol::protocol::AskForApproval::OnRequest => {
            core_protocol::AskForApproval::OnRequest
        }
        codex_protocol::protocol::AskForApproval::Never => core_protocol::AskForApproval::Never,
    }
}

// Unused legacy mappers removed to avoid warnings.
