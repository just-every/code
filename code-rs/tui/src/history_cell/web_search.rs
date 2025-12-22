use super::card_style::{
    ansi16_inverse_color,
    fill_card_background,
    hint_text_style,
    primary_text_style,
    rows_to_lines,
    secondary_text_style,
    title_text_style,
    truncate_with_ellipsis,
    web_search_card_style,
    CardRow,
    CardSegment,
    CardStyle,
    CARD_ACCENT_WIDTH,
};
use super::{HistoryCell, HistoryCellType, ToolCellStatus};
use crate::colors;
use crate::theme::{palette_mode, PaletteMode};
use code_common::elapsed::format_duration_digital;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

const BORDER_TOP: &str = "╭─";
const BORDER_BODY: &str = "│";
const BORDER_BOTTOM: &str = "╰─";
const HINT_TEXT: &str = " [Ctrl+S] Settings · [Esc] Stop";
const ACTION_TIME_INDENT: usize = 1;
const ACTION_TIME_SEPARATOR_WIDTH: usize = 2;
const ACTION_TIME_COLUMN_MIN_WIDTH: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WebSearchStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WebSearchActionKind {
    Info,
    Success,
    Error,
}

impl WebSearchActionKind {
    fn glyph(self) -> &'static str {
        match self {
            WebSearchActionKind::Info => "•",
            WebSearchActionKind::Success => "✓",
            WebSearchActionKind::Error => "✗",
        }
    }
}

#[derive(Clone, Debug)]
struct WebSearchAction {
    text: String,
    kind: WebSearchActionKind,
    timestamp: Duration,
}

impl WebSearchAction {
    fn new(text: String, kind: WebSearchActionKind, timestamp: Duration) -> Self {
        Self {
            text,
            kind,
            timestamp,
        }
    }

}

#[derive(Clone)]
pub(crate) struct WebSearchSessionCell {
    query: Option<String>,
    status: WebSearchStatus,
    actions: Vec<WebSearchAction>,
    duration: Option<Duration>,
    cell_key: Option<String>,
    signature: Option<String>,
}

impl WebSearchSessionCell {
    pub(crate) fn new() -> Self {
        Self {
            query: None,
            status: WebSearchStatus::Running,
            actions: Vec::new(),
            duration: None,
            cell_key: None,
            signature: None,
        }
    }

