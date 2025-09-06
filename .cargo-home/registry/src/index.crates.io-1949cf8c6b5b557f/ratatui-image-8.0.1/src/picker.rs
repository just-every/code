//! Helper module to build a protocol, and swap protocols at runtime

use std::{
    env,
    io::{self, Read, Write},
    time::Duration,
};

use crate::{
    errors::Errors,
    protocol::{
        halfblocks::Halfblocks,
        iterm2::Iterm2,
        kitty::{Kitty, StatefulKitty},
        sixel::Sixel,
        Protocol, StatefulProtocol, StatefulProtocolType,
    },
    FontSize, ImageSource, Resize, Result,
};
use cap_parser::{Parser, QueryStdioOptions, Response};
use image::{DynamicImage, Rgba};
use rand::random;
use ratatui::layout::Rect;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub mod cap_parser;

#[derive(Debug, PartialEq, Clone)]
pub enum Capability {
    /// Reports supporting kitty graphics protocol.
    Kitty,
    /// Reports supporting sixel graphics protocol.
    Sixel,
    /// Reports supporting rectangular ops.
    RectangularOps,
    /// Reports font size in pixels.
    CellSize(Option<(u16, u16)>),
    /// Reports supporting text sizing protocol.
    TextSizingProtocol,
}

const DEFAULT_BACKGROUND: Rgba<u8> = Rgba([0, 0, 0, 0]);

#[derive(Clone, Debug)]
pub struct Picker {
    font_size: FontSize,
    protocol_type: ProtocolType,
    background_color: Rgba<u8>,
    is_tmux: bool,
    capabilities: Vec<Capability>,
}

/// Serde-friendly protocol-type enum for [Picker].
#[derive(PartialEq, Clone, Debug, Copy)]
#[cfg_attr(
    feature = "serde",
    derive(Deserialize, Serialize),
    serde(rename_all = "lowercase")
)]
pub enum ProtocolType {
    Halfblocks,
    Sixel,
    Kitty,
    Iterm2,
}

impl ProtocolType {
    pub fn next(&self) -> ProtocolType {
        match self {
            ProtocolType::Halfblocks => ProtocolType::Sixel,
            ProtocolType::Sixel => ProtocolType::Kitty,
            ProtocolType::Kitty => ProtocolType::Iterm2,
            ProtocolType::Iterm2 => ProtocolType::Halfblocks,
        }
    }
}

/// Helper for building widgets
impl Picker {
    /// Query terminal stdio for graphics capabilities and font-size with some escape sequences.
    ///
    /// This writes and reads from stdio momentarily. WARNING: this method should be called after
    /// entering alternate screen but before reading terminal events.
    ///
    /// # Example
    /// ```rust
    /// use ratatui_image::picker::Picker;
    /// let mut picker = Picker::from_query_stdio();
    /// ```
    ///
    pub fn from_query_stdio() -> Result<Self> {
        Picker::from_query_stdio_with_options(QueryStdioOptions {
            text_sizing_protocol: false,
        })
    }

    /// This should ONLY be used if [Capability::TextSizingProtocol] is needed for some external
    /// reason.
    ///
    /// Query for additional capabilities, currently supports querying for [Text Sizing Protocol].
    ///
    /// The result can be checked by searching for [Capability::TextSizingProtocol] in [Picker::capabilities].
    ///
    /// [Text Sizing Protocol] <https://sw.kovidgoyal.net/kitty/text-sizing-protocol//>
    pub fn from_query_stdio_with_options(options: QueryStdioOptions) -> Result<Self> {
        // Detect tmux, and only if positive then take some risky guess for iTerm2 support.
        let (is_tmux, tmux_proto) = detect_tmux_and_outer_protocol_from_env();

        // Write and read to stdin to query protocol capabilities and font-size.
        match query_with_timeout(is_tmux, Duration::from_secs(1), options) {
            Ok((capability_proto, font_size, caps)) => {
                // If some env var says that we should try iTerm2, then disregard protocol-from-capabilities.
                let iterm2_proto = iterm2_from_env();

                let protocol_type = tmux_proto
                    .or(iterm2_proto)
                    .or(capability_proto)
                    .unwrap_or(ProtocolType::Halfblocks);

                if let Some(font_size) = font_size {
                    Ok(Self {
                        font_size,
                        background_color: DEFAULT_BACKGROUND,
                        protocol_type,
                        is_tmux,
                        capabilities: caps,
                    })
                } else {
                    Err(Errors::NoFontSize)
                }
            }
            Err(Errors::NoCap) => Ok(Self {
                // This is completely arbitrary. For halfblocks, it doesn't have to be precise
                // since we're not rendering pixels. It should be roughly 1:2 ratio, and some
                // reasonable size.
                font_size: (10, 20),
                background_color: DEFAULT_BACKGROUND,
                protocol_type: ProtocolType::Halfblocks,
                is_tmux,
                capabilities: Vec::new(),
            }),
            Err(err) => Err(err),
        }
    }

