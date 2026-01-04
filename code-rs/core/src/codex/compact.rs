use std::collections::HashSet;
use std::sync::Arc;

use super::streaming::AgentTask;
use super::Session;
use super::compact_remote;
use super::TurnContext;
use super::streaming::get_last_assistant_message_from_turn;
use crate::Prompt;
use crate::client_common::ResponseEvent;
use crate::environment_context::EnvironmentContext;
use crate::error::CodexErr;
use crate::error::RetryAfter;
use crate::error::Result as CodexResult;
use crate::protocol::AgentMessageEvent;
use crate::protocol::ErrorEvent;
use crate::protocol::EventMsg;
use code_protocol::protocol::CompactionCheckpointWarningEvent;
use crate::protocol::InputItem;
use crate::protocol::TaskCompleteEvent;
use crate::truncate::truncate_middle;
use crate::util::backoff;
use askama::Template;
use code_protocol::models::ContentItem;
use code_protocol::models::FunctionCallOutputPayload;
use code_protocol::models::ResponseInputItem;
use code_protocol::models::ResponseItem;
use code_protocol::protocol::CompactedItem;
use code_protocol::protocol::InputMessageKind;
use code_protocol::protocol::RolloutItem;
use base64::Engine;
use chrono::Utc;
use futures::prelude::*;
use code_app_server_protocol::AuthMode;
use std::time::Duration;

pub const SUMMARIZATION_PROMPT: &str = include_str!("../../templates/compact/prompt.md");
pub const COMPACTION_CHECKPOINT_MESSAGE: &str = "History checkpoint: earlier conversation compacted.";
const COMPACT_USER_MESSAGE_MAX_TOKENS: usize = 20_000;
const COMPACT_TEXT_CONTENT_MAX_BYTES: usize = 8 * 1024;
const COMPACT_TOOL_ARGS_MAX_BYTES: usize = 4 * 1024;
const COMPACT_TOOL_OUTPUT_MAX_BYTES: usize = 4 * 1024;
const COMPACT_IMAGE_URL_MAX_BYTES: usize = 512;
const MAX_COMPACTION_SNIPPETS: usize = 12;

/// Determine whether to use remote compaction (ChatGPT-based) or local compaction.
///
/// Upstream codex-rs checks if auth mode is ChatGPT and RemoteCompaction feature is enabled.
/// In code-rs, remote compaction infrastructure is not yet implemented, so this always
/// returns false (always use local compaction).
///
/// TODO: Once ChatGPT auth and remote compaction are implemented, update this to:
/// ```
/// session
///     .services
///     .auth_manager
///     .auth()
///     .is_some_and(|auth| auth.mode == AuthMode::ChatGPT)
///     && session.enabled(Feature::RemoteCompaction).await
/// ```
pub(super) async fn should_use_remote_compact_task(session: &Session) -> bool {
    session
        .client
        .get_auth_manager()
        .and_then(|manager| manager.auth())
        .is_some_and(|auth| auth.mode == AuthMode::ChatGPT)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompactionSnippet {
    pub role: String,
    pub text: String,
}

#[derive(Template)]
#[template(path = "compact/history_bridge.md", escape = "none")]
struct HistoryBridgeTemplate<'a> {
    snippets: &'a [CompactionSnippet],
    summary_text: &'a str,
}

pub fn collect_compaction_snippets(items: &[ResponseItem]) -> Vec<CompactionSnippet> {
    let mut snippets = Vec::new();
    let mut total_bytes = 0usize;

    for item in items.iter().rev() {
        if let ResponseItem::Message { role, content, .. } = item {
            if role != "user" && role != "assistant" {
                continue;
            }
            let Some(text) = content_items_to_text(content) else {
                continue;
            };
            if role == "user" && is_session_prefix_message(&text) {
                continue;
            }
            let truncated = truncate_for_compact(text, COMPACT_TEXT_CONTENT_MAX_BYTES);
            if truncated.trim().is_empty() {
                continue;
            }
            if snippets.len() >= MAX_COMPACTION_SNIPPETS {
                break;
            }
            let snippet_len = truncated.len();
            if !snippets.is_empty() && total_bytes + snippet_len > COMPACT_USER_MESSAGE_MAX_TOKENS * 4 {
                break;
            }
            total_bytes += snippet_len;
            snippets.push(CompactionSnippet {
                role: role.clone(),
                text: truncated,
            });
        }
    }

    snippets.reverse();
    snippets
}

