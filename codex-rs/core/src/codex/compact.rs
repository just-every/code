use std::sync::Arc;

use super::AgentTask;
use super::Session;
use super::TurnContext;
use super::get_last_assistant_message_from_turn;
use crate::Prompt;
use crate::client_common::ResponseEvent;
use crate::environment_context::EnvironmentContext;
use crate::error::CodexErr;
use crate::error::Result as CodexResult;
use crate::protocol::AgentMessageEvent;
use crate::protocol::ErrorEvent;
use crate::protocol::EventMsg;
use crate::protocol::InputItem;
use crate::protocol::TaskCompleteEvent;
use crate::truncate::truncate_middle;
use crate::util::backoff;
use askama::Template;
use base64::Engine;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::CompactedItem;
use codex_protocol::protocol::InputMessageKind;
use codex_protocol::protocol::RolloutItem;
use futures::prelude::*;

pub const SUMMARIZATION_PROMPT: &str = include_str!("../../templates/compact/prompt.md");
const COMPACT_USER_MESSAGE_MAX_TOKENS: usize = 20_000;
const COMPACT_TEXT_CONTENT_MAX_BYTES: usize = 8 * 1024;
const COMPACT_TOOL_ARGS_MAX_BYTES: usize = 4 * 1024;
const COMPACT_TOOL_OUTPUT_MAX_BYTES: usize = 4 * 1024;
const COMPACT_IMAGE_URL_MAX_BYTES: usize = 512;

#[derive(Template)]
#[template(path = "compact/history_bridge.md", escape = "none")]
struct HistoryBridgeTemplate<'a> {
    user_messages_text: &'a str,
    summary_text: &'a str,
}

pub(super) fn spawn_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    sub_id: String,
    input: Vec<InputItem>,
) {
    let task = AgentTask::compact(
        sess.clone(),
        turn_context,
        sub_id,
        input,
        SUMMARIZATION_PROMPT.to_string(),
    );
    // set_task is synchronous in our fork
    sess.set_task(task);
}

pub(super) async fn run_inline_auto_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
) -> Vec<ResponseItem> {
    let sub_id = sess.next_internal_sub_id();
    let input = vec![InputItem::Text {
        text: SUMMARIZATION_PROMPT.to_string(),
    }];
    run_compact_task_inner_inline(
        sess,
        turn_context,
        sub_id,
        input,
        SUMMARIZATION_PROMPT.to_string(),
    )
    .await
}

pub(super) async fn run_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    sub_id: String,
    input: Vec<InputItem>,
    compact_instructions: String,
) {
    let start_event = sess.make_event(&sub_id, EventMsg::TaskStarted);
    sess.send_event(start_event).await;
    let _ = perform_compaction(
        sess.clone(),
        turn_context,
        sub_id.clone(),
        input,
        compact_instructions,
        true,
    )
    .await;
    let event = sess.make_event(
        &sub_id,
        EventMsg::TaskComplete(TaskCompleteEvent {
            last_agent_message: None,
        }),
    );
    sess.send_event(event).await;
}

