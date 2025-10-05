//! Background event cell used for status messages derived from `BackgroundEventRecord`.

use super::*;
use crate::history::state::BackgroundEventRecord;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

pub(crate) struct BackgroundEventCell {
    state: BackgroundEventRecord,
}

impl BackgroundEventCell {
    pub(crate) fn new(state: BackgroundEventRecord) -> Self {
        Self { state }
    }

    pub(crate) fn state(&self) -> &BackgroundEventRecord {
        &self.state
    }

    pub(crate) fn state_mut(&mut self) -> &mut BackgroundEventRecord {
        &mut self.state
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        let dim_style = Style::default().fg(crate::colors::text_dim());

        if !self.state.title.trim().is_empty() {
            lines.push(Line::from(Span::styled(
                self.state.title.clone(),
                dim_style,
            )));
        }

        if !self.state.description.trim().is_empty() {
            if !lines.is_empty() {
                lines.push(Line::from(String::new()));
            }
            for line in self.state.description.lines() {
                lines.push(Line::from(Span::styled(line.to_string(), dim_style)));
            }
        }

        if lines.is_empty() {
            lines.push(Line::from(String::new()));
        }

        lines
    }
}

impl HistoryCell for BackgroundEventCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::BackgroundEvent
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        self.lines()
    }
}
