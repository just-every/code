use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ratatui::buffer::Cell as BufferCell;
use ratatui::text::Line;

use crate::history::state::{HistoryId, HistoryRecord, HistoryState};
use crate::history_cell::{
    assistant_markdown_lines,
    compute_assistant_layout,
    explore_lines_from_record,
    diff_lines_from_record,
    exec_display_lines_from_record,
    merged_exec_lines_from_record,
    stream_lines_from_state,
    AssistantLayoutCache,
    AssistantMarkdownCell,
    HistoryCell,
};
use codex_core::config::Config;
use crate::insert_history::word_wrap_lines;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Memoized layout data for history rendering.
pub(crate) struct HistoryRenderState {
    pub(crate) layout_cache: RefCell<HashMap<CacheKey, Rc<CachedLayout>>>,
    pub(crate) height_cache_last_width: Cell<u16>,
    pub(crate) prefix_sums: RefCell<Vec<u16>>,
    pub(crate) last_prefix_width: Cell<u16>,
    pub(crate) last_prefix_count: Cell<usize>,
    pub(crate) prefix_valid: Cell<bool>,
}

impl HistoryRenderState {
    pub(crate) fn new() -> Self {
        Self {
            layout_cache: RefCell::new(HashMap::new()),
            height_cache_last_width: Cell::new(0),
            prefix_sums: RefCell::new(Vec::new()),
            last_prefix_width: Cell::new(0),
            last_prefix_count: Cell::new(0),
            prefix_valid: Cell::new(false),
        }
    }

    pub(crate) fn invalidate_height_cache(&self) {
        self.layout_cache.borrow_mut().clear();
        self.prefix_sums.borrow_mut().clear();
        self.prefix_valid.set(false);
    }

    pub(crate) fn handle_width_change(&self, width: u16) {
        if self.height_cache_last_width.get() != width {
            self.layout_cache
                .borrow_mut()
                .retain(|key, _| key.width == width);
            self.prefix_sums.borrow_mut().clear();
            self.prefix_valid.set(false);
            self.height_cache_last_width.set(width);
        }
    }

    pub(crate) fn invalidate_history_id(&self, id: HistoryId) {
        if id == HistoryId::ZERO {
            return;
        }
        self.layout_cache
            .borrow_mut()
            .retain(|key, _| key.history_id != id);
    }

    pub(crate) fn invalidate_all(&self) {
        self.layout_cache.borrow_mut().clear();
        self.prefix_sums.borrow_mut().clear();
        self.prefix_valid.set(false);
    }

    pub(crate) fn visible_cells<'a>(
        &self,
        history_state: &HistoryState,
        requests: &[RenderRequest<'a>],
        settings: RenderSettings,
    ) -> Vec<VisibleCell<'a>> {
        requests
            .iter()
            .map(|req| {
                let assistant_plan = match req.kind {
                    RenderRequestKind::Assistant { id } => history_state
                        .record(id)
                        .and_then(|record| match record {
                            HistoryRecord::AssistantMessage(state) => Some(
                                compute_assistant_layout(state, req.config, settings.width),
                            ),
                            _ => None,
                        }),
                    _ => req.assistant.map(|asst| asst.ensure_layout(settings.width)),
                };

                let layout = if settings.width == 0 {
                    None
                } else if req.use_cache && req.history_id != HistoryId::ZERO {
                    Some(self.render_cached(req.history_id, settings, || {
                        req.build_lines(history_state)
                    }))
                } else {
                    Some(self.render_adhoc(settings.width, || {
                        req.build_lines(history_state)
                    }))
                };

                VisibleCell {
                    cell: req.cell,
                    assistant_plan,
                    layout,
                }
            })
            .collect()
    }

    fn render_cached<F>(&self, history_id: HistoryId, settings: RenderSettings, build_lines: F) -> LayoutRef
    where
        F: FnOnce() -> Vec<Line<'static>>,
    {
        if settings.width == 0 {
            return LayoutRef::empty();
        }

        let key = CacheKey::new(history_id, settings);
        if let Some(layout) = self.layout_cache.borrow().get(&key).cloned() {
            return LayoutRef { data: layout };
        }

        let layout = Rc::new(build_cached_layout(build_lines(), settings.width));
        self.layout_cache
            .borrow_mut()
            .insert(key, Rc::clone(&layout));
        LayoutRef { data: layout }
    }

    fn render_adhoc<F>(&self, width: u16, build_lines: F) -> LayoutRef
    where
        F: FnOnce() -> Vec<Line<'static>>,
    {
        if width == 0 {
            return LayoutRef::empty();
        }
        LayoutRef {
            data: Rc::new(build_cached_layout(build_lines(), width)),
        }
    }
}