    /// Create a picker from a given terminal [FontSize].
    /// This is the only way to create a picker on windows, for now.
    ///
    /// # Example
    /// ```rust
    /// use ratatui_image::picker::Picker;
    ///
    /// let user_fontsize = (7, 14);
    ///
    /// let mut picker = Picker::from_fontsize(user_fontsize);
    /// ```
    pub fn from_fontsize(font_size: FontSize) -> Self {
        // Detect tmux, and if positive then take some risky guess for iTerm2 support.
        let (is_tmux, tmux_proto) = detect_tmux_and_outer_protocol_from_env();

        // Disregard protocol-from-capabilities if some env var says that we could try iTerm2.
        let iterm2_proto = iterm2_from_env();

        let protocol_type = tmux_proto
            .or(iterm2_proto)
            .unwrap_or(ProtocolType::Halfblocks);

        Self {
            font_size,
            background_color: DEFAULT_BACKGROUND,
            protocol_type,
            is_tmux,
            capabilities: Vec::new(),
        }
    }

    /// Returns the current protocol type.
    pub fn protocol_type(&self) -> ProtocolType {
        self.protocol_type
    }

    /// Force a protocol type.
    pub fn set_protocol_type(&mut self, protocol_type: ProtocolType) {
        self.protocol_type = protocol_type;
    }

    /// Returns the [FontSize] detected by [Picker::from_query_stdio].
    pub fn font_size(&self) -> FontSize {
        self.font_size
    }

    /// Change the default background color (transparent black).
    pub fn set_background_color<T: Into<Rgba<u8>>>(&mut self, background_color: T) {
        self.background_color = background_color.into();
    }

    /// Returns the capabilities detected by [Picker::from_query_stdio].
    pub fn capabilities(&self) -> &Vec<Capability> {
        &self.capabilities
    }

    /// Returns a new protocol for [`crate::Image`] widgets that fits into the given size.
    pub fn new_protocol(
        &self,
        image: DynamicImage,
        size: Rect,
        resize: Resize,
    ) -> Result<Protocol> {
        let source = ImageSource::new(image, self.font_size, self.background_color);

        let (image, area) =
            match resize.needs_resize(&source, self.font_size, source.desired, size, false) {
                Some(area) => {
                    let image = resize.resize(&source, self.font_size, area, self.background_color);
                    (image, area)
                }
                None => (source.image, source.desired),
            };

        match self.protocol_type {
            ProtocolType::Halfblocks => Ok(Protocol::Halfblocks(Halfblocks::new(image, area)?)),
            ProtocolType::Sixel => Ok(Protocol::Sixel(Sixel::new(image, area, self.is_tmux)?)),
            ProtocolType::Kitty => Ok(Protocol::Kitty(Kitty::new(
                image,
                area,
                rand::random(),
                self.is_tmux,
            )?)),
            ProtocolType::Iterm2 => Ok(Protocol::ITerm2(Iterm2::new(image, area, self.is_tmux)?)),
        }
    }

