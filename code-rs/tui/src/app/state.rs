use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender as StdSender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::KeyCode;
use portable_pty::MasterPty;
use ratatui::buffer::Buffer;
use ratatui::prelude::Size;
use ratatui::CompletedFrame;

use crate::app_event::{AppEvent, TerminalRunController};
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::{ChatWidget, GhostState};
use crate::file_search::FileSearchManager;
use crate::history::state::HistorySnapshot;
use crate::onboarding::onboarding_screen::OnboardingScreen;
use crate::thread_spawner;
use crate::tui::TerminalInfo;
use code_core::config::Config;
use code_core::ConversationManager;
use code_login::ShutdownHandle;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

#[cfg(unix)]
use signal_hook::SigId;

/// Time window for debouncing redraw requests.
///
/// Temporarily widened to ~30 FPS (33 ms) to coalesce bursts of updates while
/// we smooth out per-frame hotspots; keeps redraws responsive without pegging
/// the main thread.
pub(super) const REDRAW_DEBOUNCE: Duration = Duration::from_millis(33);
// Prevent bulk events (Codex output/tool completions) from being starved behind a
// continuous stream of high-priority events (e.g., redraw scheduling).
pub(super) const HIGH_EVENT_BURST_MAX: u32 = 32;
/// After this many consecutive backpressure skips, force a non‑blocking draw so
/// buffered output can catch up even if POLLOUT never flips true (e.g., tmux
/// reattach or XON/XOFF throttling).
pub(super) const BACKPRESSURE_FORCED_DRAW_SKIPS: u32 = 4;
pub(super) const DEFAULT_PTY_ROWS: u16 = 24;
pub(super) const DEFAULT_PTY_COLS: u16 = 80;
const FRAME_TIMER_LOG_THROTTLE_SECS: u64 = 5;

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Top-level application state: which full-screen view is currently active.
#[allow(clippy::large_enum_variant)]
pub(super) enum AppState<'a> {
    Onboarding {
        screen: OnboardingScreen,
    },
    /// The main chat UI is visible.
    Chat {
        /// Boxed to avoid a large enum variant and reduce the overall size of
        /// `AppState`.
        widget: Box<ChatWidget<'a>>,
    },
}

pub(super) struct TerminalRunState {
    pub(super) command: Vec<String>,
    pub(super) display: String,
    pub(super) cancel_tx: Option<oneshot::Sender<()>>,
    pub(super) running: bool,
    pub(super) controller: Option<TerminalRunController>,
    pub(super) writer_tx: Option<Arc<Mutex<Option<StdSender<Vec<u8>>>>>>,
    pub(super) pty: Option<Arc<Mutex<Box<dyn MasterPty + Send>>>>,
}

pub(super) struct FrameTimer {
    state: Mutex<FrameTimerState>,
    cv: Condvar,
    last_limit_log_secs: AtomicU64,
    suppressed_limit_logs: AtomicUsize,
}

struct FrameTimerState {
    deadlines: BinaryHeap<Reverse<Instant>>,
    worker_running: bool,
}

impl FrameTimer {
    pub(super) fn new() -> Self {
        Self {
            state: Mutex::new(FrameTimerState {
                deadlines: BinaryHeap::new(),
                worker_running: false,
            }),
            cv: Condvar::new(),
            last_limit_log_secs: AtomicU64::new(0),
            suppressed_limit_logs: AtomicUsize::new(0),
        }
    }

