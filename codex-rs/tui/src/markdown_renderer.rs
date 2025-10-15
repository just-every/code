use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use regex_lite::Regex;

/// Custom markdown renderer with full control over spacing and styling
pub struct MarkdownRenderer {
    lines: Vec<Line<'static>>,
    current_line: Vec<Span<'static>>,
    in_code_block: bool,
    code_block_lang: Option<String>,
    code_block_buf: String,
    #[allow(dead_code)]
    list_depth: usize,
    bold_first_sentence: bool,
    first_sentence_done: bool,
    // When set, inline code spans (created from single-backticks) are tinted
    // 30% toward this target text color. This lets inline code harmonize with
    // the surrounding context (e.g., different list nesting levels) while
    // still reading as code.
    inline_code_tint_target: Option<Color>,
}

impl MarkdownRenderer {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            current_line: Vec::new(),
            in_code_block: false,
            code_block_lang: None,
            code_block_buf: String::new(),
            list_depth: 0,
            bold_first_sentence: false,
            first_sentence_done: false,
            inline_code_tint_target: None,
        }
    }

    pub fn render(text: &str) -> Vec<Line<'static>> {
        let mut renderer = Self::new();
        // Top-level assistant text uses the theme's primary text color as the
        // base for tinting inline code spans.
        renderer.inline_code_tint_target = Some(crate::colors::text());
        renderer.process_text(text);
        renderer.finish();
        renderer.lines
    }

    pub fn render_with_bold_first_sentence(text: &str) -> Vec<Line<'static>> {
        let mut renderer = Self::new();
        renderer.bold_first_sentence = true;
        renderer.inline_code_tint_target = Some(crate::colors::text());
        renderer.process_text(text);
        renderer.finish();
        renderer.lines
    }

    fn process_text(&mut self, text: &str) {
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            // Handle code blocks
            if line.trim_start().starts_with("```") {
                self.handle_code_fence(line);
                i += 1;
                continue;
            }

            // Handle tables EARLY to avoid printing the pipe header as plain text
            if let Some((consumed, table_lines)) = parse_markdown_table(&lines[i..]) {
                self.flush_current_line();
                self.lines.extend(table_lines);
                i += consumed;
                continue;
            }

            if self.in_code_block {
                self.add_code_line(line);
                i += 1;
                continue;
            }

            // Handle headings
            if let Some(heading) = self.parse_heading(line) {
                self.flush_current_line();
                // Do not auto-insert spacing before headings; preserve exactly what the
                // assistant returned. Only explicit blank lines in the source should render.
                self.lines.push(heading);
                i += 1;
                continue;
            }

            // Blockquotes / callouts (supports nesting and [!NOTE]/[!TIP]/[!WARNING]/[!IMPORTANT])
            if let Some((consumed, quote_lines)) = parse_blockquotes(&lines[i..]) {
                self.flush_current_line();
                self.lines.extend(quote_lines);
                i += consumed;
                continue;
            }

            // Handle lists
            if let Some(list_item) = self.parse_list_item(line) {
                self.flush_current_line();
                self.lines.push(list_item);
                i += 1;
                continue;
            }

            // Handle blank lines
            if line.trim().is_empty() {
                self.flush_current_line();
                // Don't add multiple consecutive blank lines
                if !self.is_last_line_blank() {
                    self.lines.push(Line::from(""));
                }
                i += 1;
                continue;
            }

            // Regular text with inline formatting
            self.process_inline_text(line);
            i += 1;
        }
    }

    fn handle_code_fence(&mut self, line: &str) {
        let trimmed = line.trim_start();
        if self.in_code_block {
            // Closing fence
            // Render accumulated buffer with syntax highlighting
            let lang = self.code_block_lang.as_deref();
            let code_bg = crate::colors::code_block_bg();
            let mut highlighted =
                crate::syntax_highlight::highlight_code_block(&self.code_block_buf, lang);
            use ratatui::style::Style;
            use ratatui::text::Span;
            use unicode_width::UnicodeWidthStr;
            let max_w: usize = highlighted
                .iter()
                .map(|l| {
                    l.spans
                        .iter()
                        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                        .sum::<usize>()
                })
                .max()
                .unwrap_or(0);
            let target_w = max_w; // no extra horizontal padding
            // Emit hidden sentinel with language for border/title downstream
            let label = self
                .code_block_lang
                .clone()
                .unwrap_or_else(|| "text".to_string());
            self.lines.push(Line::from(Span::styled(
                format!("⟦LANG:{}⟧", label),
                Style::default().fg(code_bg).bg(code_bg),
            )));

            for l in highlighted.iter_mut() {
                // Paint background on each span, not the whole line, so width
                // matches our explicit padding rectangle.
                for sp in l.spans.iter_mut() {
                    sp.style = sp.style.bg(code_bg);
                }
                let w: usize = l
                    .spans
                    .iter()
                    .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                    .sum();
                if target_w > w {
                    let pad = " ".repeat(target_w - w);
                    l.spans
                        .push(Span::styled(pad, Style::default().bg(code_bg)));
                } else if w == 0 {
                    // Ensure at least one painted cell so the background shows
                    l.spans
                        .push(Span::styled(" ", Style::default().bg(code_bg)));
                }
            }
            self.lines.extend(highlighted);
            self.code_block_buf.clear();
            self.in_code_block = false;
            self.code_block_lang = None;
        } else {
            // Opening fence
            self.flush_current_line();
            self.in_code_block = true;
            // Extract language if present
            let lang = trimmed.trim_start_matches("```").trim();
            self.code_block_lang = if lang.is_empty() {
                None
            } else {
                Some(lang.to_string())
            };
            self.code_block_buf.clear();
        }
    }

    fn add_code_line(&mut self, line: &str) {
        // Accumulate; add a newline that was lost by `lines()` iteration
        self.code_block_buf.push_str(line);
        self.code_block_buf.push('\n');
    }

    fn parse_heading(&self, line: &str) -> Option<Line<'static>> {
        let trimmed = line.trim_start();

        // Count heading level
        let mut level = 0;
        for ch in trimmed.chars() {
            if ch == '#' {
                level += 1;
            } else {
                break;
            }
        }

        if level == 0 || level > 6 {
            return None;
        }

        // Must have space after #
        if !trimmed.chars().nth(level).map_or(false, |c| c == ' ') {
            return None;
        }

        let heading_text = trimmed[level..].trim();

        // Headings: strip the leading #'s and render in bold (no special color).
        let style = Style::default().add_modifier(Modifier::BOLD);
        Some(Line::from(Span::styled(heading_text.to_string(), style)))
    }

    fn parse_list_item(&mut self, line: &str) -> Option<Line<'static>> {
        let trimmed = line.trim_start();

        // Check for unordered list markers
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            let indent = line.len() - trimmed.len();
            let mut content = &trimmed[2..];
            // Collapse any extra spaces after the list marker so we render
            // a single space after the bullet ("-  item" -> "- item").
            content = content.trim_start();

            // Task list checkbox support: - [ ] / - [x]
            let mut checkbox_spans: Vec<Span<'static>> = Vec::new();
            if let Some(rest) = content.strip_prefix("[ ] ") {
                content = rest.trim_start();
                checkbox_spans.push(Span::raw("☐ "));
            } else if let Some(rest) = content
                .strip_prefix("[x] ")
                .or_else(|| content.strip_prefix("[X] "))
            {
                content = rest.trim_start();
                checkbox_spans.push(Span::styled(
                    "✔ ",
                    Style::default().fg(crate::colors::success()),
                ));
            }

            let mut styled_content = self.process_inline_spans(content);
            // Run autolink on list content so links in bullets convert properly.
            styled_content = autolink_spans(styled_content);
            let has_checkbox = !checkbox_spans.is_empty();
            if has_checkbox {
                // Prepend checkbox to content; do NOT render a bullet for task list items.
                let mut tmp = Vec::with_capacity(checkbox_spans.len() + styled_content.len());
                tmp.extend(checkbox_spans);
                tmp.append(&mut styled_content);
                styled_content = tmp;
            }
            // Determine nesting level from indent (2 spaces per level approximation)
            let level = (indent / 2) + 1;
            let bullet = match level {
                1 => "-",
                2 => "·",
                3 => "-",
                _ => "⋅",
            };
            // Color by nesting level:
            // 1 → text, 2 → midpoint between text and text_dim, 3+ → text_dim
            let content_fg = match level {
                1 => crate::colors::text(),
                2 => crate::colors::text_mid(),
                _ => crate::colors::text_dim(),
            };

            let mut spans = vec![Span::raw(" ".repeat(indent))];
            if !has_checkbox {
                // Render bullet to match content color
                spans.push(Span::styled(bullet, Style::default().fg(content_fg)));
                spans.push(Span::raw(" "));
            }
            // Recolor content to desired level color while preserving modifiers and any
            // spans that already carry a specific foreground color (e.g., inline code, checkmarks).
            // For inline code, blend its base color 30% toward the bullet's text color.
            let recolored: Vec<Span<'static>> = styled_content
                .into_iter()
                .map(|s| {
                    if let Some(fg) = s.style.fg {
                        if fg == crate::colors::function() {
                            let mut st = s.style;
                            st.fg = Some(crate::colors::mix_toward(fg, content_fg, 0.30));
                            return Span::styled(s.content, st);
                        }
                        return s;
                    } else {
                        let mut st = s.style;
                        st.fg = Some(content_fg);
                        return Span::styled(s.content, st);
                    }
                })
                .collect();
            spans.extend(recolored);

            return Some(Line::from(spans));
        }

        // Check for ordered list markers (1. 2. etc)
        if let Some(dot_pos) = trimmed.find(". ") {
            let number_part = &trimmed[..dot_pos];
            if number_part.chars().all(|c| c.is_ascii_digit()) && !number_part.is_empty() {
                let indent = line.len() - trimmed.len();
                let content = trimmed[dot_pos + 2..].trim_start();
                let styled_content = self.process_inline_spans(content);
                let depth = indent / 2 + 1;
                let content_fg = match depth {
                    1 => crate::colors::text(),
                    2 => crate::colors::text_mid(),
                    _ => crate::colors::text_dim(),
                };

                let mut spans = vec![
                    Span::raw(" ".repeat(indent)),
                    // Make the number bold (no primary color)
                    Span::styled(
                        format!("{}.", number_part),
                        Style::default().add_modifier(Modifier::BOLD).fg(content_fg),
                    ),
                    Span::raw(" "),
                ];
                // Recolor content preserving modifiers and pre-set foreground colors.
                // For inline code, blend base code color 30% toward the list text color.
                let recolored: Vec<Span<'static>> = styled_content
                    .into_iter()
                    .map(|s| {
                        if let Some(fg) = s.style.fg {
                            if fg == crate::colors::function() {
                                let mut st = s.style;
                                st.fg = Some(crate::colors::mix_toward(fg, content_fg, 0.30));
                                return Span::styled(s.content, st);
                            }
                            return s;
                        } else {
                            let mut st = s.style;
                            st.fg = Some(content_fg);
                            return Span::styled(s.content, st);
                        }
                    })
                    .collect();
                spans.extend(recolored);

                return Some(Line::from(spans));
            }
        }

        None
    }

    fn process_inline_text(&mut self, text: &str) {
        let spans = self.process_inline_spans(text);
        self.current_line.extend(spans);
        self.flush_current_line();
    }

    fn process_inline_spans(&mut self, text: &str) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let mut current_text = String::new();

        while i < chars.len() {
            // Markdown image ![alt](url "title")
            if chars[i] == '!' {
                let rest: String = chars[i..].iter().collect();
                if let Some((consumed, label, target)) = find_markdown_image(&rest) {
                    if !current_text.is_empty() {
                        spans.push(Span::raw(current_text.clone()));
                        current_text.clear();
                    }
                    // Render label and make the target URL visible next to it.
                    let lbl = if label.is_empty() {
                        "Image".to_string()
                    } else {
                        label
                    };
                    // Underlined label
                    let mut st = Style::default();
                    st.add_modifier.insert(Modifier::UNDERLINED);
                    spans.push(Span::styled(lbl, st));
                    // Append visible URL in parens (dimmed)
                    spans.push(Span::raw(" ("));
                    spans.push(Span::styled(
                        target.clone(),
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                    spans.push(Span::raw(")"));
                    i += consumed;
                    continue;
                }
            }
            // Bold+Italic ***text*** or ___text___
            if i + 2 < chars.len()
                && ((chars[i] == '*' && chars[i + 1] == '*' && chars[i + 2] == '*')
                    || (chars[i] == '_' && chars[i + 1] == '_' && chars[i + 2] == '_'))
            {
                // Flush current text
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }
                let marker = chars[i];
                // Find closing triple marker
                let mut j = i + 3;
                let mut content = String::new();
                while j + 2 < chars.len() {
                    if chars[j] == marker && chars[j + 1] == marker && chars[j + 2] == marker {
                        let st = Style::default()
                            .add_modifier(Modifier::BOLD | Modifier::ITALIC)
                            .fg(crate::colors::text_bright());
                        spans.push(Span::styled(content, st));
                        i = j + 3;
                        break;
                    }
                    content.push(chars[j]);
                    j += 1;
                }
                if j + 2 >= chars.len() {
                    // No closing marker; treat as literal
                    current_text.push(marker);
                    current_text.push(marker);
                    current_text.push(marker);
                    i += 3;
                }
                continue;
            }

            // Strikethrough ~~text~~
            if i + 1 < chars.len() && chars[i] == '~' && chars[i + 1] == '~' {
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }
                let mut j = i + 2;
                let mut content = String::new();
                while j + 1 < chars.len() {
                    if chars[j] == '~' && chars[j + 1] == '~' {
                        let st = Style::default().add_modifier(Modifier::CROSSED_OUT);
                        spans.push(Span::styled(content, st));
                        i = j + 2;
                        break;
                    }
                    content.push(chars[j]);
                    j += 1;
                }
                if j + 1 >= chars.len() {
                    current_text.push('~');
                    current_text.push('~');
                    i += 2;
                }
                continue;
            }

            // Simple HTML underline <u>text</u>
            if chars[i] == '<' {
                // Try <u>, <sub>, <sup>
                let rest: String = chars[i..].iter().collect();
                if let Some(inner) = rest.strip_prefix("<u>") {
                    if let Some(end) = inner.find("</u>") {
                        if !current_text.is_empty() {
                            spans.push(Span::raw(current_text.clone()));
                            current_text.clear();
                        }
                        let content = inner[..end].to_string();
                        spans.push(Span::styled(
                            content,
                            Style::default().add_modifier(Modifier::UNDERLINED),
                        ));
                        i += 3 + end + 4; // len("<u>") + content + len("</u>")
                        continue;
                    }
                } else if let Some(inner) = rest.strip_prefix("<sub>") {
                    if let Some(end) = inner.find("</sub>") {
                        if !current_text.is_empty() {
                            spans.push(Span::raw(current_text.clone()));
                            current_text.clear();
                        }
                        let content = inner[..end].to_string();
                        spans.push(Span::raw(to_subscript(&content)));
                        i += 5 + end + 6; // <sub> + content + </sub>
                        continue;
                    }
                } else if let Some(inner) = rest.strip_prefix("<sup>") {
                    if let Some(end) = inner.find("</sup>") {
                        if !current_text.is_empty() {
                            spans.push(Span::raw(current_text.clone()));
                            current_text.clear();
                        }
                        let content = inner[..end].to_string();
                        spans.push(Span::raw(to_superscript(&content)));
                        i += 5 + end + 6; // <sup> + content + </sup>
                        continue;
                    }
                }
            }

            // Check for inline code
            if chars[i] == '`' {
                // Flush current text
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }

                // Find closing backtick
                let mut j = i + 1;
                let mut code_content = String::new();
                while j < chars.len() && chars[j] != '`' {
                    code_content.push(chars[j]);
                    j += 1;
                }

                if j < chars.len() {
                    // Found closing backtick — render code without surrounding backticks
                    // Use the base code color here; context-specific tinting is
                    // applied at line flush or by list/blockquote handlers.
                    spans.push(Span::styled(
                        code_content,
                        Style::default().fg(crate::colors::function()),
                    ));
                    i = j + 1;
                } else {
                    // No closing backtick, treat as regular text
                    current_text.push('`');
                    i += 1;
                }
                continue;
            }

            // Check for bold (**text** or __text__)
            if i + 1 < chars.len()
                && ((chars[i] == '*' && chars[i + 1] == '*')
                    || (chars[i] == '_' && chars[i + 1] == '_'))
            {
                // Flush current text
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }

                let marker = chars[i];
                // Find closing markers
                let mut j = i + 2;
                let mut bold_content = String::new();
                while j + 1 < chars.len() {
                    if chars[j] == marker && chars[j + 1] == marker {
                        // Found closing markers
                        // Bold text uses bright color consistently
                        let bold_color = crate::colors::text_bright();
                        spans.push(Span::styled(
                            bold_content,
                            Style::default().fg(bold_color).add_modifier(Modifier::BOLD),
                        ));
                        i = j + 2;
                        break;
                    }
                    bold_content.push(chars[j]);
                    j += 1;
                }

                if j + 1 >= chars.len() {
                    // No closing markers, treat as regular text
                    current_text.push(marker);
                    current_text.push(marker);
                    i += 2;
                }
                continue;
            }

            // Check for italic (*text* or _text_)
            if (chars[i] == '*' || chars[i] == '_')
                && (i == 0 || !chars[i - 1].is_alphanumeric())
                && (i + 1 < chars.len() && chars[i + 1] != ' ' && chars[i + 1] != chars[i])
            {
                // Flush current text
                if !current_text.is_empty() {
                    spans.push(Span::raw(current_text.clone()));
                    current_text.clear();
                }

                let marker = chars[i];
                // Find closing marker
                let mut j = i + 1;
                let mut italic_content = String::new();
                while j < chars.len() {
                    if chars[j] == marker
                        && (j + 1 >= chars.len() || !chars[j + 1].is_alphanumeric())
                    {
                        // Found closing marker
                        spans.push(Span::styled(
                            italic_content,
                            Style::default().add_modifier(Modifier::ITALIC),
                        ));
                        i = j + 1;
                        break;
                    }
                    italic_content.push(chars[j]);
                    j += 1;
                }

                if j >= chars.len() {
                    // No closing marker, treat as regular text
                    current_text.push(marker);
                    i += 1;
                }
                continue;
            }

            // Regular character
            current_text.push(chars[i]);
            i += 1;
        }

        // Flush any remaining plain text as-is; first-sentence bolding is handled elsewhere
        if !current_text.is_empty() {
            spans.push(Span::raw(current_text));
        }

        spans
    }

    fn flush_current_line(&mut self) {
        if !self.current_line.is_empty() {
            // Autolink URLs and markdown links inside the accumulated spans.
            let mut linked = autolink_spans(std::mem::take(&mut self.current_line));
            // Apply first-sentence styling to the first rendered line.
            if self.bold_first_sentence && !self.first_sentence_done {
                if apply_first_sentence_style(&mut linked) {
                    self.first_sentence_done = true;
                }
            }
            // If requested, gently tint inline code spans toward the provided
            // context text color so they blend better with the surrounding text.
            if let Some(target) = self.inline_code_tint_target {
                let base = crate::colors::function();
                let tint = crate::colors::mix_toward(base, target, 0.30);
                for sp in &mut linked {
                    if sp.style.fg == Some(base) {
                        let mut st = sp.style;
                        st.fg = Some(tint);
                        *sp = Span::styled(sp.content.clone(), st);
                    }
                }
            }
            self.lines.push(Line::from(linked));
            self.current_line.clear();
        }
    }

    fn is_last_line_blank(&self) -> bool {
        if let Some(last) = self.lines.last() {
            last.spans.is_empty() || last.spans.iter().all(|s| s.content.trim().is_empty())
        } else {
            false
        }
    }

    fn finish(&mut self) {
        self.flush_current_line();
        // If an unterminated fence was left open, render its buffer.
        if self.in_code_block {
            let lang = self.code_block_lang.as_deref();
            let code_bg = crate::colors::code_block_bg();
            let mut highlighted =
                crate::syntax_highlight::highlight_code_block(&self.code_block_buf, lang);
            use ratatui::style::Style;
            use ratatui::text::Span;
            use unicode_width::UnicodeWidthStr;
            let max_w: usize = highlighted
                .iter()
                .map(|l| {
                    l.spans
                        .iter()
                        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                        .sum::<usize>()
                })
                .max()
                .unwrap_or(0);
            let target_w = max_w; // no extra horizontal padding
            // Emit hidden sentinel with language for border/title downstream
            let label = self
                .code_block_lang
                .clone()
                .unwrap_or_else(|| "text".to_string());
            self.lines.push(Line::from(Span::styled(
                format!("⟦LANG:{}⟧", label),
                Style::default().fg(code_bg).bg(code_bg),
            )));

            for l in highlighted.iter_mut() {
                for sp in l.spans.iter_mut() {
                    sp.style = sp.style.bg(code_bg);
                }
                let w: usize = l
                    .spans
                    .iter()
                    .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                    .sum();
                if target_w > w {
                    let pad = " ".repeat(target_w - w);
                    l.spans
                        .push(Span::styled(pad, Style::default().bg(code_bg)));
                } else if w == 0 {
                    l.spans
                        .push(Span::styled(" ", Style::default().bg(code_bg)));
                }
            }
            self.lines.extend(highlighted);
            self.code_block_buf.clear();
            self.in_code_block = false;
            self.code_block_lang = None;
        }
    }
}

