use codex_core::config_types::ThemeColors;
use codex_core::config_types::ThemeConfig;
use codex_core::config_types::ThemeName;
use lazy_static::lazy_static;
use ratatui::style::Color;
use std::sync::RwLock;

lazy_static! {
    static ref CURRENT_THEME: RwLock<Theme> = RwLock::new(Theme::default());
    static ref CUSTOM_THEME_LABEL: RwLock<Option<String>> = RwLock::new(None);
    static ref CUSTOM_THEME_COLORS: RwLock<Option<codex_core::config_types::ThemeColors>> = RwLock::new(None);
    static ref CUSTOM_THEME_IS_DARK: RwLock<Option<bool>> = RwLock::new(None);
}

/// Represents a complete theme with all colors resolved
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    // Primary colors
    pub primary: Color,
    pub secondary: Color,
    pub background: Color,
    pub foreground: Color,

    // UI elements
    pub border: Color,
    pub border_focused: Color,
    pub selection: Color,
    pub cursor: Color,

    // Status colors
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,

    // Text colors
    pub text: Color,
    pub text_dim: Color,
    pub text_bright: Color,

    // Syntax/special colors
    pub keyword: Color,
    pub string: Color,
    pub comment: Color,
    pub function: Color,

    // Animation colors
    pub spinner: Color,
    pub progress: Color,
}

impl Default for Theme {
    fn default() -> Self {
        get_predefined_theme(ThemeName::LightPhoton)
    }
}

/// Initialize the global theme from configuration
pub fn init_theme(config: &ThemeConfig) {
    let mut theme = get_predefined_theme(config.name);
    // Important: Only apply color overrides for the Custom theme.
    // Built-in themes should render exactly as defined so that switching away
    // from Custom does not keep stale custom overrides from config.
    if matches!(config.name, ThemeName::Custom) {
        apply_custom_colors(&mut theme, &config.colors);
    }

    // On some terminals (notably macOS Terminal.app with certain profiles),
    // truecolor escape sequences may render incorrectly. Detect such cases
    // and quantize the theme to the ANSI-256 palette for robust rendering.
    if needs_ansi256_fallback() {
        quantize_theme_to_ansi256(&mut theme);
    }

    let mut current = CURRENT_THEME.write().unwrap();
    *current = theme;
    // Track custom theme label for UI display
    if matches!(config.name, ThemeName::Custom) {
        *CUSTOM_THEME_LABEL.write().unwrap() = config.label.clone();
        *CUSTOM_THEME_COLORS.write().unwrap() = Some(config.colors.clone());
        *CUSTOM_THEME_IS_DARK.write().unwrap() = config.is_dark;
    }
}

/// Get the current theme
pub fn current_theme() -> Theme {
    CURRENT_THEME.read().unwrap().clone()
}

/// Get the custom theme's display label, if any
pub fn custom_theme_label() -> Option<String> {
    CUSTOM_THEME_LABEL.read().unwrap().clone()
}

/// Set/update the custom theme's label at runtime
pub fn set_custom_theme_label(label: String) {
    *CUSTOM_THEME_LABEL.write().unwrap() = Some(label);
}

/// Set/update the custom theme's colors at runtime
pub fn set_custom_theme_colors(colors: codex_core::config_types::ThemeColors) {
    *CUSTOM_THEME_COLORS.write().unwrap() = Some(colors);
}

/// Return the custom theme colors, if known in this session
pub fn custom_theme_colors() -> Option<codex_core::config_types::ThemeColors> {
    CUSTOM_THEME_COLORS.read().unwrap().clone()
}

pub fn set_custom_theme_is_dark(is_dark: Option<bool>) {
    *CUSTOM_THEME_IS_DARK.write().unwrap() = is_dark;
}

pub fn custom_theme_is_dark() -> Option<bool> {
    CUSTOM_THEME_IS_DARK.read().unwrap().clone()
}

/// Switch to a different predefined theme
pub fn switch_theme(theme_name: ThemeName) {
    let theme = get_predefined_theme(theme_name);
    let mut current = CURRENT_THEME.write().unwrap();
    let mut theme = theme;
    if needs_ansi256_fallback() {
        quantize_theme_to_ansi256(&mut theme);
    }
    *current = theme;
}