    fn log_spawn_rejection(&self, drained: usize) {
        let now = now_epoch_secs();
        let last = self.last_limit_log_secs.load(Ordering::Relaxed);
        if now.saturating_sub(last) >= FRAME_TIMER_LOG_THROTTLE_SECS
            && self
                .last_limit_log_secs
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            let suppressed = self.suppressed_limit_logs.swap(0, Ordering::Relaxed);
            tracing::info!(
                drained_deadlines = drained,
                suppressed,
                "frame timer spawn rejected: background thread limit reached; flushed deadlines"
            );
        } else {
            self.suppressed_limit_logs.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(super) fn schedule(self: &Arc<Self>, duration: Duration, tx: AppEventSender) {
        let deadline = Instant::now() + duration;
        let mut state = self.state.lock().unwrap();
        state.deadlines.push(Reverse(deadline));
        let should_spawn = if !state.worker_running {
            state.worker_running = true;
            true
        } else {
            false
        };
        self.cv.notify_one();
        drop(state);

        if should_spawn {
            let timer = Arc::clone(self);
            let tx_for_thread = tx.clone();
            if thread_spawner::spawn_lightweight("frame-timer", move || timer.run(tx_for_thread)).is_none() {
                let mut state = self.state.lock().unwrap();
                state.worker_running = false;
                let drained = state.deadlines.len();
                state.deadlines.clear();
                drop(state);
                for _ in 0..drained.max(1) {
                    tx.send(AppEvent::RequestRedraw);
                }
                self.log_spawn_rejection(drained);
            }
        }
    }

    fn run(self: Arc<Self>, tx: AppEventSender) {
        let mut state = self.state.lock().unwrap();
        loop {
            let deadline = match state.deadlines.peek().copied() {
                Some(Reverse(deadline)) => deadline,
                None => {
                    state.worker_running = false;
                    break;
                }
            };

            let now = Instant::now();
            if deadline <= now {
                state.deadlines.pop();
                drop(state);
                tx.send(AppEvent::RequestRedraw);
                state = self.state.lock().unwrap();
                continue;
            }

            let wait_dur = deadline.saturating_duration_since(now);
            let (new_state, result) = self.cv.wait_timeout(state, wait_dur).unwrap();
            state = new_state;

            if result.timed_out() {
                continue;
            }
        }
    }
}

pub(super) struct LoginFlowState {
    pub(super) shutdown: Option<ShutdownHandle>,
    pub(super) join_handle: JoinHandle<()>,
}

pub(crate) struct App<'a> {
    pub(super) _server: Arc<ConversationManager>,
    pub(super) app_event_tx: AppEventSender,
    // Split event receivers: high‑priority (input) and bulk (streaming)
    pub(super) app_event_rx_high: Receiver<AppEvent>,
    pub(super) app_event_rx_bulk: Receiver<AppEvent>,
    pub(super) consecutive_high_events: u32,
    pub(super) app_state: AppState<'a>,

    /// Config is stored here so we can recreate ChatWidgets as needed.
    pub(super) config: Config,

    /// Latest available release version (if detected) so new widgets can surface it.
    pub(super) latest_upgrade_version: Option<String>,

    pub(super) file_search: FileSearchManager,

    /// True when a redraw has been scheduled but not yet executed (debounce window).
    pub(super) pending_redraw: Arc<AtomicBool>,
    /// Tracks whether a frame is currently queued or being drawn. Used to coalesce
    /// rapid-fire redraw requests without dropping the final state.
    pub(super) redraw_inflight: Arc<AtomicBool>,
    /// Set if a redraw request arrived while another frame was in flight. Ensures we
    /// queue one more frame immediately after the current draw completes.
    pub(super) post_frame_redraw: Arc<AtomicBool>,
    /// Count of consecutive redraws skipped because stdout/PTY was not writable.
    pub(super) stdout_backpressure_skips: u32,
    /// Shared scheduler for future animation frames. Ensures the shortest
    /// requested interval wins while preserving later deadlines.
    pub(super) frame_timer: Arc<FrameTimer>,
    /// Controls the input reader thread spawned at startup.
    pub(super) input_running: Arc<AtomicBool>,

    pub(super) enhanced_keys_supported: bool,
    /// Tracks keys seen as pressed when keyboard enhancements are unavailable
    /// so duplicate release events can be filtered and release-only terminals
    /// still synthesize a press.
    pub(super) non_enhanced_pressed_keys: HashSet<KeyCode>,

    /// Debug flag for logging LLM requests/responses
    pub(super) _debug: bool,
    /// Show per-cell ordering overlay when true
    pub(super) show_order_overlay: bool,

    /// Controls the animation thread that sends CommitTick events.
    pub(super) commit_anim_running: Arc<AtomicBool>,

    /// Terminal information queried at startup
    pub(super) terminal_info: TerminalInfo,

    #[cfg(unix)]
    pub(super) sigterm_guard: Option<SigId>,
    #[cfg(unix)]
    pub(super) sigterm_flag: Arc<AtomicBool>,

    /// Perform a hard clear on the first frame to ensure the entire buffer
    /// starts with our theme background. This avoids terminals that may show
    /// profile defaults until all cells are explicitly painted.
    pub(super) clear_on_first_frame: bool,

    /// Pending ghost snapshot state to apply after a conversation fork completes.
    pub(super) pending_jump_back_ghost_state: Option<GhostState>,
    /// Pending history snapshot to seed the next widget after a jump-back fork.
    pub(super) pending_jump_back_history_snapshot: Option<HistorySnapshot>,

