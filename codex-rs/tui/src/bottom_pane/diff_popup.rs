use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs, Widget};

use super::{BottomPane, BottomPaneView, CancellationEvent};

#[allow(dead_code)]
pub(crate) struct DiffPopupView {
    tabs: Vec<(String, Vec<Line<'static>>)>,
    selected: usize,
    complete: bool,
}

#[allow(dead_code)]
impl DiffPopupView {
    pub fn new(tabs: Vec<(String, Vec<Line<'static>>)>) -> Self {
        Self {
            tabs,
            selected: 0,
            complete: false,
        }
    }
}

impl<'a> BottomPaneView<'a> for DiffPopupView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        if key_event.kind == KeyEventKind::Press || key_event.kind == KeyEventKind::Repeat {
            match key_event.code {
                KeyCode::Left => {
                    if self.selected > 0 {
                        self.selected -= 1;
                    }
                }
                KeyCode::Right => {
                    if self.selected + 1 < self.tabs.len() {
                        self.selected += 1;
                    }
                }
                KeyCode::Esc => {
                    self.complete = true;
                }
                _ => {}
            }
        }
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'a>) -> CancellationEvent {
        self.complete = true;
        CancellationEvent::Handled
    }

    fn desired_height(&self, _width: u16) -> u16 {
        // Reasonable height for a diff popup within bottom pane
        20
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Reserve the full popup area
        let inner = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height,
        };

        // Base clear
        Clear.render(inner, buf);

        // Outer popup: use selection-colored background with border styling
        let outer_block = Block::default()
            .borders(Borders::ALL)
            .title("Diffs – Esc close, ◂ ▸ change tabs")
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::selection()));
        outer_block.clone().render(inner, buf);
        let content = outer_block.inner(inner);

        // Fill inner content with selection color by default
        let content_bg = Block::default().style(Style::default().bg(crate::colors::selection()));
        content_bg.clone().render(content, buf);

        // Split inner content into a 3-row tabs header and the body
        let [tabs_area_all, body_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(content);

        // Fill the entire tabs strip with selection color so inactive tabs blend in
        let tabs_bg = Block::default().style(Style::default().bg(crate::colors::selection()));
        tabs_bg.render(tabs_area_all, buf);

        // Center the tabs vertically within the 3-row header
        let [_, tabs_area, _] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(tabs_area_all);

        // Prepare tab titles
        let titles = self
            .tabs
            .iter()
            .map(|(t, _)| Line::from(Span::raw(format!(" {t} "))))
            .collect::<Vec<_>>();

        // Unselected tabs use selection background; selected tab uses normal background
        let tabs = Tabs::new(titles)
            .select(self.selected)
            .style(
                Style::default()
                    .bg(crate::colors::selection())
                    .fg(crate::colors::text()),
            )
            .highlight_style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            )
            .divider(" ");
        Widget::render(tabs, tabs_area, buf);

        // Render selected tab content (on the inner background)
        if let Some((_, lines)) = self.tabs.get(self.selected) {
            // Ensure the diff body retains the standard background color
            let body_bg = Block::default().style(Style::default().bg(crate::colors::background()));
            body_bg.render(body_area, buf);
            let paragraph = Paragraph::new(Text::from(lines.clone()))
                .wrap(ratatui::widgets::Wrap { trim: false });
            Widget::render(paragraph, body_area, buf);
        }
    }
}
