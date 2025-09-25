use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExploreEntryStatus {
    Running,
    Success,
    NotFound,
    Error { exit_code: Option<i32> },
}

#[derive(Clone, Debug, PartialEq)]
struct CommandSummary {
    display: String,
    annotation: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum ExploreSummary {
    Search {
        query: Option<String>,
        path: Option<String>,
    },
    List {
        path: Option<String>,
    },
    Read {
        display_path: String,
        annotation: Option<String>,
        range: Option<(u32, u32)>,
    },
    Command {
        command: CommandSummary,
    },
    Fallback {
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExploreEntry {
    action: ExecAction,
    summary: ExploreSummary,
    status: ExploreEntryStatus,
}

impl ExploreEntry {
    fn label(&self) -> &'static str {
        if matches!(self.summary, ExploreSummary::Command { .. }) {
            return "Ran";
        }
        match self.action {
            ExecAction::Read => "Read",
            ExecAction::Search => "Search",
            ExecAction::List => "List",
            ExecAction::Run => "Run",
        }
    }

    fn summary_spans(&self) -> Vec<Span<'static>> {
        match &self.summary {
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
                let target = path.as_ref().cloned().unwrap_or_else(|| "./".to_string());
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
            ExploreSummary::Command { command } => {
                let mut spans = highlight_command_summary(&command.display);
                if let Some(annotation) = &command.annotation {
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
        }
    }

    fn is_running(&self) -> bool {
        matches!(self.status, ExploreEntryStatus::Running)
    }

    fn is_error(&self) -> bool {
        matches!(self.status, ExploreEntryStatus::Error { .. })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ExploreAggregationState {
    pub entries: Vec<ExploreEntry>,
    pub is_trailing: bool,
}

pub(crate) struct ExploreAggregationCell {
    state: ExploreAggregationState,
}

impl ExploreAggregationCell {
    pub(crate) fn new() -> Self {
        Self {
            state: ExploreAggregationState {
                entries: Vec::new(),
                is_trailing: true,
            },
        }
    }

    #[allow(dead_code)]
    pub(crate) fn from_state(state: ExploreAggregationState) -> Self {
        Self { state }
    }

    #[allow(dead_code)]
    pub(crate) fn state(&self) -> ExploreAggregationState {
        self.state.clone()
    }

    #[allow(dead_code)]
    pub(crate) fn set_state(&mut self, state: ExploreAggregationState) {
        self.state = state;
    }

    pub(crate) fn set_trailing(&mut self, trailing: bool) {
        self.state.is_trailing = trailing;
    }

    pub(crate) fn push_from_parsed(
        &mut self,
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
                    let formatted_path =
                        format_search_target(path.as_deref(), cwd, session_root);
                    let pretty_query =
                        query.clone().filter(|q| !q.trim().is_empty()).or_else(|| {
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
                    let (annotation, range) =
                        super::parse_read_line_annotation_with_range(cmd);
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
                ParsedCommand::ReadCommand { cmd } => Some(ExploreSummary::Command {
                    command: build_command_summary(cmd, original_command),
                }),
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
        });

        let summary = summary?;

        if let ExploreSummary::Read {
            display_path,
            annotation,
            range,
        } = &summary
        {
            let path_key = display_path.clone();
            let annot = annotation.clone();
            let range_val = *range;
            for idx in (0..self.state.entries.len()).rev() {
                if let ExploreSummary::Read {
                    display_path: existing_path,
                    annotation: existing_ann,
                    range: existing_range,
                } = &mut self.state.entries[idx].summary
                {
                    if *existing_path == path_key {
                        let reuse = match (*existing_range, range_val) {
                            (Some((es, ee)), Some((ns, ne))) => {
                                if ns <= es && ne >= ee {
                                    *existing_range = Some((ns, ne));
                                    *existing_ann =
                                        annot.clone().or_else(|| annotation_for_range(ns, ne));
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
                                *existing_ann =
                                    annot.clone().or_else(|| annotation_for_range(ns, ne));
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
                            self.state.entries[idx].status = status;
                            return Some(idx);
                        }
                    }
                }
            }
        }

        if let ExploreSummary::Command { command: new_cmd } = &summary {
            for idx in (0..self.state.entries.len()).rev() {
                if let ExploreSummary::Command { command: existing } =
                    &mut self.state.entries[idx].summary
                {
                    if existing.display == new_cmd.display
                        && existing.annotation == new_cmd.annotation
                    {
                        self.state.entries[idx].status = status;
                        return Some(idx);
                    }
                }
            }
        }

        self.state.entries.push(ExploreEntry {
            action,
            summary,
            status,
        });
        Some(self.state.entries.len().saturating_sub(1))
    }

    pub(crate) fn update_status(&mut self, idx: usize, status: ExploreEntryStatus) {
        if let Some(entry) = self.state.entries.get_mut(idx) {
            entry.status = status;
        }
    }

    fn current_exec_status(&self) -> ExecStatus {
        if self.state.entries.iter().any(|e| e.is_running()) {
            ExecStatus::Running
        } else if self.state.entries.iter().any(|e| e.is_error()) {
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
        let header = if self.state.is_trailing {
            "Exploring..."
        } else {
            "Explored"
        };

        if self.state.entries.is_empty() {
            return vec![Line::styled(
                header,
                Style::default().fg(crate::colors::text()),
            )];
        }

        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::styled(
            header,
            Style::default().fg(crate::colors::text()),
        ));

        let max_label_len = self
            .state
            .entries
            .iter()
            .map(|e| e.label().chars().count())
            .max()
            .unwrap_or(0);

        for (idx, entry) in self.state.entries.iter().enumerate() {
            let prefix = if idx == 0 { "└ " } else { "  " };
            let mut spans: Vec<Span<'static>> = vec![Span::styled(
                prefix,
                Style::default().add_modifier(Modifier::DIM),
            )];
            let label = entry.label();
            let label_len = label.chars().count();
            let padding = max_label_len.saturating_sub(label_len) + 1;
            let mut padded_label = String::with_capacity(label.len() + padding);
            padded_label.push_str(label);
            padded_label.extend(std::iter::repeat(' ').take(padding));
            spans.push(Span::styled(
                padded_label,
                Style::default().fg(crate::colors::text_dim()),
            ));
            spans.extend(entry.summary_spans());
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

fn build_command_summary(cmd: &str, original_command: &[String]) -> CommandSummary {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return CommandSummary {
            display: original_command.join(" "),
            annotation: None,
        };
    }

    let shlex = Shlex::new(trimmed);
    let parts: Vec<String> = shlex.collect();

    if parts.is_empty() {
        return CommandSummary {
            display: trimmed.to_string(),
            annotation: None,
        };
    }

    let display = parts.join(" ");
    let annotation = if parts.len() > 1 {
        Some(format!("({})", parts[1..].join(" ")))
    } else {
        None
    };

    CommandSummary { display, annotation }
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
