//! Asynchronous worker that executes an ACP tool-call inside a spawned Tokio task.

use std::sync::Arc;

use agent_client_protocol as acp;
use agent_client_protocol::ToolCallUpdateFields;
use anyhow::{Context as _, Result};
use code_core::CodexConversation;
use code_core::ConversationManager;
use code_core::NewConversation;
use code_core::config::Config as CodexConfig;
use code_core::protocol::EventMsg;
use code_core::protocol::InputItem;
use code_core::protocol::Op;
use code_protocol::protocol::TurnAbortReason;
use mcp_types::CallToolResult;
use mcp_types::ContentBlock;
use mcp_types::RequestId;
use mcp_types::TextContent;
use uuid::Uuid;

use crate::outgoing_message::OutgoingMessageSender;
use crate::outgoing_message::OutgoingNotification;
use crate::session_store::{SessionEntry, SessionMap};
use serde_json;

pub async fn new_session(
    request_id: RequestId,
    config: CodexConfig,
    outgoing: Arc<OutgoingMessageSender>,
    session_map: SessionMap,
    conversation_manager: Arc<ConversationManager>,
) -> Option<Uuid> {
    let config_for_session = config.clone();
    let NewConversation {
        conversation_id,
        conversation,
        session_configured: _,
    } = match conversation_manager.new_conversation(config).await {
        Ok(conv) => conv,
        Err(err) => {
            let result = CallToolResult {
                content: vec![ContentBlock::TextContent(TextContent {
                    r#type: "text".to_string(),
                    text: format!("Failed to start Codex session: {err}"),
                    annotations: None,
                })],
                is_error: Some(true),
                structured_content: None,
            };
            outgoing
                .send_response(request_id, serde_json::to_value(result).unwrap_or_default())
                .await;
            return None;
        }
    };
    let session_uuid: Uuid = conversation_id.into();
    let entry = SessionEntry::new(conversation.clone(), config_for_session);
    session_map.lock().await.insert(session_uuid, entry);

    Some(session_uuid)
}

pub async fn prompt(
    acp_session_id: acp::SessionId,
    codex: Arc<CodexConversation>,
    prompt: Vec<acp::ContentBlock>,
    outgoing: Arc<OutgoingMessageSender>,
) -> Result<acp::StopReason> {
    let items: Vec<InputItem> = prompt
        .into_iter()
        .filter_map(acp_content_block_to_item)
        .collect();

    codex
        .submit(Op::UserInput {
            items,
            final_output_json_schema: None,
        })
        .await
        .context("failed to submit prompt to Codex")?;

    let mut stop_reason = acp::StopReason::EndTurn;

    loop {
        let event = codex.next_event().await?;

        let acp_update = match event.msg {
            EventMsg::Error(error_event) => {
                anyhow::bail!("Error: {}", error_event.message);
            }
            EventMsg::AgentMessage(_) | EventMsg::AgentReasoning(_) => None,
            EventMsg::AgentMessageDelta(event) => Some(acp::SessionUpdate::AgentMessageChunk {
                content: event.delta.into(),
            }),
            EventMsg::AgentReasoningDelta(event) => Some(acp::SessionUpdate::AgentThoughtChunk {
                content: event.delta.into(),
            }),
            EventMsg::McpToolCallBegin(event) => {
                let invocation = event.invocation.clone();
                Some(acp::SessionUpdate::ToolCall(acp::ToolCall {
                    id: acp::ToolCallId(event.call_id.into()),
                    title: format!("{}: {}", invocation.server, invocation.tool),
                    kind: acp::ToolKind::Other,
                    status: acp::ToolCallStatus::InProgress,
                    content: vec![],
                    locations: vec![],
                    raw_input: invocation.arguments,
                    raw_output: None,
                    meta: None,
                }))
            }
            EventMsg::McpToolCallEnd(event) => {
                let call_id = acp::ToolCallId(event.call_id.clone().into());
                let content = match event.result.clone() {
                    Ok(result) => Some(
                        result
                            .content
                            .into_iter()
                            .map(|block| to_acp_content_block(block).into())
                            .collect(),
                    ),
                    Err(err) => Some(vec![err.into()]),
                };
                let raw_output = event
                    .result
                    .as_ref()
                    .ok()
                    .and_then(|result| result.structured_content.clone());
                Some(acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate {
                    id: call_id,
                    fields: ToolCallUpdateFields {
                        status: if event.is_success() {
                            Some(acp::ToolCallStatus::Completed)
                        } else {
                            Some(acp::ToolCallStatus::Failed)
                        },
                        content,
                        raw_output,
                        ..Default::default()
                    },
                    meta: None,
                }))
            }
            EventMsg::ExecApprovalRequest(_) | EventMsg::ApplyPatchApprovalRequest(_) => None,
            EventMsg::ExecCommandBegin(event) => Some(acp::SessionUpdate::ToolCall(
                code_core::acp::new_execute_tool_call(
                    &event.call_id,
                    &event.command,
                    acp::ToolCallStatus::InProgress,
                ),
            )),
            EventMsg::ExecCommandEnd(event) => {
                let call_id = acp::ToolCallId(event.call_id.clone().into());
                Some(acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate {
                    id: call_id,
                    fields: ToolCallUpdateFields {
                        status: if event.exit_code == 0 {
                            Some(acp::ToolCallStatus::Completed)
                        } else {
                            Some(acp::ToolCallStatus::Failed)
                        },
                        content: Some(vec![event.stdout.into(), event.stderr.into()]),
                        ..Default::default()
                    },
                    meta: None,
                }))
            }
            EventMsg::PatchApplyBegin(event) => Some(acp::SessionUpdate::ToolCall(
                code_core::acp::new_patch_tool_call(
                    &event.call_id,
                    &event.changes,
                    acp::ToolCallStatus::InProgress,
                ),
            )),
            EventMsg::PatchApplyEnd(event) => {
                let call_id = acp::ToolCallId(event.call_id.clone().into());
                Some(acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate {
                    id: call_id,
                    fields: ToolCallUpdateFields {
                        status: if event.success {
                            Some(acp::ToolCallStatus::Completed)
                        } else {
                            Some(acp::ToolCallStatus::Failed)
                        },
                        ..Default::default()
                    },
                    meta: None,
                }))
            }
            EventMsg::TurnAborted(event) => {
                if matches!(event.reason, TurnAbortReason::Interrupted) {
                    stop_reason = acp::StopReason::Cancelled;
                }
                None
            }
            EventMsg::TaskComplete(_) => return Ok(stop_reason),
            EventMsg::SessionConfigured(_)
            | EventMsg::TokenCount(_)
            | EventMsg::TaskStarted
            | EventMsg::GetHistoryEntryResponse(_)
            | EventMsg::BackgroundEvent(_)
            | EventMsg::ShutdownComplete => None,
            _ => None,
        };

        if let Some(update) = acp_update {
            let notification = OutgoingNotification {
                method: acp::CLIENT_METHOD_NAMES.session_update.to_string(),
                params: Some(
                    serde_json::to_value(acp::SessionNotification {
                        session_id: acp_session_id.clone(),
                        update,
                        meta: None,
                    })
                    .unwrap_or_default(),
                ),
            };
            outgoing.send_notification(notification).await;
        }
    }
}

