use super::*;
use crate::history::state::{DiffHunk, DiffLine, DiffLineKind, DiffRecord, HistoryId};
use crate::sanitize::{sanitize_for_tui, Mode as SanitizeMode, Options as SanitizeOptions};
pub(crate) struct DiffCell {
    record: DiffRecord,
}

impl DiffCell {
    pub(crate) fn from_record(record: DiffRecord) -> Self {
        Self { record }
    }

    pub(crate) fn record(&self) -> &DiffRecord {
        &self.record
    }

    pub(crate) fn record_mut(&mut self) -> &mut DiffRecord {
        &mut self.record
    }

    pub(crate) fn rebuild_with_theme(&self) {}
}

impl HistoryCell for DiffCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Diff
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        diff_lines_from_record(&self.record)
    }
}

pub(crate) fn diff_lines_from_record(record: &DiffRecord) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    if !record.title.is_empty() {
        lines.push(Line::from(record.title.clone()).fg(crate::colors::primary()));
    }

    for hunk in &record.hunks {
        if !hunk.header.is_empty() {
            lines.push(Line::from(hunk.header.clone()).fg(crate::colors::primary()));
        }

        for diff_line in &hunk.lines {
            let prefix = match diff_line.kind {
                DiffLineKind::Addition => '+',
                DiffLineKind::Removal => '-',
                DiffLineKind::Context => ' ',
            };
            let content = format!("{}{}", prefix, diff_line.content);
            let styled = match diff_line.kind {
                DiffLineKind::Addition => {
                    Line::from(content).fg(crate::colors::success())
                }
                DiffLineKind::Removal => {
                    Line::from(content).fg(crate::colors::error())
                }
                DiffLineKind::Context => Line::from(content),
            };
            lines.push(styled);
        }
    }

    lines
}

pub(crate) fn diff_record_from_string(title: String, diff: &str) -> DiffRecord {
    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut current_header: Option<String> = None;
    let mut current_lines: Vec<DiffLine> = Vec::new();

    let flush_hunk = |header: Option<String>, lines: Vec<DiffLine>, hunks: &mut Vec<DiffHunk>| {
        if let Some(header) = header {
            hunks.push(DiffHunk { header, lines });
        } else if !lines.is_empty() {
            hunks.push(DiffHunk {
                header: String::new(),
                lines,
            });
        }
    };

    for raw_line in diff.lines() {
        let line = sanitize_for_tui(
            raw_line,
            SanitizeMode::Plain,
            SanitizeOptions {
                expand_tabs: true,
                tabstop: 4,
                debug_markers: false,
            },
        );
        if line.starts_with("@@") {
            let prev_lines = std::mem::take(&mut current_lines);
            flush_hunk(current_header.take(), prev_lines, &mut hunks);
            current_header = Some(line);
            continue;
        }

        let (kind, content) = if line.starts_with("+++") || line.starts_with("---") {
            (DiffLineKind::Context, line)
        } else if let Some(rest) = line.strip_prefix('+') {
            (DiffLineKind::Addition, rest.to_string())
        } else if let Some(rest) = line.strip_prefix('-') {
            (DiffLineKind::Removal, rest.to_string())
        } else if let Some(rest) = line.strip_prefix(' ') {
            (DiffLineKind::Context, rest.to_string())
        } else {
            (DiffLineKind::Context, line)
        };
        current_lines.push(DiffLine { kind, content });
    }

    flush_hunk(current_header.take(), current_lines, &mut hunks);

    DiffRecord {
        id: HistoryId::ZERO,
        title,
        hunks,
    }
}

#[allow(dead_code)]
pub(crate) fn new_diff_output(diff_output: String) -> DiffCell {
    new_diff_cell_from_string(diff_output)
}

#[allow(dead_code)]
pub(crate) fn new_diff_cell_from_string(diff_output: String) -> DiffCell {
    let record = diff_record_from_string(String::new(), &diff_output);
    DiffCell::from_record(record)
}

#[cfg(test)]
mod tests {
    use super::diff_record_from_string;

    #[test]
    fn diff_record_strips_ansi_sequences() {
        let diff = concat!(
            "@@ -1 +1 @@\n",
            "-\u{001B}[31mold\u{001B}[0m\n",
            "+\u{001B}[32mnew\u{001B}[0m\n",
        );
        let record = diff_record_from_string(String::new(), diff);
        assert_eq!(record.hunks.len(), 1);
        let lines = &record.hunks[0].lines;
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].content, "old");
        assert_eq!(lines[1].content, "new");
        assert!(!lines[0].content.contains('\u{001B}'));
        assert!(!lines[1].content.contains('\u{001B}'));
    }

    #[test]
    fn diff_record_strips_context_prefix_space() {
        let diff = concat!(
            "@@ -1,3 +1,3 @@\n",
            " unchanged\n",
            "-old\n",
            "+new\n",
        );
        let record = diff_record_from_string(String::new(), diff);
        assert_eq!(record.hunks.len(), 1);
        let lines = &record.hunks[0].lines;
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].content, "unchanged");
        assert_eq!(lines[1].content, "old");
        assert_eq!(lines[2].content, "new");
    }
}
