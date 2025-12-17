use std::sync::Arc;

use super::compact::{
    is_context_overflow_error,
    prune_orphan_tool_outputs,
    response_input_from_core_items,
    sanitize_items_for_compact,
    send_compaction_checkpoint_warning,
};
use super::Session;
use super::TurnContext;
use crate::Prompt;
use crate::error::CodexErr;
use crate::error::Result as CodexResult;
use crate::error::RetryAfter;
use crate::protocol::AgentMessageEvent;
use crate::protocol::ErrorEvent;
use crate::protocol::EventMsg;
use crate::protocol::InputItem;
use code_protocol::models::ResponseInputItem;
use code_protocol::models::ResponseItem;
use code_protocol::protocol::CompactedItem;
use code_protocol::protocol::RolloutItem;
use crate::util::backoff;
use std::time::Duration;

pub(super) async fn run_inline_remote_auto_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    extra_input: Vec<InputItem>,
) -> Vec<ResponseItem> {
    let sub_id = sess.next_internal_sub_id();
    match run_remote_compact_task_inner(&sess, &turn_context, &sub_id, extra_input).await {
        Ok(history) => history,
        Err(err) => {
            let event = sess.make_event(
                &sub_id,
                EventMsg::Error(ErrorEvent {
                    message: format!("remote compact failed: {err}"),
                }),
            );
            sess.send_event(event).await;
            Vec::new()
        }
    }
}

pub(super) async fn run_remote_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
    sub_id: String,
    extra_input: Vec<InputItem>,
) -> CodexResult<()> {
    match run_remote_compact_task_inner(&sess, &turn_context, &sub_id, extra_input).await {
        Ok(_history) => {
            // Mirror local compaction behaviour: clear the running task when the
            // compaction finished successfully so the UI can unblock.
            sess.remove_task(&sub_id);
            Ok(())
        }
        Err(err) => {
            let event = sess.make_event(
                &sub_id,
                EventMsg::Error(ErrorEvent {
                    message: err.to_string(),
                }),
            );
            sess.send_event(event).await;
            Err(err)
        }
    }
}

async fn run_remote_compact_task_inner(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    sub_id: &str,
    extra_input: Vec<InputItem>,
) -> CodexResult<Vec<ResponseItem>> {
    let mut turn_items = sess.turn_input_with_history({
        if extra_input.is_empty() {
            Vec::new()
        } else {
            let response_input: ResponseInputItem = response_input_from_core_items(extra_input);
            vec![ResponseItem::from(response_input)]
        }
    });

    turn_items = sanitize_items_for_compact(turn_items);
    let mut truncated_count = 0usize;
    let max_retries = turn_context.client.get_provider().stream_max_retries();
    let mut retries = 0;
    let new_history = loop {
        prune_orphan_tool_outputs(&mut turn_items);

        let mut prompt = Prompt::default();
        prompt.input = turn_items.clone();
        prompt.base_instructions_override = turn_context.base_instructions.clone();
        prompt.include_additional_instructions = false;
        prompt.log_tag = Some("codex/remote-compact".to_string());

        sess.apply_remote_model_overrides(&mut prompt).await;

        match turn_context
            .client
            .compact_conversation_history(&prompt)
            .await
        {
            Ok(history) => {
                if truncated_count > 0 {
                    tracing::warn!(
                        "Context window exceeded during remote compact; trimmed {truncated_count} item(s) from prompt"
                    );
                }
                break history;
            }
            Err(err) if is_context_overflow_error(&err) => {
                if turn_items.len() > 1 {
                    tracing::warn!(
                        "Context window exceeded while remote compacting; dropping oldest item ({} remaining)",
                        turn_items.len().saturating_sub(1)
                    );
                    turn_items.remove(0);
                    truncated_count = truncated_count.saturating_add(1);
                    retries = 0;
                    continue;
                }

                return Err(err);
            }
            Err(CodexErr::UsageLimitReached(limit_err)) => {
                let now = chrono::Utc::now();
                let retry_after = limit_err
                    .retry_after(now)
                    .unwrap_or_else(|| RetryAfter::from_duration(Duration::from_secs(5 * 60), now));
                let mut message = format!("{limit_err} Auto-retrying");
                message.push('…');
                sess.notify_stream_error(sub_id, message).await;
                tokio::time::sleep(retry_after.delay).await;
                retries = 0;
                continue;
            }
            Err(err) => {
                if retries < max_retries {
                    retries += 1;
                    let delay = backoff(retries);
                    sess
                        .notify_stream_error(
                            sub_id,
                            format!(
                                "remote compact error: {err}; retrying {retries}/{max_retries} in {delay:?}…"
                            ),
                        )
                        .await;
                    tokio::time::sleep(delay).await;
                    continue;
                }

                return Err(err);
            }
        }
    };

    sess.replace_history(new_history.clone());
    {
        let mut state = sess.state.lock().unwrap();
        state.token_usage_info = None;
    }

    send_compaction_checkpoint_warning(sess, sub_id).await;

    let rollout_item = RolloutItem::Compacted(CompactedItem {
        message: "Conversation history compacted.".to_string(),
    });
    sess.persist_rollout_items(&[rollout_item]).await;

    let event = sess.make_event(
        sub_id,
        EventMsg::AgentMessage(AgentMessageEvent {
            message: "Compact task completed".to_string(),
        }),
    );
    sess.send_event(event).await;

    Ok(new_history)
}
