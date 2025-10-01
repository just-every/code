use super::*;
use crate::history::state::{DiffHunk, DiffLine, DiffLineKind, DiffRecord, HistoryId};
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
        if raw_line.starts_with("@@") {
            let prev_lines = std::mem::take(&mut current_lines);
            flush_hunk(current_header.take(), prev_lines, &mut hunks);
            current_header = Some(raw_line.to_string());
            continue;
        }

        let (kind, content) = if raw_line.starts_with("+++") || raw_line.starts_with("---") {
            (DiffLineKind::Context, raw_line.to_string())
        } else if let Some(rest) = raw_line.strip_prefix('+') {
            (DiffLineKind::Addition, rest.to_string())
        } else if let Some(rest) = raw_line.strip_prefix('-') {
            (DiffLineKind::Removal, rest.to_string())
        } else {
            (DiffLineKind::Context, raw_line.to_string())
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
pub(crate) fn new_diff_cell_from_string(diff_output: String) -> DiffCell {
    let record = diff_record_from_string(String::new(), &diff_output);
    DiffCell::from_record(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_lines_from_record_applies_styles() {
        let record = DiffRecord {
            id: HistoryId(1),
            title: "diff --git a/file b/file".into(),
            hunks: vec![DiffHunk {
                header: "@@ -1,1 +1,1 @@".into(),
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Removal,
                        content: "old".into(),
                    },
                    DiffLine {
                        kind: DiffLineKind::Addition,
                        content: "new".into(),
                    },
                    DiffLine {
                        kind: DiffLineKind::Context,
                        content: "context".into(),
                    },
                ],
            }],
        };

        let lines = diff_lines_from_record(&record);
        assert_eq!(lines.len(), 5); // title + header + three lines
        assert_eq!(lines[0].style.fg, Some(crate::colors::primary()));
        assert_eq!(lines[2].style.fg, Some(crate::colors::error()));
        assert_eq!(lines[3].style.fg, Some(crate::colors::success()));
        assert_eq!(lines[4].style.fg, None);
    }
}
