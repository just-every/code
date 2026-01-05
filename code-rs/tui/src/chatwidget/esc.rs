use crossterm::event::KeyEvent;
use std::time::{Duration, Instant};

use super::{ChatWidget, AUTO_ESC_EXIT_HINT, DOUBLE_ESC_HINT};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EscIntent {
    DismissModal,
    CloseSettings,
    CloseFilePopup,
    AutoPauseForEdit,
    AutoStopDuringApproval,
    AutoStopActive,
    AutoGoalEnableEdit,
    AutoGoalExitPreserveDraft,
    AutoDismissSummary,
    DiffConfirm,
    AgentsTerminal,
    CancelAgents,
    CancelTask,
    ClearComposer,
    ShowUndoHint,
    OpenUndoTimeline,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AutoGoalEscState {
    Inactive,
    NeedsEnableEditing,
    ArmedForExit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EscRoute {
    pub intent: EscIntent,
    pub consume: bool,
    pub allows_double_esc: bool,
}

impl EscRoute {
    const fn new(intent: EscIntent, consume: bool, allows_double_esc: bool) -> Self {
        Self {
            intent,
            consume,
            allows_double_esc,
        }
    }
}

impl ChatWidget<'_> {
    // --- Double‑Escape helpers ---
    pub(crate) fn double_esc_hint_label() -> &'static str {
        DOUBLE_ESC_HINT
    }

    pub(crate) fn show_esc_undo_hint(&mut self) {
        self.bottom_pane
            .flash_footer_notice(format!("Esc {}", Self::double_esc_hint_label()));
    }

    pub(super) fn show_auto_drive_exit_hint(&mut self) {
        self.bottom_pane
            .set_standard_terminal_hint(Some(AUTO_ESC_EXIT_HINT.to_string()));
    }

    fn auto_stop_via_escape(&mut self, message: Option<String>) {
        self.auto_stop(message);
        self.bottom_pane
            .update_status_text("Auto Drive stopped.".to_string());
        if self.auto_state.last_run_summary.is_some() {
            self.auto_clear_summary_panel();
        } else {
            self.bottom_pane.set_standard_terminal_hint(None);
            self.bottom_pane.ensure_input_focus();
            self.request_redraw();
        }
    }

    fn auto_clear_summary_panel(&mut self) {
        if self.auto_state.last_run_summary.is_none() {
            self.bottom_pane.set_standard_terminal_hint(None);
            self.bottom_pane.ensure_input_focus();
            self.request_redraw();
            return;
        }
        self.auto_state.last_run_summary = None;
        self.bottom_pane.clear_auto_coordinator_view(true);
        self.bottom_pane.clear_live_ring();
        self.bottom_pane.set_standard_terminal_hint(None);
        self.bottom_pane.ensure_input_focus();
        self.auto_rebuild_live_ring();
        self.request_redraw();
    }

    pub(crate) fn describe_esc_context(&self) -> EscRoute {
        if self.diffs.confirm.is_some() {
            return EscRoute::new(EscIntent::DiffConfirm, true, false);
        }

        if self.settings.overlay.is_some() {
            return EscRoute::new(EscIntent::CloseSettings, true, false);
        }

        if self.has_active_modal_view() {
            return EscRoute::new(EscIntent::DismissModal, true, false);
        }

        if self.agents_terminal.active {
            return EscRoute::new(EscIntent::AgentsTerminal, true, false);
        }

        if self.bottom_pane.file_popup_visible() {
            return EscRoute::new(EscIntent::CloseFilePopup, false, false);
        }

        if self.auto_state.is_active() {
            let awaiting_continue_cta = self.auto_should_show_continue_cta();

            if awaiting_continue_cta {
                return EscRoute::new(EscIntent::AutoStopDuringApproval, true, false);
            }

            if self.auto_state.countdown_active()
                || (self.auto_state.awaiting_coordinator_submit()
                    && !self.auto_state.is_paused_manual())
            {
                return EscRoute::new(EscIntent::AutoPauseForEdit, true, false);
            }

            if self.has_cancelable_task_activity() {
                return EscRoute::new(EscIntent::CancelTask, true, false);
            }

            if self.auto_state.awaiting_coordinator_submit() {
                return EscRoute::new(EscIntent::AutoStopDuringApproval, true, false);
            }

            return EscRoute::new(EscIntent::AutoStopActive, true, false);
        }

        if self.has_cancelable_task_activity() {
            return EscRoute::new(EscIntent::CancelTask, true, false);
        }

        if self.has_cancelable_agents() {
            return EscRoute::new(EscIntent::CancelAgents, true, false);
        }

        if self.auto_state.should_show_goal_entry() {
            return EscRoute::new(
                match self.auto_goal_escape_state {
                    AutoGoalEscState::Inactive => EscIntent::AutoGoalExitPreserveDraft,
                    AutoGoalEscState::NeedsEnableEditing => EscIntent::AutoGoalEnableEdit,
                    AutoGoalEscState::ArmedForExit => EscIntent::AutoGoalExitPreserveDraft,
                },
                true,
                false,
            );
        }

        if self.auto_state.last_run_summary.is_some() {
            return EscRoute::new(EscIntent::AutoDismissSummary, true, false);
        }

        if self.auto_manual_entry_active() && !self.composer_is_empty() {
            return EscRoute::new(EscIntent::ClearComposer, true, false);
        }

        if !self.composer_is_empty() {
            return EscRoute::new(EscIntent::ClearComposer, true, false);
        }

        EscRoute::new(EscIntent::ShowUndoHint, true, true)
    }

    fn has_cancelable_task_activity(&self) -> bool {
        self.stream.is_write_cycle_active()
            || !self.active_task_ids.is_empty()
            || self.terminal_is_running()
            || !self.exec.running_commands.is_empty()
            || !self.tools_state.running_custom_tools.is_empty()
            || !self.tools_state.web_search_sessions.is_empty()
            || !self.tools_state.running_wait_tools.is_empty()
            || !self.tools_state.running_kill_tools.is_empty()
    }

    pub(crate) fn execute_esc_intent(&mut self, intent: EscIntent, key_event: KeyEvent) -> bool {
        match intent {
            EscIntent::DismissModal => {
                self.handle_key_event(key_event);
                true
            }
            EscIntent::CloseSettings => {
                self.handle_key_event(key_event);
                true
            }
            EscIntent::CloseFilePopup => self.close_file_popup_if_active(),
            EscIntent::AutoPauseForEdit => {
                self.auto_pause_for_manual_edit(false);
                true
            }
            EscIntent::AutoStopDuringApproval => {
                self.bottom_pane
                    .update_status_text("Auto Drive stopped during approval.".to_string());
                self.auto_stop_via_escape(Some("Auto Drive stopped during approval.".to_string()));
                true
            }
            EscIntent::AutoStopActive => {
                self.bottom_pane
                    .update_status_text("Stopping Auto Drive…".to_string());
                self.auto_stop_via_escape(Some("Auto Drive stopped by user.".to_string()));
                true
            }
            EscIntent::AutoGoalEnableEdit => {
                self.auto_goal_escape_state = AutoGoalEscState::ArmedForExit;
                self.bottom_pane.ensure_input_focus();
                self.request_redraw();
                true
            }
            EscIntent::AutoGoalExitPreserveDraft => self.auto_exit_goal_entry_preserve_draft(),
            EscIntent::AutoDismissSummary => {
                self.auto_clear_summary_panel();
                true
            }
            EscIntent::DiffConfirm => {
                self.diffs.confirm = None;
                self.request_redraw();
                true
            }
            EscIntent::AgentsTerminal => {
                self.handle_key_event(key_event);
                true
            }
            EscIntent::CancelAgents => self.cancel_active_agents(),
            EscIntent::CancelTask => {
                let had_running = self.is_task_running();
                let auto_was_active = self.auto_state.is_active();
                let _ = self.on_ctrl_c();
                if had_running {
                    if auto_was_active {
                        self.bottom_pane.update_status_text(
                            "Command cancelled. Esc stops Auto Drive.".to_string(),
                        );
                        self.auto_stop_via_escape(Some("Auto Drive stopped by user.".to_string()));
                    } else {
                        self.bottom_pane
                            .update_status_text("Command cancelled.".to_string());
                    }
                }
                true
            }
            EscIntent::ClearComposer => {
                self.clear_composer();
                true
            }
            EscIntent::ShowUndoHint => {
                self.show_esc_undo_hint();
                true
            }
            EscIntent::OpenUndoTimeline => {
                self.handle_undo_command();
                true
            }
            EscIntent::None => false,
        }
    }

    pub(crate) fn handle_app_esc(
        &mut self,
        esc_event: KeyEvent,
        last_esc_time: &mut Option<Instant>,
    ) -> bool {
        let now = Instant::now();
        const THRESHOLD: Duration = Duration::from_millis(600);
        let double_ready = last_esc_time.is_some_and(|prev| now.duration_since(prev) <= THRESHOLD);

        let mut handled = false;
        let mut attempts = 0;

        while attempts < 8 {
            attempts += 1;
            let route = self.describe_esc_context();
            let mut intent = route.intent;

            if intent == EscIntent::None {
                break;
            }

            if intent == EscIntent::ShowUndoHint && route.allows_double_esc && double_ready {
                intent = EscIntent::OpenUndoTimeline;
            }

            let performed = self.execute_esc_intent(intent, esc_event);

            match intent {
                EscIntent::CloseFilePopup if !route.consume => {
                    if !performed {
                        break;
                    }
                    continue;
                }
                EscIntent::CloseFilePopup => {
                    handled = true;
                    break;
                }
                EscIntent::ShowUndoHint => {
                    if route.allows_double_esc && !double_ready {
                        *last_esc_time = Some(now);
                    } else {
                        *last_esc_time = None;
                    }
                    handled = true;
                    break;
                }
                EscIntent::OpenUndoTimeline => {
                    *last_esc_time = None;
                    handled = true;
                    break;
                }
                EscIntent::CancelTask | EscIntent::ClearComposer => {
                    let route_after = self.describe_esc_context();
                    if route_after.intent == EscIntent::ShowUndoHint && route_after.allows_double_esc {
                        *last_esc_time = Some(now);
                    } else {
                        *last_esc_time = None;
                    }
                    handled = true;
                    break;
                }
                _ => {
                    handled = true;
                    break;
                }
            }
        }

        handled
    }

    pub(super) fn auto_sync_goal_escape_state_from_composer(&mut self) {
        if !self.auto_state.should_show_goal_entry() {
            return;
        }

        let has_content = !self.bottom_pane.composer_text().trim().is_empty();
        match self.auto_goal_escape_state {
            AutoGoalEscState::Inactive => {
                if has_content {
                    self.auto_goal_escape_state = AutoGoalEscState::NeedsEnableEditing;
                }
            }
            AutoGoalEscState::NeedsEnableEditing | AutoGoalEscState::ArmedForExit => {
                if !has_content {
                    self.auto_goal_escape_state = AutoGoalEscState::Inactive;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::chatwidget::message::UserMessage;
    use crate::chatwidget::smoke_helpers::ChatWidgetHarness;
    use crate::chatwidget::{ExecCallId, RunningCommand};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn make_running_command() -> RunningCommand {
        RunningCommand {
            command: vec!["sleep".to_string(), "1".to_string()],
            parsed: Vec::new(),
            history_index: None,
            history_id: None,
            explore_entry: None,
            stdout_offset: 0,
            stderr_offset: 0,
            wait_total: None,
            wait_active: false,
            wait_notes: Vec::new(),
        }
    }

    #[test]
    fn esc_cancel_does_not_prime_undo_when_queue_restores_composer() {
        let mut harness = ChatWidgetHarness::new();
        let chat = harness.chat();

        chat.exec
            .running_commands
            .insert(ExecCallId("exec-1".to_string()), make_running_command());
        chat.bottom_pane.set_task_running(true);
        chat.queued_user_messages
            .push_back(UserMessage::from("next prompt".to_string()));

        let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let mut last_esc_time = None;

        assert!(chat.handle_app_esc(esc_event, &mut last_esc_time));
        assert!(
            !chat.composer_is_empty(),
            "queued message should restore into composer after cancel"
        );
        assert!(
            last_esc_time.is_none(),
            "double-esc should not prime while composer refilled after cancel"
        );
    }
}
