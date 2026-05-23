use super::*;
use crate::exec_command::strip_bash_lc_and_escape;
use crate::history::state::{
    ExecAction,
    ExploreEntry,
    ExploreEntryStatus,
    ExploreRecord,
    ExploreSummary,
};
use code_core::parse_command::ParsedCommand;
use code_core::util::extract_shell_script;
use shlex::Shlex;
use std::path::{Component, Path};

pub(crate) struct ExploreAggregationCell {
    record: ExploreRecord,
    force_exploring_header: bool,
}

impl ExploreAggregationCell {
    pub(crate) fn from_record(record: ExploreRecord) -> Self {
        Self {
            record,
            force_exploring_header: false,
        }
    }

    pub(crate) fn record(&self) -> &ExploreRecord {
        &self.record
    }

    pub(crate) fn record_mut(&mut self) -> &mut ExploreRecord {
        &mut self.record
    }

    pub(crate) fn set_force_exploring_header(&mut self, force: bool) -> bool {
        if self.force_exploring_header == force {
            return false;
        }
        self.force_exploring_header = force;
        true
    }

    fn current_exec_status(&self) -> ExecStatus {
        if self
            .record
            .entries
            .iter()
            .any(|entry| matches!(entry.status, ExploreEntryStatus::Running))
        {
            ExecStatus::Running
        } else if self
            .record
            .entries
            .iter()
            .any(|entry| matches!(entry.status, ExploreEntryStatus::Error { .. }))
        {
            ExecStatus::Error
        } else {
            ExecStatus::Success
        }
    }
}

impl HistoryCell for ExploreAggregationCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Exec {
            kind: ExecKind::Search,
            status: self.current_exec_status(),
        }
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        explore_lines_from_record_with_force(&self.record, self.force_exploring_header)
    }

    fn desired_height(&self, width: u16) -> u16 {
        Paragraph::new(Text::from(self.display_lines_trimmed()))
            .wrap(Wrap { trim: false })
            .line_count(width)
            .try_into()
            .unwrap_or(0)
    }

    fn gutter_symbol(&self) -> Option<&'static str> {
        None
    }
}

