use crate::util::buffer::fill_rect;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::StatefulWidgetRef;
use ratatui::widgets::WidgetRef;
use std::cell::Ref;
use std::cell::RefCell;
use std::ops::Range;
use textwrap::Options;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

#[derive(Debug)]
pub(crate) struct TextArea {
    text: String,
    cursor_pos: usize,
    wrap_cache: RefCell<Option<WrapCache>>,
    preferred_col: Option<usize>,
    // Simple undo stack capturing full snapshots of text and cursor before edits.
    // This is intentionally simple to reliably undo paste and bulk edits across terminals.
    undo_stack: Vec<UndoSnapshot>,
}

#[derive(Debug, Clone)]
struct WrapCache {
    width: u16,
    lines: Vec<Range<usize>>,
}

#[derive(Debug, Clone)]
struct UndoSnapshot {
    text: String,
    cursor_pos: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct TextAreaState {
    /// Index into wrapped lines of the first visible line.
    scroll: u16,
}

impl TextArea {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor_pos: 0,
            wrap_cache: RefCell::new(None),
            preferred_col: None,
            undo_stack: Vec::new(),
        }
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor_pos = self.cursor_pos.clamp(0, self.text.len());
        self.wrap_cache.replace(None);
        self.preferred_col = None;
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn insert_str(&mut self, text: &str) {
        self.insert_str_at(self.cursor_pos, text);
    }

    pub fn insert_str_at(&mut self, pos: usize, text: &str) {
        self.push_undo_snapshot();
        let pos = self.clamp_pos_for_insertion(pos);
        self.text.insert_str(pos, text);
        self.wrap_cache.replace(None);
        if pos <= self.cursor_pos {
            self.cursor_pos += text.len();
        }
        self.shift_elements(pos, 0, text.len());
        self.preferred_col = None;
    }

    pub fn replace_range(&mut self, range: std::ops::Range<usize>, text: &str) {
        let range = self.expand_range_to_element_boundaries(range);
        self.replace_range_raw(range, text);
    }

    fn replace_range_raw(&mut self, range: std::ops::Range<usize>, text: &str) {
        assert!(range.start <= range.end);
        self.push_undo_snapshot();
        let start = range.start.clamp(0, self.text.len());
        let end = range.end.clamp(0, self.text.len());
        let removed_len = end - start;
        let inserted_len = text.len();
        if removed_len == 0 && inserted_len == 0 {
            return;
        }
        let diff = inserted_len as isize - removed_len as isize;

        self.text.replace_range(range, text);
        self.wrap_cache.replace(None);
        self.preferred_col = None;
        self.update_elements_after_replace(start, end, inserted_len);

        // Update the cursor position to account for the edit.
        self.cursor_pos = if self.cursor_pos < start {
            // Cursor was before the edited range – no shift.
            self.cursor_pos
        } else if self.cursor_pos <= end {
            // Cursor was inside the replaced range – move to end of the new text.
            start + inserted_len
        } else {
            // Cursor was after the replaced range – shift by the length diff.
            ((self.cursor_pos as isize) + diff) as usize
        }
        .min(self.text.len());

        // Ensure cursor is not inside an element
        self.cursor_pos = self.clamp_pos_to_nearest_boundary(self.cursor_pos);
    }

    pub fn cursor(&self) -> usize {
        self.cursor_pos
    }

    pub fn set_cursor(&mut self, pos: usize) {
        self.cursor_pos = pos.clamp(0, self.text.len());
        self.cursor_pos = self.clamp_pos_to_nearest_boundary(self.cursor_pos);
        self.preferred_col = None;
    }

    pub fn desired_height(&self, width: u16) -> u16 {
        self.wrapped_lines(width).len() as u16
    }