    pub(crate) fn set_query(&mut self, query: Option<String>) -> bool {
        let previous = self.query.clone();
        self.query = query
            .and_then(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });
        self.query != previous
    }

    pub(crate) fn set_status(&mut self, status: WebSearchStatus) {
        self.status = status;
    }

    fn record_action(
        &mut self,
        timestamp: Duration,
        text: impl Into<String>,
        kind: WebSearchActionKind,
    ) {
        self.actions
            .push(WebSearchAction::new(text.into(), kind, timestamp));
    }

    pub(crate) fn record_info(&mut self, timestamp: Duration, text: impl Into<String>) {
        self.record_action(timestamp, text, WebSearchActionKind::Info);
    }

    pub(crate) fn record_success(&mut self, timestamp: Duration, text: impl Into<String>) {
        self.record_action(timestamp, text, WebSearchActionKind::Success);
    }

    pub(crate) fn record_error(&mut self, timestamp: Duration, text: impl Into<String>) {
        self.record_action(timestamp, text, WebSearchActionKind::Error);
    }

    pub(crate) fn set_duration(&mut self, duration: Option<Duration>) {
        self.duration = duration;
    }

    pub(crate) fn ensure_started_message(&mut self) {
        if self.actions.is_empty() {
            self.record_action(Duration::ZERO, "Searching…", WebSearchActionKind::Info);
        }
    }

    pub(crate) fn tool_title(&self) -> &'static str {
        "Web Search"
    }

    pub(crate) fn status_label(&self) -> &'static str {
        match self.status {
            WebSearchStatus::Running => "Running",
            WebSearchStatus::Completed => "Completed",
            WebSearchStatus::Failed => "Failed",
        }
    }

    fn accent_style(style: &CardStyle) -> Style {
        if palette_mode() == PaletteMode::Ansi16 {
            return Style::default().fg(ansi16_inverse_color());
        }
        let dim = colors::mix_toward(style.accent_fg, style.text_secondary, 0.85);
        Style::default().fg(dim)
    }

    fn build_card_rows(&self, width: u16, style: &CardStyle) -> Vec<CardRow> {
        if width == 0 {
            return Vec::new();
        }

        let accent_width = CARD_ACCENT_WIDTH.min(width as usize);
        let body_width = width
            .saturating_sub(accent_width as u16)
            .saturating_sub(1) as usize;
        if body_width == 0 {
            return Vec::new();
        }

        let mut rows: Vec<CardRow> = Vec::new();

        rows.push(self.title_row(body_width, style));
        rows.push(self.blank_border_row(body_width, style));

        rows.extend(self.actions_section_rows(body_width, style));

        rows.push(self.blank_border_row(body_width, style));
        rows.push(self.bottom_border_row(body_width, style));

        rows
    }

    fn title_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        let mut segments: Vec<CardSegment> = Vec::new();
        let title_text = " Web Search";
        let status_text = format!(" · {}", self.status_label());
        let total = format!("{title_text}{status_text}");
        let title_style = if palette_mode() == PaletteMode::Ansi16 {
            Style::default().fg(ansi16_inverse_color())
        } else {
            title_text_style(style)
        };
        let status_style = if palette_mode() == PaletteMode::Ansi16 {
            Style::default().fg(ansi16_inverse_color())
        } else {
            secondary_text_style(style)
        };

        if UnicodeWidthStr::width(total.as_str()) <= body_width {
            segments.push(CardSegment::new(
                title_text.to_string(),
                title_style,
            ));
            segments.push(CardSegment::new(
                status_text,
                status_style,
            ));
        } else {
            let display = truncate_with_ellipsis(title_text, body_width);
            segments.push(CardSegment::new(display, title_style));
        }

        CardRow::new(
            BORDER_TOP.to_string(),
            Self::accent_style(style),
            segments,
            None,
        )
    }

    fn blank_border_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        if body_width == 0 {
            return CardRow::new(
                BORDER_BODY.to_string(),
                Self::accent_style(style),
                Vec::new(),
                None,
            );
        }

        let filler = " ".repeat(body_width);
        CardRow::new(
            BORDER_BODY.to_string(),
            Self::accent_style(style),
            vec![CardSegment::new(filler, Style::default())],
            None,
        )
    }

    fn actions_section_rows(&self, body_width: usize, style: &CardStyle) -> Vec<CardRow> {
        if body_width == 0 {
            return Vec::new();
        }

        let mut rows = Vec::new();
        rows.push(self.actions_heading_row(body_width, style));

        if self.actions.is_empty() {
            rows.push(self.placeholder_row(body_width, style));
            return rows;
        }

        let elapsed_labels: Vec<String> = self
            .actions
            .iter()
            .map(|action| format!(" {}", format_duration_digital(action.timestamp)))
            .collect();

        let time_width = elapsed_labels
            .iter()
            .map(|label| UnicodeWidthStr::width(label.as_str()))
            .max()
            .unwrap_or(0)
            .max(ACTION_TIME_COLUMN_MIN_WIDTH);

        let indent_text = " ".repeat(ACTION_TIME_INDENT);
        let indent_style = secondary_text_style(style);
        let time_style = primary_text_style(style);
        let separator_text = if ACTION_TIME_SEPARATOR_WIDTH > 0 {
            Some(" ".repeat(ACTION_TIME_SEPARATOR_WIDTH))
        } else {
            None
        };

        for (action, elapsed) in self.actions.iter().zip(elapsed_labels.iter()) {
            let mut segments = Vec::new();
            segments.push(CardSegment::new(indent_text.clone(), indent_style));

            let mut remaining = body_width.saturating_sub(ACTION_TIME_INDENT);
            if remaining == 0 {
                rows.push(CardRow::new(
                    BORDER_BODY.to_string(),
                    Self::accent_style(style),
                    segments,
                    None,
                ));
                continue;
            }

            let padded_time = format!("{elapsed:<width$}", width = time_width);
            segments.push(CardSegment::new(padded_time, time_style));
            remaining = remaining.saturating_sub(time_width);

            if let Some(separator) = separator_text.as_ref() {
                if remaining < ACTION_TIME_SEPARATOR_WIDTH {
                    rows.push(CardRow::new(
                        BORDER_BODY.to_string(),
                        Self::accent_style(style),
                        segments,
                        None,
                    ));
                    continue;
                }
                segments.push(CardSegment::new(separator.clone(), Style::default()));
                remaining = remaining.saturating_sub(ACTION_TIME_SEPARATOR_WIDTH);
            }

            if remaining == 0 {
                rows.push(CardRow::new(
                    BORDER_BODY.to_string(),
                    Self::accent_style(style),
                    segments,
                    None,
                ));
                continue;
            }

            let description_text = format!("{} {}", action.kind.glyph(), action.text);
            let display = truncate_with_ellipsis(description_text.as_str(), remaining);
            let mut description_segment = CardSegment::new(display, secondary_text_style(style));
            description_segment.inherit_background = true;
            segments.push(description_segment);

            rows.push(CardRow::new(
                BORDER_BODY.to_string(),
                Self::accent_style(style),
                segments,
                None,
            ));
        }

        rows
    }

    fn actions_heading_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        if body_width == 0 {
            return CardRow::new(
                BORDER_BODY.to_string(),
                Self::accent_style(style),
                Vec::new(),
                None,
            );
        }

        let mut segments = Vec::new();
        if ACTION_TIME_INDENT > 0 {
            segments.push(CardSegment::new(
                " ".repeat(ACTION_TIME_INDENT),
                secondary_text_style(style),
            ));
        }

        let available = body_width.saturating_sub(ACTION_TIME_INDENT);
        if available > 0 {
            let title = truncate_with_ellipsis("Actions", available);
            let mut heading = CardSegment::new(title, primary_text_style(style));
            heading.inherit_background = true;
            segments.push(heading);
        }

        CardRow::new(
            BORDER_BODY.to_string(),
            Self::accent_style(style),
            segments,
            None,
        )
    }

    fn placeholder_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        if body_width == 0 {
            return CardRow::new(
                BORDER_BODY.to_string(),
                Self::accent_style(style),
                Vec::new(),
                None,
            );
        }

        let mut segments = Vec::new();
        if ACTION_TIME_INDENT > 0 {
            segments.push(CardSegment::new(
                " ".repeat(ACTION_TIME_INDENT),
                secondary_text_style(style),
            ));
        }

        let available = body_width.saturating_sub(ACTION_TIME_INDENT);
        if available > 0 {
            let message = truncate_with_ellipsis("Awaiting web search activity", available);
            let mut placeholder = CardSegment::new(message, secondary_text_style(style));
            placeholder.inherit_background = true;
            segments.push(placeholder);
        }

        CardRow::new(
            BORDER_BODY.to_string(),
            Self::accent_style(style),
            segments,
            None,
        )
    }

    fn bottom_border_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        let text = truncate_with_ellipsis(HINT_TEXT, body_width);
        let hint_style = if palette_mode() == PaletteMode::Ansi16 {
            Style::default().fg(ansi16_inverse_color())
        } else {
            hint_text_style(style)
        };
        let mut segment = CardSegment::new(text, hint_style);
        segment.inherit_background = true;
        CardRow::new(
            BORDER_BOTTOM.to_string(),
            Self::accent_style(style),
            vec![segment],
            None,
        )
    }

    fn desired_rows(&self, width: u16) -> usize {
        let style = web_search_card_style();
        let trimmed_width = width.saturating_sub(2);
        if trimmed_width == 0 {
            return 0;
        }
        self.build_card_rows(trimmed_width, &style).len().max(1)
    }

    fn render_rows(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        if area.width <= 2 || area.height == 0 {
            return;
        }
        let style = web_search_card_style();
        let draw_width = area.width - 2;
        let render_area = Rect {
            width: draw_width,
            ..area
        };
        fill_card_background(buf, render_area, &style);
        let rows = self.build_card_rows(render_area.width, &style);
        let lines = rows_to_lines(&rows, &style, render_area.width);
        let text = Text::from(lines);
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((skip_rows, 0))
            .render(render_area, buf);

        let clear_start = area.x + draw_width;
        let clear_end = area.x + area.width;
        for x in clear_start..clear_end {
            for row in 0..area.height {
                let cell = &mut buf[(x, area.y + row)];
                cell.set_symbol(" ");
                cell.set_bg(crate::colors::background());
            }
        }
    }

    pub(crate) fn set_tool_card_key_internal(&mut self, key: Option<String>) {
        self.cell_key = key;
    }

    pub(crate) fn set_signature(&mut self, signature: Option<String>) {
        self.signature = signature;
    }

    pub(crate) fn current_tool_card_key(&self) -> Option<&str> {
        self.cell_key.as_deref()
    }

    pub(crate) fn signature(&self) -> Option<&str> {
        self.signature.as_deref()
    }
}

