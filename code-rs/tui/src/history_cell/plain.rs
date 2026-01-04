use super::*;
use super::text::{message_lines_from_ratatui, message_lines_to_ratatui};
use crate::account_label::key_suffix;
use crate::history::state::{
    HistoryId,
    InlineSpan,
    MessageHeader,
    MessageLine,
    MessageLineKind,
    NoticeRecord,
    PlainMessageKind,
    PlainMessageRole,
    PlainMessageState,
    TextEmphasis,
    TextTone,
};
use crate::colors;
use crate::sanitize::Mode as SanitizeMode;
use crate::sanitize::Options as SanitizeOptions;
use crate::sanitize::sanitize_for_tui;
use crate::slash_command::SlashCommand;
use crate::theme::{current_theme, Theme};
use code_ansi_escape::ansi_escape_line;
use code_common::create_config_summary_entries;
use code_core::config::Config;
use code_core::config_types::ReasoningEffort;
use code_core::protocol::{SessionConfiguredEvent, TokenUsage};
use code_protocol::num_format::format_with_separators;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Padding, Paragraph, Wrap};
use std::collections::HashMap;

struct PlainLayoutCache {
    requested_width: u16,
    effective_width: u16,
    height: u16,
    buffer: Option<Buffer>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlainCellState {
    pub message: PlainMessageState,
    pub kind: HistoryCellType,
}

impl PlainCellState {
    fn role(&self) -> PlainMessageRole {
        self.message.role
    }

    fn header(&self) -> Option<&MessageHeader> {
        self.message.header.as_ref()
    }

    fn body(&self) -> &[MessageLine] {
        &self.message.lines
    }
}

pub(crate) struct PlainHistoryCell {
    state: PlainCellState,
    cached_layout: std::cell::RefCell<Option<PlainLayoutCache>>,
}

impl PlainHistoryCell {
    pub(crate) fn is_auto_review_notice(&self) -> bool {
        self.state
            .header()
            .map(|h| h.label.to_lowercase().contains("auto review"))
            .unwrap_or(false)
    }

    pub(crate) fn auto_review_bg() -> ratatui::style::Color {
        // Match the diff success background tint for consistent success styling.
        colors::tint_background_toward(colors::success())
    }

    pub(crate) fn auto_review_padding() -> (u16, u16) {
        // Symmetric top/bottom padding for auto-review notices.
        (1, 1)
    }
    pub(crate) fn from_state(state: PlainMessageState) -> Self {
        let mut kind = history_cell_kind_from_plain(state.kind);
        if kind == HistoryCellType::User {
            if let Some(first_line) = state.lines.first() {
                if first_line.spans.first().map_or(false, |span| {
                    span.text.starts_with("[Compaction Summary]")
                }) {
                    kind = HistoryCellType::CompactionSummary;
                }
            }
        }
        Self {
            state: PlainCellState {
                message: state,
                kind,
            },
            cached_layout: std::cell::RefCell::new(None),
        }
    }

    pub(crate) fn from_notice_record(record: NoticeRecord) -> Self {
        let header = record
            .title
            .filter(|title| !title.trim().is_empty())
            .map(|label| MessageHeader { label, badge: None });
        let state = PlainMessageState {
            id: record.id,
            role: PlainMessageRole::System,
            kind: PlainMessageKind::Notice,
            header,
            lines: record.body,
            metadata: None,
        };
        Self::from_state(state)
    }

    pub(crate) fn state(&self) -> &PlainMessageState {
        &self.state.message
    }

    pub(crate) fn state_mut(&mut self) -> &mut PlainMessageState {
        self.invalidate_layout_cache();
        &mut self.state.message
    }

    pub(crate) fn invalidate_layout_cache(&self) {
        self.cached_layout.borrow_mut().take();
    }

    fn ensure_layout(&self, requested_width: u16, effective_width: u16) {
        let mut cache = self.cached_layout.borrow_mut();
        let needs_rebuild = cache
            .as_ref()
            .map_or(true, |cached| {
                cached.requested_width != requested_width
                    || cached.effective_width != effective_width
            });
        if needs_rebuild {
            *cache = Some(self.build_layout(requested_width, effective_width));
        }
    }

