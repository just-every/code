use std::borrow::Cow;
use std::cell::Cell;
use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use codex_ansi_escape::ansi_escape_line;
use ratatui::text::Line as RtLine;
use unicode_segmentation::UnicodeSegmentation;
use vt100::Parser as VtParser;

use crate::app_event::{TerminalAfter, TerminalCommandGate};
use crate::colors;
use crate::sanitize::{Mode as SanitizeMode, Options as SanitizeOptions, sanitize_for_tui};

pub(crate) const TERMINAL_MAX_LINES: usize = 10_000;
pub(crate) const TERMINAL_MAX_RAW: usize = 1_048_576;
pub(crate) const TERMINAL_PTY_ROWS: u16 = 24;
pub(crate) const TERMINAL_PTY_COLS: u16 = 80;
pub(crate) const TERMINAL_SCROLLBACK: usize = TERMINAL_MAX_LINES;
const TERMINAL_TABSTOP: usize = 4;

#[derive(Default)]
pub(crate) struct TerminalState {
    pub(crate) overlay: Option<TerminalOverlay>,
    pub(crate) next_id: u64,
    pub(crate) after: Option<TerminalAfter>,
    pub(crate) last_visible_rows: Cell<u16>,
    pub(crate) last_visible_cols: Cell<u16>,
}

impl TerminalState {
    pub(crate) fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    pub(crate) fn overlay(&self) -> Option<&TerminalOverlay> {
        self.overlay.as_ref()
    }

    pub(crate) fn overlay_mut(&mut self) -> Option<&mut TerminalOverlay> {
        self.overlay.as_mut()
    }

    pub(crate) fn clear(&mut self) {
        self.overlay = None;
        self.after = None;
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PendingManualTerminal {
    pub(crate) command: String,
    pub(crate) run_direct: bool,
}

pub(crate) struct TerminalOverlay {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) command_display: String,
    pub(crate) lines: VecDeque<RtLine<'static>>,
    pub(crate) terminal_lines: Vec<RtLine<'static>>,
    pub(crate) terminal_plain_lines: Vec<String>,
    pub(crate) info_lines: Vec<RtLine<'static>>,
    pub(crate) parser: VtParser,
    pub(crate) raw_stream: Vec<u8>,
    pub(crate) scroll: u16,
    pub(crate) visible_rows: u16,
    pub(crate) running: bool,
    pub(crate) exit_code: Option<i32>,
    pub(crate) duration: Option<Duration>,
    pub(crate) start_time: Option<Instant>,
    pub(crate) truncated: bool,
    pub(crate) auto_close_on_success: bool,
    pub(crate) pending_command: Option<PendingCommand>,
    pub(crate) last_info_message: Option<String>,
    pub(crate) pty_rows: u16,
    pub(crate) pty_cols: u16,
    tab_column: usize,
}

pub(crate) enum PendingCommandAction {
    Forwarded(String),
    Manual(String),
}

impl TerminalOverlay {
    pub(crate) fn new(
        id: u64,
        title: String,
        command_display: String,
        auto_close_on_success: bool,
    ) -> Self {
        Self {
            id,
            title,
            command_display,
            lines: VecDeque::new(),
            terminal_lines: Vec::new(),
            terminal_plain_lines: Vec::new(),
            info_lines: Vec::new(),
            parser: VtParser::new(TERMINAL_PTY_ROWS, TERMINAL_PTY_COLS, TERMINAL_SCROLLBACK),
            raw_stream: Vec::new(),
            scroll: 0,
            visible_rows: 0,
            running: true,
            exit_code: None,
            duration: None,
            start_time: None,
            truncated: false,
            auto_close_on_success,
            pending_command: None,
            last_info_message: None,
            pty_rows: TERMINAL_PTY_ROWS,
            pty_cols: TERMINAL_PTY_COLS,
            tab_column: 0,
        }
    }

    pub(crate) fn total_render_lines(&self) -> usize {
        let base = self.lines.len();
        if self.truncated {
            base.saturating_add(1)
        } else {
            base
        }
    }

    pub(crate) fn max_scroll(&self) -> u16 {
        let visible = self.visible_rows.max(1) as usize;
        let total = self.total_render_lines();
        total.saturating_sub(visible).min(u16::MAX as usize) as u16
    }

