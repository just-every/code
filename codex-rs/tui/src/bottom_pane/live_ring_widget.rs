use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::WidgetRef;

/// Minimal rendering-only widget for the transient ring rows.
pub(crate) struct LiveRingWidget {
    max_rows: u16,
    rows: Vec<Line<'static>>, // newest at the end
}

impl LiveRingWidget {
    #[cfg(all(test, feature = "legacy_tests"))]
    pub fn new(max_rows: usize, rows: Vec<Line<'static>>) -> Self {
        Self {
            max_rows: max_rows as u16,
            rows,
        }
    }

    pub fn desired_height(&self, _width: u16) -> u16 {
        let len = self.rows.len() as u16;
        len.min(self.max_rows)
    }
}

impl WidgetRef for LiveRingWidget {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }
        let visible = self.rows.len().saturating_sub(self.max_rows as usize);
        let slice = &self.rows[visible..];
        let para = Paragraph::new(slice.to_vec());
        para.render_ref(area, buf);
    }
}
