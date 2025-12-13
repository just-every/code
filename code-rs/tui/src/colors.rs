use ratatui::style::Color;
use crate::theme::{current_theme, palette_mode, quantize_color_for_palette, PaletteMode};

// Legacy color constants - now redirect to theme
pub(crate) fn light_blue() -> Color {
    current_theme().primary
}

pub(crate) fn success_green() -> Color {
    current_theme().success
}

pub(crate) fn success() -> Color {
    current_theme().success
}

pub(crate) fn warning() -> Color {
    current_theme().warning
}

pub(crate) fn error() -> Color {
    current_theme().error
}

// Convenience functions for common theme colors
pub(crate) fn primary() -> Color {
    current_theme().primary
}

#[allow(dead_code)]
pub(crate) fn secondary() -> Color {
    current_theme().secondary
}

pub(crate) fn border() -> Color {
    current_theme().border
}

/// A slightly dimmer variant of the standard border color.
/// Blends the theme border toward the background by 30% to reduce contrast
/// while preserving the original hue relationship.
pub(crate) fn border_dim() -> Color {
    match palette_mode() {
        PaletteMode::Ansi16 => {
            if is_dark_background(current_theme().background) {
                Color::Indexed(8)
            } else {
                Color::Indexed(8)
            }
        }
        PaletteMode::Ansi256 => {
            let b = current_theme().border;
            let bg = current_theme().background;
            let (br, bg_g, bb) = color_to_rgb(b);
            let (rr, rg, rb) = color_to_rgb(bg);
            let t: f32 = 0.30; // 30% toward background
            let mix = |a: u8, b: u8| -> u8 { ((a as f32) * (1.0 - t) + (b as f32) * t).round() as u8 };
            let r = mix(br, rr);
            let g = mix(bg_g, rg);
            let bl = mix(bb, rb);
            quantize_color_for_palette(Color::Rgb(r, g, bl))
        }
    }
}

fn is_dark_background(color: Color) -> bool {
    matches!(color, Color::Indexed(0) | Color::Black)
}

#[allow(dead_code)]
pub(crate) fn border_focused() -> Color {
    current_theme().border_focused
}

pub(crate) fn text() -> Color {
    current_theme().text
}

pub(crate) fn text_dim() -> Color {
    current_theme().text_dim
}

pub(crate) fn text_bright() -> Color {
    current_theme().text_bright
}

pub(crate) fn spinner() -> Color {
    current_theme().spinner
}

/// Midpoint color between `text` and `text_dim` for secondary list levels.
pub(crate) fn text_mid() -> Color {
    match palette_mode() {
        PaletteMode::Ansi16 => {
            if is_dark_background(current_theme().background) {
                Color::Indexed(7)
            } else {
                Color::Indexed(8)
            }
        }
        PaletteMode::Ansi256 => {
            let a = current_theme().text;
            let b = current_theme().text_dim;
            mix_toward(a, b, 0.5)
        }
    }
}

pub(crate) fn info() -> Color {
    current_theme().info
}

// Alias for text_dim
pub(crate) fn dim() -> Color {
    text_dim()
}

pub(crate) fn background() -> Color {
    current_theme().background
}

#[allow(dead_code)]
pub(crate) fn selection() -> Color {
    current_theme().selection
}

// Syntax/special helpers
pub(crate) fn function() -> Color {
    current_theme().function
}

pub(crate) fn keyword() -> Color {
    current_theme().keyword
}

// Overlay/scrim helper: a dimmed background used behind modal overlays.
// We derive it from the current theme background so it looks consistent for
// both light and dark themes.
pub(crate) fn color_to_rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0, 0, 0),
        Color::White => (255, 255, 255),
        Color::Gray => (192, 192, 192),
        Color::DarkGray => (128, 128, 128),
        Color::Red => (205, 49, 49),
        Color::Green => (13, 188, 121),
        Color::Yellow => (229, 229, 16),
        Color::Blue => (36, 114, 200),
        Color::Magenta => (188, 63, 188),
        Color::Cyan => (17, 168, 205),
        Color::LightRed => (255, 102, 102),
        Color::LightGreen => (102, 255, 178),
        Color::LightYellow => (255, 255, 102),
        Color::LightBlue => (102, 153, 255),
        Color::LightMagenta => (255, 102, 255),
        Color::LightCyan => (102, 255, 255),
        // Correct mapping for ANSI-256 indexes used when we quantize themes.
        // This avoids treating all Indexed colors as grayscale and fixes
        // luminance decisions (e.g., mistaking light themes for dark) on
        // terminals that don’t advertise truecolor, including some Windows setups.
        Color::Indexed(i) => ansi256_to_rgb(i),
        // When theme background is Color::Reset (to use terminal default),
        // avoid recursion by treating Reset as pure white in RGB space.
        Color::Reset => (255, 255, 255),
    }
}

// Convert an ANSI-256 color index into an approximate RGB triple using the
// standard xterm 256-color palette: 0–15 ANSI, 16–231 6×6×6 cube, 232–255 grayscale.
fn ansi256_to_rgb(i: u8) -> (u8, u8, u8) {
    // ANSI 16 base colors
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

    if i < 16 {
        return ANSI16[i as usize];
    }
    if (16..=231).contains(&i) {
        // 6×6×6 color cube
        let idx = i - 16;
        let r = idx / 36;
        let g = (idx % 36) / 6;
        let b = idx % 6;
        let step = [0, 95, 135, 175, 215, 255];
        return (step[r as usize], step[g as usize], step[b as usize]);
    }
    // Grayscale ramp 232–255 maps to 8,18,28,...,238
    let level = i.saturating_sub(232);
    let v = 8 + 10 * level;
    (v, v, v)
}

