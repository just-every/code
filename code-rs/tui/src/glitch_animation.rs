use ratatui::buffer::Buffer;
use ratatui::prelude::*;
// Paragraph/Widget previously used; manual cell writes now keep static layer intact.

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum IntroArtSize {
    Large,
    Medium,
    Small,
    Tiny,
}

const LARGE_MIN_WIDTH: u16 = 80;
const MEDIUM_MIN_WIDTH: u16 = 56;
const SMALL_MIN_WIDTH: u16 = 50;
const LARGE_MIN_HEIGHT: u16 = 28;
const MEDIUM_MIN_HEIGHT: u16 = 21;
const SMALL_MIN_HEIGHT: u16 = 19;
const LARGE_VERSION_COLUMN: usize = 65;
const MEDIUM_VERSION_COLUMN: usize = 43;
const ANIMATED_CHARS: &[char] = &['█'];

pub fn intro_art_size_for_width(width: u16) -> IntroArtSize {
    if width >= LARGE_MIN_WIDTH {
        IntroArtSize::Large
    } else if width >= MEDIUM_MIN_WIDTH {
        IntroArtSize::Medium
    } else if width >= SMALL_MIN_WIDTH {
        IntroArtSize::Small
    } else {
        IntroArtSize::Tiny
    }
}

pub(crate) fn intro_art_size_for_area(width: u16, height: u16) -> IntroArtSize {
    if width >= LARGE_MIN_WIDTH && height >= LARGE_MIN_HEIGHT {
        IntroArtSize::Large
    } else if width >= MEDIUM_MIN_WIDTH && height >= MEDIUM_MIN_HEIGHT {
        IntroArtSize::Medium
    } else if width >= SMALL_MIN_WIDTH && height >= SMALL_MIN_HEIGHT {
        IntroArtSize::Small
    } else {
        IntroArtSize::Tiny
    }
}

pub fn intro_art_height(size: IntroArtSize) -> u16 {
    match size {
        IntroArtSize::Large => 28,
        IntroArtSize::Medium => 21,
        IntroArtSize::Small => 19,
        IntroArtSize::Tiny => 7,
    }
}

pub(crate) fn render_intro_animation_with_size_and_alpha_offset(
    area: Rect,
    buf: &mut Buffer,
    t: f32,
    alpha: f32,
    size: IntroArtSize,
    version: &str,
    row_offset: u16,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let t = t.clamp(0.0, 1.0);
    let alpha = alpha.clamp(0.0, 1.0);
    let outline_p = smoothstep(0.00, 0.60, t);
    let fill_p = smoothstep(0.35, 0.95, t);
    let fade = smoothstep(0.90, 1.00, t);
    let scan_p = smoothstep(0.55, 0.85, t);
    let frame = (t * 60.0) as u32;

    let mut lines = welcome_lines(size, version);
    let full_width = lines.iter().map(|line| line.chars().count()).max().unwrap_or(0);
    let start = row_offset as usize;
    if start >= lines.len() {
        return;
    }
    let end = (start + area.height as usize).min(lines.len());
    if start > 0 || end < lines.len() {
        lines = lines[start..end].iter().cloned().collect();
        if full_width > 0 {
            for line in &mut lines {
                let len = line.chars().count();
                if len < full_width {
                    line.push_str(&" ".repeat(full_width - len));
                }
            }
        }
    }
    let (char_mask, anim_mask, shadow_mask, w, h) =
        lines_masks(&lines, |ch| ANIMATED_CHARS.contains(&ch));
    if w == 0 || h == 0 {
        return;
    }
    let border = compute_border(&anim_mask);

    let mut render_area = area;
    render_area.height = h.min(render_area.height as usize) as u16;

    let bg = crate::colors::background();
    for y in render_area.y..render_area.y.saturating_add(render_area.height) {
        for x in render_area.x..render_area.x.saturating_add(render_area.width) {
            buf[(x, y)].set_bg(bg);
        }
    }

    let reveal_x_outline = (w as f32 * outline_p).round() as isize;
    let reveal_x_fill = (w as f32 * fill_p).round() as isize;
    let reveal_x_shadow = reveal_x_outline;

    render_static_lines(
        &lines,
        &shadow_mask,
        render_area,
        buf,
        alpha,
        frame,
        reveal_x_shadow,
    );

    let shine_x = (w as f32 * scan_p).round() as isize;
    let shine_band = 3isize;

    if alpha >= 1.0 {
        render_overlay_lines(
            &char_mask,
            &anim_mask,
            &border,
            reveal_x_outline,
            reveal_x_fill,
            shine_x,
            shine_band,
            fade,
            frame,
            render_area,
            buf,
        );
    } else {
        render_overlay_lines_with_alpha(
            &char_mask,
            &anim_mask,
            &border,
            reveal_x_outline,
            reveal_x_fill,
            shine_x,
            shine_band,
            fade,
            frame,
            alpha,
            render_area,
            buf,
        );
    }
}

