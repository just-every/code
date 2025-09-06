use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use super::{BottomPane, BottomPaneView, CancellationEvent};

pub(crate) struct HelpOverlayView {
    complete: bool,
}

impl HelpOverlayView {
    pub fn new() -> Self { Self { complete: false } }

    fn lines() -> Vec<Line<'static>> {
        // Key summary. Keep aligned and concise.
        let key = |k: &str| Span::styled(k.to_string(), Style::default().add_modifier(Modifier::BOLD));
        let dim = |s: &str| Span::styled(s.to_string(), Style::default().add_modifier(Modifier::DIM));

        let mut v: Vec<Line<'static>> = Vec::new();

        // Global controls
        v.push(Line::from(vec![dim("Global"), Span::raw(":")]));
        v.push(Line::from(vec![key("Ctrl+B"), Span::raw("  toggle Browser panel")]));
        v.push(Line::from(vec![key("Ctrl+A"), Span::raw("  toggle Agents panel")]));
        v.push(Line::from(vec![key("Ctrl+D"), Span::raw("  toggle Diffs overlay")]));
        v.push(Line::from(vec![key("Ctrl+R / Ctrl+T"), Span::raw("  toggle reasoning visibility")]));
        v.push(Line::from(vec![key("Ctrl+M"), Span::raw("  toggle mouse capture (select text)")]));
        v.push(Line::from(vec![key("Esc"), Span::raw("  stop task / clear input / backtrack (double Esc)")]));
        v.push(Line::from(vec![key("Ctrl+C"), Span::raw("  cancel modal / quit if repeated") ]));
        v.push(Line::from(""));

        // Transcript navigation
        v.push(Line::from(vec![dim("Transcript"), Span::raw(":")]));
        v.push(Line::from(vec![key("PgUp / PgDn"), Span::raw("  scroll a page")]));
        v.push(Line::from(vec![key("↑ / ↓"), Span::raw("  scroll or navigate input history")]));
        v.push(Line::from(vec![key("Home / End"), Span::raw("  jump to top/bottom (overlays)")]));
        v.push(Line::from(""));

        // Composer
        v.push(Line::from(vec![dim("Composer"), Span::raw(":")]));
        v.push(Line::from(vec![key("Enter"), Span::raw("  send message")]));
        v.push(Line::from(vec![key("Shift+Enter"), Span::raw("  new line")]));
        v.push(Line::from(vec![key("Ctrl+V / Shift+Insert"), Span::raw("  paste (images supported)")]));

        v
    }
}

impl<'a> BottomPaneView<'a> for HelpOverlayView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        if key_event.kind == KeyEventKind::Press || key_event.kind == KeyEventKind::Repeat {
            match key_event.code {
                KeyCode::Esc | KeyCode::Char('q') => { self.complete = true; }
                _ => {}
            }
        }
    }

    fn is_complete(&self) -> bool { self.complete }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'a>) -> CancellationEvent {
        self.complete = true;
        CancellationEvent::Handled
    }

    fn desired_height(&self, _width: u16) -> u16 { 16 }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Reserve the full popup area in the bottom pane
        let inner = Rect { x: area.x, y: area.y, width: area.width, height: area.height };
        Clear.render(inner, buf);

        // Outer popup styling to match other popups
        let outer_block = Block::default()
            .borders(Borders::ALL)
            .title("Help – Key Shortcuts (Esc to close)")
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::selection()));
        outer_block.clone().render(inner, buf);
        let content = outer_block.inner(inner);

        // Fill the inner content background
        let content_bg = Block::default().style(Style::default().bg(crate::colors::background()));
        content_bg.render(content, buf);

        // Provide a little top padding inside the content
        let [_, body_area] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(content);
        let paragraph = Paragraph::new(Text::from(Self::lines())).wrap(ratatui::widgets::Wrap { trim: false });
        Widget::render(paragraph, body_area, buf);
    }
}

