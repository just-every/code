use ratatui::buffer::Buffer;
use ratatui::prelude::*;
use ratatui::style::Color;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::card_theme;
use crate::card_theme::{CardThemeDefinition, GradientSpec};
use crate::colors;
use crate::gradient_background::GradientBackground;
use crate::theme::{palette_mode, PaletteMode};
use crate::util::buffer::fill_rect;

#[derive(Clone, Copy)]
pub(crate) struct CardStyle {
    pub accent_fg: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub title_text: Color,
    pub gradient: GradientSpec,
}

#[derive(Clone, Debug)]
pub(crate) struct CardSegment {
    pub text: String,
    pub style: Style,
    pub inherit_background: bool,
}

impl CardSegment {
    pub fn new(text: String, style: Style) -> Self {
        Self {
            text,
            style,
            inherit_background: true,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CardRow {
    pub accent: String,
    pub accent_style: Style,
    pub segments: Vec<CardSegment>,
    pub body_bg: Option<Color>,
}

impl CardRow {
    pub fn new(
        accent: String,
        accent_style: Style,
        segments: Vec<CardSegment>,
        body_bg: Option<Color>,
    ) -> Self {
        Self {
            accent,
            accent_style,
            segments,
            body_bg,
        }
    }
}

pub(crate) const CARD_ACCENT_WIDTH: usize = 2;

pub(crate) fn agent_card_style(_write_enabled: Option<bool>) -> CardStyle {
    // Agent batches share the calmer green theme in full-color terminals.
    // In ANSI-16 mode we keep the surface transparent and rely on inverted text.
    let is_dark = is_dark_theme_active();
    let definition = if is_dark {
        card_theme::agent_read_only_dark_theme()
    } else {
        card_theme::agent_read_only_light_theme()
    };
    let mut style = style_from_theme(definition, is_dark);

    if palette_mode() == PaletteMode::Ansi16 {
        strip_ansi16_background(&mut style);
    }

    style
}

pub(crate) fn browser_card_style() -> CardStyle {
    let is_dark = is_dark_theme_active();
    let definition = if is_dark {
        card_theme::browser_dark_theme()
    } else {
        card_theme::browser_light_theme()
    };
    let mut style = style_from_theme(definition, is_dark);

    if palette_mode() == PaletteMode::Ansi16 {
        strip_ansi16_background(&mut style);
    }

    style
}

pub(crate) fn auto_drive_card_style() -> CardStyle {
    let is_dark = is_dark_theme_active();
    let definition = if is_dark {
        card_theme::auto_drive_dark_theme()
    } else {
        card_theme::auto_drive_light_theme()
    };
    let mut style = style_from_theme(definition, is_dark);

    if palette_mode() == PaletteMode::Ansi16 {
        strip_ansi16_background(&mut style);
        let text_color = ansi16_inverse_color();
        style.title_text = text_color;
        style.accent_fg = text_color;
        style.text_primary = text_color;
        style.text_secondary = colors::warning();
    }

    style
}

pub(crate) fn web_search_card_style() -> CardStyle {
    let is_dark = is_dark_theme_active();
    let definition = if is_dark {
        card_theme::search_dark_theme()
    } else {
        card_theme::search_light_theme()
    };
    let mut style = style_from_theme(definition, is_dark);

    if palette_mode() == PaletteMode::Ansi16 {
        strip_ansi16_background(&mut style);
    }

    style
}

pub(crate) fn ansi16_inverse_color() -> Color {
    if is_dark_theme_active() {
        Color::White
    } else {
        Color::Black
    }
}

fn strip_ansi16_background(style: &mut CardStyle) {
    style.gradient = GradientSpec {
        left: Color::Reset,
        right: Color::Reset,
        bias: 0.0,
    };
}

fn style_from_theme(definition: CardThemeDefinition, is_dark: bool) -> CardStyle {
    let theme = definition.theme;
    let mut text_primary = theme.palette.text;
    let mut text_secondary = theme.palette.footer;

    let is_rgb = |color: Color, r: u8, g: u8, b: u8| matches!(color, Color::Rgb(rr, gg, bb) if rr == r && gg == g && bb == b);

    if !is_dark && is_rgb(text_primary, 0, 0, 0) {
        let left = theme.gradient.left;
        text_primary = Color::Black;
        text_secondary = colors::mix_toward(Color::Black, left, 0.35);
    } else if is_dark && is_rgb(text_primary, 255, 255, 255) {
        let left = theme.gradient.left;
        text_primary = Color::White;
        text_secondary = colors::mix_toward(Color::White, left, 0.25);
    }

    let adjust = |color: Color, target: Color| match color {
        Color::Rgb(_, _, _) => colors::mix_toward(color, target, 0.15),
        other => other,
    };

    if is_dark {
        text_primary = adjust(text_primary, Color::White);
    } else {
        text_primary = adjust(text_primary, Color::Black);
    }

    let title_text = text_primary;

    CardStyle {
        accent_fg: theme.palette.border,
        text_primary,
        text_secondary,
        title_text,
        gradient: adjust_gradient(theme.gradient, is_dark),
    }
}

fn is_dark_theme_active() -> bool {
    let (r, g, b) = colors::color_to_rgb(colors::background());
    let luminance = (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0;
    luminance < 0.5
}

fn adjust_gradient(gradient: GradientSpec, _is_dark: bool) -> GradientSpec {
    gradient
}

pub(crate) fn fill_card_background(buf: &mut Buffer, area: Rect, style: &CardStyle) {
    if palette_mode() == PaletteMode::Ansi16
        && matches!(style.gradient.left, Color::Reset)
        && matches!(style.gradient.right, Color::Reset)
    {
        return;
    }

    if palette_mode() == PaletteMode::Ansi16
        && style.gradient.left == style.gradient.right
        && !matches!(style.gradient.left, Color::Rgb(_, _, _))
    {
        fill_rect(
            buf,
            area,
            Some(' '),
            Style::default()
                .bg(style.gradient.left)
                .fg(style.text_primary),
        );
    } else {
        GradientBackground::render(buf, area, &style.gradient, style.text_primary, None);
    }
}

pub(crate) fn pad_icon(icon: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let trimmed = truncate_to_width(icon, width);
    let current = UnicodeWidthStr::width(trimmed.as_str());
    if current < width {
        let mut result = trimmed;
        result.push_str(&" ".repeat(width - current));
        return result;
    }
    trimmed
}

pub(crate) fn truncate_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(1);
        if used + w > width {
            break;
        }
        result.push(ch);
        used += w;
    }
    if used < width {
        result.push_str(&" ".repeat(width - used));
    }
    result
}

pub(crate) fn truncate_with_ellipsis(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= width {
        return truncate_to_width(text, width);
    }
    let ellipsis = "...";
    let ellipsis_width = UnicodeWidthStr::width(ellipsis);
    if width <= ellipsis_width {
        return truncate_to_width(text, width);
    }
    let mut result = String::new();
    let mut used = 0;
    let limit = width - ellipsis_width;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(1);
        if used + w > limit {
            break;
        }
        result.push(ch);
        used += w;
    }
    result.push_str(ellipsis);
    let current = UnicodeWidthStr::width(result.as_str());
    if current < width {
        result.push_str(&" ".repeat(width - current));
    }
    result
}

pub(crate) fn rows_to_lines(rows: &[CardRow], _style: &CardStyle, total_width: u16) -> Vec<Line<'static>> {
    if total_width == 0 {
        return Vec::new();
    }
    let has_accent = rows.iter().any(|row| !row.accent.trim().is_empty());
    let accent_width = if has_accent {
        CARD_ACCENT_WIDTH.min(total_width as usize)
    } else {
        0
    };
    let body_width = total_width.saturating_sub(accent_width as u16) as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    for row in rows.iter() {
        let mut spans: Vec<Span<'static>> = Vec::new();
        if accent_width > 0 {
            let accent_text = pad_icon(row.accent.as_str(), accent_width);
            let accent_span = Span::styled(accent_text, row.accent_style);
            spans.push(accent_span);
        }

        let row_bg = row.body_bg;
        let mut used_width = 0;
        for segment in &row.segments {
            let mut seg_style = segment.style;
            if let (true, Some(bg)) = (segment.inherit_background, row_bg) {
                seg_style = seg_style.bg(bg);
            }
            let width = UnicodeWidthStr::width(segment.text.as_str());
            used_width += width;
            spans.push(Span::styled(segment.text.clone(), seg_style));
        }
        if used_width < body_width {
            let filler = " ".repeat(body_width - used_width);
            let filler_style = row_bg
                .map(|bg| Style::default().bg(bg))
                .unwrap_or_else(Style::default);
            spans.push(Span::styled(filler, filler_style));
        }
        lines.push(Line::from(spans));
    }
    lines
}

pub(crate) fn primary_text_style(style: &CardStyle) -> Style {
    Style::default().fg(style.text_primary)
}

pub(crate) fn secondary_text_style(style: &CardStyle) -> Style {
    Style::default().fg(style.text_secondary)
}

pub(crate) fn title_text_style(style: &CardStyle) -> Style {
    Style::default().fg(style.title_text)
}

pub(crate) fn hint_text_style(style: &CardStyle) -> Style {
    Style::default().fg(style.text_secondary)
}