#[derive(Clone)]
pub(crate) struct LayoutRef {
    pub(crate) data: Rc<CachedLayout>,
}

impl LayoutRef {
    fn empty() -> Self {
        LayoutRef {
            data: Rc::new(CachedLayout {
                lines: Vec::new(),
                rows: Vec::new(),
            }),
        }
    }

    pub(crate) fn layout(&self) -> Rc<CachedLayout> {
        Rc::clone(&self.data)
    }

    pub(crate) fn line_count(&self) -> usize {
        self.data.lines.len()
    }
}

impl Default for HistoryRenderState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub(crate) struct CachedLayout {
    pub(crate) lines: Vec<Line<'static>>,
    pub(crate) rows: Vec<Box<[BufferCell]>>,
}

fn build_cached_layout(lines: Vec<Line<'static>>, width: u16) -> CachedLayout {
    let wrapped = if lines.is_empty() {
        Vec::new()
    } else {
        word_wrap_lines(&lines, width)
    };
    let rows = build_cached_rows(&wrapped, width);
    CachedLayout { lines: wrapped, rows }
}

fn build_cached_rows(lines: &[Line<'static>], width: u16) -> Vec<Box<[BufferCell]>> {
    let target_width = width as usize;
    lines
        .iter()
        .map(|line| build_cached_row(line, target_width))
        .collect()
}

fn build_cached_row(line: &Line<'static>, target_width: usize) -> Box<[BufferCell]> {
    if target_width == 0 {
        return Box::new([]);
    }

    let mut cells = vec![BufferCell::default(); target_width];
    let mut x: u16 = 0;
    let mut remaining = target_width as u16;

    for span in &line.spans {
        if remaining == 0 {
            break;
        }
        let span_style = line.style.patch(span.style);
        for symbol in UnicodeSegmentation::graphemes(span.content.as_ref(), true) {
            if symbol.chars().any(|ch| ch.is_control()) {
                continue;
            }
            let symbol_width = UnicodeWidthStr::width(symbol) as u16;
            if symbol_width == 0 {
                continue;
            }
            if symbol_width > remaining {
                remaining = 0;
                break;
            }

            let idx = x as usize;
            if idx >= target_width {
                remaining = 0;
                break;
            }

            cells[idx].set_symbol(symbol).set_style(span_style);

            let next_symbol = x.saturating_add(symbol_width);
            x = x.saturating_add(1);
            while x < next_symbol {
                let fill_idx = x as usize;
                if fill_idx >= target_width {
                    remaining = 0;
                    break;
                }
                cells[fill_idx].reset();
                x = x.saturating_add(1);
            }
            if remaining == 0 {
                break;
            }
            if x >= target_width as u16 {
                remaining = 0;
                break;
            }
            remaining = target_width as u16 - x;
            if remaining == 0 {
                break;
            }
        }
        if remaining == 0 {
            break;
        }
    }

    cells.into_boxed_slice()
}

/// Settings that affect layout caching. Any change to these fields invalidates
/// the cached `CachedLayout` entries keyed by `(HistoryId, width, theme_epoch,
/// reasoning_visible)`.
#[derive(Clone, Copy)]
pub(crate) struct RenderSettings {
    pub width: u16,
    pub theme_epoch: u64,
    pub reasoning_visible: bool,
}

impl RenderSettings {
    pub fn new(width: u16, theme_epoch: u64, reasoning_visible: bool) -> Self {
        Self {
            width,
            theme_epoch,
            reasoning_visible,
        }
    }
}

/// A rendering input assembled by `ChatWidget::draw_history` for a single
/// history record. We keep both the legacy `HistoryCell` (if one exists) and a
/// semantic fallback so the renderer can rebuild layouts directly from
/// `HistoryRecord` data when needed.
pub(crate) struct RenderRequest<'a> {
    pub history_id: HistoryId,
    pub cell: Option<&'a dyn HistoryCell>,
    pub assistant: Option<&'a AssistantMarkdownCell>,
    pub use_cache: bool,
    pub fallback_lines: Option<Vec<Line<'static>>>,
    pub kind: RenderRequestKind,
    pub config: &'a Config,
}

