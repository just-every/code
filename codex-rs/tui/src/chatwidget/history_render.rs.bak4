use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use ratatui::buffer::Cell as BufferCell;
use ratatui::text::Line;

use crate::history::state::{HistoryId, HistoryRecord, HistoryState};
use crate::history_cell::{
    assistant_markdown_lines,
    compute_assistant_layout,
    explore_lines_from_record_with_force,
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
    pub(crate) height_cache: RefCell<HashMap<CacheKey, u16>>,
    pub(crate) height_cache_last_width: Cell<u16>,
    pub(crate) prefix_sums: RefCell<Vec<u16>>,
    pub(crate) last_prefix_width: Cell<u16>,
    pub(crate) last_prefix_count: Cell<usize>,
    pub(crate) last_total_height: Cell<u16>,
    pub(crate) prefix_valid: Cell<bool>,
}

impl HistoryRenderState {
    pub(crate) fn new() -> Self {
        Self {
            layout_cache: RefCell::new(HashMap::new()),
            height_cache: RefCell::new(HashMap::new()),
            height_cache_last_width: Cell::new(0),
            prefix_sums: RefCell::new(Vec::new()),
            last_prefix_width: Cell::new(0),
            last_prefix_count: Cell::new(0),
            last_total_height: Cell::new(0),
            prefix_valid: Cell::new(false),
        }
    }

    pub(crate) fn invalidate_height_cache(&self) {
        self.layout_cache.borrow_mut().clear();
        self.height_cache.borrow_mut().clear();
        self.prefix_sums.borrow_mut().clear();
        self.last_total_height.set(0);
        self.prefix_valid.set(false);
    }