    /// Returns a new *stateful* protocol for [`crate::StatefulImage`] widgets.
    pub fn new_resize_protocol(&self, image: DynamicImage) -> StatefulProtocol {
        let source = ImageSource::new(image, self.font_size, self.background_color);
        let protocol_type = match self.protocol_type {
            ProtocolType::Halfblocks => StatefulProtocolType::Halfblocks(Halfblocks::default()),
            ProtocolType::Sixel => StatefulProtocolType::Sixel(Sixel {
                is_tmux: self.is_tmux,
                ..Sixel::default()
            }),
            ProtocolType::Kitty => {
                StatefulProtocolType::Kitty(StatefulKitty::new(random(), self.is_tmux))
            }
            ProtocolType::Iterm2 => StatefulProtocolType::ITerm2(Iterm2 {
                is_tmux: self.is_tmux,
                ..Iterm2::default()
            }),
        };
        StatefulProtocol::new(source, self.font_size, protocol_type)
    }
}

fn detect_tmux_and_outer_protocol_from_env() -> (bool, Option<ProtocolType>) {
    // Check if we're inside tmux.
    if !env::var("TERM").is_ok_and(|term| term.starts_with("tmux"))
        && !env::var("TERM_PROGRAM").is_ok_and(|term_program| term_program == "tmux")
    {
        return (false, None);
    }

    let _ = std::process::Command::new("tmux")
        .args(["set", "-p", "allow-passthrough", "on"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut child| child.wait()); // wait(), for check_device_attrs.

    // Crude guess based on the *existence* of some magic program specific env vars.
    // Produces false positives, for example xterm started from kitty inherits KITTY_WINDOW_ID.
    // Furthermore, tmux shares env vars from the first session, for example tmux started in xterm
    // after a previous tmux session started in kitty, inherits KITTY_WINDOW_ID.
    const OUTER_TERM_HINTS: [(&str, ProtocolType); 3] = [
        ("KITTY_WINDOW_ID", ProtocolType::Kitty), // TODO: query should work inside tmux, remove?
        ("ITERM_SESSION_ID", ProtocolType::Iterm2),
        ("WEZTERM_EXECUTABLE", ProtocolType::Iterm2),
    ];
    for (hint, proto) in OUTER_TERM_HINTS {
        if env::var(hint).is_ok_and(|s| !s.is_empty()) {
            return (true, Some(proto));
        }
    }
    (true, None)
}

fn iterm2_from_env() -> Option<ProtocolType> {
    if env::var("TERM_PROGRAM").is_ok_and(|term_program| {
        term_program.contains("iTerm")
            || term_program.contains("WezTerm")
            || term_program.contains("mintty")
            || term_program.contains("vscode")
            || term_program.contains("Tabby")
            || term_program.contains("Hyper")
            || term_program.contains("rio")
    }) {
        return Some(ProtocolType::Iterm2);
    }
    if env::var("LC_TERMINAL").is_ok_and(|lc_term| lc_term.contains("iTerm")) {
        return Some(ProtocolType::Iterm2);
    }
    None
}

#[cfg(not(windows))]
fn enable_raw_mode() -> Result<impl FnOnce() -> Result<()>> {
    use rustix::termios::{self, LocalModes, OptionalActions};

    let stdin = io::stdin();
    let mut termios = termios::tcgetattr(&stdin)?;
    let termios_original = termios.clone();

    // Disable canonical mode to read without waiting for Enter, disable echoing.
    termios.local_modes &= !LocalModes::ICANON;
    termios.local_modes &= !LocalModes::ECHO;
    termios::tcsetattr(&stdin, OptionalActions::Drain, &termios)?;

    Ok(move || {
        Ok(termios::tcsetattr(
            io::stdin(),
            OptionalActions::Now,
            &termios_original,
        )?)
    })
}

#[cfg(windows)]
fn enable_raw_mode() -> Result<impl FnOnce() -> Result<()>> {
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE},
            Storage::FileSystem::{
                self, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
            },
            System::Console::{
                self, CONSOLE_MODE, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
            },
        },
    };

    let utf16: Vec<u16> = "CONIN$\0".encode_utf16().collect();
    let utf16_ptr: *const u16 = utf16.as_ptr();

    let in_handle = unsafe {
        FileSystem::CreateFileW(
            PCWSTR(utf16_ptr),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            HANDLE::default(),
        )
    }?;

    let mut original_in_mode = CONSOLE_MODE::default();
    unsafe { Console::GetConsoleMode(in_handle, &mut original_in_mode) }?;

    let requested_in_modes = !ENABLE_ECHO_INPUT & !ENABLE_LINE_INPUT & !ENABLE_PROCESSED_INPUT;
    let in_mode = original_in_mode & requested_in_modes;
    unsafe { Console::SetConsoleMode(in_handle, in_mode) }?;

    Ok(move || {
        unsafe { Console::SetConsoleMode(in_handle, original_in_mode) }?;
        Ok(())
    })
}

