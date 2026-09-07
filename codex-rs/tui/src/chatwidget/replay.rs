//! Thread replay rendering for `ChatWidget`.
//!
//! This module rehydrates turns and items into transcript state while avoiding
//! live-only side effects.

use super::*;

impl ChatWidget {
    /// Flush prior activity before live or replayed assistant text.
    pub(super) fn prepare_assistant_message(&mut self) {
        self.flush_unified_exec_wait_streak();
        self.flush_active_cell();
    }

    /// Replay a subset of initial events into the UI to seed the transcript when
    /// resuming an existing session. This approximates the live event flow and
    /// is intentionally conservative: only safe-to-replay items are rendered to
    /// avoid triggering side effects. Event ids are passed as `None` to
    /// distinguish replayed events from live ones.
    pub(crate) fn replay_thread_turns(&mut self, turns: Vec<Turn>, replay_kind: ReplayKind) {
        if matches!(replay_kind, ReplayKind::ThreadSnapshot) && !turns.is_empty() {
            self.warning_display_state.startup_complete = true;
        }
        let latest_turn_id = turns.last().map(|turn| turn.id.clone());
        let hidden_nested_review_turns = std::iter::once(/*value*/ false)
            .chain(turns.windows(/*size*/ 2).map(|turns| {
                crate::app_backtrack::is_hidden_nested_review_turn(&turns[0], &turns[1])
            }))
            .collect::<Vec<_>>();
        for (turn, hidden_nested_review_turn) in turns.into_iter().zip(hidden_nested_review_turns) {
            // Defer completed metadata-only turns until their page loads. Active
            // turns must restore their lifecycle even before any items are available.
            if turn.status == TurnStatus::Completed
                && turn.items_view == codex_app_server_protocol::TurnItemsView::NotLoaded
                && turn.items.is_empty()
            {
                continue;
            }
            let Turn {
                id: turn_id,
                items_view: _,
                items,
                status,
                mut error,
                started_at,
                completed_at,
                duration_ms,
            } = turn;
            let delegated = items.iter().any(|item| {
                matches!(item, ThreadItem::UserMessage { content, .. }
                    if realtime::realtime_delegation_input(content).is_some())
            });
            if matches!(status, TurnStatus::InProgress) {
                if delegated {
                    self.remember_realtime_delegated_reasoning_turn(&turn_id);
                }
                self.warning_display_state.startup_complete = true;
                self.turn_lifecycle.last_turn_id = Some(turn_id.clone());
                self.last_non_retry_error = None;
                self.on_task_started();
            }
            let mut replaying_delegation = false;
            for item in items {
                if matches!(&item, ThreadItem::UserMessage { content, .. }
                    if realtime::realtime_delegation_input(content).is_some())
                {
                    replaying_delegation = true;
                }
                // Voice can steer a typed turn already in progress. Its earlier
                // commentary and reasoning still belong to the typed request.
                if replaying_delegation && realtime::is_private_realtime_agent_item(&item) {
                    continue;
                }
                if hidden_nested_review_turn && matches!(item, ThreadItem::UserMessage { .. }) {
                    continue;
                }
                self.replay_thread_item(item, turn_id.clone(), replay_kind);
            }
            let status = if hidden_nested_review_turn {
                TurnStatus::Completed
            } else {
                status
            };
            // A resolved historical precaution must not clear the restored draft or input queue.
            if Some(&turn_id) != latest_turn_id.as_ref()
                && error.as_ref().is_some_and(|error| {
                    error.codex_error_info
                        == Some(AppServerCodexErrorInfo::MisalignmentPolicyViolation)
                })
            {
                error = None;
            }
            if hidden_nested_review_turn {
                self.turn_lifecycle
                    .rendered_completion_turn_ids
                    .insert(turn_id.clone());
            }
            if matches!(
                status,
                TurnStatus::Completed | TurnStatus::Interrupted | TurnStatus::Failed
            ) {
                self.handle_turn_completed_notification(
                    TurnCompletedNotification {
                        thread_id: self.thread_id.map(|id| id.to_string()).unwrap_or_default(),
                        turn: Turn {
                            id: turn_id,
                            items_view: codex_app_server_protocol::TurnItemsView::NotLoaded,
                            items: Vec::new(),
                            status,
                            error,
                            started_at,
                            completed_at,
                            duration_ms,
                        },
                    },
                    Some(replay_kind),
                );
            }
        }
    }

    pub(crate) fn replay_thread_item(
        &mut self,
        item: ThreadItem,
        turn_id: String,
        replay_kind: ReplayKind,
    ) {
        self.handle_thread_item(item, turn_id, ThreadItemRenderSource::Replay(replay_kind));
    }

