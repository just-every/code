use super::card_style::{
    auto_drive_card_style,
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
use crate::card_theme;
use crate::glitch_animation::{gradient_multi, mix_rgb};
use crate::gradient_background::{GradientBackground, RevealRender};
use crate::util::buffer::fill_rect;
use crate::colors;
use crate::theme::{palette_mode, PaletteMode};
use code_common::elapsed::format_duration_digital;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use std::f32;
use std::time::{Duration, Instant};
use std::env;

const BORDER_TOP: &str = "╭─";
const BORDER_BODY: &str = "│";
const BORDER_BOTTOM: &str = "╰─";
const HINT_TEXT: &str = " [Ctrl+S] Settings · [Esc] Stop";
const ACTION_TIME_INDENT: usize = 1;
const ACTION_TIME_SEPARATOR_WIDTH: usize = 2;
const ACTION_TIME_COLUMN_MIN_WIDTH: usize = 6;
const CELEBRATION_ASCII: [&str; 4] = [
    " ▗▄▄▖ ▗▄▖ ▗▖  ▗▖▗▄▄▖ ▗▖   ▗▄▄▄▖▗▄▄▄▖▗▄▄▄▖",
    "▐▌   ▐▌ ▐▌▐▛▚▞▜▌▐▌ ▐▌▐▌   ▐▌     █  ▐▌  ",
    "▐▌   ▐▌ ▐▌▐▌  ▐▌▐▛▀▘ ▐▌   ▐▛▀▀▘  █  ▐▛▀▀▘",
    "▝▚▄▄▖▝▚▄▞▘▐▌  ▐▌▐▌   ▐▙▄▄▖▐▙▄▄▖  █  ▐▙▄▄▖",
];
const CELEBRATION_SPARKLE_CHOICES: &[char] = &['*', '+', 'x', '·', '•', '✶'];
const CELEBRATION_FRAME_INTERVAL: Duration = Duration::from_millis(120);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoDriveStatus {
    Running,
    Paused,
    Failed,
    Stopped,
}

impl AutoDriveStatus {
    fn label(self) -> &'static str {
        match self {
            AutoDriveStatus::Running => "Running",
            AutoDriveStatus::Paused => "Paused",
            AutoDriveStatus::Failed => "Failed",
            AutoDriveStatus::Stopped => "Stopped",
        }
    }

    fn tool_status(self) -> ToolCellStatus {
        match self {
            AutoDriveStatus::Running | AutoDriveStatus::Paused => ToolCellStatus::Running,
            AutoDriveStatus::Stopped => ToolCellStatus::Success,
            AutoDriveStatus::Failed => ToolCellStatus::Failed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoDriveActionKind {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug)]
struct AutoDriveAction {
    text: String,
    kind: AutoDriveActionKind,
    elapsed: Duration,
}

impl AutoDriveAction {
    fn new(text: String, kind: AutoDriveActionKind, elapsed: Duration) -> Self {
        Self { text, kind, elapsed }
    }
}

#[derive(Clone)]
pub(crate) struct AutoDriveCardCell {
    goal: Option<String>,
    status: AutoDriveStatus,
    actions: Vec<AutoDriveAction>,
    cell_key: Option<String>,
    signature: Option<String>,
    background_override: Option<Color>,
    reveal_started_at: Option<Instant>,
    first_action_at: Option<Instant>,
    completion_message: Option<String>,
    celebration_started_at: Option<Instant>,
}

impl AutoDriveCardCell {
    pub(crate) fn new(goal: Option<String>) -> Self {
        let reveal_started_at = if palette_mode() == PaletteMode::Ansi16 {
            None
        } else {
            active_auto_drive_theme()
                .theme
                .reveal
                .map(|_| Instant::now())
        };
        let cell = Self {
            goal: goal.and_then(Self::normalize_text),
            status: AutoDriveStatus::Running,
            actions: Vec::new(),
            cell_key: None,
            signature: None,
            background_override: None,
            reveal_started_at,
            first_action_at: None,
            completion_message: None,
            celebration_started_at: None,
        };
        cell
    }

    pub(crate) fn set_goal(&mut self, goal: Option<String>) {
        self.goal = goal.and_then(Self::normalize_text);
    }

    #[cfg(any(test, feature = "test-helpers"))]
    #[allow(dead_code)]
    pub(crate) fn goal_text(&self) -> Option<&str> {
        self.goal.as_deref()
    }

    #[cfg(any(test, feature = "test-helpers"))]
    #[allow(dead_code)]
    pub(crate) fn action_texts(&self) -> Vec<String> {
        self.actions.iter().map(|action| action.text.clone()).collect()
    }

    fn normalize_text(value: String) -> Option<String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    pub(crate) fn set_status(&mut self, status: AutoDriveStatus) {
        self.status = status;
    }

    pub(crate) fn set_background_override(&mut self, background: Option<Color>) {
        self.background_override = background;
    }

    pub(crate) fn disable_reveal(&mut self) {
        self.reveal_started_at = None;
    }

    pub(crate) fn push_action(&mut self, text: impl Into<String>, kind: AutoDriveActionKind) {
        let now = Instant::now();
        let base = self.first_action_at.get_or_insert(now);
        let elapsed = now.saturating_duration_since(*base);
        self.actions
            .push(AutoDriveAction::new(text.into(), kind, elapsed));
    }

    pub(crate) fn set_completion_message(&mut self, message: Option<String>) {
        self.completion_message = message.and_then(Self::normalize_text);
    }

    pub(crate) fn start_celebration(&mut self, message: Option<String>) {
        self.celebration_started_at = Some(Instant::now());
        if let Some(msg) = message.and_then(Self::normalize_text) {
            self.completion_message = Some(msg);
        }
        self.status = AutoDriveStatus::Stopped;
    }

    pub(crate) fn stop_celebration(&mut self) {
        self.celebration_started_at = None;
    }

    fn reveal_progress(&self) -> Option<(f32, card_theme::CardThemeDefinition)> {
        let theme = active_auto_drive_theme();
        let reveal = theme.theme.reveal?;
        let started = self.reveal_started_at?;
        let duration = reveal.duration.as_secs_f32();
        if duration <= f32::EPSILON {
            return None;
        }
        let elapsed = started.elapsed().as_secs_f32();
        let progress = (elapsed / duration).clamp(0.0, 1.0);
        Some((progress, theme))
    }

    fn accent_style(style: &CardStyle) -> Style {
        if palette_mode() == PaletteMode::Ansi16 {
            return Style::default().fg(style.accent_fg);
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

        if self.celebration_started_at.is_some() {
            rows.extend(self.build_celebration_rows(body_width, style));
            return rows;
        }

        rows.push(self.title_row(body_width, style));
        rows.push(self.celebration_blank_row(body_width, style));

        if let Some(goal) = &self.goal {
            let goal_rows = self.goal_rows(goal.as_str(), body_width, style);
            if !goal_rows.is_empty() {
                rows.extend(goal_rows);
                rows.push(self.blank_row(body_width, style));
            }
        }

        rows.push(self.actions_heading_row(body_width, style));
        let action_rows = self.action_rows(body_width, style);
        if action_rows.is_empty() {
            rows.push(self.actions_placeholder_row(body_width, style));
        } else {
            rows.extend(action_rows);
        }

        rows.push(self.blank_row(body_width, style));

        if let Some(completion) = self.completion_message.as_ref() {
            rows.push(self.complete_heading_row(body_width, style));
            let mut completion_rows = self.complete_message_rows(completion, body_width, style);
            if completion_rows.is_empty() {
                rows.push(self.blank_row(body_width, style));
            } else {
                rows.append(&mut completion_rows);
            }
            rows.push(self.blank_row(body_width, style));
        }

        rows.push(self.bottom_border_row(body_width, style));

        rows
    }

    fn build_celebration_rows(&self, body_width: usize, style: &CardStyle) -> Vec<CardRow> {
        let mut rows: Vec<CardRow> = Vec::new();

        rows.push(self.title_row(body_width, style));
        rows.push(self.blank_row(body_width, style));

        let lines = self.celebration_body_lines(body_width);
        for (line_index, line) in lines.into_iter().enumerate() {
            let segments = if line_index > 0 && line_index - 1 < CELEBRATION_ASCII.len() {
                self.celebration_ascii_segments(line)
            } else {
                vec![CardSegment::new(line, Self::celebration_background_style())]
            };

            rows.push(CardRow::new(
                BORDER_BODY.to_string(),
                Self::accent_style(style),
                segments,
                None,
            ));
        }

        rows.push(self.celebration_blank_row(body_width, style));
        rows.push(self.bottom_border_row(body_width, style));

        rows
    }

    fn title_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        let mut segments: Vec<CardSegment> = Vec::new();
        let title_text = " Auto Drive";
        let status_text = if self.celebration_started_at.is_some() {
            " · Complete".to_string()
        } else {
            format!(" · {}", self.status.label())
        };
        let combined = format!("{title_text}{status_text}");
        let title_style = title_text_style(style);
        let status_style = if palette_mode() == PaletteMode::Ansi16 {
            title_text_style(style)
        } else {
            secondary_text_style(style)
        };

        if UnicodeWidthStr::width(combined.as_str()) <= body_width {
            let mut bold_title = title_style;
            bold_title = bold_title.add_modifier(Modifier::BOLD);
            segments.push(CardSegment::new(
                title_text.to_string(),
                bold_title,
            ));
            segments.push(CardSegment::new(
                status_text,
                status_style,
            ));
        } else {
            let display = truncate_with_ellipsis(title_text, body_width);
            let mut bold_title = title_style;
            bold_title = bold_title.add_modifier(Modifier::BOLD);
            segments.push(CardSegment::new(display, bold_title));
        }

        CardRow::new(
            BORDER_TOP.to_string(),
            Self::accent_style(style),
            segments,
            None,
        )
    }

    fn blank_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        let filler = " ".repeat(body_width);
        CardRow::new(
            BORDER_BODY.to_string(),
            Self::accent_style(style),
            vec![CardSegment::new(filler, Style::default())],
            None,
        )
    }

    fn celebration_blank_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        let filler = " ".repeat(body_width);
        CardRow::new(
            BORDER_BODY.to_string(),
            Self::accent_style(style),
            vec![CardSegment::new(filler, Self::celebration_background_style())],
            None,
        )
    }

    fn goal_rows(&self, goal: &str, body_width: usize, style: &CardStyle) -> Vec<CardRow> {
        if body_width == 0 {
            return Vec::new();
        }
        let cleaned = goal.trim();
        if cleaned.is_empty() {
            return Vec::new();
        }

        let indent = if body_width > 1 { " " } else { "" };
        let indent_width = UnicodeWidthStr::width(indent);
        let text_width = body_width
            .saturating_sub(indent_width)
            .max(1);
        let wrapped = Self::wrap_goal_text(cleaned, text_width);

        wrapped
            .into_iter()
            .map(|line| {
                let content = if line.is_empty() {
                    String::new()
                } else {
                    format!("{indent}{line}")
                };
                let text_style = if palette_mode() == PaletteMode::Ansi16 {
                    primary_text_style(style)
                } else {
                    secondary_text_style(style)
                };
                let mut segment = CardSegment::new(content, text_style);
                segment.inherit_background = true;
                CardRow::new(
                    BORDER_BODY.to_string(),
                    Self::accent_style(style),
                    vec![segment],
                    None,
                )
            })
            .collect()
    }

    fn wrap_goal_text(text: &str, width: usize) -> Vec<String> {
        if width == 0 {
            return vec![String::new()];
        }

        let mut rows: Vec<String> = Vec::new();
        for paragraph in text.split('\n') {
            let trimmed = paragraph.trim_end();
            if trimmed.is_empty() {
                rows.push(String::new());
                continue;
            }
            rows.extend(Self::wrap_goal_line(trimmed, width));
        }

        if rows.is_empty() {
            rows.push(String::new());
        }

        rows
    }

    fn wrap_goal_line(line: &str, width: usize) -> Vec<String> {
        if width == 0 {
            return vec![String::new()];
        }

        let mut rows: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut current_width = 0usize;

        for word in line.split_whitespace() {
            let word_width = UnicodeWidthStr::width(word);
            if word_width == 0 {
                continue;
            }
            if word_width > width {
                if !current.is_empty() {
                    rows.push(current);
                    current = String::new();
                    current_width = 0;
                }
                rows.extend(Self::break_long_word(word, width));
                continue;
            }

            let separator_width = if current.is_empty() { 0 } else { 1 };
            if current_width + separator_width + word_width > width {
                if !current.is_empty() {
                    rows.push(current);
                }
                current = word.to_string();
                current_width = word_width;
            } else {
                if separator_width > 0 {
                    current.push(' ');
                }
                current.push_str(word);
                current_width += separator_width + word_width;
            }
        }

        if !current.is_empty() {
            rows.push(current);
        }

        if rows.is_empty() {
            rows.push(String::new());
        }

        rows
    }

    fn break_long_word(word: &str, width: usize) -> Vec<String> {
        if width == 0 {
            return vec![String::new()];
        }

        let mut rows: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut current_width = 0usize;

        for ch in word.chars() {
            let char_width = UnicodeWidthChar::width(ch).unwrap_or(1);
            if current_width + char_width > width {
                if !current.is_empty() {
                    rows.push(current);
                }
                current = String::new();
                current_width = 0;
            }
            current.push(ch);
            current_width += char_width;
        }

        if !current.is_empty() {
            rows.push(current);
        }

        if rows.is_empty() {
            rows.push(String::new());
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

    fn actions_placeholder_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
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
            let message = truncate_with_ellipsis("Awaiting auto drive activity", available);
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

    fn complete_heading_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
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
            let label = truncate_with_ellipsis("Complete", available);
            let mut heading = CardSegment::new(label, primary_text_style(style));
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

    fn complete_message_rows(&self, message: &str, body_width: usize, style: &CardStyle) -> Vec<CardRow> {
        if body_width == 0 {
            return Vec::new();
        }

        let indent_text = if ACTION_TIME_INDENT > 0 {
            " ".repeat(ACTION_TIME_INDENT)
        } else {
            String::new()
        };
        let indent_style = secondary_text_style(style);
        let content_style = primary_text_style(style);
        let available = body_width.saturating_sub(ACTION_TIME_INDENT);
        if available == 0 {
            return Vec::new();
        }

        let mut rows = Vec::new();
        for (idx, paragraph) in message.lines().enumerate() {
            let trimmed = paragraph.trim();
            if trimmed.is_empty() {
                if idx > 0 {
                    rows.push(self.blank_row(body_width, style));
                }
                continue;
            }
            for segment in Self::wrap_segments(trimmed, available) {
                let mut segments = Vec::new();
                if ACTION_TIME_INDENT > 0 {
                    segments.push(CardSegment::new(indent_text.clone(), indent_style));
                }
                let mut body = CardSegment::new(segment, content_style);
                body.inherit_background = true;
                segments.push(body);
                rows.push(CardRow::new(
                    BORDER_BODY.to_string(),
                    Self::accent_style(style),
                    segments,
                    None,
                ));
            }
        }

        rows
    }

    fn celebration_body_lines(&self, body_width: usize) -> Vec<String> {
        self.celebration_body_lines_at(body_width, Instant::now())
    }

    fn celebration_body_lines_at(&self, body_width: usize, now: Instant) -> Vec<String> {
        let reduced_motion = Self::celebration_reduced_motion();
        self.celebration_body_lines_internal(body_width, now, reduced_motion)
    }

    #[cfg(test)]
    fn celebration_body_lines_at_with_reduced_motion(
        &self,
        body_width: usize,
        now: Instant,
        reduced_motion: bool,
    ) -> Vec<String> {
        self.celebration_body_lines_internal(body_width, now, reduced_motion)
    }

    fn celebration_frame_index_at(started_at: Instant, now: Instant, reduced_motion: bool) -> usize {
        if reduced_motion {
            return 0;
        }
        let interval_ms = CELEBRATION_FRAME_INTERVAL.as_millis().max(1);
        let elapsed = now.saturating_duration_since(started_at);
        (elapsed.as_millis() / interval_ms) as usize
    }

    fn celebration_body_lines_internal(
        &self,
        body_width: usize,
        now: Instant,
        reduced_motion: bool,
    ) -> Vec<String> {
        if body_width == 0 {
            return Vec::new();
        }

        let frame = self
            .celebration_started_at
            .map(|started| Self::celebration_frame_index_at(started, now, reduced_motion))
            .unwrap_or(0);

        let mut lines: Vec<String> = Vec::new();
        lines.push(Self::pad_to_width("", body_width));

        let ascii_block_width = CELEBRATION_ASCII
            .iter()
            .map(|line| Self::display_width(line))
            .max()
            .unwrap_or(0);

        let (ascii_left_pad, ascii_right_pad) = if ascii_block_width <= body_width {
            let left = (body_width - ascii_block_width) / 2;
            let right = body_width - left - ascii_block_width;
            (left, right)
        } else {
            (0, 0)
        };

        for (ascii_index, ascii) in CELEBRATION_ASCII.iter().enumerate() {
            let mut line = if ascii_block_width <= body_width {
                Self::pad_line_for_block(ascii, ascii_block_width, ascii_left_pad, ascii_right_pad)
            } else {
                Self::pad_to_width(ascii, body_width)
            };
            if !reduced_motion {
                let protected = Self::occupied_range(&line);
                line = Self::sprinkle_sparkles(line, frame as u64, ascii_index as u64, protected);
            }
            lines.push(line);
        }

        lines.push(Self::pad_to_width("", body_width));

        lines
    }

    fn celebration_reduced_motion() -> bool {
        match env::var("CODE_TUI_REDUCED_MOTION") {
            Ok(value) => {
                let normalized = value.trim().to_ascii_lowercase();
                !matches!(normalized.as_str(), "" | "0" | "false" | "off" | "no")
            }
            Err(_) => false,
        }
    }

    fn sprinkle_sparkles(
        line: String,
        frame_seed: u64,
        row_seed: u64,
        protected_range: Option<(usize, usize)>,
    ) -> String {
        let mut chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            return line;
        }

        let protected = protected_range.unwrap_or((0, 0));
        let (protected_start, protected_end) = protected;

        let mut state = frame_seed
            .wrapping_mul(0x9E3779B97F4A7C15)
            .wrapping_add(row_seed.wrapping_mul(0xBF58476D1CE4E5B9))
            ^ 0x94D049BB133111EB;

        let sparkle_count = 3 + (Self::prng_step(&mut state) % 4) as usize;
        for _ in 0..sparkle_count {
            let position = (Self::prng_step(&mut state) as usize) % chars.len();
            let in_protected = position >= protected_start && position < protected_end;
            if !in_protected && chars[position] == ' ' {
                let sparkle_idx = (Self::prng_step(&mut state) as usize)
                    % CELEBRATION_SPARKLE_CHOICES.len();
                chars[position] = CELEBRATION_SPARKLE_CHOICES[sparkle_idx];
            }
        }

        chars.into_iter().collect()
    }

    fn occupied_range(line: &str) -> Option<(usize, usize)> {
        let mut start = None;
        let mut end = 0usize;
        for (idx, ch) in line.chars().enumerate() {
            if ch != ' ' {
                start = start.or(Some(idx));
                end = idx + 1;
            }
        }
        start.map(|s| (s, end))
    }

    fn prng_step(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        *state
    }

    fn action_rows(&self, body_width: usize, style: &CardStyle) -> Vec<CardRow> {
        if body_width == 0 {
            return Vec::new();
        }
        if self.actions.is_empty() {
            return Vec::new();
        }

        let elapsed_labels: Vec<String> = self
            .actions
            .iter()
            .map(|action| format!(" {}", format_duration_digital(action.elapsed)))
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

        let mut rows = Vec::new();

        for (action, elapsed) in self.actions.iter().zip(elapsed_labels.iter()) {
            let mut segments = Vec::new();
            if ACTION_TIME_INDENT > 0 {
                segments.push(CardSegment::new(indent_text.clone(), indent_style));
            }

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

            if remaining > 0 {
                let description = match action.kind {
                    AutoDriveActionKind::Info => action.text.trim().to_string(),
                    AutoDriveActionKind::Warning => format!("! {}", action.text.trim()),
                    AutoDriveActionKind::Error => format!("✗ {}", action.text.trim()),
                };
                let display = truncate_with_ellipsis(description.as_str(), remaining);
                let mut description_segment =
                    CardSegment::new(display, secondary_text_style(style));
                description_segment.inherit_background = true;
                segments.push(description_segment);
            }

            rows.push(CardRow::new(
                BORDER_BODY.to_string(),
                Self::accent_style(style),
                segments,
                None,
            ));
        }

        rows
    }

    fn wrap_segments(text: &str, width: usize) -> Vec<String> {
        if width == 0 {
            return vec![String::new()];
        }

        let mut rows: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut current_width = 0usize;

        for word in text.split_whitespace() {
            let word_width = UnicodeWidthStr::width(word);
            if current.is_empty() {
                if word_width > width {
                    rows.extend(Self::chunk_long_word(word, width));
                    current.clear();
                    current_width = 0;
                } else {
                    current.push_str(word);
                    current_width = word_width;
                }
                continue;
            }

            let needs_space = 1;
            if current_width + needs_space + word_width <= width {
                current.push(' ');
                current.push_str(word);
                current_width += needs_space + word_width;
            } else {
                rows.push(Self::pad_to_width(&current, width));
                current.clear();
                current_width = 0;
                if word_width > width {
                    rows.extend(Self::chunk_long_word(word, width));
                } else {
                    current.push_str(word);
                    current_width = word_width;
                }
            }
        }

        if !current.is_empty() {
            rows.push(Self::pad_to_width(&current, width));
        }

        if rows.is_empty() {
            rows.push(Self::pad_to_width("", width));
        }

        rows
    }

    fn chunk_long_word(word: &str, width: usize) -> Vec<String> {
        if width == 0 {
            return vec![String::new()];
        }

        let mut rows: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut current_width = 0usize;
        for ch in word.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if current_width + ch_width > width && !current.is_empty() {
                rows.push(Self::pad_to_width(&current, width));
                current.clear();
                current_width = 0;
            }
            current.push(ch);
            current_width += ch_width;
        }

        if !current.is_empty() {
            rows.push(Self::pad_to_width(&current, width));
        }

        rows
    }

    fn pad_to_width(text: &str, width: usize) -> String {
        if width == 0 {
            return String::new();
        }

        let mut output = String::new();
        let mut accumulated = 0usize;
        for ch in text.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if accumulated + ch_width > width {
                break;
            }
            output.push(ch);
            accumulated += ch_width;
        }

        if accumulated < width {
            output.push_str(&" ".repeat(width - accumulated));
        }

        output
    }

    fn pad_line_for_block(
        line: &str,
        block_width: usize,
        left_pad: usize,
        right_pad: usize,
    ) -> String {
        let mut output = String::with_capacity(left_pad + block_width + right_pad);
        output.push_str(&" ".repeat(left_pad));
        output.push_str(line);
        let line_width = Self::display_width(line);
        if line_width < block_width {
            output.push_str(&" ".repeat(block_width - line_width));
        }
        output.push_str(&" ".repeat(right_pad));
        output
    }

    fn display_width(text: &str) -> usize {
        text.chars()
            .map(|ch| UnicodeWidthChar::width(ch).unwrap_or(0))
            .sum()
    }

    fn celebration_ascii_segments(&self, line: String) -> Vec<CardSegment> {
        let chars: Vec<char> = line.chars().collect();
        let total = chars.len();
        let left_pad = chars.iter().take_while(|c| **c == ' ').count();
        let right_pad = chars.iter().rev().take_while(|c| **c == ' ').count();
        let ascii_span_end = total.saturating_sub(right_pad);
        let ascii_slice = &chars[left_pad..ascii_span_end];
        let ascii_len = ascii_slice.len().max(1);

        let mut segments = Vec::new();
        let mut buffer = String::new();
        let mut active_style: Option<(Color, bool)> = None;

        let flush = |segments: &mut Vec<CardSegment>, buffer: &mut String, style: Option<(Color, bool)>| {
            if let Some((color, bold)) = style {
                if !buffer.is_empty() {
                    let mut segment_style = Style::default().fg(color);
                    if bold {
                        segment_style = segment_style.add_modifier(Modifier::BOLD);
                    }
                    segments.push(CardSegment::new(std::mem::take(buffer), segment_style));
                }
            }
        };

        for idx in 0..total {
            let ch = chars[idx];
            let style_for_char = if idx < left_pad || idx >= ascii_span_end {
                (Color::Rgb(255, 255, 255), false)
            } else {
                let ascii_pos = idx - left_pad;
                let ascii_ch = ascii_slice[ascii_pos];
                if ascii_ch == ' ' {
                    (Color::Rgb(255, 255, 255), false)
                } else {
                    let denom = (ascii_len - 1).max(1) as f32;
                    let ratio = (ascii_pos as f32) / denom;
                    let base = gradient_multi(ratio);
                    let neon = mix_rgb(base, Color::Rgb(255, 255, 255), 0.18);
                    (neon, true)
                }
            };

            if active_style.map(|s| s == style_for_char).unwrap_or(false) {
                buffer.push(ch);
            } else {
                flush(&mut segments, &mut buffer, active_style);
                buffer.push(ch);
                active_style = Some(style_for_char);
            }
        }

        flush(&mut segments, &mut buffer, active_style);

        segments
    }

    fn celebration_background_style() -> Style {
        if palette_mode() == PaletteMode::Ansi16 {
            let fg = if is_dark_theme_active() {
                Color::White
            } else {
                Color::Black
            };
            return Style::default().fg(fg);
        }

        Style::default().fg(Color::Rgb(255, 255, 255))
    }

    fn bottom_border_row(&self, body_width: usize, style: &CardStyle) -> CardRow {
        let text = truncate_with_ellipsis(HINT_TEXT, body_width);
        let hint_style = if palette_mode() == PaletteMode::Ansi16 {
            primary_text_style(style)
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

    fn render_rows(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        if area.width <= 2 || area.height == 0 {
            return;
        }
        let style = auto_drive_card_style();

        let draw_width = area.width - 2;
        let render_area = Rect {
            width: draw_width,
            ..area
        };

        if let Some(bg) = self.background_override {
            fill_rect(
                buf,
                render_area,
                Some(' '),
                Style::default().fg(style.text_primary).bg(bg),
            );
        } else if palette_mode() == PaletteMode::Ansi16 {
            fill_rect(
                buf,
                render_area,
                Some(' '),
                Style::default()
                    .fg(style.text_primary)
                    .bg(style.gradient.left),
            );
        } else {
            let reveal = self.reveal_progress().and_then(|(progress, theme)| {
                theme.theme.reveal.map(|config| RevealRender {
                    progress,
                    variant: config.variant,
                    intro_light: !is_dark_theme_active(),
                })
            });
            GradientBackground::render(
                buf,
                render_area,
                &style.gradient,
                style.text_primary,
                reveal,
            );
        }

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

    fn desired_rows(&self, width: u16) -> usize {
        let style = auto_drive_card_style();
        let trimmed_width = width.saturating_sub(2);
        if trimmed_width == 0 {
            return 0;
        }
        self.build_card_rows(trimmed_width, &style).len().max(1)
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

fn is_dark_theme_active() -> bool {
    let (r, g, b) = colors::color_to_rgb(colors::background());
    let luminance = (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0;
    luminance < 0.5
}

fn active_auto_drive_theme() -> card_theme::CardThemeDefinition {
    if is_dark_theme_active() {
        card_theme::auto_drive_dark_theme()
    } else {
        card_theme::auto_drive_light_theme()
    }
}

impl crate::chatwidget::tool_cards::ToolCardCell for AutoDriveCardCell {
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

impl HistoryCell for AutoDriveCardCell {
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
        HistoryCellType::Tool {
            status: self.status.tool_status(),
        }
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from(format!("{} — {}", "Auto Drive", self.status.label())));
        if let Some(goal) = &self.goal {
            lines.push(Line::from(format!("goal: {goal}")));
        }
        for action in &self.actions {
            lines.push(Line::from(format!("- {}", action.text)));
        }
        if let Some(completion) = &self.completion_message {
            lines.push(Line::from("complete:"));
            for line in completion.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                lines.push(Line::from(format!("  {line}")));
            }
        }
        lines
    }

    fn is_animating(&self) -> bool {
        self.celebration_started_at.is_some()
            || self
                .reveal_progress()
                .map(|(progress, _)| progress < 0.999)
                .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use unicode_width::UnicodeWidthStr;

    const TEST_WIDTH: usize = 60;

    #[test]
    fn set_goal_trims_and_rejects_empty_text() {
        let mut cell = AutoDriveCardCell::new(None);
        cell.set_goal(Some("  Build the release pipeline  ".to_string()));
        assert_eq!(cell.goal.as_deref(), Some("Build the release pipeline"));

        cell.set_goal(Some("   \n \t  ".to_string()));
        assert!(cell.goal.is_none());
    }

    #[test]
    fn goal_rows_wrap_long_text() {
        let goal = "Ship the multiline goal by implementing wrapping so the Auto Drive card can show every detail clearly";
        let cell = AutoDriveCardCell::new(Some(goal.to_string()));
        let style = auto_drive_card_style();
        let rows = cell.goal_rows(goal, 16, &style);

        assert!(rows.len() >= 3, "expected multiple wrapped rows, got {}", rows.len());
        for row in &rows {
            let combined: String = row
                .segments
                .iter()
                .map(|seg| seg.text.clone())
                .collect();
            assert!(UnicodeWidthStr::width(combined.as_str()) <= 16);
        }
    }

    #[test]
    fn celebration_lines_have_fixed_width() {
        let mut cell = AutoDriveCardCell::new(Some("Goal".to_string()));
        let start = Instant::now();
        cell.celebration_started_at = Some(start);

        let lines = cell.celebration_body_lines_at_with_reduced_motion(
            TEST_WIDTH,
            start + Duration::from_millis(120),
            false,
        );
        assert!(!lines.is_empty());
        assert!(lines
            .iter()
            .all(|line| UnicodeWidthStr::width(line.as_str()) == TEST_WIDTH));
    }

    #[test]
    fn celebration_frames_differ_over_time() {
        let mut cell = AutoDriveCardCell::new(Some("Goal".to_string()));
        let start = Instant::now();
        cell.celebration_started_at = Some(start);

        let first = cell.celebration_body_lines_at_with_reduced_motion(
            TEST_WIDTH,
            start + Duration::from_millis(120),
            false,
        );
        let second = cell.celebration_body_lines_at_with_reduced_motion(
            TEST_WIDTH,
            start + Duration::from_millis(360),
            false,
        );

        assert!(first
            .iter()
            .zip(second.iter())
            .any(|(a, b)| a != b), "expected frames to differ");

        for line in first.iter().chain(second.iter()) {
            assert_eq!(UnicodeWidthStr::width(line.as_str()), TEST_WIDTH);
        }
    }
}
