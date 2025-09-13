use crate::protocol::EventMsg;
use codex_protocol::protocol::RolloutItem;
use codex_protocol::models::ResponseItem;

/// Whether a rollout `item` should be persisted in rollout files.
#[inline]
pub(crate) fn is_persisted_response_item(item: &RolloutItem) -> bool {
    match item {
        RolloutItem::ResponseItem(item) => should_persist_response_item(item),
        RolloutItem::EventMsg(_ev) => {
            // We do not persist protocol EventMsg entries in this fork.
            false
        }
        // Always persist session meta
        RolloutItem::SessionMeta(_) => true,
        // Do not persist variants not used by this fork.
        RolloutItem::Compacted(_) | RolloutItem::TurnContext(_) => false,
    }
}

/// Whether a `ResponseItem` should be persisted in rollout files.
#[inline]
pub(crate) fn should_persist_response_item(item: &ResponseItem) -> bool {
    match item {
        ResponseItem::Message { .. }
        | ResponseItem::Reasoning { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::FunctionCallOutput { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::CustomToolCallOutput { .. } => true,
        ResponseItem::WebSearchCall { .. } | ResponseItem::Other => false,
    }
}

/// Whether an `EventMsg` should be persisted in rollout files.
#[inline]
#[allow(dead_code)]
pub(crate) fn should_persist_event_msg(ev: &EventMsg) -> bool {
    match ev {
        EventMsg::AgentMessage(_)
            | EventMsg::AgentReasoning(_)
            | EventMsg::AgentReasoningRawContent(_)
            | EventMsg::TokenCount(_) => true,
        _ => false,
    }
}
