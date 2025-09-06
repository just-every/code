//! Halfblocks protocol implementations.
//! Uses the unicode character `▀` combined with foreground and background color. Assumes that the
//! font aspect ratio is roughly 1:2. Should work in all terminals.
use image::{imageops::FilterType, DynamicImage};
use ratatui::{buffer::Buffer, layout::Rect, style::Color};

use super::{ProtocolTrait, StatefulProtocolTrait};
use crate::Result;

// Fixed Halfblocks protocol
#[derive(Clone, Default)]
pub struct Halfblocks {
    data: Vec<HalfBlock>,
    area: Rect,
}

#[derive(Clone, Debug)]
struct HalfBlock {
    upper: Color,
    lower: Color,
}

impl Halfblocks {
    /// Create a FixedHalfblocks from an image.
    ///
    /// The "resolution" is determined by the font size of the terminal. Smaller fonts will result
    /// in more half-blocks for the same image size. To get a size independent of the font size,
    /// the image could be resized in relation to the font size beforehand.
    /// Also note that the font-size is probably just some arbitrary size with a 1:2 ratio when the
    /// protocol is Halfblocks, and not the actual font size of the terminal.
    pub fn new(image: DynamicImage, area: Rect) -> Result<Self> {
        let data = encode(&image, area);
        Ok(Self { data, area })
    }
}

fn encode(img: &DynamicImage, rect: Rect) -> Vec<HalfBlock> {
    let img = img.resize_exact(
        rect.width as u32,
        (rect.height * 2) as u32,
        FilterType::Triangle,
    );

    let mut data = vec![
        HalfBlock {
            upper: Color::Rgb(0, 0, 0),
            lower: Color::Rgb(0, 0, 0),
        };
        (rect.width * rect.height) as usize
    ];

    for (y, row) in img.to_rgb8().rows().enumerate() {
        for (x, pixel) in row.enumerate() {
            let position = x + (rect.width as usize) * (y / 2);
            if y % 2 == 0 {
                data[position].upper = Color::Rgb(pixel[0], pixel[1], pixel[2]);
            } else {
                data[position].lower = Color::Rgb(pixel[0], pixel[1], pixel[2]);
            }
        }
    }
    data
}

impl ProtocolTrait for Halfblocks {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        for (i, hb) in self.data.iter().enumerate() {
            let x = i as u16 % self.area.width;
            let y = i as u16 / self.area.width;
            if x >= area.width || y >= area.height {
                continue;
            }

            buf.cell_mut((area.x + x, area.y + y))
                .map(|cell| cell.set_fg(hb.upper).set_bg(hb.lower).set_char('▀'));
        }
    }
    fn area(&self) -> Rect {
        self.area
    }
}

impl StatefulProtocolTrait for Halfblocks {
    fn resize_encode(&mut self, img: DynamicImage, area: Rect) -> Result<()> {
        let data = encode(&img, area);
        *self = Halfblocks { data, area };
        Ok(())
    }
}
