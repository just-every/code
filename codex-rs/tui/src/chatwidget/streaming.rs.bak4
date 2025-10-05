//! Streaming-related helpers for `ChatWidget`.

use super::ChatWidget;
use crate::height_manager::HeightEvent;
use crate::streaming::controller::AppEventHistorySink;
use crate::streaming::StreamKind;

pub(super) fn on_commit_tick(chat: &mut ChatWidget<'_>) {
    let sink = AppEventHistorySink(chat.app_event_tx.clone());
    let _finished = chat.stream.on_commit_tick(&sink);
}

pub(super) fn is_write_cycle_active(chat: &ChatWidget<'_>) -> bool {
    chat.stream.is_write_cycle_active()
}

// Note: direct streaming is triggered from ChatWidget with explicit sequence numbers

// New facade: begin a stream for a kind, with optional id
pub(super) fn begin(chat: &mut ChatWidget<'_>, kind: StreamKind, id: Option<String>) {
    chat.stream_state.current_kind = Some(kind);
    let sink = AppEventHistorySink(chat.app_event_tx.clone());
    chat.stream.begin_with_id(kind, id, &sink);
}

// New facade: apply a delta (ensures begin is called for this id/kind)
pub(super) fn delta_text(chat: &mut ChatWidget<'_>, kind: StreamKind, id: String, delta: String, seq: Option<u64>) {
    chat.stream_state.current_kind = Some(kind);
    let sink = AppEventHistorySink(chat.app_event_tx.clone());
    let stream_id = id.clone();
    chat.stream.begin_with_id(kind, Some(id), &sink);
    chat.stream.set_last_sequence_number(kind, seq);
    
    chat.stream.push_and_maybe_commit(&delta, &sink);
    if matches!(kind, StreamKind::Answer) {
        chat.track_answer_stream_delta(&stream_id, &delta, seq);
    }
}

// New facade: finalize a specific kind and optionally follow bottom
pub(super) fn finalize(chat: &mut ChatWidget<'_>, kind: StreamKind, follow_bottom: bool) {
    let sink = AppEventHistorySink(chat.app_event_tx.clone());
    chat.stream_state.current_kind = Some(kind);
    chat.stream.finalize(kind, follow_bottom, &sink);
}

pub(super) fn finalize_active_stream(chat: &mut ChatWidget<'_>) {
    if chat.stream.is_write_cycle_active() {
        finalize(chat, StreamKind::Reasoning, true);
        finalize(chat, StreamKind::Answer, true);
        chat.height_manager
            .borrow_mut()
            .record_event(HeightEvent::HistoryFinalize);
    }
    chat.stream_state.current_kind = None;
}