/// Parse a color string (hex or named color)
fn parse_color(color_str: &str) -> Option<Color> {
    if let Some(hex) = color_str.strip_prefix('#') {
        if hex.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                return Some(Color::Rgb(r, g, b));
            }
        }
    }

    // Named colors
    match color_str.to_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" => Some(Color::DarkGray),
        "lightred" => Some(Color::LightRed),
        "lightgreen" => Some(Color::LightGreen),
        "lightyellow" => Some(Color::LightYellow),
        "lightblue" => Some(Color::LightBlue),
        "lightmagenta" => Some(Color::LightMagenta),
        "lightcyan" => Some(Color::LightCyan),
        "white" => Some(Color::White),
        _ => None,
    }
}

/// Apply custom color overrides to a theme
fn apply_custom_colors(theme: &mut Theme, colors: &ThemeColors) {
    if let Some(ref c) = colors.primary {
        if let Some(color) = parse_color(c) {
            theme.primary = color;
        }
    }
    if let Some(ref c) = colors.secondary {
        if let Some(color) = parse_color(c) {
            theme.secondary = color;
        }
    }
    if let Some(ref c) = colors.background {
        if let Some(color) = parse_color(c) {
            theme.background = color;
        }
    }
    if let Some(ref c) = colors.foreground {
        if let Some(color) = parse_color(c) {
            theme.foreground = color;
        }
    }
    if let Some(ref c) = colors.border {
        if let Some(color) = parse_color(c) {
            theme.border = color;
        }
    }
    if let Some(ref c) = colors.border_focused {
        if let Some(color) = parse_color(c) {
            theme.border_focused = color;
        }
    }
    if let Some(ref c) = colors.selection {
        if let Some(color) = parse_color(c) {
            theme.selection = color;
        }
    }
    if let Some(ref c) = colors.cursor {
        if let Some(color) = parse_color(c) {
            theme.cursor = color;
        }
    }
    if let Some(ref c) = colors.success {
        if let Some(color) = parse_color(c) {
            theme.success = color;
        }
    }
    if let Some(ref c) = colors.warning {
        if let Some(color) = parse_color(c) {
            theme.warning = color;
        }
    }
    if let Some(ref c) = colors.error {
        if let Some(color) = parse_color(c) {
            theme.error = color;
        }
    }
    if let Some(ref c) = colors.info {
        if let Some(color) = parse_color(c) {
            theme.info = color;
        }
    }
    if let Some(ref c) = colors.text {
        if let Some(color) = parse_color(c) {
            theme.text = color;
        }
    }
    if let Some(ref c) = colors.text_dim {
        if let Some(color) = parse_color(c) {
            theme.text_dim = color;
        }
    }
    if let Some(ref c) = colors.text_bright {
        if let Some(color) = parse_color(c) {
            theme.text_bright = color;
        }
    }
    if let Some(ref c) = colors.keyword {
        if let Some(color) = parse_color(c) {
            theme.keyword = color;
        }
    }
    if let Some(ref c) = colors.string {
        if let Some(color) = parse_color(c) {
            theme.string = color;
        }
    }
    if let Some(ref c) = colors.comment {
        if let Some(color) = parse_color(c) {
            theme.comment = color;
        }
    }
    if let Some(ref c) = colors.function {
        if let Some(color) = parse_color(c) {
            theme.function = color;
        }
    }
    if let Some(ref c) = colors.spinner {
        if let Some(color) = parse_color(c) {
            theme.spinner = color;
        }
    }
    if let Some(ref c) = colors.progress {
        if let Some(color) = parse_color(c) {
            theme.progress = color;
        }
    }
}

/// Return true when we should prefer ANSI-256 over truecolor for safety.
///
/// Heuristics:
/// - Respect `CODE_FORCE_ANSI256=1` to force fallback.
/// - Default to ANSI-256 on Apple's built-in Terminal (TERM_PROGRAM=Apple_Terminal),
///   where some profiles are known to misrender truecolor in alternate screen.
/// - Otherwise, allow truecolor when `COLORTERM` advertises it or when running
///   in modern terminals known to support it well.
fn needs_ansi256_fallback() -> bool {
    // Hard overrides first
    if std::env::var("CODE_FORCE_TRUECOLOR").map(|v| v == "1").unwrap_or(false) {
        return false;
    }
    if std::env::var("CODE_FORCE_ANSI256").map(|v| v == "1").unwrap_or(false) {
        return true;
    }

    let term_program = std::env::var("TERM_PROGRAM").unwrap_or_default();
    // Apple Terminal profiles are known to misrender truecolor in alt-screen.
    if term_program == "Apple_Terminal" {
        return true;
    }
    // Windows Terminal (conpty) supports truecolor; avoid fallback.
    if term_program == "Windows_Terminal" || std::env::var("WT_SESSION").is_ok() {
        return false;
    }

    // Environment advertisement
    let colorterm = std::env::var("COLORTERM").unwrap_or_default().to_lowercase();
    let has_truecolor_env = colorterm.contains("truecolor") || colorterm.contains("24bit");

    // Known good terminals
    let known_truecolor = matches!(
        term_program.as_str(),
        "iTerm.app" | "WezTerm" | "Ghostty" | "Alacritty" | "kitty" | "vscode"
    );

    // Library-based probe as a final signal (may be conservative on Windows).
    let has_truecolor_probe = supports_color::on_cached(supports_color::Stream::Stdout)
        .map(|lvl| lvl.has_16m)
        .unwrap_or(false);

    !(has_truecolor_env || known_truecolor || has_truecolor_probe)
}

