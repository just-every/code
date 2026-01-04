use std::path::Path;
use std::time::{Duration, Instant};

use code_common::elapsed::format_duration;
use code_core::parse_command::ParsedCommand;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use shlex::Shlex;

use crate::exec_command::strip_bash_lc_and_escape;
use crate::history::compat::ExecAction;

use super::core::CommandOutput;
use super::exec::ParsedExecMetadata;
use super::formatting::output_lines;

pub(crate) fn action_enum_from_parsed(
    parsed: &[code_core::parse_command::ParsedCommand],
) -> ExecAction {
    use code_core::parse_command::ParsedCommand;
    for p in parsed {
        match p {
            ParsedCommand::Read { .. } => return ExecAction::Read,
            ParsedCommand::Search { .. } => return ExecAction::Search,
            ParsedCommand::ListFiles { .. } => return ExecAction::List,
            _ => {}
        }
    }
    ExecAction::Run
}

pub(crate) fn exec_command_lines(
    command: &[String],
    parsed: &[ParsedCommand],
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    start_time: Option<Instant>,
) -> Vec<Line<'static>> {
    match parsed.is_empty() {
        true => new_exec_command_generic(command, output, stream_preview, start_time),
        false => new_parsed_command(parsed, output, stream_preview, start_time),
    }
}