impl<'a> RenderRequest<'a> {
    /// Returns the best-effort lines for this record. We prefer the existing
    /// `HistoryCell` cache (which may include per-cell layout bridges) and fall
    /// back to semantic lines derived from the record state.
    fn build_lines(&self, history_state: &HistoryState) -> Vec<Line<'static>> {
        if let RenderRequestKind::Exec { id } = self.kind {
            if let Some(HistoryRecord::Exec(record)) = history_state.record(id) {
                return exec_display_lines_from_record(record);
            }
        }

        if let RenderRequestKind::MergedExec { id } = self.kind {
            if let Some(HistoryRecord::MergedExec(record)) = history_state.record(id) {
                return merged_exec_lines_from_record(record);
            }
        }

        if let RenderRequestKind::Explore { id } = self.kind {
            if let Some(HistoryRecord::Explore(record)) = history_state.record(id) {
                return explore_lines_from_record(record);
            }
        }

        if let RenderRequestKind::Diff { id } = self.kind {
            if let Some(HistoryRecord::Diff(record)) = history_state.record(id) {
                return diff_lines_from_record(record);
            }
        }

        if let RenderRequestKind::Streaming { id } = self.kind {
            if let Some(HistoryRecord::AssistantStream(record)) = history_state.record(id) {
                return stream_lines_from_state(record, self.config, record.in_progress);
            }
        }

        if let RenderRequestKind::Assistant { id } = self.kind {
            if let Some(HistoryRecord::AssistantMessage(record)) = history_state.record(id) {
                return assistant_markdown_lines(record, self.config);
            }
        }

        if let Some(cell) = self.cell {
            return cell.display_lines_trimmed();
        }

        if let Some(lines) = &self.fallback_lines {
            return lines.clone();
        }
        Vec::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Identifies the source for `RenderRequest` line construction.
/// Exec variants always rebuild lines from `HistoryState`, ensuring the
/// shared renderer cache is the single source of truth for layout data.
pub(crate) enum RenderRequestKind {
    Legacy,
    Exec { id: HistoryId },
    MergedExec { id: HistoryId },
    Explore { id: HistoryId },
    Diff { id: HistoryId },
    Streaming { id: HistoryId },
    Assistant { id: HistoryId },
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_core::config::{Config, ConfigOverrides, ConfigToml};
    use crate::history::state::{
        AssistantMessageState,
        AssistantStreamDelta,
        ExecAction,
        ExecStatus,
        ExecStreamChunk,
        ExploreEntry,
        ExploreEntryStatus,
        ExploreRecord,
        ExploreSummary,
        HistoryDomainEvent,
        HistoryDomainRecord,
        HistoryMutation,
        HistoryRecord,
        PlainMessageKind,
        PlainMessageRole,
        PlainMessageState,
        HistoryState,
    };
    use std::time::{Duration, SystemTime};

