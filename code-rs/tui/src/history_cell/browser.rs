use super::card_style::{
    ansi16_inverse_color,
    browser_card_style,
    fill_card_background,
    hint_text_style,
    primary_text_style,
    rows_to_lines,
    secondary_text_style,
    title_text_style,
    truncate_with_ellipsis,
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
use ratatui::prelude::Style;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use ratatui_image::{Image, Resize};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::FilterType;
use image::ImageReader;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use url::Url;
use std::time::Duration;
use unicode_width::UnicodeWidthChar;

const BORDER_TOP: &str = "╭─";
const BORDER_BODY: &str = "│";
const BORDER_BOTTOM: &str = "╰─";

const MAX_ACTIONS: usize = 24;
const MAX_CONSOLE: usize = 12;
const ACTION_DISPLAY_HEAD: usize = 4;
const ACTION_DISPLAY_TAIL: usize = 4;
const MIN_SCREENSHOT_ROWS: usize = 6;
const MAX_SCREENSHOT_ROWS: usize = 60;
const DEFAULT_TEXT_INDENT: usize = 2;
const TEXT_RIGHT_PADDING: usize = 2;
const SCREENSHOT_GAP: usize = 2;
const SCREENSHOT_MIN_WIDTH: usize = 18;
const SCREENSHOT_MAX_WIDTH: usize = 64;
const SCREENSHOT_LEFT_PAD: usize = 1;
const MIN_TEXT_WIDTH: usize = 28;
const ACTION_LABEL_GAP: usize = 2;
const ACTION_TIME_GAP: usize = 2;
const ACTION_TIME_COLUMN_MIN_WIDTH: usize = 2;
const MAX_SCREENSHOT_HISTORY: usize = 24;

#[derive(Clone)]
pub(crate) struct BrowserScreenshotRecord {
    pub path: PathBuf,
    pub url: Option<String>,
    pub timestamp: Duration,
}
pub(crate) struct BrowserSessionCell {
    url: Option<String>,
    title: Option<String>,
    actions: Vec<BrowserAction>,
    console_messages: Vec<String>,
    screenshot_path: Option<String>,
    screenshot_history: Vec<BrowserScreenshotRecord>,
    total_duration: Duration,
    completed: bool,
    cell_key: Option<String>,
    headless: Option<bool>,
    status_code: Option<String>,
    cached_picker: Rc<RefCell<Option<ratatui_image::picker::Picker>>>,
    cached_image_protocol: Rc<RefCell<Option<(PathBuf, ratatui::layout::Rect, ratatui_image::protocol::Protocol)>>>,
}

impl Clone for BrowserSessionCell {
    fn clone(&self) -> Self {
        Self {
            url: self.url.clone(),
            title: self.title.clone(),
            actions: self.actions.clone(),
            console_messages: self.console_messages.clone(),
            screenshot_path: self.screenshot_path.clone(),
            screenshot_history: self.screenshot_history.clone(),
            total_duration: self.total_duration,
            completed: self.completed,
            cell_key: self.cell_key.clone(),
            headless: self.headless,
            status_code: self.status_code.clone(),
            cached_picker: Rc::clone(&self.cached_picker),
            cached_image_protocol: Rc::clone(&self.cached_image_protocol),
        }
    }
}

impl Default for BrowserSessionCell {
    fn default() -> Self {
        Self {
            url: None,
            title: None,
            actions: Vec::new(),
            console_messages: Vec::new(),
            screenshot_path: None,
            screenshot_history: Vec::new(),
            total_duration: Duration::ZERO,
            completed: false,
            cell_key: None,
            headless: None,
            status_code: None,
            cached_picker: Rc::new(RefCell::new(None)),
            cached_image_protocol: Rc::new(RefCell::new(None)),
        }
    }
}

struct ScreenshotLayout {
    start_row: usize,
    height_rows: usize,
    width_cols: usize,
    indent_cols: usize,
}

#[derive(Clone)]
struct ActionEntry {
    label: String,
    detail: String,
    time_label: String,
}

enum ActionDisplayLine {
    Entry(ActionEntry),
    Ellipsis,
}

#[derive(Clone)]
struct BrowserAction {
    action: String,
    target: Option<String>,
    value: Option<String>,
    outcome: Option<String>,
    timestamp: Duration,
}

impl BrowserSessionCell {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn ensure_picker_initialized(
        &self,
        picker: Option<Picker>,
        font_size: (u16, u16),
    ) {
        let mut slot = self.cached_picker.borrow_mut();
        if slot.is_some() {
            return;
        }
        if let Some(p) = picker {
            *slot = Some(p);
        } else {
            *slot = Some(Picker::from_fontsize(font_size));
        }
    }

    pub(crate) fn set_url(&mut self, url: impl Into<String>) {
        self.url = Some(url.into());
    }

    pub(crate) fn summary_label(&self) -> String {
        self.display_label()
    }

    pub(crate) fn current_url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub(crate) fn record_action(
        &mut self,
        timestamp: Duration,
        duration: Duration,
        action: String,
        target: Option<String>,
        value: Option<String>,
        outcome: Option<String>,
    ) {
        if self.actions.last().map_or(false, |last| {
            last.action == action
                && last.target == target
                && last.value == value
                && last.outcome == outcome
        }) {
            return;
        }
        let action_entry = BrowserAction {
            action,
            target,
            value,
            outcome: outcome.clone(),
            timestamp,
        };
        self.actions.push(action_entry);
        if self.actions.len() > MAX_ACTIONS {
            let overflow = self.actions.len() - MAX_ACTIONS;
            self.actions.drain(0..overflow);
        }
        let finish = timestamp.saturating_add(duration);
        if finish > self.total_duration {
            self.total_duration = finish;
        }
        if let Some(outcome) = outcome {
            if let Some(code) = extract_status_code(&outcome) {
                self.status_code = Some(code);
            }
        }
    }

    pub(crate) fn add_console_message(&mut self, message: String) {
        if self
            .console_messages
            .last()
            .map_or(false, |last| last == &message)
        {
            return;
        }
        self.console_messages.push(message);
        if self.console_messages.len() > MAX_CONSOLE {
            let overflow = self.console_messages.len() - MAX_CONSOLE;
            self.console_messages.drain(0..overflow);
        }
    }

    pub(crate) fn record_screenshot(
        &mut self,
        timestamp: Duration,
        path: PathBuf,
        url: Option<String>,
    ) {
        let display_path = path.display().to_string();
        self.screenshot_path = Some(display_path);
        self.screenshot_history.push(BrowserScreenshotRecord {
            path,
            url,
            timestamp,
        });
        if self.screenshot_history.len() > MAX_SCREENSHOT_HISTORY {
            let overflow = self.screenshot_history.len() - MAX_SCREENSHOT_HISTORY;
            self.screenshot_history.drain(0..overflow);
        }
        self.cached_image_protocol.borrow_mut().take();
    }

    pub(crate) fn set_headless(&mut self, headless: Option<bool>) {
        self.headless = headless;
    }

    pub(crate) fn set_status_code(&mut self, code: Option<String>) {
        self.status_code = code;
    }

    pub(crate) fn set_cell_key(&mut self, key: Option<String>) {
        self.cell_key = key;
    }

    pub(crate) fn cell_key(&self) -> Option<&str> {
        self.cell_key.as_deref()
    }

    pub(crate) fn screenshot_history(&self) -> &[BrowserScreenshotRecord] {
        &self.screenshot_history
    }

    pub(crate) fn total_duration(&self) -> Duration {
        self.total_duration
    }

    pub(crate) fn full_action_entries(&self) -> Vec<(String, String, String)> {
        let show_minutes = self.total_duration.as_secs() >= 60;
        let mut entries: Vec<(String, String, String)> = Vec::new();
        if self.actions.is_empty() {
            if let Some(url) = self.url.as_ref() {
                let time_label = format!(
                    " {}",
                    Self::format_elapsed_label(Duration::ZERO, show_minutes)
                );
                entries.push((time_label, "Opened".to_string(), url.clone()));
            }
            return entries;
        }

        for action in &self.actions {
            let time_label = format!(
                " {}",
                Self::format_elapsed_label(action.timestamp, show_minutes)
            );
            let entry = format_action_entry(action, time_label);
            entries.push((entry.time_label, entry.label, entry.detail));
        }

        entries
    }

    fn accent_style(style: &CardStyle) -> Style {
        if palette_mode() == PaletteMode::Ansi16 {
            return Style::default().fg(ansi16_inverse_color());
        }
        let dim = colors::mix_toward(style.accent_fg, style.text_secondary, 0.85);
        Style::default().fg(dim)
    }

    fn normalized_title(&self) -> Option<String> {
        self.title
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("(pending)"))
            .map(|value| value.to_string())
    }

    fn header_summary_text(&self) -> String {
        let label = if self.headless.unwrap_or(true) {
            "Browser (headless)"
        } else {
            "Browser"
        };

        let mut title = format!("{}: {}", label, self.display_label());
        if let Some(code) = &self.status_code {
            title.push_str(&format!(" [{}]", code));
        }
        title
    }

    fn top_border_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        let mut segments = Vec::new();
        if body_width == 0 {
            return CardRow::new(
                BORDER_TOP.to_string(),
                Self::accent_style(style),
                segments,
                None,
            );
        }

        let title_style = if palette_mode() == PaletteMode::Ansi16 {
            Style::default().fg(ansi16_inverse_color())
        } else {
            title_text_style(style)
        };

        segments.push(CardSegment::new(" ".to_string(), title_style));
        let remaining = body_width.saturating_sub(1);
        let text = truncate_with_ellipsis(self.header_summary_text().as_str(), remaining);
        if !text.is_empty() {
            segments.push(CardSegment::new(text, title_style));
        }
        CardRow::new(BORDER_TOP.to_string(), Self::accent_style(style), segments, None)
    }

    fn blank_border_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        CardRow::new(
            BORDER_BODY.to_string(),
            Self::accent_style(style),
            vec![CardSegment::new(" ".repeat(body_width), Style::default())],
            None,
        )
    }

    fn body_text_row(
        &self,
        text: impl Into<String>,
        body_width: usize,
        style: &CardStyle,
        text_style: Style,
        indent_cols: usize,
        right_padding_cols: usize,
    ) -> CardRow {
        if body_width == 0 {
            return CardRow::new(BORDER_BODY.to_string(), Self::accent_style(style), Vec::new(), None);
        }
        let indent = indent_cols.min(body_width.saturating_sub(1));
        let available = body_width.saturating_sub(indent);
        let mut segments = Vec::new();
        if indent > 0 {
            segments.push(CardSegment::new(" ".repeat(indent), Style::default()));
        }
        let text: String = text.into();
        if available == 0 {
            return CardRow::new(BORDER_BODY.to_string(), Self::accent_style(style), segments, None);
        }
        let usable_width = available.saturating_sub(right_padding_cols);
        let display = if usable_width == 0 {
            String::new()
        } else {
            truncate_with_ellipsis(text.as_str(), usable_width)
        };
        segments.push(CardSegment::new(display, text_style));
        if right_padding_cols > 0 && available > 0 {
            let pad = right_padding_cols.min(available);
            segments.push(CardSegment::new(" ".repeat(pad), Style::default()));
        }
        CardRow::new(BORDER_BODY.to_string(), Self::accent_style(style), segments, None)
    }

    fn bottom_border_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        let text_value = format!(" [Ctrl+B] View · [Esc] Stop");
        let text = truncate_with_ellipsis(text_value.as_str(), body_width);
        let hint_style = if palette_mode() == PaletteMode::Ansi16 {
            Style::default().fg(ansi16_inverse_color())
        } else {
            hint_text_style(style)
        };
        let segment = CardSegment::new(text, hint_style);
        CardRow::new(BORDER_BOTTOM.to_string(), Self::accent_style(style), vec![segment], None)
    }

    fn display_host(&self) -> Option<String> {
        self
            .url
            .as_ref()
            .and_then(|url| Url::parse(url).ok())
            .and_then(|parsed| parsed.host_str().map(|host| host.to_string()))
    }

    fn display_label(&self) -> String {
        if let Some(title) = self.normalized_title() {
            return title;
        }
        if let Some(host) = self.display_host() {
            return host;
        }
        self
            .url
            .as_ref()
            .map(|url| url.clone())
            .unwrap_or_else(|| "Browser Session".to_string())
    }

    fn build_card_rows(&self, width: u16, style: &CardStyle) -> (Vec<CardRow>, Option<ScreenshotLayout>) {
        if width == 0 {
            return (Vec::new(), None);
        }

        let accent_width = CARD_ACCENT_WIDTH.min(width as usize);
        let body_width = width
            .saturating_sub(accent_width as u16)
            .saturating_sub(1) as usize;
        if body_width == 0 {
            return (Vec::new(), None);
        }

        let mut rows: Vec<CardRow> = Vec::new();
        rows.push(self.top_border_row(body_width, style));
        rows.push(self.blank_border_row(body_width, style));

        let mut screenshot_layout = self.compute_screenshot_layout(body_width);
        let indent_cols = screenshot_layout
            .as_ref()
            .map(|layout| layout.indent_cols)
            .unwrap_or(DEFAULT_TEXT_INDENT);
        let indent_cols = indent_cols.min(body_width.saturating_sub(1));
        let right_padding = TEXT_RIGHT_PADDING.min(body_width);

        let content_start = rows.len();

        let show_minutes = self.total_duration.as_secs() >= 60;
        let action_display = self.formatted_action_display(show_minutes);
        let label_width = action_display
            .iter()
            .filter_map(|line| match line {
                ActionDisplayLine::Entry(entry) => Some(string_display_width(entry.label.as_str())),
                ActionDisplayLine::Ellipsis => None,
            })
            .max()
            .unwrap_or(0);
        let time_width = action_display
            .iter()
            .filter_map(|line| match line {
                ActionDisplayLine::Entry(entry) => {
                    Some(string_display_width(entry.time_label.as_str()))
                }
                ActionDisplayLine::Ellipsis => None,
            })
            .max()
            .unwrap_or(0)
            .max(ACTION_TIME_COLUMN_MIN_WIDTH);

        rows.push(self.body_text_row(
            "Actions",
            body_width,
            style,
            primary_text_style(style),
            indent_cols,
            right_padding,
        ));

        if action_display.is_empty() {
            for wrapped in wrap_card_lines(
                "No browser actions yet",
                body_width,
                indent_cols,
                right_padding,
            ) {
                rows.push(self.body_text_row(
                    wrapped,
                    body_width,
                    style,
                    secondary_text_style(style),
                    indent_cols,
                    right_padding,
                ));
            }
        } else {
            for line in action_display {
                match line {
                    ActionDisplayLine::Entry(entry) => {
                        let entry_rows = self.render_action_entry_rows(
                            &entry,
                            body_width,
                            style,
                            indent_cols,
                            right_padding,
                            label_width,
                            time_width,
                        );
                        rows.extend(entry_rows);
                    }
                    ActionDisplayLine::Ellipsis => {
                        let mut segments = Vec::new();
                        if indent_cols > 0 {
                            segments.push(CardSegment::new(" ".repeat(indent_cols), Style::default()));
                        }
                        let padded = format!("{:>width$}", "⋮", width = time_width.max(1));
                        segments.push(CardSegment::new(
                            padded,
                            secondary_text_style(style),
                        ));
                        if ACTION_TIME_GAP > 0 {
                            segments.push(CardSegment::new(
                                " ".repeat(ACTION_TIME_GAP),
                                Style::default(),
                            ));
                        }
                        rows.push(CardRow::new(
                            BORDER_BODY.to_string(),
                            Self::accent_style(style),
                            segments,
                            None,
                        ));
                    }
                }
            }
        }

        let console_rows = self.console_rows(body_width, style, indent_cols, right_padding);
        if !console_rows.is_empty() {
            rows.push(self.blank_border_row(body_width, style));
            rows.extend(console_rows);
        }

        if let Some(layout) = screenshot_layout.as_mut() {
            layout.start_row = content_start;
            let existing = rows.len().saturating_sub(content_start);
            if existing < layout.height_rows {
                let missing = layout.height_rows - existing;
                for _ in 0..missing {
                    rows.push(self.body_text_row(
                        "",
                        body_width,
                        style,
                        Style::default(),
                        indent_cols,
                        right_padding,
                    ));
                }
            }
        }

        rows.push(self.blank_border_row(body_width, style));
        rows.push(self.bottom_border_row(body_width, style));

        (rows, screenshot_layout)
    }

    fn console_rows(
        &self,
        body_width: usize,
        style: &CardStyle,
        indent_cols: usize,
        right_padding: usize,
    ) -> Vec<CardRow> {
        let last = match self.console_messages.last() {
            Some(value) => value.clone(),
            None => return Vec::new(),
        };
        let style_color = if last.contains('⚠') {
            Style::default().fg(colors::warning())
        } else {
            secondary_text_style(style)
        };
        let text = format!("Console: {}", last);
        wrap_card_lines(text.as_str(), body_width, indent_cols, right_padding)
            .into_iter()
            .map(|wrapped| {
                self.body_text_row(
                    wrapped,
                    body_width,
                    style,
                    style_color,
                    indent_cols,
                    right_padding,
                )
            })
            .collect()
    }

    fn render_action_entry_rows(
        &self,
        entry: &ActionEntry,
        body_width: usize,
        style: &CardStyle,
        indent_cols: usize,
        right_padding: usize,
        label_width: usize,
        time_width: usize,
    ) -> Vec<CardRow> {
        if body_width == 0 || time_width == 0 {
            return Vec::new();
        }

        let indent = indent_cols.min(body_width.saturating_sub(1));
        let available = body_width.saturating_sub(indent);
        if available <= time_width {
            return self.render_fallback_entry(entry, body_width, style, indent_cols, right_padding, time_width);
        }

        const TIME_GAP: usize = ACTION_TIME_GAP;
        let after_time = available.saturating_sub(time_width);
        if after_time <= TIME_GAP {
            return self.render_fallback_entry(entry, body_width, style, indent_cols, right_padding, time_width);
        }
        let after_time_gap = after_time.saturating_sub(TIME_GAP);
        if after_time_gap == 0 {
            return self.render_fallback_entry(entry, body_width, style, indent_cols, right_padding, time_width);
        }

        let base_available = after_time_gap.saturating_sub(right_padding);
        if base_available == 0 {
            return self.render_fallback_entry(entry, body_width, style, indent_cols, right_padding, time_width);
        }

        let max_label_width = base_available.saturating_sub(ACTION_LABEL_GAP + 1);
        if max_label_width == 0 {
            return self.render_fallback_entry(entry, body_width, style, indent_cols, right_padding, time_width);
        }

        let effective_label_width = label_width.min(max_label_width);
        let detail_width = base_available
            .saturating_sub(effective_label_width + ACTION_LABEL_GAP);
        if detail_width == 0 {
            return self.render_fallback_entry(entry, body_width, style, indent_cols, right_padding, time_width);
        }

        let label_full_width = string_display_width(entry.label.as_str());
        if effective_label_width < label_full_width {
            return self.render_fallback_entry(entry, body_width, style, indent_cols, right_padding, time_width);
        }

        let label_display = entry.label.clone();
        let label_padding = effective_label_width.saturating_sub(label_full_width);
        let gap = " ".repeat(ACTION_LABEL_GAP);

        let mut lines = wrap_line_to_width(entry.detail.as_str(), detail_width);
        if lines.is_empty() {
            lines.push(String::new());
        }

        let time_column = format!("{:<width$}", entry.time_label.as_str(), width = time_width);
        let continuation_time = " ".repeat(time_width);

        let label_column = format!("{}{}", label_display, " ".repeat(label_padding));
        let continuation_label = " ".repeat(effective_label_width);

        let mut rows = Vec::new();
        let label_style = secondary_text_style(style);
        let detail_style = primary_text_style(style);
        let time_style = primary_text_style(style);

        let indent_string = if indent > 0 {
            Some(" ".repeat(indent))
        } else {
            None
        };

        if let Some(first) = lines.first() {
            rows.push(self.build_action_row(
                body_width,
                style,
                indent_string.as_deref().unwrap_or(""),
                time_column.as_str(),
                time_width,
                TIME_GAP,
                label_column.as_str(),
                effective_label_width,
                &gap,
                first,
                indent,
                right_padding,
                time_style,
                label_style,
                detail_style,
            ));
        }

        for detail_line in lines.iter().skip(1) {
            rows.push(self.build_action_row(
                body_width,
                style,
                indent_string.as_deref().unwrap_or(""),
                continuation_time.as_str(),
                time_width,
                TIME_GAP,
                continuation_label.as_str(),
                effective_label_width,
                &gap,
                detail_line,
                indent,
                right_padding,
                time_style,
                label_style,
                detail_style,
            ));
        }

        rows
    }

    fn build_action_row(
        &self,
        body_width: usize,
        style: &CardStyle,
        indent: &str,
        time: &str,
        time_width: usize,
        time_gap: usize,
        label: &str,
        label_width: usize,
        gap: &str,
        detail: &str,
        indent_cols: usize,
        right_padding: usize,
        time_style: Style,
        label_style: Style,
        detail_style: Style,
    ) -> CardRow {
        let mut segments = Vec::new();
        let mut consumed = 0usize;
        if !indent.is_empty() {
            segments.push(CardSegment::new(indent.to_string(), Style::default()));
            consumed += indent_cols;
        }

        if !time.is_empty() {
            segments.push(CardSegment::new(time.to_string(), time_style));
            consumed += time_width;
        }

        if time_gap > 0 {
            segments.push(CardSegment::new(" ".repeat(time_gap), Style::default()));
            consumed += time_gap;
        }

        if !label.is_empty() {
            segments.push(CardSegment::new(label.to_string(), label_style));
            consumed += label_width;
        }

        if !gap.is_empty() {
            segments.push(CardSegment::new(gap.to_string(), Style::default()));
            consumed += ACTION_LABEL_GAP;
        }

        segments.push(CardSegment::new(detail.to_string(), detail_style));
        consumed += string_display_width(detail);

        let available = body_width.saturating_sub(consumed);
        if available > 0 {
            let pad = available.min(right_padding);
            if pad > 0 {
                segments.push(CardSegment::new(" ".repeat(pad), Style::default()));
            }
        }

        CardRow::new(BORDER_BODY.to_string(), Self::accent_style(style), segments, None)
    }

    fn render_fallback_entry(
        &self,
        entry: &ActionEntry,
        body_width: usize,
        style: &CardStyle,
        indent_cols: usize,
        _right_padding: usize,
        time_width: usize,
    ) -> Vec<CardRow> {
        if body_width == 0 {
            return Vec::new();
        }

        let indent = indent_cols.min(body_width.saturating_sub(1));
        let available = body_width.saturating_sub(indent);
        if available == 0 {
            return Vec::new();
        }

        let mut segments = Vec::new();
        if indent > 0 {
            segments.push(CardSegment::new(" ".repeat(indent), Style::default()));
        }

        let time_style = Style::default().fg(colors::text());
        let effective_time_width = available.min(time_width).max(ACTION_TIME_COLUMN_MIN_WIDTH.min(available));
        if effective_time_width == 0 {
            return Vec::new();
        }
        let time_display = format!("{:<width$}", entry.time_label.as_str(), width = effective_time_width);
        segments.push(CardSegment::new(time_display, time_style));

        let mut remaining = available.saturating_sub(effective_time_width);
        if remaining >= ACTION_TIME_GAP {
            segments.push(CardSegment::new(" ".repeat(ACTION_TIME_GAP), Style::default()));
            remaining = remaining.saturating_sub(ACTION_TIME_GAP);
        }

        if remaining > 0 {
            let combined = if entry.detail.is_empty() {
                entry.label.clone()
            } else {
                format!("{} {}", entry.label.trim(), entry.detail.trim())
            };
            let rest_display = truncate_with_ellipsis(combined.trim(), remaining);
            segments.push(CardSegment::new(rest_display, primary_text_style(style)));
        }

        vec![CardRow::new(
            BORDER_BODY.to_string(),
            Self::accent_style(style),
            segments,
            None,
        )]
    }

    fn formatted_action_display(&self, show_minutes: bool) -> Vec<ActionDisplayLine> {
        let mut entries: Vec<ActionEntry> = Vec::new();
        let has_actions = !self.actions.is_empty();
        if !has_actions {
            if let Some(url) = self.url.as_ref() {
                entries.push(ActionEntry {
                    label: "Opened".to_string(),
                    detail: url.clone(),
                    time_label: format!(" {}", Self::format_elapsed_label(Duration::ZERO, show_minutes)),
                });
            }
        }

        entries.extend(self.actions.iter().map(|action| {
            let time_label = format!(
                " {}",
                Self::format_elapsed_label(action.timestamp, show_minutes)
            );
            format_action_entry(action, time_label)
        }));

        if entries.is_empty() {
            return Vec::new();
        }

        if entries.len() > ACTION_DISPLAY_HEAD + ACTION_DISPLAY_TAIL {
            let mut display: Vec<ActionDisplayLine> = Vec::new();
            for entry in entries.iter().take(ACTION_DISPLAY_HEAD) {
                display.push(ActionDisplayLine::Entry(entry.clone()));
            }
            display.push(ActionDisplayLine::Ellipsis);
            let tail = entries
                .iter()
                .rev()
                .take(ACTION_DISPLAY_TAIL)
                .cloned()
                .collect::<Vec<_>>();
            for entry in tail.into_iter().rev() {
                display.push(ActionDisplayLine::Entry(entry));
            }
            display
        } else {
            entries
                .into_iter()
                .map(ActionDisplayLine::Entry)
                .collect()
        }
    }

    pub(crate) fn format_elapsed_label(duration: Duration, _show_minutes: bool) -> String {
        format_duration_digital(duration)
    }

    fn compute_screenshot_layout(&self, body_width: usize) -> Option<ScreenshotLayout> {
        if self.screenshot_path.is_none() {
            return None;
        }

        if body_width
            < SCREENSHOT_LEFT_PAD + SCREENSHOT_MIN_WIDTH + SCREENSHOT_GAP + MIN_TEXT_WIDTH + TEXT_RIGHT_PADDING
        {
            return None;
        }

        let max_screenshot = body_width
            .saturating_sub(SCREENSHOT_LEFT_PAD + MIN_TEXT_WIDTH + SCREENSHOT_GAP + TEXT_RIGHT_PADDING);
        if max_screenshot < SCREENSHOT_MIN_WIDTH {
            return None;
        }

        let mut screenshot_cols = max_screenshot;
        if screenshot_cols > SCREENSHOT_MAX_WIDTH {
            screenshot_cols = SCREENSHOT_MAX_WIDTH;
        }
        if screenshot_cols < SCREENSHOT_MIN_WIDTH {
            screenshot_cols = SCREENSHOT_MIN_WIDTH;
        }

        let rows = self.compute_screenshot_rows(screenshot_cols)?;
        Some(ScreenshotLayout {
            start_row: 0,
            height_rows: rows,
            width_cols: screenshot_cols,
            indent_cols: SCREENSHOT_LEFT_PAD + screenshot_cols + SCREENSHOT_GAP,
        })
    }

    fn ensure_picker(&self) -> Picker {
        let mut picker_ref = self.cached_picker.borrow_mut();
        if picker_ref.is_none() {
            *picker_ref = Some(Picker::from_fontsize((8, 16)));
        }
        picker_ref.as_ref().unwrap().clone()
    }

