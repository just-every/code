//! Bottom pane: shows the ChatComposer or a BottomPaneView, if one is active.

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::auto_drive_style::AutoDriveVariant;
use crate::bottom_pane::chat_composer::ComposerRenderMode;
use crate::chatwidget::BackgroundOrderTicket;
use crate::user_approval_widget::{ApprovalRequest, UserApprovalWidget};
use crate::thread_spawner;
pub(crate) use bottom_pane_view::BottomPaneView;
pub(crate) use bottom_pane_view::ConditionalUpdate;
use crate::util::buffer::fill_rect;
use code_protocol::custom_prompts::CustomPrompt;
use code_protocol::skills::Skill;
use code_core::protocol::TokenUsage;
use code_file_search::FileMatch;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;
use std::time::Duration;

mod approval_modal_view;
#[cfg(feature = "code-fork")]
mod approval_ui;
mod auto_coordinator_view;
mod auto_drive_settings_view;
mod account_switch_settings_view;
mod bottom_pane_view;
mod chat_composer;
mod chat_composer_history;
mod diff_popup;
mod custom_prompt_view;
pub(crate) mod prompt_args;
mod command_popup;
mod file_search_popup;
mod paste_burst;
mod popup_consts;
pub(crate) mod agent_editor_view;
pub(crate) mod model_selection_view;
mod scroll_state;
mod selection_popup_common;
pub mod list_selection_view;
pub(crate) use list_selection_view::SelectionAction;
pub(crate) use custom_prompt_view::CustomPromptView;
mod cloud_tasks_view;
pub(crate) use cloud_tasks_view::CloudTasksView;
pub mod resume_selection_view;
pub mod agents_settings_view;
pub mod mcp_settings_view;
mod login_accounts_view;
// no direct use of list_selection_view or its items here
mod textarea;
pub mod form_text_field;
pub mod prompts_settings_view;
pub mod skills_settings_view;
mod theme_selection_view;
mod planning_settings_view;
mod verbosity_selection_view;
pub(crate) mod validation_settings_view;
mod update_settings_view;
mod undo_timeline_view;
mod notifications_settings_view;
mod settings_overlay;
mod request_user_input_view;
pub(crate) use settings_overlay::SettingsSection;
pub(crate) mod review_settings_view;
pub mod settings_panel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancellationEvent {
    Ignored,
    Handled,
}

pub(crate) use chat_composer::{AgentHintLabel, AutoReviewFooterStatus, AutoReviewPhase, ChatComposer};
pub(crate) use chat_composer::InputResult;
pub(crate) use auto_coordinator_view::{
    AutoActiveViewModel,
    AutoCoordinatorButton,
    AutoCoordinatorView,
    AutoCoordinatorViewModel,
    CountdownState,
};
pub(crate) use auto_drive_settings_view::AutoDriveSettingsView;
pub(crate) use account_switch_settings_view::AccountSwitchSettingsView;
pub(crate) use login_accounts_view::{
    LoginAccountsState,
    LoginAccountsView,
    LoginAddAccountState,
    LoginAddAccountView,
};

pub(crate) use update_settings_view::{UpdateSettingsView, UpdateSharedState};
pub(crate) use notifications_settings_view::{NotificationsMode, NotificationsSettingsView};
pub(crate) use validation_settings_view::ValidationSettingsView;
pub(crate) use review_settings_view::ReviewSettingsView;
pub(crate) use planning_settings_view::PlanningSettingsView;
use approval_modal_view::ApprovalModalView;
#[cfg(feature = "code-fork")]
use approval_ui::ApprovalUi;
use code_common::model_presets::ModelPreset;
use code_core::config_types::ReasoningEffort;
use code_core::config_types::TextVerbosity;
use code_core::config_types::ThemeName;
pub(crate) use model_selection_view::{ModelSelectionTarget, ModelSelectionView};
pub(crate) use mcp_settings_view::McpSettingsView;
pub(crate) use theme_selection_view::ThemeSelectionView;
use verbosity_selection_view::VerbositySelectionView;
pub(crate) use undo_timeline_view::{UndoTimelineEntry, UndoTimelineEntryKind, UndoTimelineView};
pub(crate) use request_user_input_view::RequestUserInputView;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveViewKind {
    None,
    AutoCoordinator,
    Other,
}