    fn build_layout(&self, requested_width: u16, effective_width: u16) -> PlainLayoutCache {
        if requested_width == 0 || effective_width == 0 {
            return PlainLayoutCache {
                requested_width,
                effective_width,
                height: 0,
                buffer: None,
            };
        }

        let is_auto_review = self.is_auto_review_notice();
        let cell_bg = match self.state.kind {
            HistoryCellType::Assistant => crate::colors::assistant_bg(),
            HistoryCellType::CompactionSummary => crate::colors::background(),
            _ if is_auto_review => Self::auto_review_bg(),
            _ => crate::colors::background(),
        };
        let bg_style = Style::default().bg(cell_bg).fg(crate::colors::text());

        let trimmed_lines = self.display_lines_trimmed();
        let text = Text::from(trimmed_lines.clone());
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });

        let (pad_top, pad_bottom) = if is_auto_review {
            Self::auto_review_padding()
        } else {
            (0, 0)
        };

        let inner_height: u16 = paragraph
            .line_count(effective_width)
            .try_into()
            .unwrap_or(0);
        let height = inner_height
            .saturating_add(pad_top)
            .saturating_add(pad_bottom);

        if height == 0 {
            return PlainLayoutCache {
                requested_width,
                effective_width,
                height,
                buffer: None,
            };
        }

        let render_height = height.max(1);
        let render_area = Rect::new(0, 0, requested_width, render_height);
        let mut buffer = Buffer::empty(render_area);
        // Paint full cell (including padding) with the cell background so tint extends through padding.
        fill_rect(&mut buffer, render_area, Some(' '), Style::default().bg(cell_bg).fg(crate::colors::text()));

        let paragraph_lines = Text::from(trimmed_lines);
        if matches!(self.state.kind, HistoryCellType::User) {
            let block = Block::default()
                .style(bg_style)
                .padding(Padding {
                    left: 0,
                    right: crate::layout_consts::USER_HISTORY_RIGHT_PAD.into(),
                    top: 0,
                    bottom: 0,
                });
            Paragraph::new(paragraph_lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .style(bg_style)
                .render(render_area, &mut buffer);
        } else {
            let block = Block::default().style(Style::default().bg(cell_bg));
            let inner_area = if pad_top + pad_bottom < render_height {
                Rect::new(
                    render_area.x,
                    render_area.y.saturating_add(pad_top),
                    render_area.width,
                    render_height.saturating_sub(pad_top + pad_bottom),
                )
            } else {
                render_area
            };

            Paragraph::new(paragraph_lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .style(Style::default().bg(cell_bg))
                .render(inner_area, &mut buffer);
        }

        PlainLayoutCache {
            requested_width,
            effective_width,
            height,
            buffer: Some(buffer),
        }
    }

    fn hide_header(&self) -> bool {
        should_hide_header(self.state.kind)
    }

    fn header_line(&self, theme: &Theme) -> Option<Line<'static>> {
        let header = self.state.header()?;
        let mut spans: Vec<Span<'static>> = Vec::new();
        let style = header_style(self.state.role(), theme);
        spans.push(Span::styled(header.label.clone(), style));
        if let Some(badge) = &header.badge {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(badge.clone(), header_badge_style(theme)));
        }
        Some(Line::from(spans))
    }
}

