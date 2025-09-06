#[cfg(feature = "ratatui-crossterm")]
use ratatui::crossterm;

use crate::{Input, InputRequest, StateChanged};
use crossterm::event::{
    Event as CrosstermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Attribute as CAttribute, Print, SetAttribute},
};
use std::io::{Result, Write};

/// Converts crossterm event into input requests.
pub fn to_input_request(evt: &CrosstermEvent) -> Option<InputRequest> {
    use InputRequest::*;
    use KeyCode::*;
    match evt {
        CrosstermEvent::Key(KeyEvent {
            code,
            modifiers,
            kind,
            state: _,
        }) if *kind == KeyEventKind::Press || *kind == KeyEventKind::Repeat => {
            match (*code, *modifiers) {
                (Backspace, KeyModifiers::NONE) | (Char('h'), KeyModifiers::CONTROL) => {
                    Some(DeletePrevChar)
                }
                (Delete, KeyModifiers::NONE) => Some(DeleteNextChar),
                (Tab, KeyModifiers::NONE) => None,
                (Left, KeyModifiers::NONE) | (Char('b'), KeyModifiers::CONTROL) => {
                    Some(GoToPrevChar)
                }
                (Left, KeyModifiers::CONTROL) | (Char('b'), KeyModifiers::META) => {
                    Some(GoToPrevWord)
                }
                (Right, KeyModifiers::NONE) | (Char('f'), KeyModifiers::CONTROL) => {
                    Some(GoToNextChar)
                }
                (Right, KeyModifiers::CONTROL) | (Char('f'), KeyModifiers::META) => {
                    Some(GoToNextWord)
                }
                (Char('u'), KeyModifiers::CONTROL) => Some(DeleteLine),

                (Char('w'), KeyModifiers::CONTROL)
                | (Char('d'), KeyModifiers::META)
                | (Backspace, KeyModifiers::META)
                | (Backspace, KeyModifiers::ALT) => Some(DeletePrevWord),

                (Delete, KeyModifiers::CONTROL) => Some(DeleteNextWord),
                (Char('k'), KeyModifiers::CONTROL) => Some(DeleteTillEnd),
                (Char('a'), KeyModifiers::CONTROL) | (Home, KeyModifiers::NONE) => {
                    Some(GoToStart)
                }
                (Char('e'), KeyModifiers::CONTROL) | (End, KeyModifiers::NONE) => {
                    Some(GoToEnd)
                }
                (Char(c), KeyModifiers::NONE) => Some(InsertChar(c)),
                (Char(c), KeyModifiers::SHIFT) => Some(InsertChar(c)),
                (_, _) => None,
            }
        }
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
    queue!(stdout, MoveTo(x, y), SetAttribute(CAttribute::NoReverse))?;

    let val_width = width.max(1) as usize - 1;
    let len = value.chars().count();
    let start = (len.max(val_width) - val_width).min(cursor);
    let mut chars = value.chars().skip(start);
    let mut i = start;

    // Chars before cursor
    while i < cursor {
        i += 1;
        let c = chars.next().unwrap_or(' ');
        queue!(stdout, Print(c))?;
    }

    // Cursor
    i += 1;
    let c = chars.next().unwrap_or(' ');
    queue!(
        stdout,
        SetAttribute(CAttribute::Reverse),
        Print(c),
        SetAttribute(CAttribute::NoReverse)
    )?;

    // Chars after the cursor
    while i <= start + val_width {
        i += 1;
        let c = chars.next().unwrap_or(' ');
        queue!(stdout, Print(c))?;
    }

    Ok(())
}

/// Import this trait to implement `Input::handle_event()` for crossterm.
pub trait EventHandler {
    /// Handle crossterm event.
    fn handle_event(&mut self, evt: &CrosstermEvent) -> Option<StateChanged>;
}

impl EventHandler for Input {
    /// Handle crossterm event.
    fn handle_event(&mut self, evt: &CrosstermEvent) -> Option<StateChanged> {
        to_input_request(evt).and_then(|req| self.handle(req))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
    };

    #[test]
    fn handle_tab() {
        let evt = Event::Key(KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });

        let req = to_input_request(&evt);

        assert!(req.is_none());
    }

    #[test]
    fn handle_repeat() {
        let evt = Event::Key(KeyEvent {
            code: KeyCode::Char('a'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Repeat,
            state: KeyEventState::NONE,
        });

        let req = to_input_request(&evt);

        assert_eq!(req, Some(InputRequest::InsertChar('a')));
    }
}