pub(crate) fn explore_record_push_from_parsed(
    record: &mut ExploreRecord,
    parsed: &[ParsedCommand],
    status: ExploreEntryStatus,
    cwd: &Path,
    session_root: &Path,
    original_command: &[String],
) -> Option<usize> {
    let action = action_enum_from_parsed(parsed);
    let summary = match action {
        ExecAction::Search => parsed.iter().find_map(|p| match p {
            ParsedCommand::Search { query, path, cmd } => {
                let formatted_path = format_search_target(path.as_deref(), cwd, session_root);
                let pretty_query = query.clone().filter(|q| !q.trim().is_empty()).or_else(|| {
                    if query.is_none() {
                        Some(cmd.clone())
                    } else {
                        None
                    }
                });
                Some(ExploreSummary::Search {
                    query: pretty_query,
                    path: formatted_path,
                })
            }
            _ => None,
        }),
        ExecAction::List => parsed.iter().find_map(|p| match p {
            ParsedCommand::ListFiles { path, .. } => {
                let display = format_list_target(path.as_deref(), cwd, session_root);
                Some(ExploreSummary::List { path: display })
            }
            _ => None,
        }),
        ExecAction::Read => parsed.iter().find_map(|p| match p {
            ParsedCommand::Read { name, cmd, .. } => {
                let (annotation, range) = super::parse_read_line_annotation_with_range(cmd);
                let display_path = format_read_target(name, cwd, session_root);
                Some(ExploreSummary::Read {
                    display_path,
                    annotation,
                    range,
                })
            }
            _ => None,
        }),
        ExecAction::Run => parsed.iter().find_map(|p| match p {
            ParsedCommand::ReadCommand { cmd } => {
                if let Some(summary) = build_count_summary(cmd, cwd, session_root) {
                    return Some(ExploreSummary::Count {
                        target: summary.target,
                        annotation: summary.annotation,
                    });
                }

                let summary = build_command_summary(cmd, original_command);
                Some(ExploreSummary::Command {
                    display: summary.display,
                    annotation: summary.annotation,
                })
            }
            _ => None,
        }),
    };

    let summary = summary.or_else(|| {
        let text = parsed
            .iter()
            .map(|p| match p {
                ParsedCommand::Unknown { cmd } => cmd.clone(),
                _ => String::new(),
            })
            .find(|s| !s.is_empty())
            .unwrap_or_else(|| "exec".to_string());
        Some(ExploreSummary::Fallback { text })
    })?;

    if let ExploreSummary::Read {
        display_path,
        annotation,
        range,
    } = &summary
    {
        let path_key = display_path.clone();
        let annot = annotation.clone();
        let range_val = *range;
        for idx in (0..record.entries.len()).rev() {
            if let ExploreSummary::Read {
                display_path: existing_path,
                annotation: existing_ann,
                range: existing_range,
            } = &mut record.entries[idx].summary
            {
                if *existing_path == path_key {
                    let reuse = match (*existing_range, range_val) {
                        (Some((es, ee)), Some((ns, ne))) => {
                            if ns <= es && ne >= ee {
                                *existing_range = Some((ns, ne));
                                *existing_ann = annot.clone().or_else(|| annotation_for_range(ns, ne));
                                true
                            } else if es <= ns && ee >= ne {
                                true
                            } else {
                                let start = es.min(ns);
                                let end = if ee == u32::MAX || ne == u32::MAX {
                                    u32::MAX
                                } else {
                                    ee.max(ne)
                                };
                                *existing_range = Some((start, end));
                                *existing_ann = annotation_for_range(start, end);
                                true
                            }
                        }
                        (None, Some((ns, ne))) => {
                            *existing_range = Some((ns, ne));
                            *existing_ann = annot.clone().or_else(|| annotation_for_range(ns, ne));
                            true
                        }
                        (Some(_), None) => {
                            if annot.is_some() {
                                *existing_ann = annot.clone();
                            }
                            true
                        }
                        (None, None) => {
                            if annot.is_some() {
                                *existing_ann = annot.clone();
                            }
                            true
                        }
                    };

                    if reuse {
                        record.entries[idx].status = status;
                        return Some(idx);
                    }
                }
            }
        }
    }

    if let ExploreSummary::Command {
        display,
        annotation,
    } = &summary
    {
        for idx in (0..record.entries.len()).rev() {
            if let ExploreSummary::Command {
                display: existing_display,
                annotation: existing_annotation,
            } = &record.entries[idx].summary
            {
                if existing_display == display && existing_annotation == annotation {
                    record.entries[idx].status = status;
                    return Some(idx);
                }
            }
        }
    }

    if let ExploreSummary::Count {
        target,
        annotation,
    } = &summary
    {
        for idx in (0..record.entries.len()).rev() {
            if let ExploreSummary::Count {
                target: existing_target,
                annotation: existing_annotation,
            } = &record.entries[idx].summary
            {
                if existing_target == target && existing_annotation == annotation {
                    record.entries[idx].status = status;
                    return Some(idx);
                }
            }
        }
    }

    record.entries.push(ExploreEntry {
        action,
        summary,
        status,
    });
    Some(record.entries.len().saturating_sub(1))
}

pub(crate) fn explore_record_update_status(
    record: &mut ExploreRecord,
    idx: usize,
    status: ExploreEntryStatus,
) {
    if let Some(entry) = record.entries.get_mut(idx) {
        entry.status = status;
    }
}

pub(crate) fn explore_lines_from_record(record: &ExploreRecord) -> Vec<Line<'static>> {
    explore_lines_from_record_with_force(record, false)
}

pub(crate) fn explore_lines_from_record_with_force(
    record: &ExploreRecord,
    force_exploring: bool,
) -> Vec<Line<'static>> {
    explore_lines_with_truncation(record, force_exploring, true)
}

