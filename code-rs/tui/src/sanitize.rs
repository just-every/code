use std::sync::OnceLock;
use unicode_width::UnicodeWidthChar;

/// Sanitization mode:
/// - Plain: remove all control/escape sequences; output is pure printable text.
/// - AnsiPreserving: keep CSI (ESC '[' ... final) so color/style can be parsed
///   later by the ANSI parser; strip OSC/DCS/APC/PM/SOS and other controls.
#[derive(Clone, Copy)]
pub enum Mode {
    Plain,
    AnsiPreserving,
}

#[derive(Clone, Copy)]
pub struct Options {
    pub expand_tabs: bool,
    pub tabstop: usize,
    pub debug_markers: bool,
}

impl Default for Options {
    fn default() -> Self { Options { expand_tabs: true, tabstop: 4, debug_markers: false } }
}

pub fn sanitize_for_tui(input: &str, mode: Mode, opts: Options) -> String {
    // Optionally expand tabs first so that later stripping does not interact
    // with spaces we insert.
    let mut text = if opts.expand_tabs { expand_tabs_to_spaces(input, opts.tabstop) } else { input.to_string() };
    text = strip_specials(text, mode, opts.debug_markers);
    if ascii_only_enabled() {
        text = force_ascii(text);
    }
    text
}

fn ascii_only_enabled() -> bool {
    static ASCII_ONLY: OnceLock<bool> = OnceLock::new();
    *ASCII_ONLY.get_or_init(|| {
        std::env::var("CODEX_TUI_ASCII")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false)
    })
}

fn force_ascii(input: String) -> String {
    if input.is_ascii() {
        return input;
    }
    input
        .chars()
        .map(|ch| if ch.is_ascii() { ch } else { '?' })
        .collect()
}

fn expand_tabs_to_spaces(input: &str, tabstop: usize) -> String {
    let ts = tabstop.max(1);
    let mut out = String::with_capacity(input.len());
    for line in input.split_inclusive('\n') {
        let mut col = 0usize; // display columns in this logical line
        for ch in line.chars() {
            match ch {
                '\t' => {
                    let spaces = ts - (col % ts);
                    out.extend(std::iter::repeat(' ').take(spaces));
                    col += spaces;
                }
                '\n' => {
                    out.push('\n');
                    col = 0;
                }
                _ => {
                    out.push(ch);
                    // Advance columns using Unicode width so tabs align correctly
                    col += UnicodeWidthChar::width(ch).unwrap_or(1).max(1);
                }
            }
        }
    }
    out
}

fn strip_specials(input: String, mode: Mode, debug_markers: bool) -> String {
    // Work on chars to detect escape sequences and zero-width/bidi controls.
    let mut out = String::with_capacity(input.len());
    let mut it = input.chars().peekable();

    // Helpers
    fn is_c1(ch: char) -> bool { (0x80..=0x9F).contains(&(ch as u32)) }
    fn is_zero_width_or_bidi(ch: char) -> bool {
        matches!(
            ch,
            '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}' | '\u{00AD}' | '\u{180E}'
                | '\u{200E}' | '\u{200F}' | '\u{061C}' | '\u{202A}' | '\u{202B}' | '\u{202D}'
                | '\u{202E}' | '\u{202C}' | '\u{2066}' | '\u{2067}' | '\u{2068}' | '\u{2069}'
        )
    }
    fn consume_until_st_or_bel<I: Iterator<Item = char>>(it: &mut std::iter::Peekable<I>) {
        while let Some(&c) = it.peek() {
            match c {
                '\u{0007}' => { it.next(); break; } // BEL
                '\u{001B}' => { // ESC
                    it.next();
                    if matches!(it.peek(), Some('\\')) { it.next(); break; } // ST = ESC \
                }
                _ => { it.next(); }
            }
        }
    }

    while let Some(ch) = it.next() {
        match ch {
            '\u{001B}' => {
                match it.peek().copied() {
                    // CSI: ESC [ ... final (0x40..0x7E)
                    Some('[') => {
                        // Keep if preserving ANSI; drop otherwise
                        if let Mode::AnsiPreserving = mode {
                            out.push('\u{001B}');
                            out.push('[');
                            it.next();
                            while let Some(&c) = it.peek() {
                                let u = c as u32;
                                let is_final = (0x40..=0x7E).contains(&u);
                                out.push(c);
                                it.next();
                                if is_final { break; }
                            }
                        } else {
                            // Consume but do not emit
                            it.next();
                            while let Some(&c) = it.peek() {
                                let u = c as u32;
                                if (0x40..=0x7E).contains(&u) { it.next(); break; } else { it.next(); }
                            }
                        }
                    }
                    // OSC and other string types: strip in all modes
                    Some(']') => { it.next(); if debug_markers { out.push('·'); } consume_until_st_or_bel(&mut it); }
                    Some('P') | Some('X') | Some('^') | Some('_') => { it.next(); if debug_markers { out.push('·'); } consume_until_st_or_bel(&mut it); }
                    // Other ESC sequences: drop
                    Some(_) | None => {
                        // intermediates 0x20..0x2F then a final 0x40..0x7E
                        while let Some(&c) = it.peek() {
                            let u = c as u32;
                            if (0x20..=0x2F).contains(&u) { it.next(); } else { break; }
                        }
                        if let Some(&c) = it.peek() {
                            let u = c as u32;
                            if (0x40..=0x7E).contains(&u) { it.next(); }
                        }
                    }
                }
            }
            // Preserve newlines for layout; tabs must never be re-emitted
            // here (they should have been expanded earlier).
            '\n' => out.push('\n'),
            c if (c as u32) < 0x20 || c == '\u{007F}' => { if debug_markers { out.push('·'); } }
            c if is_c1(c) => { if debug_markers { out.push('·'); } }
            c if is_zero_width_or_bidi(c) => { if debug_markers { out.push('·'); } }
            _ => out.push(ch),
        }
    }
    out
}