// Parse a markdown pipe table starting at `lines[0]`.
// Returns (consumed_line_count, rendered_lines) on success.
// We keep it simple and robust for TUI: left-align columns and pad with spaces.
fn parse_markdown_table(lines: &[&str]) -> Option<(usize, Vec<Line<'static>>)> {
    if lines.len() < 2 {
        return None;
    }
    let header_line = lines[0].trim();
    let sep_line = lines[1].trim();
    if !header_line.contains('|') {
        return None;
    }

    // Split a row by '|' and trim spaces; drop empty edge cells from leading/trailing '|'
    fn split_row(s: &str) -> Vec<String> {
        let mut parts: Vec<String> = s.split('|').map(|x| x.trim().to_string()).collect();
        // Trim empty edge cells introduced by leading/trailing '|'
        if parts.first().is_some_and(|x| x.is_empty()) {
            parts.remove(0);
        }
        if parts.last().is_some_and(|x| x.is_empty()) {
            parts.pop();
        }
        parts
    }

    let header_cells = split_row(header_line);
    if header_cells.is_empty() {
        return None;
    }

    // Validate separator: must have at least the same number of segments and each segment is --- with optional : for alignment
    // Parse separator: either pipe-based or dashed segments separated by 2+ spaces
    let (sep_segments, has_pipe_sep) = if sep_line.contains('|') {
        (split_row(sep_line), true)
    } else {
        // Split on runs of 2+ spaces
        let mut segs: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut space_run = 0;
        for ch in sep_line.chars() {
            if ch == ' ' {
                space_run += 1;
            } else {
                space_run = 0;
            }
            if space_run >= 2 {
                if !cur.trim().is_empty() {
                    segs.push(cur.trim().to_string());
                }
                cur.clear();
                space_run = 0;
            } else {
                cur.push(ch);
            }
        }
        if !cur.trim().is_empty() {
            segs.push(cur.trim().to_string());
        }
        (segs, false)
    };
    if sep_segments.len() < header_cells.len() {
        return None;
    }
    let valid_sep = sep_segments.iter().take(header_cells.len()).all(|c| {
        let core = c.replace(':', "");
        !core.is_empty() && core.chars().all(|ch| ch == '-')
    });
    if !valid_sep {
        return None;
    }

    // Collect body rows until a non-table line
    let mut body: Vec<Vec<String>> = Vec::new();
    let mut idx = 2usize;
    while idx < lines.len() {
        let raw = lines[idx];
        if !raw.contains('|') {
            break;
        }
        let row = split_row(raw);
        if row.is_empty() {
            break;
        }
        body.push(row);
        idx += 1;
    }

    let cols = header_cells
        .len()
        .max(body.iter().map(|r| r.len()).max().unwrap_or(0));
    // Column alignment: from pipe separators with colons if present; otherwise
    // infer right alignment for numeric-only columns, left otherwise.
    #[derive(Copy, Clone)]
    enum Align {
        Left,
        Right,
    }
    let mut aligns = vec![Align::Left; cols];
    if has_pipe_sep {
        for i in 0..cols {
            let seg = sep_segments.get(i).map(|s| s.as_str()).unwrap_or("");
            let left_colon = seg.starts_with(':');
            let right_colon = seg.ends_with(':');
            aligns[i] = if right_colon && !left_colon {
                Align::Right
            } else {
                Align::Left
            };
        }
    }
    // Compute widths per column
    let mut widths = vec![0usize; cols];
    for (i, cell) in header_cells.iter().enumerate() {
        widths[i] = widths[i].max(cell.chars().count());
    }
    for row in &body {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    // Infer alignment for numeric columns if not specified by pipes
    if !has_pipe_sep {
        for i in 0..cols {
            let numeric = body
                .iter()
                .all(|r| r.get(i).map(|c| is_numeric(c)).unwrap_or(true));
            if numeric {
                aligns[i] = Align::Right;
            }
        }
    }

    fn pad_cell(s: &str, w: usize, align: Align) -> String {
        let len = s.chars().count();
        if len >= w {
            return s.to_string();
        }
        let pad = w - len;
        match align {
            Align::Left => format!("{}{}", s, " ".repeat(pad)),
            Align::Right => format!("{}{}", " ".repeat(pad), s),
        }
    }

    let mut out: Vec<Line<'static>> = Vec::new();
    // Header (bold)
    {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for i in 0..cols {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            let text = pad_cell(
                header_cells.get(i).map(String::as_str).unwrap_or(""),
                widths[i],
                aligns[i],
            );
            spans.push(Span::styled(
                text,
                Style::default().add_modifier(Modifier::BOLD),
            ));
        }
        out.push(Line::from(spans));
    }
    // Separator row using box-drawing to avoid being mistaken for a horizontal rule
    {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for i in 0..cols {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::raw("─".repeat(widths[i]).to_string()));
        }
        out.push(Line::from(spans));
    }
    // Body
    for row in body {
        let mut spans: Vec<Span<'static>> = Vec::new();
        for i in 0..cols {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            let text = pad_cell(
                row.get(i).map(String::as_str).unwrap_or(""),
                widths[i],
                aligns[i],
            );
            spans.push(Span::raw(text));
        }
        out.push(Line::from(spans));
    }

    Some((idx, out))
}