    fn collect_lines(layout: &CachedLayout) -> Vec<String> {
        layout
            .lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    fn start_exec_record(state: &mut HistoryState) -> HistoryId {
        match state.apply_domain_event(HistoryDomainEvent::StartExec {
            index: state.records.len(),
            call_id: Some("call-1".into()),
            command: vec!["echo".into(), "hello".into()],
            parsed: Vec::new(),
            action: ExecAction::Run,
            started_at: SystemTime::UNIX_EPOCH,
            working_dir: None,
            env: Vec::new(),
            tags: Vec::new(),
        }) {
            HistoryMutation::Inserted { id, .. } => id,
            _ => panic!("unexpected mutation inserting exec record"),
        }
    }

    fn upsert_stream_record(state: &mut HistoryState, markdown: &str) -> HistoryId {
        match state.apply_domain_event(HistoryDomainEvent::UpsertAssistantStream {
            stream_id: "stream-1".into(),
            preview_markdown: markdown.into(),
            delta: None,
            metadata: None,
        }) {
            HistoryMutation::Inserted { id, .. } => id,
            _ => panic!("unexpected mutation inserting stream record"),
        }
    }

    fn insert_explore_record(state: &mut HistoryState) -> HistoryId {
        let record = ExploreRecord {
            id: HistoryId::ZERO,
            entries: vec![ExploreEntry {
                action: ExecAction::Search,
                summary: ExploreSummary::Search {
                    query: Some("pattern".into()),
                    path: Some("src".into()),
                },
                status: ExploreEntryStatus::Success,
            }],
        };

        match state.apply_domain_event(HistoryDomainEvent::Insert {
            index: state.records.len(),
            record: HistoryDomainRecord::Explore(record),
        }) {
            HistoryMutation::Inserted { id, .. } => id,
            other => panic!("unexpected mutation inserting explore record: {other:?}"),
        }
    }

    fn test_config() -> Config {
        Config::load_from_base_config_with_overrides(
            ConfigToml::default(),
            ConfigOverrides::default(),
            std::env::temp_dir(),
        )
        .expect("cfg")
    }

    #[test]
    fn visible_cells_uses_exec_state_for_running_records() {
        let mut state = HistoryState::new();
        let exec_id = start_exec_record(&mut state);
        let cfg = test_config();

        let render_state = HistoryRenderState::new();
        let settings = RenderSettings::new(80, 0, false);
        let request = RenderRequest {
            history_id: exec_id,
            cell: None,
            assistant: None,
            use_cache: false,
            fallback_lines: None,
            kind: RenderRequestKind::Exec { id: exec_id },
            config: &cfg,
        };
        let cells = render_state.visible_cells(&state, &[request], settings);
        let layout = cells
            .first()
            .and_then(|cell| cell.layout.as_ref())
            .expect("exec layout missing")
            .layout();
        let text = collect_lines(&layout);
        assert!(text.iter().any(|line| line.contains("echo") && line.contains("hello")));
    }

    #[test]
    fn visible_cells_uses_exec_state_for_completed_records() {
        let mut state = HistoryState::new();
        let exec_id = start_exec_record(&mut state);
        let _ = state.apply_domain_event(HistoryDomainEvent::FinishExec {
            id: Some(exec_id),
            call_id: None,
            status: ExecStatus::Success,
            exit_code: Some(0),
            completed_at: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1)),
            wait_total: None,
            wait_active: false,
            wait_notes: Vec::new(),
            stdout_tail: Some("done".into()),
            stderr_tail: None,
        });

        let cfg = test_config();
        let render_state = HistoryRenderState::new();
        let settings = RenderSettings::new(80, 0, false);
        let request = RenderRequest {
            history_id: exec_id,
            cell: None,
            assistant: None,
            use_cache: false,
            fallback_lines: None,
            kind: RenderRequestKind::Exec { id: exec_id },
            config: &cfg,
        };
        let cells = render_state.visible_cells(&state, &[request], settings);
        let layout = cells
            .first()
            .and_then(|cell| cell.layout.as_ref())
            .expect("exec layout missing")
            .layout();
        let text = collect_lines(&layout);
        assert!(text
            .iter()
            .any(|line| line.contains("exit code 0") || line.contains("Success")));
    }

    #[test]
    fn visible_cells_uses_assistant_state_for_messages() {
        let mut state = HistoryState::new();
        let message_state = AssistantMessageState {
            id: HistoryId::ZERO,
            stream_id: None,
            markdown: "Hello **world**".into(),
            citations: Vec::new(),
            metadata: None,
            token_usage: None,
            created_at: SystemTime::UNIX_EPOCH,
        };
        let message_id = state.push(HistoryRecord::AssistantMessage(message_state));
        let cfg = test_config();

        let render_state = HistoryRenderState::new();
        let settings = RenderSettings::new(80, 0, false);
        let request = RenderRequest {
            history_id: message_id,
            cell: None,
            assistant: None,
            use_cache: true,
            fallback_lines: None,
            kind: RenderRequestKind::Assistant { id: message_id },
            config: &cfg,
        };
        let cells = render_state.visible_cells(&state, &[request], settings);
        let layout = cells
            .first()
            .and_then(|cell| cell.layout.as_ref())
            .expect("assistant layout missing")
            .layout();
        let text = collect_lines(&layout);
        assert!(text.iter().any(|line| line.contains("Hello")));
        assert!(text.iter().any(|line| line.contains("world")));
    }