/// Return true if the current terminal supports truecolor rendering.
/// Mirrors `needs_ansi256_fallback` but inverted and with the same overrides.
pub(crate) fn has_truecolor_terminal() -> bool {
    !needs_ansi256_fallback()
}

/// Quantize all theme colors to the ANSI-256 palette so backends that do not
/// render truecolor reliably still get consistent colors.
fn quantize_theme_to_ansi256(theme: &mut Theme) {
    // Preserve exact white backgrounds as truecolor to avoid terminals whose
    // ANSI palette's "white" (15) is a light gray. This specifically fixes
    // macOS Terminal.app where bright white can appear gray.
    fn preserve_true_white(c: Color, for_background: bool) -> Option<Color> {
        if !for_background { return None; }
        if let Color::Rgb(r, g, b) = c {
            if r >= 245 && g >= 245 && b >= 245 {
                // On limited-color terminals we want a strong white for light themes.
                // Use ANSI bright white (index 15), which maps to the profile's
                // bright white and is reliably high‑contrast (unlike grayscale 231).
                return Some(Color::Indexed(15));
            }
        }
        None
    }

    let q = quantize_color_to_ansi256;
    theme.primary = q(theme.primary);
    theme.secondary = q(theme.secondary);
    theme.background = preserve_true_white(theme.background, true).unwrap_or_else(|| q(theme.background));
    theme.foreground = q(theme.foreground);
    theme.border = q(theme.border);
    theme.border_focused = q(theme.border_focused);
    theme.selection = q(theme.selection);
    theme.cursor = q(theme.cursor);
    theme.success = q(theme.success);
    theme.warning = q(theme.warning);
    theme.error = q(theme.error);
    theme.info = q(theme.info);
    theme.text = q(theme.text);
    theme.text_dim = q(theme.text_dim);
    theme.text_bright = q(theme.text_bright);
    theme.keyword = q(theme.keyword);
    theme.string = q(theme.string);
    theme.comment = q(theme.comment);
    theme.function = q(theme.function);
    theme.spinner = q(theme.spinner);
    theme.progress = q(theme.progress);
}

fn quantize_color_to_ansi256(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Indexed(rgb_to_ansi256_index(r, g, b)),
        // Named colors already map to ANSI; keep as-is.
        other => other,
    }
}