fn explore_lines_with_truncation(
    record: &ExploreRecord,
    force_exploring: bool,
    allow_truncation: bool,
) -> Vec<Line<'static>> {
    let exploring = force_exploring
        || record.entries.is_empty()
        || record
            .entries
            .iter()
            .any(|entry| matches!(entry.status, ExploreEntryStatus::Running));
    let header = if exploring { "Exploring..." } else { "Explored" };

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::styled(
        header,
        Style::default().fg(crate::colors::text()),
    ));

    if record.entries.is_empty() {
        return lines;
    }

    const MAX_VISIBLE_ENTRIES: usize = 6;
    const HEAD_ENTRIES: usize = 2;
    const TAIL_ENTRIES: usize = 4;

    let entry_count = record.entries.len();
    let (visible_indices, truncated) = if !allow_truncation || entry_count <= MAX_VISIBLE_ENTRIES {
        ((0..entry_count).collect::<Vec<_>>(), false)
    } else {
        let tail_start = entry_count.saturating_sub(TAIL_ENTRIES);
        let mut indices = Vec::with_capacity(HEAD_ENTRIES + TAIL_ENTRIES);
        indices.extend(0..HEAD_ENTRIES.min(entry_count));
        indices.extend(tail_start..entry_count);
        (indices, true)
    };

    let max_label_len = visible_indices
        .iter()
        .map(|&idx| entry_label_width(&record.entries[idx]))
        .max()
        .unwrap_or(0);

    for (display_idx, entry_idx) in visible_indices.iter().enumerate() {
        if truncated && display_idx == HEAD_ENTRIES {
            lines.push(Line::styled(
                "  ⋮",
                Style::default().add_modifier(Modifier::DIM),
            ));
        }

        let entry = &record.entries[*entry_idx];
        let prefix = if *entry_idx == 0 { "└ " } else { "  " };
        let mut spans: Vec<Span<'static>> = vec![Span::styled(
            prefix,
            Style::default().add_modifier(Modifier::DIM),
        )];
        let label = entry_label(entry);
        let padding = max_label_len.saturating_sub(label.chars().count()) + 1;
        let mut padded_label = String::with_capacity(label.len() + padding);
        padded_label.push_str(label);
        padded_label.extend(std::iter::repeat(' ').take(padding));
        spans.push(Span::styled(
            padded_label,
            Style::default().fg(crate::colors::text_dim()),
        ));
        spans.extend(entry_summary_spans(entry));
        match entry.status {
            ExploreEntryStatus::Running => spans.push(Span::styled(
                "…",
                Style::default().fg(crate::colors::text_dim()),
            )),
            ExploreEntryStatus::NotFound => spans.push(Span::styled(
                " (not found)",
                Style::default().fg(crate::colors::text_dim()),
            )),
            ExploreEntryStatus::Error { exit_code } => {
                let msg = match (entry.action, exit_code) {
                    (ExecAction::Search, Some(2)) => " (invalid pattern)".to_string(),
                    (ExecAction::Search, _) => " (search error)".to_string(),
                    (ExecAction::List, _) => " (list error)".to_string(),
                    (ExecAction::Read, _) => " (read error)".to_string(),
                    _ => exit_code
                        .map(|code| format!(" (exit {})", code))
                        .unwrap_or_else(|| " (failed)".to_string()),
                };
                spans.push(Span::styled(
                    msg,
                    Style::default().fg(crate::colors::error()),
                ));
            }
            ExploreEntryStatus::Success => {}
        }
        lines.push(Line::from(spans));
    }

    lines
}

pub(crate) fn explore_lines_without_truncation(
    record: &ExploreRecord,
    force_exploring: bool,
) -> Vec<Line<'static>> {
    explore_lines_with_truncation(record, force_exploring, false)
}



fn entry_label(entry: &ExploreEntry) -> &'static str {
    match entry.summary {
        ExploreSummary::Command { .. } => return "Ran",
        ExploreSummary::Count { .. } => return "Count",
        _ => {}
    }
    match entry.action {
        ExecAction::Read => "Read",
        ExecAction::Search => "Search",
        ExecAction::List => "List",
        ExecAction::Run => "Run",
    }
}

fn entry_label_width(entry: &ExploreEntry) -> usize {
    entry_label(entry).chars().count()
}

