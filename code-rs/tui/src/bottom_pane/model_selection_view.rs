use super::bottom_pane_view::BottomPaneView;
use super::settings_panel::{render_panel, PanelFrameStyle};
use super::BottomPane;
use crate::app_event::{AppEvent, ModelSelectionKind};
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
    Planning,
    AutoDrive,
    ReviewResolve,
    AutoReview,
    AutoReviewResolve,
}

impl From<ModelSelectionTarget> for ModelSelectionKind {
    fn from(target: ModelSelectionTarget) -> Self {
        match target {
            ModelSelectionTarget::Session => ModelSelectionKind::Session,
            ModelSelectionTarget::Review => ModelSelectionKind::Review,
            ModelSelectionTarget::Planning => ModelSelectionKind::Planning,
            ModelSelectionTarget::AutoDrive => ModelSelectionKind::AutoDrive,
            ModelSelectionTarget::ReviewResolve => ModelSelectionKind::ReviewResolve,
            ModelSelectionTarget::AutoReview => ModelSelectionKind::AutoReview,
            ModelSelectionTarget::AutoReviewResolve => ModelSelectionKind::AutoReviewResolve,
        }
    }
}

impl ModelSelectionTarget {
    fn panel_title(self) -> &'static str {
        match self {
            ModelSelectionTarget::Session => "Select Model & Reasoning",
            ModelSelectionTarget::Review => "Select Review Model & Reasoning",
            ModelSelectionTarget::Planning => "Select Planning Model & Reasoning",
            ModelSelectionTarget::AutoDrive => "Select Auto Drive Model & Reasoning",
            ModelSelectionTarget::ReviewResolve => "Select Resolve Model & Reasoning",
            ModelSelectionTarget::AutoReview => "Select Auto Review Model & Reasoning",
            ModelSelectionTarget::AutoReviewResolve => "Select Auto Review Resolve Model & Reasoning",
        }
    }

    fn current_label(self) -> &'static str {
        match self {
            ModelSelectionTarget::Session => "Current model",
            ModelSelectionTarget::Review => "Review model",
            ModelSelectionTarget::Planning => "Planning model",
            ModelSelectionTarget::AutoDrive => "Auto Drive model",
            ModelSelectionTarget::ReviewResolve => "Resolve model",
            ModelSelectionTarget::AutoReview => "Auto Review model",
            ModelSelectionTarget::AutoReviewResolve => "Auto Review resolve model",
        }
    }

    fn reasoning_label(self) -> &'static str {
        match self {
            ModelSelectionTarget::Session => "Reasoning effort",
            ModelSelectionTarget::Review => "Review reasoning",
            ModelSelectionTarget::Planning => "Planning reasoning",
            ModelSelectionTarget::AutoDrive => "Auto Drive reasoning",
            ModelSelectionTarget::ReviewResolve => "Resolve reasoning",
            ModelSelectionTarget::AutoReview => "Auto Review reasoning",
            ModelSelectionTarget::AutoReviewResolve => "Auto Review resolve reasoning",
        }
    }

    fn supports_follow_chat(self) -> bool {
        !matches!(self, ModelSelectionTarget::Session)
    }
}

pub(crate) struct ModelSelectionView {
    flat_presets: Vec<FlatPreset>,
    selected_index: usize,
    current_model: String,
    current_effort: ReasoningEffort,
    use_chat_model: bool,
    app_event_tx: AppEventSender,
    is_complete: bool,
    target: ModelSelectionTarget,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum EntryKind {
    FollowChat,
    Preset(usize),
}

impl ModelSelectionView {
    pub fn new(
        presets: Vec<ModelPreset>,
        current_model: String,
        current_effort: ReasoningEffort,
        use_chat_model: bool,
        target: ModelSelectionTarget,
        app_event_tx: AppEventSender,
    ) -> Self {
        let flat_presets: Vec<FlatPreset> = presets
            .iter()
            .flat_map(FlatPreset::from_model_preset)
            .collect();

        let initial_index = Self::initial_selection(
            target.supports_follow_chat(),
            use_chat_model,
            &flat_presets,
            &current_model,
            current_effort,
        );
        Self {
            flat_presets,
            selected_index: initial_index,
            current_model,
            current_effort,
            use_chat_model,
            app_event_tx,
            is_complete: false,
            target,
        }
    }

