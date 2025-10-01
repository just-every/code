//! Exec and tool call lifecycle helpers for `ChatWidget`.

use super::ChatWidget;
use crate::app_event::AppEvent;
use crate::height_manager::HeightEvent;
use crate::history::state::{
    ExecAction,
    ExecRecord,
    ExecStatus,
    ExecWaitNote,
    ExploreRecord,
    HistoryDomainEvent,
    HistoryDomainRecord,
    HistoryId,
    HistoryMutation,
    HistoryRecord,
    InlineSpan,
    MessageLine,
    MessageLineKind,
    PlainMessageKind,
    PlainMessageRole,
    PlainMessageState,
    TextEmphasis,
    TextTone,
};
use crate::history_cell::CommandOutput;
use crate::history_cell::{self, HistoryCell};
use codex_core::parse_command::ParsedCommand;
use codex_core::protocol::{ExecCommandBeginEvent, ExecCommandEndEvent, OrderMeta};
use std::time::SystemTime;

fn find_trailing_explore_agg(chat: &ChatWidget<'_>) -> Option<usize> {
    if chat.is_reasoning_shown() {
        return None;
    }
    let mut idx = chat.history_cells.len();
    while idx > 0 {
        idx -= 1;
        let cell = &chat.history_cells[idx];
        if cell
            .as_any()
            .downcast_ref::<history_cell::CollapsibleReasoningCell>()
            .is_some()
        {
            continue;
        }
        if cell
            .as_any()
            .downcast_ref::<history_cell::ExploreAggregationCell>()
            .is_some()
        {
            return Some(idx);
        }
        break;
    }
    None
}

fn exec_record_from_begin(ev: &ExecCommandBeginEvent) -> ExecRecord {
    let action = history_cell::action_enum_from_parsed(&ev.parsed_cmd);
    ExecRecord {
        id: crate::history::state::HistoryId::ZERO,
        call_id: Some(ev.call_id.clone()),
        command: ev.command.clone(),
        parsed: ev.parsed_cmd.clone(),
        action,
        status: ExecStatus::Running,
        stdout_chunks: Vec::new(),
        stderr_chunks: Vec::new(),
        exit_code: None,
        wait_total: None,
        wait_active: false,
        wait_notes: Vec::new(),
        started_at: std::time::SystemTime::now(),
        completed_at: None,
        working_dir: Some(ev.cwd.clone()),
        env: Vec::new(),
        tags: Vec::new(),
    }
}

fn exec_wait_notes_from_pairs(pairs: &[(String, bool)]) -> Vec<ExecWaitNote> {
    pairs
        .iter()
        .map(|(message, is_error)| ExecWaitNote {
            message: message.clone(),
            tone: if *is_error {
                TextTone::Error
            } else {
                TextTone::Info
            },
            timestamp: SystemTime::now(),
        })
        .collect()
}

fn stream_tail(full: &str, streamed: &str) -> Option<String> {
    if full.is_empty() {
        return None;
    }
    if streamed.is_empty() {
        return Some(full.to_string());
    }
    if let Some(tail) = full.strip_prefix(streamed) {
        if tail.is_empty() {
            None
        } else {
            Some(tail.to_string())
        }
    } else {
        Some(full.to_string())
    }
}

fn history_record_for_cell(chat: &ChatWidget<'_>, idx: usize) -> Option<HistoryRecord> {
    if let Some(Some(id)) = chat.history_cell_ids.get(idx) {
        if let Some(record) = chat.history_state.record(*id).cloned() {
            return Some(record);
        }
    }
    chat.history_cells
        .get(idx)
        .and_then(|cell| history_cell::record_from_cell(cell.as_ref()))
}

fn exec_record_has_output(record: &ExecRecord) -> bool {
    !record.stdout_chunks.is_empty() || !record.stderr_chunks.is_empty()
}

pub(super) fn finalize_exec_cell_at(
    chat: &mut ChatWidget<'_>,
    idx: usize,
    exit_code: i32,
    stdout: String,
    stderr: String,
) {
    if idx >= chat.history_cells.len() {
        return;
    }
    if let Some(exec) = chat.history_cells[idx]
        .as_any()
        .downcast_ref::<history_cell::ExecCell>()
    {
        if exec.output.is_none() {
            let completed = history_cell::new_completed_exec_command(
                exec.command.clone(),
                exec.parsed.clone(),
                CommandOutput {
                    exit_code,
                    stdout,
                    stderr,
                },
            );
            chat.history_replace_at(idx, Box::new(completed));
        }
    }
}

