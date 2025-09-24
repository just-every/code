use std::sync::Arc;

use super::debug_history;
use super::get_last_assistant_message_from_turn;
use super::response_input_from_core_items;
use super::AgentTask;
use super::MutexExt;
use super::Session;
use super::TurnContext;
use crate::client_common::ResponseEvent;
use crate::config_types::ReasoningEffort as ReasoningEffortConfig;
use crate::config_types::ReasoningSummary as ReasoningSummaryConfig;
use crate::error::CodexErr;
use crate::error::Result as CodexResult;
use crate::model_family::find_family_for_model;
use crate::protocol::AgentMessageEvent;
use crate::protocol::AskForApproval as AskForApprovalCore;
use crate::protocol::ErrorEvent;
use crate::protocol::EventMsg;
use crate::protocol::InputItem;
use crate::protocol::SandboxPolicy as SandboxPolicyCore;
use crate::protocol::TaskCompleteEvent;
use crate::util::backoff;
use crate::Prompt;
use askama::Template;
use codex_protocol::config_types::ReasoningEffort as ReasoningEffortProtocol;
use codex_protocol::config_types::ReasoningSummary as ReasoningSummaryProtocol;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::CompactedItem;
use codex_protocol::protocol::InputMessageKind;
use codex_protocol::protocol::RolloutItem;
use codex_protocol::protocol::TurnContextItem;
use futures::prelude::*;

pub(super) const COMPACT_TRIGGER_TEXT: &str = "Start Summarization";
const SUMMARIZATION_PROMPT: &str = include_str!("../../templates/compact/prompt.md");

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
    sess.set_task(task);
}

pub(super) async fn run_inline_auto_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
) -> Vec<ResponseItem> {
    let sub_id = sess.next_internal_sub_id();
    let input = vec![InputItem::Text {
        text: COMPACT_TRIGGER_TEXT.to_string(),
    }];
    perform_compaction(
        Arc::clone(&sess),
        Arc::clone(&turn_context),
        sub_id,
        input,
        SUMMARIZATION_PROMPT.to_string(),
        false,
    )
    .await
}

#[allow(dead_code)]
pub(super) async fn run_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    sub_id: String,
    input: Vec<InputItem>,
    compact_instructions: String,
) {
    let _ = perform_compaction(
        sess,
        turn_context,
        sub_id,
        input,
        compact_instructions,
        true,
    )
    .await;
}