impl crate::chatwidget::tool_cards::ToolCardCell for WebSearchSessionCell {
    fn tool_card_key(&self) -> Option<&str> {
        self.current_tool_card_key()
    }

    fn set_tool_card_key(&mut self, key: Option<String>) {
        self.set_tool_card_key_internal(key);
    }

    fn dedupe_signature(&self) -> Option<String> {
        self.signature().map(|value| value.to_string())
    }
}

impl HistoryCell for WebSearchSessionCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn gutter_symbol(&self) -> Option<&'static str> {
        None
    }

    fn kind(&self) -> HistoryCellType {
        let status = match self.status {
            WebSearchStatus::Running => ToolCellStatus::Running,
            WebSearchStatus::Completed => ToolCellStatus::Success,
            WebSearchStatus::Failed => ToolCellStatus::Failed,
        };
        HistoryCellType::Tool { status }
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::styled(
            format!("{} — {}", self.tool_title(), self.status_label()),
            Style::default().add_modifier(Modifier::BOLD),
        ));
        if let Some(query) = &self.query {
            lines.push(Line::from(format!("query: {query}")));
        }
        for action in &self.actions {
            lines.push(Line::from(format!("- {}", action.text)));
        }
        lines
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.desired_rows(width) as u16
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        self.render_rows(area, buf, skip_rows);
    }
}
