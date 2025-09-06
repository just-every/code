//! This example demonstrates how to use the `tui_input` crate with the `crossterm` backend.
//! The example prompts the user for their name and prints a greeting.
//! The user can cancel the input by pressing `Esc` or accept the input by pressing `Enter`.

use std::io::{self, stdout, Write};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{read, Event, KeyCode, KeyEvent},
    execute, queue,
    style::Print,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use tui_input::{
    backend::crossterm::{self as backend, EventHandler},
    Input,
};

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout().lock();
    execute!(stdout, Hide, EnterAlternateScreen)?;

    let name = get_user_name(&mut stdout);

    execute!(stdout, Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;

    match name? {
        Some(name) => println!("Hello {name}!"),
        None => println!("Goodbye!"),
    }
    Ok(())
}

/// Prompts the user for their name.
///
/// Returns `None` if the user cancels the input otherwise returns the user's name. If the user
/// presses `Esc` the input is cancelled. If the user presses `Enter` the input is accepted.
///
/// # Errors
///
/// Returns an error if reading or writing to the terminal fails.
fn get_user_name(stdout: &mut impl Write) -> io::Result<Option<String>> {
    let mut input = Input::from("World");
    render_prompt(stdout, &input)?;

    loop {
        let event = read()?;
        if let Event::Key(KeyEvent { code, .. }) = event {
            match code {
                KeyCode::Esc => return Ok(None),
                KeyCode::Enter => return Ok(Some(input.to_string())),
                _ => {
                    if input.handle_event(&event).is_some() {
                        render_prompt(stdout, &input)?;
                    }
                }
            }
        }
    }
}

fn render_prompt(stdout: &mut impl Write, input: &Input) -> io::Result<()> {
    const LABEL: &str = "Name: ";
    const POSITION: (u16, u16) = (LABEL.len() as u16, 0);
    const WIDTH: u16 = 15;

    queue!(stdout, MoveTo(0, 0), Print(LABEL))?;
    backend::write(stdout, input.value(), input.cursor(), POSITION, WIDTH)?;
    stdout.flush()?;
    Ok(())
}
