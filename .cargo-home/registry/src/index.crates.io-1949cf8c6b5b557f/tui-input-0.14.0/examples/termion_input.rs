//! This example demonstrates how to use the `tui_input` crate with the `crossterm` backend.
//! The example prompts the user for their name and prints a greeting.
//! The user can cancel the input by pressing `Esc` or accept the input by pressing `Enter`.

use std::io::{self, stdin, stdout, Write};

use termion::{
    cursor::{Goto, Hide, Show},
    event::{Event, Key},
    input::TermRead,
    raw::IntoRawMode,
    screen::IntoAlternateScreen,
};
use tui_input::{
    backend::termion::{self as backend, EventHandler},
    Input,
};

fn main() -> io::Result<()> {
    let mut stdout = stdout().into_raw_mode()?.into_alternate_screen()?;
    write!(stdout, "{}", Hide)?;
    let name = get_user_name(&mut stdout);
    write!(stdout, "{}", Show)?;
    drop(stdout); // disable raw mode and leave alternate screen

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
    let mut input: Input = "World".into();
    render_prompt(stdout, &input)?;

    for event in stdin().events() {
        match event? {
            Event::Key(Key::Esc) => return Err(io::Error::other("error")),
            Event::Key(Key::Char('\n')) => return Ok(Some(input.to_string())),
            event => {
                if input.handle_event(&event).is_some() {
                    render_prompt(stdout, &input)?;
                }
            }
        }
    }
    Ok(None) // reached end of input
}

fn render_prompt(stdout: &mut impl Write, input: &Input) -> io::Result<()> {
    const LABEL: &str = "Name: ";
    const POSITION: (u16, u16) = (LABEL.len() as u16, 0);

    write!(stdout, "{}{LABEL}", Goto(1, 1))?;
    backend::write(stdout, input.value(), input.cursor(), POSITION, 15)?;
    stdout.flush()?;
    Ok(())
}
