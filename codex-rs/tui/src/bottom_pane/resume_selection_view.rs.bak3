use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};
use ratatui::widgets::Widget;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::bottom_pane_view::{BottomPaneView, ConditionalUpdate};
use super::{BottomPane, popup_consts::MAX_POPUP_ROWS};

pub struct ResumeRow {
    pub modified: String,
    pub created: String,
    pub msgs: String,
    pub branch: String,
    pub summary: String,
    pub path: std::path::PathBuf,
}

pub struct ResumeSelectionView {
    title: String,
    subtitle: String,
    rows: Vec<ResumeRow>,
    selected: usize,
    // Topmost row index currently visible in the table viewport
    top: usize,
    complete: bool,
    app_event_tx: AppEventSender,
}

impl ResumeSelectionView {
    pub fn new(title: String, subtitle: String, rows: Vec<ResumeRow>, app_event_tx: AppEventSender) -> Self {
        Self { title, subtitle, rows, selected: 0, top: 0, complete: false, app_event_tx }
    }

    fn move_up(&mut self) {
        if self.rows.is_empty() { return; }
        if self.selected == 0 { self.selected = self.rows.len().saturating_sub(1); }
        else { self.selected -= 1; }
        self.ensure_selected_visible();
    }

    fn move_down(&mut self) {
        if self.rows.is_empty() { return; }
        self.selected = (self.selected + 1) % self.rows.len();
        self.ensure_selected_visible();
    }

    fn page_up(&mut self) {
        if self.rows.is_empty() { return; }
        let page = self.visible_rows();
        if self.selected >= page { self.selected -= page; } else { self.selected = 0; }
        self.ensure_selected_visible();
    }

    fn page_down(&mut self) {
        if self.rows.is_empty() { return; }
        let page = self.visible_rows();
        self.selected = (self.selected + page).min(self.rows.len().saturating_sub(1));
        self.ensure_selected_visible();
    }

    fn go_home(&mut self) {
        if self.rows.is_empty() { return; }
        self.selected = 0;
        self.ensure_selected_visible();
    }

    fn go_end(&mut self) {
        if self.rows.is_empty() { return; }
        self.selected = self.rows.len().saturating_sub(1);
        self.ensure_selected_visible();
    }

    fn visible_rows(&self) -> usize {
        self.rows.len().clamp(1, MAX_POPUP_ROWS)
    }

    fn ensure_selected_visible(&mut self) {
        let page = self.visible_rows();
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top.saturating_add(page) {
            self.top = self.selected.saturating_sub(page.saturating_sub(1));
        }
    }
}

impl BottomPaneView<'_> for ResumeSelectionView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'_>, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Up => self.move_up(),
            KeyCode::Down => self.move_down(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::Home => self.go_home(),
            KeyCode::End => self.go_end(),
            KeyCode::Enter => {
                if let Some(row) = self.rows.get(self.selected) {
                    self.app_event_tx.send(AppEvent::ResumeFrom(row.path.clone()));
                    self.complete = true;
                }
            }
            KeyCode::Esc => self.complete = true,
            _ => {}
        }
    }

    fn is_complete(&self) -> bool { self.complete }

    fn on_ctrl_c(&mut self, _pane: &mut BottomPane<'_>) -> super::CancellationEvent {
        self.complete = true; super::CancellationEvent::Handled
    }

    fn update_status_text(&mut self, _text: String) -> ConditionalUpdate { ConditionalUpdate::NeedsRedraw }

    fn desired_height(&self, _width: u16) -> u16 {
        // Include block borders (+2), optional subtitle (+1), table header (+1),
        // clamped rows, spacer (+1), footer (+1)
        let rows = self.rows.len().clamp(1, MAX_POPUP_ROWS) as u16;
        let subtitle = if self.subtitle.is_empty() { 0 } else { 1 };
        2 + subtitle + 1 + rows + 1 + 1
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 { return; }

        // Clear and draw a bordered block that uses the active theme colors.
        // Other popups (e.g., list_selection_view) already do this; mirroring
        // that treatment ensures dialogs respect dark/light themes.
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(self.title.clone())
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        // Optional subtitle (path, etc.)
        let mut next_y = inner.y;
        if !self.subtitle.is_empty() {
            Paragraph::new(Line::from(Span::styled(
                &self.subtitle,
                Style::default().fg(crate::colors::text_dim()),
            )))
            .render(Rect { x: inner.x.saturating_add(1), y: next_y, width: inner.width.saturating_sub(1), height: 1 }, buf);
            next_y = next_y.saturating_add(1);
        }

        // Reserve one blank spacer line above the footer
        let footer_reserved: u16 = 2;
        let table_area = Rect {
            x: inner.x.saturating_add(1),
            y: next_y,
            width: inner.width.saturating_sub(1),
            height: inner
                .height
                .saturating_sub(footer_reserved + (next_y - inner.y)),
        };

        // Build rows (windowed to the visible viewport)
        let page = self.visible_rows();
        let start = self.top.min(self.rows.len());
        let end = (start + page).min(self.rows.len());
        let rows_iter = self.rows[start..end].iter().enumerate().map(|(idx, r)| {
            let i = start + idx; // absolute index
            let cells = vec![
                r.modified.clone(), r.created.clone(), r.msgs.clone(), r.branch.clone(), r.summary.clone()
            ].into_iter().map(|c| ratatui::widgets::Cell::from(c));
            let mut row = Row::new(cells).height(1);
            if i == self.selected {
                row = row.style(Style::default().bg(crate::colors::selection()).add_modifier(Modifier::BOLD));
            }
            row
        });

        // Column constraints roughly match header widths
        let widths = [
            Constraint::Length(10), // Modified
            Constraint::Length(10), // Created
            Constraint::Length(6),  // #Msgs
            Constraint::Length(10), // Branch
            Constraint::Min(10),    // Summary
        ];

        let header = Row::new(vec!["Modified", "Created", "#Msgs", "Branch", "Summary"]).height(1)
            .style(Style::default().fg(crate::colors::text_bright()));

        let table = Table::new(rows_iter, widths)
            .header(header)
            .highlight_symbol("")
            .column_spacing(1);
        table.render(table_area, buf);

        // Footer hints
        // Draw a spacer line above footer (implicit by not drawing into that row)
        let footer = Rect { x: inner.x.saturating_add(1), y: inner.y + inner.height - 1, width: inner.width.saturating_sub(1), height: 1 };
        let footer_line = Line::from(vec![
            Span::styled("↑↓ PgUp PgDn", Style::default().fg(crate::colors::light_blue())),
            Span::raw(" Navigate  "),
            Span::styled("Enter", Style::default().fg(crate::colors::success())),
            Span::raw(" Select  "),
            Span::styled("Esc", Style::default().fg(crate::colors::error())),
            Span::raw(" Cancel"),
        ]);
        Paragraph::new(footer_line)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .render(footer, buf);
    }
}
