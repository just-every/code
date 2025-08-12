use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Widget};
use ratatui::buffer::Buffer;

// Render the outline-fill animation
pub fn render_intro_animation(
    area: Rect,
    buf: &mut Buffer,
    t: f32,
) {
    render_intro_outline_fill(area, buf, t)
}

// Render the outline-fill animation with alpha blending for fade-out
pub fn render_intro_animation_with_alpha(
    area: Rect,
    buf: &mut Buffer,
    t: f32,
    alpha: f32,
) {
    render_intro_outline_fill_with_alpha(area, buf, t, alpha)
}

// Outline fill animation - inline, no borders
pub fn render_intro_outline_fill(
    area: Rect,
    buf: &mut Buffer,
    t: f32,
) {
    if area.width < 40 || area.height < 10 { return; }

    let t = t.clamp(0.0, 1.0);
    let outline_p = smoothstep(0.00, 0.60, t); // outline draws L->R
    let fill_p    = smoothstep(0.35, 0.95, t); // interior fills L->R
    let fade      = smoothstep(0.90, 1.00, t); // vibrant -> muted gray
    let scan_p    = smoothstep(0.55, 0.85, t); // scanline sweep
    let frame     = (t * 60.0) as u32;

    // Build scaled mask + border map
    let (scale, mask, w, h) = scaled_mask("CODER", area.width.saturating_sub(2), area.height.saturating_sub(2));
    let border = compute_border(&mask);

    // Use area directly for inline rendering
    let mut r = area;
    r.height = h.min(area.height as usize) as u16;
    r.width = w.min(area.width as usize) as u16;

    let reveal_x_outline = (w as f32 * outline_p).round() as isize;
    let reveal_x_fill    = (w as f32 * fill_p).round() as isize;
    let shine_x          = (w as f32 * scan_p).round() as isize;
    let shine_band       = scale.max(2) as isize;

    let lines = mask_to_outline_fill_lines(
        &mask, &border, reveal_x_outline, reveal_x_fill, shine_x, shine_band, fade, frame, scale,
    );

    Paragraph::new(lines).alignment(Alignment::Left).render(r, buf);
}

// Outline fill animation with alpha blending - inline, no borders
pub fn render_intro_outline_fill_with_alpha(
    area: Rect,
    buf: &mut Buffer,
    t: f32,
    alpha: f32,
) {
    if area.width < 40 || area.height < 10 { return; }

    let t = t.clamp(0.0, 1.0);
    let alpha = alpha.clamp(0.0, 1.0);
    let outline_p = smoothstep(0.00, 0.60, t); // outline draws L->R
    let fill_p    = smoothstep(0.35, 0.95, t); // interior fills L->R
    let fade      = smoothstep(0.90, 1.00, t); // vibrant -> muted gray
    let scan_p    = smoothstep(0.55, 0.85, t); // scanline sweep
    let frame     = (t * 60.0) as u32;

    // Build scaled mask + border map
    let (scale, mask, w, h) = scaled_mask("CODER", area.width.saturating_sub(2), area.height.saturating_sub(2));
    let border = compute_border(&mask);

    // Use area directly for inline rendering
    let mut r = area;
    r.height = h.min(area.height as usize) as u16;
    r.width = w.min(area.width as usize) as u16;

    let reveal_x_outline = (w as f32 * outline_p).round() as isize;
    let reveal_x_fill    = (w as f32 * fill_p).round() as isize;
    let shine_x          = (w as f32 * scan_p).round() as isize;
    let shine_band       = scale.max(2) as isize;

    let lines = mask_to_outline_fill_lines_with_alpha(
        &mask, &border, reveal_x_outline, reveal_x_fill, shine_x, shine_band, fade, frame, scale, alpha,
    );

    Paragraph::new(lines).alignment(Alignment::Left).render(r, buf);
}

/* ---------------- outline fill renderer ---------------- */

