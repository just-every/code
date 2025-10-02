use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};
use std::cell::Cell;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::bottom_pane_view::BottomPaneView;
use super::scroll_state::ScrollState;
use super::BottomPane;

#[derive(Clone, Debug)]
pub(crate) struct McpServerRow {
    pub name: String,
    pub enabled: bool,
    pub summary: String,
}

pub(crate) type McpServerRows = Vec<McpServerRow>;

pub(crate) struct McpSettingsView {
    rows: McpServerRows,
    state: ScrollState,
    is_complete: bool,
    app_event_tx: AppEventSender,
    visible_rows: Cell<usize>,
}

impl McpSettingsView {
    pub fn new(rows: McpServerRows, app_event_tx: AppEventSender) -> Self {
        let mut state = ScrollState::new();
        state.selected_idx = match rows.len() {
            0 => Some(0), // default to Add row for empty lists
            _ => Some(0),
        };
        Self { rows, state, is_complete: false, app_event_tx, visible_rows: Cell::new(16) }
    }

    fn len(&self) -> usize { self.rows.len().saturating_add(2) /* + Add + Close */ }

    fn selected_index(&self) -> usize { self.state.selected_idx.unwrap_or(0).min(self.len().saturating_sub(1)) }

    fn line_for_index(&self, index: usize) -> usize {
        if self.rows.is_empty() {
            let base = 2; // empty-state message + blank line
            return if index == 0 { base + 1 } else { base + 2 };
        }

        if index < self.rows.len() {
            return index.saturating_mul(2);
        }

        let base = self.rows.len().saturating_mul(2);
        if index == self.rows.len() {
            base + 1
        } else {
            base + 2
        }
    }

    fn ensure_visible_with_capacity(&mut self, visible: usize) {
        let total = self.total_lines();
        if visible == 0 || total <= visible {
            self.state.scroll_top = 0;
            return;
        }

        let index = self.selected_index();
        let line = self.line_for_index(index).min(total.saturating_sub(1));
        if line < self.state.scroll_top {
            self.state.scroll_top = line;
            return;
        }

        let last = self.state.scroll_top.saturating_add(visible.saturating_sub(1));
        if line > last {
            let max_top = total.saturating_sub(visible);
            let new_top = line.saturating_sub(visible.saturating_sub(1));
            self.state.scroll_top = new_top.min(max_top);
        }
    }

    fn total_lines(&self) -> usize {
        let mut base = self.rows.len().saturating_mul(2);
        if self.rows.is_empty() {
            base = base.saturating_add(2);
        }
        base.saturating_add(5)
    }

    fn move_up(&mut self) {
        let len = self.len();
        if len == 0 {
            self.state.selected_idx = Some(0);
            return;
        }
        let next = match self.state.selected_idx {
            Some(0) | None => len.saturating_sub(1),
            Some(idx) => idx.saturating_sub(1),
        };
        self.state.selected_idx = Some(next);
        self.ensure_visible_with_capacity(self.visible_rows.get().max(1));
    }

    fn move_down(&mut self) {
        let len = self.len();
        if len == 0 {
            self.state.selected_idx = Some(0);
            return;
        }
        let next = match self.state.selected_idx {
            Some(idx) if idx + 1 < len => idx + 1,
            _ => 0,
        };
        self.state.selected_idx = Some(next);
        self.ensure_visible_with_capacity(self.visible_rows.get().max(1));
    }

    fn on_toggle(&mut self) {
        let idx = self.selected_index();
        if idx >= self.rows.len() {
            return;
        }
        if let Some(row) = self.rows.get_mut(idx) {
            let new_enabled = !row.enabled;
            row.enabled = new_enabled;
            self.app_event_tx.send(AppEvent::UpdateMcpServer { name: row.name.clone(), enable: new_enabled });
        }
    }

    fn on_enter(&mut self) {
        let idx = self.selected_index();
        if idx < self.rows.len() {
            self.on_toggle();
        } else if idx == self.rows.len() {
            self.app_event_tx.send(AppEvent::PrefillComposer("/mcp add ".to_string()));
            self.is_complete = true;
        } else {
            self.is_complete = true;
        }
    }
}

impl<'a> BottomPaneView<'a> for McpSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        match key_event {
            KeyEvent { code: KeyCode::Up, .. } => self.move_up(),
            KeyEvent { code: KeyCode::Down, .. } => self.move_down(),
            KeyEvent { code: KeyCode::Left | KeyCode::Right, .. } | KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. } => self.on_toggle(),
            KeyEvent { code: KeyCode::Enter, .. } => self.on_enter(),
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
            }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool { self.is_complete }

    fn desired_height(&self, _width: u16) -> u16 { 16 }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" MCP Servers ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let mut lines: Vec<Line<'static>> = Vec::new();
        if self.rows.is_empty() {
            lines.push(Line::from(vec![Span::styled("No MCP servers configured.", Style::default().fg(crate::colors::text_dim()))]));
            lines.push(Line::from(""));
        }

        for (i, row) in self.rows.iter().enumerate() {
            let sel = Some(i) == self.state.selected_idx;
            let check = if row.enabled { "[on ]" } else { "[off]" };
            let name = format!("{} {}", check, row.name);
            let name_style = if sel { Style::default().bg(crate::colors::selection()).add_modifier(Modifier::BOLD) } else { Style::default() };
            lines.push(Line::from(vec![
                Span::styled(if sel { "› " } else { "  " }, Style::default()),
                Span::styled(name, name_style),
            ]));
            let summary_style = if sel { Style::default().bg(crate::colors::selection()).fg(crate::colors::secondary()) } else { Style::default().fg(crate::colors::text_dim()) };
            lines.push(Line::from(vec![
                Span::styled("   ", Style::default()),
                Span::styled(row.summary.clone(), summary_style),
            ]));
        }

        let add_sel = self.state.selected_idx == Some(self.rows.len());
        let add_style = if add_sel { Style::default().bg(crate::colors::selection()).add_modifier(Modifier::BOLD) } else { Style::default() };
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(if add_sel { "› " } else { "  " }, Style::default()), Span::styled("Add new server…", add_style)]));

        let close_sel = self.state.selected_idx == Some(self.rows.len().saturating_add(1));
        let close_style = if close_sel { Style::default().bg(crate::colors::selection()).add_modifier(Modifier::BOLD) } else { Style::default() };
        lines.push(Line::from(vec![Span::styled(if close_sel { "› " } else { "  " }, Style::default()), Span::styled("Close", close_style)]));

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("↑↓/←→", Style::default().fg(crate::colors::function())),
            Span::styled(" Navigate/Toggle  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Enter", Style::default().fg(crate::colors::success())),
            Span::styled(" Toggle/Open  ", Style::default().fg(crate::colors::text_dim())),
            Span::styled("Esc", Style::default().fg(crate::colors::error())),
            Span::styled(" Close", Style::default().fg(crate::colors::text_dim())),
        ]));

        let mut paragraph = Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()));

        let visible = inner.height as usize;
        self.visible_rows.set(visible);

        let mut scroll_state = self.state;
        scroll_state.ensure_visible(self.len(), visible.max(1));
        paragraph = paragraph.scroll((scroll_state.scroll_top.min(u16::MAX as usize) as u16, 0));

        paragraph.render(Rect { x: inner.x.saturating_add(1), y: inner.y, width: inner.width.saturating_sub(2), height: inner.height }, buf);
    }
}
