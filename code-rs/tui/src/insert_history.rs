use std::fmt;
use std::io;
use std::io::Write;

use crate::tui;
use crossterm::Command;
use crossterm::cursor::MoveTo;
use crossterm::queue;
use crossterm::style::Color as CColor;
use crossterm::style::Colors;
use crossterm::style::Print;
use crossterm::style::SetAttribute;
use crossterm::style::SetBackgroundColor;
use crossterm::style::SetColors;
use crossterm::style::SetForegroundColor;
// No terminal clears in terminal-mode insertion; preserve user's theme.
use ratatui::layout::Size;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::text::Line;
use ratatui::text::Span;
use textwrap::Options as TwOptions;
use textwrap::WordSplitter;

/// Insert `lines` above the viewport.
#[allow(dead_code)]
pub(crate) fn insert_history_lines(terminal: &mut tui::Tui, lines: Vec<Line>) {
    let mut out = std::io::stdout();
    insert_history_lines_to_writer(terminal, &mut out, lines);
}

/// Like `insert_history_lines`, but writes ANSI to the provided writer. This
/// is intended for testing where a capture buffer is used instead of stdout.
#[allow(dead_code)]
pub fn insert_history_lines_to_writer<B, W>(
    terminal: &mut ratatui::Terminal<B>,
    writer: &mut W,
    lines: Vec<Line>,
) where
    B: ratatui::backend::Backend,
    W: Write,
{
    let screen_size = terminal.backend().size().unwrap_or(Size::new(0, 0));
    let cursor_pos = terminal.get_cursor_position().ok();

    let mut area = terminal.get_frame().area();

    // Pre-wrap lines using word-aware wrapping so terminal scrollback sees the same
    // formatting as the TUI. This avoids character-level hard wrapping by the terminal.
    // Wrap to the full content width of the viewport in standard mode.
    let content_width = area.width.max(1);
    let wrapped = word_wrap_lines(&lines, content_width);
    let wrapped_lines = wrapped.len() as u16;
    let cursor_top = if area.bottom() < screen_size.height {
        // If the viewport is not at the bottom of the screen, scroll it down to make room.
        // Don't scroll it past the bottom of the screen.
        let scroll_amount = wrapped_lines.min(screen_size.height - area.bottom());

        // Emit ANSI to scroll the lower region (from the top of the viewport to the bottom
        // of the screen) downward by `scroll_amount` lines. We do this by:
        //   1) Limiting the scroll region to [area.top()+1 .. screen_height] (1-based bounds)
        //   2) Placing the cursor at the top margin of that region
        //   3) Emitting Reverse Index (RI, ESC M) `scroll_amount` times
        //   4) Resetting the scroll region back to full screen
        let top_1based = area.top() + 1; // Convert 0-based row to 1-based for DECSTBM
        queue!(writer, SetScrollRegion(top_1based..screen_size.height)).ok();
        queue!(writer, MoveTo(0, area.top())).ok();
        for _ in 0..scroll_amount {
            // Reverse Index (RI)
            queue!(writer, ReverseIndex).ok();
        }
        queue!(writer, ResetScrollRegion).ok();

        let cursor_top = area.top().saturating_sub(1);
        // Adjust our local notion of area to account for the pre-scroll,
        // but avoid touching ratatui::Terminal internals (set_viewport_area is private).
        area.y += scroll_amount;
        cursor_top
    } else {
        area.top().saturating_sub(1)
    };

    // Limit the scroll region to the lines from the top of the screen to the
    // top of the viewport. With this in place, when we add lines inside this
    // area, only the lines in this area will be scrolled. We place the cursor
    // at the end of the scroll region, and add lines starting there.
    //
    // ┌─Screen───────────────────────┐
    // │┌╌Scroll region╌╌╌╌╌╌╌╌╌╌╌╌╌╌┐│
    // │┆                            ┆│
    // │┆                            ┆│
    // │┆                            ┆│
    // │█╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘│
    // │╭─Viewport───────────────────╮│
    // ││                            ││
    // │╰────────────────────────────╯│
    // └──────────────────────────────┘
    queue!(writer, SetScrollRegion(1..area.top())).ok();

    // Do not force theme colors in terminal mode; let native terminal theme show.

    // NB: we are using MoveTo instead of set_cursor_position here to avoid messing with the
    // terminal's last_known_cursor_position, which hopefully will still be accurate after we
    // fetch/restore the cursor position. insert_history_lines should be cursor-position-neutral :)
    queue!(writer, MoveTo(0, cursor_top)).ok();

    for line in wrapped {
        // Emit a real newline so terminals reliably scroll when at the bottom
        // of the scroll region. Some terminals do not scroll on CSI E
        // (MoveToNextLine); LF is the most portable.
        queue!(writer, Print("\r\n")).ok();
        write_spans(writer, line.iter()).ok();
        // Avoid Clear(EOL) painting solid backgrounds over terminal theme.
    }

    queue!(writer, ResetScrollRegion).ok();

    // Restore the cursor position to where it was before we started.
    if let Some(cursor_pos) = cursor_pos {
        queue!(writer, MoveTo(cursor_pos.x, cursor_pos.y)).ok();
    }

    writer.flush().ok();
}

