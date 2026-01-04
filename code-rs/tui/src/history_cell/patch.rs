use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::prelude::{Buffer, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};

use crate::diff_render::create_diff_summary_with_width;
use crate::history::compat::{
    HistoryId,
    PatchEventType as HistoryPatchEventType,
    PatchRecord,
};
use code_ansi_escape::ansi_escape_line;
use code_core::protocol::FileChange;

use super::core::{HistoryCell, HistoryCellType, PatchEventType, PatchKind};
use super::formatting::{normalize_overwrite_sequences, trim_empty_lines};
use super::plain_message_state_from_lines;
use crate::history::compat::PlainMessageState;
use crate::sanitize::Mode as SanitizeMode;
use crate::sanitize::Options as SanitizeOptions;
use crate::sanitize::sanitize_for_tui;

// ==================== PatchSummaryCell ====================
// Renders patch summary + details with width-aware hanging indents so wrapped
// diff lines align under their code indentation.

pub(crate) struct PatchSummaryCell {
    pub(crate) title: String,
    pub(crate) kind: PatchKind,
    pub(crate) record: PatchRecord,
    cached_layout: RefCell<Option<PatchLayoutCache>>,
}

#[derive(Clone)]
struct PatchLayoutCache {
    width: u16,
    lines: Vec<Line<'static>>,
    height: u16,
}

impl PatchLayoutCache {
    fn new(width: u16, lines: Vec<Line<'static>>) -> Self {
        let trimmed = trim_empty_lines(lines);
        let height = Paragraph::new(Text::from(trimmed.clone()))
            .wrap(Wrap { trim: false })
            .line_count(width)
            .try_into()
            .unwrap_or(0);
        Self {
            width,
            lines: trimmed,
            height,
        }
    }
}

fn patch_changes_are_rename_only(changes: &HashMap<PathBuf, FileChange>) -> bool {
    !changes.is_empty() && changes.values().all(file_change_is_rename_only)
}

fn patch_changes_are_noop(changes: &HashMap<PathBuf, FileChange>) -> bool {
    !changes.is_empty()
        && changes.values().all(|change| match change {
            FileChange::Update {
                move_path: None,
                unified_diff,
                ..
            } => {
                !diff_contains_line_edits(unified_diff)
                    && !diff_contains_binary_markers(unified_diff)
                    && !diff_contains_metadata_markers(unified_diff)
            }
            _ => false,
        })
}

fn file_change_is_rename_only(change: &FileChange) -> bool {
    match change {
        FileChange::Update {
            move_path: Some(_),
            unified_diff,
            ..
        } => {
            !diff_contains_line_edits(unified_diff)
                && !diff_contains_binary_markers(unified_diff)
                && !diff_contains_metadata_markers_excluding_rename(unified_diff)
        }
        _ => false,
    }
}

fn diff_contains_line_edits(diff: &str) -> bool {
    for line in diff.lines() {
        if line.starts_with("+++") || line.starts_with("---") || line.starts_with("@@") {
            continue;
        }
        match line.as_bytes().first() {
            Some(b'+') | Some(b'-') => return true,
            _ => {}
        }
    }
    false
}

fn diff_contains_binary_markers(diff: &str) -> bool {
    diff.contains("Binary files") || diff.contains("GIT binary patch") || diff.as_bytes().contains(&0)
}

fn diff_contains_metadata_markers(diff: &str) -> bool {
    diff.contains("new file mode")
        || diff.contains("deleted file mode")
        || diff.contains("old mode")
        || diff.contains("new mode")
        || diff.contains("similarity index")
        || diff.contains("rename from")
        || diff.contains("rename to")
}

fn diff_contains_metadata_markers_excluding_rename(diff: &str) -> bool {
    diff.contains("new file mode")
        || diff.contains("deleted file mode")
        || diff.contains("old mode")
        || diff.contains("new mode")
}