    #[test]
    fn assistant_layout_includes_code_block_structure() {
        let mut state = HistoryState::new();
        let message_state = AssistantMessageState {
            id: HistoryId::ZERO,
            stream_id: None,
            markdown: "```bash\necho hi\n```".into(),
            citations: Vec::new(),
            metadata: None,
            token_usage: None,
            created_at: SystemTime::UNIX_EPOCH,
        };
        let message_id = state.push(HistoryRecord::AssistantMessage(message_state));
        let cfg = test_config();

        let render_state = HistoryRenderState::new();
        let settings = RenderSettings::new(40, 0, false);
        let request = RenderRequest {
            history_id: message_id,
            cell: None,
            assistant: None,
            use_cache: false,
            fallback_lines: None,
            kind: RenderRequestKind::Assistant { id: message_id },
            config: &cfg,
        };
        let cells = render_state.visible_cells(&state, &[request], settings);
        let layout = cells
            .first()
            .and_then(|cell| cell.layout.as_ref())
            .expect("assistant layout missing")
            .layout();
        let text = collect_lines(&layout);
        assert!(text.iter().any(|line| line.contains("echo hi")));
        assert!(text.iter().any(|line| line.contains("─")));
    }

    #[test]
    fn visible_cells_streaming_uses_history_state_lines() {
        let mut state = HistoryState::new();
        let stream_id = upsert_stream_record(&mut state, "partial answer");
        let cfg = test_config();

        let render_state = HistoryRenderState::new();
        let settings = RenderSettings::new(80, 0, false);
        let request = RenderRequest {
            history_id: stream_id,
            cell: None,
            assistant: None,
            use_cache: false,
            fallback_lines: None,
            kind: RenderRequestKind::Streaming { id: stream_id },
            config: &cfg,
        };
        let cells = render_state.visible_cells(&state, &[request], settings);
        let layout = cells
            .first()
            .and_then(|cell| cell.layout.as_ref())
            .expect("stream layout missing")
            .layout();
        let text = collect_lines(&layout);
        assert!(text.iter().any(|line| line.contains("partial answer")));
    }

    #[test]
    fn streaming_in_progress_appends_ellipsis_frame() {
        let mut state = HistoryState::new();
        let stream_id = upsert_stream_record(&mut state, "thinking");
        let cfg = test_config();

        let render_state = HistoryRenderState::new();
        let settings = RenderSettings::new(60, 0, false);
        let request = RenderRequest {
            history_id: stream_id,
            cell: None,
            assistant: None,
            use_cache: false,
            fallback_lines: None,
            kind: RenderRequestKind::Streaming { id: stream_id },
            config: &cfg,
        };
        let cells = render_state.visible_cells(&state, &[request], settings);
        let layout = cells
            .first()
            .and_then(|cell| cell.layout.as_ref())
            .expect("stream layout missing")
            .layout();
        let text = collect_lines(&layout);
        let frames = ["...", "·..", ".·.", "..·"];
        assert!(text
            .last()
            .map(|line| frames.iter().any(|frame| line.contains(frame)))
            .unwrap_or(false));

        // Mark stream as completed and ensure ellipsis disappears
        if let Some(HistoryRecord::AssistantStream(stream)) = state.record_mut(stream_id) {
            stream.in_progress = false;
        }
        render_state.invalidate_history_id(stream_id);
        let request = RenderRequest {
            history_id: stream_id,
            cell: None,
            assistant: None,
            use_cache: true,
            fallback_lines: None,
            kind: RenderRequestKind::Streaming { id: stream_id },
            config: &cfg,
        };
        let cells = render_state.visible_cells(&state, &[request], settings);
        let layout = cells
            .first()
            .and_then(|cell| cell.layout.as_ref())
            .expect("stream layout missing")
            .layout();
        let text = collect_lines(&layout);
        assert!(!text
            .last()
            .map(|line| frames.iter().any(|frame| line.contains(frame)))
            .unwrap_or(false));
    }