fn entry_summary_spans(entry: &ExploreEntry) -> Vec<Span<'static>> {
    match &entry.summary {
        ExploreSummary::Search { query, path } => {
            let mut spans = Vec::new();
            if let Some(q) = query {
                if !q.is_empty() {
                    spans.push(Span::styled(
                        q.clone(),
                        Style::default().fg(crate::colors::text()),
                    ));
                }
            }
            if let Some(p) = path {
                spans.push(Span::styled(
                    format!(" in {}", p),
                    Style::default().fg(crate::colors::text_dim()),
                ));
            }
            if spans.is_empty() {
                spans.push(Span::styled(
                    "search".to_string(),
                    Style::default().fg(crate::colors::text()),
                ));
            }
            spans
        }
        ExploreSummary::List { path } => {
            let target = path.clone().unwrap_or_else(|| "./".to_string());
            vec![Span::styled(
                target,
                Style::default().fg(crate::colors::text_dim()),
            )]
        }
        ExploreSummary::Read {
            display_path,
            annotation,
            ..
        } => {
            let mut spans = vec![Span::styled(
                display_path.clone(),
                Style::default().fg(crate::colors::text()),
            )];
            if let Some(ann) = annotation {
                spans.push(Span::styled(
                    format!(" {}", ann),
                    Style::default().fg(crate::colors::text_dim()),
                ));
            }
            spans
        }
        ExploreSummary::Command { display, annotation } => {
            let mut spans = highlight_command_summary(display);
            if let Some(annotation) = annotation {
                spans.push(Span::styled(
                    format!(" {}", annotation),
                    Style::default().fg(crate::colors::text_dim()),
                ));
            }
            spans
        }
        ExploreSummary::Fallback { text } => vec![Span::styled(
            text.clone(),
            Style::default().fg(crate::colors::text()),
        )],
        ExploreSummary::Count { target, annotation } => {
            let mut spans = Vec::new();
            if let Some(target) = target {
                spans.push(Span::styled(
                    target.clone(),
                    Style::default().fg(crate::colors::text()),
                ));
            }
            if let Some(annotation) = annotation {
                spans.push(Span::styled(
                    format!(" {}", annotation),
                    Style::default().fg(crate::colors::text_dim()),
                ));
            }
            spans
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct CommandSummary {
    display: String,
    annotation: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct CountSummary {
    target: Option<String>,
    annotation: Option<String>,
}

fn build_command_summary(cmd: &str, original_command: &[String]) -> CommandSummary {
    let trimmed = cmd.trim();
    let full_command = strip_bash_lc_and_escape(original_command);
    let annotation_command = command_string_for_annotation(original_command);

    let shlex = Shlex::new(trimmed);
    let parts: Vec<String> = shlex.collect();
    let mut display = if trimmed.is_empty() {
        full_command.clone()
    } else if parts.is_empty() {
        trimmed.to_string()
    } else {
        parts.join(" ")
    };

    let mut annotation = None;

    if let Some((head, filter)) = split_pipeline_for_filter(&annotation_command) {
        if looks_like_line_filter(&filter) {
            if let Some(ann) = super::parse_read_line_annotation(&filter) {
                annotation = Some(ann);
                display = head;
            }
        }
    }

    if annotation.is_none() && looks_like_line_filter(&annotation_command) {
        annotation = super::parse_read_line_annotation(&annotation_command);
    }

    CommandSummary { display, annotation }
}

fn build_count_summary(cmd: &str, cwd: &Path, session_root: &Path) -> Option<CountSummary> {
    let parsed = CountPipeline::parse(cmd)?;
    let annotation = parsed.line_filter.and_then(|filter| {
        super::parse_read_line_annotation(&filter)
    });

    let target = parsed
        .target
        .map(|raw| format_read_target(raw.as_str(), cwd, session_root));

    Some(CountSummary { target, annotation })
}

struct CountPipeline {
    target: Option<String>,
    line_filter: Option<String>,
}

impl CountPipeline {
    fn parse(cmd: &str) -> Option<Self> {
        let trimmed = cmd.trim();
        if trimmed.is_empty() {
            return None;
        }

        if let Some((head, tail)) = split_pipeline_for_filter(trimmed) {
            if is_wc_count(&tail) {
                let target = extract_count_target(&head);
                let line_filter = looks_like_line_filter(&head).then_some(head);
                return Some(CountPipeline { target, line_filter });
            }
        }

        if is_wc_count(trimmed) {
            let target = extract_count_target(trimmed);
            return Some(CountPipeline {
                target,
                line_filter: None,
            });
        }

        None
    }
}

fn is_wc_count(cmd: &str) -> bool {
    let tokens = tokenize_count_command(cmd);
    if tokens.is_empty() {
        return false;
    }
    tokens[0] == "wc" && tokens.iter().skip(1).any(|token| token == "-l")
}

fn tokenize_count_command(cmd: &str) -> Vec<String> {
    Shlex::new(cmd).collect()
}

fn extract_count_target(cmd: &str) -> Option<String> {
    tokenize_count_command(cmd)
        .into_iter()
        .rev()
        .find(|token| !token.starts_with('-'))
}

fn split_pipeline_for_filter(cmd: &str) -> Option<(String, String)> {
    let mut in_single = false;
    let mut in_double = false;
    let mut escape_next = false;
    let mut paren_depth = 0_i32;

    let mut last_pipe: Option<usize> = None;
    let mut iter = cmd.char_indices().peekable();
    while let Some((idx, ch)) = iter.next() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match ch {
            '\\' if !in_single => {
                escape_next = true;
            }
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            '(' if !in_single && !in_double => {
                paren_depth = paren_depth.saturating_add(1);
            }
            ')' if !in_single && !in_double => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
            }
            '|' if !in_single && !in_double => {
                if paren_depth > 0 {
                    continue;
                }
                if idx > 0 && cmd.as_bytes()[idx - 1] == b'|' {
                    continue;
                }
                if let Some(&(_, next_ch)) = iter.peek() {
                    if next_ch == '|' {
                        continue;
                    }
                }
                last_pipe = Some(idx);
            }
            '#' if !in_single && !in_double => {
                break;
            }
            _ => {}
        }
    }

    let idx = last_pipe?;
    let head = cmd[..idx].trim_end();

    let mut tail_slice = cmd[idx + 1..].trim_start();
    if tail_slice.starts_with('&') {
        tail_slice = tail_slice[1..].trim_start();
    }

    if tail_slice.is_empty() {
        return None;
    }

    Some((head.to_string(), tail_slice.to_string()))
}

fn command_string_for_annotation(original_command: &[String]) -> String {
    unwrap_shell_script(original_command)
        .unwrap_or_else(|| strip_bash_lc_and_escape(original_command))
}

fn unwrap_shell_script(command: &[String]) -> Option<String> {
    extract_shell_script(command).map(|(_, script)| script.to_string())
}

fn looks_like_line_filter(cmd: &str) -> bool {
    let mut shlex = Shlex::new(cmd);
    let mut tokens: Vec<String> = Vec::new();

    while let Some(token) = shlex.next() {
        if tokens.is_empty() && is_env_assignment(&token) {
            continue;
        }
        tokens.push(token);
    }

    if tokens.is_empty() {
        return false;
    }

    let command_name = first_filter_command(&tokens);
    let Some(command_name) = command_name else {
        return false;
    };

    match command_name.as_str() {
        "tail" => {
            let has_follow_flag = tokens.iter().skip(1).any(|token| {
                let lower = token.to_ascii_lowercase();
                lower == "-f"
                    || lower == "--follow"
                    || lower.starts_with("--follow=")
                    || lower == "-F"
            });
            if has_follow_flag {
                return false;
            }
            true
        }
        "head" | "sed" => true,
        _ => false,
    }
}

fn is_env_assignment(token: &str) -> bool {
    token.contains('=') && !token.starts_with('-')
}

fn first_filter_command(tokens: &[String]) -> Option<String> {
    let mut idx = 0;
    while idx < tokens.len() {
        let token = tokens[idx].as_str();

        if is_env_assignment(token) {
            idx += 1;
            continue;
        }

        if let Some(wrapper) = classify_wrapper(token) {
            idx = skip_wrapper_tokens(wrapper, tokens, idx + 1);
            continue;
        }

        return filter_command_name(token);
    }

    None
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum WrapperKind {
    Sudo,
    Doas,
    Env,
    Nice,
    Ionice,
    Stdbuf,
    Chronic,
}

fn classify_wrapper(token: &str) -> Option<WrapperKind> {
    let Some(name) = filter_command_name(token) else {
        return None;
    };

    match name.as_str() {
        "sudo" => Some(WrapperKind::Sudo),
        "doas" => Some(WrapperKind::Doas),
        "env" => Some(WrapperKind::Env),
        "nice" => Some(WrapperKind::Nice),
        "ionice" => Some(WrapperKind::Ionice),
        "stdbuf" => Some(WrapperKind::Stdbuf),
        "chronic" => Some(WrapperKind::Chronic),
        _ => None,
    }
}

fn skip_wrapper_tokens(wrapper: WrapperKind, tokens: &[String], mut idx: usize) -> usize {
    while idx < tokens.len() {
        let token = tokens[idx].as_str();

        if token == "--" {
            idx += 1;
            break;
        }

        if is_env_assignment(token) {
            idx += 1;
            continue;
        }

        if wrapper == WrapperKind::Env && token.contains('=') {
            idx += 1;
            continue;
        }

        if token.starts_with('-') {
            let requires_value = wrapper_option_requires_value(wrapper, token);
            idx += 1;
            if requires_value && !token.contains('=') && idx < tokens.len() {
                idx += 1;
            }
            continue;
        }

        break;
    }

    idx
}

fn wrapper_option_requires_value(wrapper: WrapperKind, option: &str) -> bool {
    let opt = option.trim_start_matches('-');
    let opt_norm = opt.trim_start_matches('-').to_ascii_lowercase();

    match wrapper {
        WrapperKind::Sudo | WrapperKind::Doas => matches!(
            opt_norm.as_str(),
            "u" | "user" | "g" | "group" | "h" | "host" | "p" | "prompt" | "c" | "close-fds" | "a" | "chdir"
        ),
        WrapperKind::Env => matches!(opt_norm.as_str(), "u" | "unset" | "s" | "split-string"),
        WrapperKind::Nice => matches!(opt_norm.as_str(), "n" | "adjustment"),
        WrapperKind::Ionice => matches!(opt_norm.as_str(), "c" | "class" | "n" | "level" | "t" | "type" | "p" | "pid"),
        WrapperKind::Stdbuf => matches!(opt_norm.as_str(), "i" | "o" | "e"),
        WrapperKind::Chronic => false,
    }
}

fn filter_command_name(token: &str) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }

    let base = Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed)
        .to_ascii_lowercase();

    if base.is_empty() {
        None
    } else {
        Some(base)
    }
}

