use crate::sanitize::Mode as SanitizeMode;
use crate::sanitize::Options as SanitizeOptions;
use crate::sanitize::sanitize_for_tui;
use crate::text_formatting::format_json_compact;
use code_ansi_escape::ansi_escape_line;
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;

use super::core::CommandOutput;

// Unified preview format: show first 2 and last 5 non-empty lines with an ellipsis between.
const PREVIEW_HEAD_LINES: usize = 2;
const PREVIEW_TAIL_LINES: usize = 5;
const EXEC_PREVIEW_MAX_CHARS: usize = 16_000;
const STREAMING_EXIT_CODE: i32 = i32::MIN;

pub(crate) fn clean_wait_command(raw: &str) -> String {
    let trimmed = raw.trim();
    let Some((first_token, rest)) = split_token(trimmed) else {
        return trimmed.to_string();
    };
    if !looks_like_shell(first_token) {
        return trimmed.to_string();
    }
    let rest = rest.trim_start();
    let Some((second_token, remainder)) = split_token(rest) else {
        return trimmed.to_string();
    };
    if second_token != "-lc" {
        return trimmed.to_string();
    }
    let mut command = remainder.trim_start();
    if command.len() >= 2 {
        let bytes = command.as_bytes();
        let first_char = bytes[0] as char;
        let last_char = bytes[bytes.len().saturating_sub(1)] as char;
        if (first_char == '"' && last_char == '"') || (first_char == '\'' && last_char == '\'') {
            command = &command[1..command.len().saturating_sub(1)];
        }
    }
    if command.is_empty() {
        trimmed.to_string()
    } else {
        command.to_string()
    }
}

fn split_token(input: &str) -> Option<(&str, &str)> {
    let s = input.trim_start();
    if s.is_empty() {
        return None;
    }
    if let Some(idx) = s.find(char::is_whitespace) {
        let (token, rest) = s.split_at(idx);
        Some((token, rest))
    } else {
        Some((s, ""))
    }
}

fn looks_like_shell(token: &str) -> bool {
    let trimmed = token.trim_matches('"').trim_matches('\'');
    let basename = trimmed
        .rsplit('/')
        .next()
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    matches!(
        basename.as_str(),
        "bash"
            | "bash.exe"
            | "sh"
            | "sh.exe"
            | "zsh"
            | "zsh.exe"
            | "dash"
            | "dash.exe"
            | "ksh"
            | "ksh.exe"
            | "busybox"
    )
}