pub fn render_compaction_summary(snippets: &[CompactionSnippet], summary_text: &str) -> String {
    let normalized_summary = if summary_text.trim().is_empty() {
        "(no summary available)".to_string()
    } else {
        summary_text.to_string()
    };

    HistoryBridgeTemplate {
        snippets,
        summary_text: normalized_summary.as_str(),
    }
    .render()
    .unwrap_or(normalized_summary)
}

pub fn make_compaction_summary_message(
    snippets: &[CompactionSnippet],
    summary_text: &str,
) -> ResponseItem {
    let text = render_compaction_summary(snippets, summary_text);
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText { text }],
    }
}

/// Resolve the compaction prompt text based on an optional override.
///
/// Empty strings are treated as missing so we always fall back to the embedded
/// template instead of sending a blank developer message.
pub fn resolve_compact_prompt_text(override_prompt: Option<&str>) -> String {
    if let Some(text) = override_prompt {
        if !text.trim().is_empty() {
            return text.to_string();
        }
    }
    SUMMARIZATION_PROMPT.to_string()
}

pub(super) fn spawn_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    sub_id: String,
    input: Vec<InputItem>,
) {
    let task = AgentTask::compact(sess.clone(), turn_context, sub_id, input);
    // set_task is synchronous in our fork
    sess.set_task(task);
}

pub(super) async fn run_inline_auto_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
) -> Vec<ResponseItem> {
    let sub_id = sess.next_internal_sub_id();
    let prompt_text = resolve_compact_prompt_text(turn_context.compact_prompt_override.as_deref());
    let input = vec![InputItem::Text { text: prompt_text.clone() }];
    run_compact_task_inner_inline(sess, turn_context, sub_id, input).await
}