/* ---------------- welcome art ---------------- */

fn welcome_lines(size: IntroArtSize, version: &str) -> Vec<String> {
    match size {
        IntroArtSize::Large => large_welcome_lines(version),
        IntroArtSize::Medium => medium_welcome_lines(version),
        IntroArtSize::Small => small_welcome_lines(version),
        IntroArtSize::Tiny => tiny_welcome_lines(version),
    }
}

const LARGE_VERSION_LINE: &str = "   ███████╗ ╚████╔╝ ███████╗██║  ██║   ██║";
const LARGE_BODY_TAIL: [&str; 22] = [
    "      █████████╗        █████████╗     ████████████╗     ███████████████╗",
    "      █████████║        █████████║     ████████████║     ███████████████║",
    "      █████████║        █████████║     ████████████║     ███████████████║",
    "   ███╔════════███╗  ███╔════════███╗  ███╔════════███╗  ███╔═══════════╝",
    "   ███║        ███║  ███║        ███║  ███║        ███║  ███║",
    "   ███║        ███║  ███║        ███║  ███║        ███║  ███║",
    "   ███║        ╚══╝  ███║        ███║  ███║        ███║  ███║",
    "   ███║              ███║        ███║  ███║        ███║  ███║",
    "   ███║              ███║        ███║  ███║        ███║  ███║",
    "   ███║              ███║        ███║  ███║        ███║  ███████████████╗",
    "   ███║              ███║        ███║  ███║        ███║  ███████████████║",
    "   ███║              ███║        ███║  ███║        ███║  ███████████████║",
    "   ███║              ███║        ███║  ███║        ███║  ███╔═══════════╝",
    "   ███║              ███║        ███║  ███║        ███║  ███║",
    "   ███║              ███║        ███║  ███║        ███║  ███║",
    "   ███║        ███╗  ███║        ███║  ███║        ███║  ███║",
    "   ███║        ███║  ███║        ███║  ███║        ███║  ███║",
    "   ███║        ███║  ███║        ███║  ███║        ███║  ███║",
    "   ╚══█████████╔══╝  ╚══█████████╔══╝  ████████████╔══╝  ███████████████╗",
    "      █████████║        █████████║     ████████████║     ███████████████║",
    "      █████████║        █████████║     ████████████║     ███████████████║",
    "      ╚════════╝        ╚════════╝      ╚══════════╝      ╚═════════════╝",
];