/// Normalize common TTY overwrite sequences within a text block so that
/// progress lines using carriage returns, backspaces, or ESC[K erase behave as
/// expected when rendered in a pure-buffered UI (no cursor movement).
pub(crate) fn normalize_overwrite_sequences(input: &str) -> String {
    // Process per line, but keep CR/BS/CSI semantics within logical lines.
    // Treat "\n" as committing a line and resetting the cursor.
    let mut out = String::with_capacity(input.len());
    let mut line: Vec<char> = Vec::new(); // visible chars only
    let mut cursor: usize = 0; // column in visible chars

    // Helper to flush current line to out
    let flush_line = |line: &mut Vec<char>, cursor: &mut usize, out: &mut String| {
        if !line.is_empty() {
            out.push_str(&line.iter().collect::<String>());
        }
        out.push('\n');
        line.clear();
        *cursor = 0;
    };

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '\n' => {
                flush_line(&mut line, &mut cursor, &mut out);
                i += 1;
            }
            '\r' => {
                // Carriage return: move cursor to column 0
                cursor = 0;
                i += 1;
            }
            '\u{0008}' => {
                // Backspace: move left one column if possible
                if cursor > 0 {
                    cursor -= 1;
                }
                i += 1;
            }
            '\u{001B}' => {
                // CSI: ESC [ ... <cmd>
                if i + 1 < chars.len() && chars[i + 1] == '[' {
                    // Find final byte (alphabetic)
                    let mut j = i + 2;
                    while j < chars.len() && !chars[j].is_alphabetic() {
                        j += 1;
                    }
                    if j < chars.len() {
                        let cmd = chars[j];
                        // Extract numeric prefix (first parameter only)
                        let num: usize = chars[i + 2..j]
                            .iter()
                            .take_while(|c| c.is_ascii_digit())
                            .collect::<String>()
                            .parse()
                            .unwrap_or(0);

                        match cmd {
                            // Erase in Line: 0/None = cursor..end, 1 = start..cursor, 2 = entire line
                            'K' => {
                                let n = num; // default 0 when absent
                                match n {
                                    0 => {
                                        if cursor < line.len() {
                                            line.truncate(cursor);
                                        }
                                    }
                                    1 => {
                                        // Replace from start to cursor with spaces to keep remaining columns stable
                                        let end = cursor.min(line.len());
                                        for k in 0..end {
                                            line[k] = ' ';
                                        }
                                        // Trim leading spaces if the whole line became spaces
                                        while line.last().map_or(false, |c| *c == ' ') {
                                            line.pop();
                                        }
                                    }
                                    2 => {
                                        line.clear();
                                        cursor = 0;
                                    }
                                    _ => {}
                                }
                                i = j + 1;
                                continue;
                            }
                            // Cursor horizontal absolute (1-based)
                            'G' => {
                                let pos = num.saturating_sub(1);
                                cursor = pos.min(line.len());
                                i = j + 1;
                                continue;
                            }
                            // Cursor forward/backward
                            'C' => {
                                cursor = cursor.saturating_add(num);
                                i = j + 1;
                                continue;
                            }
                            'D' => {
                                cursor = cursor.saturating_sub(num);
                                i = j + 1;
                                continue;
                            }
                            _ => {
                                // Unknown/unsupported CSI (incl. SGR 'm'): keep styling intact by
                                // copying the entire sequence verbatim into the output so ANSI
                                // parsing can apply later, but do not affect cursor position.
                                // First, splice current visible buffer into out to preserve order
                                if !line.is_empty() {
                                    out.push_str(&line.iter().collect::<String>());
                                    line.clear();
                                    cursor = 0;
                                }
                                for k in i..=j {
                                    out.push(chars[k]);
                                }
                                i = j + 1;
                                continue;
                            }
                        }
                    } else {
                        // Malformed CSI: drop it entirely by exiting the loop
                        break;
                    }
                } else {
                    // Other ESC sequences (e.g., OSC): pass through verbatim without affecting cursor
                    // Copy ESC and advance one; do not attempt to parse full OSC payload here.
                    if !line.is_empty() {
                        out.push_str(&line.iter().collect::<String>());
                        line.clear();
                        cursor = 0;
                    }
                    out.push(ch);
                    i += 1;
                }
            }
            _ => {
                // Put visible char at cursor, expanding with spaces if needed
                if cursor < line.len() {
                    line[cursor] = ch;
                } else {
                    while line.len() < cursor {
                        line.push(' ');
                    }
                    line.push(ch);
                }
                cursor += 1;
                i += 1;
            }
        }
    }
    // Flush any remaining visible text
    if !line.is_empty() {
        out.push_str(&line.iter().collect::<String>());
    }
    out
}