fn patch_kind_and_title(record: &PatchRecord) -> (PatchKind, String) {
    let kind = match record.patch_type {
        HistoryPatchEventType::ApprovalRequest => PatchKind::Proposed,
        HistoryPatchEventType::ApplyBegin { .. } => PatchKind::ApplyBegin,
        HistoryPatchEventType::ApplySuccess => PatchKind::ApplySuccess,
        HistoryPatchEventType::ApplyFailure => PatchKind::ApplyFailure,
    };
    let rename_only = patch_changes_are_rename_only(&record.changes);
    let noop_only = patch_changes_are_noop(&record.changes);
    let title = match record.patch_type {
        HistoryPatchEventType::ApprovalRequest => "proposed patch".to_string(),
        HistoryPatchEventType::ApplyFailure => "Patch failed".to_string(),
        HistoryPatchEventType::ApplyBegin { .. } | HistoryPatchEventType::ApplySuccess => {
            if rename_only {
                "Renamed".to_string()
            } else if noop_only {
                "No changes".to_string()
            } else {
                "Updated".to_string()
            }
        }
    };
    (kind, title)
}

impl PatchSummaryCell {
    pub(crate) fn from_record(record: PatchRecord) -> Self {
        let (kind, title) = patch_kind_and_title(&record);
        Self {
            title,
            kind,
            record,
            cached_layout: RefCell::new(None),
        }
    }

    fn ui_event_type(&self) -> PatchEventType {
        match self.record.patch_type {
            HistoryPatchEventType::ApprovalRequest => PatchEventType::ApprovalRequest,
            HistoryPatchEventType::ApplyBegin { auto_approved } => {
                PatchEventType::ApplyBegin { auto_approved }
            }
            HistoryPatchEventType::ApplySuccess => PatchEventType::ApplySuccess,
            HistoryPatchEventType::ApplyFailure => PatchEventType::ApplyFailure,
        }
    }

    pub(crate) fn record(&self) -> &PatchRecord {
        &self.record
    }

    pub(crate) fn record_mut(&mut self) -> &mut PatchRecord {
        self.invalidate_layout_cache();
        &mut self.record
    }

    pub(crate) fn update_record(&mut self, record: PatchRecord) {
        let (kind, title) = patch_kind_and_title(&record);
        self.record = record;
        self.kind = kind;
        self.title = title;
        self.invalidate_layout_cache();
    }

    fn invalidate_layout_cache(&self) {
        self.cached_layout.borrow_mut().take();
    }

    fn layout_for_width(&self, width: u16) -> std::cell::Ref<'_, PatchLayoutCache> {
        let effective_width = width.max(1);
        let needs_rebuild = {
            let cache = self.cached_layout.borrow();
            cache
                .as_ref()
                .map(|layout| layout.width != effective_width)
                .unwrap_or(true)
        };

        if needs_rebuild {
            let lines = self.build_lines(effective_width);
            let layout = PatchLayoutCache::new(effective_width, lines);
            *self.cached_layout.borrow_mut() = Some(layout);
        }

        std::cell::Ref::map(self.cached_layout.borrow(), |cache| {
            cache.as_ref().expect("patch layout cache missing")
        })
    }

    fn build_lines(&self, width: u16) -> Vec<Line<'static>> {
        let effective_width = width.max(1);
        let mut lines: Vec<Line<'static>> = create_diff_summary_with_width(
            &self.title,
            &self.record.changes,
            self.ui_event_type(),
            Some(effective_width as usize),
        )
        .into_iter()
        .collect();

        if matches!(
            self.record.patch_type,
            HistoryPatchEventType::ApplyFailure
        ) {
            if let Some(metadata) = &self.record.failure {
                if !lines.is_empty() {
                    lines.push(Line::default());
                }
                lines.push(
                    Line::from("Patch application failed")
                        .fg(crate::colors::error())
                        .bold(),
                );
                if !metadata.message.is_empty() {
                    lines.push(Line::from(metadata.message.clone()).fg(crate::colors::error()));
                }
                if let Some(stdout) = &metadata.stdout_excerpt {
                    if !stdout.is_empty() {
                        lines.push(Line::default());
                        lines.push(Line::from("stdout excerpt:").fg(crate::colors::info()));
                        for line in stdout.lines() {
                            lines.push(Line::from(line.to_string()).fg(crate::colors::text()));
                        }
                    }
                }
                if let Some(stderr) = &metadata.stderr_excerpt {
                    if !stderr.is_empty() {
                        lines.push(Line::default());
                        lines.push(Line::from("stderr excerpt:").fg(crate::colors::error()));
                        for line in stderr.lines() {
                            lines.push(Line::from(line.to_string()).fg(crate::colors::error()));
                        }
                    }
                }
            }
        }
        lines
    }
}

