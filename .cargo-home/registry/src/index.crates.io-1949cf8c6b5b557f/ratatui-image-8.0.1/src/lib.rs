//! # Image widgets with multiple graphics protocol backends for [ratatui]
//!
//! [ratatui] is an immediate-mode TUI library.
//! ratatui-image tackles 3 general problems when rendering images with an immediate-mode TUI:
//!
//! **Query the terminal for available graphics protocols**
//!
//! Some terminals may implement one or more graphics protocols, such as Sixels, or the iTerm2 or
//! Kitty graphics protocols. Guess by env vars. If that fails, query the terminal with some
//! control sequences.
//! Fallback to "halfblocks" which uses some unicode half-block characters with fore- and
//! background colors.
//!
//! **Query the terminal for the font-size in pixels.**
//!
//! If there is an actual graphics protocol available, it is necessary to know the font-size to
//! be able to map the image pixels to character cell area. The image can be resized, fit, or
//! cropped to an area. Query the terminal for the window and columns/rows sizes, and derive the
//! font-size.
//!
//! **Render the image by the means of the guessed protocol.**
//!
//! Some protocols, like Sixels, are essentially "immediate-mode", but we still need to avoid the
//! TUI from overwriting the image area, even with blank characters.
//! Other protocols, like Kitty, are essentially stateful, but at least provide a way to re-render
//! an image that has been loaded, at a different or same position.
//! Since we have the font-size in pixels, we can precisely map the characters/cells/rows-columns that
//! will be covered by the image and skip drawing over the image.
//!
//! # Quick start
//! ```rust
//! use ratatui::{backend::TestBackend, Terminal, Frame};
//! use ratatui_image::{picker::Picker, StatefulImage, protocol::StatefulProtocol};
//!
//! struct App {
//!     // We need to hold the render state.
//!     image: StatefulProtocol,
//! }
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let backend = TestBackend::new(80, 30);
//!     let mut terminal = Terminal::new(backend)?;
//!
//!     // Should use Picker::from_query_stdio() to get the font size and protocol,
//!     // but we can't put that here because that would break doctests!
//!     let mut picker = Picker::from_fontsize((8, 12));
//!
//!     // Load an image with the image crate.
//!     let dyn_img = image::ImageReader::open("./assets/Ada.png")?.decode()?;
//!
//!     // Create the Protocol which will be used by the widget.
//!     let image = picker.new_resize_protocol(dyn_img);
//!
//!     let mut app = App { image };
//!
//!     // This would be your typical `loop {` in a real app:
//!     terminal.draw(|f| ui(f, &mut app))?;
//!     // It is recommended to handle the encoding result
//!     app.image.last_encoding_result().unwrap()?;
//!     Ok(())
//! }
//!
//! fn ui(f: &mut Frame<'_>, app: &mut App) {
//!     // The image widget.
//!     let image = StatefulImage::default();
//!     // Render with the protocol state.
//!     f.render_stateful_widget(image, f.area(), &mut app.image);
//! }
//! ```
//!
//! The [picker::Picker] helper is there to do all this font-size and graphics-protocol guessing,
//! and also to map character-cell-size to pixel size so that we can e.g. "fit" an image inside
//! a desired columns+rows bound, and so on.
//!
//! # Widget choice
//! * The [Image] widget does not adapt to rendering area (except not drawing at all if space
//!   is insufficient), may be a bit more bug prone (overdrawing or artifacts), and is not friendly
//!   with some of the protocols (e.g. the Kitty graphics protocol, which is stateful). Its big
//!   upside is that it is _stateless_ (in terms of ratatui, i.e. immediate-mode), and thus can never
//!   block the rendering thread/task. A lot of ratatui apps only use stateless widgets.
//! * The [StatefulImage] widget adapts to its render area, is more robust against overdraw bugs and
//!   artifacts, and plays nicer with some of the graphics protocols.
//!   The resizing and encoding is blocking by default, but it is possible to offload this to another
//!   thread or async task (see `examples/async.rs`). It must be rendered with
//!   [`render_stateful_widget`] (i.e. with some mutable state).
//!
//! # Examples
//!
//! * `examples/demo.rs` is a fully fledged demo.
//! * `examples/async.rs` shows how to offload resize and encoding to another thread, to avoid
//!   blocking the UI thread.
//!
//! The lib also includes a binary that renders an image file, but it is focused on testing.
//!
//! # Features
//! * `crossterm` or `termion` should match your ratatui backend. `termwiz` is available, but not
//!   working correctly with ratatu-image.
//! * `serde` for `#[derive]`s on [picker::ProtocolType] for convenience, because it might be
//!   useful to save it in some user configuration.
//! * `image-defaults` (default) just enables `image/defaults` (`image` has `default-features =
//! false`). To only support a selection of image formats and cut down dependencies, disable this
//!   feature, add `image` to your crate, and enable its features/formats as desired. See
//!   <https://doc.rust-lang.org/cargo/reference/features.html#feature-unification/>.
//!
//! [ratatui]: https://github.com/ratatui-org/ratatui
//! [sixel]: https://en.wikipedia.org/wiki/Sixel
//! [`render_stateful_widget`]: https://docs.rs/ratatui/latest/ratatui/terminal/struct.Frame.html#method.render_stateful_widget
use std::{
    cmp::{max, min},
    marker::PhantomData,
};