fn acp_content_block_to_item(block: acp::ContentBlock) -> Option<InputItem> {
    match block {
        acp::ContentBlock::Text(text_content) => Some(InputItem::Text {
            text: text_content.text,
        }),
        acp::ContentBlock::ResourceLink(link) => Some(InputItem::Text {
            text: format!("@{}", link.uri),
        }),
        acp::ContentBlock::Image(image_content) => Some(InputItem::Image {
            image_url: image_content.data,
        }),
        acp::ContentBlock::Audio(_) | acp::ContentBlock::Resource(_) => None,
    }
}

fn to_acp_annotations(annotations: mcp_types::Annotations) -> acp::Annotations {
    acp::Annotations {
        audience: annotations.audience.map(|roles| {
            roles
                .into_iter()
                .map(|role| match role {
                    mcp_types::Role::User => acp::Role::User,
                    mcp_types::Role::Assistant => acp::Role::Assistant,
                })
                .collect()
        }),
        last_modified: annotations.last_modified,
        priority: annotations.priority,
        meta: None,
    }
}

fn to_acp_embedded_resource_resource(
    resource: mcp_types::EmbeddedResourceResource,
) -> acp::EmbeddedResourceResource {
    match resource {
        mcp_types::EmbeddedResourceResource::TextResourceContents(text_contents) => {
            acp::EmbeddedResourceResource::TextResourceContents(acp::TextResourceContents {
                mime_type: text_contents.mime_type,
                text: text_contents.text,
                uri: text_contents.uri,
                meta: None,
            })
        }
        mcp_types::EmbeddedResourceResource::BlobResourceContents(blob_contents) => {
            acp::EmbeddedResourceResource::BlobResourceContents(acp::BlobResourceContents {
                blob: blob_contents.blob,
                mime_type: blob_contents.mime_type,
                uri: blob_contents.uri,
                meta: None,
            })
        }
    }
}

fn to_acp_content_block(block: mcp_types::ContentBlock) -> acp::ContentBlock {
    match block {
        ContentBlock::TextContent(text_content) => acp::ContentBlock::Text(acp::TextContent {
            annotations: text_content.annotations.map(to_acp_annotations),
            text: text_content.text,
            meta: None,
        }),
        ContentBlock::ImageContent(image_content) => acp::ContentBlock::Image(acp::ImageContent {
            annotations: image_content.annotations.map(to_acp_annotations),
            data: image_content.data,
            mime_type: image_content.mime_type,
            uri: None,
            meta: None,
        }),
        ContentBlock::AudioContent(audio_content) => acp::ContentBlock::Audio(acp::AudioContent {
            annotations: audio_content.annotations.map(to_acp_annotations),
            data: audio_content.data,
            mime_type: audio_content.mime_type,
            meta: None,
        }),
        ContentBlock::ResourceLink(resource_link) => {
            acp::ContentBlock::ResourceLink(acp::ResourceLink {
                annotations: resource_link.annotations.map(to_acp_annotations),
                uri: resource_link.uri,
                description: resource_link.description,
                mime_type: resource_link.mime_type,
                name: resource_link.name,
                size: resource_link.size,
                title: resource_link.title,
                meta: None,
            })
        }
        ContentBlock::EmbeddedResource(embedded_resource) => {
            acp::ContentBlock::Resource(acp::EmbeddedResource {
                annotations: embedded_resource.annotations.map(to_acp_annotations),
                resource: to_acp_embedded_resource_resource(embedded_resource.resource),
                meta: None,
            })
        }
    }
}
