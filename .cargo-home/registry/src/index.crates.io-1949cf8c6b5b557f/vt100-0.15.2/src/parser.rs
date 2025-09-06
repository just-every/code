/// A parser for terminal output which produces an in-memory representation of
/// the terminal contents.
pub struct Parser {
    parser: vte::Parser,
    screen: crate::screen::Screen,
}

impl Parser {
    /// Creates a new terminal parser of the given size and with the given
    /// amount of scrollback.
    #[must_use]
    pub fn new(rows: u16, cols: u16, scrollback_len: usize) -> Self {
        Self {
            parser: vte::Parser::new(),
            screen: crate::screen::Screen::new(
                crate::grid::Size { rows, cols },
                scrollback_len,
            ),
        }
    }

    /// Processes the contents of the given byte string, and updates the
    /// in-memory terminal state.
    pub fn process(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.parser.advance(&mut self.screen, *byte);
        }
    }

    /// Resizes the terminal.
    pub fn set_size(&mut self, rows: u16, cols: u16) {
        self.screen.set_size(rows, cols);
    }

    /// Scrolls to the given position in the scrollback.
    ///
    /// This position indicates the offset from the top of the screen, and
    /// should be `0` to put the normal screen in view.
    ///
    /// This affects the return values of methods called on `parser.screen()`:
    /// for instance, `parser.screen().cell(0, 0)` will return the top left
    /// corner of the screen after taking the scrollback offset into account.
    /// It does not affect `parser.process()` at all.
    ///
    /// The value given will be clamped to the actual size of the scrollback.
    pub fn set_scrollback(&mut self, rows: usize) {
        self.screen.set_scrollback(rows);
    }

    /// Returns a reference to a `Screen` object containing the terminal
    /// state.
    #[must_use]
    pub fn screen(&self) -> &crate::screen::Screen {
        &self.screen
    }
}

impl Default for Parser {
    /// Returns a parser with dimensions 80x24 and no scrollback.
    fn default() -> Self {
        Self::new(24, 80, 0)
    }
}

impl std::io::Write for Parser {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.process(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
