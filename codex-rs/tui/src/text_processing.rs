use ratatui::prelude::*;
use ratatui::style::{Modifier, Style};

/// Process text with basic markdown support
pub(crate) fn process_markdown_text(text: &str, _width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    
    // Split by existing newlines to preserve intentional line breaks
    for line in text.lines() {
        lines.push(parse_markdown_line(line));
    }
    
    lines
}

/// Parse a single line with markdown formatting
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
                    Style::default().fg(crate::colors::text())
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
            Style::default().fg(crate::colors::text())
        ));
    }
    
    Line::from(spans)
}


#[cfg(test)]
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