fn large_welcome_lines(version: &str) -> Vec<String> {
    let mut animated = vec![
        "   ███████╗██╗   ██╗███████╗██████╗ ██╗   ██╗".to_string(),
        "   ██╔════╝██║   ██║██╔════╝██╔══██╗╚██╗ ██╔╝".to_string(),
        "   █████╗  ██║   ██║█████╗  ██████╔╝ ╚████╔╝".to_string(),
        "   ██╔══╝  ╚██╗ ██╔╝██╔══╝  ██╔══██╗  ╚██╔╝".to_string(),
    ];

    let base_width = LARGE_VERSION_LINE.chars().count();
    let padding = LARGE_VERSION_COLUMN.saturating_sub(base_width);
    let _version_pad = " ".repeat(padding);
    let footer_line = "   ╚══════╝  ╚═══╝  ╚══════╝╚═╝  ╚═╝   ╚═╝";
    let footer_len = footer_line.chars().count();
    let footer_pad = LARGE_VERSION_COLUMN.saturating_sub(footer_len);
    let footer_version_pad = " ".repeat(footer_pad);
    animated.push(format!("{LARGE_VERSION_LINE}{footer_version_pad}{version}"));
    animated.push(footer_line.to_string());
    animated.extend(LARGE_BODY_TAIL.iter().map(|line| (*line).to_string()));

    shift_left(animated, 3)
}

const MEDIUM_VERSION_LINE: &str = "   ╚══════╝  ╚═══╝  ╚══════╝╚═╝  ╚═╝   ╚═╝ ";
const MEDIUM_BODY_TAIL: [&str; 15] = [
    "     ██████╗     ██████╗   ████████╗   ██████████╗",
    "     ██████║     ██████║   ████████║   ██████████║",
    "   ██╔═════██╗ ██╔═════██╗ ██╔═════██╗ ██╔═══════╝",
    "   ██║     ██║ ██║     ██║ ██║     ██║ ██║",
    "   ██║     ╚═╝ ██║     ██║ ██║     ██║ ██║",
    "   ██║         ██║     ██║ ██║     ██║ ██║",
    "   ██║         ██║     ██║ ██║     ██║ ██████████╗",
    "   ██║         ██║     ██║ ██║     ██║ ██████████║",
    "   ██║         ██║     ██║ ██║     ██║ ██╔═══════╝",
    "   ██║         ██║     ██║ ██║     ██║ ██║",
    "   ██║     ██╗ ██║     ██║ ██║     ██║ ██║",
    "   ██║     ██║ ██║     ██║ ██║     ██║ ██║",
    "   ╚═██████╔═╝ ╚═██████╔═╝ ████████╔═╝ ██████████╗",
    "     ██████║     ██████║   ████████║   ██████████║",
    "     ╚═════╝     ╚═════╝   ╚═══════╝   ╚═════════╝",
];

fn medium_welcome_lines(version: &str) -> Vec<String> {
    let mut animated = vec![
        "   ███████╗██╗   ██╗███████╗██████╗ ██╗   ██╗".to_string(),
        "   ██╔════╝██║   ██║██╔════╝██╔══██╗╚██╗ ██╔╝".to_string(),
        "   █████╗  ██║   ██║█████╗  ██████╔╝ ╚████╔╝".to_string(),
        "   ██╔══╝  ╚██╗ ██╔╝██╔══╝  ██╔══██╗  ╚██╔╝".to_string(),
        "   ███████╗ ╚████╔╝ ███████╗██║  ██║   ██║".to_string(),
    ];

    let base_width = MEDIUM_VERSION_LINE.chars().count();
    let padding = MEDIUM_VERSION_COLUMN.saturating_sub(base_width);
    let _version_pad = " ".repeat(padding);
    if let Some(first_tail) = MEDIUM_BODY_TAIL.first() {
        let tail_len = first_tail.chars().count();
        let tail_pad = MEDIUM_VERSION_COLUMN.saturating_sub(tail_len);
        let tail_version_pad = " ".repeat(tail_pad);
        animated.push(format!("{MEDIUM_VERSION_LINE}{tail_version_pad}{version}  "));
        animated.extend(MEDIUM_BODY_TAIL.iter().map(|line| (*line).to_string()));
    }
    shift_left(animated, 3)
}

const SMALL_VERSION_LINE: &str = "   ╚═════╝  ╚═══╝  ╚═════╝╚═╝  ╚═╝   ╚═╝  ";