fn normalize_separators(mut value: String) -> String {
    value = value.replace('\\', "/");
    while value.contains("//") {
        value = value.replace("//", "/");
    }
    value
}

fn ensure_dir_suffix(mut value: String) -> String {
    if value.is_empty() {
        value.push('.');
    }
    value = normalize_separators(value);
    if !value.ends_with('/') {
        value.push('/');
    }
    value
}

fn format_cwd_display(cwd: &Path, session_root: &Path) -> String {
    if let Ok(rel) = cwd.strip_prefix(session_root) {
        if rel.as_os_str().is_empty() {
            return "./".to_string();
        }
        let mut parts: Vec<String> = Vec::new();
        for comp in rel.components() {
            match comp {
                Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
                Component::ParentDir => parts.push("..".to_string()),
                Component::CurDir => {}
                _ => {}
            }
        }
        if parts.is_empty() {
            "./".to_string()
        } else {
            ensure_dir_suffix(parts.join("/"))
        }
    } else {
        ensure_dir_suffix(cwd.display().to_string())
    }
}

fn format_list_target(path: Option<&str>, cwd: &Path, session_root: &Path) -> Option<String> {
    let trimmed = path.and_then(|p| {
        let t = p.trim();
        if t.is_empty() { None } else { Some(t) }
    });

    let display = match trimmed {
        Some(".") | Some("./") => format_cwd_display(cwd, session_root),
        Some("/") => normalize_separators("/".to_string()),
        Some(raw) => {
            let stripped = raw.trim_end_matches('/');
            let base = if stripped.is_empty() { raw } else { stripped };
            ensure_dir_suffix(base.to_string())
        }
        None => format_cwd_display(cwd, session_root),
    };

    Some(display)
}

