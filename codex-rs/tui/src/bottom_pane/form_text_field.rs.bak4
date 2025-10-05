use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::StatefulWidgetRef};
use std::cell::RefCell;

use super::textarea::{TextArea, TextAreaState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFilter {
    None,
    /// Allow only ASCII alphanumeric, '-' and '_'; disallow spaces and newlines
    Id,
}

/// Lightweight text field wrapper for bottom‑pane forms.
///
/// - Centralizes key handling (Shift‑modified chars, Enter/newlines, undo, nav)
/// - Renders via the same TextArea engine used by the chat composer, so
///   wrapping and height match exactly.
/// - Supports single‑line mode (ignores Enter pastes/newlines) and multi‑line
///   mode (Enter inserts a newline; paste preserves newlines).
#[derive(Debug)]
pub struct FormTextField {
    textarea: TextArea,
    state: RefCell<TextAreaState>,
    single_line: bool,
    filter: InputFilter,
}

impl FormTextField {
    pub fn new_single_line() -> Self {
        Self { textarea: TextArea::new(), state: RefCell::new(TextAreaState::default()), single_line: true, filter: InputFilter::None }
    }
    pub fn new_multi_line() -> Self {
        Self { textarea: TextArea::new(), state: RefCell::new(TextAreaState::default()), single_line: false, filter: InputFilter::None }
    }

    pub fn set_filter(&mut self, filter: InputFilter) { self.filter = filter; }

    fn id_char_allowed(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '-' || c == '_'
    }

    pub fn set_text(&mut self, text: &str) {
        if self.single_line {
            let mut t = text.replace('\r', "\n");
            t = t.replace('\n', " ");
            self.textarea.set_text(&t);
        } else {
            self.textarea.set_text(&text.replace('\r', "\n"));
        }
        self.textarea.set_cursor(self.textarea.text().len());
    }

    pub fn move_cursor_to_start(&mut self) {
        self.textarea.set_cursor(0);
    }

    // Intentionally no "move_cursor_to_end" to avoid unused-warn; add if needed.

    pub fn text(&self) -> &str { self.textarea.text() }

    pub fn cursor_is_at_start(&self) -> bool { self.textarea.cursor() == 0 }
    pub fn cursor_is_at_end(&self) -> bool { self.textarea.cursor() == self.textarea.text().len() }

    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // For single-line inputs, swallow Enter (treat as no-op here; the form
        // can decide to move focus or trigger an action). Also convert any
        // Control-Enter to newline only in multi-line mode.
        match key.code {
            KeyCode::Enter if self.single_line => return true, // consumed
            _ => {}
        }

        // Block Alt-modified printable chars from being inserted as text since
        // many terminals map Option/Meta to Alt for word navigation shortcuts.
        if matches!(key.code, KeyCode::Char(_))
            && key.modifiers.contains(KeyModifiers::ALT)
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            return false; // let parent handle (e.g., form navigation)
        }

        // If an input filter is active and this is a Char, validate and insert
        if let KeyCode::Char(c) = key.code {
            if matches!(self.filter, InputFilter::Id) {
                if Self::id_char_allowed(c) {
                    self.textarea.insert_str(&c.to_string());
                }
                return true; // consumed either way
            }
        }

        // Delegate remaining keys to TextArea which already handles:
        // - Shift-modified chars
        // - Enter/newline (multi-line only)
        // - Undo (Ctrl+Z), word nav, Home/End, etc.
        self.textarea.input(key);
        true
    }

    pub fn handle_paste(&mut self, mut pasted: String) {
        pasted = pasted.replace('\r', "\n");
        // Remove newlines entirely for ID filter; otherwise normalize
        match self.filter {
            InputFilter::Id => {
                let filtered: String = pasted
                    .chars()
                    .filter(|&c| Self::id_char_allowed(c))
                    .collect();
                if !filtered.is_empty() { self.textarea.insert_str(&filtered); }
            }
            InputFilter::None => {
                if self.single_line {
                    pasted = pasted.replace('\n', " ");
                }
                self.textarea.insert_str(&pasted);
            }
        }
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        if self.single_line { 1 } else { self.textarea.desired_height(width).max(1) }
    }

    /// Render the field text within `area`. When `focused` is true, draw a thin
    /// caret marker at the logical cursor position (the real terminal cursor is
    /// hidden while overlays are active).
    pub fn render(&self, area: Rect, buf: &mut Buffer, focused: bool) {
        // Paint text using the TextArea renderer for exact wrapping
        let mut state = self.state.borrow().clone();
        StatefulWidgetRef::render_ref(&(&self.textarea), area, buf, &mut state);
        // Persist any scroll changes made during rendering
        *self.state.borrow_mut() = state;

        // Draw a pseudo-caret when focused without hiding the underlying glyph.
        // Invert colors on the cursor cell so the character remains visible.
        if focused {
            if let Some((cx, cy)) = self
                .textarea
                .cursor_pos_with_state(area, *self.state.borrow())
            {
                let max_x = area.x.saturating_add(area.width.saturating_sub(1));
                let x = cx.min(max_x);
                if cy >= area.y
                    && cy < area.y + area.height
                    && x >= area.x
                    && x < area.x + area.width
                {
                    let style = Style::default()
                        .bg(crate::colors::text())
                        .fg(crate::colors::background());
                    buf[(x, cy)].set_style(style);
                }
            }
        }
        // Note: We intentionally do not persist state.scroll changes from a
        // read-only &self; form owns a &mut when calling this in practice.
    }

    // Note: a mutable render variant previously existed to persist internal scroll
    // state during form rendering. It was unused across the workspace, so it was
    // removed to keep builds warning‑free per repo policy. If a future caller
    // needs to preserve state, it can call `render` after cloning and restoring
    // the state across frames.
}