#[cfg(not(windows))]
fn font_size_fallback() -> Option<FontSize> {
    use rustix::termios::{self, Winsize};

    let winsize = termios::tcgetwinsize(io::stdout()).ok()?;
    let Winsize {
        ws_xpixel: x,
        ws_ypixel: y,
        ws_col: cols,
        ws_row: rows,
    } = winsize;
    if x == 0 || y == 0 || cols == 0 || rows == 0 {
        return None;
    }

    Some((x / cols, y / rows))
}

#[cfg(windows)]
fn font_size_fallback() -> Option<FontSize> {
    None
}

/// Query the terminal, by writing and reading to stdin and stdout.
/// The terminal must be in "raw mode" and should probably be reset to "cooked mode" when this
/// operation has completed.
///
/// The returned [ProtocolType] and [FontSize] may be included in the list of [Capability]s,
/// but the burden of picking out the right one or a font-size fallback is already resolved here.
fn query_stdio_capabilities(
    is_tmux: bool,
    options: QueryStdioOptions,
) -> Result<(Option<ProtocolType>, Option<FontSize>, Vec<Capability>)> {
    // Send several control sequences at once:
    // `_Gi=...`: Kitty graphics support.
    // `[c`: Capabilities including sixels.
    // `[16t`: Cell-size (perhaps we should also do `[14t`).
    // `[1337n`: iTerm2 (some terminals implement the protocol but sadly not this custom CSI)
    // `[5n`: Device Status Report, implemented by all terminals, ensure that there is some
    // response and we don't hang reading forever.
    let query = Parser::query(is_tmux, options);
    io::stdout().write_all(query.as_bytes())?;
    io::stdout().flush()?;

    let mut parser = Parser::new();
    let mut responses = vec![];
    'out: loop {
        let mut charbuf: [u8; 50] = [0; 50];
        let result = io::stdin().read(&mut charbuf);
        match result {
            Ok(read) => {
                for ch in charbuf.iter().take(read) {
                    let mut more_caps = parser.push(char::from(*ch));
                    match more_caps[..] {
                        [Response::Status] => {
                            break 'out;
                        }
                        _ => responses.append(&mut more_caps),
                    }
                }
            }
            Err(err) => {
                return Err(err.into());
            }
        }
    }

    interpret_parser_responses(responses)
}

