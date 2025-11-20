use std::sync::Arc;

use super::compact::{
    response_input_from_core_items,
    sanitize_items_for_compact,
    send_compaction_checkpoint_warning,
};
use super::Session;
use super::TurnContext;
use crate::Prompt;
use crate::error::Result as CodexResult;
use crate::protocol::AgentMessageEvent;
use crate::protocol::ErrorEvent;
use crate::protocol::EventMsg;
use crate::protocol::InputItem;
use code_protocol::models::ResponseInputItem;
use code_protocol::models::ResponseItem;
use code_protocol::protocol::CompactedItem;
use code_protocol::protocol::RolloutItem;

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

    let mut prompt = Prompt::default();
    prompt.input = turn_items;
    prompt.base_instructions_override = turn_context.base_instructions.clone();
    prompt.include_additional_instructions = false;
    prompt.log_tag = Some("codex/remote-compact".to_string());

    let new_history = turn_context
        .client
        .compact_conversation_history(&prompt)
        .await?;

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
