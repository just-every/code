use super::*;
use crate::history::compat::{
    ContextBrowserSnapshotRecord,
    ContextDeltaField,
    ContextDeltaRecord,
};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

const MAX_DELTA_PREVIEW: usize = 10;

pub(crate) struct ContextCell {
    record: ContextRecord,
    lines: Vec<Line<'static>>,
}

impl ContextCell {
    pub(crate) fn new(record: ContextRecord) -> Self {
        let clamped = clamp_record(record);
        let lines = build_lines(&clamped);
        Self { record: clamped, lines }
    }

    pub(crate) fn update(&mut self, record: ContextRecord) {
        let clamped = clamp_record(record);
        self.lines = build_lines(&clamped);
        self.record = clamped;
    }

    pub(crate) fn record(&self) -> &ContextRecord {
        &self.record
    }
}

fn clamp_record(mut record: ContextRecord) -> ContextRecord {
    if record.deltas.len() > MAX_DELTA_PREVIEW {
        record.deltas = record
            .deltas
            .into_iter()
            .rev()
            .take(MAX_DELTA_PREVIEW)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    }
    record
}

fn build_lines(record: &ContextRecord) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let dim = Style::default().fg(crate::colors::text_dim());
    let primary = Style::default().fg(crate::colors::text());
    let accent = Style::default()
        .fg(crate::colors::primary())
        .add_modifier(Modifier::BOLD);
    let badge_style = Style::default()
        .fg(crate::colors::text())
        .bg(crate::colors::selection())
        .add_modifier(Modifier::BOLD);

    let mut header_spans = Vec::new();
    if let Some(cwd) = &record.cwd {
        header_spans.push(Span::styled("📁 ", accent));
        header_spans.push(Span::styled(cwd.clone(), primary));
    } else {
        header_spans.push(Span::styled("Context", accent));
    }

    if !record.deltas.is_empty() {
        let label = if record.deltas.len() == 1 {
            " 1 change ".to_string()
        } else {
            format!(" {} changes ", record.deltas.len())
        };
        header_spans.push(Span::raw(" "));
        header_spans.push(Span::styled(label, badge_style));
    }

    if !header_spans.is_empty() {
        lines.push(Line::from(header_spans));
    }

    let mut meta_spans: Vec<Span<'static>> = Vec::new();
    if let Some(branch) = &record.git_branch {
        meta_spans.push(Span::styled(" ", accent));
        meta_spans.push(Span::styled(branch.clone(), primary));
    }

    if let Some(reasoning) = &record.reasoning_effort {
        if !meta_spans.is_empty() {
            meta_spans.push(Span::raw("  "));
        }
        meta_spans.push(Span::styled("🧠 ", accent));
        meta_spans.push(Span::styled(reasoning.clone(), primary));
    }

    if record.browser_session_active && record.browser_snapshot.is_none() {
        if !meta_spans.is_empty() {
            meta_spans.push(Span::raw("  "));
        }
        meta_spans.push(Span::styled("[browser]", accent));
    }

    if !meta_spans.is_empty() {
        lines.push(Line::from(meta_spans));
    }

    if let Some(snapshot) = &record.browser_snapshot {
        lines.push(build_browser_line(snapshot, primary, dim, accent));
    }

    if record.expanded && !record.deltas.is_empty() {
        lines.push(Line::from(vec![Span::styled("Recent changes:", accent)]));
        for delta in record.deltas.iter().rev() {
            lines.push(build_delta_line(delta, primary, dim));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(String::new()));
    }

    lines
}

fn build_browser_line(
    snapshot: &ContextBrowserSnapshotRecord,
    primary: Style,
    dim: Style,
    accent: Style,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = vec![Span::styled("🖼 ", accent)];
    if let Some(title) = snapshot.title.as_ref().filter(|s| !s.is_empty()) {
        spans.push(Span::styled(title.clone(), primary));
    } else if let Some(url) = snapshot.url.as_ref() {
        spans.push(Span::styled(url.clone(), primary));
    } else {
        spans.push(Span::styled("Browser snapshot", primary));
    }

    if snapshot.width.zip(snapshot.height).is_some() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!(
                "{}×{}",
                snapshot.width.unwrap_or_default(),
                snapshot.height.unwrap_or_default()
            ),
            dim,
        ));
    }

    if let Some(captured) = snapshot.captured_at.as_ref() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(captured.clone(), dim));
    }

    if !snapshot.metadata.is_empty() {
        let meta = snapshot
            .metadata
            .iter()
            .take(2)
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(", ");
        spans.push(Span::raw("  "));
        spans.push(Span::styled(meta, dim));
    }

    Line::from(spans)
}

fn build_delta_line(delta: &ContextDeltaRecord, primary: Style, dim: Style) -> Line<'static> {
    let field = match delta.field {
        ContextDeltaField::Cwd => "cwd",
        ContextDeltaField::GitBranch => "branch",
        ContextDeltaField::ReasoningEffort => "reasoning",
        ContextDeltaField::BrowserSnapshot => "browser",
    };

    let mut spans = Vec::new();
    spans.push(Span::styled("• ", dim));
    spans.push(Span::styled(field.to_string(), primary));
    spans.push(Span::styled(": ", dim));

    let previous = delta.previous.clone().unwrap_or_else(|| "—".to_string());
    let current = delta.current.clone().unwrap_or_else(|| "—".to_string());

    spans.push(Span::styled(previous, dim));
    spans.push(Span::styled(" → ", dim));
    spans.push(Span::styled(current, primary));

    if let Some(seq) = delta.sequence {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(format!("#{}", seq), dim));
    }

    Line::from(spans)
}

impl HistoryCell for ContextCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Context
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        self.lines.clone()
    }

    fn display_lines_trimmed(&self) -> Vec<Line<'static>> {
        self.lines.clone()
    }
}