fn is_numeric(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return true;
    }
    let mut has_digit = false;
    for ch in t.chars() {
        if ch.is_ascii_digit() {
            has_digit = true;
            continue;
        }
        if matches!(ch, '+' | '-' | '.' | ',') {
            continue;
        }
        return false;
    }
    has_digit
}

// Parse consecutive blockquote lines, supporting nesting with multiple '>' markers
// and callouts: [!NOTE], [!TIP], [!WARNING], [!IMPORTANT]
fn parse_blockquotes(lines: &[&str]) -> Option<(usize, Vec<Line<'static>>)> {
    if lines.is_empty() {
        return None;
    }
    // Must start with '>'
    if !lines[0].trim_start().starts_with('>') {
        return None;
    }
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut i = 0usize;
    let mut callout_kind: Option<String> = None;
    let mut callout_color = crate::colors::info();
    let mut first_content_seen = false;
    while i < lines.len() {
        let raw = lines[i];
        let t = raw.trim_start();
        if !t.starts_with('>') {
            break;
        }
        // Count nesting depth (allow spaces between >)
        let mut idx = 0usize;
        let bytes = t.as_bytes();
        let mut depth = 0usize;
        while idx < bytes.len() {
            if bytes[idx] == b'>' {
                depth += 1;
                idx += 1;
                while idx < bytes.len() && bytes[idx] == b' ' {
                    idx += 1;
                }
            } else {
                break;
            }
        }
        let content = t[idx..].to_string();
        if !first_content_seen {
            let trimmed = content.trim();
            if let Some(inner) = trimmed.strip_prefix("[!") {
                if let Some(end) = inner.find(']') {
                    let kind = inner[..end].to_ascii_uppercase();
                    match kind.as_str() {
                        "NOTE" => {
                            callout_kind = Some("NOTE".into());
                            callout_color = crate::colors::info();
                        }
                        "TIP" => {
                            callout_kind = Some("TIP".into());
                            callout_color = crate::colors::success();
                        }
                        "WARNING" => {
                            callout_kind = Some("WARNING".into());
                            callout_color = crate::colors::warning();
                        }
                        "IMPORTANT" => {
                            callout_kind = Some("IMPORTANT".into());
                            callout_color = crate::colors::info();
                        }
                        _ => {}
                    }
                    if let Some(ref k) = callout_kind {
                        // Eagerly emit the label so the block never returns None
                        // even if there are no subsequent quoted lines.
                        if out.is_empty() {
                            let label = format!("{}", k);
                            out.push(Line::from(vec![Span::styled(
                                label,
                                Style::default()
                                    .fg(callout_color)
                                    .add_modifier(Modifier::BOLD),
                            )]));
                        }
                        i += 1; // consume marker line and continue scanning quoted content
                        continue;
                    }
                }
            }
            first_content_seen = true;
        }

        // For callouts, render a label line once
        if let Some(ref kind) = callout_kind {
            if out.is_empty() {
                let label = format!("{}", kind);
                out.push(Line::from(vec![Span::styled(
                    label,
                    Style::default()
                        .fg(callout_color)
                        .add_modifier(Modifier::BOLD),
                )]));
            }
        }

        // Render the quote content as raw literal text without interpreting
        // Markdown syntax inside the blockquote. This preserves the exact
        // characters shown by the model (e.g., `**bold**`, lists, images)
        // rather than re‑parsing them. Each input line corresponds to a
        // single rendered line of content.
        let lines_to_render = if content.is_empty() {
            vec![Line::from("")]
        } else {
            vec![Line::from(Span::raw(content.clone()))]
        };

        let bar_style = if callout_kind.is_some() {
            Style::default().fg(callout_color)
        } else {
            Style::default().fg(crate::colors::text_dim())
        };
        let content_fg = if callout_kind.is_some() {
            crate::colors::text()
        } else {
            crate::colors::text_dim()
        };

        for inner_line in lines_to_render {
            // Prefix depth bars (│ ) once per nesting level
            let mut prefixed: Vec<Span<'static>> = Vec::new();
            for _ in 0..depth.max(1) {
                prefixed.push(Span::styled("│ ", bar_style));
            }
            // Recolor inner content spans only if they don't already have a specific FG
            let recolored: Vec<Span<'static>> = inner_line
                .spans
                .into_iter()
                .map(|s| {
                    if let Some(_fg) = s.style.fg {
                        // Preserve explicit colors (e.g., code spans) even though we
                        // no longer parse markdown inside quotes. If a span already has
                        // an FG, keep it as-is.
                        s
                    } else {
                        let mut st = s.style;
                        st.fg = Some(content_fg);
                        Span::styled(s.content, st)
                    }
                })
                .collect();
            prefixed.extend(recolored);
            out.push(Line::from(prefixed));
        }
        i += 1;
    }
    if out.is_empty() { None } else { Some((i, out)) }
}

