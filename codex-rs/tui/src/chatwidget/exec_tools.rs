//! Exec and tool call lifecycle helpers for `ChatWidget`.

use super::{ChatWidget, spec_kit};
use crate::app_event::AppEvent;
use crate::height_manager::HeightEvent;
use crate::history_cell::CommandOutput;
use crate::history_cell::{self, HistoryCell};
use codex_core::parse_command::ParsedCommand;
use codex_core::protocol::{ExecCommandBeginEvent, ExecCommandEndEvent, OrderMeta};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;

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
                if let Some(agg) = chat.history_cells[*agg_idx]
                    .as_any_mut()
                    .downcast_mut::<history_cell::ExploreAggregationCell>()
                {
                    agg.update_status(
                        *entry_idx,
                        history_cell::ExploreEntryStatus::Error { exit_code: None },
                    );
                }
            }
        }
        chat.canceled_exec_call_ids.insert(call_id.clone());
    }
    let agg_was_updated = running.iter().any(|(_, _, entry)| entry.is_some());
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
                    let wait_cancel_cell = Box::new(history_cell::PlainHistoryCell::new(
                        vec![Line::styled(
                            "Wait cancelled",
                            Style::default()
                                .fg(crate::colors::error())
                                .add_modifier(Modifier::BOLD),
                        )],
                        history_cell::HistoryCellType::Error,
                    ));

                    let replaced = chat.history_cells[idx]
                        .as_any()
                        .downcast_ref::<history_cell::RunningToolCallCell>()
                        .map(|cell| cell.has_title("Waiting"))
                        .unwrap_or(false);

                    if replaced {
                        chat.history_replace_at(idx, wait_cancel_cell);
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
                if let Some(agg) = chat.history_cells[*agg_idx]
                    .as_any_mut()
                    .downcast_mut::<history_cell::ExploreAggregationCell>()
                {
                    agg.update_status(*entry_idx, history_cell::ExploreEntryStatus::Success);
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
}

pub(super) fn try_merge_completed_exec_at(chat: &mut ChatWidget<'_>, idx: usize) {
    if idx == 0 || idx >= chat.history_cells.len() {
        return;
    }
    let to_kind = |e: &history_cell::ExecCell| -> history_cell::ExecKind {
        match history_cell::action_enum_from_parsed(&e.parsed) {
            history_cell::ExecAction::Read => history_cell::ExecKind::Read,
            history_cell::ExecAction::Search => history_cell::ExecKind::Search,
            history_cell::ExecAction::List => history_cell::ExecKind::List,
            history_cell::ExecAction::Run => history_cell::ExecKind::Run,
        }
    };

    let new_exec = match chat.history_cells[idx]
        .as_any()
        .downcast_ref::<history_cell::ExecCell>()
    {
        Some(e) if e.output.is_some() => e,
        _ => return,
    };
    let new_kind = to_kind(new_exec);
    if matches!(new_kind, history_cell::ExecKind::Run) {
        return;
    }

    if let Some(prev_exec) = chat.history_cells[idx - 1]
        .as_any()
        .downcast_ref::<history_cell::ExecCell>()
    {
        if prev_exec.output.is_some() {
            if to_kind(prev_exec) == new_kind {
                let mut merged = history_cell::MergedExecCell::from_exec(prev_exec);
                if let Some(current_exec) = chat.history_cells[idx]
                    .as_any()
                    .downcast_ref::<history_cell::ExecCell>()
                {
                    merged.push_exec(current_exec);
                }
                chat.history_replace_at(idx - 1, Box::new(merged));
                chat.history_remove_at(idx);
                chat.invalidate_height_cache();
                chat.autoscroll_if_near_bottom();
                chat.bottom_pane.set_has_chat_history(true);
                chat.process_animation_cleanup();
                chat.app_event_tx.send(AppEvent::RequestRedraw);
                return;
            }
        }
    }

    let mut did_merge_into_prev = false;
    if idx < chat.history_cells.len() {
        let (left, right) = chat.history_cells.split_at_mut(idx);
        if let Some(prev_merged) = left[idx - 1]
            .as_any_mut()
            .downcast_mut::<history_cell::MergedExecCell>()
        {
            if prev_merged.exec_kind() == new_kind {
                if let Some(current_exec) =
                    right[0].as_any().downcast_ref::<history_cell::ExecCell>()
                {
                    prev_merged.push_exec(current_exec);
                    did_merge_into_prev = true;
                }
            }
        }
    }
    if did_merge_into_prev {
        chat.history_remove_at(idx);
        chat.invalidate_height_cache();
        chat.autoscroll_if_near_bottom();
        chat.bottom_pane.set_has_chat_history(true);
        chat.process_animation_cleanup();
        chat.app_event_tx.send(AppEvent::RequestRedraw);
    }
}

fn try_upgrade_fallback_exec_cell(chat: &mut ChatWidget<'_>, ev: &ExecCommandBeginEvent) -> bool {
    for i in (0..chat.history_cells.len()).rev() {
        if let Some(exec) = chat.history_cells[i]
            .as_any_mut()
            .downcast_mut::<history_cell::ExecCell>()
        {
            let looks_like_fallback = exec.output.is_some()
                && exec.parsed.is_empty()
                && exec.command.len() == 1
                && exec
                    .command
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
        history_cell::ExecAction::Read
            | history_cell::ExecAction::Search
            | history_cell::ExecAction::List
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
            let idx = chat.history_insert_with_key_global(
                Box::new(history_cell::ExploreAggregationCell::new()),
                key,
            );
            created_new = true;
            agg_idx = Some(idx);
        }

        if let Some(idx) = agg_idx {
            let entry_idx = chat.history_cells[idx]
                .as_any_mut()
                .downcast_mut::<history_cell::ExploreAggregationCell>()
                .and_then(|agg| {
                    agg.push_from_parsed(
                        &parsed_command,
                        history_cell::ExploreEntryStatus::Running,
                        &ev.cwd,
                        &chat.config.cwd,
                        &ev.command,
                    )
                });
            if let Some(entry_idx) = entry_idx {
                chat.exec.running_explore_agg_index = Some(idx);
                chat.exec.running_commands.insert(
                    super::ExecCallId(ev.call_id.clone()),
                    super::RunningCommand {
                        command: ev.command.clone(),
                        parsed: parsed_command.clone(),
                        history_index: None,
                        explore_entry: Some((idx, entry_idx)),
                        stdout: String::new(),
                        stderr: String::new(),
                        wait_total: None,
                        wait_active: false,
                        wait_notes: Vec::new(),
                    },
                );
                chat.invalidate_height_cache();
                chat.autoscroll_if_near_bottom();
                chat.request_redraw();
                chat.bottom_pane.set_has_chat_history(true);
                let status_text = match action {
                    history_cell::ExecAction::Read => "reading files…",
                    _ => "exploring…",
                };
                chat.bottom_pane.update_status_text(status_text.to_string());
                return;
            } else if created_new {
                chat.history_remove_at(idx);
                chat.invalidate_height_cache();
            }
        }
    }

    let cell = history_cell::new_active_exec_command(ev.command.clone(), parsed_command.clone());
    let key = ChatWidget::order_key_from_order_meta(order);
    let idx = chat.history_insert_with_key_global(Box::new(cell), key);
    chat.exec.running_commands.insert(
        super::ExecCallId(ev.call_id.clone()),
        super::RunningCommand {
            command: ev.command.clone(),
            parsed: parsed_command,
            history_index: Some(idx),
            explore_entry: None,
            stdout: String::new(),
            stderr: String::new(),
            wait_total: None,
            wait_active: false,
            wait_notes: Vec::new(),
        },
    );
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
        return;
    }
    chat.ended_call_ids
        .insert(super::ExecCallId(ev.call_id.clone()));
    // If this call was already marked as cancelled, drop the End to avoid
    // inserting a duplicate completed cell after the user interrupt.
    if chat
        .canceled_exec_call_ids
        .remove(&super::ExecCallId(ev.call_id.clone()))
    {
        chat.maybe_hide_spinner();
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
    let (command, parsed, history_index, explore_entry, wait_total, wait_notes) = match cmd {
        Some(super::RunningCommand {
            command,
            parsed,
            history_index,
            explore_entry,
            wait_total,
            wait_notes,
            ..
        }) => (
            command,
            parsed,
            history_index,
            explore_entry,
            wait_total,
            wait_notes,
        ),
        None => (vec![call_id.clone()], vec![], None, None, None, Vec::new()),
    };

    if let Some((agg_idx, entry_idx)) = explore_entry {
        let action = history_cell::action_enum_from_parsed(&parsed);
        let status = match (exit_code, action) {
            (0, _) => history_cell::ExploreEntryStatus::Success,
            // No matches for searches
            (1, history_cell::ExecAction::Search) => history_cell::ExploreEntryStatus::NotFound,
            // Missing file/dir for list operations (e.g., ls path)
            (1, history_cell::ExecAction::List) => history_cell::ExploreEntryStatus::NotFound,
            // Anything else is an error; preserve exit code
            _ => history_cell::ExploreEntryStatus::Error {
                exit_code: Some(exit_code),
            },
        };
        if agg_idx < chat.history_cells.len() {
            if let Some(agg) = chat.history_cells[agg_idx]
                .as_any_mut()
                .downcast_mut::<history_cell::ExploreAggregationCell>()
            {
                agg.update_status(entry_idx, status.clone());
            }
        }
        if !chat
            .exec
            .running_commands
            .values()
            .any(|rc| rc.explore_entry.is_some())
        {
            chat.exec.running_explore_agg_index = None;
        }
        chat.invalidate_height_cache();
        chat.request_redraw();
        let status_text = match status {
            history_cell::ExploreEntryStatus::Success => match action {
                history_cell::ExecAction::Read => "files read".to_string(),
                _ => "exploration updated".to_string(),
            },
            history_cell::ExploreEntryStatus::NotFound => match action {
                history_cell::ExecAction::List => "path not found".to_string(),
                _ => "no matches found".to_string(),
            },
            history_cell::ExploreEntryStatus::Error { .. } => match action {
                history_cell::ExecAction::Read => format!("read failed (exit {exit_code})"),
                history_cell::ExecAction::Search => {
                    if exit_code == 2 {
                        "invalid pattern".to_string()
                    } else {
                        format!("search failed (exit {exit_code})")
                    }
                }
                history_cell::ExecAction::List => format!("list failed (exit {exit_code})"),
                _ => format!("exploration failed (exit {exit_code})"),
            },
            history_cell::ExploreEntryStatus::Running => "exploring…".to_string(),
        };
        chat.bottom_pane.update_status_text(status_text);
        chat.maybe_hide_spinner();
        return;
    }

    let command_for_watch = command.clone();
    let mut completed_opt = Some(history_cell::new_completed_exec_command(
        command,
        parsed,
        CommandOutput {
            exit_code,
            stdout,
            stderr,
        },
    ));
    if let Some(ref cell) = completed_opt {
        cell.set_wait_total(wait_total);
        cell.set_wait_notes(&wait_notes);
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
            // Attempt standard merge with previous Exec if applicable.
            crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
        }
    }

    if exit_code == 0 {
        chat.bottom_pane
            .update_status_text("command completed".to_string());
        // If this was a successful `git push`, start background GH Actions watch if enabled.
        crate::chatwidget::gh_actions::maybe_watch_after_push(
            chat.app_event_tx.clone(),
            chat.config.clone(),
            &command_for_watch,
        );
    } else {
        chat.bottom_pane
            .update_status_text(format!("command failed (exit {})", exit_code));
    }

    // === FORK-SPECIFIC: spec-auto guardrail completion handler ===
    // Check if this exec completion is for a spec-auto guardrail command
    if let Some(ref state) = chat.spec_auto_state {
        if let Some(ref wait) = state.waiting_guardrail {
            // Check if this is the guardrail we're waiting for
            // Project commands have call_id pattern: "project_cmd_spec_ops_*"
            if call_id.starts_with("project_cmd_spec_ops") {
                if exit_code == 0 {
                    // Guardrail succeeded - transition to agent execution phase
                    let stage = wait.stage;
                    let spec_id = state.spec_id.clone();

                    // Clear waiting state
                    if let Some(state_mut) = chat.spec_auto_state.as_mut() {
                        state_mut.waiting_guardrail = None;
                    }

                    // Spawn agents for this stage
                    spec_kit::auto_submit_spec_stage_prompt(chat, stage, &spec_id);
                } else {
                    // Guardrail failed - halt pipeline
                    spec_kit::halt_spec_auto_with_error(
                        chat,
                        format!(
                            "Guardrail {} failed with exit code {}",
                            wait.stage.display_name(),
                            exit_code
                        ),
                    );
                }
            }
        }
    }
    // === END FORK-SPECIFIC ===

    chat.maybe_hide_spinner();
}

// Stable ordering now inserts at the correct position; these helpers are removed.

// `handle_exec_approval_now` remains on ChatWidget in chatwidget.rs because
// it is referenced directly from interrupt handling and is trivial.
