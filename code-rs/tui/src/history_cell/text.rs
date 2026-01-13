use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::history::state::{InlineSpan, MessageLine, MessageLineKind, TextEmphasis, TextTone};
use crate::theme::Theme;

pub(crate) fn message_lines_from_ratatui(lines: Vec<Line<'static>>) -> Vec<MessageLine> {
    let theme = crate::theme::current_theme();
    lines
        .into_iter()
        .map(|line| message_line_from_ratatui_line(line, &theme))
        .collect()
}

pub(crate) fn message_lines_to_ratatui(lines: &[MessageLine], theme: &Theme) -> Vec<Line<'static>> {
    lines.iter().map(|line| message_line_to_line(line, theme)).collect()
}

pub(crate) fn inline_spans_from_ratatui(line: &Line<'static>, theme: &Theme) -> Vec<InlineSpan> {
    if line.spans.is_empty() {
        return vec![InlineSpan {
            text: String::new(),
            tone: TextTone::Default,
            emphasis: TextEmphasis::default(),
            entity: None,
        }];
    }

    line.spans
        .iter()
        .map(|span| inline_span_from_ratatui_span(span, theme))
        .collect()
}

pub(crate) fn inline_span_to_span(span: &InlineSpan, theme: &Theme) -> Span<'static> {
    let mut style = Style::default().fg(color_for_tone(span.tone, theme));
    if span.emphasis.bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if span.emphasis.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if span.emphasis.dim {
        style = style.add_modifier(Modifier::DIM);
    }
    if span.emphasis.strike {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    if span.emphasis.underline {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    Span::styled(span.text.clone(), style)
}

fn inline_span_from_ratatui_span(span: &Span<'_>, theme: &Theme) -> InlineSpan {
    InlineSpan {
        text: span.content.to_string(),
        tone: text_tone_from_style(span.style, theme),
        emphasis: text_emphasis_from_style(span.style),
        entity: None,
    }
}

pub(crate) fn color_for_tone(tone: TextTone, theme: &Theme) -> Color {
    match tone {
        TextTone::Default => theme.text,
        TextTone::Dim => theme.text_dim,
        TextTone::Primary => theme.primary,
        TextTone::Success => theme.success,
        TextTone::Warning => theme.warning,
        TextTone::Error => theme.error,
        TextTone::Info => theme.info,
        TextTone::Accent(index) => accent_color(index, theme),
    }
}

fn accent_color(index: u8, theme: &Theme) -> Color {
    let palette = [
        theme.primary,
        theme.secondary,
        theme.info,
        theme.success,
        theme.warning,
        theme.keyword,
        theme.string,
        theme.function,
    ];
    let idx = (index as usize) % palette.len();
    palette[idx]
}

fn message_line_from_ratatui_line(line: Line<'static>, theme: &Theme) -> MessageLine {
    let spans = inline_spans_from_ratatui(&line, theme);
    let is_blank = spans
        .iter()
        .all(|span| span.text.trim().is_empty());
    MessageLine {
        kind: if is_blank {
            MessageLineKind::Blank
        } else {
            MessageLineKind::Paragraph
        },
        spans,
    }
}

fn message_line_to_line(line: &MessageLine, theme: &Theme) -> Line<'static> {
    match line.kind {
        MessageLineKind::Blank => Line::from(String::new()),
        _ => {
            let spans: Vec<Span<'static>> = line
                .spans
                .iter()
                .map(|span| inline_span_to_span(span, theme))
                .collect();
            Line::from(spans)
        }
    }
}

fn text_tone_from_style(style: Style, theme: &Theme) -> TextTone {
    if let Some(fg) = style.fg {
        if fg == theme.text_dim {
            return TextTone::Dim;
        }
        if fg == theme.primary || fg == theme.text_bright {
            return TextTone::Primary;
        }
        if fg == theme.success {
            return TextTone::Success;
        }
        if fg == theme.error {
            return TextTone::Error;
        }
        if fg == theme.info {
            return TextTone::Info;
        }
        if fg == theme.warning {
            return TextTone::Warning;
        }
    }
    TextTone::Default
}

fn text_emphasis_from_style(style: Style) -> TextEmphasis {
    let modifiers = style.add_modifier;
    TextEmphasis {
        bold: modifiers.contains(Modifier::BOLD),
        italic: modifiers.contains(Modifier::ITALIC),
        dim: modifiers.contains(Modifier::DIM),
        strike: modifiers.contains(Modifier::CROSSED_OUT),
        underline: modifiers.contains(Modifier::UNDERLINED),
    }
}