fn interpret_parser_responses(
    responses: Vec<Response>,
) -> Result<(Option<ProtocolType>, Option<FontSize>, Vec<Capability>)> {
    if responses.is_empty() {
        return Err(Errors::NoCap);
    }

    let mut capabilities = Vec::new();

    let mut proto = None;
    let mut font_size = None;

    let mut cursor_position_reports = vec![];
    for response in &responses {
        if let Some(capability) = match response {
            Response::Kitty => {
                proto = Some(ProtocolType::Kitty);
                Some(Capability::Kitty)
            }
            Response::Sixel => {
                if proto.is_none() {
                    // Only if kitty is not supported.
                    proto = Some(ProtocolType::Sixel);
                }
                Some(Capability::Sixel)
            }
            Response::RectangularOps => Some(Capability::RectangularOps),
            Response::CellSize(cell_size) => {
                if let Some((w, h)) = cell_size {
                    font_size = Some((*w, *h));
                }
                Some(Capability::CellSize(*cell_size))
            }
            Response::CursorPositionReport(x, y) => {
                cursor_position_reports.push((x, y));
                None
            }
            Response::Status => None,
        } {
            capabilities.push(capability);
        }
    }

    // In case some terminal didn't support the cell-size query.
    font_size = font_size.or_else(font_size_fallback);

    if let [(x1, _y1), (x2, _y2), (x3, _y3)] = cursor_position_reports[..] {
        // Test if the cursor advanced exactly two columns (instead of one) on both the width and
        // scaling queries of the protocol.
        // The documentation is a bit ambiguous, as it only says the cursor positions "need to be
        // different from each other".
        // However from my testing on Kitty and other terminals that do not support the feature,
        // the cursor always advances at least one column since it is printing a space, so the CPRs
        // will always be different from each other (unless we would move the cursor to a known
        // position or something like that - and this also begs the question of needing to do this
        // anyway, for the edge case of the cursor being at the very end of a line).
        // My interpretation is that the cursor should advance 2 columns, instead of one, with both
        // queries, and only then can we interpret it as supported.
        // The Foot terminal notably reports a 2 column movement but fortunately only for the `w=2`
        // query.
        //
        // The row part can be ignored.
        if *x2 == x1 + 2 && *x3 == x2 + 2 {
            capabilities.push(Capability::TextSizingProtocol);
        }
    }

    Ok((proto, font_size, capabilities))
}

fn query_with_timeout(
    is_tmux: bool,
    timeout: Duration,
    options: QueryStdioOptions,
) -> Result<(Option<ProtocolType>, Option<FontSize>, Vec<Capability>)> {
    use std::{sync::mpsc, thread};
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let _ = tx.send(
            enable_raw_mode()
                .map_err(Errors::into)
                .and_then(|disable_raw_mode| {
                    let result = query_stdio_capabilities(is_tmux, options);
                    // Always try to return to raw_mode.
                    disable_raw_mode()?;
                    result
                }),
        );
    });

    match rx.recv_timeout(timeout) {
        Ok(result) => Ok(result?),
        Err(_recvtimeout) => Err(Errors::NoStdinResponse),
    }
}

#[cfg(test)]
mod tests {
    use std::assert_eq;

    use crate::picker::{Capability, Picker, ProtocolType};

    use super::{cap_parser::Response, interpret_parser_responses};

    #[test]
    fn test_cycle_protocol() {
        let mut proto = ProtocolType::Halfblocks;
        proto = proto.next();
        assert_eq!(proto, ProtocolType::Sixel);
        proto = proto.next();
        assert_eq!(proto, ProtocolType::Kitty);
        proto = proto.next();
        assert_eq!(proto, ProtocolType::Iterm2);
        proto = proto.next();
        assert_eq!(proto, ProtocolType::Halfblocks);
    }

    #[test]
    fn test_from_query_stdio_no_hang() {
        let _ = Picker::from_query_stdio();
    }

    #[test]
    fn test_interpret_parser_responses_text_sizing_protocol() {
        let (_, _, caps) = interpret_parser_responses(vec![
            // Example response from Kitty.
            Response::CursorPositionReport(1, 1),
            Response::CursorPositionReport(3, 1),
            Response::CursorPositionReport(5, 1),
        ])
        .unwrap();
        assert!(caps.contains(&Capability::TextSizingProtocol));
    }

    #[test]
    fn test_interpret_parser_responses_text_sizing_protocol_incomplete() {
        let (_, _, caps) = interpret_parser_responses(vec![
            // Example response from Foot, notably moves 2 columns only on `w=2` query, but not
            // `s=2`.
            Response::CursorPositionReport(1, 22),
            Response::CursorPositionReport(3, 22),
            Response::CursorPositionReport(4, 22),
        ])
        .unwrap();
        assert!(!caps.contains(&Capability::TextSizingProtocol));
    }
}