    pub(crate) fn clamp_scroll(&mut self) {
        let max_scroll = self.max_scroll();
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    pub(crate) fn is_following(&self) -> bool {
        let visible = self.visible_rows.max(1) as usize;
        let total = self.total_render_lines();
        (self.scroll as usize).saturating_add(visible) >= total
    }

    pub(crate) fn auto_follow(&mut self, was_following: bool) {
        if !was_following {
            return;
        }
        let visible = self.visible_rows.max(1) as usize;
        let total = self.total_render_lines();
        let max_scroll = total.saturating_sub(visible);
        self.scroll = max_scroll.min(u16::MAX as usize) as u16;
    }

    pub(crate) fn reset_for_rerun(&mut self) {
        self.lines.clear();
        self.terminal_lines.clear();
        self.terminal_plain_lines.clear();
        self.info_lines.clear();
        let rows = if self.pty_rows == 0 {
            TERMINAL_PTY_ROWS
        } else {
            self.pty_rows
        };
        let cols = if self.pty_cols == 0 {
            TERMINAL_PTY_COLS
        } else {
            self.pty_cols
        };
        self.parser = VtParser::new(rows, cols, TERMINAL_SCROLLBACK);
        self.raw_stream.clear();
        self.scroll = 0;
        self.visible_rows = 0;
        self.running = true;
        self.exit_code = None;
        self.duration = None;
        self.start_time = None;
        self.truncated = false;
        self.pending_command = None;
        self.last_info_message = None;
        self.tab_column = 0;
        self.rebuild_lines();
    }

    pub(crate) fn set_pending_command(
        &mut self,
        suggestion: String,
        ack: Sender<TerminalCommandGate>,
    ) {
        self.cancel_pending_command();
        self.pending_command = Some(PendingCommand::new(suggestion, ack));
    }

    pub(crate) fn ensure_pending_command(&mut self) {
        if self.pending_command.is_none() {
            self.pending_command = Some(PendingCommand::manual());
        }
    }

    pub(crate) fn accept_pending_command(&mut self) -> Option<PendingCommandAction> {
        let pending = self.pending_command.take()?;
        pending.action_after_enter()
    }

    pub(crate) fn cancel_pending_command(&mut self) {
        if let Some(mut pending) = self.pending_command.take() {
            if let Some(tx) = pending.ack.take() {
                let _ = tx.send(TerminalCommandGate::Cancel);
            }
        }
    }

    pub(crate) fn push_info_message(&mut self, message: &str) {
        self.push_info_message_with_style(message, false);
    }

    pub(crate) fn push_assistant_message(&mut self, message: &str) {
        self.push_info_message_with_style(message, true);
    }

    pub(crate) fn push_info_message_with_style(&mut self, message: &str, emphasize: bool) {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return;
        }
        let was_following = self.is_following();

        if self.last_info_message.as_deref() == Some(trimmed) {
            if was_following {
                self.auto_follow(true);
            } else {
                self.clamp_scroll();
            }
            return;
        }

        let mut block: Vec<RtLine<'static>> = Vec::new();
        if !self.terminal_last_line_is_blank() {
            block.push(blank_line());
        }

        let sanitized = sanitize_for_tui(
            trimmed,
            SanitizeMode::AnsiPreserving,
            SanitizeOptions {
                expand_tabs: true,
                ..Default::default()
            },
        );
        let mut line = ansi_escape_line(&sanitized);
        line.spans.insert(
            0,
            ratatui::text::Span::styled("• ", ratatui::style::Style::default().fg(colors::text())),
        );
        if emphasize {
            for span in line.spans.iter_mut() {
                span.style = span.style.add_modifier(ratatui::style::Modifier::BOLD);
            }
        }
        block.push(line);
        block.push(blank_line());

        self.info_lines = block;
        self.last_info_message = Some(trimmed.to_string());
        self.rebuild_lines();

        if was_following {
            self.auto_follow(true);
        } else {
            self.clamp_scroll();
        }
    }

    pub(crate) fn terminal_last_line_is_blank(&self) -> bool {
        self.terminal_lines
            .last()
            .map(line_is_blank)
            .unwrap_or(true)
    }