fn compute_screenshot_rows(&self, screenshot_cols: usize) -> Option<usize> {
        if screenshot_cols == 0 {
            return None;
        }
        let path = Path::new(self.screenshot_path.as_ref()?);

        let picker = self.ensure_picker();
        let (cell_w, cell_h) = picker.font_size();
        if cell_w == 0 || cell_h == 0 {
            return Some(MIN_SCREENSHOT_ROWS);
        }

        let (img_w, img_h) = match image::image_dimensions(path) {
            Ok(dim) if dim.0 > 0 && dim.1 > 0 => dim,
            _ => return Some(MIN_SCREENSHOT_ROWS),
        };

        let cols = screenshot_cols as u32;
        if cols == 0 {
            return None;
        }

        let cw = cell_w as u32;
        let ch = cell_h as u32;
        let img_w = img_w as u32;
        let img_h = img_h as u32;

        let rows_by_w = (cols * cw * img_h) as f64 / (img_w * ch) as f64;
        let rows = rows_by_w.ceil().max(1.0) as usize;
        Some(rows.clamp(MIN_SCREENSHOT_ROWS, MAX_SCREENSHOT_ROWS))
    }

    fn build_plain_summary(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let status = if self.completed { "done" } else { "running" };
        lines.push(format!("Browser Session: {} [{}]", self.display_label(), status));
        if let Some(url) = &self.url {
            lines.push(format!("Opened: {}", url));
        }
        if let Some(code) = &self.status_code {
            lines.push(format!("Status: {}", code));
        }
        for action in self
            .actions
            .iter()
            .rev()
            .take(3)
            .rev()
        {
            lines.push(format!("Action: {}", format_action_line(action)));
        }
        if let Some(path) = &self.screenshot_path {
            lines.push(format!("Screenshot: {}", path));
        }
        lines
    }
}