/// Pane displayed in the lower half of the chat UI.
pub(crate) struct BottomPane<'a> {
    /// Composer is retained even when a BottomPaneView is displayed so the
    /// input state is retained when the view is closed.
    composer: ChatComposer,

    /// If present, this is displayed instead of the `composer`.
    active_view: Option<Box<dyn BottomPaneView<'a> + 'a>>,
    active_view_kind: ActiveViewKind,

    app_event_tx: AppEventSender,
    has_input_focus: bool,
    is_task_running: bool,
    ctrl_c_quit_hint: bool,

    /// True if the active view is the StatusIndicatorView that replaces the
    /// composer during a running task.
    status_view_active: bool,

    /// Whether to reserve an empty spacer line above the input composer.
    /// Defaults to true for visual breathing room, but can be disabled when
    /// the chat history is scrolled up to allow history to reclaim that row.
    top_spacer_enabled: bool,

    pub(crate) using_chatgpt_auth: bool,

    auto_drive_variant: AutoDriveVariant,
    auto_drive_active: bool,

    custom_prompts: Vec<CustomPrompt>,
    skills: Vec<Skill>,

}

pub(crate) struct BottomPaneParams {
    pub(crate) app_event_tx: AppEventSender,
    pub(crate) has_input_focus: bool,
    pub(crate) enhanced_keys_supported: bool,
    pub(crate) using_chatgpt_auth: bool,
    pub(crate) auto_drive_variant: AutoDriveVariant,
}

