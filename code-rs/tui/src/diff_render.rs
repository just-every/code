use crossterm::terminal;
// Color type is already in scope at the top of this module
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line as RtLine;
use ratatui::text::Span as RtSpan;
use std::collections::HashMap;
use std::path::PathBuf;

use code_core::protocol::FileChange;

use crate::history_cell::PatchEventType;
use crate::sanitize::{sanitize_for_tui, Mode as SanitizeMode, Options as SanitizeOptions};

// Sanitize diff content so tabs and control characters don’t break terminal layout.
// Mirrors the behavior we use for user input and command output:
// - Expand tabs to spaces using a fixed tab stop (4)
// - Remove ASCII control characters (including ESC/CSI sequences) that could
//   confuse terminal rendering; keep plain text only
#[allow(dead_code)]
fn expand_tabs_to_spaces(input: &str, tabstop: usize) -> String {
    let ts = tabstop.max(1);
    let mut out = String::with_capacity(input.len());
    let mut col = 0usize;
    for ch in input.chars() {
        match ch {
            '\t' => {
                let spaces = ts - (col % ts);
                out.extend(std::iter::repeat(' ').take(spaces));
                col += spaces;
            }
            _ => {
                // Treat all other chars as width 1 for our fixed-width wrapping pre-pass.
                // The ratatui layer will handle wide glyphs.
                out.push(ch);
                col += 1;
            }
        }
    }
    out
}

#[allow(dead_code)]
fn strip_control_sequences(input: &str) -> String {
    fn is_c1(ch: char) -> bool {
        let u = ch as u32;
        (0x80..=0x9F).contains(&u)
    }

    fn is_zero_width_or_bidi(ch: char) -> bool {
        matches!(
            ch,
            // Zero-width, joiners, BOM
            '\u{200B}' /* ZWSP */
                | '\u{200C}' /* ZWNJ */
                | '\u{200D}' /* ZWJ */
                | '\u{2060}' /* WJ */
                | '\u{FEFF}' /* BOM */
                | '\u{00AD}' /* SOFT HYPHEN */
                | '\u{180E}' /* MONGOLIAN VOWEL SEPARATOR (historic) */
                // BiDi controls and isolates
                | '\u{200E}' /* LRM */
                | '\u{200F}' /* RLM */
                | '\u{061C}' /* ALM */
                | '\u{202A}' /* LRE */
                | '\u{202B}' /* RLE */
                | '\u{202D}' /* LRO */
                | '\u{202E}' /* RLO */
                | '\u{202C}' /* PDF */
                | '\u{2066}' /* LRI */
                | '\u{2067}' /* RLI */
                | '\u{2068}' /* FSI */
                | '\u{2069}' /* PDI */
        )
    }

    fn consume_until_st_or_bel<I: Iterator<Item = char>>(it: &mut std::iter::Peekable<I>) {
        while let Some(&c) = it.peek() {
            match c {
                // BEL terminator for OSC
                '\u{0007}' => {
                    it.next(); // eat BEL
                    break;
                }
                // ST = ESC \
                '\u{001B}' => {
                    // lookahead for '\\'
                    let _ = it.next(); // ESC
                    if matches!(it.peek(), Some('\\')) {
                        let _ = it.next(); // '\\'
                        break;
                    }
                    // Otherwise, keep eating (this also drops nested sequences defensively)
                }
                _ => {
                    let _ = it.next();
                }
            }
        }
    }

    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            // ESC-prefixed sequences
            '\u{001B}' => {
                match chars.peek().copied() {
                    // CSI: ESC [ params/intermediates final
                    Some('[') => {
                        let _ = chars.next(); // '['
                        // params 0x30..0x3F, intermediates 0x20..0x2F, final 0x40..0x7E
                        while let Some(&c) = chars.peek() {
                            let u = c as u32;
                            if (0x40..=0x7E).contains(&u) {
                                let _ = chars.next();
                                break;
                            } else {
                                let _ = chars.next();
                            }
                        }
                    }
                    // OSC: ESC ] ... (BEL | ST)
                    Some(']') => {
                        let _ = chars.next(); // ']'
                        consume_until_st_or_bel(&mut chars);
                    }
                    // String types (DCS/SOS/PM/APC): ESC P | X | ^ | _ ... ST
                    Some('P') | Some('X') | Some('^') | Some('_') => {
                        let _ = chars.next();
                        consume_until_st_or_bel(&mut chars);
                    }
                    // Other ESC: consume optional intermediates then a final (0x40..0x7E)
                    Some(_) | None => {
                        // intermediates 0x20..0x2F
                        while let Some(&c) = chars.peek() {
                            let u = c as u32;
                            if (0x20..=0x2F).contains(&u) {
                                let _ = chars.next();
                            } else {
                                break;
                            }
                        }
                        if let Some(&c) = chars.peek() {
                            let u = c as u32;
                            if (0x40..=0x7E).contains(&u) {
                                let _ = chars.next();
                            }
                        }
                    }
                }
                // In all ESC cases: skip emission
            }
            // Drop other C0 control characters (0x00..0x1F, 0x7F)
            c if (c as u32) < 0x20 || c == '\u{007F}' => {}
            // Drop raw C1 controls if present (0x80..0x9F)
            c if is_c1(c) => {}
            // Drop zero-width and bidi controls that can affect layout
            c if is_zero_width_or_bidi(c) => {}
            // Keep printable character
            _ => out.push(ch),
        }
    }
    out
}

