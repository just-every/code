use ratatui::prelude::*;
use ratatui::style::Modifier;
use ratatui::style::Style;

/// Process text with basic markdown support
#[allow(dead_code)]
pub(crate) fn process_markdown_text(text: &str, _width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Split by newlines but preserve empty lines (text.lines() skips empty lines!)
    for line in text.split('\n') {
        lines.push(parse_markdown_line(line));
    }

    lines
}

/// Process text with markdown support and apply dimming to all text
#[allow(dead_code)]
pub(crate) fn process_dimmed_markdown_text(text: &str, width: u16) -> Vec<Line<'static>> {
    let lines = process_markdown_text(text, width);

    // Remove empty lines that immediately follow bold titles (common in thinking content)
    let mut filtered_lines = Vec::new();
    let mut last_was_bold_title = false;

    for line in &lines {
        let is_empty =
            line.spans.is_empty() || (line.spans.len() == 1 && line.spans[0].content.is_empty());

        // Check if this line has bold content (likely a title)
        let has_bold = line.spans.iter().any(|span| {
            span.style
                .add_modifier
                .contains(ratatui::style::Modifier::BOLD)
        });

        // Skip empty lines that come right after bold titles
        if is_empty && last_was_bold_title {
            last_was_bold_title = false; // Reset flag
            continue; // Skip this empty line
        }

        filtered_lines.push(line.clone());
        last_was_bold_title = has_bold;
    }

    // Apply dimming to all spans in all remaining lines
    filtered_lines
        .into_iter()
        .map(|line| {
            let dimmed_spans: Vec<_> = line
                .spans
                .into_iter()
                .map(|span| {
                    // Apply dim color while preserving other styling (bold, etc.)
                    ratatui::text::Span::styled(
                        span.content,
                        span.style.fg(crate::colors::text_dim()),
                    )
                })
                .collect();

            ratatui::text::Line::from(dimmed_spans)
        })
        .collect()
}

/// Parse a single line with markdown formatting
#[allow(dead_code)]
fn parse_markdown_line(text: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut current_text = String::new();
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '*' && chars.peek() == Some(&'*') {
            // Found **bold** marker
            chars.next(); // consume second *

            // Save any accumulated text
            if !current_text.is_empty() {
                spans.push(Span::styled(
                    current_text.clone(),
                    Style::default().fg(crate::colors::text()),
                ));
                current_text.clear();
            }

            // Find the closing **
            let mut bold_text = String::new();
            let mut found_closing = false;

            while let Some(ch) = chars.next() {
                if ch == '*' && chars.peek() == Some(&'*') {
                    chars.next(); // consume second *
                    found_closing = true;
                    break;
                }
                bold_text.push(ch);
            }

            if found_closing && !bold_text.is_empty() {
                spans.push(Span::styled(
                    bold_text,
                    Style::default()
                        .fg(crate::colors::text_bright())
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                // No closing **, treat as regular text
                current_text.push_str("**");
                current_text.push_str(&bold_text);
            }
        } else {
            current_text.push(ch);
        }
    }

    // Handle remaining text
    if !current_text.is_empty() {
        spans.push(Span::styled(
            current_text,
            Style::default().fg(crate::colors::text()),
        ));
    }

    Line::from(spans)
}

#[cfg(all(test, feature = "legacy_tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_bold_parsing() {
        let line = parse_markdown_line("Hello **world** and **bold text** here");
        // Check that we have multiple spans with different styles
        assert!(line.spans.len() > 1);
    }

    #[test]
    fn test_preserve_lines() {
        let lines = process_markdown_text("Line one\nLine two\nLine three", 80);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_markdown_text() {
        let lines = process_markdown_text("Normal **bold** text", 80);
        assert_eq!(lines.len(), 1);
        // The first line should have spans for normal, bold, and text
        assert!(lines[0].spans.len() >= 3);
    }
}
