//! Asynchronous worker that executes a **Codex** tool-call inside a spawned
//! Tokio task. Separated from `message_processor.rs` to keep that file small
//! and to make future feature-growth easier to manage.

use std::collections::HashMap;
use std::sync::Arc;

use code_core::CodexConversation;
use code_core::ConversationManager;
use code_core::NewConversation;
use code_core::config::Config as CodexConfig;
use code_core::protocol::AgentMessageEvent;
use code_core::protocol::ApplyPatchApprovalRequestEvent;
use code_core::protocol::Event;
use code_core::protocol::EventMsg;
use code_core::protocol::ExecApprovalRequestEvent;
use code_core::protocol::InputItem;
use code_core::protocol::Op;
use code_core::protocol::Submission;
use code_core::protocol::TaskCompleteEvent;
use mcp_types::CallToolResult;
use mcp_types::ContentBlock;
use mcp_types::RequestId;
use mcp_types::TextContent;
use serde_json::json;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::exec_approval::handle_exec_approval_request;
use crate::outgoing_message::OutgoingMessageSender;
use crate::outgoing_message::OutgoingMessageSenderExt;
use crate::outgoing_message::OutgoingNotificationMeta;
use crate::patch_approval::handle_patch_approval_request;
use crate::session_store::{SessionEntry, SessionMap};

pub(crate) const INVALID_PARAMS_ERROR_CODE: i64 = -32602;

/// Run a complete Codex session and stream events back to the client.
///
/// On completion (success or error) the function sends the appropriate
/// `tools/call` response so the LLM can continue the conversation.
pub async fn run_code_tool_session(
    id: RequestId,
    initial_prompt: String,
    config: CodexConfig,
    outgoing: Arc<OutgoingMessageSender>,
    session_map: SessionMap,
    conversation_manager: Arc<ConversationManager>,
    running_requests_id_to_code_uuid: Arc<Mutex<HashMap<RequestId, Uuid>>>,
) {
    let config_for_session = config.clone();
    let NewConversation {
        conversation_id,
        conversation,
        session_configured,
    } = match conversation_manager.new_conversation(config).await {
        Ok(res) => res,
        Err(e) => {
            let result = CallToolResult {
                content: vec![ContentBlock::TextContent(TextContent {
                    r#type: "text".to_string(),
                    text: format!("Failed to start Codex session: {e}"),
                    annotations: None,
                })],
                is_error: Some(true),
                structured_content: None,
            };
            outgoing.send_response(id.clone(), result).await;
            return;
        }
    };
    let session_uuid: Uuid = conversation_id.into();
    let entry = SessionEntry::new(conversation.clone(), config_for_session);
    session_map.lock().await.insert(session_uuid, entry);

    let session_configured_event = Event {
        // Use a fake id value for now.
        id: "".to_string(),
        event_seq: 0,
        msg: EventMsg::SessionConfigured(session_configured.clone()),
        order: None,
    };
    outgoing
        .send_event_as_notification(
            &session_configured_event,
            Some(OutgoingNotificationMeta::new(Some(id.clone()))),
        )
        .await;

    // Use the original MCP request ID as the `sub_id` for the Codex submission so that
    // any events emitted for this tool-call can be correlated with the
    // originating `tools/call` request.
    let sub_id = match &id {
        RequestId::String(s) => s.clone(),
        RequestId::Integer(n) => n.to_string(),
    };
    running_requests_id_to_code_uuid
        .lock()
        .await
        .insert(id.clone(), session_uuid);
    let submission = Submission {
        id: sub_id.clone(),
        op: Op::UserInput {
            items: vec![InputItem::Text {
                text: initial_prompt.clone(),
            }],
            final_output_json_schema: None,
        },
    };

    if let Err(e) = conversation.submit_with_id(submission).await {
        tracing::error!("Failed to submit initial prompt: {e}");
        // unregister the id so we don't keep it in the map
        running_requests_id_to_code_uuid.lock().await.remove(&id);
        return;
    }

    run_code_tool_session_inner(
        conversation,
        outgoing,
        id,
        running_requests_id_to_code_uuid,
    )
    .await;
}

pub async fn run_code_tool_session_reply(
    conversation: Arc<CodexConversation>,
    outgoing: Arc<OutgoingMessageSender>,
    request_id: RequestId,
    prompt: String,
    running_requests_id_to_code_uuid: Arc<Mutex<HashMap<RequestId, Uuid>>>,
    session_id: Uuid,
) {
    running_requests_id_to_code_uuid
        .lock()
        .await
        .insert(request_id.clone(), session_id);
    if let Err(e) = conversation
        .submit(Op::UserInput {
            items: vec![InputItem::Text { text: prompt }],
            final_output_json_schema: None,
        })
        .await
    {
        tracing::error!("Failed to submit user input: {e}");
        // unregister the id so we don't keep it in the map
        running_requests_id_to_code_uuid
            .lock()
            .await
            .remove(&request_id);
        return;
    }

    run_code_tool_session_inner(
        conversation,
        outgoing,
        request_id,
        running_requests_id_to_code_uuid,
    )
    .await;
}

