#![allow(dead_code)]

use code_core::config::Config;
use ratatui::text::Line;
use ratatui::style::Modifier;

use super::HeaderEmitter;
use super::StreamKind;
use super::StreamState;

/// Sink for history insertions and animation control.
pub(crate) trait HistorySink {
    fn insert_history(&self, lines: Vec<Line<'static>>);
    fn insert_history_with_kind(&self, id: Option<String>, kind: StreamKind, lines: Vec<Line<'static>>);
    fn insert_final_answer(&self, id: Option<String>, lines: Vec<Line<'static>>, full_markdown_source: String);
    fn start_commit_animation(&self);
    fn stop_commit_animation(&self);
}

/// Concrete sink backed by `AppEventSender`.
pub(crate) struct AppEventHistorySink(pub(crate) crate::app_event_sender::AppEventSender);

impl HistorySink for AppEventHistorySink {
    fn insert_history(&self, lines: Vec<Line<'static>>) {
        tracing::debug!("sink.insert_history lines={}", lines.len());
        self.0
            .send(crate::app_event::AppEvent::InsertHistory(lines))
    }
    fn insert_history_with_kind(&self, id: Option<String>, kind: StreamKind, lines: Vec<Line<'static>>) {
        tracing::debug!("sink.insert_history_with_kind kind={:?} id={:?} lines={}", kind, id, lines.len());
        self.0
            .send(crate::app_event::AppEvent::InsertHistoryWithKind { id, kind, lines })
    }
    fn insert_final_answer(&self, id: Option<String>, lines: Vec<Line<'static>>, full_markdown_source: String) {
        tracing::debug!("sink.insert_final_answer id={:?} lines={} source_len={}", id, lines.len(), full_markdown_source.len());
        self.0
            .send(crate::app_event::AppEvent::InsertFinalAnswer { id, lines, source: full_markdown_source })
    }
    fn start_commit_animation(&self) {
        self.0
            .send(crate::app_event::AppEvent::StartCommitAnimation)
    }
    fn stop_commit_animation(&self) {
        self.0.send(crate::app_event::AppEvent::StopCommitAnimation)
    }
}

type Lines = Vec<Line<'static>>;

/// Controller that manages newline-gated streaming, header emission, and
/// commit animation across streams.
pub(crate) struct StreamController {
    config: Config,
    header: HeaderEmitter,
    states: [StreamState; 2],
    current_stream: Option<StreamKind>,
    current_stream_id: Option<String>,
    finishing_after_drain: bool,
    thinking_placeholder_shown: bool,
}

impl StreamController {
    pub(crate) fn new(config: Config) -> Self {
        Self {
            config,
            header: HeaderEmitter::new(),
            states: [StreamState::new_for_kind(StreamKind::Answer), StreamState::new_for_kind(StreamKind::Reasoning)],
            current_stream: None,
            current_stream_id: None,
            finishing_after_drain: false,
            thinking_placeholder_shown: false,
        }
    }

    pub(crate) fn reset_headers_for_new_turn(&mut self) {
        self.header.reset_for_new_turn();
    }

    pub(crate) fn is_write_cycle_active(&self) -> bool {
        self.current_stream.is_some()
    }

    pub(crate) fn is_current_stream_idle(&self) -> bool {
        match self.current_stream {
            Some(kind) => {
                let state = self.state(kind);
                state.is_idle() && !state.collector.has_buffered_content()
            }
            None => true,
        }
    }
    

    pub(crate) fn clear_all(&mut self) {
        tracing::debug!("clear_all called, current_stream={:?}", self.current_stream);
        self.states.iter_mut().for_each(|s| s.clear());
        self.current_stream = None;
        self.finishing_after_drain = false;
        self.thinking_placeholder_shown = false;
        // leave header state unchanged; caller decides when to reset
    }

    #[inline]
    fn idx(kind: StreamKind) -> usize {
        kind as usize
    }
    fn state(&self, kind: StreamKind) -> &StreamState {
        &self.states[Self::idx(kind)]
    }
fn state_mut(&mut self, kind: StreamKind) -> &mut StreamState {
    &mut self.states[Self::idx(kind)]
}

/// Record the latest provider sequence_number for this stream kind.
pub(crate) fn set_last_sequence_number(&mut self, kind: StreamKind, seq: Option<u64>) {
    self.state_mut(kind).last_sequence_number = seq;
}

    pub(crate) fn preview_source_for_kind(&self, kind: StreamKind) -> Option<String> {
        if self.current_stream != Some(kind) {
            return None;
        }
        Some(self.state(kind).collector.full_render_source_preview())
    }

    fn emit_header_if_needed(&mut self, kind: StreamKind, out_lines: &mut Lines) -> bool {
        let emitted = self.header.maybe_emit(kind, out_lines);
        if emitted {
            tracing::debug!("stream: emitted header for {:?}", kind);
        }
        emitted
    }

    /// Optionally append a separate dimmed debug marker line indicating the
    /// current reasoning summary index (parsed from the stream id like "…#s3").
    /// This avoids mutating the model content so title detection remains intact.
    fn maybe_append_reasoning_debug_marker(&self, kind: StreamKind, lines: &mut Vec<Line<'static>>) {
        // Only when explicitly enabled to avoid noise in normal use.
        let enabled = match std::env::var("CODEX_DEBUG_REASONING_INDEX") {
            Ok(val) => !val.is_empty() && val != "0",
            Err(_) => false,
        };
        if !enabled || !matches!(kind, StreamKind::Reasoning) {
            return;
        }
        let id = match self.current_stream_id() { Some(s) => s.clone(), None => return };
        // Parse trailing #s<idx>
        let idx = id.split('#').last().and_then(|frag| frag.strip_prefix('s'));
        if let Some(sidx) = idx {
            let seq_part = self.state(kind).last_sequence_number.map(|n| format!(" seq{}", n)).unwrap_or_default();
            let marker = format!("[s{}{}]", sidx, seq_part);
            let dim = crate::colors::text_dim();
            lines.push(Line::from(ratatui::text::Span::styled(marker, ratatui::style::Style::default().fg(dim))));
        }
    }

    #[inline]
    fn ensure_single_trailing_blank(_lines: &mut Lines) {
        // Removed - we don't need to add extra blank lines
        // The markdown renderer and section breaks already handle spacing
    }
    
    /// Get the current stream kind being processed
    pub(crate) fn current_stream(&self) -> Option<StreamKind> {
        self.current_stream
    }
    
    /// Get the current stream ID
    pub(crate) fn current_stream_id(&self) -> Option<&String> {
        self.current_stream_id.as_ref()
    }

    /// Begin a stream, flushing previously completed lines from any other
    /// active stream to maintain ordering.
    pub(crate) fn begin_with_id(&mut self, kind: StreamKind, id: Option<String>, sink: &impl HistorySink) {
        tracing::debug!("stream.begin kind={:?} prev={:?} new_id={:?}", kind, self.current_stream, id);
        // NOTE (dup‑guard): Historically we cleared `current_stream[_id]` even when
        // `kind` did not change, which caused the active Answer stream to lose its id.
        // Downstream, the UI could not find the streaming cell by id on finalization
        // and appended a new Assistant cell (visible duplicate). Keep state when the
        // kind is unchanged, and if the id changes mid‑stream, flush under the old id
        // and adopt the new one so the final can match and replace in‑place.
        if let Some(current) = self.current_stream {
            if current != kind {
                tracing::debug!("Switching from {:?} to {:?}, flushing previous", current, kind);
                // Synchronously flush completed lines from previous stream.
                let cfg = self.config.clone();
                let step = {
                    let prev_state = self.state_mut(current);
                    let newly_completed = prev_state.collector.commit_complete_lines(&cfg);
                    if !newly_completed.is_empty() {
                        prev_state.enqueue(newly_completed);
                    }
                    let result = prev_state.drain_all();
                    // Clear the previous stream state to ensure no contamination
                    tracing::debug!("Clearing {:?} stream state", current);
                    prev_state.clear();
                    result
                };
                if !step.history.is_empty() {
                    tracing::debug!("stream.flush prev={:?} lines={}", current, step.history.len());
                    let mut lines: Lines = Vec::new();
                    self.emit_header_if_needed(current, &mut lines);
                    lines.extend(step.history);
                    // Don't add extra blank line - markdown renderer handles spacing
                    sink.insert_history_with_kind(self.current_stream_id.clone(), current, lines);
                }
                // Only clear current stream tracking when actually switching kinds.
                self.current_stream = None;
                self.current_stream_id = None;
            }
            // If the kind is unchanged, we may still need to handle id transitions.
            // If the incoming id differs from our current id, flush any buffered
            // content under the old id and then adopt the new id so downstream
            // finalize uses a matching identifier.
            if current == kind {
                if let Some(ref new_id) = id {
                    if self.current_stream_id.as_ref() != Some(new_id) {
                        let cfg = self.config.clone();
                        let step = {
                            let prev_state = self.state_mut(current);
                            let newly_completed = prev_state.collector.commit_complete_lines(&cfg);
                            if !newly_completed.is_empty() { prev_state.enqueue(newly_completed); }
                            let result = prev_state.drain_all();
                            tracing::debug!("Flushing {:?} due to id change {:?} -> {:?}", current, self.current_stream_id, id);
                            result
                        };
                        if !step.history.is_empty() {
                            let mut lines: Lines = Vec::new();
                            self.emit_header_if_needed(current, &mut lines);
                            lines.extend(step.history);
                            sink.insert_history_with_kind(self.current_stream_id.clone(), current, lines);
                        }
                        // Now adopt the new id; do not reset kind.
                        self.current_stream_id = Some(new_id.clone());
                    }
                } else if self.current_stream_id.is_none() {
                    // If we previously had no id and a None arrives again, keep as None.
                }
            }
        }

        if self.current_stream != Some(kind) {
            let prev = self.current_stream;
            self.current_stream = Some(kind);
            // Always adopt the provided id when starting a new stream
            self.current_stream_id = id;
            // Starting a new stream cancels any pending finish-from-previous-stream animation.
            self.finishing_after_drain = false;
            if prev.is_some() {
                self.header.reset_for_stream(kind);
            }
            // Emit header immediately for reasoning; for answers, optionally emit immediately.
            if matches!(kind, StreamKind::Reasoning)
                || (matches!(kind, StreamKind::Answer) && self.config.tui.stream.answer_header_immediate)
            {
                let mut header_lines = Vec::new();
                if self.emit_header_if_needed(kind, &mut header_lines) {
                    // Always associate header lines with the active stream id so
                    // the TUI can enforce strict per-stream ordering.
                    sink.insert_history_with_kind(self.current_stream_id.clone(), kind, header_lines);
                    self.thinking_placeholder_shown = true;
                    // For answers, optionally insert an empty streaming cell with a hidden header so
                    // the UI can show a body placeholder (ellipsis) before the first text arrives.
                    if matches!(kind, StreamKind::Answer) && self.config.tui.stream.show_answer_ellipsis {
                        sink.insert_history_with_kind(self.current_stream_id.clone(), kind, vec![ratatui::text::Line::from("codex")]);
                    }
                }
            }
        }
    }

    /// Backwards-compatible entry point without an id.
    pub(crate) fn begin(&mut self, kind: StreamKind, sink: &impl HistorySink) {
        self.begin_with_id(kind, None, sink);
    }

    /// Push a delta; if it contains a newline, commit completed lines and start animation.
    pub(crate) fn push_and_maybe_commit(&mut self, delta: &str, sink: &impl HistorySink) {
        let Some(kind) = self.current_stream else {
            tracing::debug!("push_and_maybe_commit called but no current_stream");
            return;
        };
        tracing::debug!("push_and_maybe_commit for {:?}, delta.len={} contains_nl={}", kind, delta.len(), delta.contains('\n'));
        let cfg = self.config.clone();

        // Check header flag before borrowing state (used only to avoid double headers)
        let _just_emitted_header = self.header.consume_header_flag();
        
        // Mutate collector and counters in a short scope to avoid long mutable borrows.
        {
            let state = self.state_mut(kind);
            if !delta.is_empty() {
                state.has_seen_delta = true;
            }
            state.collector.push_delta(delta);
            state.tail_chars_since_commit = state.tail_chars_since_commit.saturating_add(delta.len());
        }
        if delta.contains('\n') {
            let mut newly_completed = self.state_mut(kind).collector.commit_complete_lines(&cfg);
            // Reduce leading blanks to at most one across commits
            if !newly_completed.is_empty() {
                let mut skip_count = 0;
                while skip_count < newly_completed.len()
                    && crate::render::line_utils::is_blank_line_trim(&newly_completed[skip_count]) {
                    skip_count += 1;
                }
                if skip_count > 1 {
                    for _ in 0..(skip_count - 1) {
                        newly_completed.remove(0);
                    }
                }
            }
            if !newly_completed.is_empty() {
                // IMPORTANT: Do not recolor entire Answer lines. We only dim Reasoning lines.
                // Recoloring the whole Answer line can mask per-span BOLD styling on some
                // terminals. See regression: inline bold appeared normal due to line FG.
                let color = match kind {
                    StreamKind::Reasoning => Some(crate::colors::text_dim()),
                    StreamKind::Answer => Some(crate::colors::text_bright()),
                };
                let mut styled: Vec<Line<'static>> = Vec::with_capacity(newly_completed.len());
                for mut line in newly_completed {
                    if let Some(c) = color { line.style = line.style.patch(ratatui::style::Style::default().fg(c)); }
                    // No per-span overrides needed for Answer: line FG is already bright.
                    styled.push(line);
                }
                let count = styled.len();
                tracing::debug!("stream.commit {:?} newly_completed_lines={}", kind, count);
                {
                    // Add a non-content debug marker line for reasoning
                    let mut with_marker = styled;
                    self.maybe_append_reasoning_debug_marker(kind, &mut with_marker);
                    let state = self.state_mut(kind);
                    state.enqueue(with_marker);
                    state.last_commit_instant = Some(std::time::Instant::now());
                    state.tail_chars_since_commit = 0;
                }
                sink.start_commit_animation();
            }
        }

        // Char-threshold soft commit (when no newline has arrived for a while)
        if !delta.contains('\n') {
            let threshold = self.config.tui.stream.soft_commit_chars
                .or(if self.config.tui.stream.responsive { Some(160) } else { None });
            if let Some(limit) = threshold {
                let ready = { self.state(kind).tail_chars_since_commit >= limit };
                if ready {
                    let relax_list = self.config.tui.stream.relax_list_holdback;
                    let relax_code = self.config.tui.stream.relax_code_holdback;
                    let cfg2 = self.config.clone();
                    let mut newly_completed = {
                        let state = self.state_mut(kind);
                        state.collector.commit_soft_lines(&cfg2, relax_list, relax_code)
                    };
                    if !newly_completed.is_empty() {
                        // Apply stream-specific color
                        let color = match kind {
                            StreamKind::Reasoning => Some(crate::colors::text_dim()),
                            StreamKind::Answer => Some(crate::colors::text_bright()),
                        };
                        let mut styled: Vec<Line<'static>> = Vec::with_capacity(newly_completed.len());
                        for mut line in newly_completed.drain(..) {
                            if let Some(c) = color { line.style = line.style.patch(ratatui::style::Style::default().fg(c)); }
                            styled.push(line);
                        }
                        {
                            let mut with_marker = styled;
                            self.maybe_append_reasoning_debug_marker(kind, &mut with_marker);
                            let state = self.state_mut(kind);
                            state.enqueue(with_marker);
                            state.last_commit_instant = Some(std::time::Instant::now());
                            state.tail_chars_since_commit = 0;
                        }
                        sink.start_commit_animation();
                    }
                }
            }

            // Early commit hint for Reasoning titles: if no soft-commit threshold is
            // configured, opportunistically commit when a new bold-only heading line
            // appears in the preview. This makes additional section titles visible in
            // collapsed mode as they stream, instead of waiting until finalization.
            if self.config.tui.stream.soft_commit_chars.is_none()
                && self.config.tui.stream.soft_commit_timeout_ms.is_none()
                && matches!(kind, StreamKind::Reasoning)
            {
                let cfg2 = self.config.clone();
                // Peek at the rendered lines without changing collector state
                let (committed, saw_heading) = {
                    let state = self.state(kind);
                    let committed = state.collector.committed_count();
                    let preview = state.collector.render_preview_lines(&cfg2);
                    let mut saw_heading = false;
                    for l in preview.iter().skip(committed) {
                        if !l.spans.is_empty()
                            && l
                                .spans
                                .iter()
                                .all(|s| s.style.add_modifier.contains(Modifier::BOLD)
                                    || s.content.trim().is_empty())
                        {
                            saw_heading = true;
                            break;
                        }
                    }
                    (committed, saw_heading)
                };
                // Only early-commit a heading when we are at a line boundary to
                // avoid truncating partially streamed titles (e.g., "Summar").
                let at_boundary = { self.state(kind).collector.ends_with_newline() };
                if saw_heading && at_boundary {
                    let relax_list = self.config.tui.stream.relax_list_holdback;
                    let relax_code = self.config.tui.stream.relax_code_holdback;
                    let mut newly_completed = {
                        let state = self.state_mut(kind);
                        // This advances committed_count; ensure we enqueue the exact lines.
                        state
                            .collector
                            .commit_soft_lines(&cfg2, relax_list, relax_code)
                    };
                    if !newly_completed.is_empty() {
                        let color = Some(crate::colors::text_dim());
                        let mut styled: Vec<Line<'static>> = Vec::with_capacity(newly_completed.len());
                        for mut line in newly_completed.drain(..) {
                            if let Some(c) = color {
                                line.style = line.style.patch(ratatui::style::Style::default().fg(c));
                            }
                            styled.push(line);
                        }
                        {
                            let state = self.state_mut(kind);
                            state.enqueue(styled);
                            state.last_commit_instant = Some(std::time::Instant::now());
                            state.tail_chars_since_commit = 0;
                        }
                        sink.start_commit_animation();
                    }
                } else {
                    let _ = committed; // silence unused warning when cfg gates change
                }
            }
        }
    }

    /// Insert a reasoning section break and commit any newly completed lines.
    pub(crate) fn insert_reasoning_section_break(&mut self, sink: &impl HistorySink) {
        // Only insert a section break when we are actively streaming Reasoning
        // and have a seeded stream id. Without an id, the TUI will drop the
        // insert per strict ordering rules.
        if self.current_stream != Some(StreamKind::Reasoning) || self.current_stream_id.is_none() {
            tracing::debug!("skip section break: no active reasoning stream or missing id");
            return;
        }
        let cfg = self.config.clone();
        // Scope the mutable borrow of state to collector ops only
        let mut newly_completed = {
            let state = self.state_mut(StreamKind::Reasoning);
        // Insert an explicit section break so upcoming section titles are
        // rendered on a fresh line. Without this, bold titles that arrive
        // mid-line can be glued to the previous sentence and fail to be
        // recognized as titles in collapsed view.
            state.collector.insert_section_break();
            state.collector.commit_complete_lines(&cfg)
        };
        // Reduce leading blanks to at most one after section breaks
        if !newly_completed.is_empty() {
            let mut skip_count = 0;
            while skip_count < newly_completed.len()
                && crate::render::line_utils::is_blank_line_trim(&newly_completed[skip_count]) {
                skip_count += 1;
            }
            if skip_count > 1 {
                for _ in 0..(skip_count - 1) {
                    newly_completed.remove(0);
                }
            }
        }
        if !newly_completed.is_empty() {
            // Reasoning sections use dim text
            let color = crate::colors::text_dim();
            let mut styled: Vec<Line<'static>> = Vec::with_capacity(newly_completed.len());
            for mut line in newly_completed {
                let spans = line
                    .spans
                    .into_iter()
                    .map(|s| s.style(ratatui::style::Style::default().fg(color)))
                    .collect();
                line.spans = spans;
                styled.push(line);
            }
            let mut with_marker = styled;
            self.maybe_append_reasoning_debug_marker(StreamKind::Reasoning, &mut with_marker);
            let state = self.state_mut(StreamKind::Reasoning);
            state.enqueue(with_marker);
            sink.start_commit_animation();
        }
    }

    /// Finalize the active stream. If `flush_immediately` is true, drain and emit now.
    pub(crate) fn finalize(
        &mut self,
        kind: StreamKind,
        flush_immediately: bool,
        sink: &impl HistorySink,
    ) -> bool {
        if self.current_stream != Some(kind) {
            return false;
        }
        let cfg = self.config.clone();
        // Capture the full render source BEFORE draining/clearing the collector so
        // we can rebuild the final Assistant cell without losing any content.
        let full_source_before_drain = {
            let state = self.state(kind);
            state.collector.full_render_source_preview()
        };
        // Finalize collector (this clears internal buffers).
        let remaining = {
            let state = self.state_mut(kind);
            state.collector.finalize_and_drain(&cfg)
        };
        if flush_immediately {
            // Collect all output first to avoid emitting headers when there is no content.
            let mut out_lines: Lines = Vec::new();
            {
                let state = self.state_mut(kind);
                if !remaining.is_empty() {
                    state.enqueue(remaining);
                }
                let step = state.drain_all();
                out_lines.extend(step.history);
            }
            // Build output regardless of whether out_lines is empty so we can still
            // replace the streaming cell with a re-renderable final cell.
            let mut lines_with_header: Lines = Vec::new();
            let emitted_header = self.emit_header_if_needed(kind, &mut lines_with_header);
            // Reduce leading blanks to at most one
            let mut skip_count = 0;
            while skip_count < out_lines.len()
                && crate::render::line_utils::is_blank_line_trim(&out_lines[skip_count]) {
                skip_count += 1;
            }
            if skip_count > 1 {
                for _ in 0..(skip_count - 1) {
                    out_lines.remove(0);
                }
            }
            // Apply stream-specific color to body lines
            let color = match kind {
                StreamKind::Reasoning => Some(crate::colors::text_dim()),
                StreamKind::Answer => Some(crate::colors::text_bright()),
            };
            let mut out_lines: Vec<Line<'static>> = out_lines
                .into_iter()
                .map(|mut line| {
                    if let Some(c) = color { line.style = line.style.patch(ratatui::style::Style::default().fg(c)); }
                    line
                })
                .collect();

            // For finalized Reasoning blocks, include a debug marker as a separate line.
            if matches!(kind, StreamKind::Reasoning) {
                // Append a FINAL marker variant to distinguish from streaming commits
                let enabled = match std::env::var("CODEX_DEBUG_REASONING_INDEX") {
                    Ok(val) => !val.is_empty() && val != "0",
                    Err(_) => false,
                };
                if enabled {
                    if let Some(id) = self.current_stream_id() {
                        if let Some(sidx) = id.split('#').last().and_then(|frag| frag.strip_prefix('s')) {
                            let marker = format!("[s{} final]", sidx);
                            let dim = crate::colors::text_dim();
                            out_lines.push(Line::from(ratatui::text::Span::styled(marker, ratatui::style::Style::default().fg(dim))));
                        }
                    }
                }
            }
            lines_with_header.extend(out_lines);
            // Don't add extra blank line - markdown renderer handles spacing
            if matches!(kind, StreamKind::Answer) {
                // Use the source captured before draining so we don't lose content
                // when the collector was cleared by finalize_and_drain.
                tracing::debug!(
                    "stream.finalize ANSWER id={:?} header={} out_lines={} source_len={}",
                    self.current_stream_id,
                    emitted_header,
                    lines_with_header.len(),
                    full_source_before_drain.len()
                );
                sink.insert_final_answer(self.current_stream_id.clone(), lines_with_header, full_source_before_drain);
            } else if !lines_with_header.is_empty() {
                tracing::debug!(
                    "stream.finalize REASONING id={:?} header={} out_lines={}",
                    self.current_stream_id,
                    emitted_header,
                    lines_with_header.len()
                );
                sink.insert_history_with_kind(self.current_stream_id.clone(), kind, lines_with_header);
            }

            // Cleanup
            self.state_mut(kind).clear();
            // Allow a subsequent block of the same kind in this turn to emit its header.
            self.header.allow_reemit_for_same_kind_in_turn(kind);
            // Also clear the per-stream emitted flag so the header can render again.
            self.header.reset_for_stream(kind);
            self.current_stream = None;
            self.current_stream_id = None;
            self.finishing_after_drain = false;
            // Ensure any commit animation thread is stopped when we finalize immediately.
            sink.stop_commit_animation();
            true
        } else {
            if !remaining.is_empty() {
                let state = self.state_mut(kind);
                state.enqueue(remaining);
            }
            // Don't add spacer - causes extra blank lines
            // self.state_mut(kind).enqueue(vec![Line::from("")]);
            self.finishing_after_drain = true;
            sink.start_commit_animation();
            false
        }
    }

    /// Step animation: commit at most one queued line and handle end-of-drain cleanup.
    pub(crate) fn on_commit_tick(&mut self, sink: &impl HistorySink) -> bool {
        let Some(kind) = self.current_stream else {
            return false;
        };
        // Timeout-based soft commit: if no newline arrived and nothing is queued, force a soft commit.
        let timeout_ms = self.config.tui.stream.soft_commit_timeout_ms
            .or(if self.config.tui.stream.responsive { Some(400) } else { None });
        if let Some(ms) = timeout_ms {
            let queue_empty = self.state(kind).is_idle();
            let overdue = self
                .state(kind)
                .last_commit_instant
                .map(|t| t.elapsed() >= std::time::Duration::from_millis(ms))
                .unwrap_or(false);
            if queue_empty && overdue {
                let relax_list = self.config.tui.stream.relax_list_holdback;
                let relax_code = self.config.tui.stream.relax_code_holdback;
                let cfg2 = self.config.clone();
                let mut newly_completed = {
                    let state = self.state_mut(kind);
                    state.collector.commit_soft_lines(&cfg2, relax_list, relax_code)
                };
                if !newly_completed.is_empty() {
                    let color = match kind {
                        StreamKind::Reasoning => Some(crate::colors::text_dim()),
                        StreamKind::Answer => Some(crate::colors::text_bright()),
                    };
                    let mut styled: Vec<Line<'static>> = Vec::with_capacity(newly_completed.len());
                    for mut line in newly_completed.drain(..) {
                        if let Some(c) = color { line.style = line.style.patch(ratatui::style::Style::default().fg(c)); }
                        styled.push(line);
                    }
                    {
                        let state = self.state_mut(kind);
                        state.enqueue(styled);
                        state.last_commit_instant = Some(std::time::Instant::now());
                        state.tail_chars_since_commit = 0;
                    }
                    sink.start_commit_animation();
                }
            }
        }
        let step = {
            let state = self.state_mut(kind);
            state.step()
        };
        if !step.history.is_empty() {
            let mut lines: Lines = Vec::new();
            // Emit header if needed for this stream; ignore return value
            self.emit_header_if_needed(kind, &mut lines);
            let mut out = lines;
            let mut history = step.history;
            // Reduce leading blanks to at most one
            if !history.is_empty() {
                let mut skip_count = 0;
                while skip_count < history.len()
                    && crate::render::line_utils::is_blank_line_trim(&history[skip_count]) {
                    skip_count += 1;
                }
                if skip_count > 1 {
                    for _ in 0..(skip_count - 1) {
                        history.remove(0);
                    }
                }
            }
            // Apply stream-specific color to body lines while preserving modifiers
            let color = match kind {
                StreamKind::Reasoning => Some(crate::colors::text_dim()),
                StreamKind::Answer => Some(crate::colors::text_bright()),
            };
            let mut history: Vec<Line<'static>> = history
                .into_iter()
                .map(|mut line| {
                    if let Some(c) = color { line.style = line.style.patch(ratatui::style::Style::default().fg(c)); }
                    line
                })
                .collect();
            // Append debug marker to streamed reasoning batches as their own line.
            self.maybe_append_reasoning_debug_marker(kind, &mut history);
            out.extend(history);
            sink.insert_history_with_kind(self.current_stream_id.clone(), kind, out);
        }

        let is_idle = self.state(kind).is_idle();
        if is_idle {
            sink.stop_commit_animation();
            if self.finishing_after_drain {
                // Reset and notify
                self.state_mut(kind).clear();
                // Allow a subsequent block of the same kind in this turn to emit its header.
                self.header.allow_reemit_for_same_kind_in_turn(kind);
                // Also clear the per-stream emitted flag so the header can render again.
                self.header.reset_for_stream(kind);
                self.current_stream = None;
                self.current_stream_id = None;
                self.finishing_after_drain = false;
                return true;
            }
        }
        false
    }

    /// Apply a full final answer: replace queued content with only the remaining tail,
    /// then finalize immediately and notify completion.
    pub(crate) fn apply_final_answer(&mut self, message: &str, sink: &impl HistorySink) -> bool {
        tracing::debug!("apply_final_answer called with: {:?}...", message.chars().take(100).collect::<String>());
        self.apply_full_final(StreamKind::Answer, message, true, sink)
    }

    pub(crate) fn apply_final_reasoning(&mut self, message: &str, sink: &impl HistorySink) -> bool {
        tracing::debug!("apply_final_reasoning called with: {:?}...", message.chars().take(100).collect::<String>());
        self.apply_full_final(StreamKind::Reasoning, message, false, sink)
    }

    fn apply_full_final(
        &mut self,
        kind: StreamKind,
        message: &str,
        immediate: bool,
        sink: &impl HistorySink,
    ) -> bool {
        tracing::debug!("apply_full_final for {:?}, immediate={}, message_len={}, current_stream={:?}", 
            kind, immediate, message.len(), self.current_stream);
        
        // Check if we're already processing this stream
        if self.current_stream == Some(kind) {
            let state = self.state(kind);
            let has_delta = state.has_seen_delta;

            if has_delta {
                // Key-based deduplication for Reasoning:
                // If we have already streamed deltas for this reasoning section
                // (same stream id within this request), do NOT inject the final
                // full text again. Finalize the stream using the accumulated
                // deltas only. This avoids duplicate headings/paragraphs without
                // relying on string comparisons.
                if matches!(kind, StreamKind::Reasoning) {
                    tracing::debug!(
                        "Dedup: ignoring final {:?} after deltas; finalizing existing content",
                        kind
                    );
                    return self.finalize(kind, immediate, sink);
                }
                // For Answer (or empty message), finalize existing streamed content.
                tracing::debug!(
                    "Already streaming {:?} via deltas, finalizing without injection",
                    kind
                );
                return self.finalize(kind, immediate, sink);
            } else if self.finishing_after_drain {
                // We're already in the process of finishing this stream (animation phase)
                // This is likely a duplicate event - ignore it
                tracing::debug!("Already finishing {:?} stream, ignoring duplicate final event", kind);
                return false;
            }
            // else: We have a stream open but no deltas yet - could be a header-only stream
            // Fall through to inject the message
        }
        
        // Strict ordering policy: We must already have begun this stream with an id.
        // Do NOT auto-begin without an id; the caller (ChatWidget) is responsible for
        // seeding the stream with `begin_with_id(kind, Some(id), ...)` prior to applying
        // a full final. If this is violated, drop and log.
        if self.current_stream != Some(kind) {
            tracing::error!("strict ordering: apply_full_final called without active {:?} stream; missing begin_with_id(id)", kind);
            return false;
        }

        {
            let state = self.state_mut(kind);
            tracing::debug!("State for {:?}: has_seen_delta={}, committed_count={}, message_empty={}",
                kind, state.has_seen_delta, 
                state.collector.committed_count(),
                message.is_empty());
            
            // Inject the full message since we haven't been streaming it
            if !message.is_empty() {
                tracing::debug!("Injecting full message into {:?} collector", kind);
                // normalize to end with newline
                let mut msg = message.to_owned();
                if !msg.ends_with('\n') {
                    msg.push('\n');
                }

                // replace while preserving already committed count
                let committed = state.collector.committed_count();
                state
                    .collector
                    .replace_with_and_mark_committed(&msg, committed);
            }
        }

        self.finalize(kind, immediate, sink)
    }
}
