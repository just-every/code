use crate::app_event::{AppEvent, AutoContinueMode};
use crate::app_event_sender::AppEventSender;
use crate::colors;
use code_core::config_types::{
    AutoDriveModelRoutingEntry,
    ReasoningEffort,
    default_auto_drive_model_routing_entries,
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::settings_panel::{PanelFrameStyle, render_panel};
use super::BottomPane;

const ROUTING_REASONING_LEVELS: [ReasoningEffort; 5] = [
    ReasoningEffort::Minimal,
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
    ReasoningEffort::XHigh,
];

const ROUTING_DESCRIPTION_MAX_CHARS: usize = 200;

#[derive(Clone)]
enum AutoDriveSettingsMode {
    Main,
    RoutingList,
    RoutingEditor(RoutingEditorState),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RoutingEditorField {
    Model,
    Enabled,
    Reasoning,
    Description,
    Save,
    Cancel,
}

impl RoutingEditorField {
    fn all() -> &'static [RoutingEditorField] {
        &[
            RoutingEditorField::Model,
            RoutingEditorField::Enabled,
            RoutingEditorField::Reasoning,
            RoutingEditorField::Description,
            RoutingEditorField::Save,
            RoutingEditorField::Cancel,
        ]
    }

    fn next(self) -> Self {
        let fields = Self::all();
        let current_idx = fields.iter().position(|field| *field == self).unwrap_or(0);
        fields
            .get((current_idx + 1) % fields.len())
            .copied()
            .unwrap_or(RoutingEditorField::Model)
    }

    fn previous(self) -> Self {
        let fields = Self::all();
        let current_idx = fields.iter().position(|field| *field == self).unwrap_or(0);
        if current_idx == 0 {
            fields.last().copied().unwrap_or(RoutingEditorField::Model)
        } else {
            fields
                .get(current_idx - 1)
                .copied()
                .unwrap_or(RoutingEditorField::Model)
        }
    }
}

#[derive(Clone)]
struct RoutingEditorState {
    index: Option<usize>,
    model_cursor: usize,
    enabled: bool,
    reasoning_cursor: usize,
    reasoning_enabled: [bool; 5],
    description: String,
    selected_field: RoutingEditorField,
}

impl RoutingEditorState {
    fn from_entry(
        index: Option<usize>,
        entry: Option<&AutoDriveModelRoutingEntry>,
        model_options: &[String],
    ) -> Self {
        let mut reasoning_enabled = [false; 5];
        let mut model_cursor = 0;
        let mut enabled = true;
        let mut description = String::new();

        if let Some(existing) = entry {
            for (idx, level) in ROUTING_REASONING_LEVELS.iter().enumerate() {
                reasoning_enabled[idx] = existing.reasoning_levels.contains(level);
            }
            if let Some(found) = model_options
                .iter()
                .position(|model| model.eq_ignore_ascii_case(&existing.model))
            {
                model_cursor = found;
            }
            enabled = existing.enabled;
            description = existing.description.clone();
        } else if let Some(high_idx) = ROUTING_REASONING_LEVELS
            .iter()
            .position(|level| *level == ReasoningEffort::High)
        {
            reasoning_enabled[high_idx] = true;
        }

        Self {
            index,
            model_cursor,
            enabled,
            reasoning_cursor: 0,
            reasoning_enabled,
            description,
            selected_field: RoutingEditorField::Model,
        }
    }

    fn selected_reasoning_levels(&self) -> Vec<ReasoningEffort> {
        ROUTING_REASONING_LEVELS
            .iter()
            .enumerate()
            .filter_map(|(idx, level)| self.reasoning_enabled[idx].then_some(*level))
            .collect()
    }

    fn toggle_reasoning_at_cursor(&mut self) {
        if self.reasoning_cursor >= self.reasoning_enabled.len() {
            self.reasoning_cursor = self.reasoning_enabled.len().saturating_sub(1);
        }
        if let Some(slot) = self.reasoning_enabled.get_mut(self.reasoning_cursor) {
            *slot = !*slot;
        }
    }
}

pub(crate) struct AutoDriveSettingsView {
    app_event_tx: AppEventSender,
    selected_index: usize,
    mode: AutoDriveSettingsMode,
    model: String,
    model_reasoning: ReasoningEffort,
    use_chat_model: bool,
    review_enabled: bool,
    agents_enabled: bool,
    cross_check_enabled: bool,
    qa_automation_enabled: bool,
    diagnostics_enabled: bool,
    model_routing_enabled: bool,
    model_routing_entries: Vec<AutoDriveModelRoutingEntry>,
    routing_model_options: Vec<String>,
    routing_selected_index: usize,
    continue_mode: AutoContinueMode,
    status_message: Option<String>,
    closing: bool,
}

impl AutoDriveSettingsView {
    const PANEL_TITLE: &'static str = "Auto Drive Settings";

    pub fn new(
        app_event_tx: AppEventSender,
        model: String,
        model_reasoning: ReasoningEffort,
        use_chat_model: bool,
        review_enabled: bool,
        agents_enabled: bool,
        cross_check_enabled: bool,
        qa_automation_enabled: bool,
        model_routing_enabled: bool,
        model_routing_entries: Vec<AutoDriveModelRoutingEntry>,
        routing_model_options: Vec<String>,
        continue_mode: AutoContinueMode,
    ) -> Self {
        let diagnostics_enabled = qa_automation_enabled && (review_enabled || cross_check_enabled);
        let normalized_entries = Self::sanitize_routing_entries(model_routing_entries);
        let model_routing_entries = if normalized_entries.is_empty() {
            default_auto_drive_model_routing_entries()
        } else {
            normalized_entries
        };
        let routing_model_options =
            Self::build_routing_model_options(routing_model_options, &model_routing_entries);

        Self {
            app_event_tx,
            selected_index: 0,
            mode: AutoDriveSettingsMode::Main,
            model,
            model_reasoning,
            use_chat_model,
            review_enabled,
            agents_enabled,
            cross_check_enabled,
            qa_automation_enabled,
            diagnostics_enabled,
            model_routing_enabled,
            model_routing_entries,
            routing_model_options,
            routing_selected_index: 0,
            continue_mode,
            status_message: None,
            closing: false,
        }
    }

    fn option_count() -> usize {
        6
    }

    fn routing_row_count(&self) -> usize {
        self.model_routing_entries.len().saturating_add(1)
    }

    fn enabled_routing_entry_count(&self) -> usize {
        self.model_routing_entries
            .iter()
            .filter(|entry| entry.enabled)
            .count()
    }

    fn default_routing_model() -> String {
        "gpt-5.3-codex".to_string()
    }

    fn normalize_routing_model(model: &str) -> Option<String> {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            return None;
        }

        let normalized = trimmed.to_ascii_lowercase();
        if !normalized.starts_with("gpt-") {
            return None;
        }

        Some(normalized)
    }

    fn normalize_routing_reasoning_levels(levels: &[ReasoningEffort]) -> Vec<ReasoningEffort> {
        let mut normalized = Vec::new();
        for level in ROUTING_REASONING_LEVELS {
            if levels.contains(&level) {
                normalized.push(level);
            }
        }
        normalized
    }

    fn sanitize_routing_entries(
        entries: Vec<AutoDriveModelRoutingEntry>,
    ) -> Vec<AutoDriveModelRoutingEntry> {
        let mut normalized_entries = Vec::new();
        for entry in entries {
            let Some(model) = Self::normalize_routing_model(&entry.model) else {
                continue;
            };
            let reasoning_levels = Self::normalize_routing_reasoning_levels(&entry.reasoning_levels);
            if reasoning_levels.is_empty() {
                continue;
            }

            normalized_entries.push(AutoDriveModelRoutingEntry {
                model,
                enabled: entry.enabled,
                reasoning_levels,
                description: entry.description.trim().to_string(),
            });
        }
        normalized_entries
    }

    fn build_routing_model_options(
        model_options: Vec<String>,
        entries: &[AutoDriveModelRoutingEntry],
    ) -> Vec<String> {
        let mut normalized = Vec::new();
        for model in model_options {
            let Some(canonical) = Self::normalize_routing_model(&model) else {
                continue;
            };
            if !normalized.contains(&canonical) {
                normalized.push(canonical);
            }
        }

        for entry in entries {
            if !normalized.contains(&entry.model) {
                normalized.push(entry.model.clone());
            }
        }

        if normalized.is_empty() {
            normalized.push(Self::default_routing_model());
        }

        normalized
    }

    fn route_entry_summary(entry: &AutoDriveModelRoutingEntry) -> String {
        let levels = entry
            .reasoning_levels
            .iter()
            .map(|level| Self::reasoning_label(*level).to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join("/");
        let description = if entry.description.trim().is_empty() {
            "(no description)".to_string()
        } else {
            entry.description.trim().to_string()
        };
        format!("{} · {levels} · {description}", entry.model)
    }

    fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    fn clear_status_message(&mut self) {
        self.status_message = None;
    }

    fn send_update(&self) {
        self.app_event_tx.send(AppEvent::AutoDriveSettingsChanged {
            review_enabled: self.review_enabled,
            agents_enabled: self.agents_enabled,
            cross_check_enabled: self.cross_check_enabled,
            qa_automation_enabled: self.qa_automation_enabled,
            model_routing_enabled: self.model_routing_enabled,
            model_routing_entries: self.model_routing_entries.clone(),
            continue_mode: self.continue_mode,
        });
    }

    pub fn set_model(&mut self, model: String, effort: ReasoningEffort) {
        self.model = model;
        self.model_reasoning = effort;
    }

    pub fn set_use_chat_model(&mut self, use_chat: bool, model: String, effort: ReasoningEffort) {
        self.use_chat_model = use_chat;
        if use_chat {
            self.model = model;
            self.model_reasoning = effort;
        }
    }

    fn set_diagnostics(&mut self, enabled: bool) {
        self.review_enabled = enabled;
        self.cross_check_enabled = enabled;
        self.qa_automation_enabled = enabled;
        self.diagnostics_enabled = self.qa_automation_enabled && (self.review_enabled || self.cross_check_enabled);
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

    fn cycle_continue_mode(&mut self, forward: bool) {
        self.continue_mode = if forward {
            self.continue_mode.cycle_forward()
        } else {
            self.continue_mode.cycle_backward()
        };
        self.send_update();
    }

    fn open_routing_list(&mut self) {
        self.mode = AutoDriveSettingsMode::RoutingList;
        let rows = self.routing_row_count();
        if self.routing_selected_index >= rows {
            self.routing_selected_index = rows.saturating_sub(1);
        }
        self.clear_status_message();
    }

    fn open_routing_editor(&mut self, index: Option<usize>) {
        let entry = index.and_then(|idx| self.model_routing_entries.get(idx));
        let state = RoutingEditorState::from_entry(index, entry, &self.routing_model_options);
        self.mode = AutoDriveSettingsMode::RoutingEditor(state);
        self.clear_status_message();
    }

    fn close_routing_editor(&mut self) {
        self.mode = AutoDriveSettingsMode::RoutingList;
        self.clear_status_message();
    }

    fn try_toggle_routing_entry_enabled(&mut self, index: usize) {
        let Some(entry) = self.model_routing_entries.get(index).cloned() else {
            return;
        };

        if entry.enabled && self.model_routing_enabled && self.enabled_routing_entry_count() <= 1 {
            self.set_status_message("At least one routing entry must stay enabled.");
            return;
        }

        if let Some(target) = self.model_routing_entries.get_mut(index) {
            target.enabled = !target.enabled;
            self.send_update();
            self.clear_status_message();
        }
    }

    fn try_remove_routing_entry(&mut self, index: usize) {
        let Some(entry) = self.model_routing_entries.get(index).cloned() else {
            return;
        };

        if self.model_routing_enabled && entry.enabled && self.enabled_routing_entry_count() <= 1 {
            self.set_status_message("At least one routing entry must stay enabled.");
            return;
        }

        self.model_routing_entries.remove(index);
        if self.model_routing_entries.is_empty() {
            self.model_routing_entries = default_auto_drive_model_routing_entries();
        }
        let max_row = self.routing_row_count().saturating_sub(1);
        self.routing_selected_index = self.routing_selected_index.min(max_row);
        self.send_update();
        self.clear_status_message();
    }

    fn try_set_model_routing_enabled(&mut self, enabled: bool) {
        if enabled && self.enabled_routing_entry_count() == 0 {
            self.set_status_message("Enable at least one routing entry before turning routing on.");
            return;
        }
        self.model_routing_enabled = enabled;
        self.send_update();
        self.clear_status_message();
    }

    fn toggle_selected(&mut self) {
        match self.selected_index {
            0 => {
                self.app_event_tx.send(AppEvent::ShowAutoDriveModelSelector);
            }
            1 => {
                self.agents_enabled = !self.agents_enabled;
                self.send_update();
            }
            2 => {
                let next = !self.diagnostics_enabled;
                self.set_diagnostics(next);
                self.send_update();
            }
            3 => {
                self.try_set_model_routing_enabled(!self.model_routing_enabled);
            }
            4 => {
                self.open_routing_list();
            }
            5 => self.cycle_continue_mode(true),
            _ => {}
        }
    }

    fn close(&mut self) {
        if !self.closing {
            self.closing = true;
            self.app_event_tx.send(AppEvent::CloseAutoDriveSettings);
        }
    }

    fn option_label(&self, index: usize) -> Line<'static> {
        let selected = index == self.selected_index;
        let indicator = if selected { "›" } else { " " };
        let prefix = format!("{indicator} ");
        let (label, enabled) = match index {
            0 => ("Auto Drive model", true),
            1 => (
                "Agents enabled (uses multiple agents to speed up complex tasks)",
                self.agents_enabled,
            ),
            2 => (
                "Diagnostics enabled (monitors and adjusts system in real time)",
                self.diagnostics_enabled,
            ),
            3 => (
                "Coordinator model routing (choose model + reasoning per turn)",
                self.model_routing_enabled,
            ),
            4 => ("Routing entries (add/remove/edit per-model routes)", true),
            5 => (
                "Auto-continue delay",
                matches!(self.continue_mode, AutoContinueMode::Manual),
            ),
            _ => ("", false),
        };

        let label_style = if selected {
            Style::default()
                .fg(colors::primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::text())
        };

        let mut spans = vec![Span::styled(prefix, label_style)];
        match index {
            0 => {
                if self.use_chat_model {
                    spans.push(Span::styled("Follow Chat Mode", label_style));
                    if selected {
                        spans.push(Span::raw("  (Enter to change)"));
                    }
                } else {
                    let model_label = self.model.trim();
                    let display = if model_label.is_empty() {
                        "(not set)".to_string()
                    } else {
                        format!(
                            "{} · {}",
                            Self::format_model_label(model_label),
                            Self::reasoning_label(self.model_reasoning)
                        )
                    };
                    spans.push(Span::styled(display, label_style));
                    if selected {
                        spans.push(Span::raw("  (Enter to change)"));
                    }
                }
            }
            1 | 2 | 3 => {
                let checkbox = if enabled { "[x]" } else { "[ ]" };
                spans.push(Span::styled(format!("{checkbox} {label}"), label_style));
            }
            4 => {
                let count = self.model_routing_entries.len();
                let enabled_count = self.enabled_routing_entry_count();
                spans.push(Span::styled(label.to_string(), label_style));
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    format!("{enabled_count}/{count} enabled"),
                    Style::default()
                        .fg(colors::text_dim())
                        .add_modifier(if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ));
                if selected {
                    spans.push(Span::raw("  (Enter to edit)"));
                }
            }
            5 => {
                spans.push(Span::styled(label.to_string(), label_style));
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    self.continue_mode.label().to_string(),
                    Style::default()
                        .fg(colors::text_dim())
                        .add_modifier(if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ));
            }
            _ => {}
        }

        Line::from(spans)
    }

    fn info_lines_main(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        for idx in 0..Self::option_count() {
            lines.push(self.option_label(idx));
        }
        lines.push(Line::default());

        if let Some(message) = self.status_message.as_deref() {
            lines.push(Line::from(Span::styled(
                message.to_string(),
                Style::default().fg(colors::warning()),
            )));
            lines.push(Line::default());
        }

        let footer_style = Style::default().fg(colors::text_dim());
        lines.push(Line::from(vec![
            Span::styled("Enter", Style::default().fg(colors::primary())),
            Span::styled(" select/toggle", footer_style),
            Span::raw("   "),
            Span::styled("←/→", Style::default().fg(colors::primary())),
            Span::styled(" adjust delay", footer_style),
            Span::raw("   "),
            Span::styled("Esc", Style::default().fg(colors::primary())),
            Span::styled(" close", footer_style),
            Span::raw("   "),
            Span::styled("Ctrl+S", Style::default().fg(colors::primary())),
            Span::styled(" close", footer_style),
        ]));

        lines
    }

    fn info_lines_routing_list(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        lines.push(Line::from(Span::styled(
            "Routing entries",
            Style::default()
                .fg(colors::primary())
                .add_modifier(Modifier::BOLD),
        )));

        for (idx, entry) in self.model_routing_entries.iter().enumerate() {
            let selected = self.routing_selected_index == idx;
            let prefix = if selected { "› " } else { "  " };
            let style = if selected {
                Style::default()
                    .fg(colors::primary())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::text())
            };
            let checkbox = if entry.enabled { "[x]" } else { "[ ]" };
            lines.push(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("{checkbox} {}", Self::route_entry_summary(entry)), style),
            ]));
        }

        let add_idx = self.model_routing_entries.len();
        let add_selected = self.routing_selected_index == add_idx;
        let add_style = if add_selected {
            Style::default()
                .fg(colors::primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::text())
        };
        let add_prefix = if add_selected { "› " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(add_prefix, add_style),
            Span::styled("+ Add routing entry", add_style),
        ]));

        lines.push(Line::default());
        if let Some(message) = self.status_message.as_deref() {
            lines.push(Line::from(Span::styled(
                message.to_string(),
                Style::default().fg(colors::warning()),
            )));
            lines.push(Line::default());
        }

        let footer_style = Style::default().fg(colors::text_dim());
        lines.push(Line::from(vec![
            Span::styled("Enter", Style::default().fg(colors::primary())),
            Span::styled(" edit/add", footer_style),
            Span::raw("   "),
            Span::styled("Space", Style::default().fg(colors::primary())),
            Span::styled(" toggle enabled", footer_style),
            Span::raw("   "),
            Span::styled("D", Style::default().fg(colors::primary())),
            Span::styled(" remove", footer_style),
            Span::raw("   "),
            Span::styled("Esc", Style::default().fg(colors::primary())),
            Span::styled(" back", footer_style),
        ]));

        lines
    }

    fn info_lines_routing_editor(&self, editor: &RoutingEditorState) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let title = if editor.index.is_some() {
            "Edit routing entry"
        } else {
            "Add routing entry"
        };
        lines.push(Line::from(Span::styled(
            title,
            Style::default()
                .fg(colors::primary())
                .add_modifier(Modifier::BOLD),
        )));

        let model = self
            .routing_model_options
            .get(editor.model_cursor)
            .cloned()
            .unwrap_or_else(Self::default_routing_model);
        lines.push(self.editor_row(
            editor,
            RoutingEditorField::Model,
            format!("Model: {model}"),
        ));

        lines.push(self.editor_row(
            editor,
            RoutingEditorField::Enabled,
            format!("Enabled: {}", if editor.enabled { "Yes" } else { "No" }),
        ));

        let reasoning_parts = ROUTING_REASONING_LEVELS
            .iter()
            .enumerate()
            .map(|(idx, level)| {
                let cursor = if editor.reasoning_cursor == idx {
                    ">"
                } else {
                    " "
                };
                let checkbox = if editor.reasoning_enabled[idx] {
                    "[x]"
                } else {
                    "[ ]"
                };
                format!("{cursor}{checkbox}{}", Self::reasoning_label(*level).to_ascii_lowercase())
            })
            .collect::<Vec<_>>()
            .join("  ");
        lines.push(self.editor_row(
            editor,
            RoutingEditorField::Reasoning,
            format!("Reasoning: {reasoning_parts}"),
        ));

        let description = if editor.description.trim().is_empty() {
            "(empty)".to_string()
        } else {
            editor.description.clone()
        };
        lines.push(self.editor_row(
            editor,
            RoutingEditorField::Description,
            format!("Description: {description}"),
        ));

        let save_selected = editor.selected_field == RoutingEditorField::Save;
        let cancel_selected = editor.selected_field == RoutingEditorField::Cancel;
        let save_style = if save_selected {
            Style::default()
                .fg(colors::primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::text())
        };
        let cancel_style = if cancel_selected {
            Style::default()
                .fg(colors::primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::text())
        };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(if save_selected { "› Save" } else { "  Save" }, save_style),
            Span::raw("    "),
            Span::styled(
                if cancel_selected { "› Cancel" } else { "  Cancel" },
                cancel_style,
            ),
        ]));

        lines.push(Line::default());
        if let Some(message) = self.status_message.as_deref() {
            lines.push(Line::from(Span::styled(
                message.to_string(),
                Style::default().fg(colors::warning()),
            )));
            lines.push(Line::default());
        }

        let footer_style = Style::default().fg(colors::text_dim());
        lines.push(Line::from(vec![
            Span::styled("Tab", Style::default().fg(colors::primary())),
            Span::styled(" next field", footer_style),
            Span::raw("   "),
            Span::styled("Space", Style::default().fg(colors::primary())),
            Span::styled(" toggle", footer_style),
            Span::raw("   "),
            Span::styled("Enter", Style::default().fg(colors::primary())),
            Span::styled(" save/activate", footer_style),
            Span::raw("   "),
            Span::styled("Esc", Style::default().fg(colors::primary())),
            Span::styled(" back", footer_style),
        ]));

        lines
    }

    fn editor_row(
        &self,
        editor: &RoutingEditorState,
        field: RoutingEditorField,
        text: String,
    ) -> Line<'static> {
        let selected = editor.selected_field == field;
        let prefix = if selected { "› " } else { "  " };
        let style = if selected {
            Style::default()
                .fg(colors::primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors::text())
        };
        Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(text, style),
        ])
    }

    fn info_lines(&self) -> Vec<Line<'static>> {
        match &self.mode {
            AutoDriveSettingsMode::Main => self.info_lines_main(),
            AutoDriveSettingsMode::RoutingList => self.info_lines_routing_list(),
            AutoDriveSettingsMode::RoutingEditor(editor) => self.info_lines_routing_editor(editor),
        }
    }

    fn render_panel_body(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        Paragraph::new(self.info_lines())
            .wrap(Wrap { trim: true })
            .style(Style::default().bg(colors::background()).fg(colors::text()))
            .render(area, buf);
    }

    pub(crate) fn render_without_frame(&self, area: Rect, buf: &mut Buffer) {
        self.render_panel_body(area, buf);
    }

    fn handle_main_key(&mut self, key_event: KeyEvent) -> bool {
        match key_event.code {
            KeyCode::Esc => {
                self.close();
                true
            }
            KeyCode::Up => {
                if self.selected_index == 0 {
                    self.selected_index = Self::option_count() - 1;
                } else {
                    self.selected_index -= 1;
                }
                true
            }
            KeyCode::Down => {
                self.selected_index = (self.selected_index + 1) % Self::option_count();
                true
            }
            KeyCode::Left => {
                if self.selected_index == 5 {
                    self.cycle_continue_mode(false);
                    true
                } else {
                    false
                }
            }
            KeyCode::Right => {
                if self.selected_index == 5 {
                    self.cycle_continue_mode(true);
                    true
                } else {
                    false
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.toggle_selected();
                true
            }
            _ => false,
        }
    }

    fn handle_routing_list_key(&mut self, key_event: KeyEvent) -> bool {
        match key_event.code {
            KeyCode::Esc => {
                self.mode = AutoDriveSettingsMode::Main;
                self.clear_status_message();
                true
            }
            KeyCode::Up => {
                let total = self.routing_row_count();
                if self.routing_selected_index == 0 {
                    self.routing_selected_index = total.saturating_sub(1);
                } else {
                    self.routing_selected_index -= 1;
                }
                true
            }
            KeyCode::Down => {
                let total = self.routing_row_count();
                self.routing_selected_index = (self.routing_selected_index + 1) % total;
                true
            }
            KeyCode::Enter => {
                if self.routing_selected_index >= self.model_routing_entries.len() {
                    self.open_routing_editor(None);
                } else {
                    self.open_routing_editor(Some(self.routing_selected_index));
                }
                true
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.open_routing_editor(None);
                true
            }
            KeyCode::Char(' ') => {
                if self.routing_selected_index < self.model_routing_entries.len() {
                    self.try_toggle_routing_entry_enabled(self.routing_selected_index);
                    true
                } else {
                    false
                }
            }
            KeyCode::Delete | KeyCode::Backspace | KeyCode::Char('d') | KeyCode::Char('D') => {
                if self.routing_selected_index < self.model_routing_entries.len() {
                    self.try_remove_routing_entry(self.routing_selected_index);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn save_routing_editor(&mut self) {
        let AutoDriveSettingsMode::RoutingEditor(editor) = &self.mode else {
            return;
        };
        let editor = editor.clone();

        let model = self
            .routing_model_options
            .get(editor.model_cursor)
            .cloned()
            .unwrap_or_else(Self::default_routing_model);
        let reasoning_levels = editor.selected_reasoning_levels();
        if reasoning_levels.is_empty() {
            self.set_status_message("Select at least one reasoning level.");
            return;
        }

        let entry = AutoDriveModelRoutingEntry {
            model,
            enabled: editor.enabled,
            reasoning_levels,
            description: editor.description.trim().to_string(),
        };

        let mut updated_entries = self.model_routing_entries.clone();
        if let Some(index) = editor.index {
            if let Some(slot) = updated_entries.get_mut(index) {
                *slot = entry;
            } else {
                updated_entries.push(entry);
            }
        } else {
            updated_entries.push(entry);
        }

        let sanitized = Self::sanitize_routing_entries(updated_entries);
        if sanitized.is_empty() {
            self.set_status_message("At least one valid gpt-* routing entry is required.");
            return;
        }
        if self.model_routing_enabled && !sanitized.iter().any(|entry| entry.enabled) {
            self.set_status_message("At least one routing entry must stay enabled.");
            return;
        }

        self.model_routing_entries = sanitized;
        let row_count = self.routing_row_count();
        self.routing_selected_index = editor
            .index
            .unwrap_or_else(|| self.model_routing_entries.len().saturating_sub(1))
            .min(row_count.saturating_sub(1));
        self.mode = AutoDriveSettingsMode::RoutingList;
        self.send_update();
        self.clear_status_message();
    }

    fn update_routing_editor<F>(&mut self, updater: F)
    where
        F: FnOnce(&mut RoutingEditorState),
    {
        if let AutoDriveSettingsMode::RoutingEditor(editor) = &mut self.mode {
            updater(editor);
        }
    }

    fn handle_routing_editor_key(&mut self, key_event: KeyEvent) -> bool {
        match key_event.code {
            KeyCode::Esc => {
                self.close_routing_editor();
                return true;
            }
            KeyCode::Tab => {
                self.update_routing_editor(|editor| {
                    editor.selected_field = editor.selected_field.next();
                });
                return true;
            }
            KeyCode::BackTab => {
                self.update_routing_editor(|editor| {
                    editor.selected_field = editor.selected_field.previous();
                });
                return true;
            }
            KeyCode::Up => {
                self.update_routing_editor(|editor| {
                    editor.selected_field = editor.selected_field.previous();
                });
                return true;
            }
            KeyCode::Down => {
                self.update_routing_editor(|editor| {
                    editor.selected_field = editor.selected_field.next();
                });
                return true;
            }
            _ => {}
        }

        let mut handled = false;
        let mut request_save = false;
        let mut request_cancel = false;
        let has_models = !self.routing_model_options.is_empty();
        let model_options_len = self.routing_model_options.len();

        self.update_routing_editor(|editor| match editor.selected_field {
            RoutingEditorField::Model => match key_event.code {
                KeyCode::Left => {
                    if has_models {
                        if editor.model_cursor == 0 {
                            editor.model_cursor = model_options_len.saturating_sub(1);
                        } else {
                            editor.model_cursor -= 1;
                        }
                        handled = true;
                    }
                }
                KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ') => {
                    if has_models {
                        editor.model_cursor = (editor.model_cursor + 1) % model_options_len;
                        handled = true;
                    }
                }
                _ => {}
            },
            RoutingEditorField::Enabled => {
                if matches!(
                    key_event.code,
                    KeyCode::Left | KeyCode::Right | KeyCode::Enter | KeyCode::Char(' ')
                ) {
                    editor.enabled = !editor.enabled;
                    handled = true;
                }
            }
            RoutingEditorField::Reasoning => match key_event.code {
                KeyCode::Left => {
                    if editor.reasoning_cursor == 0 {
                        editor.reasoning_cursor = ROUTING_REASONING_LEVELS.len().saturating_sub(1);
                    } else {
                        editor.reasoning_cursor -= 1;
                    }
                    handled = true;
                }
                KeyCode::Right => {
                    editor.reasoning_cursor =
                        (editor.reasoning_cursor + 1) % ROUTING_REASONING_LEVELS.len();
                    handled = true;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    editor.toggle_reasoning_at_cursor();
                    handled = true;
                }
                _ => {}
            },
            RoutingEditorField::Description => match key_event.code {
                KeyCode::Backspace => {
                    editor.description.pop();
                    handled = true;
                }
                KeyCode::Char(c) => {
                    if !key_event.modifiers.contains(KeyModifiers::CONTROL)
                        && !key_event.modifiers.contains(KeyModifiers::ALT)
                        && editor.description.chars().count() < ROUTING_DESCRIPTION_MAX_CHARS
                    {
                        editor.description.push(c);
                        handled = true;
                    }
                }
                _ => {}
            },
            RoutingEditorField::Save => {
                if matches!(key_event.code, KeyCode::Enter | KeyCode::Char(' ')) {
                    request_save = true;
                    handled = true;
                }
            }
            RoutingEditorField::Cancel => {
                if matches!(key_event.code, KeyCode::Enter | KeyCode::Char(' ')) {
                    request_cancel = true;
                    handled = true;
                }
            }
        });

        if request_save {
            self.save_routing_editor();
            return true;
        }
        if request_cancel {
            self.close_routing_editor();
            return true;
        }

        handled
    }

    fn handle_key_event_internal(&mut self, key_event: KeyEvent) -> bool {
        if key_event.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key_event.code, KeyCode::Char('s') | KeyCode::Char('S'))
        {
            self.close();
            return true;
        }

        let mode = self.mode.clone();
        match mode {
            AutoDriveSettingsMode::Main => self.handle_main_key(key_event),
            AutoDriveSettingsMode::RoutingList => self.handle_routing_list_key(key_event),
            AutoDriveSettingsMode::RoutingEditor(_) => self.handle_routing_editor_key(key_event),
        }
    }

    pub fn handle_key_event_direct(&mut self, key_event: KeyEvent) {
        if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return;
        }
        if self.handle_key_event_internal(key_event) {
            self.app_event_tx.send(AppEvent::RequestRedraw);
        }
    }

    pub fn handle_paste(&mut self, text: String) -> bool {
        let mut handled = false;
        self.update_routing_editor(|editor| {
            if editor.selected_field == RoutingEditorField::Description {
                let remaining = ROUTING_DESCRIPTION_MAX_CHARS.saturating_sub(editor.description.chars().count());
                if remaining > 0 {
                    let sanitized = text.replace(['\r', '\n'], " ");
                    let insert: String = sanitized.chars().take(remaining).collect();
                    if !insert.is_empty() {
                        editor.description.push_str(&insert);
                        handled = true;
                    }
                }
            }
        });

        if handled {
            self.app_event_tx.send(AppEvent::RequestRedraw);
        }
        handled
    }

    pub fn is_view_complete(&self) -> bool {
        self.closing
    }
}

impl<'a> BottomPaneView<'a> for AutoDriveSettingsView {
    fn handle_key_event(&mut self, pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return;
        }

        if self.handle_key_event_internal(key_event) {
            pane.request_redraw();
        }
    }

    fn desired_height(&self, _width: u16) -> u16 {
        match &self.mode {
            AutoDriveSettingsMode::Main => 12,
            AutoDriveSettingsMode::RoutingList => 14,
            AutoDriveSettingsMode::RoutingEditor(_) => 16,
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        render_panel(
            area,
            buf,
            Self::PANEL_TITLE,
            PanelFrameStyle::bottom_pane(),
            |inner, buf| self.render_panel_body(inner, buf),
        );
    }

    fn update_status_text(&mut self, _text: String) -> ConditionalUpdate {
        ConditionalUpdate::NoRedraw
    }

    fn is_complete(&self) -> bool {
        self.closing
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn build_view(
        model_routing_enabled: bool,
        entries: Vec<AutoDriveModelRoutingEntry>,
    ) -> AutoDriveSettingsView {
        let (tx, _rx) = channel();
        AutoDriveSettingsView::new(
            AppEventSender::new(tx),
            "gpt-5.3-codex".to_string(),
            ReasoningEffort::High,
            false,
            true,
            true,
            true,
            true,
            model_routing_enabled,
            entries,
            vec!["gpt-5.3-codex".to_string(), "gpt-5.3-codex-spark".to_string()],
            AutoContinueMode::Manual,
        )
    }

    #[test]
    fn routing_list_keeps_one_entry_enabled_when_routing_on() {
        let mut view = build_view(
            true,
            vec![AutoDriveModelRoutingEntry {
                model: "gpt-5.3-codex".to_string(),
                enabled: true,
                reasoning_levels: vec![ReasoningEffort::High],
                description: String::new(),
            }],
        );

        for _ in 0..4 {
            view.handle_key_event_direct(key(KeyCode::Down));
        }
        view.handle_key_event_direct(key(KeyCode::Enter));
        view.handle_key_event_direct(key(KeyCode::Char(' ')));

        assert!(view.model_routing_entries[0].enabled);
    }

    #[test]
    fn routing_list_supports_add_and_save_entry() {
        let mut view = build_view(
            true,
            vec![AutoDriveModelRoutingEntry {
                model: "gpt-5.3-codex".to_string(),
                enabled: true,
                reasoning_levels: vec![ReasoningEffort::High],
                description: String::new(),
            }],
        );

        for _ in 0..4 {
            view.handle_key_event_direct(key(KeyCode::Down));
        }
        view.handle_key_event_direct(key(KeyCode::Enter));
        view.handle_key_event_direct(key(KeyCode::Char('a')));

        for _ in 0..3 {
            view.handle_key_event_direct(key(KeyCode::Down));
        }
        for ch in "fast loop".chars() {
            view.handle_key_event_direct(key(KeyCode::Char(ch)));
        }
        view.handle_key_event_direct(key(KeyCode::Down));
        view.handle_key_event_direct(key(KeyCode::Enter));

        assert_eq!(view.model_routing_entries.len(), 2);
        assert_eq!(view.model_routing_entries[1].description, "fast loop");
    }

    #[test]
    fn routing_toggle_requires_enabled_entry_before_turning_on() {
        let mut view = build_view(
            false,
            vec![AutoDriveModelRoutingEntry {
                model: "gpt-5.3-codex".to_string(),
                enabled: false,
                reasoning_levels: vec![ReasoningEffort::High],
                description: String::new(),
            }],
        );

        for _ in 0..3 {
            view.handle_key_event_direct(key(KeyCode::Down));
        }
        view.handle_key_event_direct(key(KeyCode::Enter));

        assert!(!view.model_routing_enabled);
        assert_eq!(
            view.status_message.as_deref(),
            Some("Enable at least one routing entry before turning routing on.")
        );
    }

    #[test]
    fn routing_editor_rejects_save_without_reasoning() {
        let mut view = build_view(
            true,
            vec![AutoDriveModelRoutingEntry {
                model: "gpt-5.3-codex".to_string(),
                enabled: true,
                reasoning_levels: vec![ReasoningEffort::High],
                description: String::new(),
            }],
        );

        for _ in 0..4 {
            view.handle_key_event_direct(key(KeyCode::Down));
        }
        view.handle_key_event_direct(key(KeyCode::Enter));
        view.handle_key_event_direct(key(KeyCode::Char('a')));

        view.handle_key_event_direct(key(KeyCode::Down));
        view.handle_key_event_direct(key(KeyCode::Down));
        for _ in 0..3 {
            view.handle_key_event_direct(key(KeyCode::Right));
        }
        view.handle_key_event_direct(key(KeyCode::Enter));
        view.handle_key_event_direct(key(KeyCode::Down));
        view.handle_key_event_direct(key(KeyCode::Down));
        view.handle_key_event_direct(key(KeyCode::Enter));

        assert!(matches!(view.mode, AutoDriveSettingsMode::RoutingEditor(_)));
        assert_eq!(view.model_routing_entries.len(), 1);
        assert_eq!(
            view.status_message.as_deref(),
            Some("Select at least one reasoning level.")
        );
    }

    #[test]
    fn routing_editor_cannot_disable_last_enabled_entry_when_routing_on() {
        let mut view = build_view(
            true,
            vec![AutoDriveModelRoutingEntry {
                model: "gpt-5.3-codex".to_string(),
                enabled: true,
                reasoning_levels: vec![ReasoningEffort::High],
                description: String::new(),
            }],
        );

        for _ in 0..4 {
            view.handle_key_event_direct(key(KeyCode::Down));
        }
        view.handle_key_event_direct(key(KeyCode::Enter));
        view.handle_key_event_direct(key(KeyCode::Enter));

        view.handle_key_event_direct(key(KeyCode::Down));
        view.handle_key_event_direct(key(KeyCode::Enter));
        view.handle_key_event_direct(key(KeyCode::Down));
        view.handle_key_event_direct(key(KeyCode::Down));
        view.handle_key_event_direct(key(KeyCode::Down));
        view.handle_key_event_direct(key(KeyCode::Enter));

        assert!(matches!(view.mode, AutoDriveSettingsMode::RoutingEditor(_)));
        assert!(view.model_routing_entries[0].enabled);
        assert_eq!(
            view.status_message.as_deref(),
            Some("At least one routing entry must stay enabled.")
        );
    }
}
