//! Small helper to compute an anchored list window for a selection.
//! Given a total `count`, selected index, and `max_visible` rows (odd preferred),
//! returns `(start, visible, middle)` where `start` is the first index to render.

pub fn anchored_window(selected: usize, count: usize, max_visible: usize) -> (usize, usize, usize) {
    if count == 0 || max_visible == 0 {
        return (0, 0, 0);
    }
    let visible = max_visible.min(count).max(1);
    let middle = visible / 2; // centered index within the window
    let start = selected
        .saturating_sub(middle)
        .min(count.saturating_sub(visible));
    (start, visible, middle)
}
