use super::semantic::{SemanticLine, lines_from_ratatui, lines_to_ratatui};
use super::*;
use crate::theme::current_theme;

const TARGET_WIDTH: u16 = 70;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UpgradeNoticeState {
    pub lines: Vec<SemanticLine>,
    pub backdrop: ratatui::style::Color,
    pub border_style: Style,
}

impl UpgradeNoticeState {
    pub(crate) fn new(
        lines: Vec<Line<'static>>,
        backdrop: ratatui::style::Color,
        border_style: Style,
    ) -> Self {
        Self {
            lines: lines_from_ratatui(lines),
            backdrop,
            border_style,
        }
    }
}

pub(crate) struct UpgradeNoticeCell {
    state: UpgradeNoticeState,
}

impl UpgradeNoticeCell {
    pub(crate) fn new(current_version: String, latest_version: String) -> Self {
        let current_version = current_version.trim().to_string();
        let latest_version = latest_version.trim().to_string();
        let primary = crate::colors::primary();
        let backdrop = crate::colors::mix_toward(primary, crate::colors::background(), 0.95);
        let base_style = Style::default().bg(backdrop).fg(crate::colors::text());
        let title_style = Style::default()
            .bg(backdrop)
            .fg(primary)
            .add_modifier(Modifier::BOLD);
        let highlight_style = Style::default().bg(backdrop).fg(primary);
        let dim_style = Style::default().bg(backdrop).fg(crate::colors::text_dim());

        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from(vec![Span::styled(
            "★ Upgrade Available ★",
            title_style,
        )]));
        lines.push(Line::from(vec![
            Span::styled("Latest release: ", dim_style),
            Span::styled(
                format!("{current_version} → {latest_version}"),
                highlight_style,
            ),
        ]));
        lines.push(Line::from(vec![Span::styled(String::new(), base_style)]));
        lines.push(Line::from(vec![
            Span::styled("Use ", base_style),
            Span::styled("/upgrade", highlight_style),
            Span::styled(" to upgrade now or enable auto-update.", base_style),
        ]));

        Self {
            state: UpgradeNoticeState::new(
                lines,
                backdrop,
                Style::default().bg(backdrop).fg(primary),
            ),
        }
    }

    fn text(&self) -> Text<'static> {
        let theme = current_theme();
        Text::from(lines_to_ratatui(&self.state.lines, &theme))
    }

    fn inner_width(total_width: u16) -> u16 {
        total_width.saturating_sub(2).max(1)
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
        let theme = current_theme();
        lines_to_ratatui(&self.state.lines, &theme)
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
            .border_style(self.state.border_style)
            .style(Style::default().bg(self.state.backdrop));

        Paragraph::new(self.text())
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Center)
            .scroll((skip_rows, 0))
            .block(block)
            .style(
                Style::default()
                    .bg(self.state.backdrop)
                    .fg(crate::colors::text()),
            )
            .render(render_area, buf);
    }
}