// Apply bold + text_bright to the first sentence in a span list (first line only),
// preserving any existing bold spans and other inline styles.
fn apply_first_sentence_style(spans: &mut Vec<Span<'static>>) -> bool {
    use ratatui::style::Modifier;
    // Concatenate text to find terminator
    let full: String = spans.iter().map(|s| s.content.as_ref()).collect();
    let trimmed = full.trim_start();
    // Skip if line begins with a markdown bullet glyph
    if trimmed.starts_with('-')
        || trimmed.starts_with('•')
        || trimmed.starts_with('◦')
        || trimmed.starts_with('·')
        || trimmed.starts_with('∘')
        || trimmed.starts_with('⋅')
    {
        return false;
    }
    // Find a sensible terminator index with simple heuristics
    let chars: Vec<char> = full.chars().collect();
    let mut term: Option<usize> = None;
    for i in 0..chars.len() {
        let ch = chars[i];
        if ch == '.' || ch == '!' || ch == '?' || ch == ':' {
            let next = chars.get(i + 1).copied();
            // Skip filename-like or abbreviation endings
            if matches!(next, Some(c) if c.is_ascii_alphanumeric()) {
                continue;
            }
            if i >= 3 {
                let tail: String = chars[i - 3..=i].iter().collect::<String>().to_lowercase();
                if tail == "e.g." || tail == "i.e." {
                    continue;
                }
            }
            // Accept if eol/space or quote then space/eol
            let ok = match next {
                None => true,
                Some(c) if c.is_whitespace() => true,
                Some('"') | Some('\'') => {
                    let n2 = chars.get(i + 2).copied();
                    n2.is_none() || n2.map(|c| c.is_whitespace()).unwrap_or(false)
                }
                _ => false,
            };
            if ok {
                term = Some(i + 1);
                break;
            }
        }
    }
    let Some(limit) = term else { return false };
    // If no non-space content after limit, consider single-sentence → no bold
    if !chars.iter().skip(limit).any(|c| !c.is_whitespace()) {
        return false;
    }

    // Walk spans and apply style up to limit (build a new vec to avoid borrow conflicts)
    let original = std::mem::take(spans);
    let mut out: Vec<Span<'static>> = Vec::with_capacity(original.len() + 2);
    let mut consumed = 0usize; // chars consumed across spans
    for sp in original.into_iter() {
        if consumed >= limit {
            out.push(sp);
            continue;
        }
        let text = sp.content.into_owned();
        let len = text.chars().count();
        let end_here = (limit - consumed).min(len);
        if end_here == len {
            // Entire span within bold range
            let mut st = sp.style;
            if !st.add_modifier.contains(Modifier::BOLD) {
                st.add_modifier.insert(Modifier::BOLD);
                st.fg = Some(crate::colors::text_bright());
            }
            out.push(Span::styled(text, st));
        } else if end_here == 0 {
            out.push(Span::styled(text, sp.style));
        } else {
            // Split span
            let mut iter = text.chars();
            let left: String = iter.by_ref().take(end_here).collect();
            let right: String = iter.collect();
            let mut left_style = sp.style;
            if !left_style.add_modifier.contains(Modifier::BOLD) {
                left_style.add_modifier.insert(Modifier::BOLD);
                left_style.fg = Some(crate::colors::text_bright());
            }
            out.push(Span::styled(left, left_style));
            out.push(Span::styled(right, sp.style));
        }
        consumed += end_here;
    }
    *spans = out;
    true
}