impl HistoryCell for BrowserSessionCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn gutter_symbol(&self) -> Option<&'static str> {
        if self.completed {
            Some("✔")
        } else {
            None
        }
    }

    fn kind(&self) -> HistoryCellType {
        let status = if self.completed {
            ToolCellStatus::Success
        } else {
            ToolCellStatus::Running
        };
        HistoryCellType::Tool { status }
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        self.build_plain_summary().into_iter().map(Line::from).collect()
    }

    fn desired_height(&self, width: u16) -> u16 {
        let style = browser_card_style();
        let trimmed_width = width.saturating_sub(2);
        if trimmed_width == 0 {
            return 0;
        }
        let (rows, _) = self.build_card_rows(trimmed_width, &style);
        rows.len().max(1) as u16
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        if area.width <= 2 || area.height == 0 {
            return;
        }

        let style = browser_card_style();
        let draw_width = area.width - 2;
        let render_area = Rect {
            width: draw_width,
            ..area
        };

        fill_card_background(buf, render_area, &style);
        let (rows, screenshot_meta) = self.build_card_rows(render_area.width, &style);
        let lines = rows_to_lines(&rows, &style, render_area.width);
        let text = Text::from(lines);

        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((skip_rows, 0))
            .render(render_area, buf);

        if let Some(layout) = screenshot_meta.as_ref() {
            if let Some(path) = self.screenshot_path.as_ref() {
                self.render_screenshot_preview(render_area, buf, skip_rows, layout, path);
            }
        }

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
}

