//! Tool (Web Search, MCP) lifecycle handlers extracted from ChatWidget.

use super::{running_tools, web_search_sessions, ChatWidget, OrderKey};
use crate::history_cell;
use code_core::protocol::{McpToolCallBeginEvent, McpToolCallEndEvent, OrderMeta};

pub(super) fn web_search_begin(
    chat: &mut ChatWidget<'_>,
    call_id: String,
    query: Option<String>,
    order: Option<&OrderMeta>,
    key: OrderKey,
) {
    for cell in &chat.history_cells { cell.trigger_fade(); }
    chat.finalize_active_stream();
    chat.flush_interrupt_queue();

    web_search_sessions::handle_begin(chat, order, call_id, query, key);
}

pub(super) fn web_search_complete(
    chat: &mut ChatWidget<'_>,
    call_id: String,
    query: Option<String>,
    order: Option<&OrderMeta>,
    key: OrderKey,
) {
    running_tools::collapse_spinner(chat, &call_id);
    web_search_sessions::handle_complete(chat, order, call_id, query, key);
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
    let map_key = super::ToolCallId(call_id.clone());
    let entry_removed = chat
        .tools_state
        .running_custom_tools
        .remove(&map_key);
    let resolved_idx = entry_removed
        .as_ref()
        .and_then(|entry| running_tools::resolve_entry_index(chat, entry, &call_id));
    let resolved_idx =
        resolved_idx.or_else(|| running_tools::find_by_call_id(chat, &call_id));

    if let Some(idx) = resolved_idx {
        chat.history_replace_at(idx, Box::new(completed));
        chat.history_debug(format!(
            "mcp_tool_end.in_place call_id={} idx={} order=({}, {}, {})",
            call_id,
            idx,
            key.req,
            key.out,
            key.seq
        ));
    } else {
        chat.history_debug(format!(
            "mcp_tool_end.fallback_insert call_id={} order=({}, {}, {})",
            call_id,
            key.req,
            key.out,
            key.seq
        ));
        running_tools::collapse_spinner(chat, &call_id);
        let _ = chat.history_insert_with_key_global(Box::new(completed), key);
    }
}