fn format_search_target(path: Option<&str>, cwd: &Path, session_root: &Path) -> Option<String> {
    let trimmed = path.and_then(|p| {
        let t = p.trim();
        if t.is_empty() { None } else { Some(t) }
    });
    trimmed.map(|p| format_read_target(p, cwd, session_root))
}

fn format_read_target(name: &str, cwd: &Path, session_root: &Path) -> String {
    let trimmed = name.trim();
    let path = Path::new(trimmed);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };

    let normalized = if let Ok(rel) = resolved.strip_prefix(session_root) {
        if rel.as_os_str().is_empty() {
            trimmed.to_string()
        } else {
            normalize_separators(rel.display().to_string())
        }
    } else {
        normalize_separators(resolved.display().to_string())
    };

    if normalized.is_empty() {
        trimmed.to_string()
    } else {
        normalized
    }
}

fn annotation_for_range(start: u32, end: u32) -> Option<String> {
    if end == u32::MAX {
        Some(format!("(from {} to end)", start))
    } else {
        Some(format!("(lines {} to {})", start, end))
    }
}

fn highlight_command_summary(command: &str) -> Vec<Span<'static>> {
    let normalized = super::normalize_shell_command_display(command);
    let display_line = super::insert_line_breaks_after_double_ampersand(&normalized);
    let highlighted = crate::syntax_highlight::highlight_code_block(&display_line, Some("bash"));
    if let Some(mut first) = highlighted.into_iter().next() {
        super::emphasize_shell_command_name(&mut first);
        first.spans
    } else {
        vec![Span::styled(
            display_line,
            Style::default().fg(crate::colors::text()),
        )]
    }
}