#[inline]
fn sanitize_diff_text(s: &str) -> String {
    sanitize_for_tui(
        s,
        SanitizeMode::Plain,
        SanitizeOptions { expand_tabs: true, tabstop: 4, debug_markers: false },
    )
}

#[allow(dead_code)]
// Keep one space between the line number and the sign column for typical
// 4‑digit line numbers (e.g., "1235 + "). This value is the total target
// width for "<ln><gap>", so with 4 digits we get 1 space gap.
const SPACES_AFTER_LINE_NUMBER: usize = 6;

// Internal representation for diff line rendering
#[allow(dead_code)]
enum DiffLineType {
    Insert,
    Delete,
    Context,
}

#[allow(dead_code)]
pub(super) fn create_diff_summary(
    title: &str,
    changes: &HashMap<PathBuf, FileChange>,
    event_type: PatchEventType,
) -> Vec<RtLine<'static>> {
    create_diff_summary_with_width(title, changes, event_type, None)
}

/// Same as `create_diff_summary` but allows specifying a target content width in columns.
/// When `width_cols` is provided, wrapping for detailed diff lines uses that width to
/// ensure hanging indents align within the caller’s render area.
pub(super) fn create_diff_summary_with_width(
    title: &str,
    changes: &HashMap<PathBuf, FileChange>,
    event_type: PatchEventType,
    width_cols: Option<usize>,
) -> Vec<RtLine<'static>> {
    struct FileSummary {
        original_path: String,
        rename_target: Option<String>,
        added: usize,
        removed: usize,
        change: FileSummaryKind,
    }

    enum FileSummaryKind {
        Add { empty: bool },
        Delete { removed_unknown: bool },
        Update {
            rename_only: bool,
            no_content_change: bool,
            binary: bool,
            metadata_only: bool,
        },
    }

    struct DiffTally {
        added: usize,
        removed: usize,
        binary_marker: bool,
        metadata_marker: bool,
    }

    let count_from_unified = |diff: &str| -> DiffTally {
        let mut binary_marker = diff.contains("Binary files") || diff.contains("GIT binary patch");
        if diff.as_bytes().contains(&0) {
            binary_marker = true;
        }
        let mut metadata_marker = diff_contains_metadata_markers(diff);
        if let Ok(patch) = diffy::Patch::from_str(diff) {
            let (added, removed) = patch
                .hunks()
                .iter()
                .flat_map(|h| h.lines())
                .fold((0, 0), |(a, d), l| match l {
                    diffy::Line::Insert(_) => (a + 1, d),
                    diffy::Line::Delete(_) => (a, d + 1),
                    _ => (a, d),
                });
            DiffTally {
                added,
                removed,
                binary_marker,
                metadata_marker,
            }
        } else {
            // Fallback: manual scan to preserve counts even for unparsable diffs
            let mut adds = 0usize;
            let mut dels = 0usize;
            for l in diff.lines() {
                if l.starts_with("+++") || l.starts_with("---") || l.starts_with("@@") {
                    continue;
                }
                let first = l.as_bytes().first().copied();
                match first {
                    Some(b'+') => adds += 1,
                    Some(b'-') => dels += 1,
                    _ => {}
                }
                if l.contains("Binary files") || l.contains("GIT binary patch") {
                    binary_marker = true;
                }
                if !metadata_marker && line_has_metadata_marker(l) {
                    metadata_marker = true;
                }
            }
            DiffTally {
                added: adds,
                removed: dels,
                binary_marker,
                metadata_marker,
            }
        }
    };

    let mut files: Vec<FileSummary> = Vec::new();
    for (path, change) in changes.iter() {
        match change {
            FileChange::Add { content } => {
                let added = content.lines().count();
                let empty = added == 0;
                files.push(FileSummary {
                    original_path: path.display().to_string(),
                    rename_target: None,
                    added,
                    removed: 0,
                    change: FileSummaryKind::Add { empty },
                });
            }
            FileChange::Delete => {
                let (removed, removed_unknown) = match std::fs::read_to_string(path) {
                    Ok(existing) => (existing.lines().count(), false),
                    Err(_) => (0, true),
                };
                files.push(FileSummary {
                    original_path: path.display().to_string(),
                    rename_target: None,
                    added: 0,
                    removed,
                    change: FileSummaryKind::Delete { removed_unknown },
                });
            }
            FileChange::Update {
                unified_diff,
                move_path,
                ..
            } => {
                let tally = count_from_unified(unified_diff);
                let rename_target = move_path.as_ref().map(|new_path| new_path.display().to_string());
                let binary_change = tally.binary_marker;
                let metadata_only = tally.metadata_marker
                    && tally.added == 0
                    && tally.removed == 0;
                let rename_only = rename_target.is_some()
                    && tally.added == 0
                    && tally.removed == 0
                    && !binary_change
                    && !tally.metadata_marker;
                let no_content_change = rename_target.is_none()
                    && !binary_change
                    && !tally.metadata_marker
                    && tally.added == 0
                    && tally.removed == 0;
                files.push(FileSummary {
                    original_path: path.display().to_string(),
                    rename_target,
                    added: tally.added,
                    removed: tally.removed,
                    change: FileSummaryKind::Update {
                        rename_only,
                        no_content_change,
                        binary: binary_change,
                        metadata_only,
                    },
                });
            }
        }
    }

    let file_count = files.len();
    let total_added: usize = files.iter().map(|f| f.added).sum();
    let total_removed: usize = files.iter().map(|f| f.removed).sum();
    let noun = if file_count == 1 { "file" } else { "files" };

    let mut out: Vec<RtLine<'static>> = Vec::new();

    // Header
    let mut header_spans: Vec<RtSpan<'static>> = Vec::new();
    // Colorize title: success for apply events, keep primary for approval requests
    let title_style = match event_type {
        PatchEventType::ApplyBegin { .. } | PatchEventType::ApplySuccess => Style::default()
            .fg(crate::colors::success())
            .add_modifier(Modifier::BOLD),
        PatchEventType::ApplyFailure => Style::default()
            .fg(crate::colors::error())
            .add_modifier(Modifier::BOLD),
        PatchEventType::ApprovalRequest => Style::default()
            .fg(crate::colors::primary())
            .add_modifier(Modifier::BOLD),
    };
    header_spans.push(RtSpan::styled(title.to_owned(), title_style));
    // Only include aggregate counts in header for approval requests; omit for apply/updated.
    if matches!(event_type, PatchEventType::ApprovalRequest) {
        header_spans.push(RtSpan::raw(" "));
        header_spans.push(RtSpan::raw(format!("{file_count} {noun} ")));
        header_spans.push(RtSpan::raw("("));
        header_spans.push(RtSpan::styled(
            format!("+{total_added}"),
            Style::default().fg(crate::colors::success()),
        ));
        header_spans.push(RtSpan::raw(" "));
        header_spans.push(RtSpan::styled(
            format!("-{total_removed}"),
            Style::default().fg(crate::colors::error()),
        ));
        header_spans.push(RtSpan::raw(")"));
    }
    out.push(RtLine::from(header_spans));

    // Per-file lines with prefix
    for (idx, f) in files.iter().enumerate() {
        let mut spans: Vec<RtSpan<'static>> = Vec::new();
        // Prefix
        let prefix = if idx == 0 { "└ " } else { "  " };
        spans.push(RtSpan::styled(
            prefix.to_string(),
            Style::default().add_modifier(Modifier::DIM),
        ));
        let dim_style = Style::default().fg(crate::colors::text_dim());

        if let FileSummaryKind::Update {
            rename_only: true,
            ..
        } = &f.change
        {
            if let Some(rename_target) = &f.rename_target {
                spans.push(RtSpan::styled(f.original_path.clone(), dim_style));
                spans.push(RtSpan::styled(" to ".to_string(), dim_style));
                spans.push(RtSpan::styled(
                    rename_target.clone(),
                    Style::default().fg(crate::colors::text()),
                ));
            } else {
                spans.push(RtSpan::styled(f.original_path.clone(), dim_style));
                spans.push(RtSpan::styled(" (renamed)".to_string(), dim_style));
            }
            out.push(RtLine::from(spans));
            continue;
        }

        let descriptor = if let Some(rename_target) = &f.rename_target {
            format!("{} → {}", f.original_path, rename_target)
        } else {
            f.original_path.clone()
        };
        let mut annotation: Option<String> = None;
        let mut skip_counts = false;

        match &f.change {
            FileSummaryKind::Update {
                metadata_only: true,
                ..
            } => {
                annotation = Some(" (metadata change)".to_string());
                skip_counts = true;
            }
            FileSummaryKind::Update {
                no_content_change: true,
                ..
            } => {
                annotation = Some(" (no changes)".to_string());
                skip_counts = true;
            }
            FileSummaryKind::Update {
                binary: true,
                ..
            } => {
                annotation = Some(" (binary change)".to_string());
                skip_counts = true;
            }
            FileSummaryKind::Delete {
                removed_unknown: true,
            } => {
                annotation = Some(" (deleted)".to_string());
                skip_counts = true;
            }
            FileSummaryKind::Add { empty: true } => {
                annotation = Some(" (empty file)".to_string());
                skip_counts = true;
            }
            _ => {}
        }

        spans.push(RtSpan::styled(descriptor, dim_style));
        if let Some(extra) = annotation {
            spans.push(RtSpan::styled(extra, dim_style));
        }

        if skip_counts {
            out.push(RtLine::from(spans));
            continue;
        }

        if !skip_counts {
            if let FileSummaryKind::Delete { removed_unknown: false } = &f.change {
                spans.push(RtSpan::styled(" (".to_string(), dim_style));
                spans.push(RtSpan::styled(
                    format!("-{}", f.removed),
                    Style::default().fg(crate::colors::error()),
                ));
                spans.push(RtSpan::styled(")".to_string(), dim_style));
            } else if matches!(
                &f.change,
                FileSummaryKind::Update { .. } | FileSummaryKind::Add { empty: false }
            ) {
                spans.push(RtSpan::styled(" (".to_string(), dim_style));
                spans.push(RtSpan::styled(
                    format!("+{}", f.added),
                    Style::default().fg(crate::colors::success()),
                ));
                spans.push(RtSpan::raw(" "));
                spans.push(RtSpan::styled(
                    format!("-{}", f.removed),
                    Style::default().fg(crate::colors::error()),
                ));
                spans.push(RtSpan::styled(")".to_string(), dim_style));
            }
        }
        out.push(RtLine::from(spans));
    }

    let show_details = matches!(
        event_type,
        PatchEventType::ApplyBegin {
            auto_approved: true
        }
            | PatchEventType::ApplySuccess
            | PatchEventType::ApprovalRequest
    );

    if show_details {
        out.extend(render_patch_details_with_width(changes, width_cols));
    }

    out
}