impl BrowserSessionCell {
    fn render_screenshot_preview(
        &self,
        area: Rect,
        buf: &mut Buffer,
        skip_rows: u16,
        layout: &ScreenshotLayout,
        path_str: &str,
    ) {
        let accent_width = CARD_ACCENT_WIDTH.min(area.width as usize) as u16;
        if accent_width >= area.width {
            return;
        }

        let viewport_top = skip_rows as usize;
        let viewport_bottom = viewport_top + area.height as usize;
        let shot_top = layout.start_row;
        let shot_bottom = layout.start_row + layout.height_rows;

        if shot_bottom <= viewport_top || shot_top >= viewport_bottom {
            return;
        }

        let visible_top = shot_top.max(viewport_top);
        let visible_bottom = shot_bottom.min(viewport_bottom);
        if visible_bottom <= visible_top {
            return;
        }

        let body_width = area.width.saturating_sub(accent_width);
        if body_width == 0 {
            return;
        }

        let left_pad = SCREENSHOT_LEFT_PAD.min(body_width as usize) as u16;
        if body_width <= left_pad {
            return;
        }

        let usable_width = body_width.saturating_sub(left_pad);
        let screenshot_width = layout.width_cols.min(usable_width as usize) as u16;
        if screenshot_width == 0 {
            return;
        }


        let path = Path::new(path_str);
        let rows_to_copy = (visible_bottom - visible_top) as u16;
        if rows_to_copy == 0 {
            return;
        }

        let dest_x = area.x + accent_width + left_pad;
        let dest_y = area.y + (visible_top - viewport_top) as u16;
        let placeholder_area = Rect {
            x: dest_x,
            y: dest_y,
            width: screenshot_width,
            height: rows_to_copy,
        };

        if !path.exists() {
            self.render_screenshot_placeholder(path, placeholder_area, buf);
            return;
        }

        let full_height = layout.height_rows as u16;
        if full_height == 0 {
            return;
        }

        let picker = self.ensure_picker();
        let supports_partial_render = matches!(picker.protocol_type(), ProtocolType::Halfblocks);
        let is_partially_visible = visible_top != shot_top || visible_bottom != shot_bottom;
        if is_partially_visible && !supports_partial_render {
            self.render_screenshot_placeholder(path, placeholder_area, buf);
            return;
        }

        if !is_partially_visible {
            let protocol_target = Rect::new(0, 0, screenshot_width, full_height);
            if self.ensure_protocol(path, protocol_target, &picker).is_err() {
                self.render_screenshot_placeholder(path, placeholder_area, buf);
                return;
            }
            let dest_target = Rect::new(dest_x, dest_y, screenshot_width, full_height);
            if let Some((_, _, protocol)) = self.cached_image_protocol.borrow_mut().as_mut() {
                let image = Image::new(protocol);
                image.render(dest_target, buf);
            } else {
                self.render_screenshot_placeholder(path, placeholder_area, buf);
            }
            return;
        }

        if !supports_partial_render {
            self.render_screenshot_placeholder(path, placeholder_area, buf);
            return;
        }

        let offscreen = match self.render_screenshot_buffer(path, screenshot_width, full_height) {
            Ok(buffer) => buffer,
            Err(_) => {
                self.render_screenshot_placeholder(path, placeholder_area, buf);
                return;
            }
        };

        let src_start_row = (visible_top - shot_top) as u16;
        let area_bottom = area.y + area.height;
        let area_right = area.x + area.width;

        for row in 0..rows_to_copy {
            let dest_row = dest_y + row;
            if dest_row >= area_bottom {
                break;
            }
            let src_row = src_start_row + row;
            for col in 0..screenshot_width {
                let dest_col = dest_x + col;
                if dest_col >= area_right {
                    break;
                }
                let Some(src_cell) = offscreen.cell((col, src_row)) else { continue; };
                if let Some(dest_cell) = buf.cell_mut((dest_col, dest_row)) {
                    *dest_cell = src_cell.clone();
                }
            }
        }
    }

