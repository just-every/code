//! Packs footer hints into rows without separating a shortcut from its label.
//! Callers retain ownership of styling and of clipping or wrapping oversized hints.

pub(crate) fn wrap_hint_rows<T>(
    hints: impl IntoIterator<Item = T>,
    width: u16,
    separator_width: usize,
    hint_width: impl Fn(&T) -> usize,
) -> Vec<Vec<T>> {
    let width = usize::from(width.max(1));
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut used = 0usize;
    for hint in hints {
        let hint_width = hint_width(&hint).min(width);
        let extra = if row.is_empty() {
            hint_width
        } else {
            separator_width.saturating_add(hint_width)
        };
        if !row.is_empty() && used.saturating_add(extra) > width {
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
        if row.is_empty() {
            used = hint_width;
        } else {
            used = used
                .saturating_add(separator_width)
                .saturating_add(hint_width);
        }
        row.push(hint);
    }
    rows.push(row);
    rows
}

#[cfg(test)]
#[path = "footer_hint_tests.rs"]
mod tests;