    #[test]
    fn streaming_updates_replace_record_in_place() {
        let mut state = HistoryState::new();
        let stream_id = "stream-replace";
        let first_id = state.upsert_assistant_stream_state(stream_id, "partial".into(), None, None);
        assert_ne!(first_id, HistoryId::ZERO);

        let mutation = state.apply_domain_event(HistoryDomainEvent::UpsertAssistantStream {
            stream_id: stream_id.to_string(),
            preview_markdown: "partial updated".into(),
            delta: None,
            metadata: None,
        });

        match mutation {
            HistoryMutation::Replaced { id, .. } => assert_eq!(id, first_id),
            other => panic!("expected replacement mutation, got {other:?}"),
        }

        let cfg = test_config();
        let render_state = HistoryRenderState::new();
        let settings = RenderSettings::new(80, 0, false);
        let request = RenderRequest {
            history_id: first_id,
            cell: None,
            assistant: None,
            use_cache: true,
            fallback_lines: None,
            kind: RenderRequestKind::Streaming { id: first_id },
            config: &cfg,
        };

        let cells = render_state.visible_cells(&state, &[request], settings);
        let layout = cells
            .first()
            .and_then(|cell| cell.layout.as_ref())
            .expect("stream layout missing")
            .layout();
        let text = collect_lines(&layout);
        assert!(text
            .iter()
            .any(|line| line.contains("partial updated")),
            "expected updated preview text");
    }

    #[test]
    fn streaming_flow_handles_deltas_and_finalize() {
        let mut state = HistoryState::new();
        let stream_id = "flow-stream";
        let inserted_id = match state.apply_domain_event(HistoryDomainEvent::UpsertAssistantStream {
            stream_id: stream_id.to_string(),
            preview_markdown: "step 1".into(),
            delta: None,
            metadata: None,
        }) {
            HistoryMutation::Inserted { id, .. } => id,
            other => panic!("unexpected mutation inserting stream record: {other:?}"),
        };

        let cfg = test_config();
        let render_state = HistoryRenderState::new();
        let settings = RenderSettings::new(80, 0, false);
        let stream_lines = |state: &HistoryState| {
            let request = RenderRequest {
                history_id: inserted_id,
                cell: None,
                assistant: None,
                use_cache: true,
                fallback_lines: None,
                kind: RenderRequestKind::Streaming { id: inserted_id },
                config: &cfg,
            };
            render_state
                .visible_cells(state, &[request], settings)
                .first()
                .and_then(|cell| cell.layout.as_ref())
                .expect("stream layout missing")
                .layout()
        };

        let collected = collect_lines(&stream_lines(&state));
        assert!(collected.iter().any(|line| line.contains("step 1")));

        let delta = AssistantStreamDelta {
            delta: "\nstep 2".into(),
            sequence: Some(1),
            received_at: SystemTime::UNIX_EPOCH,
        };
        match state.apply_domain_event(HistoryDomainEvent::UpsertAssistantStream {
            stream_id: stream_id.to_string(),
            preview_markdown: "step 1\nstep 2".into(),
            delta: Some(delta),
            metadata: None,
        }) {
            HistoryMutation::Replaced { id, .. } => assert_eq!(id, inserted_id),
            other => panic!("expected replacement mutation, got {other:?}"),
        }

        // Insert an unrelated record to ensure ordering/invalidation stays stable.
        let filler = PlainMessageState {
            id: HistoryId::ZERO,
            role: PlainMessageRole::System,
            kind: PlainMessageKind::Plain,
            header: None,
            lines: vec![],
            metadata: None,
        };
        state.push(HistoryRecord::PlainMessage(filler));

        let collected = collect_lines(&stream_lines(&state));
        assert!(collected.iter().any(|line| line.contains("step 2")));

        // Finalize stream; ensure the streaming record is removed and replaced with a message.
        let final_state = state.finalize_assistant_stream_state(
            Some(stream_id),
            "step 1\nstep 2\ndone".into(),
            None,
            None,
        );
        let final_id = final_state.id;
        assert!(state
            .records
            .iter()
            .all(|record| !matches!(record, HistoryRecord::AssistantStream(s) if s.stream_id == stream_id)));

        let message_request = RenderRequest {
            history_id: final_id,
            cell: None,
            assistant: None,
            use_cache: true,
            fallback_lines: None,
            kind: RenderRequestKind::Assistant { id: final_id },
            config: &cfg,
        };
        let lines = render_state
            .visible_cells(&state, &[message_request], settings)
            .first()
            .and_then(|cell| cell.layout.as_ref())
            .expect("assistant message layout missing")
            .layout();
        let collected = collect_lines(&lines);
        assert!(collected.iter().any(|line| line.contains("done")));
    }