    pub(crate) fn update_presets(&mut self, presets: Vec<ModelPreset>) {
        let include_follow_chat = self.target.supports_follow_chat();
        let previous_entries = self.entries();
        let previous_selected = previous_entries.get(self.selected_index).copied();
        let previous_flat = self.flat_presets.clone();

        self.flat_presets = presets
            .iter()
            .flat_map(FlatPreset::from_model_preset)
            .collect();

        let mut next_selected: Option<usize> = None;
        match previous_selected {
            Some(EntryKind::FollowChat) => {
                if include_follow_chat {
                    next_selected = Some(0);
                }
            }
            Some(EntryKind::Preset(idx)) => {
                if let Some(old) = previous_flat.get(idx) {
                    if let Some((new_idx, _)) = self
                        .flat_presets
                        .iter()
                        .enumerate()
                        .find(|(_, preset)| {
                            preset.model.eq_ignore_ascii_case(&old.model)
                                && preset.effort == old.effort
                        })
                    {
                        next_selected = Some(new_idx + if include_follow_chat { 1 } else { 0 });
                    }
                }
            }
            None => {}
        }

        self.selected_index = next_selected.unwrap_or_else(|| {
            Self::initial_selection(
                include_follow_chat,
                self.use_chat_model,
                &self.flat_presets,
                &self.current_model,
                self.current_effort,
            )
        });

        let total = self.entries().len();
        if total == 0 {
            self.selected_index = 0;
        } else if self.selected_index >= total {
            self.selected_index = total - 1;
        }
    }

    fn initial_selection(
        include_follow_chat: bool,
        use_chat_model: bool,
        flat_presets: &[FlatPreset],
        current_model: &str,
        current_effort: ReasoningEffort,
    ) -> usize {
        if include_follow_chat && use_chat_model {
            return 0;
        }

        if let Some((idx, _)) = flat_presets.iter().enumerate().find(|(_, preset)| {
            preset.model.eq_ignore_ascii_case(current_model) && preset.effort == current_effort
        }) {
            return idx + if include_follow_chat { 1 } else { 0 };
        }

        if let Some((idx, _)) = flat_presets
            .iter()
            .enumerate()
            .find(|(_, preset)| preset.model.eq_ignore_ascii_case(current_model))
        {
            return idx + if include_follow_chat { 1 } else { 0 };
        }

        if include_follow_chat {
            if flat_presets.is_empty() {
                0
            } else {
                1
            }
        } else {
            0
        }
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

    fn entries(&self) -> Vec<EntryKind> {
        let mut entries = Vec::new();
        if self.target.supports_follow_chat() {
            entries.push(EntryKind::FollowChat);
        }
        for idx in self.sorted_indices() {
            entries.push(EntryKind::Preset(idx));
        }
        entries
    }

    fn move_selection_up(&mut self) {
        let total = self.entries().len();
        if total == 0 {
            return;
        }
        self.selected_index = if self.selected_index == 0 {
            total - 1
        } else {
            self.selected_index.saturating_sub(1)
        };
    }

    fn move_selection_down(&mut self) {
        let total = self.entries().len();
        if total == 0 {
            return;
        }
        self.selected_index = (self.selected_index + 1) % total;
    }

    fn confirm_selection(&mut self) {
        let entries = self.entries();
        if let Some(entry) = entries.get(self.selected_index) {
            match entry {
                EntryKind::FollowChat => {
                    match self.target {
                        ModelSelectionTarget::Session => {}
                        ModelSelectionTarget::Review => {
                            let _ =
                                self.app_event_tx.send(AppEvent::UpdateReviewUseChatModel(true));
                        }
                        ModelSelectionTarget::Planning => {
                            let _ = self
                                .app_event_tx
                                .send(AppEvent::UpdatePlanningUseChatModel(true));
                        }
                        ModelSelectionTarget::AutoDrive => {
                            let _ = self
                                .app_event_tx
                                .send(AppEvent::UpdateAutoDriveUseChatModel(true));
                        }
                        ModelSelectionTarget::ReviewResolve => {
                            let _ = self
                                .app_event_tx
                                .send(AppEvent::UpdateReviewResolveUseChatModel(true));
                        }
                        ModelSelectionTarget::AutoReview => {
                            let _ = self
                                .app_event_tx
                                .send(AppEvent::UpdateAutoReviewUseChatModel(true));
                        }
                        ModelSelectionTarget::AutoReviewResolve => {
                            let _ = self
                                .app_event_tx
                                .send(AppEvent::UpdateAutoReviewResolveUseChatModel(true));
                        }
                    }
                    self.send_closed(true);
                    return;
                }
                EntryKind::Preset(idx) => {
                    if let Some(flat_preset) = self.flat_presets.get(*idx) {
                        match self.target {
                            ModelSelectionTarget::Session => {
                                let _ = self.app_event_tx.send(AppEvent::UpdateModelSelection {
                                    model: flat_preset.model.clone(),
                                    effort: Some(flat_preset.effort),
                                });
                            }
                            ModelSelectionTarget::Review => {
                                let _ = self
                                    .app_event_tx
                                    .send(AppEvent::UpdateReviewModelSelection {
                                        model: flat_preset.model.clone(),
                                        effort: flat_preset.effort,
                                    });
                            }
                            ModelSelectionTarget::Planning => {
                                let _ = self
                                    .app_event_tx
                                    .send(AppEvent::UpdatePlanningModelSelection {
                                        model: flat_preset.model.clone(),
                                        effort: flat_preset.effort,
                                    });
                            }
                            ModelSelectionTarget::AutoDrive => {
                                let _ = self
                                    .app_event_tx
                                    .send(AppEvent::UpdateAutoDriveModelSelection {
                                        model: flat_preset.model.clone(),
                                        effort: flat_preset.effort,
                                    });
                            }
                            ModelSelectionTarget::ReviewResolve => {
                                let _ = self
                                    .app_event_tx
                                    .send(AppEvent::UpdateReviewResolveModelSelection {
                                        model: flat_preset.model.clone(),
                                        effort: flat_preset.effort,
                                    });
                            }
                            ModelSelectionTarget::AutoReview => {
                                let _ = self
                                    .app_event_tx
                                    .send(AppEvent::UpdateAutoReviewModelSelection {
                                        model: flat_preset.model.clone(),
                                        effort: flat_preset.effort,
                                    });
                            }
                            ModelSelectionTarget::AutoReviewResolve => {
                                let _ = self
                                    .app_event_tx
                                    .send(AppEvent::UpdateAutoReviewResolveModelSelection {
                                        model: flat_preset.model.clone(),
                                        effort: flat_preset.effort,
                                    });
                            }
                        }
                    }
                    self.send_closed(true);
                }
            }
        }
    }

    fn content_line_count(&self) -> u16 {
        let mut lines: u16 = 3;
        if self.target.supports_follow_chat() {
            // Header + description + entry + spacer
            lines = lines.saturating_add(4);
        }

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
        if model.eq_ignore_ascii_case("gpt-5.2-codex") {
            0
        } else if model.eq_ignore_ascii_case("gpt-5.2") {
            1
        } else if model.eq_ignore_ascii_case("gpt-5.1-codex-max") {
            2
        } else if model.eq_ignore_ascii_case("gpt-5.1-codex") {
            3
        } else if model.eq_ignore_ascii_case("gpt-5.1-codex-mini") {
            4
        } else if model.eq_ignore_ascii_case("gpt-5.1") {
            5
        } else {
            6
        }
    }

    fn model_description(model: &str) -> Option<&'static str> {
        if model.eq_ignore_ascii_case("gpt-5.2-codex") {
            Some("Latest frontier agentic coding model.")
        } else if model.eq_ignore_ascii_case("gpt-5.2") {
            Some("Latest frontier model with improvements across knowledge, reasoning, and coding.")
        } else if model.eq_ignore_ascii_case("gpt-5.1-codex-max") {
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
                self.send_closed(false);
                true
            }
            _ => false,
        }
    }