    fn render_screenshot_placeholder(&self, path: &Path, area: Rect, buf: &mut Buffer) {
        use ratatui::style::{Modifier, Style};
        use ratatui::widgets::{Block, Borders};

        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("screenshot");
        let placeholder_text = format!("Screenshot:\n{}", filename);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(colors::info()))
            .title("Browser");
        let inner = block.inner(area);
        block.render(area, buf);
        Paragraph::new(placeholder_text)
            .style(
                Style::default()
                    .fg(colors::text_dim())
                    .add_modifier(Modifier::ITALIC),
            )
            .wrap(Wrap { trim: true })
            .render(inner, buf);
    }

    fn render_screenshot_buffer(&self, path: &Path, width: u16, height: u16) -> Result<Buffer, ()> {
        if width == 0 || height == 0 {
            return Err(());
        }

        let picker = self.ensure_picker();
        let target = Rect::new(0, 0, width, height);
        self.ensure_protocol(path, target, &picker)?;

        let mut buffer = Buffer::empty(target);
        if let Some((_, _, protocol)) = self.cached_image_protocol.borrow_mut().as_mut() {
            let image = Image::new(protocol);
            image.render(target, &mut buffer);
            Ok(buffer)
        } else {
            Err(())
        }
    }

    fn ensure_protocol(&self, path: &Path, target: Rect, picker: &Picker) -> Result<(), ()> {
        let mut cache = self.cached_image_protocol.borrow_mut();
        let needs_recreate = match cache.as_ref() {
            Some((cached_path, cached_rect, _)) => cached_path != path || *cached_rect != target,
            None => true,
        };

        if needs_recreate {
            let dyn_img = match ImageReader::open(path) {
                Ok(reader) => reader.decode().map_err(|_| ())?,
                Err(_) => return Err(()),
            };
            let protocol = picker
                .new_protocol(dyn_img, target, Resize::Fit(Some(FilterType::Lanczos3)))
                .map_err(|_| ())?;
            *cache = Some((path.to_path_buf(), target, protocol));
        }

        Ok(())
    }

}

