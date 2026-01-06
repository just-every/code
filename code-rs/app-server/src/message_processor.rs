use std::path::PathBuf;

use crate::code_message_processor::CodexMessageProcessor;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::OutgoingMessageSender;
use code_protocol::mcp_protocol::ClientInfo;
use code_protocol::mcp_protocol::ClientRequest;
use code_protocol::mcp_protocol::InitializeResponse;
use code_protocol::protocol::SessionSource;

use code_core::AuthManager;
use code_core::ConversationManager;
use code_core::config::Config;
use code_core::default_client::USER_AGENT_SUFFIX;
use code_core::default_client::get_code_user_agent;
use mcp_types::JSONRPCError;
use mcp_types::JSONRPCErrorError;
use mcp_types::JSONRPCNotification;
use mcp_types::JSONRPCRequest;
use mcp_types::JSONRPCResponse;
use std::sync::Arc;

pub(crate) struct MessageProcessor {
    outgoing: Arc<OutgoingMessageSender>,
    code_message_processor: CodexMessageProcessor,
    base_config: Arc<Config>,
    initialized: bool,
}

impl MessageProcessor {
    /// Create a new `MessageProcessor`, retaining a handle to the outgoing
    /// `Sender` so handlers can enqueue messages to be written to stdout.
    pub(crate) fn new(
        outgoing: OutgoingMessageSender,
        code_linux_sandbox_exe: Option<PathBuf>,
        config: Arc<Config>,
    ) -> Self {
        let outgoing = Arc::new(outgoing);
        let auth_manager = AuthManager::shared_with_mode_and_originator(
            config.code_home.clone(),
            config.preferred_auth_mode(),
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

        Self {
            outgoing,
            code_message_processor,
            base_config: config_for_processor,
            initialized: false,
        }
    }

    pub(crate) async fn process_request(&mut self, request: JSONRPCRequest) {
        let request_id = request.id.clone();
        if let Ok(request_json) = serde_json::to_value(request)
            && let Ok(code_request) = serde_json::from_value::<ClientRequest>(request_json)
        {
            match code_request {
                // Handle Initialize internally so CodexMessageProcessor does not have to concern
                // itself with the `initialized` bool.
                ClientRequest::Initialize { request_id, params } => {
                    if self.initialized {
                        let error = JSONRPCErrorError {
                            code: INVALID_REQUEST_ERROR_CODE,
                            message: "Already initialized".to_string(),
                            data: None,
                        };
                        self.outgoing.send_error(request_id, error).await;
                        return;
                    } else {
                        let ClientInfo {
                            name,
                            title: _title,
                            version,
                        } = params.client_info;
                        let user_agent_suffix = format!("{name}; {version}");
                        if let Ok(mut suffix) = USER_AGENT_SUFFIX.lock() {
                            *suffix = Some(user_agent_suffix);
                        }

                        let user_agent = get_code_user_agent(Some(
                            &self.base_config.responses_originator_header,
                        ));
                        let response = InitializeResponse { user_agent };
                        self.outgoing.send_response(request_id, response).await;

                        self.initialized = true;
                        return;
                    }
                }
                _ => {
                    if !self.initialized {
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
                .process_request(code_request)
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

    /// Handle a standalone JSON-RPC response originating from the peer.
    pub(crate) async fn process_response(&mut self, response: JSONRPCResponse) {
        tracing::info!("<- response: {:?}", response);
        let JSONRPCResponse { id, result, .. } = response;
        self.outgoing.notify_client_response(id, result).await
    }

    /// Handle an error object received from the peer.
    pub(crate) fn process_error(&mut self, err: JSONRPCError) {
        tracing::error!("<- error: {:?}", err);
    }
}