pub(crate) fn build_preview_lines(text: &str, _include_left_pipe: bool) -> Vec<Line<'static>> {
    // Prefer UI‑themed JSON highlighting when the (ANSI‑stripped) text parses as JSON.
    let stripped_plain = sanitize_for_tui(
        text,
        SanitizeMode::Plain,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );
    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&stripped_plain) {
        let pretty =
            serde_json::to_string_pretty(&json_val).unwrap_or_else(|_| json_val.to_string());
        let highlighted = crate::syntax_highlight::highlight_code_block(&pretty, Some("json"));
        return select_preview_from_lines(&highlighted, PREVIEW_HEAD_LINES, PREVIEW_TAIL_LINES);
    }

    // Otherwise, compact valid JSON (without ANSI) to improve wrap, or pass original through.
    let processed = format_json_compact(text).unwrap_or_else(|| text.to_string());
    let processed = normalize_overwrite_sequences(&processed);
    let (processed, clipped) = clip_preview_text(&processed, EXEC_PREVIEW_MAX_CHARS);
    let processed = sanitize_for_tui(
        &processed,
        SanitizeMode::AnsiPreserving,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );
    let non_empty: Vec<&str> = processed.lines().filter(|line| !line.is_empty()).collect();

    enum Seg<'a> {
        Line(&'a str),
        Ellipsis,
    }
    let segments: Vec<Seg> = if non_empty.len() <= PREVIEW_HEAD_LINES + PREVIEW_TAIL_LINES {
        non_empty.iter().map(|s| Seg::Line(s)).collect()
    } else {
        let mut v: Vec<Seg> = Vec::with_capacity(PREVIEW_HEAD_LINES + PREVIEW_TAIL_LINES + 1);
        // Head
        for i in 0..PREVIEW_HEAD_LINES {
            v.push(Seg::Line(non_empty[i]));
        }
        v.push(Seg::Ellipsis);
        // Tail
        let start = non_empty.len().saturating_sub(PREVIEW_TAIL_LINES);
        for s in &non_empty[start..] {
            v.push(Seg::Line(s));
        }
        v
    };

    fn ansi_line_with_theme_bg(s: &str) -> Line<'static> {
        let mut ln = ansi_escape_line(s);
        for sp in ln.spans.iter_mut() {
            sp.style.bg = None;
        }
        ln
    }

    let mut out: Vec<Line<'static>> = Vec::new();
    if clipped {
        out.push(Line::styled(
            format!("… output truncated to last {} chars", EXEC_PREVIEW_MAX_CHARS),
            Style::default().fg(crate::colors::text_dim()),
        ));
    }
    for seg in segments {
        match seg {
            Seg::Line(line) => out.push(ansi_line_with_theme_bg(line)),
            Seg::Ellipsis => out.push(Line::from("⋮".dim())),
        }
    }
    out
}

fn clip_preview_text(text: &str, limit: usize) -> (String, bool) {
    let char_count = text.chars().count();
    if char_count <= limit {
        return (text.to_string(), false);
    }
    let tail: String = text
        .chars()
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    (tail, true)
}

pub(crate) fn output_lines(
    output: Option<&CommandOutput>,
    only_err: bool,
    include_angle_pipe: bool,
) -> Vec<Line<'static>> {
    let CommandOutput {
        exit_code,
        stdout,
        stderr,
    } = match output {
        Some(o) => o,
        None => return Vec::new(),
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    let is_streaming_preview = *exit_code == STREAMING_EXIT_CODE;

    if !only_err && !stdout.is_empty() {
        lines.extend(build_preview_lines(stdout, include_angle_pipe));
    }

    if !stderr.is_empty() && (is_streaming_preview || *exit_code != 0) {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        if !is_streaming_preview {
            lines.push(Line::styled(
                format!("Error (exit code {})", exit_code),
                Style::default().fg(crate::colors::error()),
            ));
        }
        let stderr_norm = sanitize_for_tui(
            &normalize_overwrite_sequences(stderr),
            SanitizeMode::AnsiPreserving,
            SanitizeOptions {
                expand_tabs: true,
                tabstop: 4,
                debug_markers: false,
            },
        );
        for line in stderr_norm.lines().filter(|line| !line.is_empty()) {
            lines.push(ansi_escape_line(line).style(Style::default().fg(crate::colors::error())));
        }
    }

    if !lines.is_empty() {
        lines.push(Line::from(""));
    }

    lines
}

pub(crate) fn pretty_provider_name(id: &str) -> String {
    // Special case common providers with human-friendly names
    match id {
        "brave-search" => "brave",
        "screenshot-website-fast" => "screenshot",
        "read-website-fast" => "readweb",
        "sequential-thinking" => "think",
        "discord-bot" => "discord",
        _ => id,
    }
    .to_string()
}

