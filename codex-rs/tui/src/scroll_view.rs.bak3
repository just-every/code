use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::{Block, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget, Widget},
};

/// A generic scrollable viewport for any Widget.
/// You must provide the content height (in rows) for the current width.
#[derive(Clone)]
pub struct ScrollView<W> {
    inner: W,
    /// total logical content size
    pub content_height: usize,
    pub content_width: Option<usize>, // if you can measure it; otherwise None
    /// scroll offsets
    pub scroll_y: usize,
    pub scroll_x: usize,
    /// optional decorations
    pub block: Option<Block<'static>>,
    pub show_scrollbar: bool,
    pub scrollbar_style: Style,
}

impl<W> ScrollView<W> {
    pub fn new(inner: W, content_height: usize) -> Self {
        Self {
            inner,
            content_height,
            content_width: None,
            scroll_y: 0,
            scroll_x: 0,
            block: None,
            show_scrollbar: false, // We handle scrollbar separately in ChatWidget
            scrollbar_style: Style::default(),
        }
    }

    #[allow(dead_code)]
    pub fn block(mut self, block: Block<'static>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn scroll_y(mut self, y: usize) -> Self {
        self.scroll_y = y;
        self
    }

    #[allow(dead_code)]
    pub fn scroll_x(mut self, x: usize) -> Self {
        self.scroll_x = x;
        self
    }

    #[allow(dead_code)]
    pub fn show_scrollbar(mut self, yes: bool) -> Self {
        self.show_scrollbar = yes;
        self
    }

    #[allow(dead_code)]
    pub fn scrollbar_style(mut self, style: Style) -> Self {
        self.scrollbar_style = style;
        self
    }
}

impl<W> Widget for ScrollView<W>
where
    W: Widget + Clone, // render takes ownership; Clone keeps ergonomics
{
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Apply outer block (optional)
        let mut inner_area = area;
        if let Some(block) = &self.block {
            block.clone().render(area, buf);
            inner_area = block.inner(area);
            if inner_area.height == 0 || inner_area.width == 0 {
                return;
            }
        }

        let viewport_h = inner_area.height as usize;
        let viewport_w = inner_area.width as usize;

        // Clamp scroll positions
        let max_y = self.content_height.saturating_sub(viewport_h);
        let sy = self.scroll_y.min(max_y);
        let sx = self
            .scroll_x
            .min(self.content_width.unwrap_or(0).saturating_sub(viewport_w));

        // Offscreen render: pretend we have a huge height = content_height
        // (Width = viewport width so we don't waste memory horizontally)
        let virtual_h = self.content_height.max(viewport_h) as u16;
        let virtual_area = Rect {
            x: 0,
            y: 0,
            width: inner_area.width,
            height: virtual_h,
        };

        // Create an offscreen buffer and render the child into it
        let mut off = Buffer::empty(virtual_area);
        self.inner.clone().render(virtual_area, &mut off);

        // Blit the visible window [sy..sy+viewport_h] and [sx..sx+viewport_w] into real buffer
        for row in 0..viewport_h.min(self.content_height.saturating_sub(sy)) {
            for col in 0..viewport_w {
                let src_x = sx + col;
                // If content_width is unknown, assume child draws within viewport width
                if let Some(cw) = self.content_width {
                    if src_x >= cw {
                        continue;
                    }
                }
                let src_y = sy + row;
                if let Some(src_cell) = off.cell((src_x as u16, src_y as u16)) {
                    let dst_x = inner_area.x + col as u16;
                    let dst_y = inner_area.y + row as u16;
                    if let Some(dst_cell) = buf.cell_mut((dst_x, dst_y)) {
                        *dst_cell = src_cell.clone();
                    }
                }
            }
        }

        // Optional scrollbar on the right
        if self.show_scrollbar && self.content_height > viewport_h {
            let mut state = ScrollbarState::new(self.content_height)
                .position(sy)
                .viewport_content_length(viewport_h);
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(self.scrollbar_style)
                .render(inner_area, buf, &mut state);
        }
    }
}
