//! Terminal stdio query parser module.
use std::fmt::Write;

pub struct Parser {
    data: String,
    sequence: ResponseParseState,
}

#[derive(Debug, PartialEq)]
pub enum ResponseParseState {
    Unknown,
    CSIResponse,
    KittyResponse,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Response {
    Kitty,
    Sixel,
    RectangularOps,
    CellSize(Option<(u16, u16)>),
    CursorPositionReport(u16, u16),
    Status,
}

/// Extra query options
pub struct QueryStdioOptions {
    /// Query for [Text Sizing Protocol]. The result can be checked by searching for
    /// [Capability::TextSizingProtocol] in [crate::picker::Picker::capabilities].
    ///
    /// [Text Sizing Protocol] <https://sw.kovidgoyal.net/kitty/text-sizing-protocol//>
    pub text_sizing_protocol: bool,
}

impl Default for Parser {
    fn default() -> Self {
        Parser {
            data: String::new(),
            sequence: ResponseParseState::Unknown,
        }
    }
}

impl Parser {
    pub fn new() -> Self {
        Parser {
            data: String::new(),
            sequence: ResponseParseState::Unknown,
        }
    }
    // Tmux requires escapes to be escaped, and some special start/end sequences.
    pub fn escape_tmux(is_tmux: bool) -> (&'static str, &'static str, &'static str) {
        match is_tmux {
            false => ("", "\x1b", ""),
            true => ("\x1bPtmux;", "\x1b\x1b", "\x1b\\"),
        }
    }
    pub fn query(is_tmux: bool, options: QueryStdioOptions) -> String {
        let (start, escape, end) = Parser::escape_tmux(is_tmux);

        let mut buf = String::with_capacity(100);
        buf.push_str(start);

        // Kitty graphics
        write!(buf, "{escape}_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA{escape}\\").unwrap();

        // Device Attributes Report 1 (sixel support)
        write!(buf, "{escape}[c").unwrap();

        // Font size in pixels
        write!(buf, "{escape}[16t").unwrap();

        // iTerm2 proprietary, unknown response, untested so far.
        //write!(buf, "{escape}[1337n").unwrap();

        if options.text_sizing_protocol {
            const BEL: &str = "\u{7}";
            // Send CPR (Cursor Position Report) and Text Sizing Protocol commands.
            // https://sw.kovidgoyal.net/kitty/text-sizing-protocol/#detecting-if-the-terminal-supports-this-protocol
            // We need to write a CPR, a resized space, and CPR again, to see if it moved the cursor
            // correctly with extra width.
            // Do it again for the scaling part of the protocol.
            // See [Picker::interpret_parser_responses] for how the responses are interpreted - it
            // differs slightly from the spec!
            write!(
                buf,
                "{escape}[6n{escape}]66;w=2; {BEL}{escape}[6n{escape}]66;s=2; {BEL}{escape}[6n"
            )
            .unwrap();
        }

        // End with Device Status Report, implemented by all terminals, ensure that there is some
        // response and we don't hang reading forever.
        write!(buf, "{escape}[5n").unwrap();

        write!(buf, "{end}").unwrap();
        buf
    }
    pub fn push(&mut self, next: char) -> Vec<Response> {
        match self.sequence {
            ResponseParseState::Unknown => {
                match (&self.data[..], next) {
                    (_, '\x1b') => {
                        // If the current sequence hasn't been identified yet, start a new one on Esc.
                        return self.restart();
                    }
                    ("_Gi=31", ';') => {
                        self.sequence = ResponseParseState::KittyResponse;
                    }

                    ("[", _) => {
                        self.sequence = ResponseParseState::CSIResponse;
                    }
                    _ => {}
                };
                self.data.push(next);
            }
            ResponseParseState::CSIResponse => {
                if self.data == "[0" && next == 'n' {
                    self.restart();
                    return vec![Response::Status];
                }
                match next {
                    'c' if self.data.starts_with("[?") => {
                        let mut caps = vec![];
                        let inner: Vec<&str> = (self.data[2..]).split(';').collect();
                        for cap in inner {
                            match cap {
                                "4" => caps.push(Response::Sixel),
                                "28" => caps.push(Response::RectangularOps),
                                _ => {}
                            }
                        }
                        self.restart();
                        return caps;
                    }
                    't' => {
                        let mut cell_size = None;
                        let inner: Vec<&str> = self.data.split(';').collect();
                        if let [_, h, w] = inner[..] {
                            if let (Ok(h), Ok(w)) = (h.parse::<u16>(), w.parse::<u16>()) {
                                if w > 0 && h > 0 {
                                    cell_size = Some((w, h));
                                }
                            }
                        }
                        self.restart();
                        return vec![Response::CellSize(cell_size)];
                    }
                    'R' => {
                        let mut cursor_pos = None;
                        let inner: Vec<&str> = self.data[1..].split(';').collect();
                        if let [x, w] = inner[..] {
                            if let (Ok(x), Ok(y)) = (x.parse::<u16>(), w.parse::<u16>()) {
                                cursor_pos = Some((y, x));
                            }
                        }
                        if let Some((x, y)) = cursor_pos {
                            self.restart();
                            return vec![Response::CursorPositionReport(x, y)];
                        } else {
                            self.restart();
                            return vec![];
                        }
                    }
                    '\x1b' => {
                        // Give up?
                        return self.restart();
                    }
                    _ => {
                        self.data.push(next);
                    }
                };
            }

            ResponseParseState::KittyResponse => match next {
                '\\' => {
                    let caps = match &self.data[..] {
                        "_Gi=31;OK\x1b" => vec![Response::Kitty],
                        _ => vec![],
                    };
                    self.restart();
                    return caps;
                }
                _ => {
                    self.data.push(next);
                }
            },
        };
        vec![]
    }
    fn restart(&mut self) -> Vec<Response> {
        self.data = String::new();
        self.sequence = ResponseParseState::Unknown;
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use std::assert_eq;

    use super::{Parser, Response};

    fn parse(response: &str) -> Vec<Response> {
        let mut parser = Parser::new();
        let mut caps: Vec<Response> = vec![];
        for ch in response.chars() {
            let mut more_caps = parser.push(ch);
            caps.append(&mut more_caps)
        }
        caps
    }

    #[test]
    fn test_parse_all() {
        let caps =
            parse("\x1b_Gi=31;OK\x1b\\\x1b[?64;4c\x1b[6;7;14t\x1b[6;6R\x1b[7;7R\x1b[6;6R\x1b[0n");
        assert_eq!(
            caps,
            vec![
                Response::Kitty,
                Response::Sixel,
                Response::CellSize(Some((14, 7))),
                Response::CursorPositionReport(6, 6),
                Response::CursorPositionReport(7, 7),
                Response::CursorPositionReport(6, 6),
                Response::Status,
            ],
        );
    }

    #[test]
    fn test_parse_only_garbage() {
        let caps = parse("\x1bhonkey\x1btonkey\x1b[42\x1b\\");
        assert_eq!(caps, vec![]);
    }

    #[test]
    fn test_parse_preceding_garbage() {
        let caps = parse("\x1bgarbage...\x1b[?64;5c\x1b[0n");
        assert_eq!(caps, vec![Response::Status]);
    }

    #[test]
    fn test_parse_inner_garbage() {
        let caps = parse("\x1b[6;7;14t\x1bgarbage...\x1b[?64;5c\x1b[0n");
        assert_eq!(
            caps,
            vec![Response::CellSize(Some((14, 7))), Response::Status]
        );
    }

    // #[test]
    // fn test_parse_incomplete_support_in_text_sizing_protocol() {
    // let caps = parse("\x1b[6;7;14t\x1b[6;6R\x1b[7;7R\x1b[6;6R\x1b[0n");
    // assert_eq!(
    // caps,
    // vec![
    // Response::CellSize(Some((14, 7))),
    // Response::CursorPositionReport(6, 6),
    // Response::CursorPositionReport(7, 7),
    // Response::CursorPositionReport(6, 6),
    // Response::Status,
    // ],
    // );
    // }
}
