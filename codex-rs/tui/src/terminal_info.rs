use std::env;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Write;
use std::time::Duration;
use std::time::Instant;

const ANSI_16_TO_RGB: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (205, 0, 0),
    (0, 205, 0),
    (205, 205, 0),
    (0, 0, 205),
    (205, 0, 205),
    (0, 205, 205),
    (229, 229, 229),
    (127, 127, 127),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (92, 92, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalBackgroundSource {
    Osc11,
    ColorFgBg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalBackgroundDetection {
    pub is_dark: bool,
    pub source: TerminalBackgroundSource,
    pub rgb: Option<(u8, u8, u8)>,
}

fn set_nonblocking(tty: &std::fs::File) {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = tty.as_raw_fd();
        if fd != -1 {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
            if flags != -1 {
                unsafe {
                    libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }
            }
        }
    }
}

fn read_reply(tty: &mut std::fs::File, timeout: Duration) -> Option<String> {
    let start = Instant::now();
    let mut buf = [0u8; 256];
    let mut s = String::new();

    set_nonblocking(tty);

    while start.elapsed() < timeout {
        match tty.read(&mut buf) {
            Ok(n) if n > 0 => {
                s.push_str(&String::from_utf8_lossy(&buf[..n]));
                // Check if we got a complete response (ends with 't')
                if s.contains('t') && s.contains("\x1b[") {
                    break;
                }
            }
            _ => {
                // Small sleep to avoid busy waiting
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }

    (!s.is_empty()).then_some(s)
}

fn parse_three_nums(s: &str) -> Option<(u32, u32, u32)> {
    // Parse response like "\x1b[6;20;10t" (kind;height;width)
    // Use simple parsing to avoid regex dependency
    if let Some(start) = s.find("\x1b[") {
        let s = &s[start + 2..]; // Skip "\x1b["
        if let Some(end) = s.find('t') {
            let nums_str = &s[..end];
            let parts: Vec<&str> = nums_str.split(';').collect();
            if parts.len() == 3 {
                if let (Ok(a), Ok(b), Ok(c)) = (
                    parts[0].parse::<u32>(),
                    parts[1].parse::<u32>(),
                    parts[2].parse::<u32>(),
                ) {
                    return Some((a, b, c));
                }
            }
        }
    }
    None
}

pub fn get_cell_size_pixels() -> Option<(u16, u16)> {
    // Open /dev/tty for reading and writing
    let mut tty_w = OpenOptions::new().write(true).open("/dev/tty").ok()?;
    let mut tty_r = OpenOptions::new().read(true).open("/dev/tty").ok()?;

    // Try direct cell size query (CSI 16 t) -> expect: CSI 6;height;width t
    tty_w.write_all(b"\x1b[16t").ok()?;
    tty_w.flush().ok()?;
    if let Some(reply) = read_reply(&mut tty_r, Duration::from_millis(100)) {
        if let Some((kind, height, width)) = parse_three_nums(&reply) {
            if kind == 6 && width > 0 && height > 0 {
                return Some((width as u16, height as u16));
            }
        }
    }

    // Fallback: window size in pixels (CSI 14 t) -> CSI 4;win_h;win_w t
    tty_w.write_all(b"\x1b[14t").ok()?;
    tty_w.flush().ok()?;
    let (mut win_h, mut win_w) = (0u32, 0u32);
    if let Some(reply) = read_reply(&mut tty_r, Duration::from_millis(100)) {
        if let Some((kind, h, w)) = parse_three_nums(&reply) {
            if kind == 4 {
                win_h = h;
                win_w = w;
            }
        }
    }

    // Text area in characters (CSI 18 t) -> CSI 8;rows;cols t
    tty_w.write_all(b"\x1b[18t").ok()?;
    tty_w.flush().ok()?;
    let (mut rows, mut cols) = (0u32, 0u32);
    if let Some(reply) = read_reply(&mut tty_r, Duration::from_millis(100)) {
        if let Some((kind, r, c)) = parse_three_nums(&reply) {
            if kind == 8 {
                rows = r;
                cols = c;
            }
        }
    }

    if win_h > 0 && win_w > 0 && rows > 0 && cols > 0 {
        let cell_w = (win_w as f32 / cols as f32).round() as u16;
        let cell_h = (win_h as f32 / rows as f32).round() as u16;
        if cell_w > 0 && cell_h > 0 {
            return Some((cell_w, cell_h));
        }
    }
    None
}

fn read_osc_reply(tty: &mut std::fs::File, timeout: Duration) -> Option<Vec<u8>> {
    let start = Instant::now();
    let mut buf = [0u8; 256];
    let mut data = Vec::new();

    set_nonblocking(tty);

    while start.elapsed() < timeout {
        match tty.read(&mut buf) {
            Ok(n) if n > 0 => {
                data.extend_from_slice(&buf[..n]);
                let has_bel = data.contains(&b'\x07');
                let has_st = data.windows(2).any(|w| w == b"\x1b\\");
                if has_bel || has_st {
                    break;
                }
            }
            _ => std::thread::sleep(Duration::from_millis(10)),
        }
    }

    if data.is_empty() { None } else { Some(data) }
}

fn parse_component(component: &str) -> Option<u8> {
    let trimmed = component.trim();
    match trimmed.len() {
        2 => u8::from_str_radix(trimmed, 16).ok(),
        4 => u16::from_str_radix(trimmed, 16)
            .ok()
            .map(|value| ((value as u32 * 255 + 32_767) / 65_535) as u8),
        _ => None,
    }
}

fn parse_osc_rgb(reply: &str) -> Option<(u8, u8, u8)> {
    let start = reply.find("]11;")?;
    let payload = &reply[start + 4..];
    let payload = payload.trim_start_matches('?');
    let end = payload
        .find('\u{7}')
        .or_else(|| payload.find("\x1b\\"))
        .unwrap_or(payload.len());
    let payload = &payload[..end];

    if let Some(rest) = payload.strip_prefix("rgb:") {
        let mut parts = rest.split('/');
        let r = parse_component(parts.next()?)?;
        let g = parse_component(parts.next()?)?;
        let b = parse_component(parts.next()?)?;
        return Some((r, g, b));
    }

    if let Some(rest) = payload.strip_prefix("rgba:") {
        let mut parts = rest.split('/');
        let r = parse_component(parts.next()?)?;
        let g = parse_component(parts.next()?)?;
        let b = parse_component(parts.next()?)?;
        return Some((r, g, b));
    }

    if let Some(hex) = payload.strip_prefix('#') {
        if hex.len() >= 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some((r, g, b));
        }
    }

    None
}

fn query_osc_background_color() -> Option<(u8, u8, u8)> {
    let mut tty_w = OpenOptions::new().write(true).open("/dev/tty").ok()?;
    let mut tty_r = OpenOptions::new().read(true).open("/dev/tty").ok()?;

    tty_w.write_all(b"\x1b]11;?\x07").ok()?;
    tty_w.flush().ok()?;

    let reply = read_osc_reply(&mut tty_r, Duration::from_millis(150))?;
    let _ = tty_w.write_all(b"\r\x1b[K");
    let _ = tty_w.flush();
    let reply_str = String::from_utf8_lossy(&reply);
    parse_osc_rgb(&reply_str)
}

fn osc_background_query_supported() -> bool {
    if env::var("TMUX").is_ok() || env::var("STY").is_ok() {
        return false;
    }

    let term = env::var("TERM").unwrap_or_default();
    if term.is_empty() {
        return false;
    }
    let term_lower = term.to_ascii_lowercase();

    const UNSUPPORTED_PREFIXES: [&str; 2] = ["screen", "tmux"];
    if UNSUPPORTED_PREFIXES
        .iter()
        .any(|prefix| term_lower.starts_with(prefix))
    {
        return false;
    }

    const UNSUPPORTED_TERMS: [&str; 5] = [
        "dumb",
        "linux",
        "vt100",
        "xterm-color",
        "ansi",
    ];
    if UNSUPPORTED_TERMS.contains(&term_lower.as_str()) {
        return false;
    }

    true
}

fn xterm_color_to_rgb(idx: u32) -> Option<(u8, u8, u8)> {
    if idx <= 15 {
        return Some(ANSI_16_TO_RGB[idx as usize]);
    }
    if (16..=231).contains(&idx) {
        let idx = idx - 16;
        let r = idx / 36;
        let g = (idx % 36) / 6;
        let b = idx % 6;
        let to_component = |v: u32| if v == 0 { 0 } else { 55 + v * 40 };
        return Some((
            to_component(r) as u8,
            to_component(g) as u8,
            to_component(b) as u8,
        ));
    }
    if (232..=255).contains(&idx) {
        let level = (idx - 232) * 10 + 8;
        let level = level as u8;
        return Some((level, level, level));
    }
    None
}

fn parse_colorfgbg_env() -> Option<(u8, u8, u8)> {
    let raw = env::var("COLORFGBG").ok()?;
    let bg_part = raw
        .split(';')
        .filter(|segment| !segment.is_empty())
        .last()?;
    if bg_part.eq_ignore_ascii_case("default") {
        return None;
    }
    let idx = bg_part.parse::<u32>().ok()?;
    xterm_color_to_rgb(idx)
}

fn relative_luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let to_linear = |component: u8| {
        let c = component as f64 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };

    0.2126 * to_linear(r) + 0.7152 * to_linear(g) + 0.0722 * to_linear(b)
}

fn detect_dark_from_rgb(rgb: (u8, u8, u8)) -> bool {
    relative_luminance(rgb) < 0.45
}

pub fn detect_dark_terminal_background() -> Option<TerminalBackgroundDetection> {
    if let Ok(value) = env::var("CODE_DISABLE_THEME_AUTODETECT") {
        if matches!(value.as_str(), "1" | "true" | "TRUE" | "True") {
            return None;
        }
    }

    if osc_background_query_supported() {
        if let Some(rgb) = query_osc_background_color() {
            return Some(TerminalBackgroundDetection {
                is_dark: detect_dark_from_rgb(rgb),
                source: TerminalBackgroundSource::Osc11,
                rgb: Some(rgb),
            });
        }
    }

    if let Some(rgb) = parse_colorfgbg_env() {
        return Some(TerminalBackgroundDetection {
            is_dark: detect_dark_from_rgb(rgb),
            source: TerminalBackgroundSource::ColorFgBg,
            rgb: Some(rgb),
        });
    }

    None
}
