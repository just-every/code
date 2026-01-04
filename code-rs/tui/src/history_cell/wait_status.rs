use super::*;
use crate::history::state::{HistoryId, TextTone, WaitStatusDetail, WaitStatusHeader, WaitStatusState};
use crate::theme::{current_theme, Theme};
use code_common::elapsed::format_duration;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::time::Duration;

pub(crate) struct WaitStatusCell {
    state: WaitStatusState,
}

impl WaitStatusCell {
    pub(crate) fn new(mut state: WaitStatusState) -> Self {
        state.id = HistoryId::ZERO;
        Self { state }
    }

    pub(crate) fn retint(&mut self, _old: &crate::theme::Theme, _new: &crate::theme::Theme) {}

    #[allow(dead_code)]
    pub(crate) fn from_state(state: WaitStatusState) -> Self {
        Self { state }
    }

    pub(crate) fn state(&self) -> &WaitStatusState {
        &self.state
    }

    pub(crate) fn state_mut(&mut self) -> &mut WaitStatusState {
        &mut self.state
    }
}

impl HistoryCell for WaitStatusCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Plain
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let theme = current_theme();
        let mut lines: Vec<Line<'static>> = Vec::new();

        lines.push(render_header(&self.state.header, &theme));

        for detail in &self.state.details {
            lines.push(render_detail(detail, &theme));
        }

        lines.push(Line::from(""));
        lines
    }

    fn gutter_symbol(&self) -> Option<&'static str> {
        Some("◓")
    }
}

fn render_header(header: &WaitStatusHeader, theme: &Theme) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled(
        header.title.clone(),
        Style::default()
            .fg(color_for_tone(header.title_tone, theme))
            .add_modifier(Modifier::BOLD),
    ));
    if let Some(summary) = &header.summary {
        spans.push(Span::styled(
            format!(" ({summary})"),
            Style::default().fg(color_for_tone(header.summary_tone, theme)),
        ));
    }
    Line::from(spans)
}

fn render_detail(detail: &WaitStatusDetail, theme: &Theme) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let tone_color = color_for_tone(detail.tone, theme);
    spans.push(Span::styled(
        detail.label.clone(),
        Style::default().fg(tone_color),
    ));
    if let Some(value) = &detail.value {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(value.clone(), Style::default().fg(tone_color)));
    }
    Line::from(spans)
}

fn color_for_tone(tone: TextTone, theme: &Theme) -> ratatui::style::Color {
    match tone {
        TextTone::Default => theme.text,
        TextTone::Dim => theme.text_dim,
        TextTone::Primary => theme.primary,
        TextTone::Success => theme.success,
        TextTone::Warning => theme.warning,
        TextTone::Error => theme.error,
        TextTone::Info => theme.info,
    }
}

pub(crate) fn new_completed_wait_tool_call(target: String, duration: Duration) -> WaitStatusCell {
    let mut duration_str = format_duration(duration);
    if duration_str.ends_with(" 00s") {
        duration_str.truncate(duration_str.len().saturating_sub(4));
    }

    let header = crate::history::WaitStatusHeader {
        title: "Waited".to_string(),
        title_tone: crate::history::TextTone::Success,
        summary: Some(duration_str),
        summary_tone: crate::history::TextTone::Dim,
    };

    let mut details: Vec<crate::history::WaitStatusDetail> = Vec::new();
    if !target.is_empty() {
        details.push(crate::history::WaitStatusDetail {
            label: "for".to_string(),
            value: Some(target),
            tone: crate::history::TextTone::Dim,
        });
    }

    let state = crate::history::WaitStatusState {
        id: crate::history::HistoryId::ZERO,
        header,
        details,
    };

    WaitStatusCell::new(state)
}