impl HistoryCell for PlainHistoryCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        self.state.kind
    }

    fn gutter_symbol(&self) -> Option<&'static str> {
        if let Some(header) = self.state.header() {
            let label = header.label.trim().to_lowercase();
            if label == "auto review" {
                return Some("•");
            }
        }
        super::gutter_symbol_for_kind(self.kind())
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let theme = current_theme();
        let mut lines: Vec<Line<'static>> = Vec::new();

        if !self.hide_header() {
            if let Some(header) = self.header_line(&theme) {
                lines.push(header);
            }
        }

        lines.extend(message_lines_to_ratatui(self.state.body(), &theme));
        lines
    }

    fn has_custom_render(&self) -> bool {
        matches!(self.state.kind, HistoryCellType::User) || self.is_auto_review_notice()
    }

    fn desired_height(&self, width: u16) -> u16 {
        let effective_width = if matches!(self.state.kind, HistoryCellType::User) {
            width.saturating_sub(crate::layout_consts::USER_HISTORY_RIGHT_PAD.into())
        } else {
            width
        };

        self.ensure_layout(width, effective_width);
        self.cached_layout
            .borrow()
            .as_ref()
            .map(|cache| cache.height)
            .unwrap_or(0)
    }

    fn render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        let requested_width = area.width;
        let effective_width = if matches!(self.state.kind, HistoryCellType::User) {
            requested_width
                .saturating_sub(crate::layout_consts::USER_HISTORY_RIGHT_PAD.into())
        } else {
            requested_width
        };

        let is_auto_review = self.is_auto_review_notice();
        let cell_bg = match self.state.kind {
            HistoryCellType::Assistant => crate::colors::assistant_bg(),
            _ if is_auto_review => Self::auto_review_bg(),
            _ => crate::colors::background(),
        };
        if matches!(self.state.kind, HistoryCellType::Assistant) || is_auto_review {
            let bg_style = Style::default().bg(cell_bg).fg(crate::colors::text());
            fill_rect(buf, area, Some(' '), bg_style);
        }

        if requested_width == 0 || effective_width == 0 {
            return;
        }

        self.ensure_layout(requested_width, effective_width);
        let cache_ref = self.cached_layout.borrow();
        let Some(cache) = cache_ref.as_ref() else {
            return;
        };
        let Some(src_buffer) = cache.buffer.as_ref() else {
            return;
        };

        let content_height = cache.height as usize;
        if content_height == 0 || skip_rows as usize >= content_height {
            return;
        }

        let src_area = src_buffer.area();
        let copy_width = usize::from(src_area.width.min(area.width));
        let max_rows = usize::from(area.height);

        for row_offset in 0..max_rows {
            let src_y = skip_rows as usize + row_offset;
            if src_y >= content_height || src_y >= usize::from(src_area.height) {
                break;
            }
            let dest_y = area.y + row_offset as u16;
            for col_offset in 0..copy_width {
                let dest_x = area.x + col_offset as u16;
                let src_cell = &src_buffer[(col_offset as u16, src_y as u16)];
                buf[(dest_x, dest_y)] = src_cell.clone();
            }
        }
    }

    fn display_lines_trimmed(&self) -> Vec<Line<'static>> {
        trim_empty_lines(self.display_lines())
    }
}
struct PlainMessageStateBuilder;

impl PlainMessageStateBuilder {
    fn from_lines(lines: Vec<Line<'static>>, kind: HistoryCellType) -> PlainMessageState {
        let role = plain_role_from_kind(kind);
        let mut iter = lines.into_iter();
        let header_line = if should_hide_header(kind) {
            iter.next()
        } else {
            None
        };

        let header = header_line.map(|line| MessageHeader {
            label: line_plain_text(&line),
            badge: None,
        });

        let body_lines: Vec<Line<'static>> = iter.collect();
        let message_lines = message_lines_from_ratatui(body_lines);

        PlainMessageState {
            id: HistoryId::ZERO,
            role,
            kind: plain_message_kind_from_cell_kind(kind),
            header,
            lines: message_lines,
            metadata: None,
        }
    }
}

pub(crate) fn plain_message_state_from_lines(
    lines: Vec<Line<'static>>,
    kind: HistoryCellType,
) -> PlainMessageState {
    PlainMessageStateBuilder::from_lines(lines, kind)
}

pub(crate) fn plain_message_state_from_paragraphs<I, S>(
    kind: PlainMessageKind,
    role: PlainMessageRole,
    lines: I,
) -> PlainMessageState
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let message_lines = lines
        .into_iter()
        .map(|text| MessageLine {
            kind: MessageLineKind::Paragraph,
            spans: vec![InlineSpan {
                text: text.into(),
                tone: TextTone::Default,
                emphasis: TextEmphasis::default(),
                entity: None,
            }],
        })
        .collect();

    PlainMessageState {
        id: HistoryId::ZERO,
        role,
        kind,
        header: None,
        lines: message_lines,
        metadata: None,
    }
}

pub(crate) fn plain_role_for_kind(kind: PlainMessageKind) -> PlainMessageRole {
    match kind {
        PlainMessageKind::User => PlainMessageRole::User,
        PlainMessageKind::Assistant => PlainMessageRole::Assistant,
        PlainMessageKind::Tool => PlainMessageRole::Tool,
        PlainMessageKind::Error => PlainMessageRole::Error,
        PlainMessageKind::Background => PlainMessageRole::BackgroundEvent,
        PlainMessageKind::Notice | PlainMessageKind::Plain => PlainMessageRole::System,
    }
}