    pub(crate) fn append_chunk(&mut self, chunk: &[u8], is_stderr: bool) {
        if chunk.is_empty() {
            return;
        }
        self.raw_stream.extend_from_slice(chunk);
        if self.raw_stream.len() > TERMINAL_MAX_RAW {
            let excess = self.raw_stream.len() - TERMINAL_MAX_RAW;
            self.raw_stream.drain(..excess);
        }
        let was_following = self.is_following();
        let previous_plain = if is_stderr {
            Some(self.terminal_plain_lines.clone())
        } else {
            None
        };
        let processed = self.preprocess_chunk_for_parser(chunk);
        self.parser.process(processed.as_ref());
        self.refresh_terminal_lines(is_stderr, previous_plain);
        if was_following {
            self.auto_follow(true);
        } else {
            self.clamp_scroll();
        }
    }

    fn preprocess_chunk_for_parser<'a>(&mut self, chunk: &'a [u8]) -> Cow<'a, [u8]> {
        let mut idx = 0usize;
        let mut col = self.tab_column;
        let mut needs_expand = false;
        while idx < chunk.len() {
            match chunk[idx] {
                b'\t' => {
                    needs_expand = true;
                    col += TERMINAL_TABSTOP - (col % TERMINAL_TABSTOP);
                    idx += 1;
                }
                b'\n' | b'\r' => {
                    col = 0;
                    idx += 1;
                }
                0x1B => {
                    let consumed = consume_escape_sequence(&chunk[idx..]);
                    if consumed == 0 {
                        break;
                    }
                    idx += consumed;
                }
                0x08 => {
                    if col > 0 {
                        col -= 1;
                    }
                    idx += 1;
                }
                byte if byte & 0xC0 == 0x80 => {
                    idx += 1;
                }
                byte if byte < 0x20 || (0x80..=0x9F).contains(&byte) => {
                    idx += 1;
                }
                _ => {
                    col = col.saturating_add(1);
                    idx += 1;
                }
            }
        }

        if !needs_expand {
            self.tab_column = col;
            return Cow::Borrowed(chunk);
        }