    pub(crate) fn handle_width_change(&self, width: u16) {
        if self.height_cache_last_width.get() != width {
            self.layout_cache
                .borrow_mut()
                .retain(|key, _| key.width == width);
            self.height_cache
                .borrow_mut()
                .retain(|key, _| key.width == width);
            self.prefix_sums.borrow_mut().clear();
            self.last_total_height.set(0);
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
        self.height_cache
            .borrow_mut()
            .retain(|key, _| key.history_id != id);
        self.prefix_sums.borrow_mut().clear();
        self.last_total_height.set(0);
        self.prefix_valid.set(false);
    }

    pub(crate) fn invalidate_all(&self) {
        self.layout_cache.borrow_mut().clear();
        self.height_cache.borrow_mut().clear();
        self.prefix_sums.borrow_mut().clear();
        self.last_total_height.set(0);
        self.prefix_valid.set(false);
    }

    pub(crate) fn should_rebuild_prefix(&self, width: u16, count: usize) -> bool {
        if !self.prefix_valid.get() {
            return true;
        }
        if self.last_prefix_width.get() != width {
            return true;
        }
        if self.last_prefix_count.get() != count {
            return true;
        }
        false
    }

    pub(crate) fn update_prefix_cache(
        &self,
        width: u16,
        prefix: Vec<u16>,
        total_height: u16,
        count: usize,
    ) {
        {
            let mut ps = self.prefix_sums.borrow_mut();
            *ps = prefix;
        }
        self.last_prefix_width.set(width);
        self.last_prefix_count.set(count);
        self.last_total_height.set(total_height);
        self.prefix_valid.set(true);
    }

    pub(crate) fn last_total_height(&self) -> u16 {
        self.last_total_height.get()
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
                let assistant_plan = if settings.width == 0 {
                    None
                } else if let Some(assistant_cell) = req.assistant {
                    Some(assistant_cell.ensure_layout(settings.width))
                } else if let RenderRequestKind::Assistant { id } = req.kind {
                    history_state
                        .record(id)
                        .and_then(|record| match record {
                            HistoryRecord::AssistantMessage(state) => Some(Rc::new(
                                compute_assistant_layout(state, req.config, settings.width),
                            )),
                            _ => None,
                        })
                } else {
                    None
                };

                let has_custom_render = req
                    .cell
                    .map(|cell| cell.has_custom_render())
                    .unwrap_or(false);

                let layout = if has_custom_render {
                    None
                } else if settings.width == 0 {
                    None
                } else if assistant_plan.is_some() {
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

                let use_height_cache = req.use_cache && req.history_id != HistoryId::ZERO;
                let cached_height = if use_height_cache {
                    let key = CacheKey::new(req.history_id, settings);
                    self.height_cache
                        .borrow()
                        .get(&key)
                        .copied()
                        .map(|h| (h, HeightSource::Cached, None))
                } else {
                    None
                };

                let (height, height_source, height_measure_ns) = if settings.width == 0 {
                    (0, HeightSource::ZeroWidth, None)
                } else if let Some(plan) = assistant_plan.as_ref() {
                    (plan.total_rows(), HeightSource::AssistantPlan, None)
                } else if let Some(layout_ref) = layout.as_ref() {
                    (
                        layout_ref
                            .line_count()
                            .min(u16::MAX as usize) as u16,
                        HeightSource::Layout,
                        None,
                    )
                } else if let Some((h, src, measure)) = cached_height {
                    (h, src, measure)
                } else if let Some(lines) = req.fallback_lines.as_ref() {
                    let wrapped = word_wrap_lines(lines, settings.width);
                    let height = wrapped.len().min(u16::MAX as usize) as u16;
                    if use_height_cache {
                        let key = CacheKey::new(req.history_id, settings);
                        self.height_cache.borrow_mut().insert(key, height);
                    }
                    (height, HeightSource::FallbackLines, None)
                } else if let Some(cell) = req.cell {
                    let start = Instant::now();
                    let computed = cell.desired_height(settings.width);
                    let elapsed = start.elapsed().as_nanos();
                    if use_height_cache {
                        let key = CacheKey::new(req.history_id, settings);
                        self.height_cache.borrow_mut().insert(key, computed);
                    }
                    (
                        computed,
                        HeightSource::DesiredHeight,
                        Some(elapsed),
                    )
                } else {
                    (0, HeightSource::Unknown, None)
                };

                VisibleCell {
                    cell: req.cell,
                    assistant_plan,
                    layout,
                    height,
                    height_source,
                    height_measure_ns,
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
                let hold_title = explore_should_hold_title(history_state, id);
                return explore_lines_from_record_with_force(record, hold_title);
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

pub(crate) fn explore_should_hold_title(history_state: &HistoryState, explore_id: HistoryId) -> bool {
    if explore_id == HistoryId::ZERO {
        return true;
    }

    let Some(mut idx) = history_state.index_of(explore_id) else {
        return true;
    };

    idx += 1;
    while let Some(record) = history_state.get(idx) {
        match record {
            HistoryRecord::Reasoning(_) => {
                idx += 1;
                continue;
            }
            _ => return false,
        }
    }

    true
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
        HistoryState,
        InlineSpan,
        MessageLine,
        MessageLineKind,
        PlainMessageKind,
        PlainMessageRole,
        PlainMessageState,
        ReasoningBlock,
        ReasoningSection,
        ReasoningState,
        TextEmphasis,
        TextTone,
    };
    use crate::history_cell::assistant::AssistantSeg;
    use crate::history_cell::CollapsibleReasoningCell;
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

    fn collect_plan_lines(plan: &AssistantLayoutCache) -> Vec<String> {
        let mut out = Vec::new();
        for seg in &plan.segs {
            match seg {
                AssistantSeg::Text(lines) | AssistantSeg::Bullet(lines) => {
                    for line in lines {
                        let text = line
                            .spans
                            .iter()
                            .map(|span| span.content.as_ref())
                            .collect::<String>();
                        out.push(text);
                    }
                }
                AssistantSeg::Code { lines, .. } => {
                    for line in lines {
                        let text = line
                            .spans
                            .iter()
                            .map(|span| span.content.as_ref())
                            .collect::<String>();
                        out.push(text);
                    }
                }
            }
        }
        out
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

    fn make_inline_span(text: &str) -> InlineSpan {
        InlineSpan {
            text: text.into(),
            tone: TextTone::Default,
            emphasis: TextEmphasis::default(),
            entity: None,
        }
    }

    fn make_reasoning_state(summary: &str) -> ReasoningState {
        let span = make_inline_span(summary);
        ReasoningState {
            id: HistoryId::ZERO,
            sections: vec![ReasoningSection {
                heading: Some(format!("{summary} heading")),
                summary: Some(vec![span.clone()]),
                blocks: vec![ReasoningBlock::Paragraph(vec![span])],
            }],
            effort: None,
            in_progress: false,
        }
    }

    fn make_plain_message(text: &str) -> PlainMessageState {
        PlainMessageState {
            id: HistoryId::ZERO,
            role: PlainMessageRole::Assistant,
            kind: PlainMessageKind::Plain,
            header: None,
            lines: vec![MessageLine {
                kind: MessageLineKind::Paragraph,
                spans: vec![make_inline_span(text)],
            }],
            metadata: None,
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
        let plan = cells
            .first()
            .and_then(|cell| cell.assistant_plan.as_ref())
            .expect("assistant plan missing");
        let text = collect_plan_lines(plan);
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
        let plan = cells
            .first()
            .and_then(|cell| cell.assistant_plan.as_ref())
            .expect("assistant plan missing");
        let text = collect_plan_lines(plan);
        assert!(text.iter().any(|line| line.contains("echo hi")));
        assert!(plan
            .segs
            .iter()
            .any(|seg| matches!(seg, AssistantSeg::Code { lang_label, .. } if lang_label.as_deref() == Some("bash"))));
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
        let plan = render_state
            .visible_cells(&state, &[message_request], settings)
            .first()
            .and_then(|cell| cell.assistant_plan.as_ref())
            .expect("assistant message plan missing");
        let collected = collect_plan_lines(plan);
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

        let plan = render_state
            .visible_cells(&state, &[request], settings)
            .first()
            .and_then(|cell| cell.assistant_plan.as_ref())
            .expect("assistant plan missing");
        let text = collect_plan_lines(plan);
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

        let plan = render_state
            .visible_cells(&state, &[request], settings)
            .first()
            .and_then(|cell| cell.assistant_plan.as_ref())
            .expect("assistant plan missing after insert");
        let text = collect_plan_lines(plan);
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
        assert!(text.iter().any(|line| line.contains("Exploring")));
        assert!(text.iter().any(|line| line.contains("pattern")));
    }

    #[test]
    fn collapsed_reasoning_cells_should_not_reserve_height_when_hidden() {
        let mut state = HistoryState::new();
        let explore_id = insert_explore_record(&mut state);
        let first_reasoning_id = state.push(HistoryRecord::Reasoning(make_reasoning_state(
            "first hidden reasoning",
        )));
        let second_reasoning_id = state.push(HistoryRecord::Reasoning(make_reasoning_state(
            "second hidden reasoning",
        )));
        let thinking_id = state.push(HistoryRecord::PlainMessage(make_plain_message(
            "Composing repository overview",
        )));

        let cfg = test_config();
        let ids = [
            explore_id,
            first_reasoning_id,
            second_reasoning_id,
            thinking_id,
        ];

        let mut cells: Vec<Box<dyn HistoryCell>> = ids
            .iter()
            .map(|id| {
                let record = state.record(*id).expect("record missing");
                crate::history_cell::cell_from_record(record, &cfg)
            })
            .collect();

        for cell in &mut cells {
            if let Some(reasoning_cell) = cell
                .as_any_mut()
                .downcast_mut::<CollapsibleReasoningCell>()
            {
                reasoning_cell.set_collapsed(true);
                reasoning_cell.set_hide_when_collapsed(true);
            }
        }

        let cell_refs: Vec<&dyn HistoryCell> = cells.iter().map(|cell| cell.as_ref()).collect();

        let mut render_requests: Vec<RenderRequest> = Vec::new();
        for (idx, history_id) in ids.iter().enumerate() {
            let record = state.record(*history_id).expect("record missing");
            let (kind, fallback_lines) = match record {
                HistoryRecord::Explore(_) => (RenderRequestKind::Explore { id: *history_id }, None),
                other => (
                    RenderRequestKind::Legacy,
                    Some(crate::history_cell::lines_from_record(other, &cfg)),
                ),
            };

            render_requests.push(RenderRequest {
                history_id: *history_id,
                cell: Some(cell_refs[idx]),
                assistant: None,
                use_cache: false,
                fallback_lines,
                kind,
                config: &cfg,
            });
        }

        let render_state = HistoryRenderState::new();
        let settings = RenderSettings::new(80, 0, false);
        let visible = render_state.visible_cells(&state, &render_requests, settings);
        assert_eq!(visible.len(), ids.len());

        let explore_height = visible[0].height;
        assert_eq!(visible[1].height, 0, "hidden reasoning cell should have zero height");
        assert_eq!(visible[2].height, 0, "hidden reasoning cell should have zero height");

        let thinking_idx = 3usize;
        let spacing = 1u16;
        let mut accumulated = 0u16;
        for (idx, cell) in visible.iter().enumerate() {
            if idx == thinking_idx {
                break;
            }
            accumulated = accumulated.saturating_add(cell.height);
            if idx < thinking_idx && idx < visible.len().saturating_sub(1) {
                let mut should_add_spacing = cell.height > 0;
                if should_add_spacing {
                    let this_collapsed = visible[idx]
                        .cell
                        .and_then(|c| c.as_any().downcast_ref::<CollapsibleReasoningCell>())
                        .map(|rc| rc.is_collapsed())
                        .unwrap_or(false);
                    if this_collapsed {
                        let next_collapsed = visible
                            .get(idx + 1)
                            .and_then(|next| next.cell)
                            .and_then(|c| c.as_any().downcast_ref::<CollapsibleReasoningCell>())
                            .map(|rc| rc.is_collapsed())
                            .unwrap_or(false);
                        if next_collapsed {
                            should_add_spacing = false;
                        }
                    }
                }
                if should_add_spacing {
                    accumulated = accumulated.saturating_add(spacing);
                }
            }
        }

        assert_eq!(
            accumulated, explore_height,
            "collapsed reasoning cells should not push the next visible cell"
        );
    }

    #[test]
    fn explore_should_hold_title_until_non_reasoning_block() {
        let mut state = HistoryState::new();
        let explore_id = insert_explore_record(&mut state);
        assert!(explore_should_hold_title(&state, explore_id));

        let _reasoning_id = state.push(HistoryRecord::Reasoning(make_reasoning_state(
            "reasoning only",
        )));
        assert!(explore_should_hold_title(&state, explore_id));

        let _plain_id = state.push(HistoryRecord::PlainMessage(make_plain_message(
            "ready to summarize",
        )));
        assert!(!explore_should_hold_title(&state, explore_id));
    }
}
/// Output from `HistoryRenderState::visible_cells()`. Contains the resolved
/// layout (if any), plus the optional `HistoryCell` pointer so the caller can
/// reuse existing caches.
pub(crate) struct VisibleCell<'a> {
    pub cell: Option<&'a dyn HistoryCell>,
    pub assistant_plan: Option<Rc<AssistantLayoutCache>>,
    pub layout: Option<LayoutRef>,
    pub height: u16,
    pub height_source: HeightSource,
    pub height_measure_ns: Option<u128>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeightSource {
    AssistantPlan,
    Layout,
    Cached,
    DesiredHeight,
    FallbackLines,
    ZeroWidth,
    Unknown,
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
