//! Sixel protocol implementations.
//! Uses [`sixel-bytes`] to draw image pixels, if the terminal [supports] the [Sixel] protocol.
//! Needs the `sixel` feature.
//!
//! [`sixel-bytes`]: https://github.com/benjajaja/sixel-bytes
//! [supports]: https://arewesixelyet.com
//! [Sixel]: https://en.wikipedia.org/wiki/Sixel
use icy_sixel::{
    sixel_string, DiffusionMethod, MethodForLargest, MethodForRep, PixelFormat, Quality,
};
use image::DynamicImage;
use ratatui::{buffer::Buffer, layout::Rect};
use std::cmp::min;

use super::{ProtocolTrait, StatefulProtocolTrait};
use crate::{errors::Errors, picker::cap_parser::Parser, Result};

// Fixed sixel protocol
#[derive(Clone, Default)]
pub struct Sixel {
    pub data: String,
    pub area: Rect,
    pub is_tmux: bool,
}

impl Sixel {
    pub fn new(image: DynamicImage, area: Rect, is_tmux: bool) -> Result<Self> {
        let data = encode(&image, is_tmux)?;
        Ok(Self {
            data,
            area,
            is_tmux,
        })
    }
}

// TODO: change E to sixel_rs::status::Error and map when calling
fn encode(img: &DynamicImage, is_tmux: bool) -> Result<String> {
    let (w, h) = (img.width(), img.height());
    let img_rgb8 = img.to_rgb8();
    let bytes = img_rgb8.as_raw();

    let mut data = sixel_string(
        bytes,
        w as i32,
        h as i32,
        PixelFormat::RGB888,
        DiffusionMethod::Stucki,
        MethodForLargest::Auto,
        MethodForRep::Auto,
        Quality::HIGH,
    )
    .map_err(|err| Errors::Sixel(err.to_string()))?;

    if is_tmux {
        let (start, escape, end) = Parser::escape_tmux(is_tmux);
        if data.strip_prefix('\x1b').is_none() {
            return Err(Errors::Tmux("sixel string did not start with escape"));
        }

        data.insert_str(0, escape);
        data.insert_str(0, start);
        data.push_str(end);
    }
    Ok(data)
}

impl ProtocolTrait for Sixel {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        render(self.area, &self.data, area, buf, false)
    }

    fn area(&self) -> Rect {
        self.area
    }
}

fn render(rect: Rect, data: &str, area: Rect, buf: &mut Buffer, overdraw: bool) {
    let render_area = match render_area(rect, area, overdraw) {
        None => {
            // If we render out of area, then the buffer will attempt to write regular text (or
            // possibly other sixels) over the image.
            //
            // On some implementations (e.g. Xterm), this actually works but the image is
            // forever overwritten since we won't write out the same sixel data for the same
            // (col,row) position again (see buffer diffing).
            // Thus, when the area grows, the newly available cells will skip rendering and
            // leave artifacts instead of the image data.
            //
            // On some implementations (e.g. ???), only text with its foreground color is
            // overlayed on the image, also forever overwritten.
            //
            // On some implementations (e.g. patched Alactritty), image graphics are never
            // overwritten and simply draw over other UI elements.
            //
            // Note that [ResizeProtocol] forces to ignore this early return, since it will
            // always resize itself to the area.
            return;
        }
        Some(r) => r,
    };

    buf.cell_mut(render_area).map(|cell| cell.set_symbol(data));
    let mut skip_first = false;

    // Skip entire area
    for y in render_area.top()..render_area.bottom() {
        for x in render_area.left()..render_area.right() {
            if !skip_first {
                skip_first = true;
                continue;
            }
            buf.cell_mut((x, y)).map(|cell| cell.set_skip(true));
        }
    }
}

fn render_area(rect: Rect, area: Rect, overdraw: bool) -> Option<Rect> {
    if overdraw {
        return Some(Rect::new(
            area.x,
            area.y,
            min(rect.width, area.width),
            min(rect.height, area.height),
        ));
    }

    if rect.width > area.width || rect.height > area.height {
        return None;
    }
    Some(Rect::new(area.x, area.y, rect.width, rect.height))
}

impl StatefulProtocolTrait for Sixel {
    fn resize_encode(&mut self, img: DynamicImage, area: Rect) -> Result<()> {
        let data = encode(&img, self.is_tmux)?;
        *self = Sixel {
            data,
            area,
            ..*self
        };
        Ok(())
    }
}
