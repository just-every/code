use crate::theme::{current_theme, Theme};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Tone {
    Default,
    Dim,
    Primary,
    Success,
    Error,
    Info,
    Warning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub(crate) struct Emphasis {
    pub bold: bool,
    pub italic: bool,
    pub dim: bool,
    pub underline: bool,
    pub strike: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SemanticSpan {
    pub text: String,
    pub tone: Tone,
    pub emphasis: Emphasis,
}

impl SemanticSpan {
    fn from_span(span: &Span<'_>, theme: &Theme) -> Self {
        let tone = tone_from_style(span.style, theme);
        let modifiers = span.style.add_modifier;
        let emphasis = Emphasis {
            bold: modifiers.contains(Modifier::BOLD),
            italic: modifiers.contains(Modifier::ITALIC),
            dim: modifiers.contains(Modifier::DIM),
            underline: modifiers.contains(Modifier::UNDERLINED),
            strike: modifiers.contains(Modifier::CROSSED_OUT),
        };
        Self {
            text: span.content.to_string(),
            tone,
            emphasis,
        }
    }

    fn to_span(&self, theme: &Theme) -> Span<'static> {
        let mut style = Style::default().fg(self.tone.to_color(theme));
        if self.emphasis.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.emphasis.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if self.emphasis.dim {
            style = style.add_modifier(Modifier::DIM);
        }
        if self.emphasis.underline {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        if self.emphasis.strike {
            style = style.add_modifier(Modifier::CROSSED_OUT);
        }
        Span::styled(self.text.clone(), style)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SemanticLine {
    pub spans: Vec<SemanticSpan>,
    pub tone: Tone,
}

impl SemanticLine {
    pub(crate) fn from_line(line: Line<'static>) -> Self {
        let theme = current_theme();
        let tone = tone_from_style(line.style, &theme);
        let spans = if line.spans.is_empty() {
            vec![SemanticSpan {
                text: String::new(),
                tone: Tone::Default,
                emphasis: Emphasis::default(),
            }]
        } else {
            line.spans
                .iter()
                .map(|span| SemanticSpan::from_span(span, &theme))
                .collect()
        };
        Self { spans, tone }
    }

    pub(crate) fn to_line(&self, theme: &Theme) -> Line<'static> {
        let style = Style::default().fg(self.tone.to_color(theme));
        let spans: Vec<Span<'static>> = self
            .spans
            .iter()
            .map(|span| span.to_span(theme))
            .collect();
        Line::from(spans).style(style)
    }
}

impl Tone {
    fn to_color(self, theme: &Theme) -> Color {
        match self {
            Tone::Default => theme.text,
            Tone::Dim => theme.text_dim,
            Tone::Primary => theme.primary,
            Tone::Success => theme.success,
            Tone::Error => theme.error,
            Tone::Info => theme.info,
            Tone::Warning => theme.warning,
        }
    }
}

fn tone_from_style(style: Style, theme: &Theme) -> Tone {
    if let Some(fg) = style.fg {
        if fg == theme.text_dim {
            return Tone::Dim;
        }
        if fg == theme.primary {
            return Tone::Primary;
        }
        if fg == theme.success {
            return Tone::Success;
        }
        if fg == theme.error {
            return Tone::Error;
        }
        if fg == theme.info {
            return Tone::Info;
        }
        if fg == theme.warning {
            return Tone::Warning;
        }
        if fg == theme.text_bright {
            return Tone::Primary;
        }
    }
    Tone::Default
}

pub(crate) fn lines_from_ratatui(source: Vec<Line<'static>>) -> Vec<SemanticLine> {
    source.into_iter().map(SemanticLine::from_line).collect()
}

pub(crate) fn lines_to_ratatui(lines: &[SemanticLine], theme: &Theme) -> Vec<Line<'static>> {
    lines.iter().map(|line| line.to_line(theme)).collect()
}