fn blend_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let inv = 1.0 - t;
    let r = (a.0 as f32 * inv + b.0 as f32 * t).round() as u8;
    let g = (a.1 as f32 * inv + b.1 as f32 * t).round() as u8;
    let bl = (a.2 as f32 * inv + b.2 as f32 * t).round() as u8;
    (r, g, bl)
}

/// Blend `from` toward `to` by fraction `t` (0.0..=1.0) in RGB space.
#[allow(dead_code)]
pub(crate) fn mix_toward(from: Color, to: Color, t: f32) -> Color {
    let a = color_to_rgb(from);
    let b = color_to_rgb(to);
    let (r, g, b) = blend_rgb(a, b, t.clamp(0.0, 1.0));
    quantize_color_for_palette(Color::Rgb(r, g, b))
}

fn is_dark_rgb(rgb: (u8, u8, u8)) -> bool {
    let l = (0.2126 * rgb.0 as f32 + 0.7152 * rgb.1 as f32 + 0.0722 * rgb.2 as f32) / 255.0;
    l < 0.55
}

/// Lightly tint the terminal background toward an accent color. Matches the
/// blending used for success backgrounds in diff rendering so shared surfaces
/// stay consistent.
pub(crate) fn tint_background_toward(accent: Color) -> Color {
    let bg = color_to_rgb(background());
    let fg = color_to_rgb(accent);
    let alpha = if is_dark_rgb(bg) { 0.20 } else { 0.10 };
    let (r, g, b) = blend_rgb(bg, fg, alpha);
    Color::Rgb(r, g, b)
}

fn blend_with_black(rgb: (u8, u8, u8), alpha: f32) -> (u8, u8, u8) {
    // target = bg*(1-alpha) + black*alpha => bg*(1-alpha)
    let inv = 1.0 - alpha;
    let r = (rgb.0 as f32 * inv).round() as u8;
    let g = (rgb.1 as f32 * inv).round() as u8;
    let b = (rgb.2 as f32 * inv).round() as u8;
    (r, g, b)
}

fn is_light(rgb: (u8, u8, u8)) -> bool {
    let l = (0.2126 * rgb.0 as f32 + 0.7152 * rgb.1 as f32 + 0.0722 * rgb.2 as f32) / 255.0;
    l >= 0.6
}

fn relative_luminance(rgb: (u8, u8, u8)) -> f32 {
    (0.2126 * rgb.0 as f32 + 0.7152 * rgb.1 as f32 + 0.0722 * rgb.2 as f32) / 255.0
}

pub(crate) fn overlay_scrim() -> Color {
    let bg = current_theme().background;
    let rgb = color_to_rgb(bg);
    // For light themes, use a slightly stronger darkening; for dark themes, a gentler one.
    let alpha = if is_light(rgb) { 0.18 } else { 0.10 };
    let (r, g, b) = blend_with_black(rgb, alpha);
    quantize_color_for_palette(Color::Rgb(r, g, b))
}

/// Background for assistant messages: theme background moved 5% toward theme info.
pub(crate) fn assistant_bg() -> Color {
    match palette_mode() {
        PaletteMode::Ansi16 => {
            if is_dark_background(current_theme().background) {
                Color::Indexed(4)
            } else {
                Color::Indexed(7)
            }
        }
        PaletteMode::Ansi256 => {
            let bg = current_theme().background;
            let info = current_theme().info;
            mix_toward(bg, info, 0.05)
        }
    }
}

/// Background for mid-turn assistant messages.
///
/// Uses a lighter tint than `assistant_bg` so progress inserts feel secondary.
pub(crate) fn assistant_mid_turn_bg() -> Color {
    match palette_mode() {
        PaletteMode::Ansi16 => assistant_bg(),
        PaletteMode::Ansi256 => {
            let bg = current_theme().background;
            let info = current_theme().info;
            mix_toward(bg, info, 0.02)
        }
    }
}

/// Background for multiline code blocks rendered in assistant markdown.
///
/// New behavior: match the assistant message background so code cards feel
/// integrated with the transcript instead of appearing as stark white/black
/// panels. Borders and inner padding also use this same background.
pub(crate) fn code_block_bg() -> Color {
    assistant_bg()
}

/// Color for horizontal rules inside assistant messages.
/// Defined as halfway from the theme background toward the assistant background tint.
/// This makes the rule more pronounced than the cell background while staying subtle.
pub(crate) fn assistant_hr() -> Color {
    match palette_mode() {
        PaletteMode::Ansi16 => {
            if is_dark_background(current_theme().background) {
                Color::Indexed(8)
            } else {
                Color::Indexed(7)
            }
        }
        PaletteMode::Ansi256 => {
            let bg = current_theme().background;
            let info = current_theme().info;
            let cell = assistant_bg();
            let candidate = mix_toward(bg, info, 0.15);
            let cand_l = relative_luminance(color_to_rgb(candidate));
            let cell_l = relative_luminance(color_to_rgb(cell));
            let result = if cand_l < cell_l {
                candidate
            } else {
                let (r, g, b) = blend_with_black(color_to_rgb(cell), 0.12);
                Color::Rgb(r, g, b)
            };
            quantize_color_for_palette(result)
        }
    }
}
