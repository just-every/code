//! Bottom pane: shows the ChatComposer or a BottomPaneView, if one is active.

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::user_approval_widget::ApprovalRequest;
use crate::util::buffer::fill_rect;
use bottom_pane_view::BottomPaneView;
use codex_core::protocol::TokenUsage;
use codex_file_search::FileMatch;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;
use std::time::Duration;

mod agent_editor_view;
mod agents_overview_view;
mod approval_modal_view;
mod bottom_pane_view;
mod chat_composer;
mod quality_gate_modal;
mod chat_composer_history;
pub mod chrome_selection_view;
mod command_popup;
mod custom_prompt_view;
mod diff_popup;
mod file_search_popup;
pub mod list_selection_view;
mod live_ring_widget;
mod model_selection_view;
mod paste_burst;
mod popup_consts;
mod scroll_state;
mod selection_popup_common;
pub(crate) use custom_prompt_view::CustomPromptView;
pub(crate) use list_selection_view::SelectionAction;
pub mod agents_settings_view;
mod github_settings_view;
mod login_accounts_view;
pub mod mcp_settings_view;
pub mod resume_selection_view;
// no direct use of list_selection_view or its items here
pub mod form_text_field;
mod textarea;
mod theme_selection_view;
mod undo_restore_view;
mod update_settings_view;
pub(crate) mod validation_settings_view;
mod verbosity_selection_view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancellationEvent {
    Ignored,
    Handled,
}

pub(crate) use chat_composer::ChatComposer;
pub(crate) use chat_composer::InputResult;
pub(crate) use login_accounts_view::{
    LoginAccountsState, LoginAccountsView, LoginAddAccountState, LoginAddAccountView,
};

pub(crate) use update_settings_view::{UpdateSettingsView, UpdateSharedState};

use approval_modal_view::ApprovalModalView;
use codex_common::model_presets::ModelPreset;
use codex_core::config_types::ReasoningEffort;
use codex_core::config_types::TextVerbosity;
use codex_core::config_types::ThemeName;
use codex_core::protocol::Op;
use model_selection_view::ModelSelectionView;
use quality_gate_modal::QualityGateModal;
use theme_selection_view::ThemeSelectionView;
pub(crate) use undo_restore_view::UndoRestoreView;
use verbosity_selection_view::VerbositySelectionView;

/// Pane displayed in the lower half of the chat UI.
pub(crate) struct BottomPane<'a> {
    /// Composer is retained even when a BottomPaneView is displayed so the
    /// input state is retained when the view is closed.
    composer: ChatComposer,

    /// If present, this is displayed instead of the `composer`.
    active_view: Option<Box<dyn BottomPaneView<'a> + 'a>>,

    app_event_tx: AppEventSender,
    has_input_focus: bool,
    is_task_running: bool,
    ctrl_c_quit_hint: bool,

    /// Optional transient ring shown above the composer. This is a rendering-only
    /// container used during development before we wire it to ChatWidget events.
    live_ring: Option<live_ring_widget::LiveRingWidget>,

    /// True if the active view is the StatusIndicatorView that replaces the
    /// composer during a running task.
    status_view_active: bool,

    /// Whether to reserve an empty spacer line above the input composer.
    /// Defaults to true for visual breathing room, but can be disabled when
    /// the chat history is scrolled up to allow history to reclaim that row.
    top_spacer_enabled: bool,

    pub(crate) using_chatgpt_auth: bool,
}

pub(crate) struct BottomPaneParams {
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) has_input_focus: bool,
    pub(crate) enhanced_keys_supported: bool,
    pub(crate) using_chatgpt_auth: bool,
}