// Turn inline markdown links and bare URLs into display-friendly spans.
// NOTE: We intentionally avoid emitting OSC 8 hyperlinks here. While OSC 8
// hyperlinks render correctly when static, some terminals exhibit artifacts
// when scrolling or re-wrapping content that contains embedded escape
// sequences. By rendering labels as plain underlined text and leaving literal
// URLs as-is (so the terminal can auto-detect them), we guarantee stable
// rendering during scroll without leaking control characters.
//
// Behavior:
// - Markdown links [label](target): render as an underlined `label` span
//   (no OSC 8). We prefer stability over clickability for labeled links.
// - Explicit http(s) URLs and bare domains: emit verbatim text so terminals
//   can auto-link them. This keeps them clickable without control sequences.
fn autolink_spans(spans: Vec<Span<'static>>) -> Vec<Span<'static>> {
    // Patterns: markdown [label](target), explicit http(s) URLs, and plain domains.
    // Keep conservative to avoid false positives.
    static EXPL_URL_RE: once_cell::sync::OnceCell<Regex> = once_cell::sync::OnceCell::new();
    static DOMAIN_RE: once_cell::sync::OnceCell<Regex> = once_cell::sync::OnceCell::new();
    // We will parse Markdown links manually to support URLs with parentheses.
    let url_re = EXPL_URL_RE.get_or_init(|| Regex::new(r"(?i)\bhttps?://[^\s)]+").unwrap());
    let dom_re = DOMAIN_RE.get_or_init(|| {
        // Conservative bare-domain matcher (no scheme). Examples:
        //   apps.shopify.com
        //   foo.example.io/path?x=1
        // It intentionally over-matches a bit; we further filter below.
        Regex::new(r"\b([a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)+(?:/[\w\-./?%&=#]*)?)")
            .unwrap()
    });

    // Trim common trailing punctuation from a detected URL/domain, returning
    // (core, trailing). The trailing part will be emitted as normal text (not
    // hyperlinked) so that tokens like "example.com." don’t include the period.
    fn split_trailing_punct(s: &str) -> (&str, &str) {
        let bytes = s.as_bytes();
        let mut end = bytes.len();
        while end > 0 {
            let ch = bytes[end - 1] as char;
            if ")]}>'\".,!?;:".contains(ch) {
                // common trailing punctuation
                end -= 1;
                continue;
            }
            break;
        }
        (&s[..end], &s[end..])
    }

    // Additional heuristic to avoid false positives like "e.g." or
    // "filename.rs" by requiring a well-known TLD. Precision over recall.
    fn is_probable_domain(dom: &str) -> bool {
        if dom.contains('@') {
            return false;
        }
        let dot_count = dom.matches('.').count();
        if dot_count == 0 {
            return false;
        }

        // Extract the final label (candidate TLD) and normalize.
        let tld = dom
            .rsplit_once('.')
            .map(|(_, t)| t)
            .unwrap_or("")
            .trim_matches(|c: char| !c.is_ascii_alphanumeric())
            .to_ascii_lowercase();

        // Small allowlist of popular TLDs. This intentionally excludes
        // language/file extensions like `.rs`, `.ts`, `.php`, etc.
        const ALLOWED_TLDS: &[&str] = &[
            "com", "org", "net", "edu", "gov", "io", "ai", "app", "dev", "co", "us", "uk", "ca",
            "de", "fr", "jp", "cn", "in", "au",
        ];

        ALLOWED_TLDS.contains(&tld.as_str())
    }

    let mut out: Vec<Span<'static>> = Vec::with_capacity(spans.len());
    // Slight blue-tinted color for visible URLs/domains. Blend theme text toward primary (blue-ish).
    let link_fg = crate::colors::mix_toward(crate::colors::text(), crate::colors::primary(), 0.35);
    for s in spans {
        // Skip autolinking inside inline code spans (we style code with the
        // theme's function color). This avoids linking snippets like
        // `curl https://example.com` or code identifiers containing dots.
        // Also skip when span already contains OSC8 sequences to avoid corrupting
        // hyperlinks with additional parsing or wrapping.
        if s.style.fg == Some(crate::colors::function()) || s.content.contains('\u{1b}') {
            out.push(s);
            continue;
        }
        let text = s.content.clone();
        let mut cursor = 0usize;
        let mut changed = false;

        // Scan left-to-right, preferring markdown links first (explicit intent),
        // then explicit URLs, then bare domains.
        while cursor < text.len() {
            let after = &text[cursor..];

            // 1) markdown links [label](target) with balanced parentheses in target
            if let Some((start, end, label, target)) = find_markdown_link(after) {
                let abs_start = cursor + start;
                let abs_end = cursor + end;
                if cursor < abs_start {
                    let mut span = s.clone();
                    span.content = text[cursor..abs_start].to_string().into();
                    out.push(span);
                }
                // Special case: when the label is just a short preview of the target URL
                // (e.g., "front.com" for "https://front.com/integrations/…"), emit ONLY
                // the full URL so terminals can auto‑link it. Avoid underlines/parentheses
                // to keep scroll rendering stable and prevent duplicate visual tokens.
                if is_short_preview_of_url(&label, &target) {
                    let mut url_only = s.clone();
                    url_only.content = target.clone().into();
                    url_only.style = url_only.style.patch(Style::default().fg(link_fg));
                    out.push(url_only);
                } else {
                    // Default behavior: underlined label followed by dimmed URL in parens
                    let mut lbl_span = s.clone();
                    let mut st = lbl_span.style;
                    st.add_modifier.insert(Modifier::UNDERLINED);
                    lbl_span.style = st;
                    lbl_span.content = label.into();
                    out.push(lbl_span);
                    let mut open_span = s.clone();
                    open_span.content = " (".into();
                    out.push(open_span);
                    let mut url_span = s.clone();
                    url_span.style = url_span.style.patch(Style::default().fg(link_fg));
                    url_span.content = target.clone().into();
                    out.push(url_span);
                    let mut close_span = s.clone();
                    close_span.content = ")".into();
                    out.push(close_span);
                }
                cursor = abs_end;
                changed = true;
                continue;
            }

            // 2) explicit http(s) URLs (Mixed mode: do NOT wrap; let terminal detect)
            if let Some(m) = url_re.find(after) {
                let start = cursor + m.start();
                let end = cursor + m.end();
                if cursor < start {
                    let mut span = s.clone();
                    span.content = text[cursor..start].to_string().into();
                    out.push(span);
                }
                let raw = &text[start..end];
                let (core, trailing) = split_trailing_punct(raw);
                // Emit URL text verbatim; terminal will make it clickable.
                let mut core_span = s.clone();
                core_span.content = core.to_string().into();
                core_span.style = core_span.style.patch(Style::default().fg(link_fg));
                out.push(core_span);
                if !trailing.is_empty() {
                    let mut span = s.clone();
                    span.content = trailing.to_string().into();
                    out.push(span);
                }
                cursor = start + core.len() + trailing.len();
                changed = true;
                continue;
            }

            // 3) bare domain: emit as text so terminal can auto-link
            if let Some(m) = dom_re.find(after) {
                let start = cursor + m.start();
                let end = cursor + m.end();
                if cursor < start {
                    let mut span = s.clone();
                    span.content = text[cursor..start].to_string().into();
                    out.push(span);
                }
                let raw = &text[start..end];
                let (core_dom, trailing) = split_trailing_punct(raw);
                if is_probable_domain(core_dom) {
                    let mut core_span = s.clone();
                    core_span.content = core_dom.to_string().into();
                    core_span.style = core_span.style.patch(Style::default().fg(link_fg));
                    out.push(core_span);
                    if !trailing.is_empty() {
                        let mut span = s.clone();
                        span.content = trailing.to_string().into();
                        out.push(span);
                    }
                    cursor = start + core_dom.len() + trailing.len();
                    changed = true;
                    continue;
                }
                // Not a probable domain; emit raw text
                let mut span = s.clone();
                span.content = raw.to_string().into();
                out.push(span);
                cursor = end;
                changed = true;
                continue;
            }

            // No more matches
            break;
        }

        if changed {
            if cursor < text.len() {
                let mut span = s.clone();
                span.content = text[cursor..].to_string().into();
                out.push(span);
            }
        } else {
            out.push(s);
        }
    }

    out
}

// Deprecated: OSC 8 emission removed for scroll stability. Keep a small helper
// to style a label consistently if future callers rely on it.
#[allow(dead_code)]
fn hyperlink_span(label: &str, _target: &str, base: &Span<'static>) -> Span<'static> {
    let mut st = base.style;
    st.add_modifier.insert(Modifier::UNDERLINED);
    Span::styled(label.to_string(), st)
}

// Return (start, end, label, target) for the first markdown link found in `s`.
fn find_markdown_link(s: &str) -> Option<(usize, usize, String, String)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            // find closing ']'
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b']' {
                j += 1;
            }
            if j >= bytes.len() {
                return None;
            }
            // next must be '('
            let mut k = j + 1;
            if k >= bytes.len() || bytes[k] != b'(' {
                i += 1;
                continue;
            }
            k += 1; // position after '('
            // parse target allowing balanced parentheses
            let mut depth = 1usize;
            let targ_start = k;
            while k < bytes.len() {
                match bytes[k] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
            if k >= bytes.len() || depth != 0 {
                return None;
            }
            let label = &s[i + 1..j];
            let target = &s[targ_start..k];
            // Full match is from i ..= k
            return Some((i, k + 1, label.to_string(), target.to_string()));
        }
        i += 1;
    }
    None
}