pub(super) fn finalize_all_running_as_interrupted(chat: &mut ChatWidget<'_>) {
    let interrupted_msg = "Cancelled by user.".to_string();
    let stdout_empty = String::new();
    let running: Vec<(super::ExecCallId, Option<usize>, Option<(usize, usize)>)> = chat
        .exec
        .running_commands
        .iter()
        .map(|(k, v)| (k.clone(), v.history_index, v.explore_entry))
        .collect();
    let mut agg_was_updated = false;
    for (call_id, maybe_idx, explore_entry) in &running {
        if let Some(idx) = maybe_idx {
            finalize_exec_cell_at(
                chat,
                *idx,
                130,
                stdout_empty.clone(),
                interrupted_msg.clone(),
            );
        }
        if let Some((agg_idx, entry_idx)) = explore_entry {
            if *agg_idx < chat.history_cells.len() {
                if let Some(existing) = chat.history_cells[*agg_idx]
                    .as_any()
                    .downcast_ref::<history_cell::ExploreAggregationCell>()
                {
                    let mut record = existing.record().clone();
                    record.update_status(
                        *entry_idx,
                        history_cell::ExploreEntryStatus::Error { exit_code: None },
                    );
                    let cell = history_cell::ExploreAggregationCell::from_record(record.clone());
                    chat.history_replace_with_record(
                        *agg_idx,
                        Box::new(cell),
                        HistoryDomainRecord::Explore(record),
                    );
                    chat.autoscroll_if_near_bottom();
                    agg_was_updated = true;
                }
            }
        }
        chat.canceled_exec_call_ids.insert(call_id.clone());
    }
    chat.exec.running_commands.clear();
    if agg_was_updated {
        chat.exec.running_explore_agg_index = None;
        chat.invalidate_height_cache();
        chat.request_redraw();
    }

    if !chat.tools_state.running_custom_tools.is_empty() {
        let entries: Vec<(super::ToolCallId, super::RunningToolEntry)> = chat
            .tools_state
            .running_custom_tools
            .iter()
            .map(|(k, entry)| (k.clone(), *entry))
            .collect();
        for (_k, entry) in entries {
            if let Some(idx) = chat.resolve_running_tool_index(&entry) {
                if idx < chat.history_cells.len() {
                    let mut emphasis = TextEmphasis::default();
                    emphasis.bold = true;
                    let wait_state = PlainMessageState {
                        id: HistoryId::ZERO,
                        role: PlainMessageRole::Error,
                        kind: PlainMessageKind::Error,
                        header: None,
                        lines: vec![MessageLine {
                            kind: MessageLineKind::Paragraph,
                            spans: vec![InlineSpan {
                                text: "Wait cancelled".into(),
                                tone: TextTone::Error,
                                emphasis,
                                entity: None,
                            }],
                        }],
                        metadata: None,
                    };

                    let replaced = chat.history_cells[idx]
                        .as_any()
                        .downcast_ref::<history_cell::RunningToolCallCell>()
                        .map(|cell| cell.has_title("Waiting"))
                        .unwrap_or(false);

                    if replaced {
                        chat.history_replace_with_record(
                            idx,
                            Box::new(history_cell::PlainHistoryCell::from_state(wait_state.clone())),
                            HistoryDomainRecord::Plain(wait_state.clone()),
                        );
                    } else {
                        let completed = history_cell::new_completed_custom_tool_call(
                            "custom".to_string(),
                            None,
                            std::time::Duration::from_millis(0),
                            false,
                            "Cancelled by user.".to_string(),
                        );
                        chat.history_replace_at(idx, Box::new(completed));
                    }
                }
            }
        }
        chat.tools_state.running_custom_tools.clear();
        chat.invalidate_height_cache();
        chat.request_redraw();
    }

    if !chat.tools_state.running_web_search.is_empty() {
        let entries: Vec<(super::ToolCallId, (usize, Option<String>))> = chat
            .tools_state
            .running_web_search
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (call_id, (idx, query_opt)) in entries {
            let mut target_idx = None;
            if idx < chat.history_cells.len() {
                let is_ws = chat.history_cells[idx]
                    .as_any()
                    .downcast_ref::<history_cell::RunningToolCallCell>()
                    .is_some_and(|rt| rt.has_title("Web Search..."));
                if is_ws {
                    target_idx = Some(idx);
                }
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
                    let completed = rt.finalize_web_search(false, query_opt);
                    chat.history_replace_at(i, Box::new(completed));
                }
            }
            chat.tools_state.running_web_search.remove(&call_id);
        }
    }

    if !chat.tools_state.running_wait_tools.is_empty() {
        chat.tools_state.running_wait_tools.clear();
    }

    if !chat.tools_state.running_kill_tools.is_empty() {
        chat.tools_state.running_kill_tools.clear();
    }

    chat.bottom_pane.update_status_text("cancelled".to_string());
    let any_tasks_active = !chat.active_task_ids.is_empty();
    if !any_tasks_active {
        chat.bottom_pane.set_task_running(false);
    }
    chat.maybe_hide_spinner();
    chat.refresh_auto_drive_visuals();
}