    #[allow(dead_code)]
    pub fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        self.cursor_pos_with_state(area, TextAreaState::default())
    }

    /// Compute the on-screen cursor position taking scrolling into account.
    pub fn cursor_pos_with_state(&self, area: Rect, state: TextAreaState) -> Option<(u16, u16)> {
        let lines = self.wrapped_lines(area.width);
        let effective_scroll = self.effective_scroll(area.height, &lines, state.scroll);
        let i = Self::wrapped_line_index_by_start(&lines, self.cursor_pos)?;
        let ls = &lines[i];
        let col = self.text[ls.start..self.cursor_pos].width() as u16;
        let screen_row = i
            .saturating_sub(effective_scroll as usize)
            .try_into()
            .unwrap_or(0);
        Some((area.x + col, area.y + screen_row))
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn current_display_col(&self) -> usize {
        let bol = self.beginning_of_current_line();
        self.text[bol..self.cursor_pos].width()
    }

    fn wrapped_line_index_by_start(lines: &[Range<usize>], pos: usize) -> Option<usize> {
        // partition_point returns the index of the first element for which
        // the predicate is false, i.e. the count of elements with start <= pos.
        let idx = lines.partition_point(|r| r.start <= pos);
        if idx == 0 { None } else { Some(idx - 1) }
    }

    fn move_to_display_col_on_line(
        &mut self,
        line_start: usize,
        line_end: usize,
        target_col: usize,
    ) {
        let mut width_so_far = 0usize;
        for (i, g) in self.text[line_start..line_end].grapheme_indices(true) {
            width_so_far += g.width();
            if width_so_far > target_col {
                self.cursor_pos = line_start + i;
                // Avoid landing inside an element; round to nearest boundary
                self.cursor_pos = self.clamp_pos_to_nearest_boundary(self.cursor_pos);
                return;
            }
        }
        self.cursor_pos = line_end;
        self.cursor_pos = self.clamp_pos_to_nearest_boundary(self.cursor_pos);
    }

    fn beginning_of_line(&self, pos: usize) -> usize {
        self.text[..pos].rfind('\n').map(|i| i + 1).unwrap_or(0)
    }
    fn beginning_of_current_line(&self) -> usize {
        self.beginning_of_line(self.cursor_pos)
    }

    fn end_of_line(&self, pos: usize) -> usize {
        self.text[pos..]
            .find('\n')
            .map(|i| i + pos)
            .unwrap_or(self.text.len())
    }
    fn end_of_current_line(&self) -> usize {
        self.end_of_line(self.cursor_pos)
    }

    pub fn input(&mut self, event: KeyEvent) {
        // On Windows consoles and some terminals with keyboard enhancement flags,
        // crossterm reports both Press and Repeat (and sometimes Release) for
        // printable keys. Processing all kinds causes doubled characters.
        //
        // Policy:
        // - Always ignore Release events.
        // - For Repeat, only allow navigation and deletion keys to auto-repeat;
        //   ignore Repeat for character insertion and Enter so typing does not double.
        match event.kind {
            KeyEventKind::Release => return,
            KeyEventKind::Repeat => match event.code {
                KeyCode::Left
                | KeyCode::Right
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::Backspace
                | KeyCode::Delete => { /* allow auto‑repeat */ }
                _ => return,
            },
            KeyEventKind::Press => { /* handle below */ }
        }
        match event {
            // Some terminals (or configurations) send Control key chords as
            // C0 control characters without reporting the CONTROL modifier.
            // Handle common fallbacks for Ctrl-B/Ctrl-F here so they don't get
            // inserted as literal control bytes.
            KeyEvent { code: KeyCode::Char('\u{0002}'), modifiers: KeyModifiers::NONE, .. } /* ^B */ => {
                self.move_cursor_left();
            }
            KeyEvent { code: KeyCode::Char('\u{0006}'), modifiers: KeyModifiers::NONE, .. } /* ^F */ => {
                self.move_cursor_right();
            }
            KeyEvent {
                code: KeyCode::Char(c),
                ..
            } if matches!(c, '\n' | '\r') => self.insert_str("\n"),
            #[cfg(target_os = "windows")]
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                ..
            } if (modifiers == (KeyModifiers::CONTROL | KeyModifiers::ALT)
                    || modifiers == (KeyModifiers::CONTROL
                        | KeyModifiers::ALT
                        | KeyModifiers::SHIFT))
                && !c.is_ascii_control()
                && !c.is_ascii_alphabetic() =>
            {
                self.insert_str(&c.to_string());
            }
            KeyEvent {
                code: KeyCode::Char(c),
                // Insert plain characters (and Shift-modified). Do NOT insert when ALT is held,
                // because many terminals map Option/Meta combos to ALT+<char> (e.g. ESC f/ESC b)
                // for word navigation. Those are handled explicitly below.
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } => self.insert_str(&c.to_string()),
            KeyEvent {
                code: KeyCode::Char('j' | 'm'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
            | KeyEvent {
                code: KeyCode::Enter,
                ..
            } => self.insert_str("\n"),
            KeyEvent {
                code: KeyCode::Char('h'),
                modifiers,
                ..
            } if modifiers == (KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.delete_backward_word()
            },
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                // Map Ctrl+Backspace to delete the entire current line contents.
                // This provides a portable shortcut similar to macOS Cmd+Backspace
                // for terminals that do not forward the Command (SUPER) modifier.
                self.delete_entire_line();
            }
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::ALT,
                ..
            } => self.delete_backward_word(),
            KeyEvent {
                code: KeyCode::Backspace,
                modifiers: KeyModifiers::NONE,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('h'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => self.delete_backward(1),
            KeyEvent { code: KeyCode::Delete, modifiers, .. }
                if modifiers.contains(KeyModifiers::SUPER)
                    && !modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) =>
            {
                self.delete_entire_line();
            }
            // Option+Fn+Delete (Forward Delete on macOS) arrives as ALT+Delete. Keep
            // it deleting the word ahead of the cursor so we do not regress the
            // default macOS terminal behavior. Option+Backspace already covers the
            // "delete previous word" shortcut exposed in Warp.
            KeyEvent { code: KeyCode::Delete, modifiers, .. }
                if modifiers.contains(KeyModifiers::ALT) =>
            {
                self.delete_forward_word();
            }
            KeyEvent {
                code: KeyCode::Delete,
                ..
            }
            | KeyEvent {
                code: KeyCode::Char('d'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => self.delete_forward(1),

            KeyEvent {
                code: KeyCode::Char('w'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.delete_backward_word();
            }
            // Undo: Ctrl+Z reverts the last edit snapshot (paste, insert, delete, etc.).
            KeyEvent {
                code: KeyCode::Char('z'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.undo();
            }
            KeyEvent { code: KeyCode::Char('z'), modifiers, .. }
                if modifiers.contains(KeyModifiers::SUPER)
                    && !modifiers.intersects(KeyModifiers::ALT | KeyModifiers::SHIFT) =>
            {
                self.undo();
            }
            // macOS-like shortcuts (when terminals report the Command key as SUPER):
            // Cmd+Left  -> move to beginning of line
            // Cmd+Right -> move to end of line
            // Cmd+Backspace / Cmd+Delete -> delete entire current line contents
            KeyEvent { code: KeyCode::Left, modifiers, .. }
                if modifiers.contains(KeyModifiers::SUPER)
                    && !modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) =>
            {
                self.move_cursor_to_beginning_of_line(false);
            }
            KeyEvent { code: KeyCode::Right, modifiers, .. }
                if modifiers.contains(KeyModifiers::SUPER)
                    && !modifiers.intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) =>
            {
                self.move_cursor_to_end_of_line(false);
            }
            KeyEvent { code: KeyCode::Backspace, modifiers, .. }
                if modifiers.contains(KeyModifiers::SUPER)
                    && !modifiers.contains(KeyModifiers::ALT) =>
            {
                self.delete_entire_line();
            }
            // Meta-b -> move to beginning of previous word
            // Meta-f -> move to end of next word
            // Many terminals map Option (macOS) to Alt. Some send Alt|Shift, so match contains(ALT).
            KeyEvent {
                code: KeyCode::Char('b'),
                modifiers: KeyModifiers::ALT,
                ..
            } => {
                self.set_cursor(self.beginning_of_previous_word());
            }
            KeyEvent {
                code: KeyCode::Char('f'),
                modifiers: KeyModifiers::ALT,
                ..
            } => {
                self.set_cursor(self.end_of_next_word());
            }
            KeyEvent {
                code: KeyCode::Char('u'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.kill_to_beginning_of_line();
            }
            KeyEvent {
                code: KeyCode::Char('k'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.kill_to_end_of_line();
            }

            // Cursor movement
            KeyEvent { code: KeyCode::Left, modifiers, .. }
                if modifiers == KeyModifiers::NONE || modifiers == KeyModifiers::SHIFT =>
            {
                self.move_cursor_left();
            }
            KeyEvent { code: KeyCode::Right, modifiers, .. }
                if modifiers == KeyModifiers::NONE || modifiers == KeyModifiers::SHIFT =>
            {
                self.move_cursor_right();
            }
            KeyEvent {
                code: KeyCode::Char('b'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.move_cursor_left();
            }
            KeyEvent {
                code: KeyCode::Char('f'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.move_cursor_right();
            }
            // Some terminals send Alt+Arrow for word-wise movement:
            // Option/Left -> Alt+Left (previous word start)
            // Option/Right -> Alt+Right (next word end)
            KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::ALT,
                ..
            }
            | KeyEvent {
                code: KeyCode::Left,
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.set_cursor(self.beginning_of_previous_word());
            }
            KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::ALT,
                ..
            }
            | KeyEvent {
                code: KeyCode::Right,
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.set_cursor(self.end_of_next_word());
            }
            KeyEvent {
                code: KeyCode::Up, ..
            } => {
                self.move_cursor_up();
            }
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => {
                self.move_cursor_down();
            }
            KeyEvent {
                code: KeyCode::Home,
                ..
            } => {
                self.move_cursor_to_beginning_of_line(false);
            }
            KeyEvent {
                code: KeyCode::Char('a'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.move_cursor_to_beginning_of_line(true);
            }

            KeyEvent {
                code: KeyCode::End, ..
            } => {
                self.move_cursor_to_end_of_line(false);
            }
            KeyEvent {
                code: KeyCode::Char('e'),
                modifiers: KeyModifiers::CONTROL,
                ..
            } => {
                self.move_cursor_to_end_of_line(true);
            }
            _o => {
                #[cfg(feature = "debug-logs")]
                tracing::debug!("Unhandled key event in TextArea: {:?}", _o);
            }
        }
    }

    // ####### Input Functions #######
    pub fn delete_backward(&mut self, n: usize) {
        if n == 0 || self.cursor_pos == 0 {
            return;
        }
        let mut target = self.cursor_pos;
        for _ in 0..n {
            target = self.prev_atomic_boundary(target);
            if target == 0 {
                break;
            }
        }
        self.replace_range(target..self.cursor_pos, "");
    }

    pub fn delete_forward(&mut self, n: usize) {
        if n == 0 || self.cursor_pos >= self.text.len() {
            return;
        }
        let mut target = self.cursor_pos;
        for _ in 0..n {
            target = self.next_atomic_boundary(target);
            if target >= self.text.len() {
                break;
            }
        }
        self.replace_range(self.cursor_pos..target, "");
    }

    pub fn delete_backward_word(&mut self) {
        let start = self.beginning_of_previous_word();
        self.replace_range(start..self.cursor_pos, "");
    }

    /// Delete text to the right of the cursor using "word" semantics.
    ///
    /// Deletes from the current cursor position through the end of the next word as determined
    /// by `end_of_next_word()`. Any whitespace (including newlines) between the cursor and that
    /// word is included in the deletion.
    pub fn delete_forward_word(&mut self) {
        let end = self.end_of_next_word();
        if end > self.cursor_pos {
            self.replace_range(self.cursor_pos..end, "");
        }
    }

    pub fn kill_to_end_of_line(&mut self) {
        let eol = self.end_of_current_line();
        if self.cursor_pos == eol {
            if eol < self.text.len() {
                self.replace_range(self.cursor_pos..eol + 1, "");
            }
        } else {
            self.replace_range(self.cursor_pos..eol, "");
        }
    }

    pub fn kill_to_beginning_of_line(&mut self) {
        let bol = self.beginning_of_current_line();
        if self.cursor_pos == bol {
            if bol > 0 {
                self.replace_range(bol - 1..bol, "");
            }
        } else {
            self.replace_range(bol..self.cursor_pos, "");
        }
    }

    /// Delete the entire contents of the current logical line, preserving the newline.
    /// Leaves the cursor at the beginning of the line.
    pub fn delete_entire_line(&mut self) {
        let bol = self.beginning_of_current_line();
        let eol = self.end_of_current_line();
        if bol < eol {
            self.replace_range(bol..eol, "");
        }
        self.set_cursor(bol);
    }

    /// Revert the most recent edit (if any).
    pub fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            self.text = prev.text;
            self.cursor_pos = prev.cursor_pos.min(self.text.len());
            self.wrap_cache.replace(None);
            self.preferred_col = None;
        }
    }

    /// Move the cursor left by a single grapheme cluster.
    pub fn move_cursor_left(&mut self) {
        self.cursor_pos = self.prev_atomic_boundary(self.cursor_pos);
        self.preferred_col = None;
    }

    /// Move the cursor right by a single grapheme cluster.
    pub fn move_cursor_right(&mut self) {
        self.cursor_pos = self.next_atomic_boundary(self.cursor_pos);
        self.preferred_col = None;
    }

    pub fn move_cursor_up(&mut self) {
        // If we have a wrapping cache, prefer navigating across wrapped (visual) lines.
        if let Some((target_col, maybe_line)) = {
            let cache_ref = self.wrap_cache.borrow();
            if let Some(cache) = cache_ref.as_ref() {
                let lines = &cache.lines;
                if let Some(idx) = Self::wrapped_line_index_by_start(lines, self.cursor_pos) {
                    let cur_range = &lines[idx];
                    let target_col = self
                        .preferred_col
                        .unwrap_or_else(|| self.text[cur_range.start..self.cursor_pos].width());
                    if idx > 0 {
                        let prev = &lines[idx - 1];
                        let line_start = prev.start;
                        let line_end = prev.end.saturating_sub(1);
                        Some((target_col, Some((line_start, line_end))))
                    } else {
                        Some((target_col, None))
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } {
            // We had wrapping info. Apply movement accordingly.
            match maybe_line {
                Some((line_start, line_end)) => {
                    if self.preferred_col.is_none() {
                        self.preferred_col = Some(target_col);
                    }
                    self.move_to_display_col_on_line(line_start, line_end, target_col);
                    return;
                }
                None => {
                    // Already at first visual line -> move to start
                    self.cursor_pos = 0;
                    self.preferred_col = None;
                    return;
                }
            }
        }

        // Fallback to logical line navigation if we don't have wrapping info yet.
        if let Some(prev_nl) = self.text[..self.cursor_pos].rfind('\n') {
            let target_col = match self.preferred_col {
                Some(c) => c,
                None => {
                    let c = self.current_display_col();
                    self.preferred_col = Some(c);
                    c
                }
            };
            let prev_line_start = self.text[..prev_nl].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let prev_line_end = prev_nl;
            self.move_to_display_col_on_line(prev_line_start, prev_line_end, target_col);
        } else {
            self.cursor_pos = 0;
            self.preferred_col = None;
        }
    }

    pub fn move_cursor_down(&mut self) {
        // If we have a wrapping cache, prefer navigating across wrapped (visual) lines.
        if let Some((target_col, move_to_last)) = {
            let cache_ref = self.wrap_cache.borrow();
            if let Some(cache) = cache_ref.as_ref() {
                let lines = &cache.lines;
                if let Some(idx) = Self::wrapped_line_index_by_start(lines, self.cursor_pos) {
                    let cur_range = &lines[idx];
                    let target_col = self
                        .preferred_col
                        .unwrap_or_else(|| self.text[cur_range.start..self.cursor_pos].width());
                    if idx + 1 < lines.len() {
                        let next = &lines[idx + 1];
                        let line_start = next.start;
                        let line_end = next.end.saturating_sub(1);
                        Some((target_col, Some((line_start, line_end))))
                    } else {
                        Some((target_col, None))
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } {
            match move_to_last {
                Some((line_start, line_end)) => {
                    if self.preferred_col.is_none() {
                        self.preferred_col = Some(target_col);
                    }
                    self.move_to_display_col_on_line(line_start, line_end, target_col);
                    return;
                }
                None => {
                    // Already on last visual line -> move to end
                    self.cursor_pos = self.text.len();
                    self.preferred_col = None;
                    return;
                }
            }
        }

        // Fallback to logical line navigation if we don't have wrapping info yet.
        let target_col = match self.preferred_col {
            Some(c) => c,
            None => {
                let c = self.current_display_col();
                self.preferred_col = Some(c);
                c
            }
        };
        if let Some(next_nl) = self.text[self.cursor_pos..]
            .find('\n')
            .map(|i| i + self.cursor_pos)
        {
            let next_line_start = next_nl + 1;
            let next_line_end = self.text[next_line_start..]
                .find('\n')
                .map(|i| i + next_line_start)
                .unwrap_or(self.text.len());
            self.move_to_display_col_on_line(next_line_start, next_line_end, target_col);
        } else {
            self.cursor_pos = self.text.len();
            self.preferred_col = None;
        }
    }

    pub fn move_cursor_to_beginning_of_line(&mut self, move_up_at_bol: bool) {
        let bol = self.beginning_of_current_line();
        if move_up_at_bol && self.cursor_pos == bol {
            self.set_cursor(self.beginning_of_line(self.cursor_pos.saturating_sub(1)));
        } else {
            self.set_cursor(bol);
        }
        self.preferred_col = None;
    }

    pub fn move_cursor_to_end_of_line(&mut self, move_down_at_eol: bool) {
        let eol = self.end_of_current_line();
        if move_down_at_eol && self.cursor_pos == eol {
            let next_pos = (self.cursor_pos.saturating_add(1)).min(self.text.len());
            self.set_cursor(self.end_of_line(next_pos));
        } else {
            self.set_cursor(eol);
        }
    }



    fn clamp_pos_to_nearest_boundary(&self, mut pos: usize) -> usize {
        if pos > self.text.len() {
            pos = self.text.len();
        }
        pos
    }

    fn clamp_pos_for_insertion(&self, pos: usize) -> usize {
        pos
    }

    fn expand_range_to_element_boundaries(&self, range: Range<usize>) -> Range<usize> {
        range
    }

    fn shift_elements(&mut self, _at: usize, _removed: usize, _inserted: usize) {
        // No-op: elements feature removed
    }

    fn update_elements_after_replace(&mut self, start: usize, end: usize, inserted_len: usize) {
        self.shift_elements(start, end.saturating_sub(start), inserted_len);
    }

    fn prev_atomic_boundary(&self, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }
        let mut gc = unicode_segmentation::GraphemeCursor::new(pos, self.text.len(), false);
        match gc.prev_boundary(&self.text, 0) {
            Ok(Some(b)) => b,
            Ok(None) => 0,
            Err(_) => pos.saturating_sub(1),
        }
    }

    fn next_atomic_boundary(&self, pos: usize) -> usize {
        if pos >= self.text.len() {
            return self.text.len();
        }
        let mut gc = unicode_segmentation::GraphemeCursor::new(pos, self.text.len(), false);
        match gc.next_boundary(&self.text, 0) {
            Ok(Some(b)) => b,
            Ok(None) => self.text.len(),
            Err(_) => pos.saturating_add(1),
        }
    }

    fn push_undo_snapshot(&mut self) {
        // Capture current state before mutation.
        self.undo_stack.push(UndoSnapshot {
            text: self.text.clone(),
            cursor_pos: self.cursor_pos,
        });
        // Keep a bounded history to avoid unbounded memory growth.
        const MAX_UNDO_SNAPSHOTS: usize = 128;
        if self.undo_stack.len() > MAX_UNDO_SNAPSHOTS {
            let excess = self.undo_stack.len() - MAX_UNDO_SNAPSHOTS;
            self.undo_stack.drain(0..excess);
        }
    }

    pub(crate) fn beginning_of_previous_word(&self) -> usize {
        if let Some(first_non_ws) = self.text[..self.cursor_pos].rfind(|c: char| !c.is_whitespace())
        {
            self.text[..first_non_ws]
                .rfind(|c: char| c.is_whitespace())
                .map(|i| i + self.text[i..].chars().next().unwrap().len_utf8())
                .unwrap_or(0)
        } else {
            0
        }
    }

    pub(crate) fn end_of_next_word(&self) -> usize {
        let Some(first_non_ws) = self.text[self.cursor_pos..].find(|c: char| !c.is_whitespace())
        else {
            return self.text.len();
        };
        let word_start = self.cursor_pos + first_non_ws;
        match self.text[word_start..].find(|c: char| c.is_whitespace()) {
            Some(rel_idx) => word_start + rel_idx,
            None => self.text.len(),
        }
    }


    #[allow(clippy::unwrap_used)]
    fn wrapped_lines(&self, width: u16) -> Ref<'_, Vec<Range<usize>>> {
        // Ensure cache is ready (potentially mutably borrow, then drop)
        {
            let mut cache = self.wrap_cache.borrow_mut();
            let needs_recalc = match cache.as_ref() {
                Some(c) => c.width != width,
                None => true,
            };
            if needs_recalc {
                let mut lines: Vec<Range<usize>> = Vec::new();
                // Wrap one column earlier to avoid the caret visually overlapping
                // the right border before a newline is committed. This makes the
                // input feel more natural: when the cursor reaches the last
                // visible column, the next character starts on the next line.
                let effective_width = width.saturating_sub(1);
                for line in textwrap::wrap(
                    &self.text,
                    Options::new(effective_width as usize)
                        .wrap_algorithm(textwrap::WrapAlgorithm::FirstFit),
                )
                .iter()
                {
                    match line {
                        std::borrow::Cow::Borrowed(slice) => {
                            let start =
                                unsafe { slice.as_ptr().offset_from(self.text.as_ptr()) as usize };
                            let end = start + slice.len();
                            let trailing_spaces =
                                self.text[end..].chars().take_while(|c| *c == ' ').count();
                            lines.push(start..end + trailing_spaces + 1);
                        }
                        std::borrow::Cow::Owned(_) => unreachable!(),
                    }
                }
                *cache = Some(WrapCache { width, lines });
            }
        }

        let cache = self.wrap_cache.borrow();
        Ref::map(cache, |c| &c.as_ref().unwrap().lines)
    }

    /// Calculate the scroll offset that should be used to satisfy the
    /// invariants given the current area size and wrapped lines.
    ///
    /// - Cursor is always on screen.
    /// - No scrolling if content fits in the area.
    fn effective_scroll(
        &self,
        area_height: u16,
        lines: &[Range<usize>],
        current_scroll: u16,
    ) -> u16 {
        let total_lines = lines.len() as u16;
        if area_height >= total_lines {
            return 0;
        }

        // Where is the cursor within wrapped lines? Prefer assigning boundary positions
        // (where pos equals the start of a wrapped line) to that later line.
        let cursor_line_idx =
            Self::wrapped_line_index_by_start(lines, self.cursor_pos).unwrap_or(0) as u16;

        let max_scroll = total_lines.saturating_sub(area_height);
        let mut scroll = current_scroll.min(max_scroll);

        // Ensure cursor is visible within [scroll, scroll + area_height)
        if cursor_line_idx < scroll {
            scroll = cursor_line_idx;
        } else if cursor_line_idx >= scroll + area_height {
            scroll = cursor_line_idx + 1 - area_height;
        }
        scroll
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[cfg(target_os = "windows")]
    #[test]
    fn altgr_control_alt_characters_insert_printables() {
        let mut textarea = TextArea::new();
        let cases = [
            ('/', KeyModifiers::CONTROL | KeyModifiers::ALT),
            ('@', KeyModifiers::CONTROL | KeyModifiers::ALT),
            ('{', KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT),
        ];

        for (ch, modifiers) in cases {
            textarea.set_text("");
            textarea.set_cursor(0);
            textarea.input(KeyEvent::new(KeyCode::Char(ch), modifiers));
            assert_eq!(textarea.text(), ch.to_string(), "expected AltGr combination to insert {ch}");
        }
    }

    #[test]
    fn ctrl_alt_letter_shortcut_preserved() {
        let mut textarea = TextArea::new();
        textarea.set_text("word");
        textarea.set_cursor(textarea.text().len());

        textarea.input(KeyEvent::new(
            KeyCode::Char('h'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ));

        assert_eq!(textarea.text(), "", "Ctrl+Alt+H should still delete backward word");
    }

    #[test]
    fn cmd_z_with_extra_control_bit_undoes() {
        let mut textarea = TextArea::new();
        textarea.insert_str("hello");
        assert_eq!(textarea.text(), "hello");

        textarea.input(KeyEvent::new(
            KeyCode::Char('z'),
            KeyModifiers::SUPER | KeyModifiers::CONTROL,
        ));

        assert_eq!(textarea.text(), "");
        assert_eq!(textarea.cursor(), 0);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn ctrl_alt_symbol_shortcut_is_ignored_for_text_insertion() {
        let mut textarea = TextArea::new();
        textarea.set_text("");
        textarea.set_cursor(0);

        textarea.input(KeyEvent::new(
            KeyCode::Char('@'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ));

        assert!(
            textarea.text().is_empty(),
            "Ctrl+Alt symbol should not insert printable characters on non-Windows"
        );
    }

    #[test]
    fn alt_left_keeps_cursor_on_char_boundary_with_multibyte_space() {
        let mut textarea = TextArea::new();
        let text = "alpha\u{202f}beta".to_string();
        textarea.set_text(&text);
        textarea.set_cursor(text.len());

        textarea.input(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT));

        let cursor = textarea.cursor();
        assert!(
            textarea.text().is_char_boundary(cursor),
            "cursor should stay on a character boundary"
        );
        let ws_index = textarea.text().find('\u{202f}').expect("whitespace");
        let expected = ws_index + '\u{202f}'.len_utf8();
        assert_eq!(cursor, expected, "expected cursor to land after whitespace");
    }
}

impl WidgetRef for &TextArea {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        let lines = self.wrapped_lines(area.width);
        self.render_lines(area, buf, &lines, 0..lines.len());
    }
}

impl StatefulWidgetRef for &TextArea {
    type State = TextAreaState;

    fn render_ref(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        let lines = self.wrapped_lines(area.width);
        let scroll = self.effective_scroll(area.height, &lines, state.scroll);
        state.scroll = scroll;

        let start = scroll as usize;
        let end = (scroll + area.height).min(lines.len() as u16) as usize;
        self.render_lines(area, buf, &lines, start..end);
    }
}

impl TextArea {
    fn render_lines(
        &self,
        area: Rect,
        buf: &mut Buffer,
        lines: &[Range<usize>],
        range: std::ops::Range<usize>,
    ) {
        let bg = crate::colors::background();
        let fg = crate::colors::text();
        let line_style = Style::default().bg(bg).fg(fg);
        for (row, idx) in range.enumerate() {
            let r = &lines[idx];
            let y = area.y + row as u16;
            // Paint the row background in one pass to avoid per-cell index math.
            fill_rect(buf, Rect::new(area.x, y, area.width, 1), None, line_style);

            // Draw the text on top using theme foreground + background to preserve consistent look.
            if r.end > r.start {
                let line_range = r.start..r.end - 1;
                buf.set_string(area.x, y, &self.text[line_range], line_style);
            }
        }
    }
}
