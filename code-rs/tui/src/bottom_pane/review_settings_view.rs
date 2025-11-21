use code_core::config_types::{AutoResolveAttemptLimit, ReasoningEffort};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};
use std::cell::Cell;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;

use super::bottom_pane_view::BottomPaneView;
use super::scroll_state::ScrollState;
use super::BottomPane;

const DEFAULT_VISIBLE_ROWS: usize = 4;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SelectionKind {
    Model,
    Toggle,
    Attempts,
}

enum RowData {
    CustomModel,
    Toggle,
    Attempts,
}

pub(crate) struct ReviewSettingsView {
    use_chat_model: bool,
    review_model: String,
    review_reasoning: ReasoningEffort,
    auto_resolve_enabled: bool,
    auto_resolve_attempts: u32,
    auto_attempt_index: usize,
    app_event_tx: AppEventSender,
    state: ScrollState,
    is_complete: bool,
    viewport_rows: Cell<usize>,
    pending_notice: Option<String>,
}

impl ReviewSettingsView {
    pub fn set_review_model(&mut self, model: String, effort: ReasoningEffort) {
        self.review_model = model;
        self.review_reasoning = effort;
    }

    pub fn set_use_chat_model(&mut self, use_chat: bool) {
        self.use_chat_model = use_chat;
    }

    pub fn new(
        use_chat_model: bool,
        review_model: String,
        review_reasoning: ReasoningEffort,
        auto_resolve_enabled: bool,
        auto_resolve_attempts: u32,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        let default_index = AutoResolveAttemptLimit::ALLOWED
            .iter()
            .position(|&value| value == AutoResolveAttemptLimit::DEFAULT)
            .unwrap_or(0);
        let attempt_index = AutoResolveAttemptLimit::ALLOWED
            .iter()
            .position(|&value| value == auto_resolve_attempts)
            .unwrap_or(default_index);
        Self {
            use_chat_model,
            review_model,
            review_reasoning,
            auto_resolve_enabled,
            auto_resolve_attempts,
            auto_attempt_index: attempt_index,
            app_event_tx,
            state,
            is_complete: false,
            viewport_rows: Cell::new(0),
            pending_notice: None,
        }
    }

    fn toggle_auto_resolve(&mut self) {
        self.auto_resolve_enabled = !self.auto_resolve_enabled;
        self.app_event_tx
            .send(AppEvent::UpdateReviewAutoResolveEnabled(self.auto_resolve_enabled));
    }

    fn adjust_auto_resolve_attempts(&mut self, forward: bool) {
        let allowed = AutoResolveAttemptLimit::ALLOWED;
        if allowed.is_empty() {
            return;
        }

        let len = allowed.len();
        let mut next = self.auto_attempt_index;
        next = if forward {
            (next + 1) % len
        } else if next == 0 {
            len.saturating_sub(1)
        } else {
            next - 1
        };

        if next == self.auto_attempt_index {
            return;
        }

        self.auto_attempt_index = next;
        self.auto_resolve_attempts = allowed[next];
        self.app_event_tx
            .send(AppEvent::UpdateReviewAutoResolveAttempts(self.auto_resolve_attempts));
    }

    fn open_review_model_selector(&self) {
        self.app_event_tx
            .send(AppEvent::ShowReviewModelSelector);
    }

    fn build_rows(&self) -> (Vec<RowData>, Vec<usize>, Vec<SelectionKind>) {
        let rows = vec![RowData::CustomModel, RowData::Toggle, RowData::Attempts];
        let selection_rows = vec![0, 1, 2];
        let selection_kinds = vec![SelectionKind::Model, SelectionKind::Toggle, SelectionKind::Attempts];
        (rows, selection_rows, selection_kinds)
    }

    fn visible_budget(&self, total: usize) -> usize {
        if total == 0 {
            return 1;
        }
        let hint = self.viewport_rows.get();
        let target = if hint == 0 { DEFAULT_VISIBLE_ROWS } else { hint };
        target.clamp(1, total)
    }

