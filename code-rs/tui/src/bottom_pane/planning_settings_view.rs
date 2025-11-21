use code_core::config_types::ReasoningEffort;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;

use super::bottom_pane_view::BottomPaneView;
use super::scroll_state::ScrollState;
use super::BottomPane;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PlanningRow {
    CustomModel,
}

pub(crate) struct PlanningSettingsView {
    use_chat_model: bool,
    planning_model: String,
    planning_reasoning: ReasoningEffort,
    app_event_tx: AppEventSender,
    state: ScrollState,
    is_complete: bool,
}

impl PlanningSettingsView {
    pub fn new(
        use_chat_model: bool,
        planning_model: String,
        planning_reasoning: ReasoningEffort,
        app_event_tx: AppEventSender,
    ) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = Some(0);
        Self {
            use_chat_model,
            planning_model,
            planning_reasoning,
            app_event_tx,
            state,
            is_complete: false,
        }
    }

    pub fn set_planning_model(&mut self, model: String, effort: ReasoningEffort) {
        self.planning_model = model;
        self.planning_reasoning = effort;
    }

    pub fn set_use_chat_model(&mut self, use_chat: bool) {
        self.use_chat_model = use_chat;
    }

    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) {
        self.handle_key(key_event);
    }

    fn rows(&self) -> Vec<PlanningRow> {
        vec![PlanningRow::CustomModel]
    }

    fn handle_enter(&mut self, row: PlanningRow) {
        match row {
            PlanningRow::CustomModel => {
                let _ = self.app_event_tx.send(AppEvent::ShowPlanningModelSelector);
            }
        }
    }

    fn render_row(&self, row: PlanningRow, selected: bool) -> Line<'static> {
        let arrow = if selected { "› " } else { "  " };
        let arrow_style = if selected {
            Style::default().fg(colors::primary())
        } else {
            Style::default().fg(colors::text_dim())
        };

        match row {
            PlanningRow::CustomModel => {
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
                            Self::format_model_label(&self.planning_model),
                            Self::reasoning_label(self.planning_reasoning)
                        ),
                        Some("Enter to change".to_string()),
                    )
                };
                let mut spans = vec![
                    Span::styled(arrow, arrow_style),
                    Span::styled("Planning model", label_style),
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
        }
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

    fn handle_key(&mut self, key: KeyEvent) {
        let rows = self.rows();
        if self.state.selected_idx.is_none() {
            self.state.selected_idx = Some(0);
        }
        let total = rows.len();
        self.state.ensure_visible(total, 4);

        match key.code {
            KeyCode::Up => self.state.move_up_wrap(total),
            KeyCode::Down => self.state.move_down_wrap(total),
            KeyCode::Char(' ') | KeyCode::Enter => {
                if let Some(sel) = self.state.selected_idx {
                    if let Some(row) = rows.get(sel).copied() {
                        self.handle_enter(row);
                    }
                }
            }
            KeyCode::Esc => {
                self.is_complete = true;
            }
            _ => {}
        }
    }
}

impl<'a> BottomPaneView<'a> for PlanningSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        if !matches!(key_event.modifiers, KeyModifiers::NONE) {
            return;
        }
        self.handle_key(key_event);
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        6
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::border()))
            .style(Style::default().bg(colors::background()).fg(colors::text()))
            .title(" Planning Settings ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let header_lines = vec![
            Line::from(Span::styled(
                "Select the model used when you’re in Plan Mode (Read Only).",
                Style::default().fg(colors::text_dim()),
            )),
            Line::from(Span::styled(
                "Use ↑↓ to navigate · Enter/Space to toggle/open · Esc close",
                Style::default().fg(colors::text_dim()),
            )),
            Line::from(""),
        ];

        let rows = self.rows();
        let selected_idx = self.state.selected_idx.unwrap_or(0).min(rows.len().saturating_sub(1));

        let mut lines: Vec<Line> = Vec::new();
        lines.extend(header_lines.into_iter());
        for (idx, row) in rows.iter().enumerate() {
            let selected = idx == selected_idx;
            lines.push(self.render_row(*row, selected));
        }

        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(colors::background()).fg(colors::text()))
            .render(inner, buf);
    }
}