    /// Track last known terminal size. If it changes (true resize or a
    /// tab switch that altered the viewport), perform a full clear on the next
    /// draw to avoid ghost cells from the previous size. This is cheap and
    /// happens rarely, but fixes Windows/macOS terminals that don't fully
    /// repaint after focus/size changes until a manual resize occurs.
    pub(super) last_frame_size: Option<Size>,

    // Double‑Esc timing for undo timeline
    pub(super) last_esc_time: Option<Instant>,

    /// If true, enable lightweight timing collection and report on exit.
    pub(super) timing_enabled: bool,
    pub(super) timing: TimingStats,

    pub(super) buffer_diff_profiler: BufferDiffProfiler,

    /// True when TUI is currently rendering in the terminal's alternate screen.
    pub(super) alt_screen_active: bool,

    pub(super) terminal_runs: HashMap<u64, TerminalRunState>,

    pub(super) terminal_title_override: Option<String>,
    pub(super) login_flow: Option<LoginFlowState>,
}

/// Aggregate parameters needed to create a `ChatWidget`, as creation may be
/// deferred until after the Git warning screen is dismissed.
#[derive(Clone, Debug)]
pub(crate) struct ChatWidgetArgs {
    pub(crate) config: Config,
    pub(crate) initial_prompt: Option<String>,
    pub(crate) initial_images: Vec<PathBuf>,
    pub(crate) enhanced_keys_supported: bool,
    pub(crate) terminal_info: TerminalInfo,
    pub(crate) show_order_overlay: bool,
    pub(crate) enable_perf: bool,
    pub(crate) resume_picker: bool,
    pub(crate) latest_upgrade_version: Option<String>,
}

impl App<'_> {
    pub(crate) const DEFAULT_TERMINAL_TITLE: &'static str = "Code";

    #[cfg(unix)]
    pub(crate) fn sigterm_triggered(&self) -> bool {
        self.sigterm_flag.load(Ordering::Relaxed)
    }

    #[cfg(unix)]
    pub(crate) fn clear_sigterm_guard(&mut self) {
        self.sigterm_guard.take();
    }

    pub(crate) fn token_usage(&self) -> code_core::protocol::TokenUsage {
        let usage = match &self.app_state {
            AppState::Chat { widget } => widget.token_usage().clone(),
            AppState::Onboarding { .. } => code_core::protocol::TokenUsage::default(),
        };
        // ensure background helpers stop before returning
        self.commit_anim_running.store(false, Ordering::Release);
        self.input_running.store(false, Ordering::Release);
        usage
    }

    pub(crate) fn session_id(&self) -> Option<uuid::Uuid> {
        match &self.app_state {
            AppState::Chat { widget } => widget.session_id(),
            AppState::Onboarding { .. } => None,
        }
    }

    /// Return a human-readable performance summary if timing was enabled.
    pub(crate) fn perf_summary(&self) -> Option<String> {
        if !self.timing_enabled {
            return None;
        }
        let mut out = String::new();
        if let AppState::Chat { widget } = &self.app_state {
            out.push_str(&widget.perf_summary());
            out.push_str("\n\n");
        }
        out.push_str(&self.timing.summarize());
        Some(out)
    }
}

pub(super) struct BufferDiffProfiler {
    enabled: bool,
    prev: Option<Buffer>,
    frame_seq: u64,
    log_every: usize,
    min_changed: usize,
    min_percent: f64,
}