/// Perform compaction as a background task that updates session history in-place.
pub(super) async fn perform_compaction(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    sub_id: String,
    input: Vec<InputItem>,
    compact_instructions: String,
    remove_task_on_completion: bool,
) -> CodexResult<()> {
    // Convert core InputItem -> ResponseInputItem using the same logic as the main turn flow
    let initial_input_for_turn: ResponseInputItem = response_input_from_core_items(input);
    let turn_input = sess.turn_input_with_history(vec![initial_input_for_turn.clone().into()]);

    let turn_input = sanitize_items_for_compact(turn_input);

    let prompt = Prompt {
        input: turn_input,
        store: !sess.disable_response_storage,
        user_instructions: turn_context.user_instructions.clone(),
        environment_context: Some(EnvironmentContext::new(
            Some(turn_context.cwd.clone()),
            Some(turn_context.approval_policy),
            Some(turn_context.sandbox_policy.clone()),
            Some(sess.user_shell.clone()),
        )),
        tools: Vec::new(),
        status_items: Vec::new(),
        base_instructions_override: Some(compact_instructions),
        include_additional_instructions: true,
        text_format: None,
        model_override: None,
        model_family_override: None,
        output_schema: None,
    };

    let max_retries = turn_context.client.get_provider().stream_max_retries();
    let mut retries = 0;

    // Do not persist a TurnContext rollout item here; inline compaction is a
    // background maintenance task and should not affect rollout reconstruction.

    loop {
        match drain_to_completed(&sess, turn_context.as_ref(), &prompt).await {
            Ok(()) => break,
            Err(CodexErr::Interrupted) => return Err(CodexErr::Interrupted),
            Err(e) => {
                if retries < max_retries {
                    retries += 1;
                    let delay = backoff(retries);
                    sess.notify_stream_error(
                        &sub_id,
                        format!(
                            "stream error: {e}; retrying {retries}/{max_retries} in {delay:?}…"
                        ),
                    )
                    .await;
                    tokio::time::sleep(delay).await;
                    continue;
                } else {
                    let event = sess.make_event(
                        &sub_id,
                        EventMsg::Error(ErrorEvent {
                            message: e.to_string(),
                        }),
                    );
                    sess.send_event(event).await;
                    return Err(e);
                }
            }
        }
    }

    if remove_task_on_completion {
        sess.remove_task(&sub_id);
    }

    // Snapshot history and compute a compacted version
    let history_snapshot = {
        let state = sess.state.lock().unwrap();
        state.history.contents()
    };
    let summary_text = get_last_assistant_message_from_turn(&history_snapshot).unwrap_or_default();
    let user_messages = collect_user_messages(&history_snapshot);
    let initial_context = sess.build_initial_context(turn_context.as_ref());
    let new_history = build_compacted_history(initial_context, &user_messages, &summary_text);

    // Replace session history in-place
    {
        let mut state = sess.state.lock().unwrap();
        // Replace entire history with the compacted one
        state.history = crate::conversation_history::ConversationHistory::new();
        state.history.record_items(new_history.iter());
    }

    let rollout_item = RolloutItem::Compacted(CompactedItem {
        message: summary_text.clone(),
    });
    sess.persist_rollout_items(&[rollout_item]).await;

    let display_message = if summary_text.trim().is_empty() {
        "Compact task completed.".to_string()
    } else {
        summary_text.clone()
    };
    let event = sess.make_event(
        &sub_id,
        EventMsg::AgentMessage(AgentMessageEvent {
            message: display_message,
        }),
    );
    sess.send_event(event).await;
    Ok(())
}

/// Run compaction and return the rebuilt compact history without mutating session history.
async fn run_compact_task_inner_inline(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    sub_id: String,
    input: Vec<InputItem>,
    compact_instructions: String,
) -> Vec<ResponseItem> {
    // Convert core InputItem -> ResponseInputItem and build prompt
    let initial_input_for_turn: ResponseInputItem = response_input_from_core_items(input);
    let turn_input = sess.turn_input_with_history(vec![initial_input_for_turn.clone().into()]);

    let turn_input = sanitize_items_for_compact(turn_input);

    let prompt = Prompt {
        input: turn_input,
        store: !sess.disable_response_storage,
        user_instructions: turn_context.user_instructions.clone(),
        environment_context: Some(EnvironmentContext::new(
            Some(turn_context.cwd.clone()),
            Some(turn_context.approval_policy),
            Some(turn_context.sandbox_policy.clone()),
            Some(sess.user_shell.clone()),
        )),
        tools: Vec::new(),
        status_items: Vec::new(),
        base_instructions_override: Some(compact_instructions),
        include_additional_instructions: true,
        text_format: None,
        model_override: None,
        model_family_override: None,
        output_schema: None,
    };

    let max_retries = turn_context.client.get_provider().stream_max_retries();
    let mut retries = 0;
    loop {
        match drain_to_completed(&sess, turn_context.as_ref(), &prompt).await {
            Ok(()) => break,
            Err(CodexErr::Interrupted) => return Vec::new(),
            Err(e) => {
                if retries < max_retries {
                    retries += 1;
                    let delay = backoff(retries);
                    sess.notify_stream_error(
                        &sub_id,
                        format!(
                            "stream error: {e}; retrying {retries}/{max_retries} in {delay:?}…"
                        ),
                    )
                    .await;
                    tokio::time::sleep(delay).await;
                    continue;
                } else {
                    let event = sess.make_event(
                        &sub_id,
                        EventMsg::Error(ErrorEvent {
                            message: e.to_string(),
                        }),
                    );
                    sess.send_event(event).await;
                    return Vec::new();
                }
            }
        }
    }

    let history_snapshot = {
        let state = sess.state.lock().unwrap();
        state.history.contents()
    };
    let summary_text = get_last_assistant_message_from_turn(&history_snapshot).unwrap_or_default();
    let user_messages = collect_user_messages(&history_snapshot);
    let initial_context = sess.build_initial_context(turn_context.as_ref());
    let new_history = build_compacted_history(initial_context, &user_messages, &summary_text);

    let display_message = if summary_text.trim().is_empty() {
        "Compact task completed.".to_string()
    } else {
        summary_text.clone()
    };
    let event = sess.make_event(
        &sub_id,
        EventMsg::AgentMessage(AgentMessageEvent {
            message: display_message,
        }),
    );
    sess.send_event(event).await;

    new_history
}