impl BottomPane<'_> {
    // Reduce bottom padding so footer sits one line lower
    const BOTTOM_PAD_LINES: u16 = 1;
    pub fn new(params: BottomPaneParams) -> Self {
        let enhanced_keys_supported = params.enhanced_keys_supported;
        Self {
            composer: ChatComposer::new(
                params.has_input_focus,
                params.app_event_tx.clone(),
                enhanced_keys_supported,
                params.using_chatgpt_auth,
            ),
            active_view: None,
            app_event_tx: params.app_event_tx,
            has_input_focus: params.has_input_focus,
            is_task_running: false,
            ctrl_c_quit_hint: false,
            live_ring: None,
            status_view_active: false,
            top_spacer_enabled: true,
            using_chatgpt_auth: params.using_chatgpt_auth,
        }
    }

    /// Show Agents overview (Agents + Commands sections)
    pub fn show_agents_overview(
        &mut self,
        agents: Vec<(String, bool, bool, String)>,
        commands: Vec<String>,
        selected_index: usize,
    ) {
        use agents_overview_view::AgentsOverviewView;
        let view =
            AgentsOverviewView::new(agents, commands, selected_index, self.app_event_tx.clone());
        self.active_view = Some(Box::new(view));
        self.status_view_active = false;
        self.request_redraw();
    }

    pub fn show_update_settings(&mut self, view: update_settings_view::UpdateSettingsView) {
        if !crate::updates::upgrade_ui_enabled() {
            self.request_redraw();
            return;
        }

        self.active_view = Some(Box::new(view));
        self.status_view_active = false;
        self.request_redraw();
    }

    /// Show per-agent editor
    pub fn show_agent_editor(
        &mut self,
        name: String,
        enabled: bool,
        args_read_only: Option<Vec<String>>,
        args_write: Option<Vec<String>>,
        instructions: Option<String>,
        command: String,
    ) {
        use agent_editor_view::AgentEditorView;
        let view = AgentEditorView::new(
            name,
            enabled,
            args_read_only,
            args_write,
            instructions,
            command,
            self.app_event_tx.clone(),
        );
        self.active_view = Some(Box::new(view));
        self.status_view_active = false;
        self.request_redraw();
    }

    pub fn show_login_accounts(&mut self, view: LoginAccountsView) {
        self.active_view = Some(Box::new(view));
        self.status_view_active = false;
        self.request_redraw();
    }

    pub fn show_login_add_account(&mut self, view: LoginAddAccountView) {
        self.active_view = Some(Box::new(view));
        self.status_view_active = false;
        self.request_redraw();
    }

    pub fn set_using_chatgpt_auth(&mut self, using: bool) {
        if self.using_chatgpt_auth != using {
            self.using_chatgpt_auth = using;
            self.composer.set_using_chatgpt_auth(using);
            self.request_redraw();
        }
    }

    pub fn set_has_chat_history(&mut self, has_history: bool) {
        self.composer.set_has_chat_history(has_history);
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        let ring_h = self
            .live_ring
            .as_ref()
            .map(|r| r.desired_height(width))
            .unwrap_or(0);

        let (view_height, pad_lines) = if let Some(view) = self.active_view.as_ref() {
            (view.desired_height(width), 0)
        } else {
            // Optionally add 1 for the empty line above the composer
            let spacer = if self.top_spacer_enabled { 1 } else { 0 };
            (
                spacer + self.composer.desired_height(width),
                Self::BOTTOM_PAD_LINES,
            )
        };

        ring_h.saturating_add(view_height).saturating_add(pad_lines)
    }

    pub fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        // Hide the cursor whenever an overlay view is active (e.g. approval modal).
        // But keep cursor visible when only status overlay is shown.
        if self.active_view.is_some() {
            None
        } else {
            // Account for the optional empty line above the composer
            let y_offset = if self.top_spacer_enabled { 1u16 } else { 0u16 };

            // Adjust composer area to account for empty line and padding
            let horizontal_padding = 1u16; // Message input uses 1 char padding
            let composer_rect = Rect {
                x: area.x + horizontal_padding,
                y: area.y + y_offset,
                width: area.width.saturating_sub(horizontal_padding * 2),
                height: (area.height.saturating_sub(y_offset))
                    - BottomPane::BOTTOM_PAD_LINES
                        .min((area.height.saturating_sub(y_offset)).saturating_sub(1)),
            };
            self.composer.cursor_pos(composer_rect)
        }
    }

    /// Forward a key event to the active view or the composer.
    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> InputResult {
        if let Some(mut view) = self.active_view.take() {
            view.handle_key_event(self, key_event);
            if !view.is_complete() {
                self.active_view = Some(view);
            }
            // Don't create a status view - keep composer visible
            // Debounce view navigation redraws to reduce render thrash
            self.request_redraw();
            InputResult::None
        } else {
            // If a task is running and a status line is visible, allow Esc to
            // send an interrupt even while the composer has focus.
            if matches!(key_event.code, crossterm::event::KeyCode::Esc) && self.is_task_running {
                // Send Op::Interrupt directly when a task is running so Esc can cancel.
                self.app_event_tx.send(AppEvent::CodexOp(Op::Interrupt));
                self.request_redraw();
                return InputResult::None;
            }
            let (input_result, needs_redraw) = self.composer.handle_key_event(key_event);
            if needs_redraw {
                // Route input updates through the app's debounced redraw path
                // so typing doesn't attempt a full-screen redraw per key.
                self.request_redraw();
            }
            if self.composer.is_in_paste_burst() {
                self.request_redraw_in(ChatComposer::recommended_paste_flush_delay());
            }
            input_result
        }
    }

    /// Attempt to navigate history upwards from the composer. Returns true if consumed.
    pub(crate) fn try_history_up(&mut self) -> bool {
        let consumed = self.composer.try_history_up();
        if consumed {
            self.request_redraw();
        }
        consumed
    }

    /// Attempt to navigate history downwards from the composer. Returns true if consumed.
    pub(crate) fn try_history_down(&mut self) -> bool {
        let consumed = self.composer.try_history_down();
        if consumed {
            self.request_redraw();
        }
        consumed
    }

    /// Returns true if the composer is currently browsing history.
    pub(crate) fn history_is_browsing(&self) -> bool {
        self.composer.history_is_browsing()
    }

    /// After a chat scroll-up, make the next Down key scroll chat instead of moving within input.
    pub(crate) fn mark_next_down_scrolls_history(&mut self) {
        self.composer.mark_next_down_scrolls_history();
    }

    /// Handle Ctrl-C in the bottom pane. If a modal view is active it gets a
    /// chance to consume the event (e.g. to dismiss itself).
    pub(crate) fn on_ctrl_c(&mut self) -> CancellationEvent {
        let mut view = match self.active_view.take() {
            Some(view) => view,
            None => return CancellationEvent::Ignored,
        };

        let event = view.on_ctrl_c(self);
        match event {
            CancellationEvent::Handled => {
                if !view.is_complete() {
                    self.active_view = Some(view);
                }
                // Don't create a status view - keep composer visible
                self.show_ctrl_c_quit_hint();
            }
            CancellationEvent::Ignored => {
                self.active_view = Some(view);
            }
        }
        event
    }

    pub fn handle_paste(&mut self, pasted: String) {
        if let Some(ref mut view) = self.active_view {
            use crate::bottom_pane::bottom_pane_view::ConditionalUpdate;
            match view.handle_paste(pasted) {
                ConditionalUpdate::NeedsRedraw => self.request_redraw(),
                ConditionalUpdate::NoRedraw => {}
            }
            return;
        }
        let needs_redraw = self.composer.handle_paste(pasted);
        if needs_redraw {
            // Large pastes may arrive as bursts; coalesce paints
            self.request_redraw();
        }
    }

    pub(crate) fn insert_str(&mut self, text: &str) {
        self.composer.insert_str(text);
        self.request_redraw();
    }

    /// Clear the composer text and reset transient composer state.
    pub(crate) fn clear_composer(&mut self) {
        self.composer.clear_text();
        self.request_redraw();
    }

    /// Attempt to close the file-search popup if visible. Returns true if closed.
    pub(crate) fn close_file_popup_if_active(&mut self) -> bool {
        let closed = self.composer.close_file_popup_if_active();
        if closed {
            self.request_redraw();
        }
        closed
    }

    /// True if a modal/overlay view is currently displayed (not the composer popup).
    pub(crate) fn has_active_modal_view(&self) -> bool {
        // Consider a modal inactive once it has completed to avoid blocking
        // Esc routing and other overlay checks after a decision is made.
        match self.active_view.as_ref() {
            Some(view) => !view.is_complete(),
            None => false,
        }
    }

    /// Enable or disable compact compose mode. When enabled, the spacer line
    /// above the input composer is removed so the history can scroll into that
    /// row. This is typically toggled when the user scrolls up.
    pub(crate) fn set_compact_compose(&mut self, compact: bool) {
        let new_enabled = !compact;
        if self.top_spacer_enabled != new_enabled {
            self.top_spacer_enabled = new_enabled;
            self.request_redraw();
        }
    }

    /// Update the status indicator text. Shows status as overlay above composer
    /// to allow continued input while processing.
    pub(crate) fn update_status_text(&mut self, text: String) {
        // If there's an active modal view that can handle status updates, let it
        if let Some(view) = self.active_view.as_mut() {
            if matches!(
                view.update_status_text(text.clone()),
                bottom_pane_view::ConditionalUpdate::NeedsRedraw
            ) {
                self.request_redraw();
                return;
            }
        }

        // Pass status message to composer for dynamic title display
        self.composer.update_status_message(text);
        self.request_redraw();
    }

    /// Show an ephemeral footer notice for a custom duration.
    pub(crate) fn flash_footer_notice_for(&mut self, text: String, dur: Duration) {
        self.composer.flash_footer_notice_for(text, dur);
        // Ask app to clear it slightly after expiry to avoid flicker on boundary
        self.app_event_tx
            .send(AppEvent::ScheduleFrameIn(dur + Duration::from_millis(100)));
        self.request_redraw();
    }

    pub(crate) fn set_standard_terminal_hint(&mut self, hint: Option<String>) {
        self.composer.set_standard_terminal_hint(hint);
        self.request_redraw();
    }

    pub(crate) fn show_ctrl_c_quit_hint(&mut self) {
        self.ctrl_c_quit_hint = true;
        self.composer
            .set_ctrl_c_quit_hint(true, self.has_input_focus);
        self.request_redraw();
    }

    pub(crate) fn clear_ctrl_c_quit_hint(&mut self) {
        if self.ctrl_c_quit_hint {
            self.ctrl_c_quit_hint = false;
            self.composer
                .set_ctrl_c_quit_hint(false, self.has_input_focus);
            self.request_redraw();
        }
    }

    pub(crate) fn ctrl_c_quit_hint_visible(&self) -> bool {
        self.ctrl_c_quit_hint
    }

    pub fn set_task_running(&mut self, running: bool) {
        self.is_task_running = running;
        self.composer.set_task_running(running);

        if running {
            // No longer need separate status widget - title shows in composer
            self.request_redraw();
        } else {
            // Status now shown in composer title
            // Drop the status view when a task completes, but keep other
            // modal views (e.g. approval dialogs).
            if let Some(mut view) = self.active_view.take() {
                if !view.should_hide_when_task_is_done() {
                    self.active_view = Some(view);
                }
                self.status_view_active = false;
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn composer_is_empty(&self) -> bool {
        self.composer.is_empty()
    }

    pub(crate) fn composer_text(&self) -> String {
        self.composer.text().to_string()
    }

    pub(crate) fn is_task_running(&self) -> bool {
        self.is_task_running
    }

    // is_normal_backtrack_mode removed; App-level policy handles Esc behavior directly.

    /// Update the *context-window remaining* indicator in the composer. This
    /// is forwarded directly to the underlying `ChatComposer`.
    pub(crate) fn set_token_usage(
        &mut self,
        total_token_usage: TokenUsage,
        last_token_usage: TokenUsage,
        model_context_window: Option<u64>,
    ) {
        self.composer
            .set_token_usage(total_token_usage, last_token_usage, model_context_window);
        self.request_redraw();
    }

    /// Called when the agent requests user approval.
    pub fn push_approval_request(&mut self, request: ApprovalRequest) {
        let request = if let Some(view) = self.active_view.as_mut() {
            match view.try_consume_approval_request(request) {
                Some(request) => request,
                None => {
                    self.request_redraw();
                    return;
                }
            }
        } else {
            request
        };

        // Otherwise create a new approval modal overlay.
        let modal = ApprovalModalView::new(request, self.app_event_tx.clone());
        self.active_view = Some(Box::new(modal));
        // Hide any overlay status while a modal is visible.
        // Status shown in composer title now
        self.status_view_active = false;
        self.request_redraw()
    }

    /// Show the model selection UI
    pub fn show_model_selection(
        &mut self,
        presets: Vec<ModelPreset>,
        current_model: String,
        current_effort: ReasoningEffort,
    ) {
        let view = ModelSelectionView::new(
            presets,
            current_model,
            current_effort,
            self.app_event_tx.clone(),
        );
        self.active_view = Some(Box::new(view));
        // Status shown in composer title now
        self.status_view_active = false;
        self.request_redraw()
    }

    /// Show the theme selection UI
    pub fn show_theme_selection(&mut self, current_theme: ThemeName) {
        let view = ThemeSelectionView::new(current_theme, self.app_event_tx.clone());
        self.active_view = Some(Box::new(view));
        // Status shown in composer title now
        self.status_view_active = false;
        self.request_redraw()
    }

    /// Show the Chrome launch options UI
    pub fn show_chrome_selection(&mut self, port: Option<u16>) {
        use chrome_selection_view::ChromeSelectionView;
        let view = ChromeSelectionView::new(self.app_event_tx.clone(), port);
        self.active_view = Some(Box::new(view));
        // Status shown in composer title now
        self.status_view_active = false;
        self.request_redraw()
    }

    /// Show the diffs popup with tabs for each file.
    #[allow(dead_code)]
    pub fn show_diff_popup(&mut self, tabs: Vec<(String, Vec<ratatui::text::Line<'static>>)>) {
        let view = diff_popup::DiffPopupView::new(tabs);
        self.active_view = Some(Box::new(view));
        self.status_view_active = false;
        self.request_redraw()
    }

    /// Show the verbosity selection UI
    pub fn show_verbosity_selection(&mut self, current_verbosity: TextVerbosity) {
        let view = VerbositySelectionView::new(current_verbosity, self.app_event_tx.clone());
        self.active_view = Some(Box::new(view));
        // Status shown in composer title now
        self.status_view_active = false;
        self.request_redraw()
    }

    /// Show a multi-line prompt input view (used for custom review instructions)
    pub fn show_custom_prompt(&mut self, view: CustomPromptView) {
        self.active_view = Some(Box::new(view));
        self.status_view_active = false;
        self.request_redraw();
    }

    // === FORK-SPECIFIC: Quality gate modal (T85) ===
    /// Show quality gate questions modal
    pub fn show_quality_gate_modal(
        &mut self,
        checkpoint: crate::chatwidget::spec_kit::QualityCheckpoint,
        questions: Vec<crate::chatwidget::spec_kit::EscalatedQuestion>,
    ) {
        let modal = QualityGateModal::new(checkpoint, questions, self.app_event_tx.clone());
        self.active_view = Some(Box::new(modal));
        self.status_view_active = false;
        self.request_redraw()
    }
    // === END FORK-SPECIFIC ===

    /// Show a generic list selection popup with items and actions.
    pub fn show_list_selection(
        &mut self,
        _title: String,
        _subtitle: Option<String>,
        _footer_hint: Option<String>,
        items: crate::bottom_pane::list_selection_view::ListSelectionView,
    ) {
        self.active_view = Some(Box::new(items));
        // Status shown in composer title now
        self.status_view_active = false;
        self.request_redraw();
    }

    /// Show the resume selection UI with structured rows
    pub fn show_resume_selection(
        &mut self,
        title: String,
        subtitle: Option<String>,
        rows: Vec<resume_selection_view::ResumeRow>,
    ) {
        use resume_selection_view::ResumeSelectionView;
        let view = ResumeSelectionView::new(
            title,
            subtitle.unwrap_or_default(),
            rows,
            self.app_event_tx.clone(),
        );
        self.active_view = Some(Box::new(view));
        self.status_view_active = false;
        self.request_redraw()
    }

    /// Show GitHub settings (token status + watcher toggle)
    pub fn show_github_settings(
        &mut self,
        watcher_enabled: bool,
        token_status: String,
        ready: bool,
    ) {
        use github_settings_view::GithubSettingsView;
        let view = GithubSettingsView::new(
            watcher_enabled,
            token_status,
            ready,
            self.app_event_tx.clone(),
        );
        self.active_view = Some(Box::new(view));
        self.status_view_active = false;
        self.request_redraw();
    }

    pub fn show_undo_restore_view(&mut self, view: UndoRestoreView) {
        self.active_view = Some(Box::new(view));
        self.status_view_active = false;
        self.request_redraw();
    }

    /// Show MCP servers status/toggle UI
    pub fn show_mcp_settings(
        &mut self,
        rows: crate::bottom_pane::mcp_settings_view::McpServerRows,
    ) {
        use mcp_settings_view::McpSettingsView;
        let view = McpSettingsView::new(rows, self.app_event_tx.clone());
        self.active_view = Some(Box::new(view));
        self.status_view_active = false;
        self.request_redraw();
    }

    /// Show validation harness settings (master toggle + per-tool toggles).
    pub fn show_validation_settings(
        &mut self,
        groups: Vec<(validation_settings_view::GroupStatus, bool)>,
        tools: Vec<validation_settings_view::ToolRow>,
    ) {
        use validation_settings_view::ValidationSettingsView;
        let view = ValidationSettingsView::new(groups, tools, self.app_event_tx.clone());
        self.active_view = Some(Box::new(view));
        self.status_view_active = false;
        self.request_redraw();
    }

    /// Show Subagent editor UI
    pub fn show_subagent_editor(
        &mut self,
        name: String,
        available_agents: Vec<String>,
        existing: Vec<codex_core::config_types::SubagentCommandConfig>,
        is_new: bool,
    ) {
        use crate::bottom_pane::agents_settings_view::SubagentEditorView;
        let view = SubagentEditorView::new_with_data(
            name,
            available_agents,
            existing,
            is_new,
            self.app_event_tx.clone(),
        );
        self.active_view = Some(Box::new(view));
        self.status_view_active = false;
        self.request_redraw();
    }

    /// Height (terminal rows) required by the current bottom pane.
    pub(crate) fn request_redraw(&self) {
        self.app_event_tx.send(AppEvent::RequestRedraw)
    }

    // Immediate redraw path removed; all UI updates flow through the
    // debounced RequestRedraw/App::Redraw scheduler to reduce thrash.

    pub(crate) fn flash_footer_notice(&mut self, text: String) {
        self.composer.flash_footer_notice(text);
        // Ask app to schedule a redraw shortly to clear the notice automatically
        self.app_event_tx
            .send(AppEvent::ScheduleFrameIn(std::time::Duration::from_millis(
                2100,
            )));
        self.request_redraw();
    }

    /// Control footer hint visibility: whether to show Ctrl+R (reasoning) and Ctrl+D (diffs)
    #[allow(dead_code)]
    pub(crate) fn set_footer_hints(&mut self, show_reasoning: bool, show_diffs: bool) {
        self.composer.set_show_reasoning_hint(show_reasoning);
        self.composer.set_show_diffs_hint(show_diffs);
        self.request_redraw();
    }

    /// Convenience setters for individual hints
    pub(crate) fn set_reasoning_hint(&mut self, show: bool) {
        self.composer.set_show_reasoning_hint(show);
        self.request_redraw();
    }

    pub(crate) fn set_reasoning_state(&mut self, shown: bool) {
        self.composer.set_reasoning_state(shown);
        self.request_redraw();
    }

    pub(crate) fn set_diffs_hint(&mut self, show: bool) {
        self.composer.set_show_diffs_hint(show);
        self.request_redraw();
    }

    pub(crate) fn request_redraw_in(&self, dur: Duration) {
        self.app_event_tx.send(AppEvent::ScheduleFrameIn(dur));
    }

    // --- History helpers ---

    pub(crate) fn set_history_metadata(&mut self, log_id: u64, entry_count: usize) {
        self.composer.set_history_metadata(log_id, entry_count);
    }

    #[allow(dead_code)]
    pub(crate) fn flush_paste_burst_if_due(&mut self) -> bool {
        self.composer.flush_paste_burst_if_due()
    }

    #[allow(dead_code)]
    pub(crate) fn is_in_paste_burst(&self) -> bool {
        self.composer.is_in_paste_burst()
    }

    pub(crate) fn set_input_focus(&mut self, has_focus: bool) {
        self.has_input_focus = has_focus;
        self.composer.set_has_focus(has_focus);
        self.composer
            .set_ctrl_c_quit_hint(self.ctrl_c_quit_hint, self.has_input_focus);
    }

    pub(crate) fn on_history_entry_response(
        &mut self,
        log_id: u64,
        offset: usize,
        entry: Option<String>,
    ) {
        let updated = self
            .composer
            .on_history_entry_response(log_id, offset, entry);

        if updated {
            self.request_redraw();
        }
    }

    pub(crate) fn on_file_search_result(&mut self, query: String, matches: Vec<FileMatch>) {
        self.composer.on_file_search_result(query, matches);
        self.request_redraw();
    }

    pub(crate) fn clear_live_ring(&mut self) {
        self.live_ring = None;
    }

    // test helper removed

    /// Ensure input focus is maintained, especially after redraws or content updates
    pub(crate) fn ensure_input_focus(&mut self) {
        // Only ensure focus if there's no active modal view
        if self.active_view.is_none() {
            if !self.has_input_focus {
                self.set_input_focus(true);
            } else {
                self.composer
                    .set_ctrl_c_quit_hint(self.ctrl_c_quit_hint, self.has_input_focus);
            }
        }
    }

    pub(crate) fn set_access_mode_label(&mut self, label: Option<String>) {
        self.composer.set_access_mode_label(label);
        // Hide the "(Shift+Tab change)" suffix after a short time for persistent modes.
        // Avoid using a global frame scheduler which can be coalesced; instead spawn
        // a tiny timer to request a redraw slightly after expiry.
        let dur = Duration::from_secs(4);
        self.composer.set_access_mode_hint_for(dur);
        let tx = self.app_event_tx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(dur + Duration::from_millis(120));
            tx.send(AppEvent::RequestRedraw);
        });
        self.request_redraw();
    }

    pub(crate) fn set_access_mode_label_ephemeral(&mut self, label: String, dur: Duration) {
        self.composer.set_access_mode_label_ephemeral(label, dur);
        // Schedule a redraw after expiry without blocking other scheduled frames.
        let tx = self.app_event_tx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(dur + Duration::from_millis(120));
            tx.send(AppEvent::RequestRedraw);
        });
        self.request_redraw();
    }

    // Removed restart_live_status_with_text – no longer used by the current streaming UI.
}

