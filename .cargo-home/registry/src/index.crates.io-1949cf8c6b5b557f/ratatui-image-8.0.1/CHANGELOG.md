# Changelog

All notable changes to this project will be documented in this file.

# [8.0.1] - 2025-05-30

Fix halfblocks not having the same size as other protocols.

# [8.0.0] - 2025-05-18

Fix `TextSizingProtocol` detection (was incorrectly detecting support on Foot terminal).

Separate "stdio query response" from "capability", as they do not match one-to-one.
Moved `Capability` into the `picker` module, as the interpretation is done there.

# [7.0.0] - 2025-05-18

`Picker` has a new field / method `capabilities`, which return the precise capabilities detected by `Picker::from_query_stdio`.
Since the struct now holds a `Vec`, `Picker` is no longer derives `Copy` (but still derives `Clone`).
This uncovered some unnecessary copying on a method that previously consumed self for no good reason.

Add `Picker::from_query_stdio_with_options` to specifically query for [Text Sizing Protocol](https://sw.kovidgoyal.net/kitty/text-sizing-protocol/#detecting-if-the-terminal-supports-this-protocol).
Not used in this library, but rather an optional convencience feature so that stdio must not be queried twice by some programs.
The capability, if detected, is `Capability::TextSizingProtocol` and can be read from `Picker::capabilities()`.

# [6.0.0] - 2025-05-17

`Image::new` uses a non-mutable borrow.

The mutability was only required for the Kitty protocol, necessary to track wether the image has already been transmitted or only needs placement.

Overall this change should remove the need to use locks when using threads or tokio.

# [5.0.0] - 2025-03-01

Add `StatefulProtocol::size_for`, that can be used to get the size that an image will be rendered to.
This allows positioning the image before it has been rendered, for example centering the image widget with the usual ratatui layout options.

ThreadImage and ThreadProtocol work with `ResizeRequest` and `ResizeResponse` instead of some tuples.
They internally track an ID so that a response for a stale area is discarded correctly.

Huge internal refactor that removes duplicated code across image protocols.

- `Errors` variant case names have been fixed.
- `StatefulProtocol` becomes a struct (was enum).
- `StatefulProtocol::area` which returned the last rendered area has been removed, use `size_for` for accurate results.
- `StatefulProtocol` methods has some parameters removed.
- All protocols except kitty lose their `Stateful...` struct implementation, as one struct can share both protocol implementations.

# [4.2.0] - 2024-12-31 🎆

Fix Sixel and iTerm2 not working with tmux.

# [4.1.2] - 2024-12-30

Add a release job to the CI that makes a github release when a `v*.*.*` is pushed.
The tag itself and pushing to crates.io is done locally.

# [4.1.0] - 2024-12-23 🎄

### Transparency support for Kitty and iTerm2

The image data gains an alpha channel, as well as the background color, which is now transparent by default when the protocol is not Sixel.
The area behind the image is cleared with the control sequences `ECH`, `CUD`, and `CUU` repeatedly for each row and column.
This is not particularly efficient, but it works in most terminals.
`DECERA`, which should erase the entire rectangle with only one sequence, is not implemented correctly (or not at all) in some major terminals.

Sixels could also support transparency, as the spec directly supports it.
A palette color could be set to transparent, and `icy_sixel` in fact supports this, however this is after the fact that the image has been encoded from an API perspective.
In other words, we could set a palette color index to be transparent, but we don't control that this color index would match any transparency of the input image.
This is something that would have to be added to `icy_sixel`.

### Resize: Scale

This feature is brought to you by <taduradnik@proton.me>!
The scale option scales the image, keeping the aspect ratio, to fit the full size of the given area.
It is shown in the demo.

### Capability detection

The control sequence parser has improved its "capability parsing".
In addition to querying Kitty and Sixel protocol support, the "font-size in pixels" is also queried, instead of using `tcgetattr`.
The motivation is that we already need to query stdin, and `tcgetattr` is not supported on Windows Termial, but the control sequence is.

### Minor fixes and changes

- Foreground color is restored after displaying an image with Kitty protocol.
- `Picker` must no longer be mutable.
  It only was mutable so that the kitty image ids would have some sequence, but the start was based off a random number anyway to avoid clashes, so we can also just use a new random id every time.
  This avoids confusion when a new protocol is created, mutating the picker, but discarding the result.
- `area() -> Rect` method on protocols.
  Sometimes it definitely is useful to know how much of the given area a protocol will render to.
- Some logic fixes unnecessary image "needs-resize" calls.

# [3.0.0] - 2024-11-11

Compatible with `ratatui`: `v0.29.0`.

### Windows support

Thanks to Terakomari <im.terakomari@icloud.com>, windows with WSL is now supported.

The latest terminal supports sixels and necessary CSI sequences.
The CI now runs (almost) fully on Windows.

### Font size guessing and CSI parsing

Font size is queried with CSI sequence instead of tcgetattr.
The CSI response parsing has been extended into a full parser module.

### Miscellaneous improvements

- `rustix` is only used on non-windows, is not a crate feature anymore
- Improve magic env var guessing
- Kitty resets only background color
- iTerm2 use PNG intermediate encoding instead of Jpeg
- Use direnv with flake for development

### Static dispatch

Inspired by Uttarayan Mondal <email@uttarayan.me>, changed to static dispatch with an enum instead of a `Box<dyn Protocol>`.
Adding a custom protocol isn't really a use case. Errors are also static now instead of `Box<dyn Error>`.

# [2.0.1] - 2024-10-07

(See git log)
