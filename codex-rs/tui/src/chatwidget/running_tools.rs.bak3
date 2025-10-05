use super::{history_cell, ChatWidget, RunningToolEntry, ToolCallId};
use crate::history::state::ArgumentValue;
use crate::history_cell::RunningToolCallCell;
use std::collections::HashMap;
use std::mem;

pub(super) fn rehydrate(chat: &mut ChatWidget<'_>) {
    let prev_custom = chat.tools_state.running_custom_tools.len();
    let prev_web = chat.tools_state.running_web_search.len();
    let prev_wait = chat.tools_state.running_wait_tools.len();
    let prev_kill = chat.tools_state.running_kill_tools.len();

    let old_state = mem::take(&mut chat.tools_state);
    let mut new_state = super::ToolState::default();
    chat.history_debug(format!(
        "running_tools.rehydrate.begin prev_custom={} prev_web={} prev_wait={} prev_kill={}",
        prev_custom, prev_web, prev_wait, prev_kill
    ));

    for (idx, cell) in chat.history_cells.iter().enumerate() {
        let Some(order_key) = chat.cell_order_seq.get(idx).copied() else {
            chat.history_debug(format!("running_tools.rehydrate.skip idx={} reason=no_order", idx));
            continue;
        };

        let Some(running_cell) = cell
            .as_any()
            .downcast_ref::<RunningToolCallCell>()
        else {
            continue;
        };

        let state = running_cell.state();
        let Some(call_id) = state.call_id.as_ref().filter(|cid| !cid.is_empty()) else {
            chat.history_debug(format!("running_tools.rehydrate.skip idx={} reason=no_call_id", idx));
            continue;
        };
        let tool_key = ToolCallId(call_id.clone());
        let history_id = chat.history_cell_ids.get(idx).and_then(|slot| *slot);

        if running_cell.has_title("Web Search...") {
            let query = state.arguments.iter().find_map(|arg| {
                if arg.name == "query" {
                    if let ArgumentValue::Text(text) = &arg.value {
                        Some(text.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            });
            new_state
                .running_web_search
                .insert(tool_key, (idx, query.clone()));
            chat.history_debug(format!(
                "running_tools.rehydrate.web call_id={} idx={} history_id={:?} order=({}, {}, {}) query_present={}",
                call_id,
                idx,
                history_id,
                order_key.req,
                order_key.out,
                order_key.seq,
                query.is_some()
            ));
            continue;
        }

        new_state
            .running_custom_tools
            .insert(tool_key, RunningToolEntry::new(order_key, idx).with_history_id(history_id));
        chat.history_debug(format!(
            "running_tools.rehydrate.custom call_id={} idx={} history_id={:?} order=({}, {}, {})",
            call_id,
            idx,
            history_id,
            order_key.req,
            order_key.out,
            order_key.seq
        ));
    }

    let super::ToolState {
        running_wait_tools,
        running_kill_tools,
        ..
    } = old_state;
    new_state.running_wait_tools = running_wait_tools;
    new_state.running_kill_tools = running_kill_tools;
    chat.tools_state = new_state;

    chat.history_debug(format!(
        "running_tools.rehydrate.end custom={} web={} wait={} kill={}",
        chat.tools_state.running_custom_tools.len(),
        chat.tools_state.running_web_search.len(),
        chat.tools_state.running_wait_tools.len(),
        chat.tools_state.running_kill_tools.len()
    ));
}

pub(super) fn resolve_entry_index(
    chat: &ChatWidget<'_>,
    entry: &RunningToolEntry,
    call_id: &str,
) -> Option<usize> {
    if let Some(id) = entry.history_id {
        if let Some(idx) = chat.cell_index_for_history_id(id) {
            return Some(idx);
        }
    }
    find_by_call_id(chat, call_id)
        .or_else(|| {
            chat
                .cell_order_seq
                .iter()
                .position(|key| *key == entry.order_key)
        })
        .or_else(|| {
            if entry.fallback_index < chat.history_cells.len() {
                Some(entry.fallback_index)
            } else {
                None
            }
        })
}

pub(super) fn find_by_call_id(chat: &ChatWidget<'_>, call_id: &str) -> Option<usize> {
    chat
        .history_cells
        .iter()
        .enumerate()
        .find_map(|(idx, cell)| {
            cell.as_any()
                .downcast_ref::<RunningToolCallCell>()
                .and_then(|running| {
                    running
                        .state()
                        .call_id
                        .as_deref()
                        .filter(|cid| *cid == call_id)
                        .map(|_| idx)
                })
        })
}

pub(super) fn collapse_spinner(chat: &mut ChatWidget<'_>, call_id: &str) {
    if let Some(idx) = find_by_call_id(chat, call_id) {
        chat.history_remove_at(idx);
    }
}

pub(super) fn finalize_all_due_to_answer(chat: &mut ChatWidget<'_>) {
    if chat.tools_state.running_custom_tools.is_empty() {
        return;
    }

    let entries: Vec<(ToolCallId, RunningToolEntry)> = chat
        .tools_state
        .running_custom_tools
        .iter()
        .map(|(k, entry)| (k.clone(), *entry))
        .collect();

    let mut unresolved: HashMap<ToolCallId, RunningToolEntry> = HashMap::new();
    let mut any_finalized = false;

    for (tool_id, entry) in entries {
        let call_id = tool_id.0.clone();
        let resolved_idx = resolve_entry_index(chat, &entry, &call_id)
            .or_else(|| find_by_call_id(chat, &call_id));

        let Some(idx) = resolved_idx else {
            chat.history_debug(format!(
                "running_tools.finalize_due_to_answer.pending call_id={}",
                call_id
            ));
            unresolved.insert(tool_id, entry);
            continue;
        };

        if idx >= chat.history_cells.len() {
            chat.history_debug(format!(
                "running_tools.finalize_due_to_answer.pending call_id={} reason=idx_oob idx={}",
                call_id,
                idx
            ));
            unresolved.insert(tool_id, entry);
            continue;
        }

        if chat.history_cells[idx]
            .as_any()
            .downcast_ref::<RunningToolCallCell>()
            .is_none()
        {
            chat.history_debug(format!(
                "running_tools.finalize_due_to_answer.pending call_id={} reason=cell_mismatch idx={}",
                call_id,
                idx
            ));
            unresolved.insert(tool_id, entry);
            continue;
        }

        let completed = history_cell::new_completed_custom_tool_call(
            "custom".to_string(),
            None,
            std::time::Duration::from_millis(0),
            true,
            "Final answer received".to_string(),
        );
        chat.history_replace_at(idx, Box::new(completed));
        chat.history_debug(format!(
            "running_tools.finalize_due_to_answer.finalized call_id={} idx={}",
            call_id,
            idx
        ));
        any_finalized = true;
    }

    chat.tools_state.running_custom_tools = unresolved;
    if any_finalized {
        chat.invalidate_height_cache();
        chat.request_redraw();
    }
    chat.history_debug(format!(
        "running_tools.finalize_due_to_answer.summary finalized={} pending={}",
        any_finalized,
        chat.tools_state.running_custom_tools.len()
    ));
}