fn plain_role_from_kind(kind: HistoryCellType) -> PlainMessageRole {
    match kind {
        HistoryCellType::User => PlainMessageRole::User,
        HistoryCellType::Assistant => PlainMessageRole::Assistant,
        HistoryCellType::Tool { .. } => PlainMessageRole::Tool,
        HistoryCellType::Error => PlainMessageRole::Error,
        HistoryCellType::BackgroundEvent => PlainMessageRole::BackgroundEvent,
        HistoryCellType::Notice => PlainMessageRole::System,
        _ => PlainMessageRole::System,
    }
}

fn plain_message_kind_from_cell_kind(kind: HistoryCellType) -> PlainMessageKind {
    match kind {
        HistoryCellType::User => PlainMessageKind::User,
        HistoryCellType::Assistant => PlainMessageKind::Assistant,
        HistoryCellType::Tool { .. } => PlainMessageKind::Tool,
        HistoryCellType::Error => PlainMessageKind::Error,
        HistoryCellType::BackgroundEvent => PlainMessageKind::Background,
        HistoryCellType::Notice => PlainMessageKind::Notice,
        _ => PlainMessageKind::Plain,
    }
}

fn history_cell_kind_from_plain(kind: PlainMessageKind) -> HistoryCellType {
    match kind {
        PlainMessageKind::User => HistoryCellType::User,
        PlainMessageKind::Assistant => HistoryCellType::Assistant,
        PlainMessageKind::Tool => HistoryCellType::Tool {
            status: super::ToolCellStatus::Success,
        },
        PlainMessageKind::Error => HistoryCellType::Error,
        PlainMessageKind::Background => HistoryCellType::BackgroundEvent,
        PlainMessageKind::Notice => HistoryCellType::Notice,
        PlainMessageKind::Plain => HistoryCellType::Plain,
    }
}

fn should_hide_header(kind: HistoryCellType) -> bool {
    matches!(
        kind,
        HistoryCellType::User
            | HistoryCellType::Assistant
            | HistoryCellType::Tool { .. }
            | HistoryCellType::Error
            | HistoryCellType::BackgroundEvent
            | HistoryCellType::Notice
    )
}

fn header_style(role: PlainMessageRole, theme: &Theme) -> Style {
    match role {
        PlainMessageRole::User => Style::default().fg(theme.text),
        PlainMessageRole::Assistant => Style::default()
            .fg(theme.primary)
            .add_modifier(Modifier::BOLD),
        PlainMessageRole::Tool => Style::default()
            .fg(theme.info)
            .add_modifier(Modifier::BOLD),
        PlainMessageRole::Error => Style::default()
            .fg(theme.error)
            .add_modifier(Modifier::BOLD),
        PlainMessageRole::BackgroundEvent => Style::default().fg(theme.text_dim),
        PlainMessageRole::System => Style::default().fg(theme.text_dim),
    }
}

fn header_badge_style(theme: &Theme) -> Style {
    Style::default().fg(theme.text_dim).add_modifier(Modifier::ITALIC)
}

fn line_plain_text(line: &Line<'_>) -> String {
    if line.spans.is_empty() {
        String::new()
    } else {
        line.spans
            .iter()
            .map(|span| span.content.to_string())
            .collect::<Vec<String>>()
            .join("")
    }
}

pub(crate) fn new_session_info(
    config: &Config,
    event: SessionConfiguredEvent,
    is_first_event: bool,
    latest_version: Option<&str>,
) -> PlainMessageState {
    let SessionConfiguredEvent {
        model,
        session_id: _,
        history_log_id: _,
        history_entry_count: _,
        ..
    } = event;

    if is_first_event {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from("notice".dim()));
        lines.extend(popular_commands_lines(latest_version));
        plain_message_state_from_lines(lines, HistoryCellType::Notice)
    } else if config.model == model {
        plain_message_state_from_lines(Vec::new(), HistoryCellType::Notice)
    } else {
        let lines = vec![
            Line::from("model changed:")
                .fg(crate::colors::keyword())
                .bold(),
            Line::from(format!("requested: {}", config.model)),
            Line::from(format!("used: {model}")),
            // No empty line at end - trimming and spacing handled by renderer
        ];
        plain_message_state_from_lines(lines, HistoryCellType::Notice)
    }
}