impl BufferDiffProfiler {
    pub(super) fn new_from_env() -> Self {
        match std::env::var("CODE_BUFFER_DIFF_METRICS") {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() || trimmed == "0" {
                    Self::disabled()
                } else {
                    let log_every = trimmed.parse::<usize>().unwrap_or(1).max(1);
                    let min_changed = std::env::var("CODE_BUFFER_DIFF_MIN_CHANGED")
                        .ok()
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(100);
                    let min_percent = std::env::var("CODE_BUFFER_DIFF_MIN_PERCENT")
                        .ok()
                        .and_then(|v| v.trim().parse::<f64>().ok())
                        .unwrap_or(1.0_f64);
                    Self {
                        enabled: true,
                        prev: None,
                        frame_seq: 0,
                        log_every,
                        min_changed,
                        min_percent,
                    }
                }
            }
            Err(_) => Self::disabled(),
        }
    }

    fn disabled() -> Self {
        Self {
            enabled: false,
            prev: None,
            frame_seq: 0,
            log_every: 1,
            min_changed: usize::MAX,
            min_percent: f64::MAX,
        }
    }

    pub(super) fn record(&mut self, frame: &CompletedFrame<'_>) {
        if !self.enabled {
            return;
        }

        let current_buffer = frame.buffer.clone();
        self.frame_seq = self.frame_seq.saturating_add(1);

        if let Some(prev_buffer) = &self.prev {
            if self.should_log_frame() {
                if prev_buffer.area != current_buffer.area {
                    tracing::info!(
                        target: "code_tui::buffer_diff",
                        frame = self.frame_seq,
                        prev_width = prev_buffer.area.width,
                        prev_height = prev_buffer.area.height,
                        width = current_buffer.area.width,
                        height = current_buffer.area.height,
                        "Buffer area changed; skipping diff metrics for this frame"
                    );
                } else {
                    let inspected = prev_buffer.content.len().min(current_buffer.content.len());
                    let updates = prev_buffer.diff(&current_buffer);
                    let changed = updates.len();
                    if changed == 0 {
                        self.prev = Some(current_buffer);
                        return;
                    }
                    let percent = if inspected > 0 {
                        (changed as f64 / inspected as f64) * 100.0
                    } else {
                        0.0
                    };
                    if changed < self.min_changed && percent < self.min_percent {
                        self.prev = Some(current_buffer);
                        return;
                    }
                    let mut min_col = u16::MAX;
                    let mut max_col = 0u16;
                    let mut rows = BTreeSet::new();
                    let mut longest_run = 0usize;
                    let mut current_run = 0usize;
                    let mut last_cell = None;
                    for (x, y, _) in &updates {
                        min_col = min_col.min(*x);
                        max_col = max_col.max(*x);
                        rows.insert(*y);
                        match last_cell {
                            Some((last_x, last_y)) if *y == last_y && *x == last_x + 1 => {
                                current_run += 1;
                            }
                            _ => {
                                current_run = 1;
                            }
                        }
                        if current_run > longest_run {
                            longest_run = current_run;
                        }
                        last_cell = Some((*x, *y));
                    }
                    let row_min = rows.iter().copied().min().unwrap_or(0);
                    let row_max = rows.iter().copied().max().unwrap_or(0);
                    let mut spans: Vec<(u16, u16)> = Vec::new();
                    if !rows.is_empty() {
                        let mut iter = rows.iter();
                        let mut start = *iter.next().unwrap();
                        let mut prev = start;
                        for &row in iter {
                            if row == prev + 1 {
                                prev = row;
                                continue;
                            }
                            spans.push((start, prev));
                            start = row;
                            prev = row;
                        }
                        spans.push((start, prev));
                    }
                    spans.sort_by(|(a_start, a_end), (b_start, b_end)| {
                        let a_len = usize::from(*a_end) - usize::from(*a_start) + 1;
                        let b_len = usize::from(*b_end) - usize::from(*b_start) + 1;
                        b_len.cmp(&a_len)
                    });
                    let top_spans: Vec<(u16, u16)> = spans.into_iter().take(3).collect();
                    let (col_min, col_max) = if min_col == u16::MAX {
                        (0u16, 0u16)
                    } else {
                        (min_col, max_col)
                    };
                    let skipped_cells = current_buffer.content.iter().filter(|cell| cell.skip).count();
                    tracing::info!(
                        target: "code_tui::buffer_diff",
                        frame = self.frame_seq,
                        inspected,
                        changed,
                        percent = format!("{percent:.2}"),
                        width = current_buffer.area.width,
                        height = current_buffer.area.height,
                        dirty_rows = rows.len(),
                        longest_run,
                        row_min,
                        row_max,
                        col_min,
                        col_max,
                        row_spans = ?top_spans,
                        skipped_cells,
                        "Buffer diff metrics"
                    );
                }
            }
        }

        self.prev = Some(current_buffer);
    }

    fn should_log_frame(&self) -> bool {
        let interval = self.log_every.max(1) as u64;
        interval == 1 || self.frame_seq % interval == 0
    }
}

// (legacy tests removed)
#[derive(Default, Clone, Debug)]
pub(super) struct TimingStats {
    frames_drawn: u64,
    redraw_events: u64,
    key_events: u64,
    draw_ns: Vec<u64>,
    key_to_frame_ns: Vec<u64>,
    last_key_event: Option<Instant>,
    key_waiting_for_frame: bool,
}

