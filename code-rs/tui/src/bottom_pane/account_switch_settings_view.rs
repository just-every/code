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

pub(crate) struct AccountSwitchSettingsView {
    app_event_tx: AppEventSender,
    selected_index: usize,
    auto_switch_enabled: bool,
    api_key_fallback_enabled: bool,
    is_complete: bool,
}

impl AccountSwitchSettingsView {
    pub(crate) fn new(
        app_event_tx: AppEventSender,
        auto_switch_enabled: bool,
        api_key_fallback_enabled: bool,
    ) -> Self {
        Self {
            app_event_tx,
            selected_index: 0,
            auto_switch_enabled,
            api_key_fallback_enabled,
            is_complete: false,
        }
    }

    fn option_count() -> usize {
        3
    }

    fn toggle_auto_switch(&mut self) {
        self.auto_switch_enabled = !self.auto_switch_enabled;
        self.app_event_tx
            .send(AppEvent::SetAutoSwitchAccountsOnRateLimit(
                self.auto_switch_enabled,
            ));
    }

    fn toggle_api_key_fallback(&mut self) {
        self.api_key_fallback_enabled = !self.api_key_fallback_enabled;
        self.app_event_tx
            .send(AppEvent::SetApiKeyFallbackOnAllAccountsLimited(
                self.api_key_fallback_enabled,
            ));
    }

    fn close(&mut self) {
        self.is_complete = true;
    }

    fn activate_selected(&mut self) {
        match self.selected_index {
            0 => self.toggle_auto_switch(),
            1 => self.toggle_api_key_fallback(),
            2 => self.close(),
            _ => {}
        }
    }

    fn info_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        lines.push(Line::from(vec![Span::styled(
            "Accounts",
            Style::default().add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(""));

        let highlight = Style::default()
            .fg(colors::primary())
            .add_modifier(Modifier::BOLD);
        let normal = Style::default().fg(colors::text());
        let dim = Style::default().fg(colors::text_dim());

        let row = |idx: usize, label: &str, enabled: bool| -> Line<'static> {
            let selected = idx == self.selected_index;
            let indicator = if selected { ">" } else { " " };
            let style = if selected { highlight } else { normal };
            let state_style = if enabled {
                Style::default().fg(colors::success())
            } else {
                Style::default().fg(colors::text_dim())
            };
            Line::from(vec![
                Span::styled(format!("{indicator} "), style),
                Span::styled(label.to_string(), style),
                Span::raw("  "),
                Span::styled(
                    format!("[{}]", if enabled { "x" } else { " " }),
                    state_style,
                ),
            ])
        };

        lines.push(row(
            0,
            "Auto-switch on rate/usage limit",
            self.auto_switch_enabled,
        ));
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "Switches to another connected account on 429/usage_limit.",
                dim,
            ),
        ]));

        lines.push(row(
            1,
            "API key fallback when all accounts limited",
            self.api_key_fallback_enabled,
        ));
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                "Only used if every connected ChatGPT account is limited.",
                dim,
            ),
        ]));

        lines.push(Line::from(""));

        let close_selected = self.selected_index == 2;
        let close_style = if close_selected { highlight } else { normal };
        let indicator = if close_selected { ">" } else { " " };
        lines.push(Line::from(vec![
            Span::styled(format!("{indicator} "), close_style),
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