/// Build the common lines for the "Popular commands" section (without the leading
/// "notice" marker). Shared between the initial session info and the startup prelude.
fn popular_commands_lines(_latest_version: Option<&str>) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::styled(
        "Popular commands:",
        Style::default().fg(crate::colors::text_bright()),
    ));
    lines.push(Line::from(vec![
        Span::styled("/settings", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Settings.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
        Span::styled(
            " UPDATED",
            Style::default().fg(crate::colors::primary()),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("/auto", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Auto.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
        Span::styled(
            " UPDATED",
            Style::default().fg(crate::colors::primary()),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("/chrome", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Chrome.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("/plan", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Plan.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("/code", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Code.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("/skills", Style::default().fg(crate::colors::primary())),
        Span::from(" - "),
        Span::from(SlashCommand::Skills.description())
            .style(Style::default().add_modifier(Modifier::DIM)),
        Span::styled(
            " NEW",
            Style::default().fg(crate::colors::primary()),
        ),
    ]));

    lines
}

pub(crate) fn new_popular_commands_notice(
    _connecting_mcp: bool,
    latest_version: Option<&str>,
) -> PlainMessageState {
    if crate::chatwidget::is_test_mode() {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from(""));
        let legacy_line = "  /code - perform a coding task (multiple agents)";
        #[cfg(any(test, feature = "test-helpers"))]
        println!("legacy command line: {legacy_line}");
        lines.push(Line::from(legacy_line));
        return plain_message_state_from_lines(lines, HistoryCellType::Notice);
    }
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from("notice".dim()));
    lines.extend(popular_commands_lines(latest_version));
    // Connecting status is now rendered as a separate BackgroundEvent cell
    // with its own gutter icon and spacing. Keep this notice focused.
    plain_message_state_from_lines(lines, HistoryCellType::Notice)
}

pub(crate) fn new_user_prompt(message: String) -> PlainMessageState {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from("user"));
    // Sanitize user-provided text for terminal safety and stable layout:
    // - Normalize common TTY overwrite sequences (\r, \x08, ESC[K)
    // - Expand tabs to spaces with a fixed tab stop so wrapping is deterministic
    // - Parse ANSI sequences into spans so we never emit raw control bytes
    let normalized = normalize_overwrite_sequences(&message);
    let sanitized = sanitize_for_tui(
        &normalized,
        SanitizeMode::AnsiPreserving,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );
    // Build content lines with ANSI converted to styled spans
    let content: Vec<Line<'static>> = sanitized.lines().map(|l| ansi_escape_line(l)).collect();
    let content = trim_empty_lines(content);
    lines.extend(content);
    // No empty line at end - trimming and spacing handled by renderer
    plain_message_state_from_lines(lines, HistoryCellType::User)
}

/// Render a queued user message that will be sent in the next turn.
/// Visually identical to a normal user cell, but the header shows a
/// small "(queued)" suffix so it’s clear it hasn’t been executed yet.
pub(crate) fn new_queued_user_prompt(message: String) -> PlainMessageState {
    use ratatui::style::Style;
    use ratatui::text::Span;
    let mut lines: Vec<Line<'static>> = Vec::new();
    // Header: "user (queued)"
    lines.push(Line::from(vec![
        Span::from("user "),
        Span::from("(queued)").style(Style::default().fg(crate::colors::text_dim())),
    ]));
    // Normalize and render body like normal user messages
    let normalized = normalize_overwrite_sequences(&message);
    let sanitized = sanitize_for_tui(
        &normalized,
        SanitizeMode::AnsiPreserving,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );
    let content: Vec<Line<'static>> = sanitized.lines().map(|l| ansi_escape_line(l)).collect();
    let content = trim_empty_lines(content);
    lines.extend(content);
    plain_message_state_from_lines(lines, HistoryCellType::User)
}

#[allow(dead_code)]
pub(crate) fn new_text_line(line: Line<'static>) -> PlainMessageState {
    plain_message_state_from_lines(vec![line], HistoryCellType::Notice)
}

pub(crate) fn new_error_event(message: String) -> PlainMessageState {
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::styled(
        "error",
        Style::default()
            .fg(crate::colors::error())
            .add_modifier(Modifier::BOLD),
    ));
    let msg_norm = normalize_overwrite_sequences(&message);
    lines.extend(
        msg_norm
            .lines()
            .map(|line| ansi_escape_line(line).style(Style::default().fg(crate::colors::error()))),
    );
    // No empty line at end - trimming and spacing handled by renderer
    plain_message_state_from_lines(lines, HistoryCellType::Error)
}

pub(crate) fn new_reasoning_output(reasoning_effort: &ReasoningEffort) -> PlainMessageState {
    let lines = vec![
        Line::from(""),
        Line::from("Reasoning Effort")
            .fg(crate::colors::keyword())
            .bold(),
        Line::from(format!("Value: {}", reasoning_effort)),
    ];
    plain_message_state_from_lines(lines, HistoryCellType::Notice)
}

pub(crate) fn new_model_output(model: &str, effort: ReasoningEffort) -> PlainMessageState {
    let lines = vec![
        Line::from(""),
        Line::from("Model Selection")
            .fg(crate::colors::keyword())
            .bold(),
        Line::from(format!("Model: {}", model)),
        Line::from(format!("Reasoning Effort: {}", effort)),
    ];
    plain_message_state_from_lines(lines, HistoryCellType::Notice)
}

pub(crate) fn new_status_output(
    config: &Config,
    total_usage: &TokenUsage,
    last_usage: &TokenUsage,
) -> PlainMessageState {
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(Line::from("/status").fg(crate::colors::keyword()));
    lines.push(Line::from(""));

    // 🔧 Configuration
    lines.push(Line::from(vec!["🔧 ".into(), "Configuration".bold()]));

    // Prepare config summary with custom prettification
    let summary_entries = create_config_summary_entries(config);
    let summary_map: HashMap<String, String> = summary_entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect();

    let lookup = |key: &str| -> String { summary_map.get(key).unwrap_or(&String::new()).clone() };
    let title_case = |s: &str| -> String {
        s.split_whitespace()
            .map(|w| {
                let mut chars = w.chars();
                match chars.next() {
                    None => String::new(),
                    Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };

    // Format model name with proper capitalization
    let formatted_model = if config.model.to_lowercase().starts_with("gpt-") {
        format!("GPT{}", &config.model[3..])
    } else {
        config.model.clone()
    };
    lines.push(Line::from(vec![
        "  • Name: ".into(),
        formatted_model.into(),
    ]));
    let provider_disp = pretty_provider_name(&config.model_provider_id);
    lines.push(Line::from(vec![
        "  • Provider: ".into(),
        provider_disp.into(),
    ]));

    // Only show Reasoning fields if present in config summary
    let reff = lookup("reasoning effort");
    if !reff.is_empty() {
        lines.push(Line::from(vec![
            "  • Reasoning Effort: ".into(),
            title_case(&reff).into(),
        ]));
    }
    let rsum = lookup("reasoning summaries");
    if !rsum.is_empty() {
        lines.push(Line::from(vec![
            "  • Reasoning Summaries: ".into(),
            title_case(&rsum).into(),
        ]));
    }

    lines.push(Line::from(""));

    // 🔐 Authentication
    lines.push(Line::from(vec!["🔐 ".into(), "Authentication".bold()]));
    {
        use code_login::AuthMode;
        use code_login::CodexAuth;
        use code_login::OPENAI_API_KEY_ENV_VAR;
        use code_login::try_read_auth_json;

        // Determine effective auth mode the core would choose
        let auth_result = CodexAuth::from_code_home(
            &config.code_home,
            AuthMode::ChatGPT,
            &config.responses_originator_header,
        );

        match auth_result {
            Ok(Some(auth)) => match auth.mode {
                AuthMode::ApiKey => {
                    // Prefer suffix from auth.json; fall back to env var if needed
                    let suffix =
                        try_read_auth_json(&code_login::get_auth_file(&config.code_home))
                            .ok()
                            .and_then(|a| a.openai_api_key)
                            .or_else(|| std::env::var(OPENAI_API_KEY_ENV_VAR).ok())
                            .map(|k| key_suffix(&k))
                            .unwrap_or_else(|| "????".to_string());
                    lines.push(Line::from(format!("  • Method: API key (…{suffix})")));
                }
                AuthMode::ChatGPT => {
                    let account_id = auth
                        .get_account_id()
                        .unwrap_or_else(|| "unknown".to_string());
                    lines.push(Line::from(format!(
                        "  • Method: ChatGPT account (account_id: {account_id})"
                    )));
                }
            },
            _ => {
                lines.push(Line::from("  • Method: unauthenticated"));
            }
        }
    }

    lines.push(Line::from(""));

    // 📊 Token Usage
    lines.push(Line::from(vec!["📊 ".into(), "Token Usage".bold()]));
    // Input: <input> [+ <cached> cached]
    let mut input_line_spans: Vec<Span<'static>> = vec![
        "  • Input: ".into(),
        format_with_separators(last_usage.non_cached_input()).into(),
    ];
    if last_usage.cached_input_tokens > 0 {
        input_line_spans.push(
            format!(
                " (+ {} cached)",
                format_with_separators(last_usage.cached_input_tokens)
            )
            .into(),
        );
    }
    lines.push(Line::from(input_line_spans));
    // Output: <output>
    lines.push(Line::from(vec![
        "  • Output: ".into(),
        format_with_separators(last_usage.output_tokens).into(),
    ]));
    // Total: <total>
    lines.push(Line::from(vec![
        "  • Total: ".into(),
        format_with_separators(last_usage.blended_total()).into(),
    ]));
    lines.push(Line::from(vec![
        "  • Session total: ".into(),
        format_with_separators(total_usage.blended_total()).into(),
    ]));

    // 📐 Model Limits
    let context_window = config.model_context_window;
    let max_output_tokens = config.model_max_output_tokens;
    let auto_compact_limit = config.model_auto_compact_token_limit;

    if context_window.is_some() || max_output_tokens.is_some() || auto_compact_limit.is_some() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec!["📐 ".into(), "Model Limits".bold()]));

        if let Some(context_window) = context_window {
            let used = last_usage.tokens_in_context_window().min(context_window);
            let percent_full = if context_window > 0 {
                ((used as f64 / context_window as f64) * 100.0).min(100.0)
            } else {
                0.0
            };
            lines.push(Line::from(format!(
                "  • Context window: {} used of {} ({:.0}% full)",
                format_with_separators(used),
                format_with_separators(context_window),
                percent_full
            )));
        }

        if let Some(max_output_tokens) = max_output_tokens {
            lines.push(Line::from(format!(
                "  • Max output tokens: {}",
                format_with_separators(max_output_tokens)
            )));
        }

        match auto_compact_limit {
            Some(limit) if limit > 0 => {
                let limit_u64 = limit as u64;
                let remaining = limit_u64.saturating_sub(total_usage.total_tokens);
                lines.push(Line::from(format!(
                    "  • Auto-compact threshold: {} ({} remaining)",
                    format_with_separators(limit_u64),
                    format_with_separators(remaining)
                )));
                if total_usage.total_tokens > limit_u64 {
                    lines.push(Line::from("    • Compacting will trigger on the next turn".dim()));
                }
            }
            _ => {
                if let Some(window) = context_window {
                    if window > 0 {
                        let used = last_usage.tokens_in_context_window();
                        let remaining = window.saturating_sub(used);
                        let percent_left = if window == 0 {
                            0.0
                        } else {
                            (remaining as f64 / window as f64) * 100.0
                        };
                        lines.push(Line::from(format!(
                            "  • Context window: {} used of {} ({:.0}% left)",
                            format_with_separators(used),
                            format_with_separators(window),
                            percent_left
                        )));
                        lines.push(Line::from(format!(
                            "  • {} tokens before overflow",
                            format_with_separators(remaining)
                        )));
                        lines.push(Line::from("  • Auto-compaction runs after overflow errors".to_string()));
                    } else {
                        lines.push(Line::from("  • Auto-compaction runs after overflow errors".to_string()));
                    }
                } else {
                    lines.push(Line::from("  • Auto-compaction runs after overflow errors".to_string()));
                }
            }
        }
    }

    plain_message_state_from_lines(lines, HistoryCellType::Notice)
}

pub(crate) fn new_warning_event(message: String) -> PlainMessageState {
    let warn_style = Style::default().fg(crate::colors::warning());
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(2);
    lines.push(Line::from("notice"));
    lines.push(Line::from(vec![Span::styled(format!("⚠ {message}"), warn_style)]));
    plain_message_state_from_lines(lines, HistoryCellType::Notice)
}

pub(crate) fn new_prompts_output() -> PlainMessageState {
    let lines: Vec<Line<'static>> = vec![
        Line::from("/prompts").fg(crate::colors::keyword()),
        Line::from(""),
        Line::from(" 1. Explain this codebase"),
        Line::from(" 2. Summarize recent commits"),
        Line::from(" 3. Implement {feature}"),
        Line::from(" 4. Find and fix a bug in @filename"),
        Line::from(" 5. Write tests for @filename"),
        Line::from(" 6. Improve documentation in @filename"),
        Line::from(""),
    ];
    plain_message_state_from_lines(lines, HistoryCellType::Notice)
}
