//! Tool (Web Search, MCP) lifecycle handlers extracted from ChatWidget.

use super::{ChatWidget, OrderKey};
use crate::history_cell;
use codex_core::protocol::{McpToolCallBeginEvent, McpToolCallEndEvent};

pub(super) fn web_search_begin(chat: &mut ChatWidget<'_>, call_id: String, query: Option<String>, key: OrderKey) {
    for cell in &chat.history_cells { cell.trigger_fade(); }
    chat.finalize_active_stream();
    chat.flush_interrupt_queue();

    let mut cell = history_cell::new_running_web_search(query.clone());
    cell.state_mut().call_id = Some(call_id.clone());
    let idx = chat.history_insert_with_key_global(Box::new(cell), key);
    tracing::info!("[order] WebSearchBegin call_id={} idx={}", call_id, idx);
    chat.tools_state
        .running_web_search
        .insert(super::ToolCallId(call_id), (idx, query));
    chat.bottom_pane.update_status_text("Search".to_string());
    chat.mark_needs_redraw();
}

pub(super) fn web_search_complete(chat: &mut ChatWidget<'_>, call_id: String, query: Option<String>) {
    let call_key = super::ToolCallId(call_id.clone());
    if let Some((idx, maybe_query)) = chat.tools_state.running_web_search.remove(&call_key) {
        let mut target_idx = None;
        if idx < chat.history_cells.len() {
            let is_ws = chat.history_cells[idx]
                .as_any()
                .downcast_ref::<history_cell::RunningToolCallCell>()
                .is_some_and(|rt| rt.has_title("Web Search..."));
            if is_ws { target_idx = Some(idx); }
        }
        if target_idx.is_none() {
            for i in (0..chat.history_cells.len()).rev() {
                if let Some(rt) = chat.history_cells[i]
                    .as_any()
                    .downcast_ref::<history_cell::RunningToolCallCell>()
                {
                    if rt.has_title("Web Search...") {
                        target_idx = Some(i);
                        break;
                    }
                }
            }
        }
        if let Some(i) = target_idx {
            if let Some(rt) = chat.history_cells[i]
                .as_any()
                .downcast_ref::<history_cell::RunningToolCallCell>()
            {
                let final_query = query.or(maybe_query);
                let mut completed = rt.finalize_web_search(true, final_query);
                completed.state_mut().call_id = Some(call_id.clone());
                chat.history_replace_at(i, Box::new(completed));
                chat.history_maybe_merge_tool_with_previous(i);
                tracing::info!("[order] WebSearchEnd replace at idx={}", i);
            }
        }
    }
    chat.bottom_pane.update_status_text("responding".to_string());
    chat.maybe_hide_spinner();
}

pub(super) fn mcp_begin(chat: &mut ChatWidget<'_>, ev: McpToolCallBeginEvent, key: OrderKey) {
    for cell in &chat.history_cells { cell.trigger_fade(); }
    let McpToolCallBeginEvent { call_id, invocation } = ev;
    let mut cell = history_cell::new_running_mcp_tool_call(invocation);
    cell.state_mut().call_id = Some(call_id.clone());
    let idx = chat.history_insert_with_key_global(Box::new(cell), key);
    let history_id = chat
        .history_state
        .history_id_for_tool_call(&call_id)
        .or_else(|| chat.history_cell_ids.get(idx).and_then(|slot| *slot));
    chat.tools_state
        .running_custom_tools
        .insert(
            super::ToolCallId(call_id),
            super::RunningToolEntry::new(key, idx).with_history_id(history_id),
        );
}

pub(super) fn mcp_end(chat: &mut ChatWidget<'_>, ev: McpToolCallEndEvent, key: OrderKey) {
    let McpToolCallEndEvent { call_id, duration, invocation, result } = ev;
    let success = !result.as_ref().map(|r| r.is_error.unwrap_or(false)).unwrap_or(false);
    let mut completed = history_cell::new_completed_mcp_tool_call(80, invocation, duration, success, result);
    if let Some(tool_cell) = completed
        .as_any_mut()
        .downcast_mut::<history_cell::ToolCallCell>()
    {
        tool_cell.state_mut().call_id = Some(call_id.clone());
    }
    if let Some(entry) = chat.tools_state.running_custom_tools.remove(&super::ToolCallId(call_id.clone())) {
        if let Some(idx) = chat.resolve_running_tool_index(&entry) {
            if idx < chat.history_cells.len() {
                chat.history_replace_at(idx, completed);
                return;
            }
        }
    }
    let _ = chat.history_insert_with_key_global(Box::new(completed), key);
}
