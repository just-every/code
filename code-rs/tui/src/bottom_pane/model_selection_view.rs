use super::bottom_pane_view::BottomPaneView;
use super::settings_panel::{render_panel, PanelFrameStyle};
use super::BottomPane;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use code_common::model_presets::ModelPreset;
use code_core::config_types::ReasoningEffort;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::prelude::Widget;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::cmp::Ordering;

/// Flattened preset entry combining a model with a specific reasoning effort.
#[derive(Clone, Debug)]
struct FlatPreset {
    model: String,
    effort: ReasoningEffort,
    label: String,
    description: String,
}

impl FlatPreset {
    fn from_model_preset(preset: &ModelPreset) -> Vec<Self> {
        preset
            .supported_reasoning_efforts
            .iter()
            .map(|effort_preset| {
                let effort_label = Self::effort_label(effort_preset.effort.into());
                FlatPreset {
                    model: preset.model.to_string(),
                    effort: effort_preset.effort.into(),
                    label: format!("{} {}", preset.display_name, effort_label.to_lowercase()),
                    description: effort_preset.description.to_string(),
                }
            })
            .collect()
    }

    fn effort_label(effort: ReasoningEffort) -> &'static str {
        match effort {
            ReasoningEffort::XHigh => "XHigh",
            ReasoningEffort::High => "High",
            ReasoningEffort::Medium => "Medium",
            ReasoningEffort::Low => "Low",
            ReasoningEffort::Minimal => "Minimal",
            ReasoningEffort::None => "None",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModelSelectionTarget {
    Session,
    Review,
    AutoDrive,
}

impl ModelSelectionTarget {
    fn panel_title(self) -> &'static str {
        match self {
            ModelSelectionTarget::Session => "Select Model & Reasoning",
            ModelSelectionTarget::Review => "Select Review Model & Reasoning",
            ModelSelectionTarget::AutoDrive => "Select Auto Drive Model & Reasoning",
        }
    }

    fn current_label(self) -> &'static str {
        match self {
            ModelSelectionTarget::Session => "Current model",
            ModelSelectionTarget::Review => "Review model",
            ModelSelectionTarget::AutoDrive => "Auto Drive model",
        }
    }

    fn reasoning_label(self) -> &'static str {
        match self {
            ModelSelectionTarget::Session => "Reasoning effort",
            ModelSelectionTarget::Review => "Review reasoning",
            ModelSelectionTarget::AutoDrive => "Auto Drive reasoning",
        }
    }
}

pub(crate) struct ModelSelectionView {
    flat_presets: Vec<FlatPreset>,
    selected_index: usize,
    current_model: String,
    current_effort: ReasoningEffort,
    app_event_tx: AppEventSender,
    is_complete: bool,
    target: ModelSelectionTarget,
}

impl ModelSelectionView {
    pub fn new(
        presets: Vec<ModelPreset>,
        current_model: String,
        current_effort: ReasoningEffort,
        target: ModelSelectionTarget,
        app_event_tx: AppEventSender,
    ) -> Self {
        let flat_presets: Vec<FlatPreset> = presets
            .iter()
            .flat_map(FlatPreset::from_model_preset)
            .collect();

        let initial_index = Self::initial_selection(&flat_presets, &current_model, current_effort);
        Self {
            flat_presets,
            selected_index: initial_index,
            current_model,
            current_effort,
            app_event_tx,
            is_complete: false,
            target,
        }
    }

    fn initial_selection(
        flat_presets: &[FlatPreset],
        current_model: &str,
        current_effort: ReasoningEffort,
    ) -> usize {
        if let Some((idx, _)) = flat_presets.iter().enumerate().find(|(_, preset)| {
            preset.model.eq_ignore_ascii_case(current_model) && preset.effort == current_effort
        }) {
            return idx;
        }

        if let Some((idx, _)) = flat_presets
            .iter()
            .enumerate()
            .find(|(_, preset)| preset.model.eq_ignore_ascii_case(current_model))
        {
            return idx;
        }

        0
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
        if self.flat_presets.is_empty() {
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
        if self.flat_presets.is_empty() {
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
        if let Some(flat_preset) = self.flat_presets.get(self.selected_index) {
            match self.target {
                ModelSelectionTarget::Session => {
                    let _ = self.app_event_tx.send(AppEvent::UpdateModelSelection {
                        model: flat_preset.model.clone(),
                        effort: Some(flat_preset.effort),
                    });
                }
                ModelSelectionTarget::Review => {
                    let _ = self.app_event_tx.send(AppEvent::UpdateReviewModelSelection {
                        model: flat_preset.model.clone(),
                        effort: flat_preset.effort,
                    });
                }
                ModelSelectionTarget::AutoDrive => {
                    let _ = self.app_event_tx.send(AppEvent::UpdateAutoDriveModelSelection {
                        model: flat_preset.model.clone(),
                        effort: flat_preset.effort,
                    });
                }
            }
        }
        self.is_complete = true;
    }

    fn content_line_count(&self) -> u16 {
        let mut lines: u16 = 3;

        let mut previous_model: Option<&str> = None;
        for idx in self.sorted_indices() {
            let flat_preset = &self.flat_presets[idx];
            let is_new_model = previous_model
                .map(|prev| !prev.eq_ignore_ascii_case(&flat_preset.model))
                .unwrap_or(true);

            if is_new_model {
                if previous_model.is_some() {
                    lines = lines.saturating_add(1);
                }
                lines = lines.saturating_add(1);
                if Self::model_description(&flat_preset.model).is_some() {
                    lines = lines.saturating_add(1);
                }
                previous_model = Some(&flat_preset.model);
            }

            lines = lines.saturating_add(1);
        }

        lines.saturating_add(2)
    }

    fn sorted_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.flat_presets.len()).collect();
        indices.sort_by(|&a, &b| Self::compare_presets(&self.flat_presets[a], &self.flat_presets[b]));
        indices
    }