fn mask_to_outline_fill_lines(
    mask: &Vec<Vec<bool>>,
    border: &Vec<Vec<bool>>,
    reveal_x_outline: isize,
    reveal_x_fill: isize,
    shine_x: isize,
    shine_band: isize,
    fade: f32,
    frame: u32,
    scale: usize,
) -> Vec<Line<'static>> {
    let h = mask.len();
    let w = mask[0].len();
    let mut out = Vec::with_capacity(h);

    for y in 0..h {
        let mut spans: Vec<Span> = Vec::with_capacity(w);
        for x in 0..w {
            let xi = x as isize;

            // precedence: filled interior > outline > empty
            let mut ch = ' ';
            let mut color = Color::Reset;

            // Interior fill (█) once revealed
            if mask[y][x] && xi <= reveal_x_fill {
                let base = gradient_multi(x as f32 / (w.max(1) as f32));
                let dx = (xi - shine_x).abs();
                let shine = (1.0 - (dx as f32 / (shine_band as f32 + 0.001)).clamp(0.0, 1.0)).powf(1.6);
                let bright = bump_rgb(base, shine * 0.30);
                color = mix_rgb(bright, Color::Rgb(145,150,160), fade);
                ch = '█';
            }
            // Outline (▓) for border pixels
            else if border[y][x] && xi <= reveal_x_outline.max(reveal_x_fill) {
                let base = gradient_multi(x as f32 / (w.max(1) as f32));
                // marching ants along diagonals
                let period = (2*scale_or(scale, 4)) as usize; // ~scale-based speed/size
                let on = ((x + y + (frame as usize)) % period) < (period/2);
                let c = if on { bump_rgb(base, 0.22) } else { base };
                color = mix_rgb(c, Color::Rgb(160,165,172), fade*0.8);
                ch = '▓';
            }

            spans.push(Span::styled(
                ch.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
        }
        out.push(Line::from(spans));
    }
    out
}

fn mask_to_outline_fill_lines_with_alpha(
    mask: &Vec<Vec<bool>>,
    border: &Vec<Vec<bool>>,
    reveal_x_outline: isize,
    reveal_x_fill: isize,
    shine_x: isize,
    shine_band: isize,
    fade: f32,
    frame: u32,
    scale: usize,
    alpha: f32,
) -> Vec<Line<'static>> {
    let h = mask.len();
    let w = mask[0].len();
    let mut out = Vec::with_capacity(h);

    for y in 0..h {
        let mut spans: Vec<Span> = Vec::with_capacity(w);
        for x in 0..w {
            let xi = x as isize;

            // precedence: filled interior > outline > empty
            let mut ch = ' ';
            let mut color = Color::Reset;

            // Interior fill (█) once revealed
            if mask[y][x] && xi <= reveal_x_fill {
                let base = gradient_multi(x as f32 / (w.max(1) as f32));
                let dx = (xi - shine_x).abs();
                let shine = (1.0 - (dx as f32 / (shine_band as f32 + 0.001)).clamp(0.0, 1.0)).powf(1.6);
                let bright = bump_rgb(base, shine * 0.30);
                let mut final_color = mix_rgb(bright, Color::Rgb(145,150,160), fade);
                
                // Apply alpha blending to background color
                final_color = blend_to_background(final_color, alpha);
                color = final_color;
                ch = '█';
            }
            // Outline (▓) for border pixels
            else if border[y][x] && xi <= reveal_x_outline.max(reveal_x_fill) {
                let base = gradient_multi(x as f32 / (w.max(1) as f32));
                // marching ants along diagonals
                let period = (2*scale_or(scale, 4)) as usize; // ~scale-based speed/size
                let on = ((x + y + (frame as usize)) % period) < (period/2);
                let c = if on { bump_rgb(base, 0.22) } else { base };
                let mut final_color = mix_rgb(c, Color::Rgb(160,165,172), fade*0.8);
                
                // Apply alpha blending to background color
                final_color = blend_to_background(final_color, alpha);
                color = final_color;
                ch = '▓';
            }

            spans.push(Span::styled(
                ch.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
        }
        out.push(Line::from(spans));
    }
    out
}

// Helper function to blend colors towards background
fn blend_to_background(color: Color, alpha: f32) -> Color {
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
            // For non-RGB colors, just use alpha to decide between foreground and background
            if alpha > 0.5 { color } else { bg }
        }
    }
}

