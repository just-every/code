use super::semantic::{SemanticLine, lines_from_ratatui, lines_to_ratatui};
use super::*;
use crate::theme::current_theme;
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolCallStatus {
    Running,
    Success,
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ToolCallCellState {
    pub lines: Vec<SemanticLine>,
    pub status: ToolCallStatus,
}

impl ToolCallCellState {
    pub(crate) fn new(lines: Vec<Line<'static>>, status: ToolCallStatus) -> Self {
        Self {
            lines: lines_from_ratatui(lines),
            status,
        }
    }
}

pub(crate) struct ToolCallCell {
    state: ToolCallCellState,
}

impl ToolCallCell {
    pub(crate) fn new(lines: Vec<Line<'static>>, status: ToolCallStatus) -> Self {
        Self {
            state: ToolCallCellState::new(lines, status),
        }
    }
    pub(crate) fn retint(&mut self, _old: &crate::theme::Theme, _new: &crate::theme::Theme) {}
}

impl HistoryCell for ToolCallCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Tool {
            status: match self.state.status {
                ToolCallStatus::Running => ToolStatus::Running,
                ToolCallStatus::Success => ToolStatus::Success,
                ToolCallStatus::Failed => ToolStatus::Failed,
            },
        }
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let theme = current_theme();
        lines_to_ratatui(&self.state.lines, &theme)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RunningToolCallState {
    pub title: String,
    pub started_at: SystemTime,
    pub arg_lines: Vec<SemanticLine>,
    pub wait_has_target: bool,
    pub wait_has_call_id: bool,
    pub wait_cap_ms: Option<u64>,
}

impl RunningToolCallState {
    pub(crate) fn new(
        title: String,
        started_at: SystemTime,
        arg_lines: Vec<SemanticLine>,
        wait_has_target: bool,
        wait_has_call_id: bool,
        wait_cap_ms: Option<u64>,
    ) -> Self {
        Self {
            title,
            started_at,
            arg_lines,
            wait_has_target,
            wait_has_call_id,
            wait_cap_ms,
        }
    }
}

pub(crate) struct RunningToolCallCell {
    state: RunningToolCallState,
    start_clock: Instant,
}

impl RunningToolCallCell {
    pub(crate) fn new(state: RunningToolCallState) -> Self {
        Self {
            state,
            start_clock: Instant::now(),
        }
    }

    fn strip_zero_seconds_suffix(mut duration: String) -> String {
        if duration.ends_with(" 00s") {
            duration.truncate(duration.len().saturating_sub(4));
        }
        duration
    }

    fn spinner_frame(&self) -> &'static str {
        const FRAMES: [&str; 4] = ["◐", "◓", "◑", "◒"];
        let idx = ((self.start_clock.elapsed().as_millis() / 100) as usize) % FRAMES.len();
        FRAMES[idx]
    }

    pub(crate) fn has_title(&self, title: &str) -> bool {
        self.state.title == title
    }

    pub(crate) fn finalize_web_search(&self, success: bool, query: Option<String>) -> ToolCallCell {
        let duration = self.elapsed_duration();
        let title = if success {
            "Web Search"
        } else {
            "Web Search (failed)"
        };
        let duration = format_duration(duration);

        let title_line = if success {
            Line::from(vec![
                Span::styled(
                    title,
                    Style::default()
                        .fg(crate::colors::success())
                        .add_modifier(Modifier::BOLD),
                ),
                format!(", duration: {duration}").dim(),
            ])
        } else {
            Line::from(vec![
                Span::styled(
                    title,
                    Style::default()
                        .fg(crate::colors::error())
                        .add_modifier(Modifier::BOLD),
                ),
                format!(", duration: {duration}").dim(),
            ])
        };

        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(title_line);
        if let Some(q) = query {
            lines.push(Line::from(vec![
                Span::styled("└ query: ", Style::default().fg(crate::colors::text_dim())),
                Span::styled(q, Style::default().fg(crate::colors::text())),
            ]));
        }
        lines.push(Line::from(""));

        ToolCallCell::new(
            lines,
            if success {
                ToolCallStatus::Success
            } else {
                ToolCallStatus::Failed
            },
        )
    }

    fn elapsed_duration(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.state.started_at)
            .unwrap_or_else(|_| self.start_clock.elapsed())
    }
}

impl HistoryCell for RunningToolCallCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Tool {
            status: ToolStatus::Running,
        }
    }

    fn gutter_symbol(&self) -> Option<&'static str> {
        if self.state.title == "Waiting" {
            if self.state.wait_has_call_id {
                None
            } else {
                Some(self.spinner_frame())
            }
        } else {
            Some("⚙")
        }
    }

    fn is_animating(&self) -> bool {
        true
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let elapsed = self.elapsed_duration();
        let mut lines: Vec<Line<'static>> = Vec::new();
        if self.state.title == "Waiting" {
            let show_elapsed = !self.state.wait_has_target;
            let mut spans = Vec::new();
            spans.push(Span::styled(
                "Waiting...",
                Style::default()
                    .fg(crate::colors::text())
                    .add_modifier(Modifier::BOLD),
            ));
            let cap_ms = self.state.wait_cap_ms.unwrap_or(600_000);
            let cap_str =
                Self::strip_zero_seconds_suffix(format_duration(Duration::from_millis(cap_ms)));
            let suffix = if show_elapsed {
                let elapsed_str = Self::strip_zero_seconds_suffix(format_duration(elapsed));
                format!(" ({} / up to {})", elapsed_str, cap_str)
            } else {
                format!(" (up to {})", cap_str)
            };
            spans.push(Span::styled(
                suffix,
                Style::default().fg(crate::colors::text_dim()),
            ));
            lines.push(Line::from(spans));
        } else {
            lines.push(Line::styled(
                format!("{} ({})", self.state.title, format_duration(elapsed)),
                Style::default()
                    .fg(crate::colors::info())
                    .add_modifier(Modifier::BOLD),
            ));
        }
        let theme = current_theme();
        lines.extend(lines_to_ratatui(&self.state.arg_lines, &theme));
        lines.push(Line::from(""));
        lines
    }
}
