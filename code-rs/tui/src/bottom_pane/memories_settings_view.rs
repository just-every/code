use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::colors;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

pub(crate) struct MemoriesSettingsView {
    app_event_tx: AppEventSender,
    selected_index: usize,
    memories_enabled: bool,
    is_complete: bool,
}

impl MemoriesSettingsView {
    pub(crate) fn new(app_event_tx: AppEventSender, memories_enabled: bool) -> Self {
        Self {
            app_event_tx,
            selected_index: 0,
            memories_enabled,
            is_complete: false,
        }
    }

    fn option_count() -> usize {
        2
    }

    fn toggle_memories(&mut self) {
        self.memories_enabled = !self.memories_enabled;
        self.app_event_tx
            .send(AppEvent::SetMemoriesEnabled(self.memories_enabled));
    }

    fn close(&mut self) {
        self.is_complete = true;
    }

    fn activate_selected(&mut self) {
        match self.selected_index {
            0 => self.toggle_memories(),
            1 => self.close(),
            _ => {}
        }
    }

    fn info_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        lines.push(Line::from(vec![Span::styled(
            "Memories",
            Style::default().add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(""));

        let highlight = Style::default()
            .fg(colors::primary())
            .add_modifier(Modifier::BOLD);
        let normal = Style::default().fg(colors::text());
        let dim = Style::default().fg(colors::text_dim());

        let selected = self.selected_index == 0;
        let indicator = if selected { ">" } else { " " };
        let style = if selected { highlight } else { normal };
        let state_style = if self.memories_enabled {
            Style::default().fg(colors::success())
        } else {
            Style::default().fg(colors::text_dim())
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{indicator} "), style),
            Span::styled("Use Cross-Session Memories".to_string(), style),
            Span::raw("  "),
            Span::styled(
                format!("[{}]", if self.memories_enabled { "x" } else { " " }),
                state_style,
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "Controls memory prompts for future turns. Turn it off to stop injecting memory guidance.",
                dim,
            ),
        ]));

        lines.push(Line::from(""));
        let close_selected = self.selected_index == 1;
        let close_style = if close_selected { highlight } else { normal };
        let close_indicator = if close_selected { ">" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(format!("{close_indicator} "), close_style),
            Span::styled("Close", close_style),
        ]));

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(" Up/Down", Style::default().fg(colors::function())),
            Span::styled(" Navigate  ", dim),
            Span::styled("Enter", Style::default().fg(colors::success())),
            Span::styled(" Toggle  ", dim),
            Span::styled("Esc", Style::default().fg(colors::error())),
            Span::styled(" Close", dim),
        ]));

        lines
    }

    pub(crate) fn render_without_frame(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        Paragraph::new(self.info_lines())
            .wrap(Wrap { trim: true })
            .style(Style::default().bg(colors::background()).fg(colors::text()))
            .render(area, buf);
    }

    pub(crate) fn handle_key_event_direct(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Esc => self.close(),
            KeyCode::Up => {
                self.selected_index = self.selected_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Tab => {
                self.selected_index = (self.selected_index + 1) % Self::option_count();
            }
            KeyCode::BackTab => {
                if self.selected_index == 0 {
                    self.selected_index = Self::option_count() - 1;
                } else {
                    self.selected_index = self.selected_index.saturating_sub(1);
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => self.activate_selected(),
            _ => {}
        }
    }

    pub(crate) fn is_view_complete(&self) -> bool {
        self.is_complete
    }
}
