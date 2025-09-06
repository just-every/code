#[cfg(feature = "ratatui-termion")]
use ratatui::termion;

use crate::input::InputRequest;
use crate::Input;
use crate::StateChanged;
use std::io::{Result, Write};
use termion::cursor::Goto;
use termion::event::{Event, Key};
use termion::style::Invert;
use termion::style::NoInvert;

/// Converts termion event into input requests.
pub fn to_input_request(evt: &Event) -> Option<InputRequest> {
    use InputRequest::*;
    match *evt {
        Event::Key(Key::Backspace) | Event::Key(Key::Ctrl('h')) => Some(DeletePrevChar),
        Event::Key(Key::Delete) => Some(DeleteNextChar),
        Event::Key(Key::Left) | Event::Key(Key::Ctrl('b')) => Some(GoToPrevChar),
        Event::Key(Key::Right) | Event::Key(Key::Ctrl('f')) => Some(GoToNextChar),
        // Event::Key(Key::Ctrl(Key::Left)) => Some(GoToPrevWord),
        // Event::Key(Key::Ctrl(Key::Right)) => Some(GoToNextWord),
        Event::Key(Key::Ctrl('u')) => Some(DeleteLine),
        Event::Key(Key::Ctrl('w')) => Some(DeletePrevWord),
        // Event::Key(Key::Ctrl(Key::Delete)) => Some(DeleteNextWord),
        Event::Key(Key::Ctrl('a')) | Event::Key(Key::Home) => Some(GoToStart),
        Event::Key(Key::Ctrl('e')) | Event::Key(Key::End) => Some(GoToEnd),
        Event::Key(Key::Char('\t')) => None,
        Event::Key(Key::Char(c)) => Some(InsertChar(c)),
        _ => None,
    }
}

/// Renders the input UI at the given position with the given width.
pub fn write<W: Write>(
    stdout: &mut W,
    value: &str,
    cursor: usize,
    (x, y): (u16, u16),
    width: u16,
) -> Result<()> {
    write!(stdout, "{}{}", Goto(x + 1, y + 1), NoInvert)?;

    let val_width = width.max(1) as usize - 1;
    let len = value.chars().count();
    let start = (len.max(val_width) - val_width).min(cursor);
    let mut chars = value.chars().skip(start);
    let mut i = start;

    // Chars before cursor
    while i < cursor {
        i += 1;
        let c = chars.next().unwrap_or(' ');
        write!(stdout, "{}", c)?;
    }

    // Cursor
    i += 1;
    let c = chars.next().unwrap_or(' ');
    write!(stdout, "{}{}{}", Invert, c, NoInvert,)?;

    // Chars after the cursor
    while i <= start + val_width {
        i += 1;
        let c = chars.next().unwrap_or(' ');
        write!(stdout, "{}", c)?;
    }

    Ok(())
}

/// Import this trait to implement `Input::handle_event()` for termion.
pub trait EventHandler {
    /// Handle termion event.
    fn handle_event(&mut self, evt: &Event) -> Option<StateChanged>;
}

impl EventHandler for Input {
    /// Handle termion event.
    fn handle_event(&mut self, evt: &Event) -> Option<StateChanged> {
        to_input_request(evt).and_then(|req| self.handle(req))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_tab() {
        let evt = Event::Key(Key::Char('\t'));

        let req = to_input_request(&evt);

        assert!(req.is_none());
    }
}