fn wrap_card_lines(text: &str, body_width: usize, indent_cols: usize, right_padding: usize) -> Vec<String> {
    let available = body_width
        .saturating_sub(indent_cols)
        .saturating_sub(right_padding);
    if available == 0 {
        return vec![String::new()];
    }
    wrap_line_to_width(text, available)
}

fn wrap_line_to_width(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    if text.trim().is_empty() {
        return vec![String::new()];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for word in text.split_whitespace() {
        let mut word_parts = if string_display_width(word) > width {
            split_long_card_word(word, width)
        } else {
            vec![word.to_string()]
        };

        for part in word_parts.drain(..) {
            let part_width = string_display_width(part.as_str());
            if current.is_empty() {
                current.push_str(part.as_str());
                current_width = part_width;
            } else if current_width + 1 + part_width > width {
                lines.push(current);
                current = part.clone();
                current_width = part_width;
            } else {
                current.push(' ');
                current.push_str(part.as_str());
                current_width += 1 + part_width;
            }
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn split_long_card_word(word: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }

    let mut parts = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for ch in word.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(1);
        if current_width + ch_width > width && !current.is_empty() {
            parts.push(current);
            current = String::new();
            current_width = 0;
        }
        current.push(ch);
        current_width += ch_width;
    }

    if !current.is_empty() {
        parts.push(current);
    }

    if parts.is_empty() {
        parts.push(String::new());
    }
    parts
}

fn string_display_width(text: &str) -> usize {
    text
        .chars()
        .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
        .sum()
}

fn format_action_summary(action: &BrowserAction) -> String {
    match (&action.target, &action.value, &action.outcome) {
        (Some(target), Some(value), Some(outcome)) => {
            format!("{} {} → {}", action.action, target, outcome_for_display(outcome, value))
        }
        (Some(target), Some(value), None) => {
            format!("{} {} = {}", action.action, target, value)
        }
        (Some(target), None, Some(outcome)) => {
            format!("{} {} → {}", action.action, target, outcome)
        }
        (Some(target), None, None) => format!("{} {}", action.action, target),
        (None, Some(value), Some(outcome)) => {
            format!("{} {} → {}", action.action, value, outcome)
        }
        (None, Some(value), None) => format!("{} {}", action.action, value),
        (None, None, Some(outcome)) => format!("{} → {}", action.action, outcome),
        _ => action.action.clone(),
    }
}

fn format_action_entry(action: &BrowserAction, time_label: String) -> ActionEntry {
    let action_lower = action.action.to_ascii_lowercase();
    match action_lower.as_str() {
        "click" | "mouse_click" => {
            let target = action.target.as_deref().unwrap_or("").trim();
            let detail = if target.starts_with('(') && target.ends_with(')') {
                format!("at {}", target)
            } else if !target.is_empty() {
                target.to_string()
            } else if let Some(value) = action.value.as_deref() {
                value.trim().to_string()
            } else if let Some(outcome) = action.outcome.as_deref() {
                outcome.trim().to_string()
            } else {
                String::new()
            };
            ActionEntry {
                label: "Clicked".to_string(),
                detail,
                time_label,
            }
        }
        "press_key" | "key" | "press" => {
            let key_raw = action
                .value
                .as_deref()
                .or(action.outcome.as_deref())
                .or(action.target.as_deref())
                .unwrap_or("?")
                .trim();
            let key = sanitize_pressed_detail(key_raw);
            ActionEntry {
                label: "Pressed".to_string(),
                detail: key,
                time_label,
            }
        }
        "type" | "input" | "enter_text" | "fill" | "insert_text" => {
            let typed = action
                .value
                .as_deref()
                .or(action.outcome.as_deref())
                .unwrap_or("?")
                .trim()
                .to_string();
            ActionEntry {
                label: "Typed".to_string(),
                detail: typed,
                time_label,
            }
        }
        "navigate" | "open" | "nav" => {
            let dest = action
                .target
                .as_deref()
                .map(|value| sanitize_nav_text(value))
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    action
                        .value
                        .as_deref()
                        .map(|value| sanitize_nav_text(value))
                        .filter(|s| !s.is_empty())
                })
                .or_else(|| {
                    action
                        .outcome
                        .as_deref()
                        .map(|value| sanitize_nav_text(value))
                        .filter(|s| !s.is_empty())
                })
                .unwrap_or_else(|| "".to_string());
            ActionEntry {
                label: "Opened".to_string(),
                detail: dest,
                time_label,
            }
        }
        other if other.starts_with("scroll") => {
            let detail = action
                .value
                .as_deref()
                .filter(|v| !v.trim().is_empty())
                .map(|v| v.trim().to_string())
                .or_else(|| {
                    action
                        .outcome
                        .as_deref()
                        .filter(|o| !o.trim().is_empty())
                        .map(|o| o.trim().to_string())
                })
                .or_else(|| {
                    action
                        .target
                        .as_deref()
                        .filter(|t| !t.trim().is_empty())
                        .map(|t| t.trim().to_string())
                })
                .unwrap_or_else(|| {
                    format_action_summary(action)
                        .strip_prefix(other)
                        .map(|suffix| suffix.trim_start_matches(|c| c == ' ' || c == ':' || c == '-'))
                        .filter(|suffix| !suffix.is_empty())
                        .map(|suffix| suffix.to_string())
                        .unwrap_or_else(|| format_action_summary(action))
                });
            ActionEntry {
                label: "Scrolled".to_string(),
                detail,
                time_label,
            }
        }
        _ => {
            let summary = format_action_summary(action);
            let label = titleize_action(action.action.as_str());
            let trimmed = summary
                .strip_prefix(action.action.as_str())
                .map(|suffix| suffix.trim_start_matches(|c| c == ' ' || c == ':' || c == '-'))
                .filter(|suffix| !suffix.is_empty())
                .map(|suffix| suffix.to_string())
                .unwrap_or_else(|| summary.clone());
            ActionEntry {
                label,
                detail: trimmed,
                time_label,
            }
        }
    }
}

