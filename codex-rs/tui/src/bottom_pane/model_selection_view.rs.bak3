use super::BottomPane;
use super::bottom_pane_view::BottomPaneView;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use codex_common::model_presets::ModelPreset;
use codex_core::config_types::ReasoningEffort;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Alignment;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use std::cmp::Ordering;

pub(crate) struct ModelSelectionView {
    presets: Vec<ModelPreset>,
    selected_index: usize,
    current_model: String,
    current_effort: ReasoningEffort,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

impl ModelSelectionView {
    pub fn new(
        presets: Vec<ModelPreset>,
        current_model: String,
        current_effort: ReasoningEffort,
        app_event_tx: AppEventSender,
    ) -> Self {
        let initial_index = Self::initial_selection(&presets, &current_model, current_effort);
        Self {
            presets,
            selected_index: initial_index,
            current_model,
            current_effort,
            app_event_tx,
            is_complete: false,
        }
    }

    fn initial_selection(
        presets: &[ModelPreset],
        current_model: &str,
        current_effort: ReasoningEffort,
    ) -> usize {
        // Prefer an exact match on model + effort, fall back to first model match, then first entry.
        if let Some((idx, _)) = presets.iter().enumerate().find(|(_, preset)| {
            preset.model.eq_ignore_ascii_case(current_model)
                && Self::preset_effort(preset) == current_effort
        }) {
            return idx;
        }

        if let Some((idx, _)) = presets
            .iter()
            .enumerate()
            .find(|(_, preset)| preset.model.eq_ignore_ascii_case(current_model))
        {
            return idx;
        }

        0
    }

    fn preset_effort(preset: &ModelPreset) -> ReasoningEffort {
        preset
            .effort
            .map(ReasoningEffort::from)
            .unwrap_or(ReasoningEffort::Medium)
    }

    fn format_model_header(model: &str) -> String {
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

    fn move_selection_up(&mut self) {
        if self.presets.is_empty() {
            return;
        }
        let sorted = self.sorted_indices();
        if sorted.is_empty() {
            return;
        }

        let current_pos = sorted
            .iter()
            .position(|&idx| idx == self.selected_index)
            .unwrap_or(0);
        let new_pos = if current_pos == 0 {
            sorted.len() - 1
        } else {
            current_pos - 1
        };
        self.selected_index = sorted[new_pos];
    }

    fn move_selection_down(&mut self) {
        if self.presets.is_empty() {
            return;
        }
        let sorted = self.sorted_indices();
        if sorted.is_empty() {
            return;
        }

        let current_pos = sorted
            .iter()
            .position(|&idx| idx == self.selected_index)
            .unwrap_or(0);
        let new_pos = (current_pos + 1) % sorted.len();
        self.selected_index = sorted[new_pos];
    }

    fn confirm_selection(&mut self) {
        if let Some(preset) = self.presets.get(self.selected_index) {
            let effort = Self::preset_effort(preset);
            let _ = self.app_event_tx.send(AppEvent::UpdateModelSelection {
                model: preset.model.to_string(),
                effort: Some(effort),
            });
        }
        self.is_complete = true;
    }

    fn content_line_count(&self) -> u16 {
        // Current model, reasoning effort, and initial spacer.
        let mut lines: u16 = 3;

        let mut previous_model: Option<&str> = None;
        for idx in self.sorted_indices() {
            let preset = &self.presets[idx];
            let is_new_model = previous_model
                .map(|prev| !prev.eq_ignore_ascii_case(&preset.model))
                .unwrap_or(true);

            if is_new_model {
                if previous_model.is_some() {
                    // Spacer plus header when switching between model groups.
                    lines = lines.saturating_add(2);
                } else {
                    // Only the header for the first model group; initial spacer already counted.
                    lines = lines.saturating_add(1);
                }
                previous_model = Some(preset.model);
            }

            // The preset entry row.
            lines = lines.saturating_add(1);
        }

        // Spacer before footer plus footer hint row.
        lines.saturating_add(2)
    }

    fn sorted_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.presets.len()).collect();
        indices.sort_by(|&a, &b| Self::compare_presets(&self.presets[a], &self.presets[b]));
        indices
    }

    fn compare_presets(a: &ModelPreset, b: &ModelPreset) -> Ordering {
        let model_rank = Self::model_rank(a.model).cmp(&Self::model_rank(b.model));
        if model_rank != Ordering::Equal {
            return model_rank;
        }

        let model_name_rank = a
            .model
            .to_ascii_lowercase()
            .cmp(&b.model.to_ascii_lowercase());
        if model_name_rank != Ordering::Equal {
            return model_name_rank;
        }

        let effort_rank = Self::effort_rank(Self::preset_effort(a))
            .cmp(&Self::effort_rank(Self::preset_effort(b)));
        if effort_rank != Ordering::Equal {
            return effort_rank;
        }

        a.label.cmp(b.label)
    }

