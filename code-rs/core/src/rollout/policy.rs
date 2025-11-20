use crate::protocol::event_msg_from_protocol;
use crate::protocol::EventMsg;
use code_protocol::models::ResponseItem;
use code_protocol::protocol::RolloutItem;

/// Whether a rollout `item` should be persisted in rollout files.
#[inline]
pub(crate) fn should_persist_rollout_item(item: &RolloutItem) -> bool {
    match item {
        RolloutItem::ResponseItem(item) => should_persist_response_item(item),
        RolloutItem::Event(ev) => event_msg_from_protocol(&ev.msg)
            .is_some_and(|msg| should_persist_event_msg(&msg)),
        // Always persist session meta
        RolloutItem::SessionMeta(_) => true,
        // Persist compacted summaries and turn context for accurate history reconstruction.
        RolloutItem::Compacted(_) | RolloutItem::TurnContext(_) => true,
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
        | ResponseItem::CustomToolCallOutput { .. }
        | ResponseItem::CompactionSummary { .. }
        | ResponseItem::WebSearchCall { .. } => true,
        ResponseItem::Other => false,
    }
}

/// Whether an [`EventMsg`] should be persisted.
#[inline]
pub(crate) fn should_persist_event_msg(ev: &EventMsg) -> bool {
    !matches!(
        ev,
        EventMsg::AgentMessageDelta(_)
            | EventMsg::AgentReasoningDelta(_)
            | EventMsg::AgentReasoningRawContentDelta(_)
    )
}