        let mut out = Vec::with_capacity(chunk.len() + 16);
        let mut idx = 0usize;
        let mut col = self.tab_column;
        while idx < chunk.len() {
            match chunk[idx] {
                b'\t' => {
                    let spaces = TERMINAL_TABSTOP - (col % TERMINAL_TABSTOP);
                    out.extend(std::iter::repeat(b' ').take(spaces));
                    col += spaces;
                    idx += 1;
                }
                b'\n' => {
                    out.push(b'\n');
                    col = 0;
                    idx += 1;
                }
                b'\r' => {
                    out.push(b'\r');
                    col = 0;
                    idx += 1;
                }
                0x1B => {
                    let consumed = copy_escape_sequence(&chunk[idx..], &mut out);
                    if consumed == 0 {
                        break;
                    }
                    idx += consumed;
                }
                0x08 => {
                    out.push(0x08);
                    if col > 0 {
                        col -= 1;
                    }
                    idx += 1;
                }
                byte => {
                    out.push(byte);
                    if byte & 0xC0 == 0x80 {
                        // continuation byte, column already accounted for
                    } else if byte < 0x20 || (0x80..=0x9F).contains(&byte) {
                        // control byte, ignore for column advance
                    } else {
                        col = col.saturating_add(1);
                    }
                    idx += 1;
                }
            }
        }
        self.tab_column = col;
        Cow::Owned(out)
    }

    pub(crate) fn refresh_terminal_lines(
        &mut self,
        is_stderr: bool,
        previous_plain: Option<Vec<String>>,
    ) {
        self.last_info_message = None;

        let screen = self.parser.screen();
        let (_, cols) = screen.size();
        let rows: Vec<Vec<u8>> = screen.rows_formatted(0, cols).collect();

        let mut new_lines: Vec<RtLine<'static>> = Vec::with_capacity(rows.len());
        let mut new_plain: Vec<String> = Vec::with_capacity(rows.len());

        for row_bytes in rows {
            let row_string = String::from_utf8_lossy(&row_bytes);
            let filtered = strip_non_sgr_csi(&row_string);

            let sanitized = sanitize_for_tui(
                &filtered,
                SanitizeMode::AnsiPreserving,
                SanitizeOptions {
                    expand_tabs: true,
                    ..Default::default()
                },
            );
            let plain = sanitize_for_tui(
                &filtered,
                SanitizeMode::Plain,
                SanitizeOptions {
                    expand_tabs: true,
                    ..Default::default()
                },
            );
            let plain_trimmed = plain.trim_end_matches(' ').to_string();

            let mut line = if sanitized.trim().is_empty() {
                blank_line()
            } else {
                ansi_escape_line(&sanitized)
            };

            if is_command_plain(&plain_trimmed) {
                tint_command_line(&mut line);
            }

            new_plain.push(plain_trimmed);
            new_lines.push(line);
        }

        while new_lines.len() > 1
            && new_lines
                .last()
                .map(|line| line_is_blank(line))
                .unwrap_or(false)
            && new_plain.last().map(|s| s.is_empty()).unwrap_or(false)
        {
            new_lines.pop();
            new_plain.pop();
        }

        let mut truncated = false;
        if new_lines.len() > TERMINAL_MAX_LINES {
            truncated = true;
            let start = new_lines.len() - TERMINAL_MAX_LINES;
            new_lines.drain(..start);
            new_plain.drain(..start);
        }

        if let (true, Some(prev)) = (is_stderr, previous_plain.as_ref()) {
            let changed = diff_changed_indices(prev, &new_plain);
            for idx in changed {
                if let Some(line) = new_lines.get_mut(idx) {
                    tint_stderr_line(line);
                }
            }
        }

        self.terminal_plain_lines = new_plain;
        self.terminal_lines = new_lines;
        if truncated {
            self.truncated = true;
        }
        self.rebuild_lines();
    }

    pub(crate) fn rebuild_lines(&mut self) {
        let mut combined: VecDeque<RtLine<'static>> = self.terminal_lines.iter().cloned().collect();
        for info in &self.info_lines {
            combined.push_back(info.clone());
        }

        let mut dropped = 0usize;
        while combined.len() > TERMINAL_MAX_LINES {
            combined.pop_front();
            dropped += 1;
        }

        if dropped > 0 {
            self.truncated = true;
            if dropped >= self.terminal_lines.len() {
                self.terminal_lines.clear();
                self.terminal_plain_lines.clear();
            } else {
                self.terminal_lines.drain(..dropped);
                self.terminal_plain_lines.drain(..dropped);
            }
        }

        self.lines = combined;
    }

    pub(crate) fn update_pty_dimensions(&mut self, rows: u16, cols: u16) -> bool {
        if rows == 0 || cols == 0 {
            return false;
        }
        if self.pty_rows == rows && self.pty_cols == cols {
            return false;
        }
        self.pty_rows = rows;
        self.pty_cols = cols;
        let mut parser = VtParser::new(self.pty_rows, self.pty_cols, TERMINAL_SCROLLBACK);
        if !self.raw_stream.is_empty() {
            parser.process(&self.raw_stream);
        }
        self.parser = parser;
        let was_following = self.is_following();
        self.refresh_terminal_lines(false, None);
        if was_following {
            self.auto_follow(true);
        } else {
            self.clamp_scroll();
        }
        true
    }

    pub(crate) fn finalize(&mut self, exit_code: Option<i32>, duration: Duration) {
        self.running = false;
        self.exit_code = exit_code;
        self.duration = Some(duration);
        self.start_time = None;
    }
}

#[derive(Clone)]
pub(crate) struct PendingCommand {
    input: String,
    cursor: usize,
    ack: Option<Sender<TerminalCommandGate>>,
}

impl PendingCommand {
    pub(crate) fn new(suggestion: String, ack: Sender<TerminalCommandGate>) -> Self {
        let input = suggestion;
        let cursor = input.len();
        Self {
            input,
            cursor,
            ack: Some(ack),
        }
    }

    pub(crate) fn manual() -> Self {
        Self {
            input: String::new(),
            cursor: 0,
            ack: None,
        }
    }

    pub(crate) fn manual_with_input(input: String) -> Self {
        Self {
            cursor: input.len(),
            input,
            ack: None,
        }
    }