fn diff_contains_metadata_markers(diff: &str) -> bool {
    diff.contains("new file mode")
        || diff.contains("deleted file mode")
        || diff.contains("old mode")
        || diff.contains("new mode")
        || diff.contains("similarity index")
}

fn line_has_metadata_marker(line: &str) -> bool {
    line.contains("new file mode")
        || line.contains("deleted file mode")
        || line.contains("old mode")
        || line.contains("new mode")
        || line.contains("similarity index")
}

#[allow(dead_code)]
pub(super) fn render_patch_details(changes: &HashMap<PathBuf, FileChange>) -> Vec<RtLine<'static>> {
    render_patch_details_with_width(changes, None)
}

#[allow(dead_code)]
fn render_patch_details_with_width(
    changes: &HashMap<PathBuf, FileChange>,
    width_cols: Option<usize>,
) -> Vec<RtLine<'static>> {
    let mut out: Vec<RtLine<'static>> = Vec::new();
    // Use caller-provided width or fall back to a conservative estimate based on terminal width.
    // Subtract a gutter safety margin so our pre-wrapping rarely exceeds the
    // actual chat content width (prevents secondary wrapping that breaks hanging indents).
    let term_cols: usize = if let Some(w) = width_cols {
        w as usize
    } else {
        let full = terminal::size().map(|(w, _)| w as usize).unwrap_or(120);
        full.saturating_sub(20).max(40)
    };

    let total_files = changes.len();
    for (index, (path, change)) in changes.iter().enumerate() {
        let is_first_file = index == 0;
        // Add separator only between files (not at the very start)
        if !is_first_file {
            out.push(RtLine::from(vec![
                RtSpan::raw("    "),
                RtSpan::styled("...", style_dim()),
            ]));
        }
        match change {
            FileChange::Add { content } => {
                for (i, raw) in content.lines().enumerate() {
                    let ln = i + 1;
                    let cleaned = sanitize_diff_text(raw);
                    out.extend(push_wrapped_diff_line_with_width(
                        ln,
                        DiffLineType::Insert,
                        &cleaned,
                        term_cols,
                    ));
                }
            }
            FileChange::Delete => {
                let original = std::fs::read_to_string(path).unwrap_or_default();
                for (i, raw) in original.lines().enumerate() {
                    let ln = i + 1;
                    let cleaned = sanitize_diff_text(raw);
                    out.extend(push_wrapped_diff_line_with_width(
                        ln,
                        DiffLineType::Delete,
                        &cleaned,
                        term_cols,
                    ));
                }
            }
            FileChange::Update {
                unified_diff,
                move_path: _,
                ..
            } => {
                if let Ok(patch) = diffy::Patch::from_str(unified_diff) {
                    let mut is_first_hunk = true;
                    for h in patch.hunks() {
                        // Render a simple separator between non-contiguous hunks
                        // instead of diff-style @@ headers.
                        if !is_first_hunk {
                            out.push(RtLine::from(vec![
                                RtSpan::raw("    "),
                                RtSpan::styled("⋮", style_dim()),
                            ]));
                        }
                        is_first_hunk = false;

                        let mut old_ln = h.old_range().start();
                        let mut new_ln = h.new_range().start();
                        for l in h.lines() {
                            match l {
                                diffy::Line::Insert(text) => {
                                    let s = sanitize_diff_text(text.trim_end_matches('\n'));
                    out.extend(push_wrapped_diff_line_with_width(
                        new_ln,
                        DiffLineType::Insert,
                        &s,
                        term_cols,
                    ));
                                    new_ln += 1;
                                }
                                diffy::Line::Delete(text) => {
                                    let s = sanitize_diff_text(text.trim_end_matches('\n'));
                    out.extend(push_wrapped_diff_line_with_width(
                        old_ln,
                        DiffLineType::Delete,
                        &s,
                        term_cols,
                    ));
                                    old_ln += 1;
                                }
                                diffy::Line::Context(text) => {
                                    let s = sanitize_diff_text(text.trim_end_matches('\n'));
                    out.extend(push_wrapped_diff_line_with_width(
                        new_ln,
                        DiffLineType::Context,
                        &s,
                        term_cols,
                    ));
                                    old_ln += 1;
                                    new_ln += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Avoid trailing blank line at the very end; only add spacing
        // when there are more files following.
        if index + 1 < total_files {
            out.push(RtLine::from(RtSpan::raw("")));
        }
    }

    out
}

/// Produce only the detailed diff lines without any file-level headers/summaries.
/// Used by the Diff Viewer overlay where surrounding chrome already conveys context.
#[allow(dead_code)]
pub(super) fn create_diff_details_only(
    changes: &HashMap<PathBuf, FileChange>,
) -> Vec<RtLine<'static>> {
    render_patch_details(changes)
}

#[allow(dead_code)]
fn push_wrapped_diff_line_with_width(
    line_number: usize,
    kind: DiffLineType,
    text: &str,
    term_cols: usize,
) -> Vec<RtLine<'static>> {
    // Slightly smaller left padding so line numbers sit a couple of spaces left
    let indent = "  ";
    let ln_str = line_number.to_string();
    let mut remaining_text: &str = text;

    // Reserve a fixed number of spaces after the line number so that content starts
    // at a consistent column. Always include a 1‑char diff sign ("+"/"-" or space)
    // at the start of the content so gutters align across wrapped lines.
    let gap_after_ln = SPACES_AFTER_LINE_NUMBER.saturating_sub(ln_str.len());
    let prefix_cols = indent.len() + ln_str.len() + gap_after_ln;

    let mut first = true;
    // Continuation hanging indent equals the leading spaces of the content
    // (after the diff sign). This keeps wrapped rows aligned under the code
    // indentation.
    let continuation_indent: usize = text.chars().take_while(|c| *c == ' ').count();
    let (sign_opt, line_style) = match kind {
        DiffLineType::Insert => (Some('+'), Some(style_add())),
        DiffLineType::Delete => (Some('-'), Some(style_del())),
        DiffLineType::Context => (None, None),
    };
    let mut lines: Vec<RtLine<'static>> = Vec::new();

    loop {
        // Fit the content for the current terminal row:
        // compute how many columns are available after the prefix, then split
        // at a UTF-8 character boundary so this row's chunk fits exactly.
        // First line includes a visible sign plus a trailing space after it.
        // Continuation lines include only the hanging space (no sign).
        // First line reserves 1 col for the sign ('+'/'-') and 1 space after it.
        // Continuation lines must reserve BOTH columns as well (sign column + its trailing space)
        // before applying the hanging indent equal to the content's leading spaces.
        let base_prefix = if first { prefix_cols + 2 } else { prefix_cols + 2 + continuation_indent };
        let available_content_cols = term_cols
            .saturating_sub(base_prefix)
            .max(1);
        let split_at_byte_index = remaining_text
            .char_indices()
            .nth(available_content_cols)
            .map(|(i, _)| i)
            .unwrap_or_else(|| remaining_text.len());
        let (chunk, rest) = remaining_text.split_at(split_at_byte_index);
        remaining_text = rest;

        if first {
            let mut spans: Vec<RtSpan<'static>> = Vec::new();
            spans.push(RtSpan::raw(indent));
            spans.push(RtSpan::styled(ln_str.clone(), style_dim()));
            spans.push(RtSpan::raw(" ".repeat(gap_after_ln)));

            // Always prefix the content with a sign char for consistent gutters
            let sign_char = sign_opt.unwrap_or(' ');
            // Add a space after the sign so it sits centered in the sign column
            // and content starts one cell to the right: "+ <content>".
            let display_chunk = format!("{sign_char} {chunk}");

            let content_span = match line_style {
                Some(style) => RtSpan::styled(display_chunk, style),
                None => RtSpan::raw(display_chunk),
            };
            spans.push(content_span);
            let mut line = RtLine::from(spans);
            if let Some(style) = line_style {
                line.style = line.style.patch(style);
            }
            // Apply themed tinted background for added/removed lines
            if matches!(kind, DiffLineType::Insert | DiffLineType::Delete) {
                let tint = match kind {
                    DiffLineType::Insert => success_tint(),
                    DiffLineType::Delete => error_tint(),
                    DiffLineType::Context => crate::colors::background(),
                };
                line.style = line.style.bg(tint);
            }
            lines.push(line);
            first = false;
        } else {
            // Continuation lines keep a space for the sign column so content aligns
            let hang_prefix = format!(
                "{indent}{}{}  {}",
                " ".repeat(ln_str.len()),
                " ".repeat(gap_after_ln),
                " ".repeat(continuation_indent)
            );
            let content_span = match line_style {
                Some(style) => RtSpan::styled(chunk.to_string(), style),
                None => RtSpan::raw(chunk.to_string()),
            };
            let mut line = RtLine::from(vec![RtSpan::raw(hang_prefix), content_span]);
            if let Some(style) = line_style {
                line.style = line.style.patch(style);
            }
            if matches!(kind, DiffLineType::Insert | DiffLineType::Delete) {
                let tint = match kind {
                    DiffLineType::Insert => success_tint(),
                    DiffLineType::Delete => error_tint(),
                    DiffLineType::Context => crate::colors::background(),
                };
                line.style = line.style.bg(tint);
            }
            lines.push(line);
        }
        if remaining_text.is_empty() {
            break;
        }
    }
    lines
}

#[allow(dead_code)]
fn style_dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

#[allow(dead_code)]
fn style_add() -> Style {
    // Use theme success color for additions so it adapts to light/dark themes
    Style::default().fg(crate::colors::success())
}

#[allow(dead_code)]
fn style_del() -> Style {
    // Use theme error color for deletions so it adapts to light/dark themes
    Style::default().fg(crate::colors::error())
}

// --- Very light tinted backgrounds for insert/delete lines ------------------
use ratatui::style::Color;

fn success_tint() -> Color {
    crate::colors::tint_background_toward(crate::colors::success())
}

fn error_tint() -> Color {
    crate::colors::tint_background_toward(crate::colors::error())
}

// Removed per-line tinted backgrounds per design feedback
