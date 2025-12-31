use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;

use crate::history::state::HistoryId;
use crate::history_cell::{HistoryCell, HistoryCellType};
use crate::util::buffer::fill_rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrozenHistoryCell {
    history_id: HistoryId,
    kind: HistoryCellType,
    cached_width: u16,
    cached_height: u16,
}

impl FrozenHistoryCell {
    pub(crate) fn new(
        history_id: HistoryId,
        kind: HistoryCellType,
        cached_width: u16,
        cached_height: u16,
    ) -> Self {
        Self {
            history_id,
            kind,
            cached_width,
            cached_height,
        }
    }

    pub(crate) fn history_id(&self) -> HistoryId {
        self.history_id
    }

    pub(crate) fn cached_width(&self) -> u16 {
        self.cached_width
    }

    pub(crate) fn cached_height(&self) -> u16 {
        self.cached_height
    }

    pub(crate) fn update_cached_height(&mut self, width: u16, height: u16) {
        self.cached_width = width;
        self.cached_height = height;
    }
}

impl HistoryCell for FrozenHistoryCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        Vec::new()
    }

    fn kind(&self) -> HistoryCellType {
        self.kind
    }

    fn desired_height(&self, _width: u16) -> u16 {
        self.cached_height
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, _skip_rows: u16) {
        let bg_style = Style::default().bg(crate::colors::background());
        fill_rect(buf, area, Some(' '), bg_style);
    }
}