pub(super) fn finalize_all_running_due_to_answer(chat: &mut ChatWidget<'_>) {
    let running: Vec<(super::ExecCallId, Option<usize>, Option<(usize, usize)>)> = chat
        .exec
        .running_commands
        .iter()
        .map(|(k, v)| (k.clone(), v.history_index, v.explore_entry))
        .collect();
    let mut remove_after_finalize: Vec<super::ExecCallId> = Vec::new();
    let mut agg_was_updated = false;
    for (call_id, maybe_idx, explore_entry) in &running {
        // Keep streaming Exec cells alive so background commands continue to surface output.
        if maybe_idx.is_some() {
            continue;
        }

        if let Some((agg_idx, entry_idx)) = explore_entry {
            if *agg_idx < chat.history_cells.len() {
                if let Some(existing) = chat.history_cells[*agg_idx]
                    .as_any()
                    .downcast_ref::<history_cell::ExploreAggregationCell>()
                {
                    let mut record = existing.record().clone();
                    record.update_status(*entry_idx, history_cell::ExploreEntryStatus::Success);
                    let cell = history_cell::ExploreAggregationCell::from_record(record.clone());
                    chat.history_replace_with_record(
                        *agg_idx,
                        Box::new(cell),
                        HistoryDomainRecord::Explore(record),
                    );
                    chat.autoscroll_if_near_bottom();
                    agg_was_updated = true;
                }
            }
        }

        remove_after_finalize.push(call_id.clone());
    }

    for call_id in remove_after_finalize {
        chat.exec.suppress_exec_end(call_id.clone());
        chat.exec.running_commands.remove(&call_id);
    }
    if agg_was_updated {
        chat.exec.running_explore_agg_index = None;
        chat.invalidate_height_cache();
        chat.request_redraw();
    }

    if !chat.tools_state.running_custom_tools.is_empty() {
        let entries: Vec<(super::ToolCallId, super::RunningToolEntry)> = chat
            .tools_state
            .running_custom_tools
            .iter()
            .map(|(k, entry)| (k.clone(), *entry))
            .collect();
        for (_k, entry) in entries {
            if let Some(idx) = chat.resolve_running_tool_index(&entry) {
                if idx < chat.history_cells.len() {
                    let completed = history_cell::new_completed_custom_tool_call(
                        "custom".to_string(),
                        None,
                        std::time::Duration::from_millis(0),
                        true,
                        "Final answer received".to_string(),
                    );
                    chat.history_replace_at(idx, Box::new(completed));
                }
            }
        }
        chat.tools_state.running_custom_tools.clear();
        chat.invalidate_height_cache();
        chat.request_redraw();
    }

    if !chat.tools_state.running_web_search.is_empty() {
        let entries: Vec<(super::ToolCallId, (usize, Option<String>))> = chat
            .tools_state
            .running_web_search
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (call_id, (idx, query_opt)) in entries {
            let mut target_idx = None;
            if idx < chat.history_cells.len() {
                let is_ws = chat.history_cells[idx]
                    .as_any()
                    .downcast_ref::<history_cell::RunningToolCallCell>()
                    .is_some_and(|rt| rt.has_title("Web Search..."));
                if is_ws {
                    target_idx = Some(idx);
                }
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
                    let completed = rt.finalize_web_search(true, query_opt);
                    chat.history_replace_at(i, Box::new(completed));
                }
            }
            chat.tools_state.running_web_search.remove(&call_id);
        }
    }

    chat.refresh_auto_drive_visuals();
}

