use codex_core::config_types::TextVerbosity;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
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

/// Interactive UI for selecting text verbosity level
pub(crate) struct VerbositySelectionView {
    current_verbosity: TextVerbosity,
    selected_verbosity: TextVerbosity,
    app_event_tx: AppEventSender,
    is_complete: bool,
}

impl VerbositySelectionView {
    pub fn new(current_verbosity: TextVerbosity, app_event_tx: AppEventSender) -> Self {
        Self {
            current_verbosity,
            selected_verbosity: current_verbosity,
            app_event_tx,
            is_complete: false,
        }
    }

    fn get_verbosity_options() -> Vec<(TextVerbosity, &'static str, &'static str)> {
        vec![
            (TextVerbosity::Low, "Low", "Concise responses"),
            (TextVerbosity::Medium, "Medium", "Balanced detail (default)"),
            (TextVerbosity::High, "High", "Detailed responses"),
        ]
    }

    fn move_selection_up(&mut self) {
        let options = Self::get_verbosity_options();
        let current_idx = options
            .iter()
            .position(|(v, _, _)| *v == self.selected_verbosity)
            .unwrap_or(0);

        let new_idx = if current_idx == 0 {
            options.len() - 1
        } else {
            current_idx - 1
        };

        self.selected_verbosity = options[new_idx].0;
    }

    fn move_selection_down(&mut self) {
        let options = Self::get_verbosity_options();
        let current_idx = options
            .iter()
            .position(|(v, _, _)| *v == self.selected_verbosity)
            .unwrap_or(0);

        let new_idx = (current_idx + 1) % options.len();
        self.selected_verbosity = options[new_idx].0;
    }

    fn confirm_selection(&self) {
        // Send event to update text verbosity
        self.app_event_tx
            .send(AppEvent::UpdateTextVerbosity(self.selected_verbosity));
    }
}

impl<'a> BottomPaneView<'a> for VerbositySelectionView {
    fn handle_key_event(&mut self, _pane: &mut BottomPane<'a>, key_event: KeyEvent) {
        match key_event {
            KeyEvent {
                code: KeyCode::Up,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_selection_up();
            }
            KeyEvent {
                code: KeyCode::Down,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.move_selection_down();
            }
            KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.confirm_selection();
                self.is_complete = true;
            }
            KeyEvent {
                code: KeyCode::Esc,
                modifiers: KeyModifiers::NONE,
                ..
            } => {
                self.is_complete = true;
            }
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.is_complete
    }

    fn desired_height(&self, _width: u16) -> u16 {
        9 // Height for the selection box
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        // Clear the area first
        Clear.render(area, buf);

        // Create a centered box with theme-aware styling
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(crate::colors::border()))
            .style(
                Style::default()
                    .bg(crate::colors::background())
                    .fg(crate::colors::text()),
            )
            .title(" Select Text Verbosity ")
            .title_alignment(Alignment::Center);

        let inner_area = block.inner(area);
        block.render(area, buf);

        // Build the content
        let mut lines = vec![Line::from(vec![
            Span::raw("Value: "),
            Span::styled(
                format!("{}", self.current_verbosity),
                Style::default()
                    .fg(crate::colors::warning())
                    .add_modifier(Modifier::BOLD),
            ),
        ])];

        // Add options
        for (verbosity, name, description) in Self::get_verbosity_options() {
            let is_selected = verbosity == self.selected_verbosity;
            let is_current = verbosity == self.current_verbosity;

            let mut style = Style::default();
            if is_selected {
                style = style
                    .bg(crate::colors::selection())
                    .add_modifier(Modifier::BOLD);
            }
            if is_current {
                style = style.fg(crate::colors::warning());
            }

            let prefix = if is_selected { "› " } else { "  " };
            let line = Line::from(vec![
                Span::raw(prefix),
                Span::styled(format!("{:<8}", name), style),
                Span::raw(" - "),
                Span::styled(description, Style::default().fg(crate::colors::dim())),
            ]);
            lines.push(line);
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(crate::colors::light_blue())),
            Span::raw(" Navigate  "),
            Span::styled("Enter", Style::default().fg(crate::colors::success())),
            Span::raw(" Select  "),
            Span::styled("Esc", Style::default().fg(crate::colors::error())),
            Span::raw(" Cancel"),
        ]));

        let padded = Rect {
            x: inner_area.x.saturating_add(1),
            y: inner_area.y,
            width: inner_area.width.saturating_sub(1),
            height: inner_area.height,
        };
        let paragraph = Paragraph::new(lines).alignment(Alignment::Left).style(
            Style::default()
                .bg(crate::colors::background())
                .fg(crate::colors::text()),
        );
        paragraph.render(padded, buf);
    }
}
