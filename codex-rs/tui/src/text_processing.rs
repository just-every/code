use ratatui::prelude::*;
use ratatui::style::{Modifier, Style};

/// Process text with basic markdown support and proper word wrapping
pub(crate) fn process_markdown_text(text: &str, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    
    for paragraph in text.split("\n\n") {
        if paragraph.trim().is_empty() {
            lines.push(Line::from(""));
            continue;
        }
        
        // Handle each paragraph with word wrapping
        let paragraph_lines = wrap_markdown_paragraph(paragraph.trim(), width);
        lines.extend(paragraph_lines);
        lines.push(Line::from(""));
    }
    
    // Remove trailing empty line if present
    if lines.last().map_or(false, |l| l.spans.is_empty()) {
        lines.pop();
    }
    
    lines
}

/// Parse a paragraph with basic markdown and wrap it properly
fn wrap_markdown_paragraph(paragraph: &str, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut current_line = Vec::new();
    let mut current_length = 0u16;
    let max_width = width.saturating_sub(4); // Account for padding
    
    // Split paragraph into tokens (words and markdown elements)
    let tokens = parse_markdown_tokens(paragraph);
    
    for token in tokens {
        let token_width = token.display_width();
        
        // Check if we need to wrap to next line
        if current_length > 0 && current_length + token_width + 1 > max_width {
            // Finish current line
            if !current_line.is_empty() {
                lines.push(Line::from(current_line));
                current_line = Vec::new();
                current_length = 0;
            }
        }
        
        // Add space if not at start of line
        if current_length > 0 && !token.content().is_empty() {
            current_line.push(Span::raw(" "));
            current_length += 1;
        }
        
        // Add the token
        current_line.push(token.to_span());
        current_length += token_width;
    }
    
    // Add the last line if not empty
    if !current_line.is_empty() {
        lines.push(Line::from(current_line));
    }
    
    lines
}

#[derive(Debug, Clone)]
enum MarkdownToken {
    Text(String),
    Bold(String),
}

impl MarkdownToken {
    fn display_width(&self) -> u16 {
        match self {
            MarkdownToken::Text(s) | MarkdownToken::Bold(s) => s.chars().count() as u16,
        }
    }
    
    fn content(&self) -> &str {
        match self {
            MarkdownToken::Text(s) | MarkdownToken::Bold(s) => s,
        }
    }
    
    fn to_span(&self) -> Span<'static> {
        match self {
            MarkdownToken::Text(s) => Span::styled(
                s.clone(), 
                Style::default().fg(crate::colors::text())
            ),
            MarkdownToken::Bold(s) => Span::styled(
                s.clone(),
                Style::default()
                    .fg(crate::colors::text_bright())
                    .add_modifier(Modifier::BOLD),
            ),
        }
    }
}

/// Parse a paragraph into markdown tokens
fn parse_markdown_tokens(text: &str) -> Vec<MarkdownToken> {
    let mut tokens = Vec::new();
    let mut current_text = String::new();
    let mut chars = text.chars().peekable();
    
    while let Some(ch) = chars.next() {
        if ch == '*' && chars.peek() == Some(&'*') {
            // Found **bold** marker
            chars.next(); // consume second *
            
            // Save any accumulated text
            if !current_text.is_empty() {
                // Split by whitespace to handle word boundaries properly
                for word in current_text.split_whitespace() {
                    tokens.push(MarkdownToken::Text(word.to_string()));
                }
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
                // Split bold text by whitespace too
                for word in bold_text.split_whitespace() {
                    tokens.push(MarkdownToken::Bold(word.to_string()));
                }
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
        for word in current_text.split_whitespace() {
            tokens.push(MarkdownToken::Text(word.to_string()));
        }
    }
    
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bold_parsing() {
        let tokens = parse_markdown_tokens("Hello **world** and **bold text** here");
        assert_eq!(tokens.len(), 6); // "Hello", "world", "and", "bold", "text", "here"
        
        match &tokens[1] {
            MarkdownToken::Bold(text) => assert_eq!(text, "world"),
            _ => panic!("Expected bold token"),
        }
    }
    
    #[test]
    fn test_word_wrapping() {
        let lines = process_markdown_text("This is a very long line that should be wrapped properly when it exceeds the maximum width", 20);
        assert!(lines.len() > 1);
    }
    
    #[test]
    fn test_paragraph_separation() {
        let lines = process_markdown_text("First paragraph.\n\nSecond paragraph.", 80);
        // Should have: first para line, empty line, second para line
        assert!(lines.len() >= 3);
    }
}