    fn model_rank(model: &str) -> u8 {
        if model.eq_ignore_ascii_case("gpt-5-codex") {
            0
        } else if model.eq_ignore_ascii_case("gpt-5") {
            1
        } else {
            2
        }
    }

    fn effort_rank(effort: ReasoningEffort) -> u8 {
        match effort {
            ReasoningEffort::High => 0,
            ReasoningEffort::Medium => 1,
            ReasoningEffort::Low => 2,
            ReasoningEffort::Minimal => 3,
            ReasoningEffort::None => 4,
        }
    }

    fn effort_label(effort: ReasoningEffort) -> &'static str {
        match effort {
            ReasoningEffort::High => "High",
            ReasoningEffort::Medium => "Medium",
            ReasoningEffort::Low => "Low",
            ReasoningEffort::Minimal => "Minimal",
            ReasoningEffort::None => "None",
        }
    }

    fn effort_description(effort: ReasoningEffort) -> &'static str {
        match effort {
            ReasoningEffort::Minimal => {
                "Minimal reasoning. When speed is more important than accuracy. (fastest)"
            }
            ReasoningEffort::Low => "Basic reasoning. Works quickly in simple code bases. (fast)",
            ReasoningEffort::Medium => "Balanced reasoning. Ideal for most tasks. (default)",
            ReasoningEffort::High => {
                "Deep reasoning. Useful when solving difficult problems. (slower)"
            }
            ReasoningEffort::None => "Reasoning disabled",
        }
    }
}

impl<'a> BottomPaneView<'a> for ModelSelectionView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_selection_up();
            }
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_selection_down();
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.confirm_selection();
            }
            KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.is_complete = true;
            }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        // Account for content rows plus bordered block padding.
        let content_lines = self.content_line_count();
        let total = content_lines.saturating_add(2);
        total.max(9)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            )
            .title(" Select Model & Reasoning ")
            .title_alignment(Alignment::Center);

        let inner_area = block.inner(area);
        block.render(area, buf);

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(vec![
            Span::styled(
                "Current model: ",
                Style::default().fg(crate::colors::text_dim()),
            ),
            Span::styled(
                self.current_model.clone(),
                Style::default()
                    .fg(crate::colors::warning())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled(
                "Reasoning effort: ",
                Style::default().fg(crate::colors::text_dim()),
            ),
            Span::styled(
                format!("{}", self.current_effort),
                Style::default()
                    .fg(crate::colors::warning())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));

        let mut previous_model: Option<&str> = None;
        let sorted_indices = self.sorted_indices();

        for preset_index in sorted_indices {
            let preset = &self.presets[preset_index];
            if previous_model
                .map(|m| !m.eq_ignore_ascii_case(&preset.model))
                .unwrap_or(true)
            {
                if previous_model.is_some() {
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(vec![Span::styled(
                    Self::format_model_header(&preset.model),
                    Style::default()
                        .fg(crate::colors::text_bright())
                        .add_modifier(Modifier::BOLD),
                )]));
                previous_model = Some(preset.model);
            }

            let is_selected = preset_index == self.selected_index;
            let preset_effort = Self::preset_effort(preset);
            let is_current = preset.model.eq_ignore_ascii_case(&self.current_model)
                && preset_effort == self.current_effort;
            let label = Self::effort_label(preset_effort);
            let mut row_text = label.to_string();
            if is_current {
                row_text.push_str(" (current)");
            }

            let mut indent_style = Style::default();
            if is_selected {
                indent_style = indent_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }

            let mut label_style = Style::default().fg(crate::colors::text());
            if is_selected {
                label_style = label_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }
            if is_current {
                label_style = label_style.fg(crate::colors::success());
            }

            let mut divider_style = Style::default().fg(crate::colors::text_dim());
            if is_selected {
                divider_style = divider_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }

            let mut description_style = Style::default().fg(crate::colors::dim());
            if is_selected {
                description_style = description_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }

            let description = Self::effort_description(preset_effort);

            lines.push(Line::from(vec![
                Span::styled("   ", indent_style),
                Span::styled(row_text, label_style),
                Span::styled(" - ", divider_style),
                Span::styled(description, description_style),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(crate::colors::light_blue())),
            Span::raw(" Navigate  "),
            Span::styled("Enter", Style::default().fg(crate::colors::success())),
            Span::raw(" Select  "),
            Span::styled("Esc", Style::default().fg(crate::colors::error())),
            Span::raw(" Cancel"),
        ]));

        let padded = Rect {
            x: inner_area.x.saturating_add(1),
            y: inner_area.y,
            width: inner_area.width.saturating_sub(1),
            height: inner_area.height,
        };

        let paragraph = Paragraph::new(lines).alignment(Alignment::Left).style(
            Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text()),
        );
        paragraph.render(padded, buf);
    }
}
