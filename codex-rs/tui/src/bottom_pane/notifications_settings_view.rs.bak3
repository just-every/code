use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;

use super::bottom_pane_view::BottomPaneView;
use super::BottomPane;

#[derive(Clone)]
pub(crate) enum NotificationsMode {
    Toggle { enabled: bool },
    Custom { entries: Vec<String> },
}

pub(crate) struct NotificationsSettingsView {
    mode: NotificationsMode,
    app_event_tx: AppEventSender,
    ticket: BackgroundOrderTicket,
    selected_row: usize,
    is_complete: bool,
}

impl NotificationsSettingsView {
    pub fn new(
        mode: NotificationsMode,
        app_event_tx: AppEventSender,
        ticket: BackgroundOrderTicket,
    ) -> Self {
        Self {
            mode,
            app_event_tx,
            ticket,
            selected_row: 0,
            is_complete: false,
        }
    }

    fn toggle(&mut self) {
        match &mut self.mode {
            NotificationsMode::Toggle { enabled } => {
                *enabled = !*enabled;
                self.app_event_tx
                    .send(AppEvent::UpdateTuiNotifications(*enabled));
            }
            NotificationsMode::Custom { entries } => {
                let filters = if entries.is_empty() {
                    "(none)".to_string()
                } else {
                    entries.join(", ")
                };
                self.app_event_tx.send_background_event_with_ticket(
                    &self.ticket,
                    format!(
                        "TUI notifications are filtered in config: [{}]",
                        filters
                    ),
                );
                self.app_event_tx.send_background_event_with_ticket(
                    &self.ticket,
                    "Edit ~/.code/config.toml [tui].notifications to change filters.".to_string(),
                );
            }
        }
    }

    fn status_line(&self) -> Line<'static> {
        match &self.mode {
            NotificationsMode::Toggle { enabled } => {
                let status = if *enabled { "Enabled" } else { "Disabled" };
                let color = if *enabled {
                    crate::colors::success()
                } else {
                    crate::colors::warning()
                };
                Line::from(vec![
                    Span::styled("Status: ", Style::default().fg(crate::colors::text_dim())),
                    Span::styled(status, Style::default().fg(color).add_modifier(Modifier::BOLD)),
                ])
            }
            NotificationsMode::Custom { entries } => {
                let filters = if entries.is_empty() {
                    "<none>".to_string()
                } else {
                    entries.join(", ")
                };
                Line::from(vec![
                    Span::styled("Status: ", Style::default().fg(crate::colors::text_dim())),
                    Span::styled("Custom filter", Style::default().fg(crate::colors::info()).add_modifier(Modifier::BOLD)),
                    Span::raw("  "),
                    Span::styled(filters, Style::default().fg(crate::colors::dim())),
                ])
            }
        }
    }

    fn toggle_line(&self) -> Line<'static> {
        match &self.mode {
            NotificationsMode::Toggle { enabled } => {
                let label = if *enabled { "Enabled" } else { "Disabled" };
                Line::from(vec![
                    Span::styled("Notifications: ", Style::default().fg(crate::colors::text_dim())),
                    Span::raw(label),
                ])
            }
            NotificationsMode::Custom { .. } => {
                Line::from(vec![
                    Span::styled(
                        "Notifications are managed by your config file.",
                        Style::default().fg(crate::colors::text()),
                    ),
                ])
            }
        }
    }
}

impl<'a> BottomPaneView<'a> for NotificationsSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        match key_event {
            KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row > 0 {
                    self.selected_row -= 1;
                }
            }
            KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row < 1 {
                    self.selected_row += 1;
                }
            }
            KeyEvent { code: KeyCode::Left | KeyCode::Right, modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row == 0 {
                    self.toggle();
                }
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row == 0 {
                    self.toggle();
                } else {
                    self.is_complete = true;
                }
            }
            KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row == 0 {
                    self.toggle();
                }
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
            }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        9
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" Notifications ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let mut lines = Vec::new();
        lines.push(self.status_line());
        lines.push(Line::from(""));
        let mut toggle_line = self.toggle_line();
        if self.selected_row == 0 {
            toggle_line = toggle_line
                .style(
                    Style::default()
                        .bg(crate::colors::selection())
                        .add_modifier(Modifier::BOLD),
                );
        }
        lines.push(toggle_line);
        lines.push(Line::from(""));
        let mut close_line = Line::from(vec![
            Span::raw(if self.selected_row == 1 { "> " } else { "  " }),
            Span::raw("Close"),
        ]);
        if self.selected_row == 1 {
            close_line = close_line
                .style(
                    Style::default()
                        .bg(crate::colors::selection())
                        .add_modifier(Modifier::BOLD),
                );
        }
        lines.push(close_line);
        lines.push(Line::from(""));

        let footer = match &self.mode {
            NotificationsMode::Toggle { .. } => Line::from(vec![
                Span::styled("Up/Down", Style::default().fg(crate::colors::light_blue())),
                Span::raw(" Navigate  "),
                Span::styled("Left/Right or Space", Style::default().fg(crate::colors::success())),
                Span::raw(" Toggle  "),
                Span::styled("Enter", Style::default().fg(crate::colors::success())),
                Span::raw(" Toggle or Close  "),
                Span::styled("Esc", Style::default().fg(crate::colors::error())),
                Span::raw(" Cancel"),
            ]),
            NotificationsMode::Custom { .. } => Line::from(vec![
                Span::styled("Edit ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("[tui].notifications", Style::default().fg(crate::colors::info())),
                Span::styled(" in ~/.code/config.toml to adjust filters.", Style::default().fg(crate::colors::text_dim())),
            ]),
        };
        lines.push(footer);

        let paragraph = Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()));
        paragraph.render(
            Rect {
                x: inner.x.saturating_add(1),
                y: inner.y,
                width: inner.width.saturating_sub(2),
                height: inner.height,
            },
            buf,
        );
    }
}
