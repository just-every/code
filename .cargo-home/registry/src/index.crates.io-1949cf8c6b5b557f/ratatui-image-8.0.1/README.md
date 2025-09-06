# Ratatui-image

[![CI Badge]][CI]
[![Crate Badge]][Crate]
[![Docs Badge]][Docs]

[CI Badge]: https://img.shields.io/github/actions/workflow/status/benjajaja/ratatui-image/ci.yaml?style=flat-square&logo=github
[CI]: https://github.com/benjajaja/ratatui-image/actions?query=workflow%3A
[Crate Badge]: https://img.shields.io/crates/v/ratatui-image?logo=rust&style=flat-square
[Crate]: https://crates.io/crates/ratatui-image
[Docs Badge]: https://img.shields.io/docsrs/ratatui-image?logo=rust&style=flat-square
[Docs]: https://docs.rs/ratatui-image/latest/ratatui_image/index.html

### Showcase:

![Screen recording](./assets/showcase.gif)

## Image widgets with multiple graphics protocol backends for [ratatui]

[ratatui] is an immediate-mode TUI library.
ratatui-image tackles 3 general problems when rendering images with an immediate-mode TUI:

**Query the terminal for available graphics protocols**

Some terminals may implement one or more graphics protocols, such as Sixels, or the iTerm2 or
Kitty graphics protocols. Guess by env vars. If that fails, query the terminal with some
control sequences.
Fallback to "halfblocks" which uses some unicode half-block characters with fore- and
background colors.

**Query the terminal for the font-size in pixels.**

If there is an actual graphics protocol available, it is necessary to know the font-size to
be able to map the image pixels to character cell area. The image can be resized, fit, or
cropped to an area. Query the terminal for the window and columns/rows sizes, and derive the
font-size.

**Render the image by the means of the guessed protocol.**

Some protocols, like Sixels, are essentially "immediate-mode", but we still need to avoid the
TUI from overwriting the image area, even with blank characters.
Other protocols, like Kitty, are essentially stateful, but at least provide a way to re-render
an image that has been loaded, at a different or same position.
Since we have the font-size in pixels, we can precisely map the characters/cells/rows-columns that
will be covered by the image and skip drawing over the image.