    pub(crate) fn input(&self) -> &str {
        &self.input
    }

    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(crate) fn action_after_enter(mut self) -> Option<PendingCommandAction> {
        let command = self.input.trim().to_string();
        if command.is_empty() {
            return None;
        }
        if let Some(tx) = self.ack.take() {
            let _ = tx.send(TerminalCommandGate::Run(command.clone()));
            Some(PendingCommandAction::Forwarded(command))
        } else {
            Some(PendingCommandAction::Manual(command))
        }
    }

    pub(crate) fn insert_char(&mut self, ch: char) -> bool {
        if ch.is_control() {
            return false;
        }
        let mut buf = [0u8; 4];
        let encoded = ch.encode_utf8(&mut buf);
        self.input.insert_str(self.cursor, encoded);
        self.cursor = self.cursor.saturating_add(encoded.len());
        true
    }

    pub(crate) fn backspace(&mut self) -> bool {
        let Some(prev) = self.prev_boundary() else {
            return false;
        };
        self.input.drain(prev..self.cursor);
        self.cursor = prev;
        true
    }

    pub(crate) fn delete(&mut self) -> bool {
        let Some(next) = self.next_boundary() else {
            return false;
        };
        self.input.drain(self.cursor..next);
        true
    }

    pub(crate) fn move_left(&mut self) -> bool {
        let Some(prev) = self.prev_boundary() else {
            return false;
        };
        self.cursor = prev;
        true
    }

    pub(crate) fn move_right(&mut self) -> bool {
        let Some(next) = self.next_boundary() else {
            return false;
        };
        self.cursor = next;
        true
    }

    pub(crate) fn move_home(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor = 0;
        true
    }

    pub(crate) fn move_end(&mut self) -> bool {
        let len = self.input.len();
        if self.cursor == len {
            return false;
        }
        self.cursor = len;
        true
    }

    fn prev_boundary(&self) -> Option<usize> {
        if self.cursor == 0 {
            return None;
        }
        let mut prev: Option<usize> = None;
        for (idx, _) in self.input.grapheme_indices(true) {
            if idx >= self.cursor {
                break;
            }
            prev = Some(idx);
        }
        prev
    }

    fn next_boundary(&self) -> Option<usize> {
        if self.cursor >= self.input.len() {
            return None;
        }
        for (idx, _) in self.input.grapheme_indices(true) {
            if idx > self.cursor {
                return Some(idx);
            }
        }
        Some(self.input.len())
    }
}

fn blank_line() -> RtLine<'static> {
    ratatui::text::Line::from(vec![ratatui::text::Span::raw(String::new())])
}

fn line_is_blank(line: &RtLine<'_>) -> bool {
    line.spans.iter().all(|span| span.content.trim().is_empty())
}

fn strip_non_sgr_csi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{001B}' {
            if matches!(chars.peek(), Some('[')) {
                chars.next();
                let mut seq = String::from("\u{001B}[");
                while let Some(next) = chars.next() {
                    seq.push(next);
                    let final_byte = next as u32;
                    if (0x40..=0x7E).contains(&final_byte) {
                        if next == 'm' {
                            out.push_str(&seq);
                        }
                        break;
                    }
                }
                continue;
            }
            continue;
        }
        if ch == '\t' {
            out.push('\t');
            continue;
        }
        out.push(ch);
    }
    out
}

fn consume_escape_sequence(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    let mut idx = 1usize; // always consume ESC
    if idx >= bytes.len() {
        return idx;
    }
    let leader = bytes[idx];
    idx += 1;
    match leader {
        b'[' => {
            while idx < bytes.len() {
                let c = bytes[idx];
                idx += 1;
                if (0x40..=0x7E).contains(&c) {
                    break;
                }
            }
        }
        b']' | b'P' | b'X' | b'^' | b'_' => {
            while idx < bytes.len() {
                let c = bytes[idx];
                idx += 1;
                if c == 0x07 {
                    break;
                }
                if c == b'\\' {
                    if idx < bytes.len() {
                        idx += 1;
                    }
                    break;
                }
            }
        }
        _ => {
            while idx < bytes.len() && (0x20..=0x2F).contains(&bytes[idx]) {
                idx += 1;
            }
            if idx < bytes.len() {
                idx += 1;
            }
        }
    }
    idx
}