    fn reasoning_label(effort: ReasoningEffort) -> &'static str {
        match effort {
            ReasoningEffort::XHigh => "XHigh",
            ReasoningEffort::High => "High",
            ReasoningEffort::Medium => "Medium",
            ReasoningEffort::Low => "Low",
            ReasoningEffort::Minimal => "Minimal",
            ReasoningEffort::None => "None",
        }
    }

    fn format_model_label(model: &str) -> String {
        let mut parts = Vec::new();
        for (idx, part) in model.split('-').enumerate() {
            if idx == 0 {
                parts.push(part.to_ascii_uppercase());
                continue;
            }
            let mut chars = part.chars();
            let formatted = match chars.next() {
                Some(first) if first.is_ascii_alphabetic() => {
                    let mut s = String::new();
                    s.push(first.to_ascii_uppercase());
                    s.push_str(chars.as_str());
                    s
                }
                Some(first) => {
                    let mut s = String::new();
                    s.push(first);
                    s.push_str(chars.as_str());
                    s
                }
                None => String::new(),
            };
            parts.push(formatted);
        }
        parts.join("-")
    }

    fn render_row(&self, row: &RowData, selected: bool) -> Line<'static> {
        let arrow = if selected { "› " } else { "  " };
        let arrow_style = if selected {
            Style::default().fg(colors::primary())
        } else {
            Style::default().fg(colors::text_dim())
        };
        match row {
            RowData::CustomModel => {
                let label_style = if selected {
                    Style::default()
                        .fg(colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text()).add_modifier(Modifier::BOLD)
                };
                let value_style = if selected {
                    Style::default()
                        .fg(colors::function())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text())
                };
                let (value_text, hint_text) = if self.use_chat_model {
                    (
                        "Follow Chat Mode".to_string(),
                        Some("Enter to change".to_string()),
                    )
                } else {
                    (
                        format!(
                            "{} ({})",
                            Self::format_model_label(&self.review_model),
                            Self::reasoning_label(self.review_reasoning)
                        ),
                        Some("Enter to change".to_string()),
                    )
                };
                let mut spans = vec![
                    Span::styled(arrow, arrow_style),
                    Span::styled("Custom review model", label_style),
                    Span::raw("  "),
                    Span::styled(value_text, value_style),
                ];
                if selected {
                    if let Some(hint) = hint_text {
                        spans.push(Span::raw("  "));
                        spans.push(Span::styled(hint, Style::default().fg(colors::text_dim())));
                    }
                }
                Line::from(spans)
            }
            RowData::Toggle => {
                let label_style = if selected {
                    Style::default()
                        .fg(colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text()).add_modifier(Modifier::BOLD)
                };
                let status_span = if self.auto_resolve_enabled {
                    Span::styled("On", Style::default().fg(colors::success()))
                } else {
                    Span::styled("Off", Style::default().fg(colors::text_dim()))
                };
                let mut spans = vec![
                    Span::styled(arrow, arrow_style),
                    Span::styled("Auto Resolve reviews", label_style),
                    Span::raw("  "),
                    status_span,
                ];
                if selected {
                    let hint = if self.auto_resolve_enabled {
                        "(press Enter to disable)"
                    } else {
                        "(press Enter to enable)"
                    };
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(hint, Style::default().fg(colors::text_dim())));
                }
                Line::from(spans)
            }
            RowData::Attempts => {
                let label_style = if selected {
                    Style::default()
                        .fg(colors::primary())
                        .add_modifier(Modifier::BOLD)
                } else if self.auto_resolve_enabled {
                    Style::default().fg(colors::text()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors::text_dim()).add_modifier(Modifier::BOLD)
                };
                let value_style = if selected {
                    Style::default().fg(colors::function()).add_modifier(Modifier::BOLD)
                } else if self.auto_resolve_attempts == 0 {
                    Style::default().fg(colors::text_dim())
                } else {
                    Style::default().fg(colors::text())
                };
                let value_label = if self.auto_resolve_attempts == 0 {
                    "0 (no re-reviews)".to_string()
                } else if self.auto_resolve_attempts == 1 {
                    "1 re-review".to_string()
                } else {
                    format!("{} re-reviews", self.auto_resolve_attempts)
                };
                let mut spans = vec![
                    Span::styled(arrow, arrow_style),
                    Span::styled("Max follow-up reviews", label_style),
                    Span::raw("  "),
                    Span::styled(value_label, value_style),
                ];
                if selected {
                    spans.push(Span::raw("  (←→ to adjust)"));
                }
                Line::from(spans)
            }
        }
    }

    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) {
        self.handle_key_event_impl(key_event);
    }

    fn handle_key_event_impl(&mut self, key_event: KeyEvent) {
        let (_, _, selection_kinds) = self.build_rows();
        let mut total = selection_kinds.len();
        if total == 0 {
            if matches!(key_event.code, KeyCode::Esc) {
                self.is_complete = true;
            }
            return;
        }
        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        self.state.clamp_selection(total);
        let visible_budget = self.visible_budget(total);
        self.state.ensure_visible(total, visible_budget);
        let current_kind = self
            .state
            .selected_idx
            .and_then(|sel| selection_kinds.get(sel))
            .copied();

        match key_event {
            KeyEvent { code: KeyCode::Up, .. } => {
                self.state.move_up_wrap(total);
            }
            KeyEvent { code: KeyCode::Down, .. } => {
                self.state.move_down_wrap(total);
            }
            KeyEvent { code: KeyCode::Left, .. } => {
                if let Some(kind) = current_kind {
                    match kind {
                        SelectionKind::Toggle => self.toggle_auto_resolve(),
                        SelectionKind::Attempts => self.adjust_auto_resolve_attempts(false),
                        SelectionKind::Model => {}
                    }
                }
            }
            KeyEvent { code: KeyCode::Right, .. } => {
                if let Some(kind) = current_kind {
                    match kind {
                        SelectionKind::Toggle => self.toggle_auto_resolve(),
                        SelectionKind::Attempts => self.adjust_auto_resolve_attempts(true),
                        SelectionKind::Model => {}
                    }
                }
            }
            KeyEvent { code: KeyCode::Char(' '), .. }
            | KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                if let Some(kind) = current_kind {
                    match kind {
                        SelectionKind::Toggle => self.toggle_auto_resolve(),
                        SelectionKind::Attempts => self.adjust_auto_resolve_attempts(true),
                        SelectionKind::Model => {
                            if let Some(sel) = self.state.selected_idx {
                                if let Some(row) = self.build_rows().0.get(sel) {
                                    match row {
                                        RowData::CustomModel => {
                                            self.open_review_model_selector();
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
            }
            _ => {}
        }

        let (_, _, selection_kinds) = self.build_rows();
        total = selection_kinds.len();
        if total == 0 {
            self.state.selected_idx = None;
            self.state.scroll_top = 0;
        } else {
            self.state.clamp_selection(total);
            let visible_budget = self.visible_budget(total);
            self.state.ensure_visible(total, visible_budget);
        }
    }
}

impl<'a> BottomPaneView<'a> for ReviewSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        self.handle_key_event_impl(key_event);
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        6
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::border()))
            .style(Style::default().bg(colors::background()).fg(colors::text()))
            .title(" Review Settings ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let header_lines = vec![
            Line::from(Span::styled(
                "Choose review model + Auto Resolve automation for /review.",
                Style::default().fg(colors::text_dim()),
            )),
            Line::from(Span::styled(
                "Use ↑↓ to navigate · Enter select/open · Space toggle · ←→ adjust values · Esc close",
                Style::default().fg(colors::text_dim()),
            )),
            Line::from(""),
        ];
        let footer_lines = {
            let mut lines = vec![Line::from(vec![
                Span::styled("↑↓", Style::default().fg(colors::function())),
                Span::styled(" Navigate  ", Style::default().fg(colors::text_dim())),
                Span::styled("Enter", Style::default().fg(colors::success())),
                Span::styled(" Select  ", Style::default().fg(colors::text_dim())),
                Span::styled("Space", Style::default().fg(colors::success())),
                Span::styled(" Toggle  ", Style::default().fg(colors::text_dim())),
                Span::styled("←→", Style::default().fg(colors::function())),
                Span::styled(" Adjust  ", Style::default().fg(colors::text_dim())),
                Span::styled("Esc", Style::default().fg(colors::error())),
                Span::styled(" Close", Style::default().fg(colors::text_dim())),
            ])];
            if let Some(notice) = &self.pending_notice {
                lines.push(Line::from(vec![Span::styled(
                    notice.clone(),
                    Style::default().fg(colors::warning()),
                )]));
            }
            lines
        };

        let available_height = inner.height as usize;
        let header_height = header_lines.len().min(available_height);
        let footer_height = if available_height > header_height {
            1 + footer_lines.len()
        } else {
            0
        };
        let list_height = available_height.saturating_sub(header_height + footer_height);
        let visible_slots = list_height.max(1);
        self.viewport_rows.set(visible_slots);

        let (rows, selection_rows, _) = self.build_rows();
        let selection_count = selection_rows.len();
        let selected_idx = self.state.selected_idx.unwrap_or(0).min(selection_count.saturating_sub(1));
        let selected_row_index = selection_rows.get(selected_idx).copied().unwrap_or(0);

        let mut visible_lines: Vec<Line> = Vec::new();
        visible_lines.extend(header_lines.iter().cloned());

        let mut remaining = visible_slots;
        let mut row_index = 0;
        while remaining > 0 && row_index < rows.len() {
            let is_selected = row_index == selected_row_index;
            visible_lines.push(self.render_row(&rows[row_index], is_selected));
            remaining = remaining.saturating_sub(1);
            row_index += 1;
        }

        if footer_height > 0 {
            visible_lines.push(Line::from(""));
            visible_lines.extend(footer_lines.into_iter());
        }

        Paragraph::new(visible_lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(colors::background()).fg(colors::text()))
            .render(inner, buf);
    }
}
