//! Upgrade notice cell built from `UpgradeNoticeState` metadata.

use super::*;
use crate::history::state::{HistoryId, UpgradeNoticeState};

const TARGET_WIDTH: u16 = 70;

pub(crate) struct UpgradeNoticeCell {
    state: UpgradeNoticeState,
    backdrop: ratatui::style::Color,
    border_style: Style,
}

impl UpgradeNoticeCell {
    pub(crate) fn new(state: UpgradeNoticeState) -> Self {
        let mut state = state;
        state.id = HistoryId::ZERO;
        Self::with_state(state)
    }

    #[allow(dead_code)]
    pub(crate) fn from_state(state: UpgradeNoticeState) -> Self {
        Self::with_state(state)
    }

    fn with_state(state: UpgradeNoticeState) -> Self {
        let primary = crate::colors::primary();
        let backdrop = crate::colors::mix_toward(primary, crate::colors::background(), 0.95);
        let border_style = Style::default().bg(backdrop).fg(primary);
        Self {
            state,
            backdrop,
            border_style,
        }
    }

    pub(crate) fn state(&self) -> &UpgradeNoticeState {
        &self.state
    }

    pub(crate) fn state_mut(&mut self) -> &mut UpgradeNoticeState {
        &mut self.state
    }

    fn text(&self) -> Text<'static> {
        Text::from(self.message_lines())
    }

    fn inner_width(total_width: u16) -> u16 {
        total_width.saturating_sub(2).max(1)
    }

    fn styles(&self) -> (Style, Style, Style, Style) {
        let base = Style::default().bg(self.backdrop).fg(crate::colors::text());
        let title = Style::default()
            .bg(self.backdrop)
            .fg(crate::colors::primary())
            .add_modifier(Modifier::BOLD);
        let highlight = Style::default().bg(self.backdrop).fg(crate::colors::primary());
        let dim = Style::default().bg(self.backdrop).fg(crate::colors::text_dim());
        (base, title, highlight, dim)
    }

    fn message_lines(&self) -> Vec<Line<'static>> {
        let (base_style, title_style, highlight_style, dim_style) = self.styles();
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from(vec![Span::styled("★ Upgrade Available ★", title_style)]));
        lines.push(Line::from(vec![
            Span::styled("Latest release: ", dim_style),
            Span::styled(
                format!("{} → {}", self.state.current_version, self.state.latest_version),
                highlight_style,
            ),
        ]));
        lines.push(Line::from(vec![Span::styled(String::new(), base_style)]));
        lines.push(Line::from(format_upgrade_message(
            &self.state.message,
            base_style,
            highlight_style,
        )));
        lines
    }
}

impl HistoryCell for UpgradeNoticeCell {
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
        self.message_lines()
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn desired_height(&self, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }
        let box_width = width.min(TARGET_WIDTH).max(3);
        let inner_width = Self::inner_width(box_width);
        let paragraph = Paragraph::new(self.text()).wrap(Wrap { trim: false });
        let text_height = paragraph.line_count(inner_width) as u16;
        text_height.saturating_add(2)
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let box_width = area.width.min(TARGET_WIDTH).max(3);
        let render_area = Rect::new(area.x, area.y, box_width, area.height);

        let bg_style = Style::default()
            .bg(crate::colors::background())
            .fg(crate::colors::text());
        fill_rect(buf, area, Some(' '), bg_style);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.border_style)
            .style(Style::default().bg(self.backdrop));

        Paragraph::new(self.text())
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Center)
            .scroll((skip_rows, 0))
            .block(block)
            .style(Style::default().bg(self.backdrop).fg(crate::colors::text()))
            .render(render_area, buf);
    }
}

fn format_upgrade_message(
    message: &str,
    base_style: Style,
    highlight_style: Style,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut remaining = message;
    const TOKEN: &str = "/upgrade";
    while let Some(pos) = remaining.find(TOKEN) {
        if pos > 0 {
            spans.push(Span::styled(remaining[..pos].to_string(), base_style));
        }
        spans.push(Span::styled(TOKEN.to_string(), highlight_style));
        remaining = &remaining[pos + TOKEN.len()..];
    }
    if !remaining.is_empty() {
        spans.push(Span::styled(remaining.to_string(), base_style));
    }
    spans
}

/// Create a notice cell that shows the "Popular commands" immediately.
/// If `connecting_mcp` is true, include a dim status line to inform users
/// that external MCP servers are being connected in the background.
pub(crate) fn new_upgrade_prelude(latest_version: Option<&str>) -> Option<UpgradeNoticeCell> {
    if !crate::updates::upgrade_ui_enabled() {
        return None;
    }
    let latest = latest_version?.trim();
    if latest.is_empty() {
        return None;
    }

    let current = code_version::version();
    if latest == current {
        return None;
    }

    let state = UpgradeNoticeState {
        id: HistoryId::ZERO,
        current_version: current.trim().to_string(),
        latest_version: latest.trim().to_string(),
        message: "Use /upgrade to upgrade now or enable auto-update.".to_string(),
    };

    Some(UpgradeNoticeCell::new(state))
}