pub(crate) fn lines_to_plain_text(lines: &[Line<'_>]) -> String {
    lines
        .iter()
        .map(line_to_plain_text)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn line_to_plain_text(line: &Line<'_>) -> String {
    line
        .spans
        .iter()
        .map(|sp| sp.content.as_ref())
        .collect::<String>()
}

// Helper: choose first `head` and last `tail` non-empty lines from a styled line list
pub(crate) fn select_preview_from_lines(
    lines: &[Line<'static>],
    head: usize,
    tail: usize,
) -> Vec<Line<'static>> {
    fn is_non_empty(l: &Line<'_>) -> bool {
        let s: String = l.spans.iter().map(|sp| sp.content.as_ref()).collect();
        !s.trim().is_empty()
    }
    let non_empty_idx: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| if is_non_empty(l) { Some(i) } else { None })
        .collect();
    if non_empty_idx.len() <= head + tail {
        return lines.to_vec();
    }
    let mut out: Vec<Line<'static>> = Vec::new();
    for &i in non_empty_idx.iter().take(head) {
        out.push(lines[i].clone());
    }
    out.push(Line::from("⋮".dim()));
    for &i in non_empty_idx
        .iter()
        .rev()
        .take(tail)
        .collect::<Vec<_>>()
        .iter()
        .rev()
    {
        out.push(lines[*i].clone());
    }
    out
}

// Helper: like build_preview_lines but parameterized and preserving ANSI
pub(crate) fn select_preview_from_plain_text(text: &str, head: usize, tail: usize) -> Vec<Line<'static>> {
    let processed = format_json_compact(text).unwrap_or_else(|| text.to_string());
    let processed = normalize_overwrite_sequences(&processed);
    let processed = sanitize_for_tui(
        &processed,
        SanitizeMode::AnsiPreserving,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );
    let non_empty: Vec<&str> = processed.lines().filter(|line| !line.is_empty()).collect();
    fn ansi_line_with_theme_bg(s: &str) -> Line<'static> {
        let mut ln = ansi_escape_line(s);
        for sp in ln.spans.iter_mut() {
            sp.style.bg = None;
        }
        ln
    }
    let mut out: Vec<Line<'static>> = Vec::new();
    if non_empty.len() <= head + tail {
        for s in non_empty {
            out.push(ansi_line_with_theme_bg(s));
        }
        return out;
    }
    for s in non_empty.iter().take(head) {
        out.push(ansi_line_with_theme_bg(s));
    }
    out.push(Line::from("⋮".dim()));
    let start = non_empty.len().saturating_sub(tail);
    for s in &non_empty[start..] {
        out.push(ansi_line_with_theme_bg(s));
    }
    out
}

/// Check if a line appears to be a title/header (like "codex", "user", "thinking", etc.)
fn is_title_line(line: &Line) -> bool {
    // Check if the line has special formatting that indicates it's a title
    if line.spans.is_empty() {
        return false;
    }

    // Get the text content of the line
    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
        .trim()
        .to_lowercase();

    // Check for common title patterns (fallback heuristic only; primary logic uses explicit cell types)
    matches!(
        text.as_str(),
        "codex"
            | "user"
            | "thinking"
            | "event"
            | "tool"
            | "/diff"
            | "/status"
            | "/prompts"
            | "/skills"
            | "reasoning effort"
            | "error"
    ) || text.starts_with("⚡")
        || text.starts_with("⚙")
        || text.starts_with("✓")
        || text.starts_with("✗")
        || text.starts_with("↯")
        || text.starts_with("proposed patch")
        || text.starts_with("applying patch")
        || text.starts_with("updating")
        || text.starts_with("updated")
}

/// Check if a line is empty (no content or just whitespace)
fn is_empty_line(line: &Line) -> bool {
    if line.spans.is_empty() {
        return true;
    }
    // Consider a line empty when all spans have only whitespace
    line.spans
        .iter()
        .all(|s| s.content.as_ref().trim().is_empty())
}

/// Trim empty lines from the beginning and end of a Vec<Line>.
/// Also normalizes internal spacing - no more than 1 empty line between content.
/// This ensures consistent spacing when cells are rendered together.
pub(crate) fn trim_empty_lines(mut lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    // Remove ALL leading empty lines
    while lines.first().map_or(false, is_empty_line) {
        lines.remove(0);
    }

    // Remove ALL trailing empty lines
    while lines.last().map_or(false, is_empty_line) {
        lines.pop();
    }

    // Normalize internal spacing - no more than 1 empty line in a row
    let mut result = Vec::new();
    let mut prev_was_empty = false;

    for line in lines {
        let is_empty = is_empty_line(&line);

        // Skip consecutive empty lines
        if is_empty && prev_was_empty {
            continue;
        }

        // Special case: If this is an empty line right after a title, skip it
        if is_empty && result.len() == 1 && result.first().map_or(false, is_title_line) {
            continue;
        }

        result.push(line);
        prev_was_empty = is_empty;
    }

    result
}