pub fn content_items_to_text(content: &[ContentItem]) -> Option<String> {
    let mut pieces = Vec::new();
    for item in content {
        match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                if !text.is_empty() {
                    pieces.push(text.as_str());
                }
            }
            ContentItem::InputImage { .. } => {}
        }
    }
    if pieces.is_empty() {
        None
    } else {
        Some(pieces.join("\n"))
    }
}

fn truncate_for_compact(text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    truncate_middle(&text, max_bytes).0
}

fn sanitize_items_for_compact(items: Vec<ResponseItem>) -> Vec<ResponseItem> {
    items
        .into_iter()
        .filter_map(|item| match item {
            ResponseItem::Message { id, role, content } => {
                let mut filtered_content = Vec::with_capacity(content.len());
                for content_item in content {
                    match content_item {
                        ContentItem::InputText { text } => {
                            filtered_content.push(ContentItem::InputText {
                                text: truncate_for_compact(text, COMPACT_TEXT_CONTENT_MAX_BYTES),
                            });
                        }
                        ContentItem::OutputText { text } => {
                            filtered_content.push(ContentItem::OutputText {
                                text: truncate_for_compact(text, COMPACT_TEXT_CONTENT_MAX_BYTES),
                            });
                        }
                        ContentItem::InputImage { image_url } => {
                            if image_url.starts_with("data:")
                                || image_url.len() > COMPACT_IMAGE_URL_MAX_BYTES
                            {
                                let bytes = image_url.len();
                                filtered_content.push(ContentItem::InputText {
                                    text: format!("(image omitted for compaction; {bytes} bytes)",),
                                });
                            } else {
                                filtered_content.push(ContentItem::InputImage { image_url });
                            }
                        }
                    }
                }
                if filtered_content.is_empty() {
                    None
                } else {
                    Some(ResponseItem::Message {
                        id,
                        role,
                        content: filtered_content,
                    })
                }
            }
            ResponseItem::FunctionCall {
                id,
                name,
                arguments,
                call_id,
            } => {
                let arguments = truncate_for_compact(arguments, COMPACT_TOOL_ARGS_MAX_BYTES);
                Some(ResponseItem::FunctionCall {
                    id,
                    name,
                    arguments,
                    call_id,
                })
            }
            ResponseItem::FunctionCallOutput { call_id, output } => {
                let FunctionCallOutputPayload { content, success } = output;
                let content = truncate_for_compact(content, COMPACT_TOOL_OUTPUT_MAX_BYTES);
                Some(ResponseItem::FunctionCallOutput {
                    call_id,
                    output: FunctionCallOutputPayload { content, success },
                })
            }
            ResponseItem::CustomToolCall {
                id,
                status,
                call_id,
                name,
                input,
            } => {
                let input = truncate_for_compact(input, COMPACT_TOOL_ARGS_MAX_BYTES);
                Some(ResponseItem::CustomToolCall {
                    id,
                    status,
                    call_id,
                    name,
                    input,
                })
            }
            ResponseItem::CustomToolCallOutput { call_id, output } => {
                let output = truncate_for_compact(output, COMPACT_TOOL_OUTPUT_MAX_BYTES);
                Some(ResponseItem::CustomToolCallOutput { call_id, output })
            }
            ResponseItem::Reasoning { id, summary, .. } => Some(ResponseItem::Reasoning {
                id,
                summary,
                content: None,
                encrypted_content: None,
            }),
            other => Some(other),
        })
        .collect()
}