use image::{imageops, DynamicImage, ImageBuffer, Rgba};
use protocol::{ImageSource, Protocol};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{StatefulWidget, Widget},
};

pub mod errors;
pub mod picker;
pub mod protocol;
pub mod thread;
pub use image::imageops::FilterType;

type Result<T> = std::result::Result<T, errors::Errors>;

/// The terminal's font size in `(width, height)`
pub type FontSize = (u16, u16);

/// Fixed size image widget that uses [Protocol].
///
/// The widget does **not** react to area resizes, and is not even guaranteed to **not** overdraw.
/// Its advantage lies in that the [Protocol] needs only one initial resize.
///
/// ```rust
/// # use ratatui::Frame;
/// # use ratatui_image::{Resize, Image, protocol::Protocol};
/// struct App {
///     image_static: Protocol,
/// }
/// fn ui(f: &mut Frame<'_>, app: &mut App) {
///     let image = Image::new(&mut app.image_static);
///     f.render_widget(image, f.size());
/// }
/// ```
pub struct Image<'a> {
    image: &'a Protocol,
}

impl<'a> Image<'a> {
    pub fn new(image: &'a Protocol) -> Self {
        Self { image }
    }
}

impl Widget for Image<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        self.image.render(area, buf);
    }
}

pub trait ResizeEncodeRender {
    /// Resize and encode if necessary, and render immediately.
    fn resize_encode_render(&mut self, resize: &Resize, area: Rect, buf: &mut Buffer) {
        if let Some(rect) = self.needs_resize(resize, area) {
            self.resize_encode(resize, rect);
        }
        self.render(area, buf);
    }

    /// Resize the image and encode it for rendering. The result should be stored statefully so
    /// that next call for the given area does not need to redo the work.
    ///
    /// This can be done in a background thread, and the result is stored in this [protocol::StatefulProtocol].
    fn resize_encode(&mut self, resize: &Resize, area: Rect);

    /// Render the currently resized and encoded data to the buffer.
    fn render(&mut self, area: Rect, buf: &mut Buffer);
    /// Check if the current image state would need resizing (grow or shrink) for the given area.
    ///
    /// This can be called by the UI thread to check if this [protocol::StatefulProtocol] should be sent off
    /// to some background thread/task to do the resizing and encoding, instead of rendering. The
    /// thread should then return the [protocol::StatefulProtocol] so that it can be rendered.
    fn needs_resize(&self, resize: &Resize, area: Rect) -> Option<Rect>;
}

/// Resizeable image widget that uses a [protocol::StatefulProtocol] state.
///
/// This stateful widget reacts to area resizes and resizes its image data accordingly.
///
/// ```rust
/// # use ratatui::Frame;
/// # use ratatui_image::{Resize, StatefulImage, protocol::{StatefulProtocol}};
/// struct App {
///     image_state: StatefulProtocol,
/// }
/// fn ui(f: &mut Frame<'_>, app: &mut App) {
///     let image = StatefulImage::default().resize(Resize::Crop(None));
///     f.render_stateful_widget(
///         image,
///         f.area(),
///         &mut app.image_state,
///     );
/// }
/// ```
pub struct StatefulImage<T>
where
    T: ResizeEncodeRender,
{
    resize: Resize,
    phantom: PhantomData<T>,
}

impl<T> Default for StatefulImage<T>
where
    T: ResizeEncodeRender,
{
    fn default() -> Self {
        Self::new()
    }
}
impl<T> StatefulImage<T>
where
    T: ResizeEncodeRender,
{
    pub const fn resize(self, resize: Resize) -> Self {
        Self { resize, ..self }
    }

    pub const fn new() -> Self {
        Self {
            resize: Resize::Fit(None),
            phantom: PhantomData,
        }
    }
}

impl<T> StatefulWidget for StatefulImage<T>
where
    T: ResizeEncodeRender,
{
    type State = T;
    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        state.resize_encode_render(&self.resize, area, buf);
    }
}