impl TimingStats {
    pub(super) fn on_key(&mut self) {
        self.key_events = self.key_events.saturating_add(1);
        self.last_key_event = Some(Instant::now());
        self.key_waiting_for_frame = true;
    }
    pub(super) fn on_redraw_begin(&mut self) { self.redraw_events = self.redraw_events.saturating_add(1); }
    pub(super) fn on_redraw_end(&mut self, started: Instant) {
        self.frames_drawn = self.frames_drawn.saturating_add(1);
        let dt = started.elapsed().as_nanos() as u64;
        self.draw_ns.push(dt);
        if self.key_waiting_for_frame {
            if let Some(t0) = self.last_key_event.take() {
                let d = t0.elapsed().as_nanos() as u64;
                self.key_to_frame_ns.push(d);
            }
            self.key_waiting_for_frame = false;
        }
    }
    fn pct(ns: &[u64], p: f64) -> f64 {
        if ns.is_empty() { return 0.0; }
        let mut v = ns.to_vec();
        v.sort_unstable();
        let idx = ((v.len() as f64 - 1.0) * p).round() as usize;
        (v[idx] as f64) / 1_000_000.0
    }
    pub(super) fn summarize(&self) -> String {
        let draw_p50 = Self::pct(&self.draw_ns, 0.50);
        let draw_p95 = Self::pct(&self.draw_ns, 0.95);
        let kf_p50 = Self::pct(&self.key_to_frame_ns, 0.50);
        let kf_p95 = Self::pct(&self.key_to_frame_ns, 0.95);
        format!(
            "app-timing: frames={}\n  redraw_events={} key_events={}\n  draw_ms: p50={:.2} p95={:.2}\n  key->frame_ms: p50={:.2} p95={:.2}",
            self.frames_drawn,
            self.redraw_events,
            self.key_events,
            draw_p50, draw_p95,
            kf_p50, kf_p95,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::FrameTimer;
    use crate::app_event::AppEvent;
    use crate::app_event_sender::AppEventSender;
    use crate::thread_spawner;
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tracing_subscriber::filter::LevelFilter;
    use tracing_subscriber::prelude::*;

    struct SharedWriter {
        buffer: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for SharedWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let mut guard = self.buffer.lock().unwrap();
            guard.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn capture_logs(level: LevelFilter, f: impl FnOnce()) -> String {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let make_writer = {
            let buffer = Arc::clone(&buffer);
            move || SharedWriter {
                buffer: Arc::clone(&buffer),
            }
        };

        let layer = tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_ansi(false)
            .with_writer(make_writer)
            .with_filter(level);
        let subscriber = tracing_subscriber::registry().with(layer);

        tracing::dispatcher::with_default(&subscriber.into(), f);

        let guard = buffer.lock().unwrap();
        String::from_utf8_lossy(&guard).to_string()
    }

    fn count_occurrences(haystack: &str, needle: &str) -> usize {
        haystack.match_indices(needle).count()
    }

    fn saturate_background_threads() -> (Vec<std::thread::JoinHandle<()>>, Arc<AtomicBool>) {
        let stop = Arc::new(AtomicBool::new(false));
        let mut handles = Vec::new();

        loop {
            let stop_flag = Arc::clone(&stop);
            match thread_spawner::spawn_lightweight("test-blocker", move || {
                while !stop_flag.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }) {
                Some(handle) => handles.push(handle),
                None => break,
            }
        }

        (handles, stop)
    }

    #[test]
    fn frame_timer_spawn_rejection_logs_only_in_debug() {
        let (handles, stop) = saturate_background_threads();

        let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
        let app_tx = AppEventSender::new(tx);

        let warn_timer = Arc::new(FrameTimer::new());
        let warn_output = capture_logs(LevelFilter::WARN, || {
            for _ in 0..8 {
                warn_timer.schedule(Duration::from_millis(1), app_tx.clone());
            }
        });

        assert_eq!(
            count_occurrences(&warn_output, "frame timer spawn rejected"),
            0,
            "expected no warn-level frame timer spam in normal logging"
        );

        let info_timer = Arc::new(FrameTimer::new());
        let info_output = capture_logs(LevelFilter::INFO, || {
            for _ in 0..8 {
                info_timer.schedule(Duration::from_millis(1), app_tx.clone());
            }
        });

        let count = count_occurrences(&info_output, "frame timer spawn rejected");
        assert!(
            count >= 1,
            "expected debug/info logs to include frame timer rejection"
        );
        assert!(count <= 1, "expected throttling to suppress repeats");

        stop.store(true, Ordering::Relaxed);
        for handle in handles {
            let _ = handle.join();
        }
    }
}