fn small_welcome_lines(version: &str) -> Vec<String> {
    let mut lines = vec![
        "   ██████╗██╗   ██╗██████╗██████╗ ██╗   ██╗".to_string(),
        "   ██╔═══╝██║   ██║██╔═══╝██╔══██╗╚██╗ ██╔╝".to_string(),
        "   █████╗ ██║   ██║█████╗ ██████╔╝ ╚████╔╝".to_string(),
        "   ██╔══╝ ╚██╗ ██╔╝██╔══╝ ██╔══██╗  ╚██╔╝".to_string(),
        "   ██████╗ ╚████╔╝ ██████╗██║  ██║   ██║".to_string(),
    ];

    let base_width = SMALL_VERSION_LINE.chars().count();
    let padding = MEDIUM_VERSION_COLUMN.saturating_sub(base_width);
    let pad = " ".repeat(padding);
    lines.push(format!("{SMALL_VERSION_LINE}{pad}{version}  "));

    let tail = [
        "     ██████╗     ██████╗   ████████╗   ██████████╗",
        "     ██████║     ██████║   ████████║   ██████████║",
        "   ██╔═════██╗ ██╔═════██╗ ██╔═════██╗ ██╔═══════╝",
        "   ██║     ██║ ██║     ██║ ██║     ██║ ██║",
        "   ██║     ╚═╝ ██║     ██║ ██║     ██║ ██║",
        "   ██║         ██║     ██║ ██║     ██║ ██████████╗",
        "   ██║         ██║     ██║ ██║     ██║ ██████████║",
        "   ██║         ██║     ██║ ██║     ██║ ██╔═══════╝",
        "   ██║     ██╗ ██║     ██║ ██║     ██║ ██║",
        "   ██║     ██║ ██║     ██║ ██║     ██║ ██║",
        "   ╚═██████╔═╝ ╚═██████╔═╝ ████████╔═╝ ██████████╗",
        "     ██████║     ██████║   ████████║   ██████████║",
        "     ╚═════╝     ╚═════╝   ╚═══════╝   ╚═════════╝",
    ];
    lines.extend(tail.iter().map(|l| (*l).to_string()));

    shift_left(lines, 3)
}

fn tiny_welcome_lines(version: &str) -> Vec<String> {
    vec![
        format!("EVERY                 {version}    "),
        " █████╗ █████╗ █████╗ ██████╗         ".to_string(),
        "██╔═══╝██╔══██╗██╔═██╗██╔═══╝         ".to_string(),
        "██║    ██║  ██║██║ ██║████╗           ".to_string(),
        "██║    ██║  ██║██║ ██║██╔═╝           ".to_string(),
        "╚█████╗╚█████╔╝█████╔╝██████╗         ".to_string(),
        " ╚════╝ ╚════╝ ╚════╝ ╚═════╝         ".to_string(),
    ]
}

fn shift_left(lines: Vec<String>, n: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        let mut drop = n;
        let mut new_line = String::with_capacity(line.len());
        for ch in line.chars() {
            if drop > 0 && ch == ' ' {
                drop -= 1;
                continue;
            }
            new_line.push(ch);
        }
        out.push(new_line);
    }
    out
}

/* ---------------- outline fill renderer ---------------- */

fn lines_masks(
    lines: &[String],
    is_animated: impl Fn(char) -> bool,
) -> (Vec<Vec<char>>, Vec<Vec<bool>>, Vec<Vec<bool>>, usize, usize) {
    let height = lines.len();
    let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);

    let mut char_mask = vec![vec![' '; width]; height];
    let mut anim_mask = vec![vec![false; width]; height];
    let mut shadow_mask = vec![vec![false; width]; height];

    for (y, line) in lines.iter().enumerate() {
        for (x, ch) in line.chars().enumerate() {
            if x >= width {
                break;
            }
            char_mask[y][x] = ch;
            if is_animated(ch) {
                anim_mask[y][x] = true;
            } else if ch != ' ' {
                shadow_mask[y][x] = true;
            }
        }
    }

    (char_mask, anim_mask, shadow_mask, width, height)
}