/// Variant of `insert_history_lines` that reserves `reserved_bottom_rows` at the
/// bottom of the screen for a live UI (e.g., the input composer) and inserts
/// history lines into the scrollback above that region.
#[allow(dead_code)]
pub(crate) fn insert_history_lines_above(terminal: &mut tui::Tui, reserved_bottom_rows: u16, lines: Vec<Line>) {
    let mut out = std::io::stdout();
    insert_history_lines_to_writer_above(terminal, &mut out, reserved_bottom_rows, lines);
}

#[allow(dead_code)]
pub fn insert_history_lines_to_writer_above<B, W>(
    terminal: &mut ratatui::Terminal<B>,
    writer: &mut W,
    reserved_bottom_rows: u16,
    lines: Vec<Line>,
) where
    B: ratatui::backend::Backend,
    W: Write,
{
    if lines.is_empty() { return; }
    let screen_size = terminal.backend().size().unwrap_or(Size::new(0, 0));
    let cursor_pos = terminal.get_cursor_position().ok();

    // Compute the bottom of the reserved region; ensure at least 1 visible row remains
    let screen_h = screen_size.height.max(1);
    let reserved = reserved_bottom_rows.min(screen_h.saturating_sub(1));
    let region_bottom = screen_h.saturating_sub(reserved).max(1);

    // Pre-wrap to avoid terminal hard-wrap artifacts
    let content_width = screen_size.width.max(1);
    let wrapped = word_wrap_lines(&lines, content_width);
    let wrapped_count = wrapped.len();

    tracing::debug!(
        target: "code_tui::scrollback",
        screen_h,
        reserved_bottom_rows,
        reserved,
        region_bottom,
        wrapped_count,
        "scrollback insert sizing"
    );

    if region_bottom <= 1 {
        tracing::debug!(
            target: "code_tui::scrollback",
            screen_h,
            reserved_bottom_rows,
            reserved,
            "scrollback insert fallback: region bottom collapsed"
        );
        // Degenerate case (startup or unknown size): fall back to simple
        // line-by-line prints that let the terminal naturally scroll. This is
        // safe before the first bottom-pane draw and avoids a 1-line scroll
        // region that would overwrite the same line repeatedly.
        for line in word_wrap_lines(&lines, screen_size.width.max(1)) {
            write_spans(writer, line.iter()).ok();
            queue!(writer, Print("\r\n")).ok();
        }
        writer.flush().ok();
        return;
    }

    // Limit scroll region to rows [1 .. region_bottom] so the bottom reserved rows are untouched
    queue!(writer, SetScrollRegion(1..region_bottom)).ok();
    // Place cursor at the last line of the scroll region
    queue!(writer, MoveTo(0, region_bottom.saturating_sub(1))).ok();

    // Do not force theme colors in terminal mode; let native terminal theme show.

    for line in wrapped {
        // Ensure we're at the bottom row of the scroll region; printing a newline
        // while at the bottom margin scrolls the region by one.
        write_spans(writer, line.iter()).ok();
        // Newline scrolls the region up by one when at the bottom margin.
        queue!(writer, Print("\r\n")).ok();
    }

    tracing::debug!(
        target: "code_tui::scrollback",
        screen_h,
        reserved_bottom_rows,
        reserved,
        region_bottom,
        wrapped_count,
        "scrollback insert complete"
    );

    queue!(writer, ResetScrollRegion).ok();
    if let Some(cursor_pos) = cursor_pos {
        queue!(writer, MoveTo(cursor_pos.x, cursor_pos.y)).ok();
    }
    writer.flush().ok();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReverseIndex;

impl Command for ReverseIndex {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        // RI (Reverse Index): ESC M
        write!(f, "\x1bM")
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        // Use ANSI path through ConPTY; WinAPI equivalent isn't exposed.
        Ok(())
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetScrollRegion(pub std::ops::Range<u16>);

impl Command for SetScrollRegion {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        // CSI Ps ; Ps r  (DECSTBM)
        // Set Scrolling Region [top;bottom] (default = full size of window)
        // 1-based line numbers
        write!(f, "\x1b[{};{}r", self.0.start, self.0.end)
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        panic!("tried to execute SetScrollRegion command using WinAPI, use ANSI instead");
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        // TODO(nornagon): is this supported on Windows?
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResetScrollRegion;

impl Command for ResetScrollRegion {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        // CSI r  (DECSTBM)
        // Reset Scrolling Region to full screen
        write!(f, "\x1b[r")
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        panic!("tried to execute ResetScrollRegion command using WinAPI, use ANSI instead");
    }

    #[cfg(windows)]
    fn is_ansi_code_supported(&self) -> bool {
        // TODO(nornagon): is this supported on Windows?
        true
    }
}

struct ModifierDiff {
    pub from: Modifier,
    pub to: Modifier,
}

impl ModifierDiff {
    fn queue<W>(self, mut w: W) -> io::Result<()>
    where
        W: io::Write,
    {
        use crossterm::style::Attribute as CAttribute;
        let removed = self.from - self.to;
        if removed.contains(Modifier::REVERSED) {
            queue!(w, SetAttribute(CAttribute::NoReverse))?;
        }
        if removed.contains(Modifier::BOLD) {
            queue!(w, SetAttribute(CAttribute::NormalIntensity))?;
            if self.to.contains(Modifier::DIM) {
                queue!(w, SetAttribute(CAttribute::Dim))?;
            }
        }
        if removed.contains(Modifier::ITALIC) {
            queue!(w, SetAttribute(CAttribute::NoItalic))?;
        }
        if removed.contains(Modifier::UNDERLINED) {
            queue!(w, SetAttribute(CAttribute::NoUnderline))?;
        }
        if removed.contains(Modifier::DIM) {
            queue!(w, SetAttribute(CAttribute::NormalIntensity))?;
        }
        if removed.contains(Modifier::CROSSED_OUT) {
            queue!(w, SetAttribute(CAttribute::NotCrossedOut))?;
        }
        if removed.contains(Modifier::SLOW_BLINK) || removed.contains(Modifier::RAPID_BLINK) {
            queue!(w, SetAttribute(CAttribute::NoBlink))?;
        }

        let added = self.to - self.from;
        if added.contains(Modifier::REVERSED) {
            queue!(w, SetAttribute(CAttribute::Reverse))?;
        }
        if added.contains(Modifier::BOLD) {
            queue!(w, SetAttribute(CAttribute::Bold))?;
        }
        if added.contains(Modifier::ITALIC) {
            queue!(w, SetAttribute(CAttribute::Italic))?;
        }
        if added.contains(Modifier::UNDERLINED) {
            queue!(w, SetAttribute(CAttribute::Underlined))?;
        }
        if added.contains(Modifier::DIM) {
            queue!(w, SetAttribute(CAttribute::Dim))?;
        }
        if added.contains(Modifier::CROSSED_OUT) {
            queue!(w, SetAttribute(CAttribute::CrossedOut))?;
        }
        if added.contains(Modifier::SLOW_BLINK) {
            queue!(w, SetAttribute(CAttribute::SlowBlink))?;
        }
        if added.contains(Modifier::RAPID_BLINK) {
            queue!(w, SetAttribute(CAttribute::RapidBlink))?;
        }

        Ok(())
    }
}

/// Write the spans to the writer with the correct styling
fn write_spans<'a, I>(mut writer: &mut impl Write, content: I) -> io::Result<()>
where
    I: Iterator<Item = &'a Span<'a>>,
{
    let mut fg = Color::Reset;
    let mut bg = Color::Reset;
    let mut last_modifier = Modifier::empty();
    for span in content {
        let mut modifier = Modifier::empty();
        modifier.insert(span.style.add_modifier);
        modifier.remove(span.style.sub_modifier);
        if modifier != last_modifier {
            let diff = ModifierDiff {
                from: last_modifier,
                to: modifier,
            };
            diff.queue(&mut writer)?;
            last_modifier = modifier;
        }
        let next_fg = span.style.fg.unwrap_or(Color::Reset);
        let next_bg = span.style.bg.unwrap_or(Color::Reset);
        if next_fg != fg || next_bg != bg {
            queue!(
                writer,
                SetColors(Colors::new(next_fg.into(), next_bg.into()))
            )?;
            fg = next_fg;
            bg = next_bg;
        }

        queue!(writer, Print(span.content.clone()))?;
    }

    queue!(
        writer,
        SetForegroundColor(CColor::Reset),
        SetBackgroundColor(CColor::Reset),
        SetAttribute(crossterm::style::Attribute::Reset),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub struct SetUnderlineColor(pub CColor);

impl Command for SetUnderlineColor {
    fn write_ansi(&self, f: &mut impl fmt::Write) -> fmt::Result {
        // Use the CSI 58 sequence for underline color
        // CSI 58:5:n m for 256 colors or CSI 58:2::r:g:b m for RGB
        match self.0 {
            CColor::Black => write!(f, "\x1b[58:5:0m"),
            CColor::DarkGrey => write!(f, "\x1b[58:5:8m"),
            CColor::Red => write!(f, "\x1b[58:5:1m"),
            CColor::DarkRed => write!(f, "\x1b[58:5:9m"),
            CColor::Green => write!(f, "\x1b[58:5:2m"),
            CColor::DarkGreen => write!(f, "\x1b[58:5:10m"),
            CColor::Yellow => write!(f, "\x1b[58:5:3m"),
            CColor::DarkYellow => write!(f, "\x1b[58:5:11m"),
            CColor::Blue => write!(f, "\x1b[58:5:4m"),
            CColor::DarkBlue => write!(f, "\x1b[58:5:12m"),
            CColor::Magenta => write!(f, "\x1b[58:5:5m"),
            CColor::DarkMagenta => write!(f, "\x1b[58:5:13m"),
            CColor::Cyan => write!(f, "\x1b[58:5:6m"),
            CColor::DarkCyan => write!(f, "\x1b[58:5:14m"),
            CColor::White => write!(f, "\x1b[58:5:7m"),
            CColor::Grey => write!(f, "\x1b[58:5:15m"),
            CColor::Rgb { r, g, b } => write!(f, "\x1b[58:2::{}:{}:{}m", r, g, b),
            CColor::AnsiValue(n) => write!(f, "\x1b[58:5:{}m", n),
            CColor::Reset => write!(f, "\x1b[59m"), // Reset underline color
        }
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        // Windows doesn't support underline colors in the same way
        Ok(())
    }
}

/// Word-aware wrapping for a list of `Line`s preserving styles.
pub(crate) fn word_wrap_lines(lines: &[Line], width: u16) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let w = width.max(1) as usize;
    for line in lines {
        out.extend(word_wrap_line(line, w));
    }
    out
}

fn word_wrap_line(line: &Line, width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![to_owned_line(line)];
    }
    // Horizontal rule detection: lines consisting of --- *** or ___ (3+)
    let flat_trim: String = line
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect::<String>()
        .trim()
        .to_string();
    if !flat_trim.is_empty() {
        let chars: Vec<char> = flat_trim.chars().collect();
        let only = |ch: char| chars.iter().all(|c| *c == ch || c.is_whitespace());
        let count = |ch: char| chars.iter().filter(|c| **c == ch).count();
        if (only('-') && count('-') >= 3)
            || (only('*') && count('*') >= 3)
            || (only('_') && count('_') >= 3)
        {
            let hr = Line::from(Span::styled(
                std::iter::repeat('─').take(width).collect::<String>(),
                ratatui::style::Style::default().fg(crate::colors::assistant_hr()),
            ));
            return vec![hr];
        }
    }

    // Concatenate content and keep span boundaries for later re-slicing.
    let mut flat = String::new();
    let mut span_bounds = Vec::new(); // (start_byte, end_byte, style)
    let mut cursor = 0usize;
    for s in &line.spans {
        let text = s.content.as_ref();
        let start = cursor;
        flat.push_str(text);
        cursor += text.len();
        span_bounds.push((start, cursor, s.style));
    }

    // Use textwrap for robust word-aware wrapping; no hyphenation, no breaking words.
    let opts = TwOptions::new(width)
        .break_words(false)
        .word_splitter(WordSplitter::NoHyphenation);
    let wrapped = textwrap::wrap(&flat, &opts);

    if wrapped.len() <= 1 {
        return vec![to_owned_line(line)];
    }

    // Map wrapped pieces back to byte ranges in `flat` sequentially.
    let mut start_cursor = 0usize;
    let mut out: Vec<Line<'static>> = Vec::with_capacity(wrapped.len());
    for piece in wrapped {
        let piece_str: &str = &piece;
        if piece_str.is_empty() {
            out.push(Line {
                style: line.style,
                alignment: line.alignment,
                spans: Vec::new(),
            });
            continue;
        }
        // Find the next occurrence of piece_str at or after start_cursor.
        // textwrap preserves order, so a linear scan is sufficient.
        if let Some(rel) = flat[start_cursor..].find(piece_str) {
            let s = start_cursor + rel;
            let e = s + piece_str.len();
            out.push(slice_line_spans(line, &span_bounds, s, e));
            start_cursor = e;
        } else {
            // Fallback: slice by length from cursor.
            let s = start_cursor;
            let e = (start_cursor + piece_str.len()).min(flat.len());
            out.push(slice_line_spans(line, &span_bounds, s, e));
            start_cursor = e;
        }
    }

    out
}

fn to_owned_line(l: &Line<'_>) -> Line<'static> {
    Line {
        style: l.style,
        alignment: l.alignment,
        spans: l
            .spans
            .iter()
            .map(|s| Span {
                style: s.style,
                content: std::borrow::Cow::Owned(s.content.to_string()),
            })
            .collect(),
    }
}

fn slice_line_spans(
    original: &Line<'_>,
    span_bounds: &[(usize, usize, ratatui::style::Style)],
    start_byte: usize,
    end_byte: usize,
) -> Line<'static> {
    let mut acc: Vec<Span<'static>> = Vec::new();
    for (i, (s, e, style)) in span_bounds.iter().enumerate() {
        if *e <= start_byte {
            continue;
        }
        if *s >= end_byte {
            break;
        }
        let seg_start = start_byte.max(*s);
        let seg_end = end_byte.min(*e);
        if seg_end > seg_start {
            let local_start = seg_start - *s;
            let local_end = seg_end - *s;
            let content = original.spans[i].content.as_ref();
            let slice = &content[local_start..local_end];
            acc.push(Span {
                style: *style,
                content: std::borrow::Cow::Owned(slice.to_string()),
            });
        }
        if *e >= end_byte {
            break;
        }
    }
    Line {
        style: original.style,
        alignment: original.alignment,
        spans: acc,
    }
}