impl WidgetRef for &BottomPane<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        // Base clear: fill the entire bottom pane with the theme background so
        // newly exposed rows (e.g., when the composer grows on paste) do not
        // show stale pixels from history.
        let base_style = ratatui::style::Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text());
        fill_rect(buf, area, Some(' '), base_style);

        let mut y_offset = 0u16;
        if let Some(ring) = &self.live_ring {
            let live_h = ring.desired_height(area.width).min(area.height);
            if live_h > 0 {
                let live_rect = Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: live_h,
                };
                ring.render_ref(live_rect, buf);
                y_offset = live_h;
            }
        }
        // Spacer between live ring and status view when active
        if self.live_ring.is_some() && self.status_view_active && y_offset < area.height {
            // Leave one empty line
            y_offset = y_offset.saturating_add(1);
        }

        // When a modal view is active and not yet complete, it owns the whole content area.
        if let Some(view) = &self.active_view {
            if view.is_complete() {
                // Modal finished—render composer instead on this frame.
                // We intentionally avoid mutating state here; key handling will
                // clear the view on the next interaction. This keeps render pure.
            } else if y_offset < area.height {
                if y_offset < area.height {
                    // Reserve bottom padding lines; keep at least 1 line for the view.
                    let avail = area.height - y_offset;
                    let pad = if self.active_view.is_some() {
                        0
                    } else {
                        BottomPane::BOTTOM_PAD_LINES.min(avail.saturating_sub(1))
                    };
                    // Add horizontal padding (2 chars on each side) for views
                    let horizontal_padding = 1u16;
                    let view_rect = Rect {
                        x: area.x + horizontal_padding,
                        y: area.y + y_offset,
                        width: area.width.saturating_sub(horizontal_padding * 2),
                        height: avail - pad,
                    };
                    // Ensure view background is painted under its content
                    let view_bg = ratatui::style::Style::default().bg(crate::colors::background());
                    fill_rect(buf, view_rect, None, view_bg);
                    view.render(view_rect, buf);
                }
                return;
            }
        } else if y_offset < area.height {
            // Optionally add an empty line above the input box
            if self.top_spacer_enabled {
                y_offset = y_offset.saturating_add(1);
            }

            // Add horizontal padding (2 chars on each side) for Message input
            let horizontal_padding = 1u16;
            let composer_rect = Rect {
                x: area.x + horizontal_padding,
                y: area.y + y_offset,
                width: area.width.saturating_sub(horizontal_padding * 2),
                // Reserve bottom padding
                height: (area.height - y_offset)
                    - BottomPane::BOTTOM_PAD_LINES.min((area.height - y_offset).saturating_sub(1)),
            };
            // Paint the composer area background before rendering widgets
            let comp_bg = ratatui::style::Style::default().bg(crate::colors::background());
            fill_rect(buf, composer_rect, None, comp_bg);
            (&self.composer).render_ref(composer_rect, buf);
        }
    }
}

