use std::io;

use ratatui::{
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, ToSpan},
    widgets::{Block, List, Paragraph},
    DefaultTerminal, Frame,
};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();
    let result = App::default().run(&mut terminal);
    ratatui::restore();
    result
}

/// App holds the state of the application
#[derive(Debug, Default)]
struct App {
    /// Current value of the input box
    input: Input,
    /// Current input mode
    input_mode: InputMode,
    /// History of recorded messages
    messages: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    #[default]
    Normal,
    Editing,
}

impl App {
    fn run(mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        loop {
            terminal.draw(|frame| self.render(frame))?;

            let event = event::read()?;
            if let Event::Key(key) = event {
                match self.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('e') => self.start_editing(),
                        KeyCode::Char('q') => return Ok(()), // exit
                        _ => {}
                    },
                    InputMode::Editing => match key.code {
                        KeyCode::Enter => self.push_message(),
                        KeyCode::Esc => self.stop_editing(),
                        _ => {
                            self.input.handle_event(&event);
                        }
                    },
                }
            }
        }
    }

    fn start_editing(&mut self) {
        self.input_mode = InputMode::Editing
    }

    fn stop_editing(&mut self) {
        self.input_mode = InputMode::Normal
    }

    fn push_message(&mut self) {
        self.messages.push(self.input.value_and_reset());
    }

    fn render(&self, frame: &mut Frame) {
        let [header_area, input_area, messages_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .areas(frame.area());

        self.render_help_message(frame, header_area);
        self.render_input(frame, input_area);
        self.render_messages(frame, messages_area);
    }

    fn render_help_message(&self, frame: &mut Frame, area: Rect) {
        let help_message = Line::from_iter(match self.input_mode {
            InputMode::Normal => [
                "Press ".to_span(),
                "q".bold(),
                " to exit, ".to_span(),
                "e".bold(),
                " to start editing.".to_span(),
            ],
            InputMode::Editing => [
                "Press ".to_span(),
                "Esc".bold(),
                " to stop editing, ".to_span(),
                "Enter".bold(),
                " to record the message".to_span(),
            ],
        });
        frame.render_widget(help_message, area);
    }

    fn render_input(&self, frame: &mut Frame, area: Rect) {
        // keep 2 for borders and 1 for cursor
        let width = area.width.max(3) - 3;
        let scroll = self.input.visual_scroll(width as usize);
        let style = match self.input_mode {
            InputMode::Normal => Style::default(),
            InputMode::Editing => Color::Yellow.into(),
        };
        let input = Paragraph::new(self.input.value())
            .style(style)
            .scroll((0, scroll as u16))
            .block(Block::bordered().title("Input"));
        frame.render_widget(input, area);

        if self.input_mode == InputMode::Editing {
            // Ratatui hides the cursor unless it's explicitly set. Position the  cursor past the
            // end of the input text and one line down from the border to the input line
            let x = self.input.visual_cursor().max(scroll) - scroll + 1;
            frame.set_cursor_position((area.x + x as u16, area.y + 1))
        }
    }

    fn render_messages(&self, frame: &mut Frame, area: Rect) {
        let messages = self
            .messages
            .iter()
            .enumerate()
            .map(|(i, message)| format!("{}: {}", i, message));
        let messages = List::new(messages).block(Block::bordered().title("Messages"));
        frame.render_widget(messages, area);
    }
}