/* ---------------- border computation ---------------- */

fn compute_border(mask: &Vec<Vec<bool>>) -> Vec<Vec<bool>> {
    let h = mask.len(); 
    let w = mask[0].len();
    let mut out = vec![vec![false; w]; h];
    for y in 0..h {
        for x in 0..w {
            if !mask[y][x] { continue; }
            let up    = y == 0        || !mask[y-1][x];
            let down  = y+1 >= h      || !mask[y+1][x];
            let left  = x == 0        || !mask[y][x-1];
            let right = x+1 >= w      || !mask[y][x+1];
            if up || down || left || right { out[y][x] = true; }
        }
    }
    out
}

/* ================= helpers ================= */

fn scale_or(scale: usize, min: usize) -> usize { 
    if scale < min { min } else { scale } 
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 { (a as f32 + (b as f32 - a as f32) * t).round() as u8 }

fn mix_rgb(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb(ar,ag,ab), Color::Rgb(br,bg,bb)) =>
            Color::Rgb(lerp_u8(ar,br,t), lerp_u8(ag,bg,t), lerp_u8(ab,bb,t)),
        _ => b
    }
}

// vibrant cyan -> magenta -> amber across the word
fn gradient_multi(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (r1,g1,b1) = (0u8, 224u8, 255u8);   // #00E0FF
    let (r2,g2,b2) = (255u8, 78u8, 205u8);  // #FF4ECD
    let (r3,g3,b3) = (255u8, 181u8, 0u8);   // #FFB500
    if t < 0.5 {
        Color::Rgb(lerp_u8(r1,r2,t*2.0), lerp_u8(g1,g2,t*2.0), lerp_u8(b1,b2,t*2.0))
    } else {
        Color::Rgb(lerp_u8(r2,r3,(t-0.5)*2.0), lerp_u8(g2,g3,(t-0.5)*2.0), lerp_u8(b2,b3,(t-0.5)*2.0))
    }
}

fn bump_rgb(c: Color, amt: f32) -> Color {
    match c {
        Color::Rgb(r,g,b) => {
            let add = |x:u8| ((x as f32 + 255.0*amt).min(255.0)) as u8;
            Color::Rgb(add(r), add(g), add(b))
        }
        _ => c
    }
}

// Scale a 5×7 "CODER" bitmap to fill `max_w` x `max_h`, returning (scale, grid, w, h)
fn scaled_mask(word: &str, max_w: u16, max_h: u16) -> (usize, Vec<Vec<bool>>, usize, usize) {
    let rows = 7usize;
    let w = 5usize;
    let gap = 1usize;
    let letters: Vec<[&'static str; 7]> = word.chars().map(glyph_5x7).collect();
    let cols = letters.len() * w + (letters.len().saturating_sub(1)) * gap;

    let mut scale = 16usize;
    while scale > 1 && (cols * scale > max_w as usize || rows * scale > max_h as usize) {
        scale -= 1;
    }
    if scale == 0 { scale = 1; }

    let mut grid = vec![vec![false; cols * scale]; rows * scale];
    let mut xoff = 0usize;

    for g in letters {
        for row in 0..rows {
            let line = g[row].as_bytes();
            for col in 0..w {
                if line[col] == b'#' {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            grid[row * scale + dy][(xoff + col) * scale + dx] = true;
                        }
                    }
                }
            }
        }
        xoff += w + gap;
    }
    (scale, grid, cols * scale, rows * scale)
}

// 5×7 glyphs for C O D E R
fn glyph_5x7(ch: char) -> [&'static str; 7] {
    match ch {
        'C' => [" ### ","#   #","#    ","#    ","#    ","#   #"," ### "],
        'O' => [" ### ","#   #","#   #","#   #","#   #","#   #"," ### "],
        'D' => ["#### ","#   #","#   #","#   #","#   #","#   #","#### "],
        'E' => ["#####","#    ","#    ","#####","#    ","#    ","#####"],
        'R' => ["#### ","#   #","#   #","#### ","# #  ","#  # ","#   #"],
        _   => ["#####","#####","#####","#####","#####","#####","#####"],
    }
}