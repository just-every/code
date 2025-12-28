//! Exec and tool call lifecycle helpers for `ChatWidget`.

use super::{running_tools, web_search_sessions, ChatWidget};
use crate::app_event::AppEvent;
use crate::height_manager::HeightEvent;
use crate::history::state::{
    ExecAction,
    ExecRecord,
    ExecStatus,
    ExecStreamChunk,
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
use code_core::parse_command::ParsedCommand;
use code_core::protocol::{ExecCommandBeginEvent, ExecCommandEndEvent, OrderMeta};
use std::path::PathBuf;
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

fn stream_chunks_to_text(chunks: &[ExecStreamChunk]) -> String {
    if chunks.is_empty() {
        return String::new();
    }
    let mut ordered: Vec<&ExecStreamChunk> = chunks.iter().collect();
    ordered.sort_by_key(|chunk| chunk.offset);
    let mut combined = String::new();
    for chunk in ordered {
        combined.push_str(&chunk.content);
    }
    combined
}

fn stream_chunks_tail_to_text(chunks: &[ExecStreamChunk], max_bytes: usize) -> String {
    if chunks.is_empty() || max_bytes == 0 {
        return String::new();
    }

    let mut ordered: Vec<&ExecStreamChunk> = chunks.iter().collect();
    ordered.sort_by_key(|chunk| chunk.offset);

    let mut pieces: Vec<String> = Vec::new();
    let mut total_bytes: usize = 0;
    for chunk in ordered.into_iter().rev() {
        if total_bytes >= max_bytes {
            break;
        }
        let remaining = max_bytes - total_bytes;
        let content = chunk.content.as_str();
        if content.len() <= remaining {
            pieces.push(content.to_string());
            total_bytes = total_bytes.saturating_add(content.len());
            continue;
        }

        let mut start = content.len().saturating_sub(remaining);
        while start < content.len() && !content.is_char_boundary(start) {
            start = start.saturating_add(1);
        }
        pieces.push(content[start..].to_string());
        break;
    }

    pieces.reverse();
    pieces.concat()
}

fn explore_status_from_exec(action: ExecAction, record: &ExecRecord) -> history_cell::ExploreEntryStatus {
    match record.status {
        ExecStatus::Running => history_cell::ExploreEntryStatus::Running,
        ExecStatus::Success => history_cell::ExploreEntryStatus::Success,
        ExecStatus::Error => match (record.exit_code, action) {
            (Some(1), ExecAction::Search | ExecAction::List) => history_cell::ExploreEntryStatus::NotFound,
            _ => history_cell::ExploreEntryStatus::Error {
                exit_code: record.exit_code,
            },
        },
    }
}

fn promote_exec_cell_to_explore(chat: &mut ChatWidget<'_>, idx: usize) -> bool {
    if idx >= chat.history_cells.len() {
        return false;
    }

    let (segments, action) = match history_record_for_cell(chat, idx) {
        Some(HistoryRecord::Exec(exec_record)) => {
            let action = exec_record.action;
            (vec![exec_record], action)
        }
        Some(HistoryRecord::MergedExec(merged)) => {
            let action = merged.action;
            (merged.segments, action)
        }
        _ => return false,
    };

    if !matches!(
        action,
        ExecAction::Read | ExecAction::Search | ExecAction::List
    ) {
        return false;
    }

    let session_root = chat.config.cwd.clone();

    let mut target_idx = chat.exec.running_explore_agg_index.and_then(|candidate| {
        if candidate < chat.history_cells.len()
            && chat.history_cells[candidate]
                .as_any()
                .downcast_ref::<history_cell::ExploreAggregationCell>()
                .is_some()
        {
            Some(candidate)
        } else {
            None
        }
    });

    if target_idx.is_none() {
        target_idx = find_trailing_explore_agg(chat);
    }

    let push_segments = |record: &mut ExploreRecord| -> bool {
        let mut added_any = false;
        for segment in &segments {
            if segment.parsed.is_empty() {
                continue;
            }
            let status = explore_status_from_exec(segment.action, segment);
            let cwd_buf: PathBuf = segment
                .working_dir
                .clone()
                .unwrap_or_else(|| chat.config.cwd.clone());
            if history_cell::explore_record_push_from_parsed(
                record,
                &segment.parsed,
                status,
                cwd_buf.as_path(),
                session_root.as_path(),
                &segment.command,
            )
            .is_some()
            {
                added_any = true;
            }
        }
        added_any
    };

    if let Some(agg_idx) = target_idx {
        let Some(mut record) = chat.history_cells.get(agg_idx).and_then(|cell| {
            cell.as_any()
                .downcast_ref::<history_cell::ExploreAggregationCell>()
                .map(|existing| existing.record().clone())
        }) else {
            return false;
        };

        if !push_segments(&mut record) {
            return false;
        }

        let replacement = history_cell::ExploreAggregationCell::from_record(record.clone());
        chat.history_replace_with_record(
            agg_idx,
            Box::new(replacement),
            HistoryDomainRecord::Explore(record),
        );

        if agg_idx != idx {
            chat.history_remove_at(idx);
        }

        chat.bottom_pane.set_has_chat_history(true);
        chat.autoscroll_if_near_bottom();
        chat.exec.running_explore_agg_index = None;
        return true;
    }

    let mut record = ExploreRecord {
        id: HistoryId::ZERO,
        entries: Vec::new(),
    };

    if !push_segments(&mut record) {
        return false;
    }

    let cell = history_cell::ExploreAggregationCell::from_record(record.clone());
    chat.history_replace_with_record(
        idx,
        Box::new(cell),
        HistoryDomainRecord::Explore(record),
    );
    chat.bottom_pane.set_has_chat_history(true);
    chat.autoscroll_if_near_bottom();
    chat.exec.running_explore_agg_index = None;
    true
}

fn update_explore_entry_status(
    chat: &mut ChatWidget<'_>,
    preferred_index: Option<usize>,
    entry_idx: usize,
    status: history_cell::ExploreEntryStatus,
) -> Option<usize> {
    fn try_update_at(
        chat: &mut ChatWidget<'_>,
        idx: usize,
        entry_idx: usize,
        status: &history_cell::ExploreEntryStatus,
    ) -> Option<usize> {
        if idx >= chat.history_cells.len() {
            return None;
        }
        let cell = chat.history_cells[idx]
            .as_any()
            .downcast_ref::<history_cell::ExploreAggregationCell>()?;
        if entry_idx >= cell.record().entries.len() {
            return None;
        }
        let mut record = cell.record().clone();
        history_cell::explore_record_update_status(&mut record, entry_idx, status.clone());
        let replacement = history_cell::ExploreAggregationCell::from_record(record.clone());
        chat.history_replace_with_record(
            idx,
            Box::new(replacement),
            HistoryDomainRecord::Explore(record),
        );
        chat.autoscroll_if_near_bottom();
        Some(idx)
    }

    if let Some(idx) = preferred_index.and_then(|i| try_update_at(chat, i, entry_idx, &status)) {
        return Some(idx);
    }
    if let Some(idx) = chat
        .exec
        .running_explore_agg_index
        .and_then(|i| try_update_at(chat, i, entry_idx, &status))
    {
        return Some(idx);
    }
    for i in (0..chat.history_cells.len()).rev() {
        let Some(cell) = chat.history_cells[i]
            .as_any()
            .downcast_ref::<history_cell::ExploreAggregationCell>()
        else {
            continue;
        };
        if entry_idx >= cell.record().entries.len() {
            continue;
        }
        if !matches!(
            cell.record().entries[entry_idx].status,
            history_cell::ExploreEntryStatus::Running
                | history_cell::ExploreEntryStatus::Error { .. }
                | history_cell::ExploreEntryStatus::NotFound
        ) {
            continue;
        }
        if let Some(idx) = try_update_at(chat, i, entry_idx, &status) {
            return Some(idx);
        }
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
                    history_cell::explore_record_update_status(
                        &mut record,
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
        for (tool_id, entry) in entries {
            if let Some(idx) = running_tools::resolve_entry_index(chat, &entry, &tool_id.0) {
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

    web_search_sessions::finalize_all_failed(chat, "Search cancelled by user.");

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
    const STALE_MSG: &str = "Running in background after turn end.";
    const STREAM_TAIL_MAX_BYTES: usize = 64 * 1024;

    // Drain running execs so we can mark them stale and stop the spinner.
    let mut agg_was_updated = false;
    let running_keys: Vec<super::ExecCallId> = chat
        .exec
        .running_commands
        .keys()
        .cloned()
        .collect();

    for call_id in running_keys {
        if let Some(running) = chat.exec.running_commands.remove(&call_id) {
            // Update any explore aggregation entry tied to this exec.
            if let Some((agg_idx, entry_idx)) = running.explore_entry {
                let updated = update_explore_entry_status(
                    chat,
                    Some(agg_idx),
                    entry_idx,
                    history_cell::ExploreEntryStatus::Success,
                )
                .is_some();
                agg_was_updated |= updated;
            }

            let exit_code = 0;
            let now = SystemTime::now();
            let wait_notes_pairs = running.wait_notes.clone();
            let wait_notes_record = exec_wait_notes_from_pairs(&wait_notes_pairs);

            let history_id = running
                .history_id
                .or_else(|| chat.history_state.history_id_for_exec_call(call_id.as_ref()))
                .or_else(|| {
                    running
                        .history_index
                        .and_then(|idx| chat.history_cell_ids.get(idx).and_then(|h| *h))
                });

            let stderr_prefix = if running.stderr_offset > 0 { "\n" } else { "" };
            let stderr_tail_event = Some(format!("{stderr_prefix}{STALE_MSG}"));
            let stdout_tail_event = None;

            let finish_mutation = chat.history_state.apply_domain_event(
                HistoryDomainEvent::FinishExec {
                    id: history_id,
                    call_id: Some(call_id.as_ref().to_string()),
                    status: ExecStatus::Success,
                    exit_code: Some(exit_code),
                    completed_at: Some(now),
                    wait_total: running.wait_total,
                    wait_active: false,
                    wait_notes: wait_notes_record,
                    stdout_tail: stdout_tail_event,
                    stderr_tail: stderr_tail_event,
                },
            );

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
                let mut stdout_so_far = String::new();
                let mut stderr_so_far = String::new();
                if let Some(history_id) = history_id {
                    if let Some(record) = chat.history_state.record(history_id) {
                        match record {
                            HistoryRecord::Exec(exec_record) => {
                                stdout_so_far = stream_chunks_tail_to_text(
                                    &exec_record.stdout_chunks,
                                    STREAM_TAIL_MAX_BYTES,
                                );
                                stderr_so_far = stream_chunks_tail_to_text(
                                    &exec_record.stderr_chunks,
                                    STREAM_TAIL_MAX_BYTES,
                                );
                            }
                            HistoryRecord::MergedExec(merged) => {
                                if let Some(last) = merged.segments.last() {
                                    stdout_so_far = stream_chunks_tail_to_text(
                                        &last.stdout_chunks,
                                        STREAM_TAIL_MAX_BYTES,
                                    );
                                    stderr_so_far = stream_chunks_tail_to_text(
                                        &last.stderr_chunks,
                                        STREAM_TAIL_MAX_BYTES,
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }

                if !stderr_so_far.is_empty() {
                    stderr_so_far.push('\n');
                }
                stderr_so_far.push_str(STALE_MSG);

                let mut completed_cell = history_cell::new_completed_exec_command(
                    running.command.clone(),
                    running.parsed.clone(),
                    CommandOutput {
                        exit_code,
                        stdout: stdout_so_far,
                        stderr: stderr_so_far,
                    },
                );
                // Preserve linkage to the original call id when possible.
                completed_cell.record.call_id = Some(call_id.as_ref().to_string());

                if let Some(idx) = running.history_index {
                    chat.history_replace_and_maybe_merge(idx, Box::new(completed_cell));
                } else {
                    let key = chat.next_internal_key();
                    let idx = chat.history_insert_with_key_global(Box::new(completed_cell), key);
                    crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
                }
            }
        }
    }

    if agg_was_updated {
        chat.exec.running_explore_agg_index = None;
        chat.invalidate_height_cache();
        chat.request_redraw();
    }

    crate::chatwidget::running_tools::finalize_all_due_to_answer(chat);

    web_search_sessions::finalize_all_completed(chat, "Search finished");

    chat.maybe_hide_spinner();
    chat.refresh_auto_drive_visuals();
}

pub(super) fn finalize_wait_missing_exec(
    chat: &mut ChatWidget<'_>,
    call_id: super::ExecCallId,
    message: &str,
) -> bool {
    const STREAM_TAIL_MAX_BYTES: usize = 64 * 1024;
    let trimmed = message.trim();
    let fallback_message = if trimmed.is_empty() {
        "Background job finished but output was unavailable."
    } else {
        trimmed
    };

    let history_id: Option<HistoryId>;
    let mut history_index: Option<usize> = None;
    let mut command: Vec<String> = Vec::new();
    let mut parsed: Vec<ParsedCommand> = Vec::new();
    let mut wait_total: Option<std::time::Duration> = None;
    let mut wait_notes_pairs: Vec<(String, bool)> = Vec::new();
    let mut explore_entry: Option<(usize, usize)> = None;

    if let Some(running) = chat.exec.running_commands.remove(&call_id) {
        history_id = running
            .history_id
            .or_else(|| chat.history_state.history_id_for_exec_call(call_id.as_ref()))
            .or_else(|| {
                running
                    .history_index
                    .and_then(|idx| chat.history_cell_ids.get(idx).and_then(|slot| *slot))
            });
        history_index = running.history_index;
        command = running.command;
        parsed = running.parsed;
        wait_total = running.wait_total;
        wait_notes_pairs = running.wait_notes;
        explore_entry = running.explore_entry;
    } else {
        history_id = chat.history_state.history_id_for_exec_call(call_id.as_ref());
        if let Some(id) = history_id {
            if let Some(HistoryRecord::Exec(record)) = chat.history_state.record(id).cloned() {
                command = record.command.clone();
                parsed = record.parsed.clone();
                wait_total = record.wait_total;
                wait_notes_pairs = ChatWidget::wait_pairs_from_exec_notes(&record.wait_notes);
            }
        } else {
            return false;
        }
    }

    if let Some(id) = history_id {
        if let Some(record) = chat.history_state.record(id) {
            match record {
                HistoryRecord::Exec(exec_record) => {
                    if exec_record.status != ExecStatus::Running {
                        chat.maybe_hide_spinner();
                        chat.refresh_auto_drive_visuals();
                        return true;
                    }
                }
                HistoryRecord::MergedExec(_) => {
                    chat.maybe_hide_spinner();
                    chat.refresh_auto_drive_visuals();
                    return true;
                }
                _ => {}
            }
        }
    }

    if let Some((agg_idx, entry_idx)) = explore_entry {
        let status = history_cell::ExploreEntryStatus::Error { exit_code: None };
        let updated =
            update_explore_entry_status(chat, Some(agg_idx), entry_idx, status).is_some();
        if updated && !chat
            .exec
            .running_commands
            .values()
            .any(|rc| rc.explore_entry.is_some())
        {
            chat.exec.running_explore_agg_index = None;
        }
    }

    let wait_notes_record = exec_wait_notes_from_pairs(&wait_notes_pairs);
    let finish_mutation = chat
        .history_state
        .apply_domain_event(HistoryDomainEvent::FinishExec {
            id: history_id,
            call_id: Some(call_id.as_ref().to_string()),
            status: ExecStatus::Error,
            exit_code: Some(-1),
            completed_at: Some(SystemTime::now()),
            wait_total,
            wait_active: false,
            wait_notes: wait_notes_record,
            stdout_tail: None,
            stderr_tail: Some(fallback_message.to_string()),
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
        let mut stdout_so_far = String::new();
        let mut stderr_so_far = String::new();
        if let Some(history_id) = history_id {
            if let Some(record) = chat.history_state.record(history_id) {
                match record {
                    HistoryRecord::Exec(exec_record) => {
                        stdout_so_far = stream_chunks_tail_to_text(
                            &exec_record.stdout_chunks,
                            STREAM_TAIL_MAX_BYTES,
                        );
                        stderr_so_far = stream_chunks_tail_to_text(
                            &exec_record.stderr_chunks,
                            STREAM_TAIL_MAX_BYTES,
                        );
                    }
                    HistoryRecord::MergedExec(merged) => {
                        if let Some(last) = merged.segments.last() {
                            stdout_so_far = stream_chunks_tail_to_text(
                                &last.stdout_chunks,
                                STREAM_TAIL_MAX_BYTES,
                            );
                            stderr_so_far = stream_chunks_tail_to_text(
                                &last.stderr_chunks,
                                STREAM_TAIL_MAX_BYTES,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        if !stderr_so_far.is_empty() {
            stderr_so_far.push('\n');
        }
        stderr_so_far.push_str(fallback_message);

        let mut completed_cell = history_cell::new_completed_exec_command(
            command,
            parsed,
            CommandOutput {
                exit_code: -1,
                stdout: stdout_so_far,
                stderr: stderr_so_far,
            },
        );
        completed_cell.record.call_id = Some(call_id.as_ref().to_string());

        if let Some(idx) = history_index {
            chat.history_replace_and_maybe_merge(idx, Box::new(completed_cell));
        } else {
            let key = chat.next_internal_key();
            let idx = chat.history_insert_with_key_global(Box::new(completed_cell), key);
            crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
        }
    }

    chat.maybe_hide_spinner();
    chat.refresh_auto_drive_visuals();
    true
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
            .as_any()
            .downcast_ref::<history_cell::ExecCell>()
        {
            let command_matches_call = exec.command.len() == 1
                && exec.command
                    .first()
                    .map(|cmd| cmd == &ev.call_id)
                    .unwrap_or(false);
            let record_matches_call = exec
                .record
                .call_id
                .as_deref()
                .map(|cid| cid == ev.call_id)
                .unwrap_or(false);
            let looks_like_fallback = exec.output.is_some()
                && exec.parsed.is_empty()
                && (command_matches_call || record_matches_call);
            if looks_like_fallback {
                let mut upgraded = false;
                if let Some(HistoryRecord::Exec(mut exec_record)) =
                    history_record_for_cell(chat, i)
                {
                    exec_record.command = ev.command.clone();
                    exec_record.parsed = ev.parsed_cmd.clone();
                    exec_record.action = history_cell::action_enum_from_parsed(&exec_record.parsed);
                    exec_record.call_id = Some(ev.call_id.clone());
                    if exec_record.working_dir.is_none() {
                        exec_record.working_dir = Some(ev.cwd.clone());
                    }

                    let record_index = chat
                        .record_index_for_cell(i)
                        .unwrap_or_else(|| chat.record_index_for_position(i));
                    let mutation = chat.history_state.apply_domain_event(
                        HistoryDomainEvent::Replace {
                            index: record_index,
                            record: HistoryDomainRecord::Exec(exec_record.clone()),
                        },
                    );

                    if let HistoryMutation::Replaced {
                        id,
                        record: HistoryRecord::Exec(updated_record),
                        ..
                    } = mutation
                    {
                        chat.update_cell_from_record(
                            id,
                            HistoryRecord::Exec(updated_record.clone()),
                        );
                        if let Some(idx) = chat.cell_index_for_history_id(id) {
                            if promote_exec_cell_to_explore(chat, idx) {
                                return true;
                            }
                            crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
                        }
                        upgraded = true;
                    }
                }

                if !upgraded {
                    if let Some(exec_mut) = chat.history_cells[i]
                        .as_any_mut()
                        .downcast_mut::<history_cell::ExecCell>()
                    {
                        exec_mut.replace_command_metadata(ev.command.clone(), ev.parsed_cmd.clone());
                    }
                    if promote_exec_cell_to_explore(chat, i) {
                        return true;
                    }
                    try_merge_completed_exec_at(chat, i);
                }

                chat.invalidate_height_cache();
                chat.request_redraw();
                return true;
            }
        }
    }
    false
}

fn hydrate_exec_record_from_begin(
    record: &mut ExecRecord,
    ev: &ExecCommandBeginEvent,
) -> bool {
    let mut changed = false;
    if record.command != ev.command {
        record.command = ev.command.clone();
        changed = true;
    }
    if record.parsed != ev.parsed_cmd {
        record.parsed = ev.parsed_cmd.clone();
        changed = true;
    }
    let new_action = history_cell::action_enum_from_parsed(&record.parsed);
    if record.action != new_action {
        record.action = new_action;
        changed = true;
    }
    if record.call_id.as_deref() != Some(ev.call_id.as_str()) {
        record.call_id = Some(ev.call_id.clone());
        changed = true;
    }
    if record.working_dir.is_none() {
        record.working_dir = Some(ev.cwd.clone());
        changed = true;
    }
    changed
}

fn apply_exec_begin_metadata_to_finished_call(
    chat: &mut ChatWidget<'_>,
    ev: &ExecCommandBeginEvent,
) -> bool {
    let history_id = match chat
        .history_state
        .history_id_for_exec_call(&ev.call_id)
    {
        Some(id) => id,
        None => return false,
    };
    let index = match chat.history_state.index_of(history_id) {
        Some(idx) => idx,
        None => return false,
    };
    let record = match chat.history_state.record(history_id).cloned() {
        Some(record) => record,
        None => return false,
    };
    match record {
        HistoryRecord::Exec(mut exec_record) => {
            if !hydrate_exec_record_from_begin(&mut exec_record, ev) {
                return false;
            }

            let mutation = chat
                .history_state
                .apply_domain_event(HistoryDomainEvent::Replace {
                    index,
                    record: HistoryDomainRecord::Exec(exec_record.clone()),
                });

            if let HistoryMutation::Replaced {
                id,
                record: HistoryRecord::Exec(updated_record),
                ..
            } = mutation
            {
                chat.update_cell_from_record(id, HistoryRecord::Exec(updated_record.clone()));
                if let Some(idx) = chat.cell_index_for_history_id(id) {
                    if promote_exec_cell_to_explore(chat, idx) {
                        return true;
                    }
                    crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
                }
                chat.invalidate_height_cache();
                chat.request_redraw();
                return true;
            }

            if let Some(idx) = chat.cell_index_for_history_id(history_id) {
                let cell = history_cell::ExecCell::from_record(exec_record.clone());
                chat.history_replace_with_record(
                    idx,
                    Box::new(cell),
                    HistoryDomainRecord::Exec(exec_record),
                );
                if promote_exec_cell_to_explore(chat, idx) {
                    return true;
                }
                return true;
            }

            false
        }
        HistoryRecord::MergedExec(mut merged_record) => {
            let mut segment_found = false;
            for segment in merged_record.segments.iter_mut() {
                let matches_call = segment
                    .call_id
                    .as_deref()
                    .map(|cid| cid == ev.call_id)
                    .unwrap_or(false);
                let fallback_matches = segment.call_id.is_none()
                    && segment.command.len() == 1
                    && segment
                        .command
                        .first()
                        .map(|cmd| cmd.contains(&ev.call_id))
                        .unwrap_or(false);
                if matches_call || fallback_matches {
                    if hydrate_exec_record_from_begin(segment, ev) {
                        segment_found = true;
                    }
                    break;
                }
            }

            if !segment_found {
                return false;
            }

            let new_action = history_cell::action_enum_from_parsed(&ev.parsed_cmd);
            if merged_record.action != new_action {
                merged_record.action = new_action;
            }

            let mutation = chat
                .history_state
                .apply_domain_event(HistoryDomainEvent::Replace {
                    index,
                    record: HistoryDomainRecord::MergedExec(merged_record.clone()),
                });

            if let HistoryMutation::Replaced {
                id,
                record: HistoryRecord::MergedExec(updated_record),
                ..
            } = mutation
            {
                chat.update_cell_from_record(id, HistoryRecord::MergedExec(updated_record));
                if let Some(idx) = chat.cell_index_for_history_id(id) {
                    if promote_exec_cell_to_explore(chat, idx) {
                        return true;
                    }
                }
                chat.invalidate_height_cache();
                chat.request_redraw();
                return true;
            }

            if let Some(idx) = chat.cell_index_for_history_id(history_id) {
                let cell = history_cell::MergedExecCell::from_state(merged_record.clone());
                chat.history_replace_with_record(
                    idx,
                    Box::new(cell),
                    HistoryDomainRecord::MergedExec(merged_record),
                );
                if promote_exec_cell_to_explore(chat, idx) {
                    return true;
                }
                return true;
            }

            false
        }
        _ => false,
    }
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
        if apply_exec_begin_metadata_to_finished_call(chat, &ev) {
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
    let mut upgraded_tool_idx = if let Some(entry) = chat
        .tools_state
        .running_custom_tools
        .remove(&super::ToolCallId(ev.call_id.clone()))
    {
        running_tools::resolve_entry_index(chat, &entry, &ev.call_id)
            .or_else(|| running_tools::find_by_call_id(chat, &ev.call_id))
    } else {
        running_tools::find_by_call_id(chat, &ev.call_id)
    };

    if matches!(
        action,
        ExecAction::Read | ExecAction::Search | ExecAction::List
    ) || has_read_command
    {
        if let Some(idx) = upgraded_tool_idx.take() {
            chat.history_remove_at(idx);
        }
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
            let key = chat.provider_order_key_from_order_meta(order);
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
                if let Some(entry_idx) = history_cell::explore_record_push_from_parsed(
                    &mut record,
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
                            stdout_offset: 0,
                            stderr_offset: 0,
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
    let key = chat.provider_order_key_from_order_meta(order);
    let history_idx = if let Some(idx) = upgraded_tool_idx {
        let replacement = history_cell::ExecCell::from_record(exec_record.clone());
        chat.history_replace_with_record(
            idx,
            Box::new(replacement),
            HistoryDomainRecord::Exec(exec_record.clone()),
        );
        if idx < chat.cell_order_seq.len() {
            chat.cell_order_seq[idx] = key;
        }
        if idx < chat.cell_order_dbg.len() {
            chat.cell_order_dbg[idx] = None;
        }
        chat.last_assigned_order = Some(
            chat
                .last_assigned_order
                .map(|prev| prev.max(key))
                .unwrap_or(key),
        );
        chat.bottom_pane.set_has_chat_history(true);
        idx
    } else {
        let cell = history_cell::ExecCell::from_record(exec_record.clone());
        chat.history_insert_with_key_global_tagged(
            Box::new(cell),
            key,
            "exec-begin",
            Some(HistoryDomainRecord::Exec(exec_record.clone())),
        )
    };
    chat.exec.running_commands.insert(
        super::ExecCallId(ev.call_id.clone()),
        super::RunningCommand {
            command: ev.command.clone(),
            parsed: parsed_command,
            history_index: Some(history_idx),
            history_id: None,
            explore_entry: None,
            stdout_offset: 0,
            stderr_offset: 0,
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
            .or_else(|| chat.history_cell_ids.get(history_idx).and_then(|slot| *slot));
        running.history_id = history_id;
    }
    if !chat.tools_state.web_search_sessions.is_empty() {
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
    let suppressing = chat.exec.should_suppress_exec_end(&call_id);
    if suppressing {
        chat.exec.unsuppress_exec_end(&call_id);
        if !chat.exec.running_commands.contains_key(&call_id) {
            chat.ended_call_ids.insert(super::ExecCallId(ev.call_id.clone()));
            chat.maybe_hide_spinner();
            chat.refresh_auto_drive_visuals();
            return;
        }
    }
    chat
        .ended_call_ids
        .insert(super::ExecCallId(ev.call_id.clone()));
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
            ..
        }) => {
            let mut streamed_stdout = String::new();
            let mut streamed_stderr = String::new();
            if let Some(id) = history_id {
                if let Some(HistoryRecord::Exec(record)) = chat.history_state.record(id).cloned() {
                    streamed_stdout = stream_chunks_to_text(&record.stdout_chunks);
                    streamed_stderr = stream_chunks_to_text(&record.stderr_chunks);
                }
            } else if let Some(idx) = history_index {
                if let Some(exec_cell) = chat
                    .history_cells
                    .get(idx)
                    .and_then(|cell| cell.as_any().downcast_ref::<history_cell::ExecCell>())
                {
                    streamed_stdout = stream_chunks_to_text(&exec_cell.record.stdout_chunks);
                    streamed_stderr = stream_chunks_to_text(&exec_cell.record.stderr_chunks);
                }
            }

            (
                command,
                parsed,
                history_id,
                history_index,
                explore_entry,
                wait_total,
                wait_notes,
                streamed_stdout,
                streamed_stderr,
            )
        }
        None => {
            let mut history_id = chat
                .history_state
                .history_id_for_exec_call(call_id.as_ref());
            let mut history_index = history_id.and_then(|id| chat.cell_index_for_history_id(id));
            let mut command = vec![format!("Command running ({call_id})")];
            let mut parsed: Vec<ParsedCommand> = Vec::new();
            let mut wait_total: Option<std::time::Duration> = None;
            let mut wait_notes_pairs: Vec<(String, bool)> = Vec::new();
            let mut streamed_stdout = String::new();
            let mut streamed_stderr = String::new();

            if let Some(id) = history_id {
                if let Some(HistoryRecord::Exec(record)) = chat.history_state.record(id).cloned() {
                    command = record.command.clone();
                    parsed = record.parsed.clone();
                    wait_total = record.wait_total;
                    wait_notes_pairs = ChatWidget::wait_pairs_from_exec_notes(&record.wait_notes);
                    streamed_stdout = stream_chunks_to_text(&record.stdout_chunks);
                    streamed_stderr = stream_chunks_to_text(&record.stderr_chunks);
                    if history_index.is_none() {
                        history_index = chat.cell_index_for_history_id(id);
                    }
                }
            }

            if history_index.is_none() {
                if let Some((idx, exec_cell)) = chat
                    .history_cells
                    .iter()
                    .enumerate()
                    .rev()
                    .find_map(|(idx, cell)| {
                        cell.as_any()
                            .downcast_ref::<history_cell::ExecCell>()
                            .and_then(|exec_cell| {
                                if exec_cell.record.call_id.as_deref() == Some(call_id.as_ref()) {
                                    Some((idx, exec_cell))
                                } else {
                                    None
                                }
                            })
                    })
                {
                    history_index = Some(idx);
                    history_id = chat.history_cell_ids.get(idx).and_then(|slot| *slot);
                    command = exec_cell.command.clone();
                    parsed = exec_cell.parsed.clone();
                    wait_total = exec_cell.record.wait_total;
                    wait_notes_pairs = ChatWidget::wait_pairs_from_exec_notes(&exec_cell.record.wait_notes);
                    streamed_stdout = stream_chunks_to_text(&exec_cell.record.stdout_chunks);
                    streamed_stderr = stream_chunks_to_text(&exec_cell.record.stderr_chunks);
                }
            }

            (
                command,
                parsed,
                history_id,
                history_index,
                None,
                wait_total,
                wait_notes_pairs,
                streamed_stdout,
                streamed_stderr,
            )
        }
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
        let updated_index = update_explore_entry_status(chat, Some(agg_idx), entry_idx, status.clone());
        if !chat
            .exec
            .running_commands
            .values()
            .any(|rc| rc.explore_entry.is_some())
        {
            chat.exec.running_explore_agg_index = None;
        } else if let Some(actual_idx) = updated_index {
            chat.exec.running_explore_agg_index = Some(actual_idx);
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
            if cell.record.call_id.as_deref().is_none() {
                cell.record.call_id = Some(call_id.clone());
            }
        }

        let mut replaced = false;
        if let Some(idx) = history_index {
            if idx < chat.history_cells.len() {
                let is_match = chat.history_cells[idx]
                    .as_any()
                    .downcast_ref::<history_cell::ExecCell>()
                    .map(|e| {
                        if let Some(ref c) = completed_opt {
                            // Match by command OR call_id to reliably update the correct cell
                            let command_matches = e.command == c.command;
                            let call_id_matches = e.record.call_id.as_deref() == Some(call_id.as_ref());
                            (command_matches || call_id_matches) && e.output.is_none()
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
                            // Match by command OR call_id to avoid leaving exec cells stuck running
                            let command_matches = exec.command == c.command;
                            let call_id_matches = exec.record.call_id.as_deref() == Some(call_id.as_ref());
                            command_matches || call_id_matches
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
                let key = chat.provider_order_key_from_order_meta(order);
                let idx = chat.history_insert_with_key_global(Box::new(c), key);
                crate::chatwidget::exec_tools::try_merge_completed_exec_at(chat, idx);
            }
        }
    }

    if exit_code == 0 {
        chat
            .bottom_pane
            .update_status_text("command completed".to_string());
        let gh_ticket = chat.make_background_tail_ticket();
        let tx = chat.app_event_tx.clone();
        let cfg = chat.config.clone();
        crate::chatwidget::gh_actions::maybe_watch_after_push(
            tx,
            cfg,
            &command_for_watch,
            gh_ticket,
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