fn titleize_action(raw: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    for segment in raw.split(['_', '-']).filter(|part| !part.is_empty()) {
        let mut chars = segment.chars();
        if let Some(first) = chars.next() {
            let first_upper = first.to_uppercase().collect::<String>();
            let rest = chars.as_str().to_ascii_lowercase();
            words.push(format!("{}{}", first_upper, rest));
        }
    }
    if words.is_empty() {
        raw.to_string()
    } else {
        words.join(" ")
    }
}

fn sanitize_pressed_detail(raw: &str) -> String {
    let mut candidate = raw;
    const PREFIXES: &[&str] = &[
        "pressed key:",
        "press key:",
        "key pressed:",
        "key:",
    ];
    for prefix in PREFIXES {
        if let Some(rest) = strip_prefix_ignore_case(candidate, prefix) {
            candidate = rest;
            break;
        }
    }
    let cleaned = candidate.trim();
    if cleaned.is_empty() {
        raw.trim().to_string()
    } else {
        cleaned.to_string()
    }
}

fn sanitize_nav_text(raw: &str) -> String {
    let mut candidate = raw;
    const PREFIXES: &[&str] = &[
        "browser opened to:",
        "opened to:",
        "navigated to",
        "nav to:",
        "opened:",
    ];
    for prefix in PREFIXES {
        if let Some(rest) = strip_prefix_ignore_case(candidate, prefix) {
            candidate = rest;
            break;
        }
    }
    let cleaned = candidate.trim().trim_start_matches(':').trim();
    cleaned.to_string()
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let text_bytes = text.as_bytes();
    let prefix_bytes = prefix.as_bytes();
    if text_bytes.len() < prefix_bytes.len() {
        return None;
    }
    for (idx, prefix_byte) in prefix_bytes.iter().enumerate() {
        if text_bytes[idx].to_ascii_lowercase() != prefix_byte.to_ascii_lowercase() {
            return None;
        }
    }
    Some(text.get(prefix.len()..)?.trim_start())
}