pub(super) fn try_merge_completed_exec_at(chat: &mut ChatWidget<'_>, idx: usize) {
    if idx == 0 || idx >= chat.history_cells.len() {
        return;
    }

    let Some(HistoryRecord::Exec(current_exec)) = history_record_for_cell(chat, idx) else {
        return;
    };

    if !exec_record_has_output(&current_exec) {
        return;
    }

    if matches!(current_exec.action, ExecAction::Run) {
        return;
    }

    let Some(prev_record) = history_record_for_cell(chat, idx - 1) else {
        return;
    };

    match prev_record {
        HistoryRecord::Exec(prev_exec) => {
            if prev_exec.action != current_exec.action {
                return;
            }
            if !exec_record_has_output(&prev_exec) {
                return;
            }

            let merged = history_cell::MergedExecCell::from_records(
                prev_exec.id,
                prev_exec.action,
                vec![prev_exec.clone(), current_exec.clone()],
            );
            chat.history_replace_at(idx - 1, Box::new(merged));
            chat.history_remove_at(idx);
            chat.autoscroll_if_near_bottom();
            chat.bottom_pane.set_has_chat_history(true);
            chat.process_animation_cleanup();
            chat.app_event_tx.send(AppEvent::RequestRedraw);
        }
        HistoryRecord::MergedExec(mut merged_exec) => {
            if merged_exec.action != current_exec.action {
                return;
            }
            merged_exec.segments.push(current_exec.clone());
            let merged_cell = history_cell::MergedExecCell::from_state(merged_exec.clone());
            chat.history_replace_at(idx - 1, Box::new(merged_cell));
            chat.history_remove_at(idx);
            chat.autoscroll_if_near_bottom();
            chat.bottom_pane.set_has_chat_history(true);
            chat.process_animation_cleanup();
            chat.app_event_tx.send(AppEvent::RequestRedraw);
        }
        _ => {}
    }
}

fn try_upgrade_fallback_exec_cell(
    chat: &mut ChatWidget<'_>,
    ev: &ExecCommandBeginEvent,
) -> bool {
    for i in (0..chat.history_cells.len()).rev() {
        if let Some(exec) = chat.history_cells[i]
            .as_any_mut()
            .downcast_mut::<history_cell::ExecCell>()
        {
            let looks_like_fallback = exec.output.is_some()
                && exec.parsed.is_empty()
                && exec.command.len() == 1
                && exec.command
                    .first()
                    .map(|cmd| cmd == &ev.call_id)
                    .unwrap_or(false);
            if looks_like_fallback {
                exec.replace_command_metadata(ev.command.clone(), ev.parsed_cmd.clone());
                try_merge_completed_exec_at(chat, i);
                chat.invalidate_height_cache();
                chat.request_redraw();
                return true;
            }
        }
    }
    false
}

