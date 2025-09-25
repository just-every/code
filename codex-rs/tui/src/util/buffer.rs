use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use std::borrow::Cow;
use unicode_width::UnicodeWidthStr;

use crate::live_wrap::take_prefix_by_width;

/// Fill the given rectangular area with the provided style and optional character.
///
/// The rectangle is clipped to the buffer's bounds before being applied. When `fill_char`
/// is `Some(_)`, each cell's symbol is replaced; otherwise only the style is updated.
pub fn fill_rect(buf: &mut Buffer, area: Rect, fill_char: Option<char>, style: Style) {
    let rect = buf.area.intersection(area);
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    let buf_width = buf.area.width as usize;
    let offset_x = rect.x.saturating_sub(buf.area.x) as usize;
    let offset_y = rect.y.saturating_sub(buf.area.y) as usize;
    let row_span = rect.width as usize;

    if let Some(ch) = fill_char {
        let mut char_buf = [0; 4];
        let expected_symbol = ch.encode_utf8(&mut char_buf);
        for row in 0..rect.height as usize {
            let start = (offset_y + row) * buf_width + offset_x;
            let end = start + row_span;
            let row = &mut buf.content[start..end];
            for cell in row {
                if cell.style() != style {
                    cell.set_style(style);
                }
                if cell.symbol() != expected_symbol {
                    cell.set_char(ch);
                }
            }
        }
    } else {
        for row in 0..rect.height as usize {
            let start = (offset_y + row) * buf_width + offset_x;
            let end = start + row_span;
            let row = &mut buf.content[start..end];
            for cell in row {
                if cell.style() != style {
                    cell.set_style(style);
                }
            }
        }
    }
}

/// Draw a styled line into the buffer, clipping to the provided width and
/// applying a base style (commonly used to enforce background colors).
pub fn write_line(
    buf: &mut Buffer,
    origin_x: u16,
    origin_y: u16,
    max_width: u16,
    line: &Line<'_>,
    base_style: Style,
) {
    if max_width == 0 {
        return;
    }
    let buf_rect = buf.area;
    if origin_y < buf_rect.y || origin_y >= buf_rect.y.saturating_add(buf_rect.height) {
        return;
    }

    let line_style = base_style.patch(line.style);
    let mut cursor_x = origin_x.max(buf_rect.x);
    let right_edge = buf_rect
        .x
        .saturating_add(buf_rect.width)
        .min(origin_x.saturating_add(max_width));
    if cursor_x >= right_edge {
        return;
    }
    let mut remaining = right_edge.saturating_sub(cursor_x);

    for span in &line.spans {
        if remaining == 0 {
            break;
        }
        if span.content.is_empty() {
            continue;
        }
        let span_style = line_style.patch(span.style);
        let mut text: Cow<'_, str> = Cow::Borrowed(span.content.as_ref());
        let mut span_width = UnicodeWidthStr::width(text.as_ref());
        if span_width == 0 {
            continue;
        }
        if span_width as u16 > remaining {
            let (prefix, _, taken) = take_prefix_by_width(text.as_ref(), remaining as usize);
            if taken == 0 {
                break;
            }
            text = Cow::Owned(prefix);
            span_width = taken;
        }
        buf.set_string(cursor_x, origin_y, text.as_ref(), span_style);
        let advance = span_width.min(remaining as usize) as u16;
        cursor_x = cursor_x.saturating_add(advance);
        remaining = remaining.saturating_sub(advance);
    }
}