fn format_action_line(action: &BrowserAction) -> String {
    let action_lower = action.action.to_ascii_lowercase();
    let target = action.target.as_deref().unwrap_or("?");
    let value = action.value.as_deref();
    let outcome = action.outcome.as_deref();

    match action_lower.as_str() {
        "click" | "mouse_click" => {
            let display = target.trim();
            if display.starts_with('(') {
                format!("Clicked at {}", display)
            } else if !display.is_empty() {
                format!("Clicked {}", display)
            } else {
                "Clicked".to_string()
            }
        }
        "press_key" | "key" | "press" => {
            let key = value.or(outcome).unwrap_or("?");
            format!("Pressed key: {}", key)
        }
        "type" | "input" | "enter_text" | "fill" | "insert_text" => {
            let typed = value.or(outcome).unwrap_or("?");
            format!("Typed: {}", typed)
        }
        "navigate" | "open" => {
            let dest = value
                .or(action.target.as_deref())
                .or(outcome)
                .unwrap_or("?");
            format!("Navigated to {}", dest)
        }
        other => {
            let summary = format_action_summary(action);
            if summary.is_empty() {
                other.to_string()
            } else {
                summary
            }
        }
    }
}

fn outcome_for_display(outcome: &str, value: &str) -> String {
    if outcome == "value set" {
        value.to_string()
    } else {
        outcome.to_string()
    }
}

fn extract_status_code(outcome: &str) -> Option<String> {
    let trimmed = outcome.trim();
    if trimmed.len() < 3 {
        return None;
    }
    let code: String = trimmed
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if code.len() == 3 {
        Some(code)
    } else {
        None
    }
}


impl crate::chatwidget::tool_cards::ToolCardCell for BrowserSessionCell {
    fn tool_card_key(&self) -> Option<&str> {
        self.cell_key()
    }

    fn set_tool_card_key(&mut self, key: Option<String>) {
        self.set_cell_key(key);
    }
}