fn render_static_lines(
    lines: &[String],
    shadow_mask: &[Vec<bool>],
    area: Rect,
    buf: &mut Buffer,
    alpha: f32,
    _frame: u32,
    reveal_x_shadow: isize,
) {
    let static_target = Color::Rgb(230, 232, 235); // matches CODE/EVERY final color (#e6e8eb)
    let static_color_base = blend_to_background(static_target, alpha);
    for (row_idx, line) in lines.iter().enumerate() {
        let y = area.y + row_idx as u16;
        if y >= area.y + area.height {
            break;
        }
        for (col_idx, ch) in line.chars().enumerate() {
            let x = area.x + col_idx as u16;
            if x >= area.x + area.width {
                break;
            }
            if ch == ' ' || ANIMATED_CHARS.contains(&ch) {
                continue;
            }
            if !shadow_mask[row_idx][col_idx] {
                continue;
            }
            let xi = col_idx as isize;
            if xi > reveal_x_shadow {
                continue;
            }
            let mut utf8 = [0u8; 4];
            let sym = ch.encode_utf8(&mut utf8);
            let cell = &mut buf[(x, y)];
            cell.set_symbol(sym);
            cell.set_fg(static_color_base);
            cell.set_style(Style::default().add_modifier(Modifier::BOLD));
        }
    }
}
fn render_overlay_lines(
    chars: &[Vec<char>],
    mask: &[Vec<bool>],
    border: &[Vec<bool>],
    reveal_x_outline: isize,
    reveal_x_fill: isize,
    shine_x: isize,
    shine_band: isize,
    fade: f32,
    frame: u32,
    area: Rect,
    buf: &mut Buffer,
) {
    let h = mask.len();
    let w = mask[0].len();

    for y in 0..h {
        for x in 0..w {
            let xi = x as isize;
            let base_char = chars[y][x];

            let mut draw = false;
            let mut color = Color::Reset;

            if mask[y][x] && xi <= reveal_x_fill {
                let base = gradient_multi(x as f32 / (w.max(1) as f32));
                let dx = (xi - shine_x).abs();
                let shine =
                    (1.0 - (dx as f32 / (shine_band as f32 + 0.001)).clamp(0.0, 1.0)).powf(1.6);
                let bright = bump_rgb(base, shine * 0.30);
                color = mix_rgb(bright, Color::Rgb(230, 232, 235), fade);
                draw = true;
            } else if border[y][x] && xi <= reveal_x_outline.max(reveal_x_fill) {
                let base = gradient_multi(x as f32 / (w.max(1) as f32));
                let period = 8usize;
                let on = ((x + y + (frame as usize)) % period) < (period / 2);
                let c = if on { bump_rgb(base, 0.22) } else { base };
                color = mix_rgb(c, Color::Rgb(235, 237, 240), fade * 0.8);
                draw = true;
            }

            if draw {
                let target_x = area.x + x as u16;
                let target_y = area.y + y as u16;
                if target_x < area.x + area.width && target_y < area.y + area.height {
                    let cell = &mut buf[(target_x, target_y)];
                    let mut utf8 = [0u8; 4];
                    let sym = base_char.encode_utf8(&mut utf8);
                    cell.set_symbol(sym);
                    cell.set_fg(color);
                    cell.set_bg(crate::colors::background());
                    cell.set_style(Style::default().add_modifier(Modifier::BOLD));
                }
            }
        }
    }
}

