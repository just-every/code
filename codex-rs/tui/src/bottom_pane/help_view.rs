use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, WidgetRef};

use super::{BottomPane, BottomPaneView, CancellationEvent};

pub(crate) struct HelpView {
    scroll: u16,
    complete: bool,
}

impl HelpView {
    pub fn new() -> Self {
        Self { scroll: 0, complete: false }
    }

    fn lines() -> Vec<Line<'static>> {
        let hdr = |t: &str| Line::from(vec![
            Span::styled(t.to_string(), Style::default().fg(crate::colors::info())),
        ]);
        let key = |k: &str, desc: &str| -> Line<'static> {
            let key_style = Style::default().fg(crate::colors::function());
            Line::from(vec![
                Span::from("  "),
                Span::styled(k.to_string(), key_style),
                Span::from("  "),
                Span::from(desc.to_string()),
            ])
        };

        let mut v: Vec<Line<'static>> = Vec::new();
        v.push(Line::from(""));
        v.push(hdr("General"));
        v.push(key("Ctrl+H", "command help (this view)"));
        v.push(key("Ctrl+C", "quit"));
        v.push(key("Esc", "stop/clear; double‑Esc: backtrack/edit previous"));
        v.push(Line::from(""));
        v.push(hdr("Views & Panels"));
        v.push(key("Ctrl+R / Ctrl+T", "toggle reasoning visibility"));
        v.push(key("Ctrl+D", "diff viewer overlay"));
        v.push(key("Ctrl+B", "toggle Browser panel"));
        v.push(key("Ctrl+A", "toggle Agents panel"));
        v.push(Line::from(""));
        v.push(hdr("Navigation"));
        v.push(key("PageUp/PageDown", "scroll transcript a page"));
        v.push(key("Home/End", "jump to top/bottom"));
        v.push(key("↑/↓", "scroll (when history has focus)"));
        v.push(Line::from(""));
        v.push(hdr("Overlays"));
        v.push(key("q", "close overlay"));
        v.push(key("Ctrl+C", "close overlay"));
        v
    }
}

impl<'a> BottomPaneView<'a> for HelpView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) { return; }
        match key_event.code {
            KeyCode::Esc | KeyCode::Char('q') => { self.complete = true; },
            KeyCode::Up => { self.scroll = self.scroll.saturating_sub(1); },
            KeyCode::Down => { self.scroll = self.scroll.saturating_add(1); },
            KeyCode::PageUp => { self.scroll = self.scroll.saturating_sub(8); },
            KeyCode::PageDown | KeyCode::Char(' ') => { self.scroll = self.scroll.saturating_add(8); },
            KeyCode::Home => { self.scroll = 0; },
            KeyCode::End => { self.scroll = u16::MAX; },
            _ => {}
        }
    }

    fn is_complete(&self) -> bool { self.complete }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'a>) -> CancellationEvent {
        self.complete = true;
        CancellationEvent::Handled
    }

    fn desired_height(&self, _width: u16) -> u16 { 18 }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Base clear and outer framing
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Help — Keys and Shortcuts (Esc to close)")
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::selection()));
        block.clone().render(area, buf);
        let inner = block.inner(area);

        // Fill inner with standard background for readability
        let inner_bg = Block::default().style(Style::default().bg(crate::colors::background()));
        inner_bg.render(inner, buf);

        // Layout: top header row with brief hint, rest is content
        let [header_area, content_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
        ]).areas(inner);

        // Header hint
        let hint = Line::from(vec![
            Span::styled("↑/↓", Style::default().fg(crate::colors::function())),
            Span::from(" scroll   "),
            Span::styled("PgUp/PgDn", Style::default().fg(crate::colors::function())),
            Span::from(" page   "),
            Span::styled("q", Style::default().fg(crate::colors::function())),
            Span::from(" close"),
        ]).dim();
        hint.render_ref(header_area, buf);

        // Content
        let lines = Self::lines();
        let para = Paragraph::new(Text::from(lines)).scroll((self.scroll, 0));
        Widget::render(para, content_area, buf);
    }
}
