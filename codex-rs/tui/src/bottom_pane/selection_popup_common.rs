use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Constraint;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Cell;
use ratatui::widgets::Row;
use ratatui::widgets::Table;
use ratatui::widgets::Widget;

use super::scroll_state::ScrollState;
use unicode_width::UnicodeWidthStr;

/// A generic representation of a display row for selection popups.
pub(crate) struct GenericDisplayRow {
    pub name: String,
    pub match_indices: Option<Vec<usize>>, // indices to bold (char positions)
    pub is_current: bool,
    pub description: Option<String>, // optional grey text after the name
    /// Optional explicit color for the `name` span. When `None`, default text color is used.
    pub name_color: Option<ratatui::style::Color>,
}

impl GenericDisplayRow {}

/// Render a list of rows using the provided ScrollState, with shared styling
/// and behavior for selection popups.
pub(crate) fn render_rows(
    area: Rect,
    buf: &mut Buffer,
    rows_all: &[GenericDisplayRow],
    state: &ScrollState,
    max_results: usize,
    _dim_non_selected: bool,
) {
    let mut rows: Vec<Row> = Vec::new();
    if rows_all.is_empty() {
        rows.push(Row::new(vec![Cell::from(Line::from(Span::styled(
            "no matches",
            Style::default().add_modifier(Modifier::ITALIC | Modifier::DIM),
        )))]));
    } else {
        let max_rows_from_area = area.height as usize;
        let visible_rows = max_results
            .min(rows_all.len())
            .min(max_rows_from_area.max(1));

        // Compute starting index based on scroll state and selection.
        let mut start_idx = state.scroll_top.min(rows_all.len().saturating_sub(1));
        if let Some(sel) = state.selected_idx {
            if sel < start_idx {
                start_idx = sel;
            } else if visible_rows > 0 {
                let bottom = start_idx + visible_rows - 1;
                if sel > bottom {
                    start_idx = sel + 1 - visible_rows;
                }
            }
        }

        for (i, row) in rows_all
            .iter()
            .enumerate()
            .skip(start_idx)
            .take(visible_rows)
        {
            let GenericDisplayRow {
                name,
                match_indices,
                is_current: _is_current,
                description,
                name_color,
            } = row;

            // Highlight fuzzy indices when present.
            let mut spans: Vec<Span> = Vec::with_capacity(name.len());
            if let Some(idxs) = match_indices.as_ref() {
                let mut idx_iter = idxs.iter().peekable();
                for (char_idx, ch) in name.chars().enumerate() {
                    let mut style = Style::default();
                    if let Some(color) = *name_color {
                        style = style.fg(color);
                    }
                    if idx_iter.peek().is_some_and(|next| **next == char_idx) {
                        idx_iter.next();
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    spans.push(Span::styled(ch.to_string(), style));
                }
            } else {
                let mut style = Style::default();
                if let Some(color) = *name_color {
                    style = style.fg(color);
                }
                spans.push(Span::styled(name.clone(), style));
            }

            if let Some(desc) = description.as_ref() {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    desc.clone(),
                    Style::default().add_modifier(Modifier::DIM),
                ));
            }

            let mut cell = Cell::from(Line::from(spans));
            if Some(i) == state.selected_idx {
                cell = cell.style(Style::default().fg(crate::colors::primary()));
            } else if *_is_current {
                cell = cell.style(Style::default().fg(crate::colors::light_blue()));
            }
            rows.push(Row::new(vec![cell]));
        }
    }

    let table = Table::new(rows, vec![Constraint::Percentage(100)])
        .widths([Constraint::Percentage(100)])
        .style(Style::default().bg(crate::colors::background()).fg(crate::colors::text()));

    table.render(area, buf);
}

/// Estimate the required height (in terminal rows) to render up to
/// `max_results` rows for the provided `rows_all`, taking wrapping into
/// account for the given content `width` and the current `state` window.
///
/// This mirrors the selection windowing logic in `render_rows` so that the
/// composer can allocate an appropriate hint/popup height prior to render.
pub(crate) fn measure_rows_height(
    rows_all: &[GenericDisplayRow],
    state: &ScrollState,
    max_results: usize,
    width: u16,
) -> u16 {
    // Empty -> one line placeholder ("no matches").
    if rows_all.is_empty() {
        return 1;
    }

    // Prevent division by zero; treat zero-width as a single-column layout.
    let content_width = width.max(1) as usize;

    let visible_rows = max_results.min(rows_all.len());

    // Compute starting index like in render_rows to keep scroll behavior aligned.
    let mut start_idx = state.scroll_top.min(rows_all.len().saturating_sub(1));
    if let Some(sel) = state.selected_idx {
        if sel < start_idx {
            start_idx = sel;
        } else if visible_rows > 0 {
            let bottom = start_idx + visible_rows - 1;
            if sel > bottom {
                start_idx = sel + 1 - visible_rows;
            }
        }
    }

    // Sum the wrapped line count for the visible window.
    let mut total_lines: usize = 0;
    for row in rows_all
        .iter()
        .enumerate()
        .skip(start_idx)
        .take(visible_rows)
    {
        let (_i, GenericDisplayRow { name, description, .. }) = row;

        // Compute the display string width: name [+ two spaces + description].
        let mut line_width = UnicodeWidthStr::width(name.as_str());

        if let Some(desc) = description.as_ref() {
            // Two spaces between name and description, like in render_rows.
            line_width += 2 + UnicodeWidthStr::width(desc.as_str());
        }

        // Wrapped height = ceil(line_width / content_width), minimum 1.
        let wrapped = if line_width == 0 { 1 } else { (line_width + content_width - 1) / content_width };
        total_lines += wrapped.max(1);
    }

    total_lines.min(u16::MAX as usize) as u16
}