#[cfg(all(test, feature = "legacy_tests"))]
mod tests_removed {
    use super::*;
    use crate::app_event::AppEvent;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use std::sync::mpsc::channel;

    fn exec_request() -> ApprovalRequest {
        ApprovalRequest::Exec {
            id: "1".to_string(),
            command: vec!["echo".into(), "ok".into()],
            reason: None,
        }
    }

    #[test]
    fn ctrl_c_on_modal_consumes_and_shows_quit_hint() {
        let (tx_raw, _rx) = channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            has_input_focus: true,
            enhanced_keys_supported: false,
            using_chatgpt_auth: false,
        });
        pane.push_approval_request(exec_request());
        assert_eq!(CancellationEvent::Handled, pane.on_ctrl_c());
        assert!(pane.ctrl_c_quit_hint_visible());
        assert_eq!(CancellationEvent::Ignored, pane.on_ctrl_c());
    }

    #[test]
    fn live_ring_renders_above_composer() {
        let (tx_raw, _rx) = channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            has_input_focus: true,
            enhanced_keys_supported: false,
            using_chatgpt_auth: false,
        });

        // Provide 4 rows with max_rows=3; only the last 3 should be visible.
        pane.set_live_ring_rows(
            3,
            vec![
                Line::from("one".to_string()),
                Line::from("two".to_string()),
                Line::from("three".to_string()),
                Line::from("four".to_string()),
            ],
        );

        let area = Rect::new(0, 0, 10, 5);
        let mut buf = Buffer::empty(area);
        (&pane).render_ref(area, &mut buf);

        // Extract the first 3 rows and assert they contain the last three lines.
        let mut lines: Vec<String> = Vec::new();
        for y in 0..3 {
            let mut s = String::new();
            for x in 0..area.width {
                s.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            lines.push(s.trim_end().to_string());
        }
        assert_eq!(lines, vec!["two", "three", "four"]);
    }

    #[test]
    fn status_indicator_visible_with_live_ring() {
        let (tx_raw, _rx) = channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            has_input_focus: true,
            enhanced_keys_supported: false,
            using_chatgpt_auth: false,
        });

        // Simulate task running which replaces composer with the status indicator.
        pane.set_task_running(true);
        pane.update_status_text("waiting for model".to_string());

        // Provide 2 rows in the live ring (e.g., streaming CoT) and ensure the
        // status indicator remains visible below them.
        pane.set_live_ring_rows(
            2,
            vec![
                Line::from("cot1".to_string()),
                Line::from("cot2".to_string()),
            ],
        );

        // Allow some frames so the dot animation is present.
        std::thread::sleep(std::time::Duration::from_millis(120));

        // Height should include both ring rows, 1 spacer, and the 1-line status.
        let area = Rect::new(0, 0, 30, 4);
        let mut buf = Buffer::empty(area);
        (&pane).render_ref(area, &mut buf);

        // Top two rows are the live ring.
        let mut r0 = String::new();
        let mut r1 = String::new();
        for x in 0..area.width {
            r0.push(buf[(x, 0)].symbol().chars().next().unwrap_or(' '));
            r1.push(buf[(x, 1)].symbol().chars().next().unwrap_or(' '));
        }
        assert!(r0.contains("cot1"), "expected first live row: {r0:?}");
        assert!(r1.contains("cot2"), "expected second live row: {r1:?}");

        // Row 2 is the spacer (blank)
        let mut r2 = String::new();
        for x in 0..area.width {
            r2.push(buf[(x, 2)].symbol().chars().next().unwrap_or(' '));
        }
        assert!(r2.trim().is_empty(), "expected blank spacer line: {r2:?}");

        // Bottom row is the status line; it should contain the "Coding" header.
        let mut r3 = String::new();
        for x in 0..area.width {
            r3.push(buf[(x, 3)].symbol().chars().next().unwrap_or(' '));
        }
        assert!(
            r3.contains("Coding"),
            "expected Coding header in status line: {r3:?}"
        );
    }
    // live ring removed; related tests deleted.

    #[test]
    fn overlay_not_shown_above_approval_modal() {
        let (tx_raw, _rx) = channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            has_input_focus: true,
            enhanced_keys_supported: false,
            using_chatgpt_auth: false,
        });

        // Create an approval modal (active view).
        pane.push_approval_request(exec_request());
        // Attempt to update status; this should NOT create an overlay while modal is visible.
        pane.update_status_text("running command".to_string());

        // Render and verify the top row does not include the Coding header overlay.
        let area = Rect::new(0, 0, 60, 6);
        let mut buf = Buffer::empty(area);
        (&pane).render_ref(area, &mut buf);

        let mut r0 = String::new();
        for x in 0..area.width {
            r0.push(buf[(x, 0)].symbol().chars().next().unwrap_or(' '));
        }
        assert!(
            !r0.contains("Coding"),
            "overlay Coding header should not render above modal"
        );
    }

    #[test]
    fn composer_not_shown_after_denied_if_task_running() {
        let (tx_raw, rx) = channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx.clone(),
            has_input_focus: true,
            enhanced_keys_supported: false,
        });

        // Start a running task so the status indicator is active above the composer.
        pane.set_task_running(true);
        pane.update_status_text("waiting for model".to_string());

        // Push an approval modal (e.g., command approval) which should hide the status view.
        pane.push_approval_request(exec_request());

        // Simulate pressing 'n' (No) on the modal.
        use crossterm::event::KeyCode;
        use crossterm::event::KeyEvent;
        use crossterm::event::KeyModifiers;
        pane.handle_key_event(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));

        // After denial, since the task is still running, the status indicator should be
        // visible above the composer. The modal should be gone.
        assert!(
            pane.active_view.is_none(),
            "no active modal view after denial"
        );

        // Render and ensure the top row includes the Coding header instead of the composer.
        // Give the animation thread a moment to tick.
        std::thread::sleep(std::time::Duration::from_millis(120));
        let area = Rect::new(0, 0, 40, 3);
        let mut buf = Buffer::empty(area);
        (&pane).render_ref(area, &mut buf);
        let mut row0 = String::new();
        for x in 0..area.width {
            row0.push(buf[(x, 0)].symbol().chars().next().unwrap_or(' '));
        }
        assert!(
            row0.contains("Coding"),
            "expected Coding header after denial: {row0:?}"
        );

        // Composer placeholder should be visible somewhere below.
        let mut found_composer = false;
        for y in 1..area.height.saturating_sub(2) {
            let mut row = String::new();
            for x in 0..area.width {
                row.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
            }
            if row.contains("Ask Code") {
                found_composer = true;
                break;
            }
        }
        assert!(
            found_composer,
            "expected composer visible under status line"
        );

        // Drain the channel to avoid unused warnings.
        drop(rx);
    }

    #[test]
    fn status_indicator_visible_during_command_execution() {
        let (tx_raw, _rx) = channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            has_input_focus: true,
            enhanced_keys_supported: false,
            using_chatgpt_auth: false,
        });

        // Begin a task: show initial status.
        pane.set_task_running(true);
        pane.update_status_text("waiting for model".to_string());

        // As a long-running command begins (post-approval), ensure the status
        // indicator is visible while we wait for the command to run.
        pane.update_status_text("running command".to_string());

        // Allow some frames so the animation thread ticks.
        std::thread::sleep(std::time::Duration::from_millis(120));

        // Render and confirm the line contains the "Coding" header.
        let area = Rect::new(0, 0, 40, 3);
        let mut buf = Buffer::empty(area);
        (&pane).render_ref(area, &mut buf);

        let mut row0 = String::new();
        for x in 0..area.width {
            row0.push(buf[(x, 0)].symbol().chars().next().unwrap_or(' '));
        }
        assert!(row0.contains("Coding"), "expected Coding header: {row0:?}");
    }

    #[test]
    fn bottom_padding_present_for_status_view() {
        let (tx_raw, _rx) = channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            has_input_focus: true,
            enhanced_keys_supported: false,
            using_chatgpt_auth: false,
        });

        // Activate spinner (status view replaces composer) with no live ring.
        pane.set_task_running(true);
        pane.update_status_text("waiting for model".to_string());

        // Use height == desired_height; expect 1 status row at top and 2 bottom padding rows.
        let height = pane.desired_height(30);
        assert!(
            height >= 3,
            "expected at least 3 rows with bottom padding; got {height}"
        );
        let area = Rect::new(0, 0, 30, height);
        let mut buf = Buffer::empty(area);
        (&pane).render_ref(area, &mut buf);

        // Top row contains the status header
        let mut top = String::new();
        for x in 0..area.width {
            top.push(buf[(x, 0)].symbol().chars().next().unwrap_or(' '));
        }
        assert!(
            top.contains("Coding"),
            "expected Coding header on top row: {top:?}"
        );

        // Last row should be blank padding; the row above should generally contain composer content.
        let mut r_last = String::new();
        for x in 0..area.width {
            r_last.push(buf[(x, height - 1)].symbol().chars().next().unwrap_or(' '));
        }
        assert!(
            r_last.trim().is_empty(),
            "expected last row blank: {r_last:?}"
        );
    }

    #[test]
    fn bottom_padding_shrinks_when_tiny() {
        let (tx_raw, _rx) = channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let mut pane = BottomPane::new(BottomPaneParams {
            app_event_tx: tx,
            has_input_focus: true,
            enhanced_keys_supported: false,
            using_chatgpt_auth: false,
        });

        pane.set_task_running(true);
        pane.update_status_text("waiting for model".to_string());

        // Height=2 → pad shrinks to 1; bottom row is blank, top row has spinner.
        let area2 = Rect::new(0, 0, 20, 2);
        let mut buf2 = Buffer::empty(area2);
        (&pane).render_ref(area2, &mut buf2);
        let mut row0 = String::new();
        let mut row1 = String::new();
        for x in 0..area2.width {
            row0.push(buf2[(x, 0)].symbol().chars().next().unwrap_or(' '));
            row1.push(buf2[(x, 1)].symbol().chars().next().unwrap_or(' '));
        }
        assert!(
            row0.contains("Coding"),
            "expected Coding header on row 0: {row0:?}"
        );
        assert!(
            row1.trim().is_empty(),
            "expected bottom padding on row 1: {row1:?}"
        );

        // Height=1 → no padding; single row is the composer (status hidden).
        let area1 = Rect::new(0, 0, 20, 1);
        let mut buf1 = Buffer::empty(area1);
        (&pane).render_ref(area1, &mut buf1);
        let mut only = String::new();
        for x in 0..area1.width {
            only.push(buf1[(x, 0)].symbol().chars().next().unwrap_or(' '));
        }
        assert!(
            only.contains("Coding"),
            "expected Coding header with no padding: {only:?}"
        );
    }
}