async fn run_code_tool_session_inner(
    codex: Arc<CodexConversation>,
    outgoing: Arc<OutgoingMessageSender>,
    request_id: RequestId,
    running_requests_id_to_code_uuid: Arc<Mutex<HashMap<RequestId, Uuid>>>,
) {
    let request_id_str = match &request_id {
        RequestId::String(s) => s.clone(),
        RequestId::Integer(n) => n.to_string(),
    };

    // Stream events until the task needs to pause for user interaction or
    // completes.
    loop {
        match codex.next_event().await {
            Ok(event) => {
                outgoing
                    .send_event_as_notification(
                        &event,
                        Some(OutgoingNotificationMeta::new(Some(request_id.clone()))),
                    )
                    .await;

                match event.msg {
                    EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
                        command,
                        cwd,
                        call_id,
                        reason: _,
                    }) => {
                        handle_exec_approval_request(
                            command,
                            cwd,
                            outgoing.clone(),
                            codex.clone(),
                            request_id.clone(),
                            request_id_str.clone(),
                            event.id.clone(),
                            call_id,
                        )
                        .await;
                        continue;
                    }
                    EventMsg::Error(err_event) => {
                        // Return a response to conclude the tool call when the Codex session reports an error (e.g., interruption).
                        let result = json!({
                            "error": err_event.message,
                        });
                        outgoing.send_response(request_id.clone(), result).await;
                        break;
                    }
                    EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
                        call_id,
                        reason,
                        grant_root,
                        changes,
                    }) => {
                        handle_patch_approval_request(
                            call_id,
                            reason,
                            grant_root,
                            changes,
                            outgoing.clone(),
                            codex.clone(),
                            request_id.clone(),
                            request_id_str.clone(),
                            event.id.clone(),
                        )
                        .await;
                        continue;
                    }
                    EventMsg::RequestUserInput(ev) => {
                        let question_count = ev.questions.len();
                        let result = CallToolResult {
                            content: vec![ContentBlock::TextContent(TextContent {
                                r#type: "text".to_string(),
                                text: format!(
                                    "Codex requested user input ({question_count} question(s)) but interactive prompts are not supported in this MCP tool session."
                                ),
                                annotations: None,
                            })],
                            is_error: Some(true),
                            structured_content: None,
                        };
                        outgoing.send_response(request_id.clone(), result).await;
                        running_requests_id_to_code_uuid
                            .lock()
                            .await
                            .remove(&request_id);
                        break;
                    }
                    EventMsg::TaskComplete(TaskCompleteEvent { last_agent_message }) => {
                        let text = match last_agent_message {
                            Some(msg) => msg.clone(),
                            None => "".to_string(),
                        };
                        let result = CallToolResult {
                            content: vec![ContentBlock::TextContent(TextContent {
                                r#type: "text".to_string(),
                                text,
                                annotations: None,
                            })],
                            is_error: None,
                            structured_content: None,
                        };
                        outgoing.send_response(request_id.clone(), result).await;
                        // unregister the id so we don't keep it in the map
                        running_requests_id_to_code_uuid
                            .lock()
                            .await
                            .remove(&request_id);
                        break;
                    }
                    EventMsg::SessionConfigured(_) => {
                        tracing::error!("unexpected SessionConfigured event");
                    }
                    EventMsg::AgentMessageDelta(_) => {
                        // TODO: think how we want to support this in the MCP
                    }
                    EventMsg::AgentReasoningDelta(_) => {
                        // TODO: think how we want to support this in the MCP
                    }
                    EventMsg::AgentMessage(AgentMessageEvent { .. }) => {
                        // TODO: think how we want to support this in the MCP
                    }
                    EventMsg::AgentReasoningRawContent(_)
                    | EventMsg::AgentReasoningRawContentDelta(_)
                    | EventMsg::TaskStarted
                    | EventMsg::TokenCount(_)
                    | EventMsg::AgentReasoning(_)
                    | EventMsg::AgentReasoningSectionBreak(_)
                    | EventMsg::McpToolCallBegin(_)
                    | EventMsg::McpToolCallEnd(_)
                    // | EventMsg::McpListToolsResponse(_)
                    | EventMsg::ExecCommandBegin(_)
                    | EventMsg::ExecCommandOutputDelta(_)
                    | EventMsg::ExecCommandEnd(_)
                    | EventMsg::BackgroundEvent(_)
                    | EventMsg::PatchApplyBegin(_)
                    | EventMsg::PatchApplyEnd(_)
                    | EventMsg::TurnDiff(_)
                    | EventMsg::WebSearchBegin(_)
                    | EventMsg::WebSearchComplete(_)
                    | EventMsg::GetHistoryEntryResponse(_)
                    | EventMsg::ReplayHistory(_)
                    | EventMsg::PlanUpdate(_)
                    | EventMsg::BrowserScreenshotUpdate(_)
                    | EventMsg::BrowserSnapshot(_)
                    | EventMsg::EnvironmentContextFull(_)
                    | EventMsg::EnvironmentContextDelta(_)
                    | EventMsg::ListCustomPromptsResponse(_)
                    | EventMsg::ListSkillsResponse(_)
                    | EventMsg::AgentStatusUpdate(_)
                    | EventMsg::CompactionCheckpointWarning(_)
                    | EventMsg::TurnAborted(_)
                    | EventMsg::ConversationPath(_)
                    | EventMsg::UserMessage(_)
                    | EventMsg::ShutdownComplete
                    | EventMsg::EnteredReviewMode(_)
                    | EventMsg::ExitedReviewMode(_)
                    | EventMsg::CustomToolCallBegin(_)
                    | EventMsg::CustomToolCallEnd(_)
                    | EventMsg::ViewImageToolCall(_) => {
                        // For now, we do not do anything extra for these
                        // events. Note that
                        // send(code_event_to_notification(&event)) above has
                        // already dispatched these events as notifications,
                        // though we may want to do give different treatment to
                        // individual events in the future.
                    }
                }
            }
            Err(e) => {
                let result = CallToolResult {
                    content: vec![ContentBlock::TextContent(TextContent {
                        r#type: "text".to_string(),
                        text: format!("Codex runtime error: {e}"),
                        annotations: None,
                    })],
                    is_error: Some(true),
                    // TODO(mbolin): Could present the error in a more
                    // structured way.
                    structured_content: None,
                };
                outgoing.send_response(request_id.clone(), result).await;
                break;
            }
        }
    }
}