impl BottomPane<'_> {
    // Reduce bottom padding so footer sits one line lower
    const BOTTOM_PAD_LINES: u16 = 1;
    pub fn new(params: BottomPaneParams) -> Self {
        let enhanced_keys_supported = params.enhanced_keys_supported;
        let composer = ChatComposer::new(
            params.has_input_focus,
            params.app_event_tx.clone(),
            enhanced_keys_supported,
            params.using_chatgpt_auth,
        );

        Self {
            composer,
            active_view: None,
            active_view_kind: ActiveViewKind::None,
            app_event_tx: params.app_event_tx,
            has_input_focus: params.has_input_focus,
            is_task_running: false,
            ctrl_c_quit_hint: false,
            status_view_active: false,
            top_spacer_enabled: true,
            using_chatgpt_auth: params.using_chatgpt_auth,
            auto_drive_variant: params.auto_drive_variant,
            auto_drive_active: false,
            custom_prompts: Vec::new(),
            skills: Vec::new(),
        }
    }

    fn auto_view_mut(&mut self) -> Option<&mut AutoCoordinatorView> {
        if self.active_view_kind != ActiveViewKind::AutoCoordinator {
            return None;
        }
        self.active_view
            .as_mut()
            .and_then(|view| view.as_any_mut())
            .and_then(|any| any.downcast_mut::<AutoCoordinatorView>())
    }

    #[cfg(test)]
    pub(crate) fn auto_view_model(&self) -> Option<AutoCoordinatorViewModel> {
        if self.active_view_kind != ActiveViewKind::AutoCoordinator {
            return None;
        }

        self.active_view
            .as_ref()
            .and_then(|view| view.as_any())
            .and_then(|any| any.downcast_ref::<AutoCoordinatorView>())
            .map(|view| view.model().clone())
    }

    fn apply_auto_drive_style(&mut self) {
        if !self.auto_drive_active {
            self.composer.set_auto_drive_style(None);
            return;
        }

        let style = self.auto_drive_variant.style();
        self.composer.set_auto_drive_active(true);
        self.composer
            .set_auto_drive_style(Some(style.composer.clone()));
        if let Some(view) = self.auto_view_mut() {
            view.set_style(style.clone());
        }

        self.request_redraw();
    }

    fn enable_auto_drive_style(&mut self) {
        if !self.auto_drive_active {
            self.auto_drive_active = true;
            self.composer.set_auto_drive_active(true);
        }
        self.apply_auto_drive_style();
    }

    fn disable_auto_drive_style(&mut self) {
        if !self.auto_drive_active {
            return;
        }
        self.auto_drive_active = false;
        self.composer.set_auto_drive_active(false);
        self.composer.set_auto_drive_style(None);
        let style = self.auto_drive_variant.style();
        if let Some(view) = self.auto_view_mut() {
            view.set_style(style);
        }
        self.request_redraw();
    }

    pub(crate) fn set_auto_drive_variant(&mut self, variant: AutoDriveVariant) {
        if self.auto_drive_variant == variant {
            return;
        }
        self.auto_drive_variant = variant;
        if self.auto_drive_active {
            self.apply_auto_drive_style();
        }
    }

    #[allow(dead_code)]
    pub fn show_notifications_settings(&mut self, view: NotificationsSettingsView) {
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::Other;
        self.status_view_active = false;
        self.request_redraw();
    }

    pub fn show_login_accounts(&mut self, view: LoginAccountsView) {
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::Other;
        self.status_view_active = false;
        self.request_redraw();
    }

    pub fn show_login_add_account(&mut self, view: LoginAddAccountView) {
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::Other;
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

    #[allow(dead_code)]
    pub(crate) fn has_active_view(&self) -> bool {
        self.active_view.is_some()
    }

    pub fn set_has_chat_history(&mut self, has_history: bool) {
        self.composer.set_has_chat_history(has_history);
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        let (view_height, pad_lines) = if let Some(view) = self.active_view.as_ref() {
            let is_auto = matches!(self.active_view_kind, ActiveViewKind::AutoCoordinator);
            let top_spacer = if is_auto {
                0
            } else if self.top_spacer_enabled {
                1
            } else {
                0
            };
            let composer_height = if is_auto {
                let composer_visible = view
                    .as_ref()
                    .as_any()
                    .and_then(|any| any.downcast_ref::<AutoCoordinatorView>())
                    .map(|auto_view| auto_view.composer_visible())
                    .unwrap_or(true);
                if composer_visible {
                    self.composer.desired_height(width)
                } else {
                    self.composer.footer_height()
                }
            } else {
                0
            };
            let pad = if is_auto {
                BottomPane::BOTTOM_PAD_LINES
            } else {
                0
            };
            let base_height = view
                .desired_height(width)
                .saturating_add(top_spacer)
                .saturating_add(composer_height);

            (base_height, pad)
        } else {
            // Optionally add 1 for the empty line above the composer
            let spacer = if self.top_spacer_enabled { 1 } else { 0 };
            (spacer + self.composer.desired_height(width), Self::BOTTOM_PAD_LINES)
        };

        view_height.saturating_add(pad_lines)
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
                height: (area.height.saturating_sub(y_offset)).saturating_sub(
                    BottomPane::BOTTOM_PAD_LINES
                        .min((area.height.saturating_sub(y_offset)).saturating_sub(1)),
                ),
            };
            self.composer.cursor_pos(composer_rect)
        }
    }

    /// Forward a key event to the active view or the composer.
    pub fn handle_key_event(&mut self, key_event: KeyEvent) -> InputResult {
        if let Some(mut view) = self.active_view.take() {
            let kind = self.active_view_kind;
            if matches!(kind, ActiveViewKind::AutoCoordinator) {
                let consumed = if let Some(auto_view) = view
                    .as_any_mut()
                    .and_then(|any| any.downcast_mut::<AutoCoordinatorView>())
                {
                    auto_view.handle_active_key_event(self, key_event)
                } else {
                    view.handle_key_event(self, key_event);
                    true
                };

                if !view.is_complete() {
                    self.active_view = Some(view);
                    self.active_view_kind = kind;
                } else {
                    self.active_view_kind = ActiveViewKind::None;
                    self.set_standard_terminal_hint(None);
                }

                if consumed {
                    self.request_redraw();
                    if matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                        match key_event.code {
                            KeyCode::Up => return InputResult::ScrollUp,
                            KeyCode::Down => return InputResult::ScrollDown,
                            _ => {}
                        }
                    }
                    return InputResult::None;
                }

                return self.handle_composer_key_event(key_event);
            }

            view.handle_key_event(self, key_event);
            if !view.is_complete() {
                self.active_view = Some(view);
                self.active_view_kind = kind;
            } else {
                self.active_view_kind = ActiveViewKind::None;
                self.set_standard_terminal_hint(None);
            }
            // Don't create a status view - keep composer visible
            // Debounce view navigation redraws to reduce render thrash
            self.request_redraw();

            InputResult::None
        } else {
            self.handle_composer_key_event(key_event)
        }
    }

    fn handle_composer_key_event(&mut self, key_event: KeyEvent) -> InputResult {
        let (input_result, needs_redraw) = self.composer.handle_key_event(key_event);
        if needs_redraw {
            // Route input updates through the app's debounced redraw path so typing
            // doesn't attempt a full-screen redraw per key.
            self.request_redraw();
        }
        if self.composer.is_in_paste_burst() {
            self.request_redraw_in(ChatComposer::recommended_paste_flush_delay());
        }
        input_result
    }

    /// Attempt to navigate history upwards from the composer. Returns true if consumed.
    pub(crate) fn try_history_up(&mut self) -> bool {
        let consumed = self.composer.try_history_up();
        if consumed { self.request_redraw(); }
        consumed
    }

    /// Attempt to navigate history downwards from the composer. Returns true if consumed.
    pub(crate) fn try_history_down(&mut self) -> bool {
        let consumed = self.composer.try_history_down();
        if consumed { self.request_redraw(); }
        consumed
    }

    /// Returns true if the composer is currently browsing history.
    pub(crate) fn history_is_browsing(&self) -> bool { self.composer.history_is_browsing() }

    /// After a chat scroll-up, make the next Down key scroll chat instead of moving within input.
    pub(crate) fn mark_next_down_scrolls_history(&mut self) { self.composer.mark_next_down_scrolls_history(); }

    /// Handle Ctrl-C in the bottom pane. If a modal view is active it gets a
    /// chance to consume the event (e.g. to dismiss itself).
    pub(crate) fn on_ctrl_c(&mut self) -> CancellationEvent {
        let kind = self.active_view_kind;
        let mut view = match self.active_view.take() {
            Some(view) => view,
            None => return CancellationEvent::Ignored,
        };

        let event = view.on_ctrl_c(self);
        match event {
            CancellationEvent::Handled => {
                if !view.is_complete() {
                    self.active_view = Some(view);
                    self.active_view_kind = kind;
                } else {
                    self.active_view_kind = ActiveViewKind::None;
                    self.set_standard_terminal_hint(None);
                }
                // Don't create a status view - keep composer visible
                self.show_ctrl_c_quit_hint();
            }
            CancellationEvent::Ignored => {
                self.active_view = Some(view);
                self.active_view_kind = kind;
            }
        }
        event
    }

    pub fn handle_paste(&mut self, pasted: String) {
        if let Some(mut view) = self.active_view.take() {
            use crate::bottom_pane::bottom_pane_view::ConditionalUpdate;
            let kind = self.active_view_kind;
            let update = view.handle_paste_with_composer(&mut self.composer, pasted);
            if !view.is_complete() {
                self.active_view = Some(view);
                self.active_view_kind = kind;
            } else {
                self.active_view_kind = ActiveViewKind::None;
                self.set_standard_terminal_hint(None);
            }
            if matches!(update, ConditionalUpdate::NeedsRedraw) {
                self.request_redraw();
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
        if closed { self.request_redraw(); }
        closed
    }

    pub(crate) fn file_popup_visible(&self) -> bool {
        self.composer.file_popup_visible()
    }

    /// True if a modal/overlay view is currently displayed (not the composer popup).
    pub(crate) fn has_active_modal_view(&self) -> bool {
        // Consider a modal inactive once it has completed to avoid blocking
        // Esc routing and other overlay checks after a decision is made.
        match self.active_view.as_ref() {
            Some(_) if matches!(self.active_view_kind, ActiveViewKind::AutoCoordinator) => false,
            Some(view) => !view.is_complete(),
            None => false,
        }
    }

    pub(crate) fn auto_drive_view_active(&self) -> bool {
        matches!(self.active_view_kind, ActiveViewKind::AutoCoordinator)
    }

    pub(crate) fn auto_drive_style_active(&self) -> bool {
        self.auto_drive_active
    }

    pub(crate) fn set_custom_prompts(&mut self, prompts: Vec<CustomPrompt>) {
        self.custom_prompts = prompts.clone();
        self.composer.set_custom_prompts(prompts);
    }

    pub(crate) fn custom_prompts(&self) -> &[CustomPrompt] {
        &self.custom_prompts
    }

    pub(crate) fn set_skills(&mut self, skills: Vec<Skill>) {
        self.skills = skills;
    }

    pub(crate) fn skills(&self) -> &[Skill] {
        &self.skills
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
        if let Some(view) = self.active_view.as_mut() {
            let _ = view.update_status_text(text.clone());
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

    pub(crate) fn standard_terminal_hint(&self) -> Option<&str> {
        self.composer.standard_terminal_hint()
    }

    pub(crate) fn set_auto_review_status(&mut self, status: Option<AutoReviewFooterStatus>) {
        self.composer.set_auto_review_status(status);
        self.request_redraw();
    }

    pub(crate) fn set_agent_hint_label(&mut self, label: AgentHintLabel) {
        self.composer.set_agent_hint_label(label);
        self.request_redraw();
    }

    #[cfg(test)]
    pub(crate) fn auto_review_status(&self) -> Option<AutoReviewFooterStatus> {
        self.composer.auto_review_status()
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
                let kind = self.active_view_kind;
                if !view.should_hide_when_task_is_done() {
                    self.active_view = Some(view);
                    self.active_view_kind = kind;
                } else {
                    self.active_view_kind = ActiveViewKind::None;
                    self.set_standard_terminal_hint(None);
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
    pub fn push_approval_request(
        &mut self,
        request: ApprovalRequest,
        ticket: BackgroundOrderTicket,
    ) {
        let (request, ticket) = if let Some(view) = self.active_view.as_mut() {
            match view.try_consume_approval_request(request, ticket.clone()) {
                Some((request, ticket)) => (request, ticket),
                None => {
                    self.request_redraw();
                    return;
                }
            }
        } else {
            (request, ticket)
        };

        // Otherwise create a new approval modal overlay.
        let modal = ApprovalModalView::new(request, ticket, self.app_event_tx.clone());
        self.active_view = Some(Box::new(modal));
        self.active_view_kind = ActiveViewKind::Other;
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
        use_chat_model: bool,
        target: ModelSelectionTarget,
    ) {
        let view = ModelSelectionView::new(
            presets,
            current_model,
            current_effort,
            use_chat_model,
            target,
            self.app_event_tx.clone(),
        );
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::Other;
        // Status shown in composer title now
        self.status_view_active = false;
        self.request_redraw()
    }

    #[allow(dead_code)]
    /// Show the theme selection UI
    pub fn show_theme_selection(
        &mut self,
        _current_theme: ThemeName,
        tail_ticket: BackgroundOrderTicket,
        before_ticket: BackgroundOrderTicket,
    ) {
        let view = ThemeSelectionView::new(
            crate::theme::current_theme_name(),
            self.app_event_tx.clone(),
            tail_ticket,
            before_ticket,
        );
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::Other;
        // Status shown in composer title now
        self.status_view_active = false;
        self.request_redraw()
    }

    /// Show the diffs popup with tabs for each file.
    #[allow(dead_code)]
    pub fn show_diff_popup(&mut self, tabs: Vec<(String, Vec<ratatui::text::Line<'static>>)>) {
        let view = diff_popup::DiffPopupView::new(tabs);
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::Other;
        self.status_view_active = false;
        self.request_redraw()
    }

    /// Show the verbosity selection UI
    pub fn show_verbosity_selection(&mut self, current_verbosity: TextVerbosity) {
        let view = VerbositySelectionView::new(current_verbosity, self.app_event_tx.clone());
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::Other;
        // Status shown in composer title now
        self.status_view_active = false;
        self.request_redraw()
    }

    /// Show a multi-line prompt input view (used for custom review instructions)
    pub fn show_custom_prompt(&mut self, view: CustomPromptView) {
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::Other;
        self.status_view_active = false;
        self.request_redraw();
    }

    pub(crate) fn show_request_user_input(&mut self, view: RequestUserInputView) {
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::Other;
        self.status_view_active = false;
        self.request_redraw();
    }

    pub(crate) fn close_request_user_input_view(&mut self) {
        let should_close = self
            .active_view
            .as_ref()
            .and_then(|view| view.as_any())
            .is_some_and(|any| any.is::<RequestUserInputView>());
        if !should_close {
            return;
        }

        self.active_view = None;
        self.active_view_kind = ActiveViewKind::None;
        self.set_standard_terminal_hint(None);
        self.status_view_active = false;
    }

    /// Show a generic list selection popup with items and actions.
    pub fn show_list_selection(
        &mut self,
        _title: String,
        _subtitle: Option<String>,
        _footer_hint: Option<String>,
        items: crate::bottom_pane::list_selection_view::ListSelectionView,
    ) {
        self.active_view = Some(Box::new(items));
        self.active_view_kind = ActiveViewKind::Other;
        // Status shown in composer title now
        self.status_view_active = false;
        self.request_redraw();
    }

    pub fn show_cloud_tasks(&mut self, view: CloudTasksView) {
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::Other;
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
        let view = ResumeSelectionView::new(title, subtitle.unwrap_or_default(), rows, self.app_event_tx.clone());
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::Other;
        self.status_view_active = false;
        self.request_redraw()
    }

    pub fn show_undo_timeline_view(&mut self, view: UndoTimelineView) {
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::Other;
        self.status_view_active = false;
        self.request_redraw();
    }

    /// Show MCP servers status/toggle UI
    #[allow(dead_code)]
    pub fn show_mcp_settings(&mut self, rows: crate::bottom_pane::mcp_settings_view::McpServerRows) {
        use mcp_settings_view::McpSettingsView;
        let view = McpSettingsView::new(rows, self.app_event_tx.clone());
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::Other;
        self.status_view_active = false;
        self.request_redraw();
    }

    pub(crate) fn show_auto_coordinator_view(&mut self, model: AutoCoordinatorViewModel) {
        if let Some(existing) = self.active_view.as_mut() {
            if self.active_view_kind == ActiveViewKind::AutoCoordinator {
                if let Some(existing_any) = existing.as_any_mut() {
                    if let Some(auto_view) = existing_any.downcast_mut::<AutoCoordinatorView>() {
                        auto_view.update_model(model);
                        auto_view.set_style(self.auto_drive_variant.style());
                        let status_text = self
                            .composer
                            .status_message()
                            .map_or_else(String::new, str::to_string);
                        let _ = auto_view.update_status_text(status_text);
                        let mode = if auto_view.composer_visible() {
                            ComposerRenderMode::Full
                        } else {
                            ComposerRenderMode::FooterOnly
                        };
                        self.composer.set_render_mode(mode);
                        self.status_view_active = false;
                        self.composer.set_embedded_mode(false);
                        self.enable_auto_drive_style();
                        self.request_redraw();
                        return;
                    }
                }
            }
        }

        if self.active_view.is_some() && self.active_view_kind != ActiveViewKind::AutoCoordinator {
            self.composer.set_render_mode(ComposerRenderMode::Full);
            return;
        }

        let mut view = AutoCoordinatorView::new(
            model,
            self.app_event_tx.clone(),
            self.auto_drive_variant.style(),
        );
        let status_text = self
            .composer
            .status_message()
            .map_or_else(String::new, str::to_string);
        let _ = view.update_status_text(status_text);
        let mode = if view.composer_visible() {
            ComposerRenderMode::Full
        } else {
            ComposerRenderMode::FooterOnly
        };
        self.active_view = Some(Box::new(view));
        self.active_view_kind = ActiveViewKind::AutoCoordinator;
        self.status_view_active = false;
        self.composer.set_embedded_mode(false);
        self.composer.set_render_mode(mode);
        self.enable_auto_drive_style();
        self.request_redraw();
    }

    pub(crate) fn clear_auto_coordinator_view(&mut self, disable_style: bool) {
        if self.active_view_kind == ActiveViewKind::AutoCoordinator {
            self.active_view = None;
            self.active_view_kind = ActiveViewKind::None;
            self.set_standard_terminal_hint(None);
            self.status_view_active = false;
            self.composer.set_embedded_mode(false);
            self.composer.set_render_mode(ComposerRenderMode::Full);
            if disable_style {
                self.disable_auto_drive_style();
            } else if self.auto_drive_active {
                self.apply_auto_drive_style();
            }
            self.request_redraw();
            return;
        }

        if disable_style {
            self.disable_auto_drive_style();
        }
    }

    pub(crate) fn release_auto_drive_style(&mut self) {
        self.disable_auto_drive_style();
    }

    /// Height (terminal rows) required by the current bottom pane.
    pub(crate) fn request_redraw(&self) {
        self.app_event_tx.send(AppEvent::RequestRedraw)
    }

    pub(crate) fn update_model_selection_presets(&mut self, presets: Vec<ModelPreset>) {
        let Some(view) = self.active_view.as_mut() else {
            return;
        };
        let Some(model_view) = view
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<ModelSelectionView>())
        else {
            return;
        };

        model_view.update_presets(presets);
        self.request_redraw();
    }

    // Immediate redraw path removed; all UI updates flow through the
    // debounced RequestRedraw/App::Redraw scheduler to reduce thrash.

    pub(crate) fn flash_footer_notice(&mut self, text: String) {
        self.composer.flash_footer_notice(text);
        // Ask app to schedule a redraw shortly to clear the notice automatically
        self.app_event_tx
            .send(AppEvent::ScheduleFrameIn(std::time::Duration::from_millis(2100)));
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

    pub(crate) fn clear_live_ring(&mut self) {}
    
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
        let fallback_tx = self.app_event_tx.clone();
        if thread_spawner::spawn_lightweight("access-hint", move || {
            std::thread::sleep(dur + Duration::from_millis(120));
            tx.send(AppEvent::RequestRedraw);
        })
        .is_none()
        {
            fallback_tx.send(AppEvent::RequestRedraw);
        }
        self.request_redraw();
    }

    pub(crate) fn set_access_mode_label_ephemeral(&mut self, label: String, dur: Duration) {
        self.composer.set_access_mode_label_ephemeral(label, dur);
        // Schedule a redraw after expiry without blocking other scheduled frames.
        let tx = self.app_event_tx.clone();
        let fallback_tx = self.app_event_tx.clone();
        if thread_spawner::spawn_lightweight("access-hint-ephemeral", move || {
            std::thread::sleep(dur + Duration::from_millis(120));
            tx.send(AppEvent::RequestRedraw);
        })
        .is_none()
        {
            fallback_tx.send(AppEvent::RequestRedraw);
        }
        self.request_redraw();
    }

    #[allow(dead_code)]
    fn render_auto_coordinator_footer(&self, _area: Rect, _buf: &mut Buffer) {}

    // Removed restart_live_status_with_text – no longer used by the current streaming UI.
}

#[cfg(feature = "code-fork")]
fn build_user_approval_widget<'a>(
    request: ApprovalRequest,
    ticket: BackgroundOrderTicket,
    app_event_tx: AppEventSender,
) -> UserApprovalWidget<'a> {
    <UserApprovalWidget<'a> as ApprovalUi>::build(request, ticket, app_event_tx)
}

#[cfg(not(feature = "code-fork"))]
fn build_user_approval_widget<'a>(
    request: ApprovalRequest,
    ticket: BackgroundOrderTicket,
    app_event_tx: AppEventSender,
) -> UserApprovalWidget<'a> {
    UserApprovalWidget::new(request, ticket, app_event_tx)
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

        let mut composer_rect = compute_composer_rect(area, self.top_spacer_enabled);
        let mut composer_needs_render = true;
        let horizontal_padding = 1u16;

        if let Some(view) = &self.active_view {
            if !view.is_complete() {
                let is_auto = matches!(self.active_view_kind, ActiveViewKind::AutoCoordinator);
                if is_auto {
                    let content_width = area.width.saturating_sub(horizontal_padding * 2);
                    let composer_visible = view
                        .as_ref()
                        .as_any()
                        .and_then(|any| any.downcast_ref::<AutoCoordinatorView>())
                        .map(|auto_view| auto_view.composer_visible())
                        .unwrap_or(true);
                    let composer_height = if composer_visible {
                        self.composer.desired_height(area.width)
                    } else {
                        self.composer.footer_height()
                    };
                    let pad = BottomPane::BOTTOM_PAD_LINES;
                    let max_view_height = area
                        .height
                        .saturating_sub(composer_height)
                        .saturating_sub(pad);
                    let desired_height = view.desired_height(content_width);
                    let view_height = desired_height.min(max_view_height);

                    if view_height > 0 {
                        let view_rect = Rect {
                            x: area.x + horizontal_padding,
                            y: area.y,
                            width: content_width,
                            height: view_height,
                        };
                        let view_bg = ratatui::style::Style::default().bg(crate::colors::background());
                        fill_rect(buf, view_rect, None, view_bg);
                        view.render(view_rect, buf);
                        let remaining_height = area.height.saturating_sub(view_height);
                        if remaining_height > 0 {
                            let composer_area = Rect {
                                x: area.x,
                                y: view_rect.y.saturating_add(view_rect.height),
                                width: area.width,
                                height: remaining_height,
                            };
                            composer_rect = compute_composer_rect(composer_area, false);
                        }
                    } else {
                        composer_rect = compute_composer_rect(area, self.top_spacer_enabled);
                    }
                } else {
                    let mut avail = area.height;
                    if self.top_spacer_enabled && avail > 0 {
                        avail = avail.saturating_sub(1);
                    }
                    if avail > 0 {
                        let pad = BottomPane::BOTTOM_PAD_LINES.min(avail.saturating_sub(1));
                        let view_height = avail.saturating_sub(pad);
                        if view_height > 0 {
                            let y_base = if self.top_spacer_enabled {
                                area.y + 1
                            } else {
                                area.y
                            };
                            let view_rect = Rect {
                                x: area.x + horizontal_padding,
                                y: y_base,
                                width: area.width.saturating_sub(horizontal_padding * 2),
                                height: view_height,
                            };
                            let view_bg = ratatui::style::Style::default().bg(crate::colors::background());
                            fill_rect(buf, view_rect, None, view_bg);
                            view.render_with_composer(view_rect, buf, &self.composer);
                            composer_needs_render = false;
                        }
                    }
                }
            }
        }

        if composer_needs_render && composer_rect.width > 0 && composer_rect.height > 0 {
            let comp_bg = ratatui::style::Style::default().bg(crate::colors::background());
            fill_rect(buf, composer_rect, None, comp_bg);
            (&self.composer).render_ref(composer_rect, buf);
        }

    }
}

fn compute_composer_rect(area: Rect, top_spacer_enabled: bool) -> Rect {
    let horizontal_padding = 1u16;
    let mut y_offset = 0u16;
    if top_spacer_enabled {
        y_offset = y_offset.saturating_add(1);
    }
    let available = area.height.saturating_sub(y_offset);
    let height = available.saturating_sub(
        BottomPane::BOTTOM_PAD_LINES.min(available.saturating_sub(1)),
    );
    Rect {
        x: area.x + horizontal_padding,
        y: area.y + y_offset,
        width: area.width.saturating_sub(horizontal_padding * 2),
        height,
    }
}