pub(super) fn handle_exec_begin_now(
    chat: &mut ChatWidget<'_>,
    ev: ExecCommandBeginEvent,
    order: &OrderMeta,
) {
    if chat
        .ended_call_ids
        .contains(&super::ExecCallId(ev.call_id.clone()))
    {
        if try_upgrade_fallback_exec_cell(chat, &ev) {
            return;
        }
        return;
    }
    for cell in &chat.history_cells {
        cell.trigger_fade();
    }
    let parsed_command = ev.parsed_cmd.clone();
    let action = history_cell::action_enum_from_parsed(&parsed_command);
    chat.height_manager
        .borrow_mut()
        .record_event(HeightEvent::RunBegin);

    let has_read_command = parsed_command
        .iter()
        .any(|p| matches!(p, ParsedCommand::ReadCommand { .. }));

    if matches!(
        action,
        ExecAction::Read | ExecAction::Search | ExecAction::List
    ) || has_read_command
    {
        let mut created_new = false;
        let mut agg_idx = chat.exec.running_explore_agg_index.and_then(|idx| {
            if idx < chat.history_cells.len()
                && chat.history_cells[idx]
                    .as_any()
                    .downcast_ref::<history_cell::ExploreAggregationCell>()
                    .is_some()
            {
                Some(idx)
            } else {
                None
            }
        });

        if agg_idx.is_none() {
            agg_idx = find_trailing_explore_agg(chat);
        }

        if agg_idx.is_none() {
            let key = ChatWidget::order_key_from_order_meta(order);
            let record = ExploreRecord {
                id: HistoryId::ZERO,
                entries: Vec::new(),
            };
            let idx = chat.history_insert_with_key_global_tagged(
                Box::new(history_cell::ExploreAggregationCell::from_record(record.clone())),
                key,
                "explore",
                Some(HistoryDomainRecord::Explore(record)),
            );
            created_new = true;
            agg_idx = Some(idx);
        }

        if let Some(idx) = agg_idx {
            if let Some(mut record) = chat.history_cells.get(idx).and_then(|cell| {
                cell.as_any()
                    .downcast_ref::<history_cell::ExploreAggregationCell>()
                    .map(|existing| existing.record().clone())
            }) {
                if let Some(entry_idx) = record.push_from_parsed(
                    &parsed_command,
                    history_cell::ExploreEntryStatus::Running,
                    &ev.cwd,
                    &chat.config.cwd,
                    &ev.command,
                ) {
                    let cell = history_cell::ExploreAggregationCell::from_record(record.clone());
                    chat.history_replace_with_record(
                        idx,
                        Box::new(cell),
                        HistoryDomainRecord::Explore(record),
                    );
                    chat.autoscroll_if_near_bottom();
                    chat.exec.running_explore_agg_index = Some(idx);
                    chat.exec.running_commands.insert(
                        super::ExecCallId(ev.call_id.clone()),
                        super::RunningCommand {
                            command: ev.command.clone(),
                            parsed: parsed_command.clone(),
                            history_index: None,
                            history_id: None,
                            explore_entry: Some((idx, entry_idx)),
                            stdout: String::new(),
                            stderr: String::new(),
                            wait_total: None,
                            wait_active: false,
                            wait_notes: Vec::new(),
                        },
                    );
                    chat.bottom_pane.set_has_chat_history(true);
                    let status_text = match action {
                        ExecAction::Read => "reading files…",
                        _ => "exploring…",
                    };
                    chat.bottom_pane.update_status_text(status_text.to_string());
                    chat.refresh_auto_drive_visuals();
                    return;
                }
            }

            if created_new {
                chat.history_remove_at(idx);
                chat.autoscroll_if_near_bottom();
                chat.request_redraw();
            }
        }
    }

    let exec_record = exec_record_from_begin(&ev);
    let key = ChatWidget::order_key_from_order_meta(order);
    let cell = history_cell::ExecCell::from_record(exec_record.clone());
    let idx = chat.history_insert_with_key_global_tagged(
        Box::new(cell),
        key,
        "exec-begin",
        Some(HistoryDomainRecord::Exec(exec_record)),
    );
    chat.exec.running_commands.insert(
        super::ExecCallId(ev.call_id.clone()),
        super::RunningCommand {
            command: ev.command.clone(),
            parsed: parsed_command,
            history_index: Some(idx),
            history_id: None,
            explore_entry: None,
            stdout: String::new(),
            stderr: String::new(),
            wait_total: None,
            wait_active: false,
            wait_notes: Vec::new(),
        },
    );
    if let Some(running) = chat
        .exec
        .running_commands
        .get_mut(&super::ExecCallId(ev.call_id.clone()))
    {
        let history_id = chat
            .history_state
            .history_id_for_exec_call(&ev.call_id)
            .or_else(|| chat.history_cell_ids.get(idx).and_then(|slot| *slot));
        running.history_id = history_id;
    }
    if !chat.tools_state.running_web_search.is_empty() {
        chat.bottom_pane.update_status_text("Search".to_string());
    } else {
        let preview = chat
            .exec
            .running_commands
            .get(&super::ExecCallId(ev.call_id.clone()))
            .map(|rc| rc.command.join(" "))
            .unwrap_or_else(|| "command".to_string());
        let preview_short = if preview.chars().count() > 40 {
            let mut truncated: String = preview.chars().take(40).collect();
            truncated.push('…');
            truncated
        } else {
            preview
        };
        chat.bottom_pane
            .update_status_text(format!("running command: {}", preview_short));
    }
    chat.refresh_auto_drive_visuals();
}