#[derive(Debug, Clone)]
/// Resize method
pub enum Resize {
    /// Fit to area.
    ///
    /// If the width or height is smaller than the area, the image will be resized maintaining
    /// proportions.
    ///
    /// The [FilterType] (re-exported from the [image] crate) defaults to [FilterType::Nearest].
    Fit(Option<FilterType>),
    /// Crop to area.
    ///
    /// If the width or height is smaller than the area, the image will be cropped.
    /// The behaviour is the same as using [`Image`] widget with the overhead of resizing,
    /// but some terminals might misbehave when overdrawing characters over graphics.
    /// For example, the sixel branch of Alacritty never draws text over a cell that is currently
    /// being rendered by some sixel sequence, not necessarily originating from the same cell.
    ///
    /// The [CropOptions] defaults to clipping the bottom and the right sides.
    Crop(Option<CropOptions>),
    /// Scale the image
    ///
    /// Same as `Resize::Fit` except it resizes the image even if the image is smaller than the render area
    Scale(Option<FilterType>),
}

impl Default for Resize {
    fn default() -> Self {
        Self::Fit(None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Specifies which sides to be clipped when cropping an image.
pub struct CropOptions {
    /// If `true`, the top side should be clipped.
    pub clip_top: bool,
    /// If `true`, the left side should be clipped.
    pub clip_left: bool,
}

impl Resize {
    /// Resize [`ImageSource`] to fit the `area`.
    fn resize(
        &self,
        source: &ImageSource,
        font_size: FontSize,
        area: Rect,
        background_color: Rgba<u8>,
    ) -> DynamicImage {
        let width = (area.width * font_size.0) as u32;
        let height = (area.height * font_size.1) as u32;

        // Resize/Crop/etc., fitting a multiple of font-size, but not necessarily the area.
        let mut image = self.resize_image(source, width, height);

        // Always pad to area size with background color, Sixel doesn't have transparency
        // and would get a white background by the sixel library.
        // Once Sixel gets transparency support, only pad
        // `if image.width() != width || image.height() != height`.
        let mut bg: DynamicImage = ImageBuffer::from_pixel(width, height, background_color).into();
        imageops::overlay(&mut bg, &image, 0, 0);
        image = bg;
        image
    }

    /// Check if [`ImageSource`]'s "desired" fits into `area` and is different than `current`.
    ///
    /// The returned `Rect` is the area the image needs to be resized to, depending on the resize
    /// type.
    pub fn needs_resize(
        &self,
        image: &ImageSource,
        font_size: FontSize,
        current: Rect,
        area: Rect,
        force: bool,
    ) -> Option<Rect> {
        let desired = image.desired;
        // Check if resize is needed at all.
        if !force
            && !matches!(self, &Resize::Scale(_))
            && desired.width <= area.width
            && desired.height <= area.height
            && desired == current
        {
            let width = (desired.width * font_size.0) as u32;
            let height = (desired.height * font_size.1) as u32;
            if image.image.width() == width || image.image.height() == height {
                return None;
            }
        }

        let rect = self.render_area(image, font_size, area);
        debug_assert!(rect.width <= area.width, "needs_resize exceeds area width");
        debug_assert!(
            rect.height <= area.height,
            "needs_resize exceeds area height"
        );
        if force || rect != current {
            return Some(rect);
        }
        None
    }

    pub fn render_area(&self, image: &ImageSource, font_size: FontSize, available: Rect) -> Rect {
        let (width, height) = self.needs_resize_pixels(
            &image.image,
            (available.width as u32) * (font_size.0 as u32),
            (available.height as u32) * (font_size.1 as u32),
        );
        ImageSource::round_pixel_size_to_cells(width, height, font_size)
    }

    fn resize_image(&self, source: &ImageSource, width: u32, height: u32) -> DynamicImage {
        const DEFAULT_FILTER_TYPE: FilterType = FilterType::Nearest;
        const DEFAULT_CROP_OPTIONS: CropOptions = CropOptions {
            clip_top: false,
            clip_left: false,
        };
        let image = &source.image;
        match self {
            Self::Fit(filter_type) | Self::Scale(filter_type) => {
                image.resize(width, height, filter_type.unwrap_or(DEFAULT_FILTER_TYPE))
            }
            Self::Crop(options) => {
                let options = options.as_ref().unwrap_or(&DEFAULT_CROP_OPTIONS);
                let y = if options.clip_top {
                    image.height().saturating_sub(height)
                } else {
                    0
                };
                let x = if options.clip_left {
                    image.width().saturating_sub(width)
                } else {
                    0
                };
                image.crop_imm(x, y, width, height)
            }
        }
    }

    fn needs_resize_pixels(&self, image: &DynamicImage, width: u32, height: u32) -> (u32, u32) {
        match self {
            Self::Fit(_) => fit_area_proportionally(
                image.width(),
                image.height(),
                min(width, image.width()),
                min(height, image.height()),
            ),

            Self::Crop(_) => (min(image.width(), width), min(image.height(), height)),
            Self::Scale(_) => fit_area_proportionally(image.width(), image.height(), width, height),
        }
    }
}

/// Ripped from https://github.com/image-rs/image/blob/master/src/math/utils.rs#L12
/// Calculates the width and height an image should be resized to.
/// This preserves aspect ratio, and based on the `fill` parameter
/// will either fill the dimensions to fit inside the smaller constraint
/// (will overflow the specified bounds on one axis to preserve
/// aspect ratio), or will shrink so that both dimensions are
/// completely contained within the given `width` and `height`,
/// with empty space on one axis.
fn fit_area_proportionally(width: u32, height: u32, nwidth: u32, nheight: u32) -> (u32, u32) {
    let wratio = nwidth as f64 / width as f64;
    let hratio = nheight as f64 / height as f64;

    let ratio = f64::min(wratio, hratio);

    let nw = max((width as f64 * ratio).round() as u64, 1);
    let nh = max((height as f64 * ratio).round() as u64, 1);

    if nw > u64::from(u16::MAX) {
        let ratio = u16::MAX as f64 / width as f64;
        (u32::MAX, max((height as f64 * ratio).round() as u32, 1))
    } else if nh > u64::from(u16::MAX) {
        let ratio = u16::MAX as f64 / height as f64;
        (max((width as f64 * ratio).round() as u32, 1), u32::MAX)
    } else {
        (nw as u32, nh as u32)
    }
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgba};

    use super::*;

    const FONT_SIZE: FontSize = (10, 10);

    fn s(w: u16, h: u16) -> ImageSource {
        let image: DynamicImage =
            ImageBuffer::from_pixel(w as _, h as _, Rgba::<u8>([255, 0, 0, 255])).into();
        ImageSource::new(image, FONT_SIZE, [0, 0, 0, 0].into())
    }

    fn r(w: u16, h: u16) -> Rect {
        Rect::new(0, 0, w, h)
    }

    #[test]
    fn needs_resize_fit() {
        let resize = Resize::Fit(None);

        let to = resize.needs_resize(&s(100, 100), FONT_SIZE, r(10, 10), r(10, 10), false);
        assert_eq!(None, to);

        let to = resize.needs_resize(&s(101, 101), FONT_SIZE, r(10, 10), r(10, 10), false);
        assert_eq!(None, to);

        let to = resize.needs_resize(&s(80, 100), FONT_SIZE, r(8, 10), r(10, 10), false);
        assert_eq!(None, to);

        let to = resize.needs_resize(&s(100, 100), FONT_SIZE, r(99, 99), r(8, 10), false);
        assert_eq!(Some(r(8, 8)), to);

        let to = resize.needs_resize(&s(100, 100), FONT_SIZE, r(99, 99), r(10, 8), false);
        assert_eq!(Some(r(8, 8)), to);

        let to = resize.needs_resize(&s(100, 50), FONT_SIZE, r(99, 99), r(4, 4), false);
        assert_eq!(Some(r(4, 2)), to);

        let to = resize.needs_resize(&s(50, 100), FONT_SIZE, r(99, 99), r(4, 4), false);
        assert_eq!(Some(r(2, 4)), to);

        let to = resize.needs_resize(&s(100, 100), FONT_SIZE, r(8, 8), r(11, 11), false);
        assert_eq!(Some(r(10, 10)), to);

        let to = resize.needs_resize(&s(100, 100), FONT_SIZE, r(10, 10), r(11, 11), false);
        assert_eq!(None, to);
    }

    #[test]
    fn needs_resize_crop() {
        let resize = Resize::Crop(None);

        let to = resize.needs_resize(&s(100, 100), FONT_SIZE, r(10, 10), r(10, 10), false);
        assert_eq!(None, to);

        let to = resize.needs_resize(&s(80, 100), FONT_SIZE, r(8, 10), r(10, 10), false);
        assert_eq!(None, to);

        let to = resize.needs_resize(&s(100, 100), FONT_SIZE, r(10, 10), r(8, 10), false);
        assert_eq!(Some(r(8, 10)), to);

        let to = resize.needs_resize(&s(100, 100), FONT_SIZE, r(10, 10), r(10, 8), false);
        assert_eq!(Some(r(10, 8)), to);
    }
}