pub(crate) fn first_context_path(parsed_commands: &[ParsedCommand]) -> Option<String> {
    for parsed in parsed_commands.iter() {
        match parsed {
            ParsedCommand::ListFiles { path, .. } => {
                if let Some(p) = path {
                    return Some(p.clone());
                }
            }
            ParsedCommand::Search { path, .. } => {
                if let Some(p) = path {
                    return Some(p.clone());
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn exec_render_parts_parsed_with_meta(
    parsed_commands: &[ParsedCommand],
    meta: &ParsedExecMetadata,
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    elapsed_since_start: Option<Duration>,
    status_label: &str,
) -> (
    Vec<Line<'static>>,
    Vec<Line<'static>>,
    Option<Line<'static>>,
) {
    let action = meta.action;
    let ctx_path = meta.ctx_path.as_deref();
    let suppress_run_header = matches!(action, ExecAction::Run) && output.is_some();
    let mut pre: Vec<Line<'static>> = Vec::new();
    let mut running_status: Option<Line<'static>> = None;
    if !suppress_run_header {
        match output {
            None => match action {
                ExecAction::Read => pre.push(Line::styled(
                    "Read",
                    Style::default().fg(crate::colors::text()),
                )),
                ExecAction::Search => pre.push(Line::styled(
                    "Search",
                    Style::default().fg(crate::colors::text_dim()),
                )),
                ExecAction::List => pre.push(Line::styled(
                    "List",
                    Style::default().fg(crate::colors::text()),
                )),
                ExecAction::Run => {
                    let mut message = match &ctx_path {
                        Some(p) => format!("{}... in {p}", status_label),
                        None => format!("{}...", status_label),
                    };
                    if let Some(elapsed) = elapsed_since_start {
                        message = format!("{message} ({})", format_duration(elapsed));
                    }
                    running_status = Some(running_status_line(message));
                }
            },
            Some(o) if o.exit_code == 0 => {
                let done = match action {
                    ExecAction::Read => "Read".to_string(),
                    ExecAction::Search => "Search".to_string(),
                    ExecAction::List => "List".to_string(),
                    ExecAction::Run => match &ctx_path {
                        Some(p) => format!("Ran in {}", p),
                        None => "Ran".to_string(),
                    },
                };
                if matches!(
                    action,
                    ExecAction::Read | ExecAction::Search | ExecAction::List
                ) {
                    pre.push(Line::styled(
                        done,
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                } else {
                    pre.push(Line::styled(
                        done,
                        Style::default()
                            .fg(crate::colors::text_bright())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
            Some(_) => {
                let done = match action {
                    ExecAction::Read => "Read".to_string(),
                    ExecAction::Search => "Search".to_string(),
                    ExecAction::List => "List".to_string(),
                    ExecAction::Run => match &ctx_path {
                        Some(p) => format!("Ran in {}", p),
                        None => "Ran".to_string(),
                    },
                };
                if matches!(
                    action,
                    ExecAction::Read | ExecAction::Search | ExecAction::List
                ) {
                    pre.push(Line::styled(
                        done,
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                } else {
                    pre.push(Line::styled(
                        done,
                        Style::default()
                            .fg(crate::colors::text_bright())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
        }
    }

    // Reuse the same parsed-content rendering as new_parsed_command
    let search_paths = &meta.search_paths;
    // Compute output preview first to know whether to draw the downward corner.
    let show_stdout = matches!(action, ExecAction::Run);
    let display_output = output.or(stream_preview);
    let mut out = output_lines(display_output, !show_stdout, false);
    let mut any_content_emitted = false;
    // Determine allowed label(s) for this cell's primary action
    let expected_label: Option<&'static str> = match action {
        ExecAction::Read => Some("Read"),
        ExecAction::Search => Some("Search"),
        ExecAction::List => Some("List"),
        ExecAction::Run => None, // run: allow a set of labels
    };
    let use_content_connectors = !(matches!(action, ExecAction::Run) && output.is_none());

    for parsed in parsed_commands.iter() {
        let (label, content) = match parsed {
            ParsedCommand::Read { name, cmd, .. } => {
                let mut c = name.clone();
                if let Some(ann) = parse_read_line_annotation(cmd) {
                    c = format!("{} {}", c, ann);
                }
                ("Read".to_string(), c)
            }
            ParsedCommand::ListFiles { cmd: _, path } => match path {
                Some(p) => {
                    if search_paths.contains(p) {
                        (String::new(), String::new())
                    } else {
                        let display_p = if p.ends_with('/') {
                            p.to_string()
                        } else {
                            format!("{}/", p)
                        };
                        ("List".to_string(), format!("{}", display_p))
                    }
                }
                None => ("List".to_string(), "./".to_string()),
            },
            ParsedCommand::Search { query, path, cmd } => {
                // Make search terms human-readable:
                // - Unescape any backslash-escaped character (e.g., "\?" -> "?")
                // - Close unbalanced pairs for '(' and '{' to avoid dangling text in UI
                let prettify_term = |s: &str| -> String {
                    // General unescape: remove backslashes that escape the next char
                    let mut out = String::with_capacity(s.len());
                    let mut iter = s.chars();
                    while let Some(ch) = iter.next() {
                        if ch == '\\' {
                            if let Some(next) = iter.next() {
                                out.push(next);
                            } else {
                                out.push('\\');
                            }
                        } else {
                            out.push(ch);
                        }
                    }

                    // Balance parentheses
                    let opens_paren = out.matches("(").count();
                    let closes_paren = out.matches(")").count();
                    for _ in 0..opens_paren.saturating_sub(closes_paren) {
                        out.push(')');
                    }

                    // Balance curly braces
                    let opens_curly = out.matches("{").count();
                    let closes_curly = out.matches("}").count();
                    for _ in 0..opens_curly.saturating_sub(closes_curly) {
                        out.push('}');
                    }

                    out
                };
                let fmt_query = |q: &str| -> String {
                    let mut parts: Vec<String> = q
                        .split('|')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(prettify_term)
                        .collect();
                    match parts.len() {
                        0 => String::new(),
                        1 => parts.remove(0),
                        2 => format!("{} and {}", parts[0], parts[1]),
                        _ => {
                            let last = parts.last().cloned().unwrap_or_default();
                            let head = &parts[..parts.len() - 1];
                            format!("{} and {}", head.join(", "), last)
                        }
                    }
                };
                match (query, path) {
                    (Some(q), Some(p)) => {
                        let display_p = if p.ends_with('/') {
                            p.to_string()
                        } else {
                            format!("{}/", p)
                        };
                        (
                            "Search".to_string(),
                            format!("{} in {}", fmt_query(q), display_p),
                        )
                    }
                    (Some(q), None) => ("Search".to_string(), format!("{}", fmt_query(q))),
                    (None, Some(p)) => {
                        let display_p = if p.ends_with('/') {
                            p.to_string()
                        } else {
                            format!("{}/", p)
                        };
                        ("Search".to_string(), format!(" in {}", display_p))
                    }
                    (None, None) => ("Search".to_string(), cmd.clone()),
                }
            }
            ParsedCommand::ReadCommand { cmd } => ("Run".to_string(), cmd.clone()),
            // Upstream variants not present in our core parser are ignored or treated as generic runs
            ParsedCommand::Unknown { cmd } => {
                // Suppress separator helpers like `echo ---` which are used
                // internally to delimit chunks when reading files.
                let t = cmd.trim();
                let lower = t.to_lowercase();
                if lower.starts_with("echo") && lower.contains("---") {
                    (String::new(), String::new()) // drop from preamble
                } else {
                    ("Run".to_string(), format_inline_script_for_display(cmd))
                }
            } // Noop variant not present in our core parser
              // ParsedCommand::Noop { .. } => continue,
        };
        // Enforce per-action grouping: only keep entries matching this cell's action.
        if let Some(exp) = expected_label {
            if label != exp {
                continue;
            }
        } else if !(label == "Run" || label == "Search") {
            // For generic "run" header, keep common run-like labels only.
            continue;
        }
        if label.is_empty() && content.is_empty() {
            continue;
        }
        for line_text in content.lines() {
            if line_text.is_empty() {
                continue;
            }
            let prefix = if !any_content_emitted {
                if suppress_run_header || !use_content_connectors {
                    ""
                } else {
                    "└ "
                }
            } else if suppress_run_header || !use_content_connectors {
                ""
            } else {
                "  "
            };
            let mut spans: Vec<Span<'static>> = Vec::new();
            if !prefix.is_empty() {
                spans.push(Span::styled(
                    prefix,
                    Style::default().add_modifier(Modifier::DIM),
                ));
            }
            match label.as_str() {
                "Search" => {
                    let remaining = line_text.to_string();
                    let (terms_part, path_part) = if let Some(idx) = remaining.rfind(" (in ") {
                        (
                            remaining[..idx].to_string(),
                            Some(remaining[idx..].to_string()),
                        )
                    } else if let Some(idx) = remaining.rfind(" in ") {
                        let suffix = &remaining[idx + 1..];
                        if suffix.trim_end().ends_with('/') {
                            (
                                remaining[..idx].to_string(),
                                Some(remaining[idx..].to_string()),
                            )
                        } else {
                            (remaining.clone(), None)
                        }
                    } else {
                        (remaining.clone(), None)
                    };
                    let tmp = terms_part.clone();
                    let chunks: Vec<String> = if tmp.contains(", ") {
                        tmp.split(", ").map(|s| s.to_string()).collect()
                    } else {
                        vec![tmp.clone()]
                    };
                    for (i, chunk) in chunks.iter().enumerate() {
                        if i > 0 {
                            spans.push(Span::styled(
                                ", ",
                                Style::default().fg(crate::colors::text_dim()),
                            ));
                        }
                        if let Some((left, right)) = chunk.rsplit_once(" and ") {
                            if !left.is_empty() {
                                spans.push(Span::styled(
                                    left.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                                spans.push(Span::styled(
                                    " and ",
                                    Style::default().fg(crate::colors::text_dim()),
                                ));
                                spans.push(Span::styled(
                                    right.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                            } else {
                                spans.push(Span::styled(
                                    chunk.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                            }
                        } else {
                            spans.push(Span::styled(
                                chunk.to_string(),
                                Style::default().fg(crate::colors::text()),
                            ));
                        }
                    }
                    if let Some(p) = path_part {
                        spans.push(Span::styled(
                            p,
                            Style::default().fg(crate::colors::text_dim()),
                        ));
                    }
                }
                "Read" => {
                    if let Some(idx) = line_text.find(" (") {
                        let (fname, rest) = line_text.split_at(idx);
                        spans.push(Span::styled(
                            fname.to_string(),
                            Style::default().fg(crate::colors::text()),
                        ));
                        spans.push(Span::styled(
                            rest.to_string(),
                            Style::default().fg(crate::colors::text_dim()),
                        ));
                    } else {
                        spans.push(Span::styled(
                            line_text.to_string(),
                            Style::default().fg(crate::colors::text()),
                        ));
                    }
                }
                "List" => {
                    spans.push(Span::styled(
                        line_text.to_string(),
                        Style::default().fg(crate::colors::text()),
                    ));
                }
                _ => {
                    // Apply shell syntax highlighting to executed command lines.
                    // We highlight the single logical line as bash and append its spans inline.
                    let normalized = normalize_shell_command_display(line_text);
                    let display_line = insert_line_breaks_after_double_ampersand(&normalized);
                    let mut hl =
                        crate::syntax_highlight::highlight_code_block(&display_line, Some("bash"));
                    if let Some(mut first_line) = hl.pop() {
                        emphasize_shell_command_name(&mut first_line);
                        spans.extend(first_line.spans.into_iter());
                    } else {
                        spans.push(Span::styled(
                            display_line,
                            Style::default().fg(crate::colors::text()),
                        ));
                    }
                }
            }
            pre.push(Line::from(spans));
            any_content_emitted = true;
        }
    }

    // If this is a List cell and nothing emitted (e.g., suppressed due to matching Search path),
    // still show a single contextual line so users can see where we listed.
    if matches!(action, ExecAction::List) && !any_content_emitted {
        let display_p = match &ctx_path {
            Some(p) if !p.is_empty() => {
                if p.ends_with('/') {
                    p.to_string()
                } else {
                    format!("{p}/")
                }
            }
            _ => "./".to_string(),
        };
        pre.push(Line::from(vec![
            Span::styled("└ ", Style::default().add_modifier(Modifier::DIM)),
            Span::styled(
                format!("{display_p}"),
                Style::default().fg(crate::colors::text()),
            ),
        ]));
    }

    // Collapse adjacent Read ranges for the same file inside a single exec's preamble
    coalesce_read_ranges_in_lines_local(&mut pre);

    // Output: show stdout only for real run commands; errors always included
    // Collapse adjacent Read ranges for the same file inside a single exec's preamble
    coalesce_read_ranges_in_lines_local(&mut pre);

    if running_status.is_some() {
        if let Some(last) = out.last() {
            let is_blank = last
                .spans
                .iter()
                .all(|sp| sp.content.as_ref().trim().is_empty());
            if is_blank {
                out.pop();
            }
        }
    }

    (pre, out, running_status)
}

pub(crate) fn exec_render_parts_parsed(
    parsed_commands: &[ParsedCommand],
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    elapsed_since_start: Option<Duration>,
    status_label: &str,
) -> (
    Vec<Line<'static>>,
    Vec<Line<'static>>,
    Option<Line<'static>>,
) {
    let meta = ParsedExecMetadata::from_commands(parsed_commands);
    exec_render_parts_parsed_with_meta(
        parsed_commands,
        &meta,
        output,
        stream_preview,
        elapsed_since_start,
        status_label,
    )
}

// Local helper: coalesce "<file> (lines A to B)" entries when contiguous.
pub(crate) fn coalesce_read_ranges_in_lines_local(lines: &mut Vec<Line<'static>>) {
    use ratatui::style::Modifier;
    use ratatui::style::Style;
    use ratatui::text::Span;
    // Nothing to do for empty/single line vectors
    if lines.len() <= 1 {
        return;
    }

    // Parse a content line of the form
    //   "└ <file> (lines A to B)" or "  <file> (lines A to B)"
    // into (filename, start, end, prefix, original_index).
    fn parse_read_line_with_index(
        idx: usize,
        line: &Line<'_>,
    ) -> Option<(String, u32, u32, String, usize)> {
        if line.spans.is_empty() {
            return None;
        }
        let prefix = line.spans[0].content.to_string();
        if !(prefix == "└ " || prefix == "  ") {
            return None;
        }
        let rest: String = line
            .spans
            .iter()
            .skip(1)
            .map(|s| s.content.as_ref())
            .collect();
        if let Some(i) = rest.rfind(" (lines ") {
            let fname = rest[..i].to_string();
            let tail = &rest[i + 1..];
            if tail.starts_with("(lines ") && tail.ends_with(")") {
                let inner = &tail[7..tail.len() - 1];
                if let Some((s1, s2)) = inner.split_once(" to ") {
                    if let (Ok(a), Ok(b)) = (s1.trim().parse::<u32>(), s2.trim().parse::<u32>()) {
                        return Some((fname, a, b, prefix, idx));
                    }
                }
            }
        }
        None
    }

    // Collect read ranges grouped by filename, preserving first-seen order.
    // Also track the earliest prefix to reuse when emitting a single line per file.
    #[derive(Default)]
    struct FileRanges {
        prefix: String,
        first_index: usize,
        ranges: Vec<(u32, u32)>,
    }

    let mut files: Vec<(String, FileRanges)> = Vec::new();
    let mut non_read_lines: Vec<Line<'static>> = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        if let Some((fname, a, b, prefix, orig_idx)) = parse_read_line_with_index(idx, line) {
            // Insert or update entry for this file, preserving encounter order
            if let Some((_name, fr)) = files.iter_mut().find(|(n, _)| n == &fname) {
                fr.ranges.push((a.min(b), a.max(b)));
                // Keep earliest index as stable ordering anchor
                if orig_idx < fr.first_index {
                    fr.first_index = orig_idx;
                }
            } else {
                files.push((
                    fname,
                    FileRanges {
                        prefix,
                        first_index: orig_idx,
                        ranges: vec![(a.min(b), a.max(b))],
                    },
                ));
            }
        } else {
            non_read_lines.push(line.clone());
        }
    }

    if files.is_empty() {
        return;
    }

    // For each file: merge overlapping/touching ranges; then sort ascending and emit one line.
    fn merge_and_sort(mut v: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
        if v.len() <= 1 {
            return v;
        }
        v.sort_by_key(|(s, _)| *s);
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(v.len());
        let mut cur = v[0];
        for &(s, e) in v.iter().skip(1) {
            if s <= cur.1.saturating_add(1) {
                // touching or overlap
                cur.1 = cur.1.max(e);
            } else {
                out.push(cur);
                cur = (s, e);
            }
        }
        out.push(cur);
        out
    }

    // Rebuild the lines vector: keep header (if present) and any non-read lines,
    // then append one consolidated line per file in first-seen order by index.
    let mut rebuilt: Vec<Line<'static>> = Vec::with_capacity(lines.len());

    // Heuristic: preserve an initial header line that does not start with a connector.
    if !lines.is_empty() {
        if lines[0]
            .spans
            .first()
            .map(|s| s.content.as_ref() != "└ " && s.content.as_ref() != "  ")
            .unwrap_or(false)
        {
            rebuilt.push(lines[0].clone());
        }
    }

    // Sort files by their first appearance index to keep stable ordering with other files.
    files.sort_by_key(|(_n, fr)| fr.first_index);

    for (name, mut fr) in files.into_iter() {
        fr.ranges = merge_and_sort(fr.ranges);
        // Build range annotation: " (lines S1 to E1, S2 to E2, ...)"
        let mut ann = String::new();
        ann.push_str(" (");
        ann.push_str("lines ");
        for (i, (s, e)) in fr.ranges.iter().enumerate() {
            if i > 0 {
                ann.push_str(", ");
            }
            ann.push_str(&format!("{} to {}", s, e));
        }
        ann.push(')');

        let spans: Vec<Span<'static>> = vec![
            Span::styled(fr.prefix, Style::default().add_modifier(Modifier::DIM)),
            Span::styled(name, Style::default().fg(crate::colors::text())),
            Span::styled(ann, Style::default().fg(crate::colors::text_dim())),
        ];
        rebuilt.push(Line::from(spans));
    }

    // Append any other non-read lines (rare for Read sections, but safe)
    // Note: keep their original order after consolidated entries
    rebuilt.extend(non_read_lines.into_iter());

    *lines = rebuilt;
}

pub(crate) fn parse_read_line_annotation_with_range(cmd: &str) -> (Option<String>, Option<(u32, u32)>) {
    let lower = cmd.to_lowercase();
    // Try sed -n '<start>,<end>p'
    if lower.contains("sed") && lower.contains("-n") {
        // Look for a token like 123,456p possibly quoted
        for raw in cmd.split(|c: char| c.is_whitespace() || c == '"' || c == '\'') {
            let token = raw.trim();
            if token.ends_with('p') {
                let core = &token[..token.len().saturating_sub(1)];
                if let Some((a, b)) = core.split_once(',') {
                    if let (Ok(start), Ok(end)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>())
                    {
                        return (
                            Some(format!("(lines {} to {})", start, end)),
                            Some((start, end)),
                        );
                    }
                }
            }
        }
    }
    // head -n N => lines 1..N
    if lower.contains("head") && lower.contains("-n") {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        // Find the position of "head" command first
        let head_pos = parts.iter().position(|p| {
            let lower = p.to_lowercase();
            lower == "head" || lower.ends_with("/head")
        });

        if let Some(head_idx) = head_pos {
            // Only look for -n after the head command position
            for i in head_idx..parts.len() {
                if parts[i] == "-n" && i + 1 < parts.len() {
                    if let Ok(n) = parts[i + 1]
                        .trim_matches('"')
                        .trim_matches('\'')
                        .parse::<u32>()
                    {
                        return (Some(format!("(lines 1 to {})", n)), Some((1, n)));
                    }
                }
            }
        }
    }
    // bare `head` => default 10 lines
    if lower.contains("head") && !lower.contains("-n") {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.iter().any(|p| *p == "head") {
            return (Some("(lines 1 to 10)".to_string()), Some((1, 10)));
        }
    }
    // tail -n +K => from K to end; tail -n N => last N lines
    if lower.contains("tail") && lower.contains("-n") {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        // Find the position of "tail" command first
        let tail_pos = parts.iter().position(|p| {
            let lower = p.to_lowercase();
            lower == "tail" || lower.ends_with("/tail")
        });

        if let Some(tail_idx) = tail_pos {
            // Only look for -n after the tail command position
            for i in tail_idx..parts.len() {
                if parts[i] == "-n" && i + 1 < parts.len() {
                    let val = parts[i + 1].trim_matches('"').trim_matches('\'');
                    if let Some(rest) = val.strip_prefix('+') {
                        if let Ok(k) = rest.parse::<u32>() {
                            return (Some(format!("(from {} to end)", k)), Some((k, u32::MAX)));
                        }
                    } else if let Ok(n) = val.parse::<u32>() {
                        return (Some(format!("(last {} lines)", n)), None);
                    }
                }
            }
        }
    }
    // bare `tail` => default 10 lines
    if lower.contains("tail") && !lower.contains("-n") {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.iter().any(|p| *p == "tail") {
            return (Some("(last 10 lines)".to_string()), None);
        }
    }
    (None, None)
}

pub(crate) fn parse_read_line_annotation(cmd: &str) -> Option<String> {
    parse_read_line_annotation_with_range(cmd).0
}

pub(crate) fn normalize_shell_command_display(cmd: &str) -> String {
    let first_non_ws = cmd
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(idx, _)| idx);
    let Some(start) = first_non_ws else {
        return cmd.to_string();
    };
    if cmd[start..].starts_with("./") {
        let mut normalized = String::with_capacity(cmd.len().saturating_sub(2));
        normalized.push_str(&cmd[..start]);
        normalized.push_str(&cmd[start + 2..]);
        normalized
    } else {
        cmd.to_string()
    }
}

pub(crate) fn insert_line_breaks_after_double_ampersand(cmd: &str) -> String {
    if !cmd.contains("&&") {
        return cmd.to_string();
    }

    let mut result = String::with_capacity(cmd.len() + 8);
    let mut i = 0;
    let mut in_single = false;
    let mut in_double = false;

    while i < cmd.len() {
        let ch = cmd[i..].chars().next().expect("valid char boundary");
        let ch_len = ch.len_utf8();

        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                result.push(ch);
                i += ch_len;
                continue;
            }
            '"' if !in_single => {
                in_double = !in_double;
                result.push(ch);
                i += ch_len;
                continue;
            }
            '&' if !in_single && !in_double => {
                let next_idx = i + ch_len;
                if next_idx < cmd.len() {
                    if let Some(next_ch) = cmd[next_idx..].chars().next() {
                        if next_ch == '&' {
                            result.push('&');
                            result.push('&');
                            i = next_idx + next_ch.len_utf8();
                            while i < cmd.len() {
                                let ahead = cmd[i..].chars().next().expect("valid char boundary");
                                if ahead.is_whitespace() {
                                    i += ahead.len_utf8();
                                    continue;
                                }
                                break;
                            }
                            if i < cmd.len() {
                                result.push('\n');
                            }
                            continue;
                        }
                    }
                }
            }
            _ => {}
        }

        result.push(ch);
        i += ch_len;
    }

    result
}

pub(crate) fn emphasize_shell_command_name(line: &mut Line<'static>) {
    let mut emphasized = false;
    let mut rebuilt: Vec<Span<'static>> = Vec::with_capacity(line.spans.len());

    for span in line.spans.drain(..) {
        if emphasized {
            rebuilt.push(span);
            continue;
        }

        let style = span.style;
        let content_owned = span.content.into_owned();

        if content_owned.trim().is_empty() {
            rebuilt.push(Span::styled(content_owned, style));
            continue;
        }

        let mut token_start: Option<usize> = None;
        for (idx, ch) in content_owned.char_indices() {
            if !ch.is_whitespace() {
                token_start = Some(idx);
                break;
            }
        }

        let Some(start) = token_start else {
            rebuilt.push(Span::styled(content_owned, style));
            continue;
        };

        let mut end = content_owned.len();
        for (offset, ch) in content_owned[start..].char_indices() {
            if ch.is_whitespace() {
                end = start + offset;
                break;
            }
        }

        let before = &content_owned[..start];
        let token = &content_owned[start..end];
        let after = &content_owned[end..];

        if !before.is_empty() {
            rebuilt.push(Span::styled(before.to_string(), style));
        }

        if token.chars().count() <= 4 {
            rebuilt.push(Span::styled(token.to_string(), style));
        } else {
            let bright_style = style
                .fg(crate::colors::text_bright())
                .add_modifier(Modifier::BOLD);
            rebuilt.push(Span::styled(token.to_string(), bright_style));
        }

        if !after.is_empty() {
            rebuilt.push(Span::styled(after.to_string(), style));
        }

        emphasized = true;
    }

    if emphasized {
        line.spans = rebuilt;
    } else if !rebuilt.is_empty() {
        line.spans = rebuilt;
    }
}

pub(crate) fn format_inline_script_for_display(command_escaped: &str) -> String {
    if let Some(formatted) = try_format_inline_python(command_escaped) {
        return formatted;
    }
    if let Some(formatted) = format_inline_node_for_display(command_escaped) {
        return formatted;
    }
    if let Some(formatted) = format_inline_shell_for_display(command_escaped) {
        return formatted;
    }
    command_escaped.to_string()
}

fn try_format_inline_python(command_escaped: &str) -> Option<String> {
    if let Some(formatted) = format_python_dash_c(command_escaped) {
        return Some(formatted);
    }
    if let Some(formatted) = format_python_heredoc(command_escaped) {
        return Some(formatted);
    }
    None
}

fn format_python_dash_c(command_escaped: &str) -> Option<String> {
    let tokens: Vec<String> = Shlex::new(command_escaped).collect();
    if tokens.len() < 3 {
        return None;
    }

    let python_idx = tokens
        .iter()
        .position(|token| is_python_invocation_token(token))?;

    let c_idx = tokens
        .iter()
        .enumerate()
        .skip(python_idx + 1)
        .find_map(|(idx, token)| if token == "-c" { Some(idx) } else { None })?;

    let script_idx = c_idx + 1;
    if script_idx >= tokens.len() {
        return None;
    }

    let script_raw = tokens[script_idx].as_str();
    if script_raw.is_empty() {
        return None;
    }

    let script_block = build_python_script_block(script_raw)?;

    let mut parts: Vec<String> = Vec::with_capacity(tokens.len());
    for (idx, token) in tokens.iter().enumerate() {
        if idx == script_idx {
            parts.push(script_block.clone());
        } else {
            parts.push(escape_token_for_display(token));
        }
    }

    Some(parts.join(" "))
}

fn build_python_script_block(script: &str) -> Option<String> {
    let normalized = script.replace("\r\n", "\n");
    let lines: Vec<String> = if normalized.contains('\n') {
        normalized
            .lines()
            .map(|line| line.trim_end().to_string())
            .collect()
    } else if script_has_semicolon_outside_quotes(&normalized) {
        split_semicolon_statements(&normalized)
    } else {
        return None;
    };

    let meaningful: Vec<String> = merge_from_import_lines(lines)
        .into_iter()
        .map(|line| line.trim_end().to_string())
        .filter(|line| !line.trim().is_empty())
        .collect();

    if meaningful.len() <= 1 {
        return None;
    }

    let indented = indent_python_lines(meaningful);

    let mut block = String::from("'\n");
    for line in indented {
        block.push_str("    ");
        block.push_str(line.as_str());
        block.push('\n');
    }
    block.push('\'');
    Some(block)
}

fn format_python_heredoc(command_escaped: &str) -> Option<String> {
    let tokens: Vec<String> = Shlex::new(command_escaped).collect();
    if tokens.len() < 3 {
        return None;
    }

    let python_idx = tokens
        .iter()
        .position(|token| is_python_invocation_token(token))?;

    let heredoc_idx = tokens
        .iter()
        .enumerate()
        .skip(python_idx + 1)
        .find_map(|(idx, token)| heredoc_delimiter(token).map(|delim| (idx, delim)))?;

    let (marker_idx, terminator) = heredoc_idx;
    let closing_idx = tokens
        .iter()
        .enumerate()
        .skip(marker_idx + 1)
        .rev()
        .find_map(|(idx, token)| (token == &terminator).then_some(idx))?;

    if closing_idx <= marker_idx + 1 {
        return None;
    }

    let script_tokens = &tokens[marker_idx + 1..closing_idx];
    if script_tokens.is_empty() {
        return None;
    }

    let script_lines = split_heredoc_script_lines(script_tokens);
    if script_lines.is_empty() {
        return None;
    }

    let script_lines = indent_python_lines(merge_from_import_lines(script_lines));

    let header_tokens: Vec<String> = tokens[..=marker_idx]
        .iter()
        .map(|t| escape_token_for_display(t))
        .collect();

    let mut result = header_tokens.join(" ");
    if !result.ends_with('\n') {
        result.push('\n');
    }

    for line in script_lines {
        result.push_str("    ");
        result.push_str(line.trim_end());
        result.push('\n');
    }

    result.push_str(&escape_token_for_display(&tokens[closing_idx]));

    if closing_idx + 1 < tokens.len() {
        let tail: Vec<String> = tokens[closing_idx + 1..]
            .iter()
            .map(|t| escape_token_for_display(t))
            .collect();
        if !tail.is_empty() {
            result.push(' ');
            result.push_str(&tail.join(" "));
        }
    }

    Some(result)
}

fn heredoc_delimiter(token: &str) -> Option<String> {
    if !token.starts_with("<<") {
        return None;
    }
    let mut delim = token.trim_start_matches("<<").to_string();
    if delim.is_empty() {
        return None;
    }
    if delim.starts_with('"') && delim.ends_with('"') && delim.len() >= 2 {
        delim = delim[1..delim.len() - 1].to_string();
    } else if delim.starts_with('\'') && delim.ends_with('\'') && delim.len() >= 2 {
        delim = delim[1..delim.len() - 1].to_string();
    }
    if delim.is_empty() {
        None
    } else {
        Some(delim)
    }
}

fn split_heredoc_script_lines(script_tokens: &[String]) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut current_has_assignment = false;

    for (idx, token) in script_tokens.iter().enumerate() {
        if !current.is_empty()
            && paren_depth == 0
            && bracket_depth == 0
            && brace_depth == 0
        {
            let token_lower = token.to_ascii_lowercase();
            let current_first = current.first().map(|s| s.to_ascii_lowercase());
            let should_flush_before = is_statement_boundary_token(token)
                && !(token_lower == "import"
                    && current_first.as_deref() == Some("from"));
            if should_flush_before {
                let line = current.join(" ");
                lines.push(line.trim().to_string());
                current.clear();
                current_has_assignment = false;
            }
        }

        current.push(token.clone());
        adjust_bracket_depth(token, &mut paren_depth, &mut bracket_depth, &mut brace_depth);

        if is_assignment_operator(token) {
            current_has_assignment = true;
        }

        let next = script_tokens.get(idx + 1);
        let mut should_break = false;
        let mut break_here = false;

        if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
            if next.is_none() {
                should_break = true;
            } else {
                let next_token = next.unwrap();
                if is_statement_boundary_token(next_token) {
                    should_break = true;
                } else if current
                    .first()
                    .map(|s| s.as_str() == "import" || s.as_str() == "from")
                    .unwrap_or(false)
                {
                    if current.len() > 1 && next_token != "as" && next_token != "," {
                        should_break = true;
                    }
                } else if current_has_assignment
                    && !is_assignment_operator(token)
                    && next_token
                        .chars()
                        .next()
                        .map(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                        .unwrap_or(false)
                    && !next_token.contains('(')
                {
                    should_break = true;
                }

                let token_trimmed = token.trim_matches(|c| c == ')' || c == ']' || c == '}');
                if token_trimmed.ends_with(':') {
                    break_here = true;
                }

                let lowered = token.trim().to_ascii_lowercase();
                if matches!(lowered.as_str(), "return" | "break" | "continue" | "pass") {
                    break_here = true;
                }

                if let Some(next_token) = next {
                    let next_str = next_token.as_str();
                    if token.ends_with(')')
                        && (next_str.contains('.')
                            || next_str.contains('=')
                            || next_str.starts_with("print"))
                    {
                        break_here = true;
                    }
                }
            }
        }

        if break_here {
            let line = current.join(" ");
            lines.push(line.trim().to_string());
            current.clear();
            current_has_assignment = false;
            continue;
        }

        if should_break {
            let line = current.join(" ");
            lines.push(line.trim().to_string());
            current.clear();
            current_has_assignment = false;
        }
    }

    if !current.is_empty() {
        let line = current.join(" ");
        lines.push(line.trim().to_string());
    }

    lines.into_iter().filter(|line| !line.is_empty()).collect()
}

fn is_statement_boundary_token(token: &str) -> bool {
    matches!(
        token,
        "import"
            | "from"
            | "def"
            | "class"
            | "if"
            | "elif"
            | "else"
            | "for"
            | "while"
            | "try"
            | "except"
            | "with"
            | "return"
            | "raise"
            | "pass"
            | "continue"
            | "break"
    ) || token.starts_with("print")
}

fn indent_python_lines(lines: Vec<String>) -> Vec<String> {
    let mut indented: Vec<String> = Vec::with_capacity(lines.len());
    let mut indent_level: usize = 0;
    let mut pending_dedent_after_flow = false;

    for raw in lines {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            indented.push(String::new());
            continue;
        }

        let lowered_first = trimmed
            .split_whitespace()
            .next()
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();

        if pending_dedent_after_flow
            && !matches!(
                lowered_first.as_str(),
                "elif" | "else" | "except" | "finally"
            )
        {
            if indent_level > 0 {
                indent_level -= 1;
            }
        }
        pending_dedent_after_flow = false;

        if matches!(
            lowered_first.as_str(),
            "elif" | "else" | "except" | "finally"
        ) {
            if indent_level > 0 {
                indent_level -= 1;
            }
        }

        let mut line = String::with_capacity(trimmed.len() + indent_level * 4);
        for _ in 0..indent_level {
            line.push_str("    ");
        }
        line.push_str(trimmed);
        indented.push(line);

        if trimmed.ends_with(':')
            && !matches!(
                lowered_first.as_str(),
                "return" | "break" | "continue" | "pass" | "raise"
            )
        {
            indent_level += 1;
        } else if matches!(
            lowered_first.as_str(),
            "return" | "break" | "continue" | "pass" | "raise"
        ) {
            pending_dedent_after_flow = true;
        }
    }

    indented
}

fn merge_from_import_lines(lines: Vec<String>) -> Vec<String> {
    let mut merged: Vec<String> = Vec::with_capacity(lines.len());
    let mut idx = 0;
    while idx < lines.len() {
        let line = lines[idx].trim().to_string();
        if line.starts_with("from ")
            && idx + 1 < lines.len()
            && lines[idx + 1].trim_start().starts_with("import ")
        {
            let combined = format!(
                "{} {}",
                line.trim_end(),
                lines[idx + 1].trim_start()
            );
            merged.push(combined);
            idx += 2;
        } else {
            merged.push(line);
            idx += 1;
        }
    }
    merged
}

fn is_assignment_operator(token: &str) -> bool {
    matches!(
        token,
        "="
            | "+="
            | "-="
            | "*="
            | "/="
            | "//="
            | "%="
            | "^="
            | "|="
            | "&="
            | "**="
            | "<<="
            | ">>="
    )
}

fn is_shell_executable(token: &str) -> bool {
    let trimmed = token.trim_matches(|c| c == '\'' || c == '"');
    let lowered = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    matches!(
        lowered.as_str(),
        "bash"
            | "bash.exe"
            | "sh"
            | "sh.exe"
            | "dash"
            | "dash.exe"
            | "zsh"
            | "zsh.exe"
            | "ksh"
            | "ksh.exe"
            | "busybox"
    )
}

fn is_node_invocation_token(token: &str) -> bool {
    let trimmed = token.trim_matches(|c| c == '\'' || c == '"');
    let base = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    matches!(base.as_str(), "node" | "node.exe" | "nodejs" | "nodejs.exe")
}

fn format_node_script(tokens: &[String], script_idx: usize, script: &str) -> Option<String> {
    let block = build_js_script_block(script)?;
    let mut parts: Vec<String> = Vec::with_capacity(tokens.len());
    for (idx, token) in tokens.iter().enumerate() {
        if idx == script_idx {
            parts.push(block.clone());
        } else {
            parts.push(escape_token_for_display(token));
        }
    }
    Some(parts.join(" "))
}

fn build_js_script_block(script: &str) -> Option<String> {
    let normalized = script.replace("\r\n", "\n");
    let lines: Vec<String> = if normalized.contains('\n') {
        normalized
            .lines()
            .map(|line| line.trim_end().to_string())
            .collect()
    } else {
        split_js_statements(&normalized)
    };

    let meaningful: Vec<String> = lines
        .into_iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    if meaningful.len() <= 1 {
        return None;
    }

    let indented = indent_js_lines(meaningful);
    let mut block = String::from("'\n");
    for line in indented {
        block.push_str("    ");
        block.push_str(line.as_str());
        block.push('\n');
    }
    block.push('\'');
    Some(block)
}

fn split_js_statements(script: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;
    let mut paren_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;

    for ch in script.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        match ch {
            '\\' if in_single || in_double || in_backtick => {
                escape = true;
                current.push(ch);
                continue;
            }
            '\'' if !in_double && !in_backtick => {
                in_single = !in_single;
                current.push(ch);
                continue;
            }
            '"' if !in_single && !in_backtick => {
                in_double = !in_double;
                current.push(ch);
                continue;
            }
            '`' if !in_single && !in_double => {
                in_backtick = !in_backtick;
                current.push(ch);
                continue;
            }
            _ => {}
        }

        if !(in_single || in_double || in_backtick) {
            match ch {
                '{' => brace_depth += 1,
                '}' => {
                    if brace_depth > 0 {
                        brace_depth -= 1;
                    }
                }
                '(' => paren_depth += 1,
                ')' => {
                    if paren_depth > 0 {
                        paren_depth -= 1;
                    }
                }
                '[' => bracket_depth += 1,
                ']' => {
                    if bracket_depth > 0 {
                        bracket_depth -= 1;
                    }
                }
                ';' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                    current.push(ch);
                    let seg = current.trim().to_string();
                    if !seg.is_empty() {
                        segments.push(seg);
                    }
                    current.clear();
                    continue;
                }
                '\n' if brace_depth == 0 && paren_depth == 0 && bracket_depth == 0 => {
                    let seg = current.trim().to_string();
                    if !seg.is_empty() {
                        segments.push(seg);
                    }
                    current.clear();
                    continue;
                }
                _ => {}
            }
        }

        current.push(ch);
    }

    let seg = current.trim().to_string();
    if !seg.is_empty() {
        segments.push(seg);
    }
    segments
}

fn indent_js_lines(lines: Vec<String>) -> Vec<String> {
    let mut indented: Vec<String> = Vec::with_capacity(lines.len());
    let mut indent_level: usize = 0;

    for raw in lines {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            indented.push(String::new());
            continue;
        }

        let mut leading_closers = 0usize;
        let mut cut = trimmed.len();
        for (idx, ch) in trimmed.char_indices() {
            match ch {
                '}' | ']' => {
                    leading_closers += 1;
                    cut = idx + ch.len_utf8();
                    continue;
                }
                _ => {
                    cut = idx;
                    break;
                }
            }
        }

        if leading_closers > 0 && cut >= trimmed.len() {
            cut = trimmed.len();
        }

        if leading_closers > 0 {
            indent_level = indent_level.saturating_sub(leading_closers);
        }

        let remainder = trimmed[cut..].trim_start();
        let mut line = String::with_capacity(remainder.len() + indent_level * 4);
        for _ in 0..indent_level {
            line.push_str("    ");
        }
        if remainder.is_empty() && cut < trimmed.len() {
            line.push_str(trimmed);
        } else {
            line.push_str(remainder);
        }
        indented.push(line);

        let (opens, closes) = js_brace_deltas(trimmed);
        indent_level = indent_level + opens;
        indent_level = indent_level.saturating_sub(closes);
    }

    indented
}

fn js_brace_deltas(line: &str) -> (usize, usize) {
    let mut opens = 0usize;
    let mut closes = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;

    for ch in line.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_single || in_double || in_backtick => {
                escape = true;
            }
            '\'' if !in_double && !in_backtick => in_single = !in_single,
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            '{' if !(in_single || in_double || in_backtick) => opens += 1,
            '}' if !(in_single || in_double || in_backtick) => closes += 1,
            _ => {}
        }
    }

    (opens, closes)
}

fn is_shell_invocation_token(token: &str) -> bool {
    is_shell_executable(token)
}

fn format_shell_script(tokens: &[String], script_idx: usize, script: &str) -> Option<String> {
    let block = build_shell_script_block(script)?;
    let mut parts: Vec<String> = Vec::with_capacity(tokens.len());
    for (idx, token) in tokens.iter().enumerate() {
        if idx == script_idx {
            parts.push(block.clone());
        } else {
            parts.push(escape_token_for_display(token));
        }
    }
    Some(parts.join(" "))
}

fn build_shell_script_block(script: &str) -> Option<String> {
    let normalized = script.replace("\r\n", "\n");
    let segments = split_shell_statements(&normalized);
    let meaningful: Vec<String> = segments
        .into_iter()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    if meaningful.len() <= 1 {
        return None;
    }
    let indented = indent_shell_lines(meaningful);
    let mut block = String::from("'\n");
    for line in indented {
        block.push_str("    ");
        block.push_str(line.as_str());
        block.push('\n');
    }
    block.push('\'');
    Some(block)
}

fn split_shell_statements(script: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    let chars: Vec<char> = script.chars().collect();
    let mut idx = 0;
    while idx < chars.len() {
        let ch = chars[idx];
        if escape {
            current.push(ch);
            escape = false;
            idx += 1;
            continue;
        }
        match ch {
            '\\' if in_single || in_double => {
                escape = true;
                current.push(ch);
                idx += 1;
                continue;
            }
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(ch);
                idx += 1;
                continue;
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(ch);
                idx += 1;
                continue;
            }
            ';' if !(in_single || in_double) => {
                current.push(ch);
                segments.push(current.trim().to_string());
                current.clear();
                idx += 1;
                continue;
            }
            '&' | '|' if !(in_single || in_double) => {
                let current_op = ch;
                if idx + 1 < chars.len() && chars[idx + 1] == current_op {
                    if !current.trim().is_empty() {
                        segments.push(current.trim().to_string());
                    }
                    segments.push(format!("{}{}", current_op, current_op));
                    current.clear();
                    idx += 2;
                    continue;
                }
            }
            '\n' if !(in_single || in_double) => {
                segments.push(current.trim().to_string());
                current.clear();
                idx += 1;
                continue;
            }
            _ => {}
        }
        current.push(ch);
        idx += 1;
    }

    if !current.trim().is_empty() {
        segments.push(current.trim().to_string());
    }

    segments
}

fn indent_shell_lines(lines: Vec<String>) -> Vec<String> {
    let mut indented: Vec<String> = Vec::with_capacity(lines.len());
    let mut indent_level: usize = 0;

    for raw in lines {
        if raw == "&&" || raw == "||" {
            let mut line = String::new();
            for _ in 0..indent_level {
                line.push_str("    ");
            }
            line.push_str(raw.as_str());
            indented.push(line);
            continue;
        }

        let trimmed = raw.trim();
        if trimmed.is_empty() {
            indented.push(String::new());
            continue;
        }

        if trimmed.starts_with("fi") || trimmed.starts_with("done") || trimmed.starts_with("esac") {
            indent_level = indent_level.saturating_sub(1);
        }

        let mut line = String::new();
        for _ in 0..indent_level {
            line.push_str("    ");
        }
        line.push_str(trimmed);
        indented.push(line);

        if trimmed.ends_with("do")
            || trimmed.ends_with("then")
            || trimmed.ends_with("{")
            || trimmed.starts_with("case ")
        {
            indent_level += 1;
        }
    }

    indented
}

fn adjust_bracket_depth(token: &str, paren: &mut i32, bracket: &mut i32, brace: &mut i32) {
    for ch in token.chars() {
        match ch {
            '(' => *paren += 1,
            ')' => *paren -= 1,
            '[' => *bracket += 1,
            ']' => *bracket -= 1,
            '{' => *brace += 1,
            '}' => *brace -= 1,
            _ => {}
        }
    }
    *paren = (*paren).max(0);
    *bracket = (*bracket).max(0);
    *brace = (*brace).max(0);
}

fn is_python_invocation_token(token: &str) -> bool {
    if token.is_empty() || token.contains('=') {
        return false;
    }

    let trimmed = token.trim_matches(|c| c == '\'' || c == '"');
    let base = Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase();

    if !base.starts_with("python") {
        return false;
    }

    let suffix = &base["python".len()..];
    suffix.is_empty()
        || suffix
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '.' || ch == 'w')
}

fn escape_token_for_display(token: &str) -> String {
    if is_shell_word(token) {
        token.to_string()
    } else {
        let mut escaped = String::from("'");
        for ch in token.chars() {
            if ch == '\'' {
                escaped.push_str("'\\''");
            } else {
                escaped.push(ch);
            }
        }
        escaped.push('\'');
        escaped
    }
}

fn is_shell_word(token: &str) -> bool {
    token.chars().all(|ch| matches!(
        ch,
        'a'..='z'
            | 'A'..='Z'
            | '0'..='9'
            | '_'
            | '-'
            | '.'
            | '/'
            | ':'
            | ','
            | '@'
            | '%'
            | '+'
            | '='
            | '['
            | ']'
    ))
}

fn script_has_semicolon_outside_quotes(script: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    for ch in script.chars() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_single || in_double => {
                escape = true;
            }
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            ';' if !in_single && !in_double => return true,
            _ => {}
        }
    }

    false
}

fn split_semicolon_statements(script: &str) -> Vec<String> {
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;

    for ch in script.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        match ch {
            '\\' if in_single || in_double => {
                escape = true;
                current.push(ch);
            }
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(ch);
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(ch);
            }
            ';' if !in_single && !in_double => {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    segments.push(trimmed.to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        segments.push(trimmed.to_string());
    }

    segments
}

pub(crate) fn running_status_line(message: String) -> Line<'static> {
    Line::from(vec![
        Span::styled("└ ", Style::default().fg(crate::colors::border_dim())),
        Span::styled(message, Style::default().fg(crate::colors::text_dim())),
    ])
}

fn new_parsed_command(
    parsed_commands: &[ParsedCommand],
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    start_time: Option<Instant>,
) -> Vec<Line<'static>> {
    let meta = ParsedExecMetadata::from_commands(parsed_commands);
    let action = meta.action;
    let ctx_path = meta.ctx_path.as_deref();
    let suppress_run_header = matches!(action, ExecAction::Run) && output.is_some();
    let mut lines: Vec<Line> = Vec::new();
    let mut running_status: Option<Line<'static>> = None;
    if !suppress_run_header {
        match output {
            None => {
                if matches!(action, ExecAction::Run) {
                    let mut message = match &ctx_path {
                        Some(p) => format!("Running... in {p}"),
                        None => "Running...".to_string(),
                    };
                    if let Some(start) = start_time {
                        let elapsed = start.elapsed();
                        message = format!("{message} ({})", format_duration(elapsed));
                    }
                    running_status = Some(running_status_line(message));
                } else {
                    let duration_suffix = if let Some(start) = start_time {
                        let elapsed = start.elapsed();
                        format!(" ({})", format_duration(elapsed))
                    } else {
                        String::new()
                    };
                    let header = match action {
                        ExecAction::Read => "Read",
                        ExecAction::Search => "Search",
                        ExecAction::List => "List",
                        ExecAction::Run => unreachable!(),
                    };
                    lines.push(Line::styled(
                        format!("{header}{duration_suffix}"),
                        Style::default().fg(crate::colors::text_dim()),
                    ));
                }
            }
            Some(o) if o.exit_code == 0 => {
                if matches!(
                    action,
                    ExecAction::Read | ExecAction::Search | ExecAction::List
                ) {
                    lines.push(Line::styled(
                        match action {
                            ExecAction::Read => "Read",
                            ExecAction::Search => "Search",
                            ExecAction::List => "List",
                            ExecAction::Run => unreachable!(),
                        },
                        Style::default().fg(crate::colors::text()),
                    ));
                } else {
                    let done = match ctx_path {
                        Some(p) => format!("Ran in {p}"),
                        None => "Ran".to_string(),
                    };
                    lines.push(Line::styled(
                        done,
                        Style::default()
                            .fg(crate::colors::text_bright())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
            Some(_o) => {
                if matches!(
                    action,
                    ExecAction::Read | ExecAction::Search | ExecAction::List
                ) {
                    lines.push(Line::styled(
                        match action {
                            ExecAction::Read => "Read",
                            ExecAction::Search => "Search",
                            ExecAction::List => "List",
                            ExecAction::Run => unreachable!(),
                        },
                        Style::default().fg(crate::colors::text()),
                    ));
                } else {
                    let done = match ctx_path {
                        Some(p) => format!("Ran in {p}"),
                        None => "Ran".to_string(),
                    };
                    lines.push(Line::styled(
                        done,
                        Style::default()
                            .fg(crate::colors::text_bright())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
            }
        }
    }

    // Collect any paths referenced by search commands to suppress redundant directory lines
    let search_paths = &meta.search_paths;

    // We'll emit only content lines here; the header above already communicates the action.
    // Use a single leading "└ " for the very first content line, then indent subsequent ones,
    // except when we're showing an inline running status for ExecAction::Run.
    let mut any_content_emitted = false;
    let use_content_connectors = !(matches!(action, ExecAction::Run) && output.is_none());

    // Restrict displayed entries to the primary action for this cell.
    // For the generic "run" header, allow Run/Test/Lint/Format entries.
    let expected_label: Option<&'static str> = match action {
        ExecAction::Read => Some("Read"),
        ExecAction::Search => Some("Search"),
        ExecAction::List => Some("List"),
        ExecAction::Run => None,
    };

    for parsed in parsed_commands.iter() {
        // Produce a logical label and content string without icons
        let (label, content) = match parsed {
            ParsedCommand::Read { name, cmd, .. } => {
                let mut c = name.clone();
                if let Some(ann) = parse_read_line_annotation(cmd) {
                    c = format!("{c} {ann}");
                }
                ("Read".to_string(), c)
            }
            ParsedCommand::ListFiles { cmd: _, path } => match path {
                Some(p) => {
                    if search_paths.contains(p) {
                        (String::new(), String::new()) // suppressed
                    } else {
                        let display_p = if p.ends_with('/') {
                            p.to_string()
                        } else {
                            format!("{p}/")
                        };
                        ("List".to_string(), format!("{display_p}"))
                    }
                }
                None => ("List".to_string(), "./".to_string()),
            },
            ParsedCommand::Search { query, path, cmd } => {
                // Format query for display: unescape backslash-escapes and close common unbalanced delimiters
                let prettify_term = |s: &str| -> String {
                    // General unescape: turn "\X" into "X" for any X
                    let mut out = String::with_capacity(s.len());
                    let mut iter = s.chars();
                    while let Some(ch) = iter.next() {
                        if ch == '\\' {
                            if let Some(next) = iter.next() {
                                out.push(next);
                            } else {
                                out.push('\\');
                            }
                        } else {
                            out.push(ch);
                        }
                    }
                    // Balance parentheses
                    let opens_paren = out.matches("(").count();
                    let closes_paren = out.matches(")").count();
                    for _ in 0..opens_paren.saturating_sub(closes_paren) {
                        out.push(')');
                    }
                    // Balance curly braces
                    let opens_curly = out.matches("{").count();
                    let closes_curly = out.matches("}").count();
                    for _ in 0..opens_curly.saturating_sub(closes_curly) {
                        out.push('}');
                    }
                    out
                };
                let fmt_query = |q: &str| -> String {
                    let mut parts: Vec<String> = q
                        .split('|')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(prettify_term)
                        .collect();
                    match parts.len() {
                        0 => String::new(),
                        1 => parts.remove(0),
                        2 => format!("{} and {}", parts[0], parts[1]),
                        _ => {
                            let last = parts.last().cloned().unwrap_or_default();
                            let head = &parts[..parts.len() - 1];
                            format!("{} and {}", head.join(", "), last)
                        }
                    }
                };
                match (query, path) {
                    (Some(q), Some(p)) => {
                        let display_p = if p.ends_with('/') {
                            p.to_string()
                        } else {
                            format!("{p}/")
                        };
                        (
                            "Search".to_string(),
                            format!("{} in {}", fmt_query(q), display_p),
                        )
                    }
                    (Some(q), None) => ("Search".to_string(), format!("{}", fmt_query(q))),
                    (None, Some(p)) => {
                        let display_p = if p.ends_with('/') {
                            p.to_string()
                        } else {
                            format!("{p}/")
                        };
                        ("Search".to_string(), format!(" in {}", display_p))
                    }
                    (None, None) => ("Search".to_string(), cmd.clone()),
                }
            }
            ParsedCommand::ReadCommand { cmd } => ("Run".to_string(), cmd.clone()),
            // Upstream-only variants handled as generic runs in this fork
            ParsedCommand::Unknown { cmd } => {
                let t = cmd.trim();
                let lower = t.to_lowercase();
                if lower.starts_with("echo") && lower.contains("---") {
                    (String::new(), String::new())
                } else {
                    ("Run".to_string(), format_inline_script_for_display(cmd))
                }
            } // ParsedCommand::Noop { .. } => continue,
        };

        // Keep only entries that match the primary action grouping.
        if let Some(exp) = expected_label {
            if label != exp {
                continue;
            }
        } else if !(label == "Run" || label == "Search") {
            continue;
        }

        // Skip suppressed entries
        if label.is_empty() && content.is_empty() {
            continue;
        }

        // Split content into lines and push without repeating the action label
        for line_text in content.lines() {
            if line_text.is_empty() {
                continue;
            }
            let prefix = if !any_content_emitted {
                if suppress_run_header || !use_content_connectors {
                    ""
                } else {
                    "└ "
                }
            } else if suppress_run_header || !use_content_connectors {
                ""
            } else {
                "  "
            };
            let mut spans: Vec<Span<'static>> = Vec::new();
            if !prefix.is_empty() {
                spans.push(Span::styled(
                    prefix,
                    Style::default().add_modifier(Modifier::DIM),
                ));
            }

            match label.as_str() {
                // Highlight searched terms in normal text color; keep connectors/path dim
                "Search" => {
                    let remaining = line_text.to_string();
                    // Split off optional path suffix. Support both " (in ...)" and " in <dir>/" forms.
                    let (terms_part, path_part) = if let Some(idx) = remaining.rfind(" (in ") {
                        (
                            remaining[..idx].to_string(),
                            Some(remaining[idx..].to_string()),
                        )
                    } else if let Some(idx) = remaining.rfind(" in ") {
                        let suffix = &remaining[idx + 1..]; // keep leading space for styling
                        // Heuristic: treat as path if it ends with '/'
                        if suffix.trim_end().ends_with('/') {
                            (
                                remaining[..idx].to_string(),
                                Some(remaining[idx..].to_string()),
                            )
                        } else {
                            (remaining.clone(), None)
                        }
                    } else {
                        (remaining.clone(), None)
                    };
                    // Tokenize terms by ", " and " and " while preserving separators
                    let tmp = terms_part.clone();
                    // First, split by ", "
                    let chunks: Vec<String> = if tmp.contains(", ") {
                        tmp.split(", ").map(|s| s.to_string()).collect()
                    } else {
                        vec![tmp.clone()]
                    };
                    for (i, chunk) in chunks.iter().enumerate() {
                        if i > 0 {
                            // Add comma separator between items (dim)
                            spans.push(Span::styled(
                                ", ",
                                Style::default().fg(crate::colors::text_dim()),
                            ));
                        }
                        // Within each chunk, if it contains " and ", split into left and right with dimmed " and "
                        if let Some((left, right)) = chunk.rsplit_once(" and ") {
                            if !left.is_empty() {
                                spans.push(Span::styled(
                                    left.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                                spans.push(Span::styled(
                                    " and ",
                                    Style::default().fg(crate::colors::text_dim()),
                                ));
                                spans.push(Span::styled(
                                    right.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                            } else {
                                spans.push(Span::styled(
                                    chunk.to_string(),
                                    Style::default().fg(crate::colors::text()),
                                ));
                            }
                        } else {
                            spans.push(Span::styled(
                                chunk.to_string(),
                                Style::default().fg(crate::colors::text()),
                            ));
                        }
                    }
                    if let Some(p) = path_part {
                        // Dim the entire path portion including the " in " or " (in " prefix
                        spans.push(Span::styled(
                            p,
                            Style::default().fg(crate::colors::text_dim()),
                        ));
                    }
                }
                // Highlight filenames in Read; keep line ranges dim
                "Read" => {
                    if let Some(idx) = line_text.find(" (") {
                        let (fname, rest) = line_text.split_at(idx);
                        spans.push(Span::styled(
                            fname.to_string(),
                            Style::default().fg(crate::colors::text()),
                        ));
                        spans.push(Span::styled(
                            rest.to_string(),
                            Style::default().fg(crate::colors::text_dim()),
                        ));
                    } else {
                        spans.push(Span::styled(
                            line_text.to_string(),
                            Style::default().fg(crate::colors::text()),
                        ));
                    }
                }
                // List: highlight directory names
                "List" => {
                    spans.push(Span::styled(
                        line_text.to_string(),
                        Style::default().fg(crate::colors::text()),
                    ));
                }
                _ => {
                    // For executed commands (Run/Test/Lint/etc.), use shell syntax highlighting.
                    let normalized = normalize_shell_command_display(line_text);
                    let display_line = insert_line_breaks_after_double_ampersand(&normalized);
                    let mut hl =
                        crate::syntax_highlight::highlight_code_block(&display_line, Some("bash"));
                    if let Some(mut first_line) = hl.pop() {
                        emphasize_shell_command_name(&mut first_line);
                        spans.extend(first_line.spans.into_iter());
                    } else {
                        spans.push(Span::styled(
                            display_line,
                            Style::default().fg(crate::colors::text()),
                        ));
                    }
                }
            }

            lines.push(Line::from(spans));
            any_content_emitted = true;
        }
    }

    // If this is a List cell and the loop above produced no content (e.g.,
    // the list path was suppressed because a Search referenced the same path),
    // emit a single contextual line so the location is always visible.
    if matches!(action, ExecAction::List) && !any_content_emitted {
        let display_p = match ctx_path {
            Some(p) if !p.is_empty() => {
                if p.ends_with('/') {
                    p.to_string()
                } else {
                    format!("{p}/")
                }
            }
            _ => "./".to_string(),
        };
        lines.push(Line::from(vec![
            Span::styled("└ ", Style::default().add_modifier(Modifier::DIM)),
            Span::styled(
                format!("{display_p}"),
                Style::default().fg(crate::colors::text()),
            ),
        ]));
        // no-op: avoid unused assignment warning; the variable's value is not consumed later
    }

    // Show stdout for real run commands; keep read/search/list concise unless error
    let show_stdout = matches!(action, ExecAction::Run);
    let use_angle_pipe = show_stdout; // add "> " prefix for run output
    let display_output = output.or(stream_preview);
    let mut preview_lines = output_lines(display_output, !show_stdout, use_angle_pipe);
    if let Some(status_line) = running_status {
        if let Some(last) = preview_lines.last() {
            let is_blank = last
                .spans
                .iter()
                .all(|sp| sp.content.as_ref().trim().is_empty());
            if is_blank {
                preview_lines.pop();
            }
        }
        preview_lines.push(status_line);
    }
    lines.extend(preview_lines);
    lines.push(Line::from(""));
    lines
}

fn new_exec_command_generic(
    command: &[String],
    output: Option<&CommandOutput>,
    stream_preview: Option<&CommandOutput>,
    start_time: Option<Instant>,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let command_escaped = strip_bash_lc_and_escape(command);
    let normalized = normalize_shell_command_display(&command_escaped);
    let command_display = insert_line_breaks_after_double_ampersand(&normalized);
    // Highlight the command as bash and then append a dimmed duration to the
    // first visual line while running.
    let mut highlighted_cmd =
        crate::syntax_highlight::highlight_code_block(&command_display, Some("bash"));

    for (idx, line) in highlighted_cmd.iter_mut().enumerate() {
        emphasize_shell_command_name(line);
        if idx > 0 {
            line.spans.insert(
                0,
                Span::styled("  ", Style::default().fg(crate::colors::text())),
            );
        }
    }

    let render_running_header = output.is_none();
    let display_output = output.or(stream_preview);
    let mut running_status = None;
    if render_running_header {
        let mut message = "Running...".to_string();
        if let Some(start) = start_time {
            let elapsed = start.elapsed();
            message = format!("{message} ({})", format_duration(elapsed));
        }
        running_status = Some(running_status_line(message));
    }

    if output.is_some() {
        for line in highlighted_cmd.iter_mut() {
            for span in line.spans.iter_mut() {
                span.style = span.style.fg(crate::colors::text_bright());
            }
        }
    }

    lines.extend(highlighted_cmd);

    let mut preview_lines = output_lines(display_output, false, true);
    if let Some(status_line) = running_status {
        if let Some(last) = preview_lines.last() {
            let is_blank = last
                .spans
                .iter()
                .all(|sp| sp.content.as_ref().trim().is_empty());
            if is_blank {
                preview_lines.pop();
            }
        }
        preview_lines.push(status_line);
    }

    lines.extend(preview_lines);
    lines
}

fn format_inline_node_for_display(command_escaped: &str) -> Option<String> {
    let tokens: Vec<String> = Shlex::new(command_escaped).collect();
    if tokens.len() < 2 {
        return None;
    }

    let node_idx = tokens
        .iter()
        .position(|token| is_node_invocation_token(token))?;

    let mut idx = node_idx + 1;
    while idx < tokens.len() {
        match tokens[idx].as_str() {
            "-e" | "--eval" | "-p" | "--print" => {
                let script_idx = idx + 1;
                if script_idx >= tokens.len() {
                    return None;
                }
                return format_node_script(&tokens, script_idx, tokens[script_idx].as_str());
            }
            "--" => break,
            _ => idx += 1,
        }
    }

    None
}

fn format_inline_shell_for_display(command_escaped: &str) -> Option<String> {
    let tokens: Vec<String> = Shlex::new(command_escaped).collect();
    if tokens.len() < 3 {
        return None;
    }

    let shell_idx = tokens
        .iter()
        .position(|t| is_shell_invocation_token(t))?;

    let flag_idx = shell_idx + 1;
    if flag_idx >= tokens.len() {
        return None;
    }

    let flag = tokens[flag_idx].as_str();
    if flag != "-c" && flag != "-lc" {
        return None;
    }

    let script_idx = flag_idx + 1;
    if script_idx >= tokens.len() {
        return None;
    }

    format_shell_script(&tokens, script_idx, tokens[script_idx].as_str())
}