fn copy_escape_sequence(bytes: &[u8], out: &mut Vec<u8>) -> usize {
    let consumed = consume_escape_sequence(bytes);
    if consumed == 0 {
        return 0;
    }
    out.extend_from_slice(&bytes[..consumed]);
    consumed
}

fn diff_changed_indices(prev: &[String], next: &[String]) -> Vec<usize> {
    let mut changed = Vec::new();
    let shared = prev.len().min(next.len());
    for idx in 0..shared {
        if prev[idx] != next[idx] {
            changed.push(idx);
        }
    }
    if next.len() > prev.len() {
        changed.extend(prev.len()..next.len());
    }
    changed
}

fn is_command_plain(plain: &str) -> bool {
    plain.trim_start().starts_with("$ ")
}

fn tint_command_line(line: &mut RtLine<'_>) {
    let primary = colors::primary();
    for span in line.spans.iter_mut() {
        span.style.fg = Some(primary);
    }
}

fn tint_stderr_line(line: &mut RtLine<'_>) {
    let warn = colors::warning();
    for span in line.spans.iter_mut() {
        if span.style.fg.is_none() {
            span.style.fg = Some(warn);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_chunk_preserves_ls_alignment() {
        let sample = b"AGENTS.md           \x1b[34mcodex-rs\x1b[39;49m\x1b[0m            \x1b[34mFormula\x1b[39;49m\x1b[0m             pnpm-lock.yaml\r\n\
\x1b[31mbuild-fast.sh\x1b[39;49m\x1b[0m       config.toml.example \x1b[34mhomebrew-tap\x1b[39;49m\x1b[0m        pnpm-workspace.yaml\r\n\
CHANGELOG.md        \x1b[34mdocs\x1b[39;49m\x1b[0m                LICENSE             README.md\r\n\
cliff.toml          flake.lock          NOTICE              \x1b[34mrelease-notes\x1b[39;49m\x1b[0m\r\n\
\x1b[34mcodex-cli\x1b[39;49m\x1b[0m           flake.nix           package.json        \x1b[34mscripts\x1b[39;49m\x1b[0m\r\n";

        let mut overlay = TerminalOverlay::new(42, "test".into(), "ls".into(), false);
        overlay.append_chunk(sample, false);

        assert!(!overlay.terminal_plain_lines.is_empty());
        let first = &overlay.terminal_plain_lines[0];
        assert!(first.contains("AGENTS.md"));
        assert!(first.contains("codex-rs"));
        let idx_a = first.find("AGENTS.md").unwrap();
        let idx_c = first.find("codex-rs").unwrap();
        assert!(idx_c > idx_a);
        let between = &first[idx_a + "AGENTS.md".len()..idx_c];
        assert!(
            between.chars().all(|ch| ch == ' '),
            "expected padding between columns, got {between:?}"
        );

        // Ensure later rows keep column spacing as well
        let last = overlay
            .terminal_plain_lines
            .iter()
            .find(|line| line.contains("package.json"))
            .expect("missing package.json row");
        let idx_pkg = last.find("package.json").unwrap();
        let idx_scripts = last.find("scripts").unwrap();
        let between_tail = &last[idx_pkg + "package.json".len()..idx_scripts];
        assert!(
            between_tail.chars().all(|ch| ch == ' '),
            "expected padding between package.json and scripts, got {between_tail:?}"
        );
    }

    #[test]
    fn strip_non_sgr_csi_preserves_tabs() {
        assert_eq!(strip_non_sgr_csi("col1\tcol2"), "col1\tcol2");
    }

    #[test]
    fn sanitize_for_tui_expands_tabs() {
        use crate::sanitize::{sanitize_for_tui, Mode as SanitizeMode, Options as SanitizeOptions};

        let filtered = "col1\tcol2\tcol3";
        let sanitized = sanitize_for_tui(
            filtered,
            SanitizeMode::Plain,
            SanitizeOptions {
                expand_tabs: true,
                ..Default::default()
            },
        );
        assert_eq!(sanitized, "col1    col2    col3");
    }
}