// Return (consumed_chars, label, target) for the first image starting at s (which begins with '!').
fn find_markdown_image(s: &str) -> Option<(usize, String, String)> {
    let bytes = s.as_bytes();
    if !bytes.starts_with(b"![") {
        return None;
    }
    // Parse label
    let mut i = 2; // after ![
    while i < bytes.len() && bytes[i] != b']' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let label = &s[2..i];
    // Next must be '('
    let mut k = i + 1;
    if k >= bytes.len() || bytes[k] != b'(' {
        return None;
    }
    k += 1;
    // Parse target allowing balanced parentheses and optional title
    let mut depth = 1usize;
    let targ_start = k;
    while k < bytes.len() {
        match bytes[k] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        k += 1;
    }
    if k >= bytes.len() || depth != 0 {
        return None;
    }
    let mut target = s[targ_start..k].trim().to_string();
    // Strip optional quoted title at end: url "title"
    if let Some(space_idx) = target.rfind(' ') {
        let (left, right) = target.split_at(space_idx);
        let t = right.trim();
        if (t.starts_with('\"') && t.ends_with('\"')) || (t.starts_with('\'') && t.ends_with('\''))
        {
            target = left.trim().to_string();
        }
    }
    Some((k + 1, label.to_string(), target)) // consumed up to and including ')'
}