pub(super) fn handle_exec_end_now(
    chat: &mut ChatWidget<'_>,
    ev: ExecCommandEndEvent,
    order: &OrderMeta,
) {
    let call_id = super::ExecCallId(ev.call_id.clone());
    if chat.exec.should_suppress_exec_end(&call_id) {
        chat.exec.unsuppress_exec_end(&call_id);
        chat.ended_call_ids.insert(call_id);
        chat.maybe_hide_spinner();
        chat.refresh_auto_drive_visuals();
        return;
    }
    chat.ended_call_ids.insert(super::ExecCallId(ev.call_id.clone()));
    // If this call was already marked as cancelled, drop the End to avoid
    // inserting a duplicate completed cell after the user interrupt.
    if chat
        .canceled_exec_call_ids
        .remove(&super::ExecCallId(ev.call_id.clone()))
    {
        chat.maybe_hide_spinner();
        chat.refresh_auto_drive_visuals();
        return;
    }
    let ExecCommandEndEvent {
        call_id,
        exit_code,
        duration,
        stdout,
        stderr,
    } = ev;
    let cmd = chat
        .exec
        .running_commands
        .remove(&super::ExecCallId(call_id.clone()));
    chat.height_manager
        .borrow_mut()
        .record_event(HeightEvent::RunEnd);
    let (
        command,
        parsed,
        history_id,
        history_index,
        explore_entry,
        wait_total,
        wait_notes,
        streamed_stdout,
        streamed_stderr,
    ) = match cmd {
        Some(super::RunningCommand {
            command,
            parsed,
            history_index,
            history_id,
            explore_entry,
            wait_total,
            wait_notes,
            stdout: streamed_stdout,
            stderr: streamed_stderr,
            ..
        }) => (
            command,
            parsed,
            history_id,
            history_index,
            explore_entry,
            wait_total,
            wait_notes,
            streamed_stdout,
            streamed_stderr,
        ),
        None => (
            vec![call_id.clone()],
            vec![],
            None,
            None,
            None,
            None,
            Vec::new(),
            String::new(),
            String::new(),
        ),
    };

    if let Some((agg_idx, entry_idx)) = explore_entry {
        let action = history_cell::action_enum_from_parsed(&parsed);
        let status = match (exit_code, action) {
            (0, _) => history_cell::ExploreEntryStatus::Success,
            (1, ExecAction::Search) => history_cell::ExploreEntryStatus::NotFound,
            (1, ExecAction::List) => history_cell::ExploreEntryStatus::NotFound,
            _ => history_cell::ExploreEntryStatus::Error {
                exit_code: Some(exit_code),
            },
        };
        if let Some(mut record) = chat.history_cells.get(agg_idx).and_then(|cell| {
            cell.as_any()
                .downcast_ref::<history_cell::ExploreAggregationCell>()
                .map(|existing| existing.record().clone())
        }) {
            record.update_status(entry_idx, status.clone());
            let cell = history_cell::ExploreAggregationCell::from_record(record.clone());
            chat.history_replace_with_record(
                agg_idx,
                Box::new(cell),
                HistoryDomainRecord::Explore(record),
            );
            chat.autoscroll_if_near_bottom();
        }
        if !chat
            .exec
            .running_commands
            .values()
            .any(|rc| rc.explore_entry.is_some())
        {
            chat.exec.running_explore_agg_index = None;
        }
        let status_text = match status {
            history_cell::ExploreEntryStatus::Success => match action {
                ExecAction::Read => "files read".to_string(),
                _ => "exploration updated".to_string(),
            },
            history_cell::ExploreEntryStatus::NotFound => match action {
                ExecAction::List => "path not found".to_string(),
                _ => "no matches found".to_string(),
            },
            history_cell::ExploreEntryStatus::Error { .. } => match action {
                ExecAction::Read => format!("read failed (exit {exit_code})"),
                ExecAction::Search => {
                    if exit_code == 2 { "invalid pattern".to_string() } else { format!("search failed (exit {exit_code})") }
                }
                ExecAction::List => format!("list failed (exit {exit_code})"),
                _ => format!("exploration failed (exit {exit_code})"),
            },
            history_cell::ExploreEntryStatus::Running => "exploring…".to_string(),
        };
        chat.bottom_pane.update_status_text(status_text);
        chat.maybe_hide_spinner();
        chat.refresh_auto_drive_visuals();
        return;
    }

    let command_for_watch = command.clone();
    let wait_notes_pairs = wait_notes.clone();
    let status = if exit_code == 0 {
        ExecStatus::Success
    } else {
        ExecStatus::Error
    };
    let now = SystemTime::now();
    let wait_notes_record = exec_wait_notes_from_pairs(&wait_notes_pairs);
    let stdout_tail_event = stream_tail(&stdout, &streamed_stdout);
    let stderr_tail_event = stream_tail(&stderr, &streamed_stderr);

    let finish_mutation = chat
        .history_state
        .apply_domain_event(HistoryDomainEvent::FinishExec {
            id: history_id,
            call_id: Some(call_id.clone()),
            status,
            exit_code: Some(exit_code),
            completed_at: Some(now),
            wait_total,
            wait_active: false,
            wait_notes: wait_notes_record,
            stdout_tail: stdout_tail_event,
            stderr_tail: stderr_tail_event,
        });

    let mut handled_via_state = false;
    if let HistoryMutation::Replaced {
        id,
        record: HistoryRecord::Exec(exec_record),
        ..
    } = finish_mutation
    {
        chat.update_cell_from_record(id, HistoryRecord::Exec(exec_record.clone()));
        if let Some(idx) = chat.cell_index_for_history_id(id) {
            crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
        }
        handled_via_state = true;
    }

    if !handled_via_state {
        let mut completed_opt = Some(history_cell::new_completed_exec_command(
            command,
            parsed,
            CommandOutput {
                exit_code,
                stdout: stdout.clone(),
                stderr: stderr.clone(),
            },
        ));
        if let Some(cell) = completed_opt.as_mut() {
            cell.set_wait_total(wait_total);
            cell.set_wait_notes(&wait_notes_pairs);
            cell.set_waiting(false);
            cell.set_run_duration(Some(duration));
        }

        let mut replaced = false;
        if let Some(idx) = history_index {
            if idx < chat.history_cells.len() {
                let is_match = chat.history_cells[idx]
                    .as_any()
                    .downcast_ref::<history_cell::ExecCell>()
                    .map(|e| {
                        if let Some(ref c) = completed_opt {
                            e.output.is_none() && e.command == c.command
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if is_match {
                    if let Some(c) = completed_opt.take() {
                        chat.history_replace_and_maybe_merge(idx, Box::new(c));
                    }
                    replaced = true;
                }
            }
            if !replaced {
                let mut found: Option<usize> = None;
                for i in (0..chat.history_cells.len()).rev() {
                    if let Some(exec) = chat.history_cells[i]
                        .as_any()
                        .downcast_ref::<history_cell::ExecCell>()
                    {
                        let is_same = if let Some(ref c) = completed_opt {
                            exec.command == c.command
                        } else {
                            false
                        };
                        if exec.output.is_none() && is_same {
                            found = Some(i);
                            break;
                        }
                    }
                }
                if let Some(i) = found {
                    if let Some(c) = completed_opt.take() {
                        chat.history_replace_and_maybe_merge(i, Box::new(c));
                    }
                    replaced = true;
                }
            }
        }

        if !replaced {
            if let Some(c) = completed_opt.take() {
                let key = ChatWidget::order_key_from_order_meta(order);
                let idx = chat.history_insert_with_key_global(Box::new(c), key);
                crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
            }
        }
    }

    if exit_code == 0 {
        chat
            .bottom_pane
            .update_status_text("command completed".to_string());
        crate::chatwidget::gh_actions::maybe_watch_after_push(
            chat.app_event_tx.clone(),
            chat.config.clone(),
            &command_for_watch,
        );
    } else {
        chat
            .bottom_pane
            .update_status_text(format!("command failed (exit {})", exit_code));
    }
    chat.maybe_hide_spinner();
    chat.refresh_auto_drive_visuals();
}

// Stable ordering now inserts at the correct position; these helpers are removed.

// `handle_exec_approval_now` remains on ChatWidget in chatwidget.rs because
// it is referenced directly from interrupt handling and is trivial.