fn render_overlay_lines_with_alpha(
    chars: &[Vec<char>],
    mask: &[Vec<bool>],
    border: &[Vec<bool>],
    reveal_x_outline: isize,
    reveal_x_fill: isize,
    shine_x: isize,
    shine_band: isize,
    fade: f32,
    frame: u32,
    alpha: f32,
    area: Rect,
    buf: &mut Buffer,
) {
    let h = mask.len();
    let w = mask[0].len();

    for y in 0..h {
        for x in 0..w {
            let xi = x as isize;
            let base_char = chars[y][x];

            let mut draw = false;
            let mut color = Color::Reset;

            if mask[y][x] && xi <= reveal_x_fill {
                let base = gradient_multi(x as f32 / (w.max(1) as f32));
                let dx = (xi - shine_x).abs();
                let shine =
                    (1.0 - (dx as f32 / (shine_band as f32 + 0.001)).clamp(0.0, 1.0)).powf(1.6);
                let bright = bump_rgb(base, shine * 0.30);
                color = blend_to_background(mix_rgb(bright, Color::Rgb(230, 232, 235), fade), alpha);
                draw = true;
            } else if border[y][x] && xi <= reveal_x_outline.max(reveal_x_fill) {
                let base = gradient_multi(x as f32 / (w.max(1) as f32));
                let period = 8usize;
                let on = ((x + y + (frame as usize)) % period) < (period / 2);
                let c = if on { bump_rgb(base, 0.22) } else { base };
                color = blend_to_background(mix_rgb(c, Color::Rgb(235, 237, 240), fade * 0.8), alpha);
                draw = true;
            }

            if draw {
                let target_x = area.x + x as u16;
                let target_y = area.y + y as u16;
                if target_x < area.x + area.width && target_y < area.y + area.height {
                    let cell = &mut buf[(target_x, target_y)];
                    let mut utf8 = [0u8; 4];
                    let sym = base_char.encode_utf8(&mut utf8);
                    cell.set_symbol(sym);
                    cell.set_fg(color);
                    cell.set_bg(crate::colors::background());
                    cell.set_style(Style::default().add_modifier(Modifier::BOLD));
                }
            }
        }
    }
}

// Helper function to blend colors towards background
pub(crate) fn blend_to_background(color: Color, alpha: f32) -> Color {
    if alpha >= 1.0 {
        return color;
    }
    if alpha <= 0.0 {
        return crate::colors::background();
    }

    let bg = crate::colors::background();

    match (color, bg) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let r = (r1 as f32 * alpha + r2 as f32 * (1.0 - alpha)) as u8;
            let g = (g1 as f32 * alpha + g2 as f32 * (1.0 - alpha)) as u8;
            let b = (b1 as f32 * alpha + b2 as f32 * (1.0 - alpha)) as u8;
            Color::Rgb(r, g, b)
        }
        _ => {
            if alpha > 0.5 { color } else { bg }
        }
    }
}

/* ---------------- border computation ---------------- */

fn compute_border(mask: &[Vec<bool>]) -> Vec<Vec<bool>> {
    let h = mask.len();
    let w = mask[0].len();
    let mut out = vec![vec![false; w]; h];
    for y in 0..h {
        for x in 0..w {
            if !mask[y][x] {
                continue;
            }
            let up = y == 0 || !mask[y - 1][x];
            let down = y + 1 >= h || !mask[y + 1][x];
            let left = x == 0 || !mask[y][x - 1];
            let right = x + 1 >= w || !mask[y][x + 1];
            if up || down || left || right {
                out[y][x] = true;
            }
        }
    }
    out
}

/* ================= helpers ================= */

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

pub(crate) fn mix_rgb(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            Color::Rgb(lerp_u8(ar, br, t), lerp_u8(ag, bg, t), lerp_u8(ab, bb, t))
        }
        _ => b,
    }
}