    fn compare_presets(a: &FlatPreset, b: &FlatPreset) -> Ordering {
        let model_rank = Self::model_rank(&a.model).cmp(&Self::model_rank(&b.model));
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

        let effort_rank = Self::effort_rank(a.effort).cmp(&Self::effort_rank(b.effort));
        if effort_rank != Ordering::Equal {
            return effort_rank;
        }

        a.label.cmp(&b.label)
    }

    fn model_rank(model: &str) -> u8 {
        if model.eq_ignore_ascii_case("gpt-5.1-codex-max") {
            0
        } else if model.eq_ignore_ascii_case("gpt-5.1-codex") {
            1
        } else if model.eq_ignore_ascii_case("gpt-5.1-codex-mini") {
            2
        } else if model.eq_ignore_ascii_case("gpt-5.1") {
            3
        } else {
            4
        }
    }

    fn model_description(model: &str) -> Option<&'static str> {
        if model.eq_ignore_ascii_case("gpt-5.1-codex-max") {
            Some("Latest Codex-optimized flagship for deep and fast reasoning.")
        } else if model.eq_ignore_ascii_case("gpt-5.1-codex") {
            Some("Optimized for Code.")
        } else if model.eq_ignore_ascii_case("gpt-5.1-codex-mini") {
            Some("Optimized for Code. Cheaper, faster, but less capable.")
        } else if model.eq_ignore_ascii_case("gpt-5.1") {
            Some("Broad world knowledge with strong general reasoning.")
        } else {
            None
        }
    }

    fn effort_rank(effort: ReasoningEffort) -> u8 {
        match effort {
            ReasoningEffort::XHigh => 0,
            ReasoningEffort::High => 1,
            ReasoningEffort::Medium => 2,
            ReasoningEffort::Low => 3,
            ReasoningEffort::Minimal => 4,
            ReasoningEffort::None => 5,
        }
    }

    fn effort_label(effort: ReasoningEffort) -> &'static str {
        match effort {
            ReasoningEffort::XHigh => "XHigh",
            ReasoningEffort::High => "High",
            ReasoningEffort::Medium => "Medium",
            ReasoningEffort::Low => "Low",
            ReasoningEffort::Minimal => "Minimal",
            ReasoningEffort::None => "None",
        }
    }
}

impl ModelSelectionView {
    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) -> bool {
        match key_event {
            KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE, .. } => {
                self.move_selection_up();
                true
            }
            KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => {
                self.move_selection_down();
                true
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                self.confirm_selection();
                true
            }
            KeyEvent { code: KeyCode::Esc, modifiers: KeyModifiers::NONE, .. } => {
                self.is_complete = true;
                true
            }
            _ => false,
        }
    }

    fn render_panel_body(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(vec![
            Span::styled(
                format!("{}: ", self.target.current_label()),
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
                format!("{}: ", self.target.reasoning_label()),
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
        for preset_index in self.sorted_indices() {
            let flat_preset = &self.flat_presets[preset_index];
            if previous_model
                .map(|m| !m.eq_ignore_ascii_case(&flat_preset.model))
                .unwrap_or(true)
            {
                if previous_model.is_some() {
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(vec![Span::styled(
                    Self::format_model_header(&flat_preset.model),
                    Style::default()
                        .fg(crate::colors::text_bright())
                        .add_modifier(Modifier::BOLD),
                )]));
                if let Some(desc) = Self::model_description(&flat_preset.model) {
                    lines.push(Line::from(vec![Span::styled(
                        desc,
                        Style::default().fg(crate::colors::text_dim()),
                    )]));
                }
                previous_model = Some(&flat_preset.model);
            }

            let is_selected = preset_index == self.selected_index;
            let is_current = flat_preset.model.eq_ignore_ascii_case(&self.current_model)
                && flat_preset.effort == self.current_effort;
            let label = Self::effort_label(flat_preset.effort);
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

            lines.push(Line::from(vec![
                Span::styled("   ", indent_style),
                Span::styled(row_text, label_style),
                Span::styled(" - ", divider_style),
                Span::styled(&flat_preset.description, description_style),
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
            x: area.x.saturating_add(1),
            y: area.y,
            width: area.width.saturating_sub(1),
            height: area.height,
        };

        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            )
            .render(padded, buf);
    }

    pub(crate) fn render_without_frame(&self, area: Rect, buf: &mut Buffer) {
        self.render_panel_body(area, buf);
    }
}

impl<'a> BottomPaneView<'a> for ModelSelectionView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        let _ = self.handle_key_event_direct(key_event);
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        let content_lines = self.content_line_count();
        let total = content_lines.saturating_add(2);
        total.max(9)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        render_panel(
            area,
            buf,
            self.target.panel_title(),
            PanelFrameStyle::bottom_pane(),
            |inner, buf| self.render_panel_body(inner, buf),
        );
    }
}