// Heuristic: determine if `label` is a short preview (typically a bare domain)
// for the full `target` URL. When true, we should display only the full URL
// (letting the terminal auto-link it) instead of "label (url)".
fn is_short_preview_of_url(label: &str, target: &str) -> bool {
    // Must be an explicit HTTP(S) URL and label must be shorter
    let lt = label.trim();
    let tt = target.trim();
    if lt.is_empty() || tt.is_empty() {
        return false;
    }
    let lower_t = tt.to_ascii_lowercase();
    if !(lower_t.starts_with("http://") || lower_t.starts_with("https://")) {
        return false;
    }
    if lt.chars().count() >= tt.chars().count() {
        return false;
    }

    // Normalize a string into a domain host if possible.
    fn extract_host(s: &str) -> Option<String> {
        let s = s.trim();
        let lower = s.to_ascii_lowercase();
        let without_scheme = if lower.starts_with("http://") {
            &s[7..]
        } else if lower.starts_with("https://") {
            &s[8..]
        } else {
            s
        };
        let mut host = without_scheme
            .split(&['/', '?', '#'][..])
            .next()
            .unwrap_or("")
            .to_string();
        if host.is_empty() {
            return None;
        }
        // Strip userinfo and port if present
        if let Some(idx) = host.rfind('@') {
            host = host[idx + 1..].to_string();
        }
        if let Some(idx) = host.find(':') {
            host = host[..idx].to_string();
        }
        // Drop leading www.
        let host = host.trim().trim_matches('.').to_ascii_lowercase();
        let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
        if host.contains('.') { Some(host) } else { None }
    }

    // Lightweight TLD guard for labels that aren't URLs
    fn looks_like_domain(s: &str) -> bool {
        let s = s.trim().trim_end_matches('/');
        if !s.contains('.') || s.contains(' ') {
            return false;
        }
        let tld = s
            .rsplit_once('.')
            .map(|(_, t)| t)
            .unwrap_or("")
            .to_ascii_lowercase();
        const ALLOWED_TLDS: &[&str] = &[
            "com", "org", "net", "edu", "gov", "io", "ai", "app", "dev", "co", "us", "uk", "ca",
            "de", "fr", "jp", "cn", "in", "au",
        ];
        ALLOWED_TLDS.contains(&tld.as_str())
    }

    let label_host = if lower_t.starts_with("http://") || lower_t.starts_with("https://") {
        // If label itself is a URL, compare its host; otherwise, treat as domain text
        extract_host(lt).or_else(|| Some(lt.to_ascii_lowercase()))
    } else {
        Some(lt.to_ascii_lowercase())
    };
    let target_host = extract_host(tt);

    match (label_host, target_host) {
        (Some(lh_raw), Some(mut th)) => {
            let mut lh = lh_raw.trim().trim_end_matches('/').to_ascii_lowercase();
            if lh.starts_with("www.") {
                lh = lh.trim_start_matches("www.").to_string();
            }
            if th.starts_with("www.") {
                th = th.trim_start_matches("www.").to_string();
            }
            // Require label to look like a domain to avoid stripping arbitrary text
            if !looks_like_domain(&lh) {
                return false;
            }
            // Exact match or label equals the registrable/root portion of the host
            if lh == th {
                return true;
            }
            // Host may include subdomains; if it ends with .<label>, treat as preview
            if th.ends_with(&format!(".{lh}")) {
                return true;
            }
            false
        }
        _ => false,
    }
}

