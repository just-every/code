use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::history::state::{InlineSpan, MessageLine, MessageLineKind, TextTone};
use crate::theme::Theme;

use super::semantic::{SemanticLine, SemanticSpan, Tone, lines_from_ratatui};

pub(crate) fn message_lines_from_ratatui(lines: Vec<Line<'static>>) -> Vec<MessageLine> {
    let semantic_lines = lines_from_ratatui(lines);
    semantic_lines
        .into_iter()
        .map(message_line_from_semantic)
        .collect()
}

pub(crate) fn message_lines_to_ratatui(lines: &[MessageLine], theme: &Theme) -> Vec<Line<'static>> {
    lines
        .iter()
        .map(|line| message_line_to_line(line, theme))
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

pub(crate) fn inline_span_from_semantic(span: SemanticSpan) -> InlineSpan {
    InlineSpan {
        text: span.text,
        tone: map_tone(span.tone),
        emphasis: map_emphasis(span.emphasis),
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
    }
}

fn message_line_from_semantic(line: SemanticLine) -> MessageLine {
    let spans: Vec<InlineSpan> = line
        .spans
        .into_iter()
        .map(inline_span_from_semantic)
        .collect();
    let is_blank = spans.iter().all(|span| span.text.trim().is_empty());
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

fn map_tone(tone: Tone) -> TextTone {
    match tone {
        Tone::Default => TextTone::Default,
        Tone::Dim => TextTone::Dim,
        Tone::Primary => TextTone::Primary,
        Tone::Success => TextTone::Success,
        Tone::Error => TextTone::Error,
        Tone::Info => TextTone::Info,
        Tone::Warning => TextTone::Warning,
    }
}

fn map_emphasis(emphasis: super::semantic::Emphasis) -> crate::history::state::TextEmphasis {
    crate::history::state::TextEmphasis {
        bold: emphasis.bold,
        italic: emphasis.italic,
        dim: emphasis.dim,
        strike: emphasis.strike,
        underline: emphasis.underline,
    }
}