pub(crate) fn collect_user_messages(items: &[ResponseItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match item {
            ResponseItem::Message { role, content, .. } if role == "user" => {
                content_items_to_text(content)
            }
            _ => None,
        })
        .filter(|text| !is_session_prefix_message(text))
        .collect()
}

pub fn is_session_prefix_message(text: &str) -> bool {
    matches!(
        InputMessageKind::from(("user", text)),
        InputMessageKind::UserInstructions | InputMessageKind::EnvironmentContext
    )
}

pub(crate) fn build_compacted_history(
    initial_context: Vec<ResponseItem>,
    user_messages: &[String],
    summary_text: &str,
) -> Vec<ResponseItem> {
    let mut history = initial_context;
    let mut user_messages_text = if user_messages.is_empty() {
        "(none)".to_string()
    } else {
        user_messages.join("\n\n")
    };
    // Truncate the concatenated prior user messages so the bridge message
    // stays well under the context window (approx. 4 bytes/token).
    let max_bytes = COMPACT_USER_MESSAGE_MAX_TOKENS * 4;
    if user_messages_text.len() > max_bytes {
        user_messages_text = truncate_middle(&user_messages_text, max_bytes).0;
    }
    let summary_text = if summary_text.is_empty() {
        "(no summary available)".to_string()
    } else {
        summary_text.to_string()
    };
    let Ok(bridge) = HistoryBridgeTemplate {
        user_messages_text: &user_messages_text,
        summary_text: &summary_text,
    }
    .render() else {
        return vec![];
    };
    history.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText { text: bridge }],
    });
    history
}

async fn drain_to_completed(
    sess: &Session,
    turn_context: &TurnContext,
    prompt: &Prompt,
) -> CodexResult<()> {
    let mut stream = turn_context.client.clone().stream(prompt).await?;
    loop {
        let maybe_event = stream.next().await;
        let Some(event) = maybe_event else {
            return Err(CodexErr::Stream(
                "stream closed before response.completed".into(),
                None,
            ));
        };
        match event {
            Ok(ResponseEvent::OutputItemDone { item, .. }) => {
                let mut state = sess.state.lock().unwrap();
                state.history.record_items(std::slice::from_ref(&item));
            }
            Ok(ResponseEvent::Completed { .. }) => {
                return Ok(());
            }
            Ok(_) => continue,
            Err(e) => return Err(e),
        }
    }
}