    #[test]
    fn assistant_render_from_state() {
        let mut state = HistoryState::new();
        let message_state = AssistantMessageState {
            id: HistoryId::ZERO,
            stream_id: None,
            markdown: "Hello **world**".into(),
            citations: Vec::new(),
            metadata: None,
            token_usage: None,
            created_at: SystemTime::UNIX_EPOCH,
        };
        let message_id = state.push(HistoryRecord::AssistantMessage(message_state));

        let cfg = test_config();
        let render_state = HistoryRenderState::new();
        let settings = RenderSettings::new(80, 0, false);
        let request = RenderRequest {
            history_id: message_id,
            cell: None,
            assistant: None,
            use_cache: true,
            fallback_lines: None,
            kind: RenderRequestKind::Assistant { id: message_id },
            config: &cfg,
        };

        let layout = render_state
            .visible_cells(&state, &[request], settings)
            .first()
            .and_then(|cell| cell.layout.as_ref())
            .expect("assistant layout missing")
            .layout();
        let text = collect_lines(&layout);
        assert!(text.iter().any(|line| line.contains("Hello")));
        assert!(text.iter().any(|line| line.contains("world")));
    }

    #[test]
    fn assistant_render_remains_stable_after_insertions() {
        let mut state = HistoryState::new();
        let message_state = AssistantMessageState {
            id: HistoryId::ZERO,
            stream_id: None,
            markdown: "Final answer".into(),
            citations: Vec::new(),
            metadata: None,
            token_usage: None,
            created_at: SystemTime::UNIX_EPOCH,
        };
        let message_id = state.push(HistoryRecord::AssistantMessage(message_state));

        let filler = PlainMessageState {
            id: HistoryId::ZERO,
            role: PlainMessageRole::System,
            kind: PlainMessageKind::Plain,
            header: None,
            lines: Vec::new(),
            metadata: None,
        };
        match state.apply_domain_event(HistoryDomainEvent::Insert {
            index: 0,
            record: HistoryDomainRecord::Plain(filler),
        }) {
            HistoryMutation::Inserted { .. } => {}
            other => panic!("unexpected mutation inserting filler record: {other:?}"),
        }

        let cfg = test_config();
        let render_state = HistoryRenderState::new();
        let settings = RenderSettings::new(80, 0, false);
        let request = RenderRequest {
            history_id: message_id,
            cell: None,
            assistant: None,
            use_cache: true,
            fallback_lines: None,
            kind: RenderRequestKind::Assistant { id: message_id },
            config: &cfg,
        };

        let layout = render_state
            .visible_cells(&state, &[request], settings)
            .first()
            .and_then(|cell| cell.layout.as_ref())
            .expect("assistant layout missing after insert")
            .layout();
        let text = collect_lines(&layout);
        assert!(text.iter().any(|line| line.contains("Final answer")));
    }

