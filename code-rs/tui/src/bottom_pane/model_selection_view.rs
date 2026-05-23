use super::bottom_pane_view::BottomPaneView;
use super::settings_panel::{render_panel, PanelFrameStyle};
use super::BottomPane;
use crate::app_event::{AppEvent, ModelSelectionKind};
use crate::app_event_sender::AppEventSender;
use code_common::model_presets::ModelPreset;
use code_core::config_types::ContextMode;
use code_core::config_types::ReasoningEffort;
use code_core::config_types::ServiceTier;
use code_core::model_family::supports_extended_context;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::cell::Cell;
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

#[derive(Clone)]
struct ModelLine {
    line: Line<'static>,
    is_selected: bool,
}

impl FlatPreset {
    fn from_model_preset(preset: &ModelPreset) -> Vec<Self> {
        preset
            .supported_reasoning_efforts
            .iter()
            .map(|effort_preset| {
                FlatPreset {
                    model: preset.model.to_string(),
                    effort: effort_preset.effort.into(),
                    label: format!(
                        "{} {}",
                        preset.display_name,
                        Self::effort_label(effort_preset.effort.into()).to_lowercase()
                    ),
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

    fn supports_fast_mode(self) -> bool {
        matches!(self, ModelSelectionTarget::Session)
    }
}

pub(crate) struct ModelSelectionView {
    flat_presets: Vec<FlatPreset>,
    selected_index: usize,
    scroll_top: Cell<usize>,
    current_model: String,
    current_effort: ReasoningEffort,
    current_service_tier: Option<ServiceTier>,
    current_context_mode: Option<ContextMode>,
    use_chat_model: bool,
    app_event_tx: AppEventSender,
    is_complete: bool,
    target: ModelSelectionTarget,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum EntryKind {
    FastMode,
    ContextMode,
    FollowChat,
    Preset(usize),
}

impl ModelSelectionView {
    pub fn new(
        presets: Vec<ModelPreset>,
        current_model: String,
        current_effort: ReasoningEffort,
        current_service_tier: Option<ServiceTier>,
        current_context_mode: Option<ContextMode>,
        use_chat_model: bool,
        target: ModelSelectionTarget,
        app_event_tx: AppEventSender,
    ) -> Self {
        let flat_presets: Vec<FlatPreset> = presets
            .iter()
            .flat_map(FlatPreset::from_model_preset)
            .collect();

        let initial_index = Self::initial_selection(
            target.supports_fast_mode(),
            target.supports_fast_mode(),
            target.supports_follow_chat(),
            use_chat_model,
            &flat_presets,
            &current_model,
            current_effort,
        );
        Self {
            flat_presets,
            selected_index: initial_index,
            scroll_top: Cell::new(0),
            current_model,
            current_effort,
            current_service_tier,
            current_context_mode,
            use_chat_model,
            app_event_tx,
            is_complete: false,
            target,
        }
    }

    pub(crate) fn update_presets(&mut self, presets: Vec<ModelPreset>) {
        let include_fast_mode = self.target.supports_fast_mode();
        let include_context_mode = self.target.supports_fast_mode();
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
            Some(EntryKind::FastMode) => {
                if include_fast_mode {
                    next_selected = Some(0);
                }
            }
            Some(EntryKind::ContextMode) => {
                if include_context_mode {
                    next_selected = Some(usize::from(include_fast_mode));
                }
            }
            Some(EntryKind::FollowChat) => {
                if include_follow_chat {
                    next_selected = Some(
                        usize::from(include_fast_mode)
                            + usize::from(include_context_mode),
                    );
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
                        let prefix = usize::from(include_fast_mode)
                            + usize::from(include_context_mode)
                            + usize::from(include_follow_chat);
                        next_selected = Some(new_idx + prefix);
                    }
                }
            }
            None => {}
        }

        self.selected_index = next_selected.unwrap_or_else(|| {
            Self::initial_selection(
                include_fast_mode,
                include_context_mode,
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
        self.scroll_top.set(0);
    }

    fn content_scroll_top(
        &self,
        rows: usize,
        selected_row: usize,
        visible_rows: usize,
    ) -> usize {
        if visible_rows == 0 || rows <= visible_rows {
            return 0;
        }

        let max_start = rows.saturating_sub(visible_rows);

        if self.selected_index == 0 {
            // Returning to the first selectable entry should show the top context
            // whenever it still keeps the selected row visible. In compact
            // panes (e.g. Follow Chat), keep enough scroll so the selection
            // remains on screen.
            if selected_row < visible_rows {
                return 0;
            }
            return selected_row
                .saturating_sub(visible_rows.saturating_sub(1))
                .min(max_start);
        }

        let mut top = self.scroll_top.get().min(max_start);

        if selected_row < top {
            top = selected_row;
        }

        if selected_row >= top.saturating_add(visible_rows) {
            top = selected_row.saturating_sub(visible_rows - 1);
        }

        top.min(max_start)
    }

    fn initial_selection(
        include_fast_mode: bool,
        include_context_mode: bool,
        include_follow_chat: bool,
        use_chat_model: bool,
        flat_presets: &[FlatPreset],
        current_model: &str,
        current_effort: ReasoningEffort,
    ) -> usize {
        if include_fast_mode {
            return 0;
        }

        if include_follow_chat && use_chat_model {
            return 0;
        }

        if let Some((idx, _)) = flat_presets.iter().enumerate().find(|(_, preset)| {
            preset.model.eq_ignore_ascii_case(current_model) && preset.effort == current_effort
        }) {
            return idx
                + usize::from(include_context_mode)
                + usize::from(include_follow_chat);
        }

        if let Some((idx, _)) = flat_presets
            .iter()
            .enumerate()
            .find(|(_, preset)| preset.model.eq_ignore_ascii_case(current_model))
        {
            return idx
                + usize::from(include_context_mode)
                + usize::from(include_follow_chat);
        }

        if include_context_mode || include_follow_chat {
            if flat_presets.is_empty() {
                0
            } else {
                usize::from(include_context_mode) + usize::from(include_follow_chat)
            }
        } else {
            0
        }
    }

    fn supports_extended_context(&self) -> bool {
        supports_extended_context(&self.current_model)
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
        if self.target.supports_fast_mode() {
            entries.push(EntryKind::FastMode);
        }
        if self.target.supports_fast_mode() {
            entries.push(EntryKind::ContextMode);
        }
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
                EntryKind::FastMode => {
                    let next_service_tier = if matches!(self.current_service_tier, Some(ServiceTier::Fast)) {
                        None
                    } else {
                        Some(ServiceTier::Fast)
                    };
                    self.current_service_tier = next_service_tier;
                    let _ = self.app_event_tx.send(AppEvent::UpdateServiceTierSelection {
                        service_tier: next_service_tier,
                    });
                    return;
                }
                EntryKind::ContextMode => {
                    let next_context_mode = match self.current_context_mode {
                        None | Some(ContextMode::Disabled) => Some(ContextMode::OneM),
                        Some(ContextMode::OneM) => Some(ContextMode::Auto),
                        Some(ContextMode::Auto) => Some(ContextMode::Disabled),
                    };
                    self.current_context_mode = next_context_mode;
                    let _ = self.app_event_tx.send(AppEvent::UpdateSessionContextModeSelection {
                        context_mode: next_context_mode,
                    });
                    return;
                }
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

    fn rendered_rows(&self) -> Vec<ModelLine> {
        let mut lines = Vec::new();

        lines.push(ModelLine {
            line: Line::from(vec![
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
            ]),
            is_selected: false,
        });

        lines.push(ModelLine {
            line: Line::from(vec![
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
            ]),
            is_selected: false,
        });

        lines.push(ModelLine {
            line: Line::from(""),
            is_selected: false,
        });

        if self.target.supports_fast_mode() {
            let header_style = Style::default()
                .fg(crate::colors::text_bright())
                .add_modifier(Modifier::BOLD);
            let desc_style = Style::default().fg(crate::colors::text_dim());
            lines.push(ModelLine {
                line: Line::from(vec![Span::styled("Mode Settings", header_style)]),
                is_selected: false,
            });
            lines.push(ModelLine {
                line: Line::from(vec![Span::styled(
                    "Fast mode speeds up replies. 1M Context is available on supported models.",
                    desc_style,
                )]),
                is_selected: false,
            });

            let fast_selected = matches!(self.entries().get(self.selected_index), Some(EntryKind::FastMode));
            let fast_enabled = matches!(self.current_service_tier, Some(ServiceTier::Fast));
            let fast_status = if fast_enabled { "enabled" } else { "disabled" };
            let mut fast_label_style = Style::default().fg(crate::colors::text());
            if fast_selected {
                fast_label_style = fast_label_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }
            if fast_enabled {
                fast_label_style = fast_label_style.fg(crate::colors::success());
            }
            let fast_arrow = if fast_selected { "› " } else { "  " };
            let fast_arrow_style = if fast_selected {
                Style::default()
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::colors::text_dim())
            };
            lines.push(ModelLine {
                line: Line::from(vec![
                    Span::styled(fast_arrow, fast_arrow_style),
                    Span::styled(format!("Fast Mode: {fast_status}"), fast_label_style),
                ]),
                is_selected: fast_selected,
            });

            let context_selected = matches!(self.entries().get(self.selected_index), Some(EntryKind::ContextMode));
            let context_available = self.supports_extended_context();
            let context_status = if context_available {
                match self.current_context_mode {
                    Some(ContextMode::OneM) => "enabled",
                    Some(ContextMode::Auto) => "auto",
                    Some(ContextMode::Disabled) | None => "disabled",
                }
            } else {
                "unavailable"
            };
            let mut context_label_style = Style::default().fg(crate::colors::text());
            if context_selected {
                context_label_style = context_label_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }
            if context_available && self.current_context_mode.is_some() {
                context_label_style = context_label_style.fg(crate::colors::success());
            }
            if !context_available {
                context_label_style = context_label_style.fg(crate::colors::text_dim());
            }
            let context_arrow = if context_selected { "› " } else { "  " };
            let context_arrow_style = if context_selected {
                Style::default()
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(crate::colors::text_dim())
            };
            lines.push(ModelLine {
                line: Line::from(vec![
                    Span::styled(context_arrow, context_arrow_style),
                    Span::styled(
                        format!("1M Context: {context_status}"),
                        context_label_style,
                    ),
                ]),
                is_selected: context_selected,
            });
            let context_description = if context_available {
                "Press Enter to cycle Disabled, Enabled, and Auto."
            } else {
                "Unavailable for this model. Saved settings apply automatically on supported models."
            };
            lines.push(ModelLine {
                line: Line::from(vec![Span::styled(context_description, desc_style)]),
                is_selected: false,
            });
            lines.push(ModelLine {
                line: Line::from(""),
                is_selected: false,
            });
        }

        if self.target.supports_follow_chat() {
            let is_selected = matches!(
                self.entries().get(self.selected_index),
                Some(EntryKind::FollowChat)
            );

            let header_style = Style::default()
                .fg(crate::colors::text_bright())
                .add_modifier(Modifier::BOLD);
            let desc_style = Style::default().fg(crate::colors::text_dim());
            lines.push(ModelLine {
                line: Line::from(vec![Span::styled("Follow Chat Mode", header_style)]),
                is_selected: false,
            });
            lines.push(ModelLine {
                line: Line::from(vec![Span::styled(
                    "Use the active chat model and reasoning; stays in sync as chat changes.",
                    desc_style,
                )]),
                is_selected: false,
            });

            let mut label_style = Style::default().fg(crate::colors::text());
            if is_selected {
                label_style = label_style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }
            let mut arrow_style = Style::default().fg(crate::colors::text_dim());
            if is_selected {
                arrow_style = label_style;
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
            lines.push(ModelLine {
                line: Line::from(spans),
                is_selected,
            });
            lines.push(ModelLine {
                line: Line::from(""),
                is_selected: false,
            });
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
                    lines.push(ModelLine {
                        line: Line::from(""),
                        is_selected: false,
                    });
                }
                lines.push(ModelLine {
                    line: Line::from(vec![Span::styled(
                        Self::format_model_header(&flat_preset.model),
                        Style::default()
                            .fg(crate::colors::text_bright())
                            .add_modifier(Modifier::BOLD),
                    )]),
                    is_selected: false,
                });
                if let Some(desc) = Self::model_description(&flat_preset.model) {
                    lines.push(ModelLine {
                        line: Line::from(vec![Span::styled(
                            desc.to_string(),
                            Style::default().fg(crate::colors::text_dim()),
                        )]),
                        is_selected: false,
                    });
                }
                previous_model = Some(&flat_preset.model);
            }

            let is_selected = entry_idx == self.selected_index;
            let is_current = !self.use_chat_model
                && flat_preset.model.eq_ignore_ascii_case(&self.current_model)
                && flat_preset.effort == self.current_effort;
            let label = FlatPreset::effort_label(flat_preset.effort);
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

            lines.push(ModelLine {
                line: Line::from(vec![
                    Span::styled("   ", indent_style),
                    Span::styled(row_text, label_style),
                    Span::styled(" - ", divider_style),
                    Span::styled(flat_preset.description.clone(), description_style),
                ]),
                is_selected,
            });
        }

        lines
    }

    fn content_line_count(&self) -> u16 {
        let mut lines: u16 = 3;
        if self.target.supports_fast_mode() {
            lines = lines.saturating_add(6);
        }
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
        let model_name_rank = Self::compare_model_names(&a.model, &b.model);
        if model_name_rank != Ordering::Equal {
            return model_name_rank;
        }

        let effort_rank = Self::effort_rank(a.effort).cmp(&Self::effort_rank(b.effort));
        if effort_rank != Ordering::Equal {
            return effort_rank;
        }

        a.label.cmp(&b.label)
    }

    fn compare_model_names(a: &str, b: &str) -> Ordering {
        let a_lower = a.to_ascii_lowercase();
        let b_lower = b.to_ascii_lowercase();
        match (
            Self::parse_model_version_components(&a_lower),
            Self::parse_model_version_components(&b_lower),
        ) {
            (Some(a_components), Some(b_components)) => {
                let component_rank = b_components.cmp(&a_components);
                if component_rank != Ordering::Equal {
                    return component_rank;
                }
                a_lower.cmp(&b_lower)
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a_lower.cmp(&b_lower),
        }
    }

    fn parse_model_version_components(model: &str) -> Option<Vec<u32>> {
        let canonical = model.rsplit('/').next().unwrap_or(model);
        let mut components = Vec::new();

        for segment in canonical.split('-') {
            let first = segment.chars().next()?;
            if !first.is_ascii_digit() {
                continue;
            }

            for part in segment.split('.') {
                if part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()) {
                    return None;
                }
                components.push(part.parse().ok()?);
            }

            return (!components.is_empty()).then_some(components);
        }

        None
    }

    fn model_description(model: &str) -> Option<&'static str> {
        if model.eq_ignore_ascii_case("gpt-5.5") {
            Some("Frontier model for complex coding, research, and real-world work.")
        } else if model.eq_ignore_ascii_case("gpt-5.4") {
            Some("Brings together flagship reasoning, coding, and tool use in a single frontier model.")
        } else if model.eq_ignore_ascii_case("gpt-5.4-mini") {
            Some("Smaller GPT-5.4 variant tuned for faster coding loops.")
        } else if model.eq_ignore_ascii_case("gpt-5.3-codex") {
            Some("Frontier agentic coding, 25% faster than previous models.")
        } else if model.eq_ignore_ascii_case("gpt-5.3-codex-spark") {
            Some("Fast codex variant tuned for responsive coding loops and smaller edits.")
        } else if model.eq_ignore_ascii_case("gpt-5.2-codex") {
            Some("Frontier agentic coding model.")
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

        let rows = self.rendered_rows();
        let selected_row_index = rows.iter().position(|line| line.is_selected).unwrap_or(0);

        // Keep a one-line spacer between list content and footer so the fixed
        // hint line never eats into selectable content.
        let footer_rows: u16 = 1;
        let bottom_spacer_rows: u16 = 1;
        let rows_area_height = area
            .height
            .saturating_sub(footer_rows.saturating_add(bottom_spacer_rows));

        let max_visible_rows = rows_area_height as usize;
        let rows_len = rows.len();
        let scroll_top = self.content_scroll_top(rows_len, selected_row_index, max_visible_rows);
        self.scroll_top.set(scroll_top);

        let lines: Vec<Line> = rows
            .into_iter()
            .skip(scroll_top)
            .take(max_visible_rows)
            .map(|line| line.line)
            .collect();

        let mut lines = lines;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_event::AppEvent;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use code_common::model_presets::{ModelPreset, ReasoningEffortPreset};
    use code_core::config_types::TextVerbosity;
    use std::sync::mpsc;

    const TEST_VERBOSITY: [TextVerbosity; 1] = [TextVerbosity::Low];

    fn make_preset(model: &str) -> ModelPreset {
        ModelPreset {
            id: model.to_string(),
            model: model.to_string(),
            display_name: model.to_string(),
            description: format!("{model} model"),
            default_reasoning_effort: ReasoningEffort::Low.into(),
            supported_reasoning_efforts: vec![ReasoningEffortPreset {
                effort: ReasoningEffort::Low.into(),
                description: "low".to_string(),
            }],
            supported_text_verbosity: &TEST_VERBOSITY,
            is_default: false,
            upgrade: None,
            pro_only: false,
            show_in_picker: true,
        }
    }

    fn buffer_body_lines(buf: &Buffer, width: u16, height: u16) -> Vec<String> {
        let mut rows = Vec::new();
        if width < 2 || height < 2 {
            return rows;
        }

        for y in 1..height.saturating_sub(1) {
            let line: String = (1..width.saturating_sub(1))
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect();
            rows.push(line);
        }
        rows
    }

    fn sorted_model_order(view: &ModelSelectionView) -> Vec<String> {
        let mut order = Vec::new();
        for idx in view.sorted_indices() {
            let model = view.flat_presets[idx].model.clone();
            if order.last() != Some(&model) {
                order.push(model);
            }
        }
        order
    }

    #[test]
    fn model_selection_scrolls_selected_model_into_view() {
        let presets = (0..12).map(|i| make_preset(&format!("model-{i:02}"))).collect();
        let (tx, _rx) = mpsc::channel::<AppEvent>();

        let mut view = ModelSelectionView::new(
            presets,
            "model-11".to_string(),
            ReasoningEffort::Low,
            None,
            None,
            false,
            ModelSelectionTarget::Session,
            AppEventSender::new(tx),
        );

        let last_model = sorted_model_order(&view)
            .last()
            .cloned()
            .expect("at least one model");

        for _ in 0..view.entries().len() {
            let selected_entry = view
                .entries()
                .get(view.selected_index)
                .copied()
                .expect("selected entry");
            if let EntryKind::Preset(idx) = selected_entry
                && view.flat_presets[idx].model == last_model
            {
                break;
            }
            let _ = view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }

        let width = 80;
        let height = 13;
        let mut buf = ratatui::buffer::Buffer::empty(Rect {
            x: 0,
            y: 0,
            width,
            height,
        });
        view.render(Rect {
            x: 0,
            y: 0,
            width,
            height,
        }, &mut buf);

        let selected_entry = view
            .entries()
            .get(view.selected_index)
            .copied()
            .expect("selected entry");

        assert!(matches!(selected_entry, EntryKind::Preset(_)));
        assert_eq!(view.flat_presets[match selected_entry {
            EntryKind::Preset(idx) => idx,
            _ => unreachable!("selected entry should be a preset"),
        }]
        .model, last_model);
        assert!(view.scroll_top.get() > 0);
    }

    #[test]
    fn model_selection_keeps_model_header_visible_for_first_entry() {
        let presets = vec!["gpt-5.3-codex", "gpt-5.2-codex", "gpt-5.1-codex"]
            .into_iter()
            .map(make_preset)
            .collect();

        let (tx, _rx) = mpsc::channel::<AppEvent>();
        let mut view = ModelSelectionView::new(
            presets,
            "gpt-5.3-codex".to_string(),
            ReasoningEffort::Low,
            None,
            None,
            false,
            ModelSelectionTarget::Session,
            AppEventSender::new(tx),
        );

        let _ = view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let _ = view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        let width = 80;
        let height = 16;
        let mut buf = ratatui::buffer::Buffer::empty(Rect {
            x: 0,
            y: 0,
            width,
            height,
        });
        view.render(Rect {
            x: 0,
            y: 0,
            width,
            height,
        }, &mut buf);

        let lines = buffer_body_lines(&buf, width, height);
        let visible = lines.join("\n");
        let has_header = visible.contains("GPT-5.3-Codex");
        let has_desc = visible.contains("Frontier agentic coding")
            && visible.contains("25% faster than previous models.");

        assert!(has_header);
        assert!(has_desc);
    }

    #[test]
    fn model_selection_prioritizes_higher_versions_and_shows_fast_mode_toggle() {
        let mut gpt_5_5 = make_preset("gpt-5.5");
        gpt_5_5.supported_reasoning_efforts = vec![
            ReasoningEffortPreset {
                effort: ReasoningEffort::Low.into(),
                description: "Fast responses with lighter reasoning".to_string(),
            },
            ReasoningEffortPreset {
                effort: ReasoningEffort::High.into(),
                description: "Maximizes reasoning depth".to_string(),
            },
        ];

        let mut gpt_5_4 = make_preset("gpt-5.4");
        gpt_5_4.supported_reasoning_efforts = vec![
            ReasoningEffortPreset {
                effort: ReasoningEffort::Low.into(),
                description: "Fast responses with lighter reasoning".to_string(),
            },
            ReasoningEffortPreset {
                effort: ReasoningEffort::High.into(),
                description: "Maximizes reasoning depth".to_string(),
            },
        ];

        let mut gpt_5_4_mini = make_preset("gpt-5.4-mini");
        gpt_5_4_mini.supported_reasoning_efforts = vec![
            ReasoningEffortPreset {
                effort: ReasoningEffort::Low.into(),
                description: "Fast responses with lighter reasoning".to_string(),
            },
            ReasoningEffortPreset {
                effort: ReasoningEffort::High.into(),
                description: "Maximizes reasoning depth".to_string(),
            },
        ];

        let presets = vec![
            make_preset("gpt-5.3-codex"),
            make_preset("gpt-5.3-codex-spark"),
            gpt_5_4_mini,
            gpt_5_4,
            gpt_5_5,
        ];
        let (tx, _rx) = mpsc::channel::<AppEvent>();
        let view = ModelSelectionView::new(
            presets,
            "gpt-5.5".to_string(),
            ReasoningEffort::Low,
            Some(ServiceTier::Fast),
            None,
            false,
            ModelSelectionTarget::Session,
            AppEventSender::new(tx),
        );

        let width = 100;
        let height = 24;
        let mut buf = ratatui::buffer::Buffer::empty(Rect {
            x: 0,
            y: 0,
            width,
            height,
        });
        view.render(Rect {
            x: 0,
            y: 0,
            width,
            height,
        }, &mut buf);

        let order = sorted_model_order(&view);
        assert_eq!(
            order,
            vec![
                "gpt-5.5".to_string(),
                "gpt-5.4".to_string(),
                "gpt-5.4-mini".to_string(),
                "gpt-5.3-codex".to_string(),
                "gpt-5.3-codex-spark".to_string(),
            ]
        );

        let lines = buffer_body_lines(&buf, width, height);
        let visible = lines.join("\n");
        assert!(visible.contains("Mode Settings"));
        assert!(visible.contains("Fast Mode: enabled"));
        assert!(visible.contains("1M Context: unavailable"));
        assert!(visible.contains("Fast mode speeds up replies."));
        assert!(visible.contains(
            "Unavailable for this model. Saved settings apply automatically on supported models."
        ));
    }

    #[test]
    fn selecting_fast_mode_toggles_service_tier_without_closing() {
        let presets = vec![make_preset("gpt-5.4")];
        let (tx, rx) = mpsc::channel::<AppEvent>();
        let mut view = ModelSelectionView::new(
            presets,
            "gpt-5.4".to_string(),
            ReasoningEffort::Low,
            None,
            None,
            false,
            ModelSelectionTarget::Session,
            AppEventSender::new(tx),
        );

        let _ = view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        let event = rx.try_recv().expect("service tier event");
        assert!(matches!(
            event,
            AppEvent::UpdateServiceTierSelection {
                service_tier: Some(ServiceTier::Fast)
            }
        ));
        assert!(!view.is_complete());
    }

    #[test]
    fn selecting_one_m_context_toggles_without_closing() {
        let presets = vec![make_preset("gpt-5.4")];
        let (tx, rx) = mpsc::channel::<AppEvent>();
        let mut view = ModelSelectionView::new(
            presets,
            "gpt-5.4".to_string(),
            ReasoningEffort::Low,
            None,
            None,
            false,
            ModelSelectionTarget::Session,
            AppEventSender::new(tx),
        );

        let _ = view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let _ = view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        let event = rx.try_recv().expect("context mode event");
        assert!(matches!(
            event,
            AppEvent::UpdateSessionContextModeSelection {
                context_mode: Some(ContextMode::OneM)
            }
        ));
        assert!(!view.is_complete());
    }

    #[test]
    fn selecting_context_mode_cycles_to_auto_without_closing() {
        let presets = vec![make_preset("gpt-5.4")];
        let (tx, rx) = mpsc::channel::<AppEvent>();
        let mut view = ModelSelectionView::new(
            presets,
            "gpt-5.4".to_string(),
            ReasoningEffort::Low,
            None,
            None,
            false,
            ModelSelectionTarget::Session,
            AppEventSender::new(tx),
        );

        let _ = view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        let _ = view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let _ = rx.try_recv().expect("first context mode event");
        let _ = view.handle_key_event_direct(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        let event = rx.try_recv().expect("auto context mode event");
        assert!(matches!(
            event,
            AppEvent::UpdateSessionContextModeSelection {
                context_mode: Some(ContextMode::Auto)
            }
        ));
        assert!(!view.is_complete());
    }

    #[test]
    fn model_selection_shows_unavailable_context_hint_for_unsupported_model() {
        let presets = vec![make_preset("gpt-5.3-codex")];
        let (tx, _rx) = mpsc::channel::<AppEvent>();
        let view = ModelSelectionView::new(
            presets,
            "gpt-5.3-codex".to_string(),
            ReasoningEffort::Low,
            None,
            Some(ContextMode::Auto),
            false,
            ModelSelectionTarget::Session,
            AppEventSender::new(tx),
        );

        let width = 100;
        let height = 18;
        let mut buf = ratatui::buffer::Buffer::empty(Rect {
            x: 0,
            y: 0,
            width,
            height,
        });
        view.render(Rect {
            x: 0,
            y: 0,
            width,
            height,
        }, &mut buf);

        let lines = buffer_body_lines(&buf, width, height);
        assert!(
            lines.iter().any(|line| line.contains("1M Context: unavailable")),
            "expected unsupported model to show unavailable context, got:\n{}",
            lines.join("\n")
        );
        assert!(lines.iter().any(|line| {
            line.contains("Unavailable for this model. Saved settings apply automatically on supported models.")
        }));
    }

    #[test]
    fn model_selection_marks_gpt_5_5_context_unavailable() {
        let presets = vec![make_preset("gpt-5.5")];
        let (tx, _rx) = mpsc::channel::<AppEvent>();
        let view = ModelSelectionView::new(
            presets,
            "gpt-5.5".to_string(),
            ReasoningEffort::Low,
            None,
            Some(ContextMode::OneM),
            false,
            ModelSelectionTarget::Session,
            AppEventSender::new(tx),
        );

        let width = 100;
        let height = 18;
        let mut buf = ratatui::buffer::Buffer::empty(Rect {
            x: 0,
            y: 0,
            width,
            height,
        });
        view.render(
            Rect {
                x: 0,
                y: 0,
                width,
                height,
            },
            &mut buf,
        );

        let lines = buffer_body_lines(&buf, width, height);
        assert!(
            lines.iter().any(|line| line.contains("1M Context: unavailable")),
            "expected gpt-5.5 to show unavailable context, got:\n{}",
            lines.join("\n")
        );
        assert!(lines.iter().any(|line| {
            line.contains("Unavailable for this model. Saved settings apply automatically on supported models.")
        }));
    }

    #[test]
    fn model_selection_keeps_first_follow_chat_entry_visible_on_compact_height() {
        let presets = vec![make_preset("gpt-5.3-codex")];

        let (tx, _rx) = mpsc::channel::<AppEvent>();
        let view = ModelSelectionView::new(
            presets,
            "gpt-5.3-codex".to_string(),
            ReasoningEffort::Low,
            None,
            None,
            true,
            ModelSelectionTarget::Review,
            AppEventSender::new(tx),
        );

        let width = 80;
        let height = 9;
        let mut buf = ratatui::buffer::Buffer::empty(Rect {
            x: 0,
            y: 0,
            width,
            height,
        });
        view.render(
            Rect {
                x: 0,
                y: 0,
                width,
                height,
            },
            &mut buf,
        );

        let lines = buffer_body_lines(&buf, width, height);
        let has_follow_chat_option = lines.iter().any(|line| line.contains("Use chat model"));

        assert!(has_follow_chat_option);
        assert!(view.scroll_top.get() > 0);
    }

    #[test]
    fn model_selection_wraps_scroll_back_to_top_entry() {
        let presets = (0..12).map(|i| make_preset(&format!("model-{i:02}"))).collect();
        let (tx, _rx) = mpsc::channel::<AppEvent>();

        let mut view = ModelSelectionView::new(
            presets,
            "model-00".to_string(),
            ReasoningEffort::Low,
            None,
            None,
            false,
            ModelSelectionTarget::Session,
            AppEventSender::new(tx),
        );

        for _ in 0..13 {
            let _ = view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }

        let width = 80;
        let height = 13;
        let mut buf = ratatui::buffer::Buffer::empty(Rect {
            x: 0,
            y: 0,
            width,
            height,
        });

        view.render(Rect {
            x: 0,
            y: 0,
            width,
            height,
        }, &mut buf);

        let _ = view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

        let mut buf = ratatui::buffer::Buffer::empty(Rect {
            x: 0,
            y: 0,
            width,
            height,
        });
        view.render(Rect {
            x: 0,
            y: 0,
            width,
            height,
        }, &mut buf);

        let lines = buffer_body_lines(&buf, width, height);
        let has_fast_mode = lines.iter().any(|line| line.contains("Fast Mode: disabled"));
        assert!(has_fast_mode);
    }

    #[test]
    fn model_selection_scrolls_up_when_selection_moves_above_viewport() {
        let presets = (0..12).map(|i| make_preset(&format!("model-{i:02}"))).collect();
        let (tx, _rx) = mpsc::channel::<AppEvent>();

        let mut view = ModelSelectionView::new(
            presets,
            "model-00".to_string(),
            ReasoningEffort::Low,
            None,
            None,
            false,
            ModelSelectionTarget::Session,
            AppEventSender::new(tx),
        );

        for _ in 0..12 {
            let _ = view.handle_key_event_direct(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        }

        let width = 80;
        let height = 12;
        let mut buf = ratatui::buffer::Buffer::empty(Rect {
            x: 0,
            y: 0,
            width,
            height,
        });
        view.render(Rect {
            x: 0,
            y: 0,
            width,
            height,
        }, &mut buf);

        let scroll_after_down = view.scroll_top.get();
        assert!(scroll_after_down > 0);

        for _ in 0..6 {
            let _ = view.handle_key_event_direct(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        }

        let mut buf = ratatui::buffer::Buffer::empty(Rect {
            x: 0,
            y: 0,
            width,
            height,
        });
        view.render(Rect {
            x: 0,
            y: 0,
            width,
            height,
        }, &mut buf);

        let scroll_after_up = view.scroll_top.get();
        assert!(scroll_after_up < scroll_after_down);
    }
}