    fn send_closed(&mut self, accepted: bool) {
        if self.is_complete {
            return;
        }
        let _ = self.app_event_tx.send(AppEvent::ModelSelectionClosed {
            target: self.target.into(),
            accepted,
        });
        self.is_complete = true;
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
                if self.target.supports_follow_chat() && self.use_chat_model {
                    "Follow Chat Mode".to_string()
                } else {
                    Self::format_model_header(&self.current_model)
                },
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
                if self.target.supports_follow_chat() && self.use_chat_model {
                    "From chat".to_string()
                } else {
                    Self::effort_label(self.current_effort).to_string()
                },
                Style::default()
                    .fg(crate::colors::warning())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));

        if self.target.supports_follow_chat() {
            let is_selected = self.selected_index == 0;

            let header_style = Style::default()
                .fg(crate::colors::text_bright())
                .add_modifier(Modifier::BOLD);
            let desc_style = Style::default().fg(crate::colors::text_dim());
            lines.push(Line::from(vec![Span::styled("Follow Chat Mode", header_style)]));
            lines.push(Line::from(vec![Span::styled(
                "Use the active chat model and reasoning; stays in sync as chat changes.",
                desc_style,
            )]));

            let mut label_style = Style::default().fg(crate::colors::text());
            if is_selected {
                label_style = label_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }
            let mut arrow_style = Style::default().fg(crate::colors::text_dim());
            if is_selected {
                arrow_style = label_style.clone();
            }
            let indent_style = if is_selected {
                Style::default()
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let mut status = String::new();
            if self.use_chat_model {
                status.push_str("(current)");
            }
            let arrow = if is_selected { "› " } else { "  " };
            let mut spans = vec![
                Span::styled(arrow, arrow_style),
                Span::styled("   ", indent_style),
                Span::styled("Use chat model", label_style),
            ];
            if !status.is_empty() {
                spans.push(Span::raw(format!("  {}", status)));
            }
            lines.push(Line::from(spans));
            lines.push(Line::from(""));
        }

        let mut previous_model: Option<&str> = None;
        let entries = self.entries();
        for (entry_idx, entry) in entries.iter().enumerate() {
            let EntryKind::Preset(preset_index) = entry else { continue };
            let flat_preset = &self.flat_presets[*preset_index];
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

            let is_selected = entry_idx == self.selected_index;
            let is_current = !self.use_chat_model
                && flat_preset.model.eq_ignore_ascii_case(&self.current_model)
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