/// Perform a compact operation and return the rebuilt conversation history.
pub(super) async fn perform_compaction(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    sub_id: String,
    input: Vec<InputItem>,
    compact_instructions: String,
    remove_task_on_completion: bool,
) -> Vec<ResponseItem> {
    sess
        .notify_background_event(&sub_id, "Compacting conversation...")
        .await;
    let start_event = sess.make_event(&sub_id, EventMsg::TaskStarted);
    sess.send_event(start_event).await;

    let initial_input_for_turn = response_input_from_core_items(input);
    let instructions_override = compact_instructions;
    let turn_input =
        sess.turn_input_with_history(vec![initial_input_for_turn.clone().into()]);

    let mut prompt = Prompt {
        input: turn_input,
        store: !sess.disable_response_storage,
        user_instructions: None,
        environment_context: None,
        tools: Vec::new(),
        status_items: Vec::new(),
        base_instructions_override: Some(instructions_override),
        include_additional_instructions: false,
        text_format: None,
        model_override: None,
        model_family_override: None,
    };

    if turn_context.client.get_model() == "gpt-5-codex" {
        prompt.model_override = Some("gpt-5".to_string());
        if let Some(family) = find_family_for_model("gpt-5") {
            prompt.model_family_override = Some(family);
        }
    }

    let max_retries = turn_context.client.get_provider().stream_max_retries();
    let mut retries = 0;

    loop {
        let attempt_result =
            drain_to_completed(&sess, turn_context.as_ref(), &prompt).await;

        match attempt_result {
            Ok(()) => {
                break;
            }
            Err(CodexErr::Interrupted) => {
                return Vec::new();
            }
            Err(e) => {
                if retries < max_retries {
                    retries += 1;
                    let delay = backoff(retries);
                    sess
                        .notify_stream_error(
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

    if remove_task_on_completion {
        sess.remove_task(&sub_id);
    }
    let history_snapshot = {
        let state = sess.state.lock_unchecked();
        state.history.contents()
    };
    let summary_text = get_last_assistant_message_from_turn(&history_snapshot).unwrap_or_default();
    let user_messages = collect_user_messages(&history_snapshot);
    let initial_context = sess.build_initial_context(turn_context.as_ref());
    let new_history = build_compacted_history(initial_context, &user_messages, &summary_text);
    {
        let mut state = sess.state.lock_unchecked();
        state.history = super::ConversationHistory::new();
    }
    sess.record_conversation_items(&new_history).await;
    {
        let state = sess.state.lock_unchecked();
        let snapshot = state.history.contents();
        debug_history("after_compact_record", &snapshot);
    }

    // Persist rollout items for traceability and UI reconstruction.
    let ctx_item = RolloutItem::TurnContext(TurnContextItem {
        cwd: turn_context.cwd.clone(),
        approval_policy: map_approval(turn_context.approval_policy),
        sandbox_policy: map_sandbox(&turn_context.sandbox_policy),
        model: turn_context.client.get_model(),
        effort: map_effort(turn_context.client.get_reasoning_effort()),
        summary: map_summary(turn_context.client.get_reasoning_summary()),
    });
    let compact_item = RolloutItem::Compacted(CompactedItem {
        message: summary_text.clone(),
    });
    sess.persist_rollout_items(&[ctx_item, compact_item]).await;

    let message = if summary_text.trim().is_empty() {
        "Compact task completed.".to_string()
    } else {
        summary_text.clone()
    };
    let event = sess.make_event(
        &sub_id,
        EventMsg::AgentMessage(AgentMessageEvent {
            message: message.clone(),
        }),
    );
    sess.send_event(event).await;
    let assistant_summary = ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText { text: message }],
    };
    {
        let mut state = sess.state.lock_unchecked();
        state
            .history
            .record_items(std::slice::from_ref(&assistant_summary));
        let snapshot = state.history.contents();
        debug_history("after_compact_summary", &snapshot);
    }
    sess
        .persist_rollout_items(&[RolloutItem::ResponseItem(assistant_summary.clone())])
        .await;
    let event = sess.make_event(
        &sub_id,
        EventMsg::TaskComplete(TaskCompleteEvent {
            last_agent_message: None,
        }),
    );
    sess.send_event(event).await;

    {
        let state = sess.state.lock_unchecked();
        state.history.contents()
    }
}

fn map_approval(a: AskForApprovalCore) -> codex_protocol::protocol::AskForApproval {
    match a {
        AskForApprovalCore::UnlessTrusted => codex_protocol::protocol::AskForApproval::UnlessTrusted,
        AskForApprovalCore::OnFailure => codex_protocol::protocol::AskForApproval::OnFailure,
        AskForApprovalCore::OnRequest => codex_protocol::protocol::AskForApproval::OnRequest,
        AskForApprovalCore::Never => codex_protocol::protocol::AskForApproval::Never,
    }
}

fn map_sandbox(s: &SandboxPolicyCore) -> codex_protocol::protocol::SandboxPolicy {
    match s {
        SandboxPolicyCore::DangerFullAccess => codex_protocol::protocol::SandboxPolicy::DangerFullAccess,
        SandboxPolicyCore::ReadOnly => codex_protocol::protocol::SandboxPolicy::ReadOnly,
        SandboxPolicyCore::WorkspaceWrite {
            writable_roots,
            network_access,
            exclude_tmpdir_env_var,
            exclude_slash_tmp,
            ..
        } => codex_protocol::protocol::SandboxPolicy::WorkspaceWrite {
            writable_roots: writable_roots.clone(),
            network_access: *network_access,
            exclude_tmpdir_env_var: *exclude_tmpdir_env_var,
            exclude_slash_tmp: *exclude_slash_tmp,
        },
    }
}

fn map_effort(effort: ReasoningEffortConfig) -> Option<ReasoningEffortProtocol> {
    match effort {
        ReasoningEffortConfig::Minimal => Some(ReasoningEffortProtocol::Minimal),
        ReasoningEffortConfig::Low => Some(ReasoningEffortProtocol::Low),
        ReasoningEffortConfig::Medium => Some(ReasoningEffortProtocol::Medium),
        ReasoningEffortConfig::High => Some(ReasoningEffortProtocol::High),
        ReasoningEffortConfig::None => None,
    }
}

fn map_summary(summary: ReasoningSummaryConfig) -> ReasoningSummaryProtocol {
    match summary {
        ReasoningSummaryConfig::Auto => ReasoningSummaryProtocol::Auto,
        ReasoningSummaryConfig::Concise => ReasoningSummaryProtocol::Concise,
        ReasoningSummaryConfig::Detailed => ReasoningSummaryProtocol::Detailed,
        ReasoningSummaryConfig::None => ReasoningSummaryProtocol::None,
    }
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
                let mut state = sess.state.lock_unchecked();
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
    // Treat XML-tagged user instruction and environment blocks as session prefix.
    if matches!(
        InputMessageKind::from(("user", text)),
        InputMessageKind::UserInstructions | InputMessageKind::EnvironmentContext
    ) {
        return true;
    }

    // Also treat our compact bridge as a session prefix so it is excluded
    // from future compact runs. Handle both the new explicit marker and the
    // historical unmarked bridge text to be robust when resuming older sessions.
    let trimmed = text.trim_start();
    if trimmed.starts_with("[COMPACT_BRIDGE]") {
        return true;
    }
    if trimmed.starts_with("You were originally given instructions from a user over one or more turns.") {
        return true;
    }

    false
}

pub(crate) fn build_compacted_history(
    initial_context: Vec<ResponseItem>,
    user_messages: &[String],
    summary_text: &str,
) -> Vec<ResponseItem> {
    let mut history = initial_context;
    let user_messages_text = if user_messages.is_empty() {
        "(none)".to_string()
    } else {
        user_messages.join("\n\n")
    };
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
        // Tag the compact bridge so subsequent auto-compact passes (e.g. after resume)
        // can recognize and skip it when collecting prior user messages. This prevents
        // the bridge from being re-embedded and growing the prompt.
        content: vec![ContentItem::InputText { text: format!("[COMPACT_BRIDGE]\n{}", bridge) }],
    });
    history
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
}