pub(super) async fn run_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    sub_id: String,
    input: Vec<InputItem>,
) {
    let start_event = sess.make_event(&sub_id, EventMsg::TaskStarted);
    sess.send_event(start_event).await;
    let compaction_result = if should_use_remote_compact_task(&sess).await {
        compact_remote::run_remote_compact_task(
            Arc::clone(&sess),
            Arc::clone(&turn_context),
            sub_id.clone(),
            input,
        )
        .await
    } else {
        perform_compaction(
            sess.clone(),
            turn_context,
            sub_id.clone(),
            input,
            true,
        )
        .await
    };

    let _ = compaction_result;
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
    remove_task_on_completion: bool,
) -> CodexResult<()> {
    // Convert core InputItem -> ResponseInputItem using the same logic as the main turn flow
    let initial_input_for_turn: ResponseInputItem = response_input_from_core_items(input);
    let mut turn_input = sess.turn_input_with_history(vec![initial_input_for_turn.clone().into()]);

    turn_input = sanitize_items_for_compact(turn_input);

    let max_retries = turn_context.client.get_provider().stream_max_retries();
    let mut retries = 0;
    let mut truncated_count = 0usize;

    // Do not persist a TurnContext rollout item here; inline compaction is a
    // background maintenance task and should not affect rollout reconstruction.

    loop {
        prune_orphan_tool_outputs(&mut turn_input);

        let mut prompt = Prompt::default();
        prompt.input = turn_input.clone();
        prompt.store = !sess.disable_response_storage;
        prompt.user_instructions = turn_context.user_instructions.clone();
        prompt.environment_context = Some(EnvironmentContext::new(
            Some(turn_context.cwd.clone()),
            Some(turn_context.approval_policy),
            Some(turn_context.sandbox_policy.clone()),
            Some(sess.user_shell.clone()),
        ));
        prompt.model_descriptions = sess.model_descriptions.clone();
        prompt.log_tag = Some("codex/compact".to_string());

        match drain_to_completed(&sess, turn_context.as_ref(), &prompt).await {
            Ok(()) => {
                if truncated_count > 0 {
                    tracing::warn!(
                        "Context window exceeded during compact; trimmed {truncated_count} item(s) from prompt"
                    );
                }
                break;
            }
            Err(CodexErr::Interrupted) => return Err(CodexErr::Interrupted),
            Err(CodexErr::UsageLimitReached(limit_err)) => {
                let now = Utc::now();
                let retry_after = limit_err
                    .retry_after(now)
                    .unwrap_or_else(|| RetryAfter::from_duration(Duration::from_secs(5 * 60), now));
                let mut message = format!("{limit_err} Auto-retrying");
                message.push('…');
                sess.notify_stream_error(&sub_id, message).await;
                tokio::time::sleep(retry_after.delay).await;
                retries = 0;
                continue;
            }
            Err(e) if is_context_overflow_error(&e) => {
                if turn_input.len() > 1 {
                    tracing::warn!(
                        "Context window exceeded while compacting; dropping oldest item ({} remaining)",
                        turn_input.len().saturating_sub(1)
                    );
                    turn_input.remove(0);
                    truncated_count = truncated_count.saturating_add(1);
                    retries = 0;
                    continue;
                }

                // Compaction cannot fit even with minimal input. Apply emergency
                // fallback to prevent infinite retry loops: reset history to just
                // initial context with a warning message.
                tracing::error!(
                    "Compaction failed: context overflow even with minimal input. \
                     Applying emergency fallback history to prevent retry loop."
                );

                let emergency_message = "⚠️ Compaction failed: The conversation history is too large \
                    to compact within the model's context limits. The history has been reset to prevent \
                    further errors. Please start a new session or manually reduce context by clearing history.";

                let event = sess.make_event(
                    &sub_id,
                    EventMsg::Error(ErrorEvent {
                        message: emergency_message.to_string(),
                    }),
                );
                sess.send_event(event).await;

                // Apply emergency fallback: reset history to minimal state
                let initial_context = sess.build_initial_context(turn_context.as_ref());
                let emergency_history = build_emergency_compacted_history(initial_context, emergency_message);
                sess.replace_history(emergency_history);

                // Still return error to signal compaction didn't complete normally,
                // but history has been reset to prevent retry loops
                return Err(e);
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
    let snippets = collect_compaction_snippets(&history_snapshot);
    let initial_context = sess.build_initial_context(turn_context.as_ref());
    let new_history = build_compacted_history(initial_context, &snippets, &summary_text);

    // Replace session history in-place using the canonical helper so any future
    // state bookkeeping stays centralized.
    sess.replace_history(new_history);

    send_compaction_checkpoint_warning(&sess, &sub_id).await;

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

/// Run compaction inline, update the session history in-place, and return the rebuilt compact history.
async fn run_compact_task_inner_inline(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    sub_id: String,
    input: Vec<InputItem>,
) -> Vec<ResponseItem> {
    // Convert core InputItem -> ResponseInputItem and build prompt
    let initial_input_for_turn: ResponseInputItem = response_input_from_core_items(input);
    let mut turn_input = sess.turn_input_with_history(vec![initial_input_for_turn.clone().into()]);

    turn_input = sanitize_items_for_compact(turn_input);

    let max_retries = turn_context.client.get_provider().stream_max_retries();
    let mut retries = 0;
    let mut truncated_count = 0usize;
    loop {
        let mut prompt = Prompt::default();
        prompt.input = turn_input.clone();
        prompt.store = !sess.disable_response_storage;
        prompt.user_instructions = turn_context.user_instructions.clone();
        prompt.environment_context = Some(EnvironmentContext::new(
            Some(turn_context.cwd.clone()),
            Some(turn_context.approval_policy),
            Some(turn_context.sandbox_policy.clone()),
            Some(sess.user_shell.clone()),
        ));
        prompt.model_descriptions = sess.model_descriptions.clone();
        prompt.log_tag = Some("codex/compact".to_string());

        match drain_to_completed(&sess, turn_context.as_ref(), &prompt).await {
            Ok(()) => {
                if truncated_count > 0 {
                    tracing::warn!(
                        "Context window exceeded during inline compact; trimmed {truncated_count} item(s) from prompt"
                    );
                }
                break;
            }
            Err(CodexErr::Interrupted) => return Vec::new(),
            Err(CodexErr::UsageLimitReached(limit_err)) => {
                let now = Utc::now();
                let retry_after = limit_err
                    .retry_after(now)
                    .unwrap_or_else(|| RetryAfter::from_duration(Duration::from_secs(5 * 60), now));
                let mut message = format!("{limit_err} Auto-retrying");
                message.push('…');
                sess.notify_stream_error(&sub_id, message).await;
                tokio::time::sleep(retry_after.delay).await;
                retries = 0;
                continue;
            }
            Err(e) if is_context_overflow_error(&e) => {
                if turn_input.len() > 1 {
                    tracing::warn!(
                        "Context window exceeded while compacting; dropping oldest item ({} remaining)",
                        turn_input.len().saturating_sub(1)
                    );
                    turn_input.remove(0);
                    truncated_count = truncated_count.saturating_add(1);
                    retries = 0;
                    continue;
                }

                // Compaction cannot fit even with minimal input. Return an emergency
                // fallback history with just the initial context and a clear warning.
                // This prevents infinite loops where compaction repeatedly fails and
                // the system keeps retrying with the same oversized input.
                tracing::error!(
                    "Compaction failed: context overflow even with minimal input. \
                     Returning emergency fallback history to prevent retry loop."
                );

                let emergency_message = "⚠️ Compaction failed: The conversation history is too large \
                    to compact within the model's context limits. The history has been reset to prevent \
                    further errors. Please start a new session or manually reduce context by clearing history.";

                let event = sess.make_event(
                    &sub_id,
                    EventMsg::Error(ErrorEvent {
                        message: emergency_message.to_string(),
                    }),
                );
                sess.send_event(event).await;

                // Return minimal emergency history: just initial context + warning message
                let initial_context = sess.build_initial_context(turn_context.as_ref());
                let emergency_history = build_emergency_compacted_history(initial_context, emergency_message);

                // Update session history with emergency fallback
                {
                    let mut state = sess.state.lock().unwrap();
                    state.history = crate::conversation_history::ConversationHistory::new();
                    state.history.record_items(emergency_history.iter());
                    state.token_usage_info = None;
                }

                return emergency_history;
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

    let history_snapshot = {
        let state = sess.state.lock().unwrap();
        state.history.contents()
    };
    let summary_text = get_last_assistant_message_from_turn(&history_snapshot).unwrap_or_default();
    let snippets = collect_compaction_snippets(&history_snapshot);
    let initial_context = sess.build_initial_context(turn_context.as_ref());
    let new_history = build_compacted_history(initial_context, &snippets, &summary_text);

    {
        let mut state = sess.state.lock().unwrap();
        state.history = crate::conversation_history::ConversationHistory::new();
        state.history.record_items(new_history.iter());
        state.token_usage_info = None;
    }

    send_compaction_checkpoint_warning(&sess, &sub_id).await;

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

fn looks_like_context_overflow(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("context_length_exceeded")
        || lower.contains("context length exceeded")
        || lower.contains("context window")
            && (lower.contains("exceed")
                || lower.contains("exceeded")
                || lower.contains("full")
                || lower.contains("too long"))
        || lower.contains("maximum context length")
        || lower.contains("exceeds the context window")
}

pub(super) fn is_context_overflow_error(err: &CodexErr) -> bool {
    match err {
        CodexErr::UnexpectedStatus(resp) => looks_like_context_overflow(&resp.body),
        CodexErr::Stream(msg, _, _) => looks_like_context_overflow(msg),
        _ => false,
    }
}

pub fn sanitize_items_for_compact(items: Vec<ResponseItem>) -> Vec<ResponseItem> {
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
                                    text: format!(
                                        "(image omitted for compaction; {bytes} bytes)",
                                    ),
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

/// Remove tool outputs that no longer have a matching tool call in the
/// conversation slice. This can happen if earlier items are trimmed for
/// context overflow, leaving orphaned outputs that will be rejected by the
/// compact endpoint.
pub fn prune_orphan_tool_outputs(items: &mut Vec<ResponseItem>) -> usize {
    let mut seen_calls: HashSet<String> = HashSet::new();

    for item in items.iter() {
        match item {
            ResponseItem::FunctionCall { call_id, .. }
            | ResponseItem::CustomToolCall { call_id, .. } => {
                seen_calls.insert(call_id.clone());
            }
            ResponseItem::LocalShellCall {
                id,
                call_id,
                ..
            } => {
                if let Some(call_id) = call_id {
                    seen_calls.insert(call_id.clone());
                }
                if let Some(id) = id {
                    // Chat Completions flow sets only `id`; outputs use that value as call_id.
                    seen_calls.insert(id.clone());
                }
            }
            _ => {}
        }
    }

    let before = items.len();
    items.retain(|item| match item {
        ResponseItem::FunctionCallOutput { call_id, .. }
        | ResponseItem::CustomToolCallOutput { call_id, .. } => seen_calls.contains(call_id),
        _ => true,
    });

    let removed = before.saturating_sub(items.len());
    if removed > 0 {
        tracing::warn!("Dropping {removed} orphaned tool output(s) during compaction");
    }

    removed
}

fn compaction_checkpoint_warning_event() -> EventMsg {
    EventMsg::CompactionCheckpointWarning(CompactionCheckpointWarningEvent {
        message: COMPACTION_CHECKPOINT_MESSAGE.to_string(),
    })
}

pub(super) async fn send_compaction_checkpoint_warning(sess: &Arc<Session>, sub_id: &str) {
    let event = sess.make_event(sub_id, compaction_checkpoint_warning_event());
    sess.send_event(event).await;
}

#[cfg(test)]
pub(crate) fn collect_user_messages(items: &[ResponseItem]) -> Vec<String> {
    collect_compaction_snippets(items)
        .into_iter()
        .filter(|snippet| snippet.role == "user")
        .map(|snippet| snippet.text)
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
    snippets: &[CompactionSnippet],
    summary_text: &str,
) -> Vec<ResponseItem> {
    let mut history = initial_context;
    history.push(make_compaction_summary_message(snippets, summary_text));
    history
}

/// Build an emergency fallback history when compaction fails catastrophically.
/// This returns just the initial context plus a warning message, ensuring the
/// session can continue without hitting infinite retry loops.
pub(crate) fn build_emergency_compacted_history(
    initial_context: Vec<ResponseItem>,
    warning_message: &str,
) -> Vec<ResponseItem> {
    let mut history = initial_context;
    history.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: warning_message.to_string(),
        }],
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
pub(super) fn response_input_from_core_items(items: Vec<InputItem>) -> ResponseInputItem {
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
    fn resolve_compact_prompt_text_prefers_override() {
        let text = resolve_compact_prompt_text(Some("custom prompt"));
        assert_eq!(text, "custom prompt");
    }

    #[test]
    fn resolve_compact_prompt_text_falls_back_on_blank() {
        let text = resolve_compact_prompt_text(Some("   \n\t"));
        assert_eq!(text, SUMMARIZATION_PROMPT);
    }

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
    fn collect_compaction_snippets_limits_messages() {
        let mut items = Vec::new();
        for idx in 0..15 {
            items.push(ResponseItem::Message {
                id: None,
                role: if idx % 2 == 0 { "user".to_string() } else { "assistant".to_string() },
                content: vec![ContentItem::InputText {
                    text: format!("Message #{idx} {}", "x".repeat(1024)),
                }],
            });
        }

        let snippets = collect_compaction_snippets(&items);
        assert!(snippets.len() <= MAX_COMPACTION_SNIPPETS);
        assert!(snippets.iter().any(|snippet| snippet.role == "user"));
        assert!(snippets.last().unwrap().text.contains("Message #14"));
    }

    #[test]
    fn make_compaction_summary_message_renders_template() {
        let snippets = vec![
            CompactionSnippet {
                role: "user".to_string(),
                text: "Investigate bug".to_string(),
            },
            CompactionSnippet {
                role: "assistant".to_string(),
                text: "Proposed fix".to_string(),
            },
        ];
        let message = make_compaction_summary_message(&snippets, "Apply patch to parser");
        let ResponseItem::Message { content, .. } = message else {
            panic!("expected message variant");
        };
        let body = content_items_to_text(&content).expect("text body");
        assert!(body.contains("(user) Investigate bug"));
        assert!(body.contains("Key takeaways"));
        assert!(body.contains("Apply patch to parser"));
    }

    #[test]
    fn build_compacted_history_truncates_overlong_user_messages() {
        // Prepare a very large prior user message so the aggregated
        // `user_messages_text` exceeds the truncation threshold used by
        // `build_compacted_history` (80k bytes).
        let big = "X".repeat(200_000);
        let snippet_source = vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text: big.clone() }],
        }];
        let snippets = collect_compaction_snippets(&snippet_source);
        let history = build_compacted_history(Vec::new(), &snippets, "SUMMARY");

        // Expect exactly one bridge message added to history (plus any initial context we provided, which is none).
        assert_eq!(history.len(), 1);

        // Extract the text content of the bridge message.
        let bridge_text = match &history[0] {
            ResponseItem::Message { role, content, .. } if role == "user" => {
                content_items_to_text(content).unwrap_or_default()
            }
            other => panic!("unexpected item in history: {other:?}"),
        };

        assert!(bridge_text.contains("Key takeaways"));
        assert!(bridge_text.contains("SUMMARY"));
        assert!(bridge_text.len() < big.len());
    }

    #[test]
    fn build_emergency_compacted_history_creates_minimal_history() {
        let initial_context = vec![
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "<user_instructions>test</user_instructions>".to_string(),
                }],
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "<ENVIRONMENT_CONTEXT>cwd=/tmp</ENVIRONMENT_CONTEXT>".to_string(),
                }],
            },
        ];

        let warning = "Emergency fallback";
        let history = build_emergency_compacted_history(initial_context.clone(), warning);

        assert_eq!(history.len(), initial_context.len() + 1);

        if let ResponseItem::Message { role, content, .. } = &history[history.len() - 1] {
            assert_eq!(role, "user");
            let text = content_items_to_text(content).unwrap();
            assert_eq!(text, warning);
        } else {
            panic!("Expected warning message");
        }

        assert_eq!(history[0], initial_context[0]);
        assert_eq!(history[1], initial_context[1]);
    }

    #[test]
    fn build_emergency_compacted_history_with_empty_context() {
        let warning = "Emergency fallback";
        let history = build_emergency_compacted_history(Vec::new(), warning);

        assert_eq!(history.len(), 1);

        if let ResponseItem::Message { role, content, .. } = &history[0] {
            assert_eq!(role, "user");
            let text = content_items_to_text(content).unwrap();
            assert_eq!(text, warning);
        } else {
            panic!("Expected warning message");
        }
    }

    #[test]
    fn compaction_checkpoint_warning_event_has_copy() {
        match compaction_checkpoint_warning_event() {
            EventMsg::CompactionCheckpointWarning(payload) => {
                assert!(payload.message.contains("checkpoint"));
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }
}