// Map an RGB color to the closest xterm-256 color index using the standard
// 6x6x6 color cube + grayscale ramp.
fn rgb_to_ansi256_index(r: u8, g: u8, b: u8) -> u8 {
    // Helper to compute squared distance
    fn dist2(a: (u8, u8, u8), b: (u8, u8, u8)) -> i32 {
        let dr = a.0 as i32 - b.0 as i32;
        let dg = a.1 as i32 - b.1 as i32;
        let db = a.2 as i32 - b.2 as i32;
        dr * dr + dg * dg + db * db
    }

    // Candidate 1: color cube (16..231)
    const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let idx = |v: u8| -> usize {
        // Find nearest of the 6 steps
        let mut best_i = 0;
        let mut best_d = i32::MAX;
        for (i, s) in STEPS.iter().enumerate() {
            let d = (*s as i32 - v as i32).abs();
            if d < best_d { best_d = d; best_i = i; }
        }
        best_i
    };
    let ri = idx(r);
    let gi = idx(g);
    let bi = idx(b);
    let cube_r = STEPS[ri];
    let cube_g = STEPS[gi];
    let cube_b = STEPS[bi];
    let cube_index = 16 + 36 * ri as u8 + 6 * gi as u8 + bi as u8;

    // Candidate 2: grayscale (232..255)
    let gray_level = {
        let v = (r as u16 + g as u16 + b as u16) / 3;
        if v <= 8 { 0 } else { ((v as i32 - 8) / 10).clamp(0, 23) as u8 }
    };
    let gray_value = 8 + 10 * gray_level;
    let gray_index = 232 + gray_level;

    // Candidate 3: 16-color ANSI (0..15), includes true white (15) which the
    // grayscale ramp does not reach. This fixes near-white mapping to gray.
    const ANSI16: [(u8, u8, u8); 16] = [
        (0, 0, 0),       // 0 black
        (205, 0, 0),     // 1 red
        (0, 205, 0),     // 2 green
        (205, 205, 0),   // 3 yellow
        (0, 0, 205),     // 4 blue
        (205, 0, 205),   // 5 magenta
        (0, 205, 205),   // 6 cyan
        (229, 229, 229), // 7 gray
        (127, 127, 127), // 8 dark gray
        (255, 102, 102), // 9 light red
        (102, 255, 178), // 10 light green
        (255, 255, 102), // 11 light yellow
        (102, 153, 255), // 12 light blue
        (255, 102, 255), // 13 light magenta
        (102, 255, 255), // 14 light cyan
        (255, 255, 255), // 15 white
    ];

    let rgb = (r, g, b);
    let cube_rgb = (cube_r, cube_g, cube_b);
    let gray_rgb = (gray_value, gray_value, gray_value);

    let mut best_index = cube_index;
    let mut best_dist = dist2(rgb, cube_rgb);

    let gray_dist = dist2(rgb, gray_rgb);
    if gray_dist < best_dist {
        best_dist = gray_dist;
        best_index = gray_index;
    }

    for (i, &(ar, ag, ab)) in ANSI16.iter().enumerate() {
        let d = dist2(rgb, (ar, ag, ab));
        if d < best_dist {
            best_dist = d;
            best_index = i as u8;
        }
    }

    best_index
}
/// Get a predefined theme by name
fn get_predefined_theme(name: ThemeName) -> Theme {
    match name {
        ThemeName::DarkCarbonNight => Theme {
            // Dark default using terminal-friendly ANSI colors
            primary: Color::Cyan,
            secondary: Color::LightMagenta,
            background: Color::Black,
            foreground: Color::White,
            border: Color::DarkGray,
            border_focused: Color::Gray,
            selection: Color::DarkGray,
            cursor: Color::White,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            info: Color::LightBlue,
            text: Color::White,
            text_dim: Color::Gray,
            text_bright: Color::White,
            keyword: Color::LightMagenta,
            string: Color::LightCyan,
            comment: Color::DarkGray,
            function: Color::LightGreen,
            spinner: Color::DarkGray,
            progress: Color::Cyan,
        },

        ThemeName::LightPhoton => Theme {
            // Light default using terminal-friendly ANSI colors
            primary: Color::Blue,
            secondary: Color::Magenta,
            background: Color::White,
            foreground: Color::Black,
            border: Color::Gray,
            border_focused: Color::DarkGray,
            selection: Color::Gray,
            cursor: Color::Black,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            info: Color::Blue,
            text: Color::Black,
            text_dim: Color::DarkGray,
            text_bright: Color::Black,
            keyword: Color::Magenta,
            string: Color::Green,
            comment: Color::Gray,
            function: Color::Blue,
            spinner: Color::Gray,
            progress: Color::Blue,
        },

        ThemeName::LightPrismRainbow => Theme {
            // Light - Prism Rainbow
            primary: Color::Rgb(58, 134, 255),        // #3A86FF
            secondary: Color::Rgb(131, 56, 236),      // #8338EC
            background: Color::Rgb(251, 251, 253),    // #FBFBFD
            foreground: Color::Rgb(31, 35, 48),       // #1F2330
            border: Color::Rgb(157, 157, 159),        // #9D9D9F  (↑ contrast)
            border_focused: Color::Rgb(122, 122, 125), // toned contrast vs border
            selection: Color::Rgb(238, 243, 255),     // #EEF3FF
            cursor: Color::Rgb(31, 35, 48),           // #1F2330
            success: Color::Rgb(46, 196, 182),        // #2EC4B6
            warning: Color::Rgb(255, 190, 11),        // #FFBE0B
            error: Color::Rgb(255, 0, 110),           // #FF006E
            info: Color::Rgb(0, 187, 249),            // #00BBF9
            text: Color::Rgb(31, 35, 48),             // #1F2330
            text_dim: Color::Rgb(107, 114, 128),      // #6B7280
            text_bright: Color::Black,                // #000000
            keyword: Color::Rgb(131, 56, 236),        // #8338EC
            string: Color::Rgb(46, 196, 182),         // #2EC4B6
            comment: Color::Rgb(138, 143, 162),       // #8A8FA2
            function: Color::Rgb(58, 134, 255),       // #3A86FF
            spinner: Color::Rgb(165, 174, 192),       // #A5AEC0
            progress: Color::Rgb(58, 134, 255),       // #3A86FF
        },

        ThemeName::LightVividTriad => Theme {
            // Light - Vivid Triad
            primary: Color::Rgb(0, 224, 255),        // #00E0FF
            secondary: Color::Rgb(255, 166, 230),    // #FFA6E6
            background: Color::Rgb(250, 250, 250),   // #FAFAFA
            foreground: Color::Rgb(30, 34, 39),      // #1E2227
            border: Color::Rgb(156, 156, 156),       // #9C9C9C  (↑ contrast)
            border_focused: Color::Rgb(127, 127, 127), // toned contrast vs border
            selection: Color::Rgb(230, 251, 255),    // #E6FBFF
            cursor: Color::Rgb(30, 34, 39),          // #1E2227
            success: Color::Rgb(0, 179, 107),        // #00B36B
            warning: Color::Rgb(255, 181, 0),        // #FFB500
            error: Color::Rgb(233, 53, 97),          // #E93561
            info: Color::Rgb(0, 224, 255),           // #00E0FF
            text: Color::Rgb(30, 34, 39),            // #1E2227
            text_dim: Color::Rgb(106, 115, 128),     // #6A7380
            text_bright: Color::Black,               // #000000
            keyword: Color::Rgb(255, 78, 205),       // #FF4ECD
            string: Color::Rgb(14, 159, 110),        // #0E9F6E
            comment: Color::Rgb(139, 149, 163),      // #8B95A3
            function: Color::Rgb(0, 224, 255),       // #00E0FF
            spinner: Color::Rgb(154, 163, 175),      // #9AA3AF
            progress: Color::Rgb(0, 224, 255),       // #00E0FF
        },

        ThemeName::LightPorcelain => Theme {
            // Light - Porcelain
            primary: Color::Rgb(39, 110, 241),        // #276EF1
            secondary: Color::Rgb(123, 97, 255),      // #7B61FF
            background: Color::Rgb(245, 247, 250),    // #F5F7FA
            foreground: Color::Rgb(27, 31, 35),       // #1B1F23
            border: Color::Rgb(152, 154, 157),        // #989A9D  (↑ contrast)
            border_focused: Color::Rgb(122, 124, 127), // toned contrast vs border
            selection: Color::Rgb(231, 240, 255),     // #E7F0FF
            cursor: Color::Rgb(27, 31, 35),           // #1B1F23
            success: Color::Rgb(43, 168, 74),         // #2BA84A
            warning: Color::Rgb(184, 110, 0),         // #B86E00
            error: Color::Rgb(217, 45, 32),           // #D92D20
            info: Color::Rgb(20, 115, 230),           // #1473E6
            text: Color::Rgb(27, 31, 35),             // #1B1F23
            text_dim: Color::Rgb(91, 102, 115),       // #5B6673
            text_bright: Color::Black,                // #000000
            keyword: Color::Rgb(123, 97, 255),        // #7B61FF
            string: Color::Rgb(15, 123, 108),         // #0F7B6C
            comment: Color::Rgb(140, 153, 166),       // #8C99A6
            function: Color::Rgb(39, 110, 241),       // #276EF1
            spinner: Color::Rgb(154, 168, 181),       // #9AA8B5
            progress: Color::Rgb(39, 110, 241),       // #276EF1
        },

        ThemeName::LightSandbar => Theme {
            // Light - Sandbar
            primary: Color::Rgb(201, 122, 0),        // #C97A00
            secondary: Color::Rgb(91, 138, 114),     // #5B8A72
            background: Color::Rgb(251, 248, 243),   // #FBF8F3
            foreground: Color::Rgb(45, 42, 36),      // #2D2A24
            border: Color::Rgb(158, 155, 150),       // #9E9B96  (↑ contrast)
            border_focused: Color::Rgb(127, 123, 117), // toned contrast vs border
            selection: Color::Rgb(243, 232, 209),    // #F3E8D1
            cursor: Color::Rgb(45, 42, 36),          // #2D2A24
            success: Color::Rgb(46, 125, 50),        // #2E7D32
            warning: Color::Rgb(183, 110, 0),        // #B76E00
            error: Color::Rgb(198, 40, 40),          // #C62828
            info: Color::Rgb(14, 116, 144),          // #0E7490
            text: Color::Rgb(45, 42, 36),            // #2D2A24
            text_dim: Color::Rgb(124, 114, 101),     // #7C7265
            text_bright: Color::Black,               // #000000
            keyword: Color::Rgb(142, 68, 173),       // #8E44AD
            string: Color::Rgb(46, 125, 50),         // #2E7D32
            comment: Color::Rgb(138, 129, 119),      // #8A8177
            function: Color::Rgb(201, 122, 0),       // #C97A00
            spinner: Color::Rgb(183, 172, 158),      // #B7AC9E
            progress: Color::Rgb(201, 122, 0),       // #C97A00
        },

        ThemeName::LightGlacier => Theme {
            // Light - Glacier
            primary: Color::Rgb(14, 165, 233),        // #0EA5E9
            secondary: Color::Rgb(109, 40, 217),      // #6D28D9
            background: Color::Rgb(244, 248, 251),    // #F4F8FB
            foreground: Color::Rgb(24, 34, 48),       // #182230
            border: Color::Rgb(151, 155, 158),        // #979B9E  (↑ contrast)
            border_focused: Color::Rgb(118, 122, 125), // toned contrast vs border
            selection: Color::Rgb(230, 243, 255),     // #E6F3FF
            cursor: Color::Rgb(24, 34, 48),           // #182230
            success: Color::Rgb(22, 163, 74),         // #16A34A
            warning: Color::Rgb(202, 138, 4),         // #CA8A04
            error: Color::Rgb(220, 38, 38),           // #DC2626
            info: Color::Rgb(2, 132, 199),            // #0284C7
            text: Color::Rgb(24, 34, 48),             // #182230
            text_dim: Color::Rgb(108, 127, 147),      // #6C7F93
            text_bright: Color::Black,                // #000000
            keyword: Color::Rgb(109, 40, 217),        // #6D28D9
            string: Color::Rgb(15, 118, 110),         // #0F766E
            comment: Color::Rgb(112, 136, 161),       // #7088A1
            function: Color::Rgb(14, 165, 233),       // #0EA5E9
            spinner: Color::Rgb(156, 178, 199),       // #9CB2C7
            progress: Color::Rgb(14, 165, 233),       // #0EA5E9
        },

        ThemeName::DarkShinobiDusk => Theme {
            // Japanese-inspired twilight theme
            primary: Color::Rgb(122, 162, 247),        // #7AA2F7
            secondary: Color::Rgb(158, 206, 106),      // #9ECE6A
            background: Color::Rgb(15, 20, 25),        // #0F1419
            foreground: Color::Rgb(192, 202, 245),     // #C0CAF5
            border: Color::Rgb(84, 89, 94),            // #54595E  (↑ contrast)
            border_focused: Color::Rgb(108, 113, 118), // toned contrast vs border
            selection: Color::Rgb(26, 33, 48),         // #1A2130
            cursor: Color::Rgb(192, 202, 245),         // #C0CAF5
            success: Color::Rgb(158, 206, 106),        // #9ECE6A
            warning: Color::Rgb(224, 175, 104),        // #E0AF68
            error: Color::Rgb(247, 118, 142),          // #F7768E
            info: Color::Rgb(125, 207, 255),           // #7DCFFF
            text: Color::Rgb(192, 202, 245),           // #C0CAF5
            text_dim: Color::Rgb(122, 136, 160),       // #7A88A0
            text_bright: Color::White,                 // #FFFFFF
            keyword: Color::Rgb(187, 154, 247),        // #BB9AF7
            string: Color::Rgb(158, 206, 106),         // #9ECE6A
            comment: Color::Rgb(91, 100, 121),         // #5B6479
            function: Color::Rgb(122, 162, 247),       // #7AA2F7
            spinner: Color::Rgb(42, 49, 64),           // #2A3140
            progress: Color::Rgb(122, 162, 247),       // #7AA2F7
        },

        ThemeName::DarkOledBlackPro => Theme {
            // True black for OLED displays with vibrant accents
            primary: Color::Rgb(0, 209, 255),        // #00D1FF
            secondary: Color::Rgb(255, 116, 208),    // #FF74D0
            background: Color::Black,                // #000000
            foreground: Color::Rgb(218, 218, 218),   // #DADADA
            border: Color::Rgb(80, 80, 80),          // #505050  (↑ contrast)
            border_focused: Color::Rgb(112, 112, 112), // toned contrast vs border
            selection: Color::Rgb(13, 13, 13),       // #0D0D0D
            cursor: Color::Rgb(218, 218, 218),       // #DADADA
            success: Color::Rgb(33, 243, 114),       // #21F372
            warning: Color::Rgb(255, 209, 102),      // #FFD166
            error: Color::Rgb(255, 59, 48),          // #FF3B30
            info: Color::Rgb(37, 194, 255),          // #25C2FF
            text: Color::Rgb(208, 208, 208),         // #D0D0D0
            text_dim: Color::Rgb(128, 128, 128),     // #808080
            text_bright: Color::White,               // #FFFFFF
            keyword: Color::Rgb(255, 116, 208),      // #FF74D0
            string: Color::Rgb(186, 255, 128),       // #BAFF80
            comment: Color::Rgb(102, 102, 102),      // #666666
            function: Color::Rgb(37, 194, 255),      // #25C2FF
            spinner: Color::Rgb(45, 45, 45),         // #2D2D2D
            progress: Color::Rgb(0, 209, 255),       // #00D1FF
        },

        ThemeName::DarkAmberTerminal => Theme {
            // Retro amber CRT monitor aesthetic
            primary: Color::Rgb(255, 176, 0),        // #FFB000
            secondary: Color::Rgb(255, 209, 138),    // #FFD18A
            background: Color::Rgb(12, 12, 8),       // #0C0C08
            foreground: Color::Rgb(255, 209, 138),   // #FFD18A
            border: Color::Rgb(85, 85, 81),          // #555551  (↑ contrast)
            border_focused: Color::Rgb(116, 116, 110), // toned contrast vs border
            selection: Color::Rgb(26, 20, 8),        // #1A1408
            cursor: Color::Rgb(255, 209, 138),       // #FFD18A
            success: Color::Rgb(255, 207, 51),       // #FFCF33
            warning: Color::Rgb(255, 158, 0),        // #FF9E00
            error: Color::Rgb(255, 94, 58),          // #FF5E3A
            info: Color::Rgb(255, 184, 77),          // #FFB84D
            text: Color::Rgb(255, 209, 138),         // #FFD18A
            text_dim: Color::Rgb(163, 131, 77),      // #A3834D
            text_bright: Color::Rgb(255, 241, 194),  // #FFF1C2
            keyword: Color::Rgb(255, 193, 77),       // #FFC14D
            string: Color::Rgb(255, 224, 138),       // #FFE08A
            comment: Color::Rgb(156, 124, 63),       // #9C7C3F
            function: Color::Rgb(255, 176, 0),       // #FFB000
            spinner: Color::Rgb(58, 45, 23),         // #3A2D17
            progress: Color::Rgb(255, 176, 0),       // #FFB000
        },

        ThemeName::DarkAuroraFlux => Theme {
            // Northern lights inspired with cool tones
            primary: Color::Rgb(142, 202, 255),        // #8ECAFF
            secondary: Color::Rgb(158, 228, 147),      // #9EE493
            background: Color::Rgb(11, 16, 32),        // #0B1020
            foreground: Color::Rgb(230, 241, 255),     // #E6F1FF
            border: Color::Rgb(82, 87, 103),           // #525767  (↑ contrast)
            border_focused: Color::Rgb(106, 111, 127), // toned contrast vs border
            selection: Color::Rgb(19, 26, 44),         // #131A2C
            cursor: Color::Rgb(230, 241, 255),         // #E6F1FF
            success: Color::Rgb(158, 228, 147),        // #9EE493
            warning: Color::Rgb(255, 212, 121),        // #FFD479
            error: Color::Rgb(255, 107, 129),          // #FF6B81
            info: Color::Rgb(142, 202, 255),           // #8ECAFF
            text: Color::Rgb(217, 230, 255),           // #D9E6FF
            text_dim: Color::Rgb(127, 140, 168),       // #7F8CA8
            text_bright: Color::White,                 // #FFFFFF
            keyword: Color::Rgb(194, 153, 255),        // #C299FF
            string: Color::Rgb(160, 255, 179),         // #A0FFB3
            comment: Color::Rgb(95, 106, 130),         // #5F6A82
            function: Color::Rgb(142, 202, 255),       // #8ECAFF
            spinner: Color::Rgb(37, 48, 74),           // #25304A
            progress: Color::Rgb(142, 202, 255),       // #8ECAFF
        },

        ThemeName::DarkCharcoalRainbow => Theme {
            // Accessible high-contrast with rainbow accents
            primary: Color::Rgb(26, 209, 255),        // #1AD1FF
            secondary: Color::Rgb(255, 138, 0),       // #FF8A00
            background: Color::Rgb(18, 18, 18),       // #121212
            foreground: Color::Rgb(232, 232, 232),    // #E8E8E8
            border: Color::Rgb(88, 88, 88),           // #585858  (↑ contrast)
            border_focused: Color::Rgb(120, 120, 120), // toned contrast vs border
            selection: Color::Rgb(26, 26, 26),        // #1A1A1A
            cursor: Color::Rgb(232, 232, 232),        // #E8E8E8
            success: Color::Rgb(0, 194, 168),         // #00C2A8
            warning: Color::Rgb(255, 160, 0),         // #FFA000
            error: Color::Rgb(255, 77, 109),          // #FF4D6D
            info: Color::Rgb(26, 209, 255),           // #1AD1FF
            text: Color::Rgb(232, 232, 232),          // #E8E8E8
            text_dim: Color::Rgb(154, 154, 154),      // #9A9A9A
            text_bright: Color::White,                // #FFFFFF
            keyword: Color::Rgb(255, 138, 0),         // #FF8A00
            string: Color::Rgb(0, 229, 255),          // #00E5FF
            comment: Color::Rgb(108, 108, 108),       // #6C6C6C
            function: Color::Rgb(179, 136, 255),      // #B388FF
            spinner: Color::Rgb(42, 42, 42),          // #2A2A2A
            progress: Color::Rgb(26, 209, 255),       // #1AD1FF
        },

        ThemeName::DarkZenGarden => Theme {
            // Calm, peaceful theme with mint and lavender
            primary: Color::Rgb(148, 226, 213),        // #94E2D5
            secondary: Color::Rgb(242, 205, 205),      // #F2CDCD
            background: Color::Rgb(16, 20, 23),        // #101417
            foreground: Color::Rgb(220, 227, 234),     // #DCE3EA
            border: Color::Rgb(85, 89, 92),            // #55595C  (↑ contrast)
            border_focused: Color::Rgb(117, 122, 125), // toned contrast vs border
            selection: Color::Rgb(23, 32, 38),         // #172026
            cursor: Color::Rgb(220, 227, 234),         // #DCE3EA
            success: Color::Rgb(166, 227, 161),        // #A6E3A1
            warning: Color::Rgb(249, 226, 175),        // #F9E2AF
            error: Color::Rgb(243, 139, 168),          // #F38BA8
            info: Color::Rgb(137, 220, 235),           // #89DCEB
            text: Color::Rgb(220, 227, 234),           // #DCE3EA
            text_dim: Color::Rgb(139, 155, 170),       // #8B9BAA
            text_bright: Color::White,                 // #FFFFFF
            keyword: Color::Rgb(203, 166, 247),        // #CBA6F7
            string: Color::Rgb(166, 227, 161),         // #A6E3A1
            comment: Color::Rgb(108, 122, 136),        // #6C7A88
            function: Color::Rgb(137, 220, 235),       // #89DCEB
            spinner: Color::Rgb(37, 49, 58),           // #25313A
            progress: Color::Rgb(148, 226, 213),       // #94E2D5
        },

        ThemeName::DarkPaperLightPro => Theme {
            // Premium paper-like light theme
            primary: Color::Rgb(0, 95, 204),        // #005FCC
            secondary: Color::Rgb(111, 66, 193),    // #6F42C1
            background: Color::Rgb(247, 247, 245),  // #F7F7F5
            foreground: Color::Rgb(27, 31, 35),     // #1B1F23
            border: Color::Rgb(154, 154, 152),      // #9A9A98  (↑ contrast)
            border_focused: Color::Rgb(122, 122, 120), // toned contrast vs border
            selection: Color::Rgb(231, 237, 243),   // #E7EDF3
            cursor: Color::Rgb(27, 31, 35),         // #1B1F23
            success: Color::Rgb(26, 127, 55),       // #1A7F37
            warning: Color::Rgb(154, 103, 0),       // #9A6700
            error: Color::Rgb(207, 34, 46),         // #CF222E
            info: Color::Rgb(9, 105, 218),          // #0969DA
            text: Color::Rgb(36, 41, 47),           // #24292F
            text_dim: Color::Rgb(87, 96, 106),      // #57606A
            text_bright: Color::Black,              // #000000
            keyword: Color::Rgb(111, 66, 193),      // #6F42C1
            string: Color::Rgb(11, 125, 105),       // #0B7D69
            comment: Color::Rgb(140, 149, 159),     // #8C959F
            function: Color::Rgb(0, 95, 204),       // #005FCC
            spinner: Color::Rgb(140, 149, 159),     // #8C959F
            progress: Color::Rgb(0, 95, 204),       // #005FCC
        },

        ThemeName::Custom => {
            // Use DarkCarbonNight (dark default) as base for custom
            get_predefined_theme(ThemeName::DarkCarbonNight)
        }
    }
}
