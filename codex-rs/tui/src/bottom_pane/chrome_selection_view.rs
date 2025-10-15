use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Alignment;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Clear;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;

use super::BottomPane;
use super::bottom_pane_view::BottomPaneView;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChromeLaunchOption {
    CloseAndUseProfile,
    UseTempProfile,
    UseInternalBrowser,
    Cancel,
}

/// Interactive UI for selecting Chrome launch options when CDP connection fails
pub(crate) struct ChromeSelectionView {
    selected_index: usize,
    app_event_tx: AppEventSender,
    is_complete: bool,
    port: Option<u16>,
}

impl ChromeSelectionView {
    pub fn new(app_event_tx: AppEventSender, port: Option<u16>) -> Self {
        Self {
            selected_index: 0,
            app_event_tx,
            is_complete: false,
            port,
        }
    }

    fn get_options() -> Vec<(ChromeLaunchOption, &'static str, &'static str)> {
        vec![
            (
                ChromeLaunchOption::CloseAndUseProfile,
                "Close existing Chrome & use your profile",
                "Closes any running Chrome and launches with your profile",
            ),
            (
                ChromeLaunchOption::UseTempProfile,
                "Use temporary profile",
                "Launches Chrome with a clean profile (no saved logins)",
            ),
            (
                ChromeLaunchOption::UseInternalBrowser,
                "Use internal browser (/browser)",
                "Uses the built-in browser instead of Chrome",
            ),
            (
                ChromeLaunchOption::Cancel,
                "Cancel",
                "Don't launch any browser",
            ),
        ]
    }

    fn move_selection_up(&mut self) {
        let options = Self::get_options();
        if self.selected_index == 0 {
            self.selected_index = options.len() - 1;
        } else {
            self.selected_index -= 1;
        }
    }

    fn move_selection_down(&mut self) {
        let options = Self::get_options();
        self.selected_index = (self.selected_index + 1) % options.len();
    }

    fn confirm_selection(&mut self) {
        let options = Self::get_options();
        let selected = options[self.selected_index].0;

        // Send the selected option event
        self.app_event_tx
            .send(AppEvent::ChromeLaunchOptionSelected(selected, self.port));

        self.is_complete = true;
    }

    fn cancel(&mut self) {
        // Send cancel event
        self.app_event_tx.send(AppEvent::ChromeLaunchOptionSelected(
            ChromeLaunchOption::Cancel,
            self.port,
        ));

        self.is_complete = true;
    }
}

impl<'a> BottomPaneView<'a> for ChromeSelectionView {
    fn handle_key_event(&mut self, _bottom_pane: &mut BottomPane, key: KeyEvent) {
        if self.is_complete {
            return;
        }

        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_selection_up();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_selection_down();
            }
            KeyCode::Enter => {
                self.confirm_selection();
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                self.cancel();
            }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Create the selection box with theme-aware styling
        Clear.render(area, buf);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Chrome Launch Options ")
            .title_alignment(Alignment::Center)
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            )
            .border_style(Style::default().fg(crate::colors::border()));

        let inner = block.inner(area);
        block.render(area, buf);

        let mut lines = Vec::new();

        // Add header
        lines.push(Line::from(vec![Span::styled(
            "Chrome is already running or CDP connection failed",
            Style::default()
                .fg(crate::colors::warning())
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(""));
        lines.push(Line::from("Select an option:"));
        lines.push(Line::from(""));

        // Add options
        let options = Self::get_options();
        for (i, (_, label, description)) in options.iter().enumerate() {
            let is_selected = i == self.selected_index;

            if is_selected {
                // Highlighted option
                lines.push(Line::from(vec![Span::styled(
                    format!("› {}", label),
                    Style::default()
                        .fg(crate::colors::success())
                        .add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(vec![Span::styled(
                    format!("  {}", description),
                    Style::default().fg(crate::colors::secondary()),
                )]));
            } else {
                // Non-selected option
                lines.push(Line::from(vec![Span::styled(
                    format!("  {}", label),
                    Style::default().fg(crate::colors::text()),
                )]));
                lines.push(Line::from(vec![Span::styled(
                    format!("  {}", description),
                    Style::default().fg(crate::colors::text_dim()),
                )]));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "↑↓/jk: Navigate  ",
                Style::default().fg(crate::colors::text_dim()),
            ),
            Span::styled(
                "Enter: Select  ",
                Style::default().fg(crate::colors::text_dim()),
            ),
            Span::styled(
                "Esc/q: Cancel",
                Style::default().fg(crate::colors::text_dim()),
            ),
        ]));

        let padded = Rect {
            x: inner.x.saturating_add(1),
            y: inner.y,
            width: inner.width.saturating_sub(1),
            height: inner.height,
        };
        let paragraph = Paragraph::new(lines).alignment(Alignment::Left).style(
            Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text()),
        );
        paragraph.render(padded, buf);
    }

    fn desired_height(&self, _width: u16) -> u16 {
        20 // Fixed height for the selection dialog
    }
}