impl HistoryCell for PatchSummaryCell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn kind(&self) -> HistoryCellType {
        HistoryCellType::Patch { kind: self.kind }
    }

    // We compute lines based on width at render time; provide a conservative
    // default for non-width callers (not normally used in our pipeline).
    fn display_lines(&self) -> Vec<Line<'static>> {
        self.layout_for_width(80).lines.clone()
    }

    fn has_custom_render(&self) -> bool {
        true
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.layout_for_width(width).height
    }

    fn custom_render_with_skip(&self, area: Rect, buf: &mut Buffer, skip_rows: u16) {
        // Render with trimmed lines and pre-clear the area to avoid residual glyphs
        // when content shrinks (e.g., after width changes or trimming).
        let layout = self.layout_for_width(area.width);
        let text = Text::from(layout.lines.clone());

        let cell_bg = crate::colors::background();
        let bg_block = Block::default().style(Style::default().bg(cell_bg));

        // Proactively fill the full draw area with the background.
        // This mirrors other cells that ensure a clean slate before drawing.
        crate::util::buffer::fill_rect(
            buf,
            area,
            Some(' '),
            Style::default().bg(cell_bg).fg(crate::colors::text()),
        );

        Paragraph::new(text)
            .block(bg_block)
            .wrap(Wrap { trim: false })
            .scroll((skip_rows, 0))
            .style(Style::default().bg(cell_bg))
            .render(area, buf);
    }
}

pub(crate) fn new_patch_event(
    event_type: PatchEventType,
    changes: HashMap<PathBuf, FileChange>,
) -> PatchSummaryCell {
    let record = PatchRecord {
        id: HistoryId::ZERO,
        patch_type: match event_type {
            PatchEventType::ApprovalRequest => HistoryPatchEventType::ApprovalRequest,
            PatchEventType::ApplyBegin { auto_approved } => {
                HistoryPatchEventType::ApplyBegin { auto_approved }
            }
            PatchEventType::ApplySuccess => HistoryPatchEventType::ApplySuccess,
            PatchEventType::ApplyFailure => HistoryPatchEventType::ApplyFailure,
        },
        changes,
        failure: None,
    };
    PatchSummaryCell::from_record(record)
}

pub(crate) fn new_patch_apply_failure(stderr: String) -> PlainMessageState {
    let mut lines: Vec<Line<'static>> = vec![
        Line::from("❌ Patch application failed")
            .fg(crate::colors::error())
            .bold(),
        Line::from(""),
    ];

    let norm = normalize_overwrite_sequences(&stderr);
    let norm = sanitize_for_tui(
        &norm,
        SanitizeMode::AnsiPreserving,
        SanitizeOptions {
            expand_tabs: true,
            tabstop: 4,
            debug_markers: false,
        },
    );
    for line in norm.lines() {
        if !line.is_empty() {
            lines.push(ansi_escape_line(line).fg(crate::colors::error()));
        }
    }

    lines.push(Line::from(""));
    plain_message_state_from_lines(
        lines,
        HistoryCellType::Patch {
            kind: PatchKind::ApplyFailure,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_core::history::state::{HistoryId, PatchEventType, PatchRecord};
    use code_core::protocol::FileChange;
    use std::collections::HashMap;
    use std::path::PathBuf;

    #[test]
    fn patch_summary_caches_layout_by_width() {
        let mut changes = HashMap::new();
        changes.insert(
            PathBuf::from("index.html"),
            FileChange::Update {
                unified_diff: "@@ -1 +1 @@\n-hello\n+world\n".to_string(),
                move_path: None,
                original_content: "hello".to_string(),
                new_content: "world".to_string(),
            },
        );
        let record = PatchRecord {
            id: HistoryId::ZERO,
            patch_type: PatchEventType::ApplySuccess,
            changes,
            failure: None,
        };

        let cell = PatchSummaryCell::from_record(record);
        assert!(cell.cached_layout.borrow().is_none());

        let _ = cell.desired_height(80);
        assert_eq!(
            cell.cached_layout
                .borrow()
                .as_ref()
                .map(|layout| layout.width),
            Some(80)
        );

        let _ = cell.desired_height(80);
        assert_eq!(
            cell.cached_layout
                .borrow()
                .as_ref()
                .map(|layout| layout.width),
            Some(80)
        );

        let _ = cell.desired_height(120);
        assert_eq!(
            cell.cached_layout
                .borrow()
                .as_ref()
                .map(|layout| layout.width),
            Some(120)
        );
    }
}