    pub(super) fn handle_thread_item(
        &mut self,
        item: ThreadItem,
        turn_id: String,
        render_source: ThreadItemRenderSource,
    ) {
        let from_replay = render_source.is_replay();
        let replay_kind = render_source.replay_kind();
        match item {
            ThreadItem::UserMessage {
                content, client_id, ..
            } => {
                self.on_committed_user_message(
                    &content,
                    client_id.as_deref(),
                    from_replay,
                    &turn_id,
                );
            }
            ThreadItem::AgentMessage {
                id,
                text,
                phase,
                memory_citation,
                delivery,
                questions,
                ..
            } => {
                if self.complete_realtime_delegated_agent_item(
                    &turn_id,
                    &ThreadItem::AgentMessage {
                        id: id.clone(),
                        text: text.clone(),
                        phase: phase.clone(),
                        memory_citation: memory_citation.clone(),
                        delivery,
                        questions: questions.clone(),
                    },
                    from_replay,
                ) {
                    return;
                }
                self.on_agent_message_item_completed(
                    AgentMessageItem {
                        id,
                        content: vec![AgentMessageContent::Text { text }],
                        phase,
                        memory_citation: memory_citation.map(|citation| {
                            codex_protocol::memory_citation::MemoryCitation {
                                entries: citation
                                    .entries
                                    .into_iter()
                                    .map(|entry| {
                                        codex_protocol::memory_citation::MemoryCitationEntry {
                                            path: entry.path,
                                            line_start: entry.line_start,
                                            line_end: entry.line_end,
                                            note: entry.note,
                                        }
                                    })
                                    .collect(),
                                rollout_ids: citation.thread_ids,
                            }
                        }),
                        delivery,
                        questions,
                    },
                    &turn_id,
                    from_replay,
                );
            }
            ThreadItem::Plan { text, .. } => self.on_plan_item_completed(text),
            ThreadItem::Reasoning {
                summary, content, ..
            } => {
                if from_replay {
                    let reasoning_parts = summary.into_iter().chain(
                        self.config
                            .show_raw_agent_reasoning
                            .then_some(content)
                            .into_iter()
                            .flatten(),
                    );
                    for (index, delta) in reasoning_parts.enumerate() {
                        if index > 0 {
                            self.on_reasoning_section_break();
                        }
                        self.on_agent_reasoning_delta(delta);
                    }
                }
                self.on_agent_reasoning_final();
            }
            item @ ThreadItem::CommandExecution {
                status: codex_app_server_protocol::CommandExecutionStatus::InProgress,
                ..
            } => self.on_command_execution_started(item),
            item @ ThreadItem::CommandExecution {
                source: ExecCommandSource::Agent | ExecCommandSource::UnifiedExecStartup,
                status:
                    codex_app_server_protocol::CommandExecutionStatus::Completed
                    | codex_app_server_protocol::CommandExecutionStatus::Failed,
                ..
            } if from_replay => self.handle_command_execution_completed_now(item),
            item @ ThreadItem::CommandExecution { .. } => self.on_command_execution_completed(item),
            ThreadItem::FileChange {
                status: codex_app_server_protocol::PatchApplyStatus::InProgress,
                ..
            } => {}
            item @ ThreadItem::FileChange { .. } => self.on_file_change_completed(item),
            item @ ThreadItem::McpToolCall {
                status: codex_app_server_protocol::McpToolCallStatus::InProgress,
                ..
            } => self.on_mcp_tool_call_started(item),
            item @ ThreadItem::McpToolCall { .. } => self.on_mcp_tool_call_completed(item),
            ThreadItem::WebSearch(item) => {
                self.on_web_search_begin(item.id.clone());
                self.on_web_search_end(
                    item.id,
                    item.query,
                    item.action
                        .unwrap_or(codex_app_server_protocol::WebSearchAction::Other),
                );
            }
            ThreadItem::ImageView { id: _, path } => {
                self.on_view_image_tool_call(path);
            }
            ThreadItem::ImageGeneration(item) => {
                self.on_image_generation_end(
                    item.id,
                    item.status,
                    item.revised_prompt,
                    item.saved_path,
                );
            }
            ThreadItem::EnteredReviewMode { review, .. } => {
                if from_replay {
                    self.enter_review_mode_with_hint(review, /*from_replay*/ true);
                }
            }
            ThreadItem::ExitedReviewMode { .. } => {
                self.exit_review_mode_after_item();
            }
            ThreadItem::ContextCompaction { id } => {
                self.on_context_compaction_completed(&id, from_replay);
            }
            ThreadItem::FunctionCallOutput {
                name,
                namespace,
                output,
                ..
            } => {
                if let Some((source_thread_id, prompt)) =
                    crate::dynamic_tools::parse_delegated_tool_output(
                        &name,
                        namespace.as_deref(),
                        &output,
                    )
                {
                    self.add_to_history(history_cell::PrefixedWrappedHistoryCell::new(
                        format!("Sent by Codex from task {source_thread_id}\n{prompt}"),
                        "• ".dim(),
                        "  ",
                    ));
                }
            }
            ThreadItem::HookPrompt { .. } => {}
            ThreadItem::CollabAgentToolCall {
                id,
                tool,
                status,
                sender_thread_id,
                receiver_thread_ids,
                prompt,
                model,
                reasoning_effort,
                agents_states,
            } => self.on_collab_agent_tool_call(ThreadItem::CollabAgentToolCall {
                id,
                tool,
                status,
                sender_thread_id,
                receiver_thread_ids,
                prompt,
                model,
                reasoning_effort,
                agents_states,
            }),
            item @ ThreadItem::SubAgentActivity { .. } => self.on_sub_agent_activity(item),
            ThreadItem::DynamicToolCall { .. } => {}
            ThreadItem::Sleep(_) => {}
        }

        if matches!(replay_kind, Some(ReplayKind::ThreadSnapshot)) && turn_id.is_empty() {
            self.request_redraw();
        }
    }
}