    #[test]
    fn exec_render_from_state() {
        let mut state = HistoryState::new();
        let exec_id = start_exec_record(&mut state);
        let exec_index = state.index_of(exec_id).expect("exec index present");

        let chunk = ExecStreamChunk {
            offset: 0,
            content: "output line".into(),
        };
        let mutation = state.apply_domain_event(HistoryDomainEvent::UpdateExecStream {
            index: exec_index,
            stdout_chunk: Some(chunk),
            stderr_chunk: None,
        });
        assert!(matches!(
            mutation,
            HistoryMutation::Replaced { .. }
                | HistoryMutation::Inserted { .. }
                | HistoryMutation::Noop
        ));

        let finish = state.apply_domain_event(HistoryDomainEvent::FinishExec {
            id: Some(exec_id),
            call_id: None,
            status: ExecStatus::Success,
            exit_code: Some(0),
            completed_at: Some(SystemTime::UNIX_EPOCH),
            wait_total: None,
            wait_active: false,
            wait_notes: Vec::new(),
            stdout_tail: None,
            stderr_tail: None,
        });
        assert!(matches!(finish, HistoryMutation::Replaced { .. }));

        let cfg = test_config();
        let render_state = HistoryRenderState::new();
        let settings = RenderSettings::new(80, 0, false);
        let request = RenderRequest {
            history_id: exec_id,
            cell: None,
            assistant: None,
            use_cache: true,
            fallback_lines: None,
            kind: RenderRequestKind::Exec { id: exec_id },
            config: &cfg,
        };

        let layout = render_state
            .visible_cells(&state, &[request], settings)
            .first()
            .and_then(|cell| cell.layout.as_ref())
            .expect("exec layout missing")
            .layout();
        let text = collect_lines(&layout);
        assert!(text.iter().any(|line| line.contains("echo hello")));
        assert!(text.iter().any(|line| line.contains("output line")));
    }

    #[test]
    fn exec_render_remains_stable_after_insertions() {
        let mut state = HistoryState::new();
        let exec_id = start_exec_record(&mut state);

        let filler = PlainMessageState {
            id: HistoryId::ZERO,
            role: PlainMessageRole::System,
            kind: PlainMessageKind::Plain,
            header: None,
            lines: Vec::new(),
            metadata: None,
        };
        match state.apply_domain_event(HistoryDomainEvent::Insert {
            index: 0,
            record: HistoryDomainRecord::Plain(filler),
        }) {
            HistoryMutation::Inserted { .. } => {}
            other => panic!("unexpected mutation inserting filler record: {other:?}"),
        }

        let cfg = test_config();
        let render_state = HistoryRenderState::new();
        let settings = RenderSettings::new(80, 0, false);
        let request = RenderRequest {
            history_id: exec_id,
            cell: None,
            assistant: None,
            use_cache: true,
            fallback_lines: None,
            kind: RenderRequestKind::Exec { id: exec_id },
            config: &cfg,
        };

        let layout = render_state
            .visible_cells(&state, &[request], settings)
            .first()
            .and_then(|cell| cell.layout.as_ref())
            .expect("exec layout missing after insert")
            .layout();
        let text = collect_lines(&layout);
        assert!(text.iter().any(|line| line.contains("echo hello")));
    }

    #[test]
    fn visible_cells_render_explore_records_from_state() {
        let mut state = HistoryState::new();
        let explore_id = insert_explore_record(&mut state);
        let cfg = test_config();

        let render_state = HistoryRenderState::new();
        let settings = RenderSettings::new(80, 0, false);
        let request = RenderRequest {
            history_id: explore_id,
            cell: None,
            assistant: None,
            use_cache: false,
            fallback_lines: None,
            kind: RenderRequestKind::Explore { id: explore_id },
            config: &cfg,
        };
        let cells = render_state.visible_cells(&state, &[request], settings);
        let layout = cells
            .first()
            .and_then(|cell| cell.layout.as_ref())
            .expect("explore layout missing")
            .layout();
        let text = collect_lines(&layout);
        assert!(text.iter().any(|line| line.contains("Explored")));
        assert!(text.iter().any(|line| line.contains("pattern")));
    }
}

/// Output from `HistoryRenderState::visible_cells()`. Contains the resolved
/// layout (if any), plus the optional `HistoryCell` pointer so the caller can
/// reuse existing caches.
pub(crate) struct VisibleCell<'a> {
    pub cell: Option<&'a dyn HistoryCell>,
    pub assistant_plan: Option<AssistantLayoutCache>,
    pub layout: Option<LayoutRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct CacheKey {
    history_id: HistoryId,
    width: u16,
    theme_epoch: u64,
    reasoning_visible: bool,
}

impl CacheKey {
    fn new(history_id: HistoryId, settings: RenderSettings) -> Self {
        Self {
            history_id,
            width: settings.width,
            theme_epoch: settings.theme_epoch,
            reasoning_visible: settings.reasoning_visible,
        }
    }
}
