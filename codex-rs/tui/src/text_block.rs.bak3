use ratatui::text::Line;

/// Simple wrapper around Vec<Line> for holding text content
#[derive(Clone, Debug)]
pub(crate) struct TextBlock {
    pub lines: Vec<Line<'static>>,
}

impl TextBlock {
    pub fn new(lines: Vec<Line<'static>>) -> Self {
        Self { lines }
    }
}