## Quick start
```rust
use ratatui::{backend::TestBackend, Terminal, Frame};
use ratatui_image::{picker::Picker, StatefulImage, protocol::StatefulProtocol};

struct App {
    // We need to hold the render state.
    image: StatefulProtocol,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend = TestBackend::new(80, 30);
    let mut terminal = Terminal::new(backend)?;

    // Should use Picker::from_query_stdio() to get the font size and protocol,
    // but we can't put that here because that would break doctests!
    let mut picker = Picker::from_fontsize((8, 12));

    // Load an image with the image crate.
    let dyn_img = image::ImageReader::open("./assets/Ada.png")?.decode()?;

    // Create the Protocol which will be used by the widget.
    let image = picker.new_resize_protocol(dyn_img);

    let mut app = App { image };

    // This would be your typical `loop {` in a real app:
    terminal.draw(|f| ui(f, &mut app))?;
    // It is recommended to handle the encoding result
    app.image.last_encoding_result().unwrap()?;
    Ok(())
}

fn ui(f: &mut Frame<'_>, app: &mut App) {
    // The image widget.
    let image = StatefulImage::default();
    // Render with the protocol state.
    f.render_stateful_widget(image, f.area(), &mut app.image);
}
```

The [picker::Picker] helper is there to do all this font-size and graphics-protocol guessing,
and also to map character-cell-size to pixel size so that we can e.g. "fit" an image inside
a desired columns+rows bound, and so on.

## Widget choice
* The [Image] widget does not adapt to rendering area (except not drawing at all if space
  is insufficient), may be a bit more bug prone (overdrawing or artifacts), and is not friendly
  with some of the protocols (e.g. the Kitty graphics protocol, which is stateful). Its big
  upside is that it is _stateless_ (in terms of ratatui, i.e. immediate-mode), and thus can never
  block the rendering thread/task. A lot of ratatui apps only use stateless widgets.
* The [StatefulImage] widget adapts to its render area, is more robust against overdraw bugs and
  artifacts, and plays nicer with some of the graphics protocols.
  The resizing and encoding is blocking by default, but it is possible to offload this to another
  thread or async task (see `examples/async.rs`). It must be rendered with
  [`render_stateful_widget`] (i.e. with some mutable state).

## Examples

* `examples/demo.rs` is a fully fledged demo.
* `examples/async.rs` shows how to offload resize and encoding to another thread, to avoid
  blocking the UI thread.

The lib also includes a binary that renders an image file, but it is focused on testing.

## Features
* `crossterm` or `termion` should match your ratatui backend. `termwiz` is available, but not
  working correctly with ratatu-image.
* `serde` for `#[derive]`s on [picker::ProtocolType] for convenience, because it might be
  useful to save it in some user configuration.
* `image-defaults` (default) just enables `image/defaults` (`image` has `default-features =
false`). To only support a selection of image formats and cut down dependencies, disable this
  feature, add `image` to your crate, and enable its features/formats as desired. See
  <https://doc.rust-lang.org/cargo/reference/features.html#feature-unification/>.

[ratatui]: https://github.com/ratatui-org/ratatui
[sixel]: https://en.wikipedia.org/wiki/Sixel
[`render_stateful_widget`]: https://docs.rs/ratatui/latest/ratatui/terminal/struct.Frame.html#method.render_stateful_widget

### Compatibility matrix

Compatibility and QA:

Terminal  | Protocol | OK | Notes
----------|----------|----|-------
Xterm     | `Sixel`  | ✔️ | Run with `-ti 340` to make sure sixel support is enabled.
Foot      | `Sixel`  | ✔️ | Wayland.
Kitty     | `Kitty`  | ✔️ | Reference for the `Kitty` protocol (requires Kitty 0.28.0 or later).
Wezterm   | `iTerm2` | ✔️ | Also would support `Sixel` and `Kitty`, but only `iTerm2` actually works bug-free.
Ghostty   | `Kitty`  | ✔️ | Implements `Kitty` with unicode placeholders.
iTerm2    | `iTerm2` | ✔️ | Reference for the `iTerm2` protocol. Mac only.
Rio       | `iTerm2` | ✔️ | Also supports `Sixel` but has glitches.
mlterm    | `Sixel`  | ✔️ | Quite slow but no glitches.
Black Box | `Sixel`  | ✔️ | Confirmed only with the flatpak version, most distro packages don't enable Sixel support.
Alacritty | `Sixel`  | ❌ | [There is a sixel fork](https://github.com/microo8/alacritty-sixel), but it's probably never getting merged, and does not clear graphics.
Konsole   | `Sixel`  | ❌ | [Not really fixed in 24.12](https://bugs.kde.org/show_bug.cgi?id=456354)
Contour   | `Sixel`  | ❌ | Does not clear graphics.
ctx       | `Sixel`  | ❌ | Buggy.

A basic [screenshot test](./assets/screenshot_xterm.png) is run with xterm on Xvfb in the CI (or `cargo make screenshot-xvfb && cargo make screenshot-diff`).

Halfblocks should work in all terminals, even if the font size could not be detected, with a 4:8 pixel ratio.

### Known issues
Summary | Link
--------|---------
Termwiz backend does not work at all | [#1](https://github.com/benjajaja/ratatui-image/issues/1)
Sixel image rendered on the last line of terminal causes a scroll | [#57](https://github.com/benjajaja/ratatui-image/issues/57)
Terminals may or may not take DPI scale into account | [#59 (closed)](https://github.com/benjajaja/ratatui-image/issues/59)

### Projects that use ratatui-image

* [mdfried](https://github.com/benjajaja/mdfried)
  A markdown viewer that renders headers bigger (as images), and regular images too.
* [iamb](https://github.com/ulyssa/iamb)
  A matrix client with vim keybindings.
* [joshuto](https://github.com/kamiyaa/joshuto)
  A terminal file manager that can preview images.
* [Aerostream](https://github.com/shigepon7/aerostream)
  A Bluesky client using EventStream.

Many more, see ![crate dependants](https://crates.io/crates/ratatui-image/reverse_dependencies)
and ![github dependency graph](https://github.com/benjajaja/ratatui-image/network/dependencies)
(note that github includes a huge number of unrelated dotfile repos).

### Comparison

* [viuer](https://crates.io/crates/viuer)
  Renders graphics in different terminals/protocols, but "dumps" the image, making it difficult to
  work for TUI programs.
  The terminal protocol guessing code has been adapted to rustix, thus the author of viuer is
  included in the copyright notice.
* [yazi](https://github.com/sxyazi/yazi)
  Not a library but a terminal file manager that implementes many graphics protocols and lets you
  preview images in the filesystem.
* [Überzug++](https://github.com/jstkdng/ueberzugpp)
  CLI utility that draws images on terminals by using X11/wayland child windows, sixels, kitty,
  and/or iterm2 protocols (any means necessary). There exists several wrapper or bindings crates.
  More battle-tested but essentially stateful, which makes it hard to use with immediate-mode.

### Contributing

PRs and issues/discussions welcome!

There are some specific rules for a PR to be reviewed at all, please see [CONTRIBUTING.md](CONTRIBUTING.md) for reference.

License: MIT