// Helper copied from codex.rs (private there): convert core InputItem -> ResponseInputItem
fn response_input_from_core_items(items: Vec<InputItem>) -> ResponseInputItem {
    let mut content_items = Vec::new();

    for item in items {
        match item {
            InputItem::Text { text } => {
                content_items.push(ContentItem::InputText { text });
            }
            InputItem::Image { image_url } => {
                content_items.push(ContentItem::InputImage { image_url });
            }
            InputItem::LocalImage { path } => match std::fs::read(&path) {
                Ok(bytes) => {
                    let mime = mime_guess::from_path(&path)
                        .first()
                        .map(|m| m.essence_str().to_owned())
                        .unwrap_or_else(|| "application/octet-stream".to_string());
                    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                    content_items.push(ContentItem::InputImage {
                        image_url: format!("data:{mime};base64,{encoded}"),
                    });
                }
                Err(err) => {
                    tracing::warn!(
                        "Skipping image {} – could not read file: {}",
                        path.display(),
                        err
                    );
                }
            },
            InputItem::EphemeralImage { path, metadata } => {
                if let Some(meta) = metadata {
                    content_items.push(ContentItem::InputText {
                        text: format!("[EPHEMERAL:{}]", meta),
                    });
                }
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        let mime = mime_guess::from_path(&path)
                            .first()
                            .map(|m| m.essence_str().to_owned())
                            .unwrap_or_else(|| "application/octet-stream".to_string());
                        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
                        content_items.push(ContentItem::InputImage {
                            image_url: format!("data:{mime};base64,{encoded}"),
                        });
                    }
                    Err(err) => {
                        tracing::error!(
                            "Failed to read ephemeral image {} – {}",
                            path.display(),
                            err
                        );
                    }
                }
            }
        }
    }

    ResponseInputItem::Message {
        role: "user".to_string(),
        content: content_items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn content_items_to_text_joins_non_empty_segments() {
        let items = vec![
            ContentItem::InputText {
                text: "hello".to_string(),
            },
            ContentItem::OutputText {
                text: String::new(),
            },
            ContentItem::OutputText {
                text: "world".to_string(),
            },
        ];

        let joined = content_items_to_text(&items);

        assert_eq!(Some("hello\nworld".to_string()), joined);
    }

    #[test]
    fn content_items_to_text_ignores_image_only_content() {
        let items = vec![ContentItem::InputImage {
            image_url: "file://image.png".to_string(),
        }];

        let joined = content_items_to_text(&items);

        assert_eq!(None, joined);
    }

    #[test]
    fn collect_user_messages_extracts_user_text_only() {
        let items = vec![
            ResponseItem::Message {
                id: Some("assistant".to_string()),
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "ignored".to_string(),
                }],
            },
            ResponseItem::Message {
                id: Some("user".to_string()),
                role: "user".to_string(),
                content: vec![
                    ContentItem::InputText {
                        text: "first".to_string(),
                    },
                    ContentItem::OutputText {
                        text: "second".to_string(),
                    },
                ],
            },
            ResponseItem::Other,
        ];

        let collected = collect_user_messages(&items);

        assert_eq!(vec!["first\nsecond".to_string()], collected);
    }

    #[test]
    fn collect_user_messages_filters_session_prefix_entries() {
        let items = vec![
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "<user_instructions>do things</user_instructions>".to_string(),
                }],
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "<ENVIRONMENT_CONTEXT>cwd=/tmp</ENVIRONMENT_CONTEXT>".to_string(),
                }],
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "real user message".to_string(),
                }],
            },
        ];

        let collected = collect_user_messages(&items);

        assert_eq!(vec!["real user message".to_string()], collected);
    }

    #[test]
    fn build_compacted_history_truncates_overlong_user_messages() {
        // Prepare a very large prior user message so the aggregated
        // `user_messages_text` exceeds the truncation threshold used by
        // `build_compacted_history` (80k bytes).
        let big = "X".repeat(200_000);
        let history = build_compacted_history(Vec::new(), std::slice::from_ref(&big), "SUMMARY");

        // Expect exactly one bridge message added to history (plus any initial context we provided, which is none).
        assert_eq!(history.len(), 1);

        // Extract the text content of the bridge message.
        let bridge_text = match &history[0] {
            ResponseItem::Message { role, content, .. } if role == "user" => {
                content_items_to_text(content).unwrap_or_default()
            }
            other => panic!("unexpected item in history: {other:?}"),
        };

        // The bridge should contain the truncation marker and not the full original payload.
        assert!(
            bridge_text.contains("tokens truncated"),
            "expected truncation marker in bridge message"
        );
        assert!(
            !bridge_text.contains(&big),
            "bridge should not include the full oversized user text"
        );
        assert!(
            bridge_text.contains("SUMMARY"),
            "bridge should include the provided summary text"
        );
    }
}