// Map ASCII to Unicode subscript/superscript for a small useful subset.
fn to_subscript(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0' => '₀',
            '1' => '₁',
            '2' => '₂',
            '3' => '₃',
            '4' => '₄',
            '5' => '₅',
            '6' => '₆',
            '7' => '₇',
            '8' => '₈',
            '9' => '₉',
            '+' => '₊',
            '-' => '₋',
            '=' => '₌',
            '(' => '₍',
            ')' => '₎',
            'a' => 'ₐ',
            'e' => 'ₑ',
            'h' => 'ₕ',
            'i' => 'ᵢ',
            'j' => 'ⱼ',
            'k' => 'ₖ',
            'l' => 'ₗ',
            'm' => 'ₘ',
            'n' => 'ₙ',
            'o' => 'ₒ',
            'p' => 'ₚ',
            'r' => 'ᵣ',
            's' => 'ₛ',
            't' => 'ₜ',
            'u' => 'ᵤ',
            'v' => 'ᵥ',
            'x' => 'ₓ',
            _ => c,
        })
        .collect()
}

fn to_superscript(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            '9' => '⁹',
            '+' => '⁺',
            '-' => '⁻',
            '=' => '⁼',
            '(' => '⁽',
            ')' => '⁾',
            'a' => 'ᵃ',
            'b' => 'ᵇ',
            'c' => 'ᶜ',
            'd' => 'ᵈ',
            'e' => 'ᵉ',
            'f' => 'ᶠ',
            'g' => 'ᵍ',
            'h' => 'ʰ',
            'i' => 'ᶦ',
            'j' => 'ʲ',
            'k' => 'ᵏ',
            'l' => 'ˡ',
            'm' => 'ᵐ',
            'n' => 'ⁿ',
            'o' => 'ᵒ',
            'p' => 'ᵖ',
            'r' => 'ʳ',
            's' => 'ˢ',
            't' => 'ᵗ',
            'u' => 'ᵘ',
            'v' => 'ᵛ',
            'w' => 'ʷ',
            'x' => 'ˣ',
            'y' => 'ʸ',
            'z' => 'ᶻ',
            _ => c,
        })
        .collect()
}

#[cfg(all(test, feature = "legacy_tests"))]
mod tests {
    use super::*;
    use ratatui::style::Modifier;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn test_bold_first_sentence() {
        let text =
            "This is the first sentence. Here is the second sentence. And this is the third.";
        let lines = MarkdownRenderer::render_with_bold_first_sentence(text);

        assert!(!lines.is_empty(), "Should have rendered lines");

        // Check first line contains bold text
        let first_line = &lines[0];
        let mut found_bold = false;
        let mut bold_text = String::new();

        for span in &first_line.spans {
            if span.style.add_modifier.contains(Modifier::BOLD) {
                found_bold = true;
                bold_text.push_str(&span.content);
            }
        }

        assert!(found_bold, "Should have found bold text");
        assert_eq!(
            bold_text, "This is the first sentence.",
            "First sentence should be bold"
        );
    }

    #[test]
    fn test_no_bold_without_flag() {
        let text = "This is the first sentence. Here is the second sentence.";
        let lines = MarkdownRenderer::render(text);

        assert!(!lines.is_empty(), "Should have rendered lines");

        // Check no bold text
        let first_line = &lines[0];
        let mut found_bold = false;

        for span in &first_line.spans {
            if span.style.add_modifier.contains(Modifier::BOLD) {
                found_bold = true;
            }
        }

        assert!(
            !found_bold,
            "Should not have bold text when flag is not set"
        );
    }
}

#[cfg(all(test, feature = "legacy_tests"))]
mod fenced_padding_tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn fenced_blocks_have_uniform_padding_in_renderer() {
        let text = "```rust\nfn a() {}\n\nfn bbbbb() { println!(\"hi\"); }\n```";
        let lines = MarkdownRenderer::render(text);
        // Expect at least top pad, 3 content lines, bottom pad
        assert!(lines.len() >= 3);
        let widths: Vec<usize> = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                    .sum()
            })
            .collect();
        let maxw = *widths.iter().max().unwrap_or(&0);
        assert!(
            widths.iter().all(|w| *w == maxw),
            "all lines must match target width: {:?}",
            widths
        );
    }
}
