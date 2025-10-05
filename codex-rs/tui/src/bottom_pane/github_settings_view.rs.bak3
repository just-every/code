use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget};

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::bottom_pane_view::BottomPaneView;
use super::BottomPane;

/// Interactive UI for GitHub workflow monitoring settings.
/// Shows token status and allows toggling the watcher on/off.
pub(crate) struct GithubSettingsView {
    watcher_enabled: bool,
    token_status: String,
    token_ready: bool,
    app_event_tx: AppEventSender,
    is_complete: bool,
    /// Selection index: 0 = toggle, 1 = close
    selected_row: usize,
}

impl GithubSettingsView {
    pub fn new(watcher_enabled: bool, token_status: String, ready: bool, app_event_tx: AppEventSender) -> Self {
        Self {
            watcher_enabled,
            token_status,
            token_ready: ready,
            app_event_tx,
            is_complete: false,
            selected_row: 0,
        }
    }

    fn toggle(&mut self) {
        self.watcher_enabled = !self.watcher_enabled;
        self.app_event_tx
            .send(AppEvent::UpdateGithubWatcher(self.watcher_enabled));
    }
}

impl<'a> BottomPaneView<'a> for GithubSettingsView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        match key_event {
            KeyEvent { code: KeyCode::Up, modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row > 0 { self.selected_row -= 1; }
            }
            KeyEvent { code: KeyCode::Down, modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row < 1 { self.selected_row += 1; }
            }
            KeyEvent { code: KeyCode::Left | KeyCode::Right, modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row == 0 { self.toggle(); }
            }
            KeyEvent { code: KeyCode::Enter, modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row == 0 {
                    // Toggle and stay; Enter behaves like toggle
                    self.toggle();
                } else {
                    // Close
                    self.is_complete = true;
                }
            }
            KeyEvent { code: KeyCode::Char(' '), modifiers: KeyModifiers::NONE, .. } => {
                if self.selected_row == 0 { self.toggle(); }
            }
            KeyEvent { code: KeyCode::Esc, .. } => {
                self.is_complete = true;
            }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool { self.is_complete }

    fn desired_height(&self, _width: u16) -> u16 { 9 }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()))
            .title(" GitHub Settings ")
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        block.render(area, buf);

        let status_line = if self.token_ready {
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("Ready", Style::default().fg(crate::colors::success()).add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(&self.token_status, Style::default().fg(crate::colors::dim())),
            ])
        } else {
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(crate::colors::text_dim())),
                Span::styled("No token", Style::default().fg(crate::colors::warning()).add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(
                    "Set GH_TOKEN/GITHUB_TOKEN or run: 'gh auth login'",
                    Style::default().fg(crate::colors::dim()),
                ),
            ])
        };

        let toggle_label = if self.watcher_enabled { "Enabled" } else { "Disabled" };
        let mut toggle_style = Style::default().fg(crate::colors::text());
        if self.selected_row == 0 { toggle_style = toggle_style.bg(crate::colors::selection()).add_modifier(Modifier::BOLD); }

        let lines = vec![
            status_line,
            Line::from(""),
            Line::from(vec![
                Span::styled("Workflow Monitoring: ", Style::default().fg(crate::colors::text_dim())),
                Span::styled(toggle_label, toggle_style),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(if self.selected_row == 1 { "› " } else { "  " }, Style::default()),
                Span::styled("Close", if self.selected_row == 1 { Style::default().bg(crate::colors::selection()).add_modifier(Modifier::BOLD) } else { Style::default() }),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("↑↓", Style::default().fg(crate::colors::light_blue())),
                Span::raw(" Navigate  "),
                Span::styled("←→/Space", Style::default().fg(crate::colors::success())),
                Span::raw(" Toggle  "),
                Span::styled("Enter", Style::default().fg(crate::colors::success())),
                Span::raw(" Toggle/Close  "),
                Span::styled("Esc", Style::default().fg(crate::colors::error())),
                Span::raw(" Cancel"),
            ]),
        ];

        let paragraph = Paragraph::new(lines)
            .alignment(Alignment::Left)
            .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()));
        paragraph.render(Rect { x: inner.x.saturating_add(1), y: inner.y, width: inner.width.saturating_sub(2), height: inner.height }, buf);
    }
}