// vibrant cyan -> magenta -> amber across the word
pub(crate) fn gradient_multi(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (r1, g1, b1) = (0u8, 224u8, 255u8); // #00E0FF
    let (r2, g2, b2) = (255u8, 78u8, 205u8); // #FF4ECD
    let (r3, g3, b3) = (255u8, 181u8, 0u8); // #FFB500
    if t < 0.5 {
        Color::Rgb(
            lerp_u8(r1, r2, t * 2.0),
            lerp_u8(g1, g2, t * 2.0),
            lerp_u8(b1, b2, t * 2.0),
        )
    } else {
        Color::Rgb(
            lerp_u8(r2, r3, (t - 0.5) * 2.0),
            lerp_u8(g2, g3, (t - 0.5) * 2.0),
            lerp_u8(b2, b3, (t - 0.5) * 2.0),
        )
    }
}

fn bump_rgb(c: Color, amt: f32) -> Color {
    match c {
        Color::Rgb(r, g, b) => {
            let add = |x: u8| ((x as f32 + 255.0 * amt).min(255.0)) as u8;
            Color::Rgb(add(r), add(g), add(b))
        }
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use ratatui::buffer::Buffer;
    use ratatui::prelude::Rect;

    #[test]
    fn renders_large_art_pixel_perfect() {
        let version = format!("v{}", code_version::version());
        let expected = expected_large(&version);
        let width = expected.iter().map(|l| l.chars().count()).max().unwrap() as u16;
        let height = expected.len() as u16;
        let rect = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(rect);

        render_intro_animation_with_size_and_alpha_offset(
            rect,
            &mut buf,
            1.0,
            1.0,
            IntroArtSize::Large,
            &version,
            0,
        );

        let rendered = buffer_to_strings(&buf, rect);
        assert_eq!(trim_lines(rendered), trim_lines(expected));
    }

    #[test]
    fn renders_medium_art_pixel_perfect() {
        let version = format!("v{}", code_version::version());
        let expected = expected_medium(&version);
        let width = expected.iter().map(|l| l.chars().count()).max().unwrap() as u16;
        let height = expected.len() as u16;
        let rect = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(rect);

        render_intro_animation_with_size_and_alpha_offset(
            rect,
            &mut buf,
            1.0,
            1.0,
            IntroArtSize::Medium,
            &version,
            0,
        );

        let rendered = buffer_to_strings(&buf, rect);
        assert_eq!(trim_lines(rendered), trim_lines(expected));
    }

    #[test]
    fn renders_small_art_pixel_perfect() {
        let version = format!("v{}", code_version::version());
        let expected = vec!["██████╗██╗   ██╗█".to_string()];
        let width = expected[0].chars().count() as u16;
        let rect = Rect::new(0, 0, width, 1);
        let mut buf = Buffer::empty(rect);

        render_intro_animation_with_size_and_alpha_offset(
            rect,
            &mut buf,
            1.0,
            1.0,
            IntroArtSize::Small,
            &version,
            0,
        );

        let rendered = buffer_to_strings(&buf, rect);
        assert_eq!(trim_lines(rendered), trim_lines(expected));
    }

    fn buffer_to_strings(buf: &Buffer, area: Rect) -> Vec<String> {
        let mut lines = Vec::new();
        for y in area.y..area.y + area.height {
            let mut line = String::with_capacity(area.width as usize);
            for x in area.x..area.x + area.width {
                let symbol = buf[(x, y)].symbol();
                let ch = symbol.chars().next().unwrap_or(' ');
                line.push(ch);
            }
            lines.push(line);
        }
        lines
    }

    fn trim_lines(lines: Vec<String>) -> Vec<String> {
        lines
            .into_iter()
            .map(|line| line.trim_end().to_string())
            .collect()
    }

    fn expected_large(version: &str) -> Vec<String> {
        let art = indoc! {"
           ███████╗██╗   ██╗███████╗██████╗ ██╗   ██╗
           ██╔════╝██║   ██║██╔════╝██╔══██╗╚██╗ ██╔╝
           █████╗  ██║   ██║█████╗  ██████╔╝ ╚████╔╝
           ██╔══╝  ╚██╗ ██╔╝██╔══╝  ██╔══██╗  ╚██╔╝
           ███████╗ ╚████╔╝ ███████╗██║  ██║   ██║                       {VERSION}
           ╚══════╝  ╚═══╝  ╚══════╝╚═╝  ╚═╝   ╚═╝
              █████████╗        █████████╗     ████████████╗     ███████████████╗
              █████████║        █████████║     ████████████║     ███████████████║
              █████████║        █████████║     ████████████║     ███████████████║
           ███╔════════███╗  ███╔════════███╗  ███╔════════███╗  ███╔═══════════╝
           ███║        ███║  ███║        ███║  ███║        ███║  ███║
           ███║        ███║  ███║        ███║  ███║        ███║  ███║
           ███║        ╚══╝  ███║        ███║  ███║        ███║  ███║
           ███║              ███║        ███║  ███║        ███║  ███║
           ███║              ███║        ███║  ███║        ███║  ███║
           ███║              ███║        ███║  ███║        ███║  ███████████████╗
           ███║              ███║        ███║  ███║        ███║  ███████████████║
           ███║              ███║        ███║  ███║        ███║  ███████████████║
           ███║              ███║        ███║  ███║        ███║  ███╔═══════════╝
           ███║              ███║        ███║  ███║        ███║  ███║
           ███║              ███║        ███║  ███║        ███║  ███║
           ███║        ███╗  ███║        ███║  ███║        ███║  ███║
           ███║        ███║  ███║        ███║  ███║        ███║  ███║
           ███║        ███║  ███║        ███║  ███║        ███║  ███║
           ╚══█████████╔══╝  ╚══█████████╔══╝  ████████████╔══╝  ███████████████╗
              █████████║        █████████║     ████████████║     ███████████████║
              █████████║        █████████║     ████████████║     ███████████████║
              ╚════════╝        ╚════════╝      ╚══════════╝      ╚═════════════╝
        "}
        .replace("{VERSION}", version);

        art.lines().map(|line| line.to_string()).collect()
    }

    fn expected_medium(version: &str) -> Vec<String> {
        let art = indoc! {"
           ███████╗██╗   ██╗███████╗██████╗ ██╗   ██╗
           ██╔════╝██║   ██║██╔════╝██╔══██╗╚██╗ ██╔╝
           █████╗  ██║   ██║█████╗  ██████╔╝ ╚████╔╝
           ██╔══╝  ╚██╗ ██╔╝██╔══╝  ██╔══██╗  ╚██╔╝
           ███████╗ ╚████╔╝ ███████╗██║  ██║   ██║
           ╚══════╝  ╚═══╝  ╚══════╝╚═╝  ╚═╝   ╚═╝ {VERSION}  
             ██████╗     ██████╗   ████████╗   ██████████╗
             ██████║     ██████║   ████████║   ██████████║
           ██╔═════██╗ ██╔═════██╗ ██╔═════██╗ ██╔═══════╝
           ██║     ██║ ██║     ██║ ██║     ██║ ██║
           ██║     ╚═╝ ██║     ██║ ██║     ██║ ██║
           ██║         ██║     ██║ ██║     ██║ ██║
           ██║         ██║     ██║ ██║     ██║ ██████████╗
           ██║         ██║     ██║ ██║     ██║ ██████████║
           ██║         ██║     ██║ ██║     ██║ ██╔═══════╝
           ██║         ██║     ██║ ██║     ██║ ██║
           ██║     ██╗ ██║     ██║ ██║     ██║ ██║
           ██║     ██║ ██║     ██║ ██║     ██║ ██║
           ╚═██████╔═╝ ╚═██████╔═╝ ████████╔═╝ ██████████╗
             ██████║     ██████║   ████████║   ██████████║
             ╚═════╝     ╚═════╝   ╚═══════╝   ╚═════════╝
        "}
        .replace("{VERSION}", version);

        art.lines().map(|line| line.to_string()).collect()
    }
}
