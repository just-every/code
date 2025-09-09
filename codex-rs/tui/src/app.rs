use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::ChatWidget;
use crate::file_search::FileSearchManager;
use crate::get_git_diff::get_git_diff;
use crate::get_login_status;
use crate::onboarding::onboarding_screen::KeyboardHandler;
use crate::onboarding::onboarding_screen::OnboardingScreen;
use crate::onboarding::onboarding_screen::OnboardingScreenArgs;
use crate::slash_command::SlashCommand;
use crate::transcript_app::TranscriptApp;
use crate::tui;
use crate::tui::TerminalInfo;
use codex_core::ConversationManager;
use codex_login::{AuthManager, AuthMode};
use codex_core::config::Config;
use codex_core::protocol::Event;
use codex_core::protocol::Op;
use color_eyre::eyre::Result;
use crossterm::SynchronizedUpdate;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::execute;
use crossterm::terminal::supports_keyboard_enhancement;
use std::path::PathBuf;
use ratatui::prelude::Rect;
use ratatui::text::Line;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;
use std::time::Instant;

/// Time window for debouncing redraw requests.
///
/// Raising this slightly helps coalesce bursts of updates during typing and
/// reduces render thrash, improving perceived input latency while staying
/// comfortably under a 60 FPS refresh budget.
const REDRAW_DEBOUNCE: Duration = Duration::from_millis(16);

/// Top-level application state: which full-screen view is currently active.
#[allow(clippy::large_enum_variant)]
enum AppState<'a> {
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

pub(crate) struct App<'a> {
    _server: Arc<ConversationManager>,
    app_event_tx: AppEventSender,
    // Split event receivers: high‑priority (input) and bulk (streaming)
    app_event_rx_high: Receiver<AppEvent>,
    app_event_rx_bulk: Receiver<AppEvent>,
    app_state: AppState<'a>,

    /// Config is stored here so we can recreate ChatWidgets as needed.
    config: Config,

    file_search: FileSearchManager,

    /// True when a redraw has been scheduled but not yet executed (debounce window).
    pending_redraw: Arc<AtomicBool>,
    /// True while a one-shot timer for a future animation frame is armed.
    /// This prevents arming multiple timers at once, while allowing timers
    /// to run independently of the short debounce used for immediate redraws.
    scheduled_frame_armed: Arc<AtomicBool>,
    /// Controls the input reader thread spawned at startup.
    input_running: Arc<AtomicBool>,

    // Transcript overlay state
    _transcript_overlay: Option<TranscriptApp>,
    _deferred_history_lines: Vec<Line<'static>>,
    _transcript_saved_viewport: Option<Rect>,

    enhanced_keys_supported: bool,

    /// Debug flag for logging LLM requests/responses
    _debug: bool,
    /// Show per-cell ordering overlay when true
    show_order_overlay: bool,

    /// Controls the animation thread that sends CommitTick events.
    commit_anim_running: Arc<AtomicBool>,

    /// Terminal information queried at startup
    terminal_info: TerminalInfo,

    /// Perform a hard clear on the first frame to ensure the entire buffer
    /// starts with our theme background. This avoids terminals that may show
    /// profile defaults until all cells are explicitly painted.
    clear_on_first_frame: bool,

    // Double‑Esc timing for backtrack/edit‑previous
    last_esc_time: Option<Instant>,

    /// If true, enable lightweight timing collection and report on exit.
    timing_enabled: bool,
    timing: TimingStats,

    /// When false, we have left the terminal's alternate screen and paused
    /// all ratatui drawing so the user can scroll/select in the regular
    /// terminal buffer. Toggle with Ctrl+T.
    alt_screen_enabled: bool,
}

/// Aggregate parameters needed to create a `ChatWidget`, as creation may be
/// deferred until after the Git warning screen is dismissed.
#[derive(Clone, Debug)]
pub(crate) struct ChatWidgetArgs {
    pub(crate) config: Config,
    initial_prompt: Option<String>,
    initial_images: Vec<PathBuf>,
    enhanced_keys_supported: bool,
    terminal_info: TerminalInfo,
    show_order_overlay: bool,
    enable_perf: bool,
}

impl App<'_> {
    pub(crate) fn new(
        config: Config,
        initial_prompt: Option<String>,
        initial_images: Vec<std::path::PathBuf>,
        show_trust_screen: bool,
        debug: bool,
        show_order_overlay: bool,
        terminal_info: TerminalInfo,
        enable_perf: bool,
    ) -> Self {
        let conversation_manager = Arc::new(ConversationManager::new(AuthManager::shared(
            config.codex_home.clone(),
            AuthMode::ApiKey,
            config.responses_originator_header.clone(),
        )));

        // Split queues so interactive input never waits behind bulk updates.
        let (high_tx, app_event_rx_high) = channel();
        let (bulk_tx, app_event_rx_bulk) = channel();
        let app_event_tx = AppEventSender::new_dual(high_tx.clone(), bulk_tx.clone());
        let pending_redraw = Arc::new(AtomicBool::new(false));
        let scheduled_frame_armed = Arc::new(AtomicBool::new(false));

        let enhanced_keys_supported = supports_keyboard_enhancement().unwrap_or(false);

        // Spawn a dedicated thread for reading the crossterm event loop and
        // re-publishing the events as AppEvents, as appropriate.
        // Create the input thread stop flag up front so we can store it on `Self`.
        let input_running = Arc::new(AtomicBool::new(true));
        {
            let app_event_tx = app_event_tx.clone();
            let input_running_thread = input_running.clone();
            std::thread::spawn(move || {
                // Track recent typing to temporarily increase poll frequency for low latency.
                let mut last_key_time = Instant::now();
                loop {
                    if !input_running_thread.load(Ordering::Relaxed) { break; }
                    // This timeout is necessary to avoid holding the event lock
                    // that crossterm::event::read() acquires. In particular,
                    // reading the cursor position (crossterm::cursor::position())
                    // needs to acquire the event lock, and so will fail if it
                    // can't acquire it within 2 sec. Resizing the terminal
                    // crashes the app if the cursor position can't be read.
                    // Keep the timeout small to minimize input-to-echo latency.
                    // Dynamically adapt poll timeout: when the user is actively typing,
                    // use a very small timeout to minimize key->echo latency; otherwise
                    // back off to reduce CPU when idle.
                    let hot_typing = Instant::now().duration_since(last_key_time) <= Duration::from_millis(250);
                    let poll_timeout = if hot_typing { Duration::from_millis(2) } else { Duration::from_millis(10) };
                    if let Ok(true) = crossterm::event::poll(poll_timeout) {
                        if let Ok(event) = crossterm::event::read() {
                            match event {
                                crossterm::event::Event::Key(key_event) => {
                                    // Forward only Press/Repeat; skip Release to avoid doubled chars on Windows.
                                    if matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                                        last_key_time = Instant::now();
                                        app_event_tx.send(AppEvent::KeyEvent(key_event));
                                    }
                                }
                                crossterm::event::Event::Resize(_, _) => {
                                    app_event_tx.send(AppEvent::RequestRedraw);
                                }
                                // When the terminal/tab regains focus, issue a redraw.
                                // Some terminals clear the alt‑screen buffer on focus switches,
                                // which can leave the status bar and inline images blank until
                                // the next resize. A focus‑gain repaint fixes this immediately.
                                crossterm::event::Event::FocusGained => {
                                    app_event_tx.send(AppEvent::RequestRedraw);
                                }
                                crossterm::event::Event::FocusLost => {
                                    // No action needed; keep state as‑is.
                                }
                                crossterm::event::Event::Paste(pasted) => {
                                    // Many terminals convert newlines to \r when pasting (e.g., iTerm2),
                                    // but tui-textarea expects \n. Normalize CR to LF.
                                    // [tui-textarea]: https://github.com/rhysd/tui-textarea/blob/4d18622eeac13b309e0ff6a55a46ac6706da68cf/src/textarea.rs#L782-L783
                                    // [iTerm2]: https://github.com/gnachman/iTerm2/blob/5d0c0d9f68523cbd0494dad5422998964a2ecd8d/sources/iTermPasteHelper.m#L206-L216
                                    let pasted = pasted.replace("\r", "\n");
                                    app_event_tx.send(AppEvent::Paste(pasted));
                                }
                                crossterm::event::Event::Mouse(mouse_event) => {
                                    app_event_tx.send(AppEvent::MouseEvent(mouse_event));
                                }
                                // All other event variants are explicitly handled above.
                            }
                        }
                    } else {
                        // Timeout expired, no `Event` is available. If the user is typing
                        // keep the loop hot; otherwise sleep briefly to cut idle CPU.
                        if !hot_typing {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                    }
                }
            });
        }

        let login_status = get_login_status(&config);
        let should_show_onboarding =
            should_show_onboarding(login_status, &config, show_trust_screen);
        let app_state = if should_show_onboarding {
            let show_login_screen = should_show_login_screen(login_status, &config);
            let chat_widget_args = ChatWidgetArgs {
                config: config.clone(),
                initial_prompt,
                initial_images,
                enhanced_keys_supported,
                terminal_info: terminal_info.clone(),
                show_order_overlay,
                enable_perf,
            };
            AppState::Onboarding {
                screen: OnboardingScreen::new(OnboardingScreenArgs {
                    event_tx: app_event_tx.clone(),
                    codex_home: config.codex_home.clone(),
                    cwd: config.cwd.clone(),
                    show_trust_screen,
                    show_login_screen,
                    chat_widget_args,
                    login_status,
                }),
            }
        } else {
            let mut chat_widget = ChatWidget::new(
                config.clone(),
                app_event_tx.clone(),
                initial_prompt,
                initial_images,
                enhanced_keys_supported,
                terminal_info.clone(),
                show_order_overlay,
            );
            chat_widget.enable_perf(enable_perf);
            // Check for initial animations after widget is created
            chat_widget.check_for_initial_animations();
            AppState::Chat {
                widget: Box::new(chat_widget),
            }
        };

        let file_search = FileSearchManager::new(config.cwd.clone(), app_event_tx.clone());
        Self {
            _server: conversation_manager,
            app_event_tx,
            app_event_rx_high,
            app_event_rx_bulk,
            app_state,
            config,
            file_search,
            pending_redraw,
            scheduled_frame_armed,
            input_running,
            _transcript_overlay: None,
            _deferred_history_lines: Vec::new(),
            _transcript_saved_viewport: None,
            enhanced_keys_supported,
            _debug: debug,
            show_order_overlay,
            commit_anim_running: Arc::new(AtomicBool::new(false)),
            terminal_info,
            clear_on_first_frame: true,
            last_esc_time: None,
            timing_enabled: enable_perf,
            timing: TimingStats::default(),
            alt_screen_enabled: true,
        }
    }


    /// Schedule a redraw immediately and open a short debounce window to coalesce
    /// subsequent requests. Crucially, even if a timer is already armed (e.g., an
    /// animation scheduled a future frame), we still trigger an immediate redraw
    /// to keep keypress echo latency low.
    #[allow(clippy::unwrap_used)]
    fn schedule_redraw(&self) {
        // Always issue a leading-edge redraw for responsiveness.
        self.app_event_tx.send(AppEvent::Redraw);

        // Arm debounce window if not already armed.
        if self
            .pending_redraw
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            let pending_redraw = self.pending_redraw.clone();
            thread::spawn(move || {
                thread::sleep(REDRAW_DEBOUNCE);
                pending_redraw.store(false, Ordering::Release);
            });
        }
    }
    
    /// Schedule a redraw after the specified duration
    fn schedule_redraw_in(&self, duration: Duration) {
        // Coalesce timers: only arm one future frame at a time. Crucially, do
        // NOT gate this on the short debounce flag used for immediate redraws,
        // otherwise animations can stall if the timer is suppressed by debounce.
        if self
            .scheduled_frame_armed
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        { return; }
        let scheduled = self.scheduled_frame_armed.clone();
        let tx = self.app_event_tx.clone();
        thread::spawn(move || {
            thread::sleep(duration);
            // Allow a subsequent timer to be armed.
            scheduled.store(false, Ordering::Release);
            tx.send(AppEvent::Redraw);
        });
    }

    pub(crate) fn run(&mut self, terminal: &mut tui::Tui) -> Result<()> {
        // Insert an event to trigger the first render.
        let app_event_tx = self.app_event_tx.clone();
        app_event_tx.send(AppEvent::RequestRedraw);

        'main: loop {
            let event = match self.next_event_priority() { Some(e) => e, None => break 'main };
            match event {
                AppEvent::InsertHistory(mut lines) => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        // Coalesce consecutive InsertHistory events to reduce redraw churn.
                        while let Ok(AppEvent::InsertHistory(mut more)) = self.app_event_rx_bulk.try_recv() {
                            lines.append(&mut more);
                        }
                        tracing::debug!("app: InsertHistory lines={}", lines.len());
                        widget.insert_history_lines(lines)
                    },
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::InsertHistoryWithKind { id, kind, lines } => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        tracing::debug!("app: InsertHistoryWithKind kind={:?} id={:?} lines={}", kind, id, lines.len());
                        widget.insert_history_lines_with_kind(kind, id, lines)
                    },
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::InsertFinalAnswer { id, lines, source } => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        tracing::debug!("app: InsertFinalAnswer id={:?} lines={} source_len={}", id, lines.len(), source.len());
                        widget.insert_final_answer_with_id(id, lines, source)
                    },
                    AppState::Onboarding { .. } => {}
                },
                // InsertBackgroundEvent removed; use InsertBackgroundEventEarly for
                // approval decisions to appear above command begin.
                AppEvent::InsertBackgroundEventEarly(message) => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        tracing::debug!("app: InsertBackgroundEventEarly len={}", message.len());
                        widget.insert_background_event_early(message);
                    }
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::RequestRedraw => {
                    self.schedule_redraw();
                }
                AppEvent::FlushPendingExecEnds => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.flush_pending_exec_ends();
                    }
                    self.schedule_redraw();
                }
                AppEvent::Redraw => {
                    if self.timing_enabled { self.timing.on_redraw_begin(); }
                    let t0 = Instant::now();
                    // When alternate screen is disabled, skip ratatui drawing to keep the
                    // regular terminal buffer visible and scrollable.
                    if self.alt_screen_enabled {
                        std::io::stdout().sync_update(|_| self.draw_next_frame(terminal))??;
                    }
                    if self.timing_enabled { self.timing.on_redraw_end(t0); }
                }
                AppEvent::StartCommitAnimation => {
                    if self
                        .commit_anim_running
                        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                        .is_ok()
                    {
                        let tx = self.app_event_tx.clone();
                        let running = self.commit_anim_running.clone();
                        let tick_ms: u64 = self
                            .config
                            .tui
                            .stream
                            .commit_tick_ms
                            .or(if self.config.tui.stream.responsive { Some(30) } else { None })
                            .unwrap_or(50);
                        thread::spawn(move || {
                            while running.load(Ordering::Relaxed) {
                                thread::sleep(Duration::from_millis(tick_ms));
                                tx.send(AppEvent::CommitTick);
                            }
                        });
                    }
                }
                AppEvent::StopCommitAnimation => {
                    self.commit_anim_running.store(false, Ordering::Release);
                }
                AppEvent::CommitTick => {
                    if self.pending_redraw.load(Ordering::Relaxed) { continue; }
                    // Advance streaming animation: commit at most one queued line
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.on_commit_tick();
                    }
                }
                AppEvent::KeyEvent(mut key_event) => {
                    if self.timing_enabled { self.timing.on_key(); }
                    // On terminals that do not support keyboard enhancement flags
                    // (notably some Windows Git Bash/mintty setups), crossterm may
                    // report only Release events. Normalize such events to Press so
                    // keys register consistently.
                    if !self.enhanced_keys_supported {
                        key_event = KeyEvent::new(key_event.code, key_event.modifiers);
                    }
                    // Reset double‑Esc timer on any non‑Esc key
                    if !matches!(key_event.code, KeyCode::Esc) {
                        self.last_esc_time = None;
                    }

                    match key_event {
                        KeyEvent { code: KeyCode::Esc, kind: KeyEventKind::Press | KeyEventKind::Repeat, .. } => {
                            // Unified Esc policy with modal-first handling:
                            // - If any modal is active, forward Esc to the widget so the modal can close itself.
                            // - Otherwise apply global Esc ordering:
                            //   1) If agent is running, stop it (even if the composer has text).
                            //   2) Else if there's text, clear it.
                            //   3) Else double‑Esc engages backtrack/edit‑previous.
                            if let AppState::Chat { widget } = &mut self.app_state {
                                // Modal-first: give active modal views priority to handle Esc.
                                if widget.has_active_modal_view() {
                                    widget.handle_key_event(key_event);
                                    continue;
                                }

                                // If a file-search popup is visible, close it first
                                // then continue with global Esc policy in the same keypress.
                                let _closed_file_popup = widget.close_file_popup_if_active();
                                {
                                    let now = Instant::now();
                                    const THRESHOLD: Duration = Duration::from_millis(600);

                                    // Step 1: stop agent if running, regardless of composer content.
                                    if widget.is_task_running() {
                                        let _ = widget.on_ctrl_c();
                                        // Arm double‑Esc so next Esc can trigger backtrack.
                                        self.last_esc_time = Some(now);
                                        continue;
                                    }

                                    // Step 2: clear composer text if present.
                                    if !widget.composer_is_empty() {
                                        widget.clear_composer();
                                        // Arm double‑Esc so a quick second Esc proceeds to step 3.
                                        self.last_esc_time = Some(now);
                                        continue;
                                    }

                                    // Step 3: backtrack via double‑Esc.
                                    if let Some(prev) = self.last_esc_time {
                                        if now.duration_since(prev) <= THRESHOLD {
                                            self.last_esc_time = None;
                                            if widget.has_pending_jump_back() {
                                                widget.undo_jump_back();
                                            } else {
                                                widget.show_edit_previous_picker();
                                            }
                                            continue;
                                        }
                                    }
                                    // First Esc in empty/idle state: show hint and arm timer.
                                    self.last_esc_time = Some(now);
                                    widget.show_esc_backtrack_hint();
                                    continue;
                                }
                            }
                            // Otherwise fall through
                        }
                        // Fallback: attempt clipboard image paste on common shortcuts.
                        // Many terminals (e.g., iTerm2) do not emit Event::Paste for raw-image
                        // clipboards. When the user presses paste shortcuts, try an image read
                        // by dispatching a paste with an empty string. The composer will then
                        // attempt `paste_image_to_temp_png()` and no-op if no image exists.
                        KeyEvent {
                            code: KeyCode::Char('v'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Press | KeyEventKind::Repeat,
                            ..
                        } => {
                            self.dispatch_paste_event(String::new());
                        }
                        KeyEvent {
                            code: KeyCode::Char('v'),
                            modifiers: m,
                            kind: KeyEventKind::Press | KeyEventKind::Repeat,
                            ..
                        } if m.contains(crossterm::event::KeyModifiers::CONTROL)
                            && m.contains(crossterm::event::KeyModifiers::SHIFT) =>
                        {
                            self.dispatch_paste_event(String::new());
                        }
                        KeyEvent {
                            code: KeyCode::Insert,
                            modifiers: crossterm::event::KeyModifiers::SHIFT,
                            kind: KeyEventKind::Press | KeyEventKind::Repeat,
                            ..
                        } => {
                            self.dispatch_paste_event(String::new());
                        }
                        KeyEvent {
                            code: KeyCode::Char('m'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Press,
                            ..
                        } => {
                            // Toggle mouse capture to allow text selection
                            use crossterm::event::DisableMouseCapture;
                            use crossterm::event::EnableMouseCapture;
                            use crossterm::execute;
                            use std::io::stdout;

                            // Static variable to track mouse capture state
                            static mut MOUSE_CAPTURE_ENABLED: bool = true;

                            unsafe {
                                MOUSE_CAPTURE_ENABLED = !MOUSE_CAPTURE_ENABLED;
                                if MOUSE_CAPTURE_ENABLED {
                                    let _ = execute!(stdout(), EnableMouseCapture);
                                } else {
                                    let _ = execute!(stdout(), DisableMouseCapture);
                                }
                            }
                            self.app_event_tx.send(AppEvent::RequestRedraw);
                        }
                        KeyEvent {
                            code: KeyCode::Char('c'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Press,
                            ..
                        } => match &mut self.app_state {
                            AppState::Chat { widget } => {
                                // Exit immediately on the second Ctrl+C instead of
                                // waiting for the backend ShutdownComplete (which
                                // can be delayed behind streaming events).
                                let handled = matches!(widget.on_ctrl_c(), crate::bottom_pane::CancellationEvent::Handled);
                                if handled { self.app_event_tx.send(AppEvent::ExitRequest); }
                            }
                            AppState::Onboarding { .. } => { self.app_event_tx.send(AppEvent::ExitRequest); }
                        },
                        KeyEvent {
                            code: KeyCode::Char('z'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Press,
                            ..
                        } => {
                            // Prefer in-app undo in Chat (composer) over shell suspend.
                            match &mut self.app_state {
                                AppState::Chat { widget } => {
                                    widget.handle_key_event(key_event);
                                    self.app_event_tx.send(AppEvent::RequestRedraw);
                                }
                                AppState::Onboarding { .. } => {
                                    #[cfg(unix)]
                                    {
                                        self.suspend(terminal)?;
                                    }
                                    // No-op on non-Unix platforms.
                                }
                            }
                        }
                        KeyEvent { code: KeyCode::Char('r'), modifiers: crossterm::event::KeyModifiers::CONTROL, kind: KeyEventKind::Press | KeyEventKind::Repeat, .. } => {
                            // Toggle reasoning/thinking visibility (Ctrl+R)
                            match &mut self.app_state {
                                AppState::Chat { widget } => { widget.toggle_reasoning_visibility(); }
                                AppState::Onboarding { .. } => {}
                            }
                        }
                        KeyEvent { code: KeyCode::Char('t'), modifiers: crossterm::event::KeyModifiers::CONTROL, kind: KeyEventKind::Press, .. } => {
                            // Toggle alternate screen mode (Ctrl+T)
                            use std::io::stdout;
                            if self.alt_screen_enabled {
                                // Leave alternate screen and disable mouse capture so the
                                // terminal scrollback and selection work normally.
                                let _ = crossterm::execute!(stdout(), crossterm::event::DisableMouseCapture);
                                let _ = crossterm::execute!(stdout(), crossterm::terminal::LeaveAlternateScreen);
                                self.alt_screen_enabled = false;
                            } else {
                                // Re-enter alternate screen and request a full redraw.
                                let _ = crossterm::execute!(stdout(), crossterm::terminal::EnterAlternateScreen);
                                // Ensure the next frame clears the back buffer so we repaint cleanly.
                                self.clear_on_first_frame = true;
                                self.alt_screen_enabled = true;
                                self.app_event_tx.send(AppEvent::RequestRedraw);
                            }
                        }
                        KeyEvent {
                            code: KeyCode::Char('d'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Press,
                            ..
                        } => {
                            // Toggle diffs overlay
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.toggle_diffs_popup();
                            }
                        }
                        // (Ctrl+Y disabled): Previously cycled syntax themes; now intentionally no-op
                        KeyEvent {
                            kind: KeyEventKind::Press | KeyEventKind::Repeat,
                            ..
                        } => {
                            self.dispatch_key_event(key_event);
                        }
                        _ => {
                            // Ignore Release key events.
                        }
                    };
                }
                AppEvent::MouseEvent(mouse_event) => {
                    self.dispatch_mouse_event(mouse_event);
                }
                AppEvent::Paste(text) => {
                    self.dispatch_paste_event(text);
                }
                AppEvent::RegisterPastedImage { placeholder, path } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.register_pasted_image(placeholder, path);
                    }
                }
                AppEvent::CodexEvent(event) => {
                    self.dispatch_codex_event(event);
                }
                AppEvent::ExitRequest => {
                    // Stop background threads and break the UI loop.
                    self.commit_anim_running.store(false, Ordering::Release);
                    self.input_running.store(false, Ordering::Release);
                    break 'main;
                }
                AppEvent::CancelRunningTask => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.cancel_running_task_from_approval();
                    }
                }
                // fallthrough handled by break
                AppEvent::CodexOp(op) => match &mut self.app_state {
                    AppState::Chat { widget } => widget.submit_op(op),
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::DispatchCommand(command, command_text) => {
                    // Persist UI-only slash commands to cross-session history.
                    // For prompt-expanding commands (/plan, /solve, /code) we let the
                    // expanded prompt be recorded by the normal submission path.
                    if !command.is_prompt_expanding() {
                        let _ = self
                            .app_event_tx
                            .send(AppEvent::CodexOp(Op::AddToHistory { text: command_text.clone() }));
                    }
                    // Extract command arguments by removing the slash command from the beginning
                    // e.g., "/browser status" -> "status", "/chrome 9222" -> "9222"
                    let command_args = {
                        let cmd_with_slash = format!("/{}", command.command());
                        if command_text.starts_with(&cmd_with_slash) {
                            command_text[cmd_with_slash.len()..].trim().to_string()
                        } else {
                            // Fallback: if format doesn't match, use the full text
                            command_text.clone()
                        }
                    };

                    match command {
                        SlashCommand::Branch => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_branch_command(command_args);
                            }
                        }
                        SlashCommand::Resume => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.show_resume_picker();
                            }
                        }
                        SlashCommand::New => {
                            // Clear the current conversation and start fresh
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.new_conversation(self.enhanced_keys_supported);
                            } else {
                        // If we're not in chat state, create a new chat widget
                        let mut new_widget = ChatWidget::new(
                            self.config.clone(),
                            self.app_event_tx.clone(),
                            None,
                            Vec::new(),
                            self.enhanced_keys_supported,
                            self.terminal_info.clone(),
                            self.show_order_overlay,
                        );
                        new_widget.enable_perf(self.timing_enabled);
                        self.app_state = AppState::Chat { widget: Box::new(new_widget) };
                            }
                            self.app_event_tx.send(AppEvent::RequestRedraw);
                        }
                        SlashCommand::Init => {
                            // Guard: do not run if a task is active.
                            if let AppState::Chat { widget } = &mut self.app_state {
                                const INIT_PROMPT: &str =
                                    include_str!("../prompt_for_init_command.md");
                                widget.submit_text_message(INIT_PROMPT.to_string());
                            }
                        }
                        SlashCommand::Compact => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.clear_token_usage();
                                self.app_event_tx.send(AppEvent::CodexOp(Op::Compact));
                            }
                        }
                        SlashCommand::Quit => { break 'main; }
                        SlashCommand::Logout => {
                            if let Err(e) = codex_login::logout(&self.config.codex_home) { tracing::error!("failed to logout: {e}"); }
                            break 'main;
                        }
                        SlashCommand::Diff => {
                            let tx = self.app_event_tx.clone();
                            tokio::spawn(async move {
                                match get_git_diff().await {
                                    Ok((is_git_repo, diff_text)) => {
                                        let text = if is_git_repo {
                                            diff_text
                                        } else {
                                            "`/diff` — _not inside a git repository_".to_string()
                                        };
                                        tx.send(AppEvent::DiffResult(text));
                                    }
                                    Err(e) => {
                                        tx.send(AppEvent::DiffResult(format!("Failed to compute diff: {e}")));
                                    }
                                }
                            });
                        }
                        SlashCommand::Mention => {
                            // The mention feature is handled differently in our fork
                            // For now, just add @ to the composer
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.insert_str("@");
                            }
                        }
                        SlashCommand::Status => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.add_status_output();
                            }
                        }
                        SlashCommand::Agents => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.add_agents_output();
                            }
                        }
                        SlashCommand::Github => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_github_command(command_args);
                            }
                        }
                        SlashCommand::Mcp => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_mcp_command(command_args);
                            }
                        }
                        SlashCommand::Reasoning => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_reasoning_command(command_args);
                            }
                        }
                        SlashCommand::Verbosity => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_verbosity_command(command_args);
                            }
                        }
                        SlashCommand::Theme => {
                            // Theme selection is handled in submit_user_message
                            // This case is here for completeness
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.show_theme_selection();
                            }
                        }
                        SlashCommand::Prompts => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.add_prompts_output();
                            }
                        }
                        SlashCommand::Perf => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_perf_command(command_args);
                            }
                        }
                        // Prompt-expanding commands should have been handled in submit_user_message
                        // but add a fallback just in case
                        SlashCommand::Plan | SlashCommand::Solve | SlashCommand::Code => {
                            // These should have been expanded already, but handle them anyway
                            if let AppState::Chat { widget } = &mut self.app_state {
                                let expanded = command.expand_prompt(&command_text);
                                if let Some(prompt) = expanded {
                                    widget.submit_text_message(prompt);
                                }
                            }
                        }
                        SlashCommand::Browser => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_browser_command(command_args);
                            }
                        }
                        SlashCommand::Chrome => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                tracing::info!("[cdp] /chrome invoked, args='{}'", command_args);
                                widget.handle_chrome_command(command_args);
                            }
                        }
                        #[cfg(debug_assertions)]
                        SlashCommand::TestApproval => {
                            use codex_core::protocol::EventMsg;
                            use std::collections::HashMap;

                            use codex_core::protocol::ApplyPatchApprovalRequestEvent;
                            use codex_core::protocol::FileChange;

                            self.app_event_tx.send(AppEvent::CodexEvent(Event {
                                id: "1".to_string(),
                                event_seq: 0,
                                // msg: EventMsg::ExecApprovalRequest(ExecApprovalRequestEvent {
                                //     call_id: "1".to_string(),
                                //     command: vec!["git".into(), "apply".into()],
                                //     cwd: self.config.cwd.clone(),
                                //     reason: Some("test".to_string()),
                                // }),
                                msg: EventMsg::ApplyPatchApprovalRequest(
                                    ApplyPatchApprovalRequestEvent {
                                        call_id: "1".to_string(),
                                        changes: HashMap::from([
                                            (
                                                PathBuf::from("/tmp/test.txt"),
                                                FileChange::Add {
                                                    content: "test".to_string(),
                                                },
                                            ),
                                            (
                                                PathBuf::from("/tmp/test2.txt"),
                                                FileChange::Update {
                                                    unified_diff: "+test\n-test2".to_string(),
                                                    move_path: None,
                                                },
                                            ),
                                        ]),
                                        reason: None,
                                        grant_root: Some(PathBuf::from("/tmp")),
                                    },
                                ),
                                order: None,
                            }));
                        }
                    }
                }
                AppEvent::SwitchCwd(new_cwd, initial_prompt) => {
                    // Rebuild the chat widget bound to a new cwd, preserving
                    // current configuration and terminal properties.
                    let mut cfg = self.config.clone();
                    cfg.cwd = new_cwd.clone();
                    let mut new_widget = ChatWidget::new(
                        cfg,
                        self.app_event_tx.clone(),
                        None,
                        Vec::new(),
                        self.enhanced_keys_supported,
                        self.terminal_info.clone(),
                        self.show_order_overlay,
                    );
                    new_widget.enable_perf(self.timing_enabled);
                    self.app_state = AppState::Chat { widget: Box::new(new_widget) };

                    // Surface a BackgroundEvent so the user can see the effective cwd
                    // in the new session before any prompt runs.
                    {
                        use codex_core::protocol::{BackgroundEventEvent, Event, EventMsg};
                        let msg = format!("✅ Switched to worktree: {}", new_cwd.display());
                        let _ = self.app_event_tx.send(AppEvent::CodexEvent(Event {
                            id: "switch-cwd".to_string(),
                            event_seq: 0,
                            msg: EventMsg::BackgroundEvent(BackgroundEventEvent { message: msg }),
                            order: None,
                        }));
                    }
                    // Optionally submit a prompt immediately in the new session
                    if let AppState::Chat { widget } = &mut self.app_state {
                        if let Some(prompt) = initial_prompt {
                            if !prompt.is_empty() {
                                let preface = "[internal] When you finish this task, ask the user if they want any changes. If they are happy, offer to merge the branch back into the repository's default branch and delete the worktree. Use '/branch finalize' (or an equivalent git worktree remove + switch) rather than deleting the folder directly so the UI can switch back cleanly. Wait for explicit confirmation before merging.".to_string();
                                widget.submit_text_message_with_preface(prompt, preface);
                            }
                        }
                    }
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                }
                AppEvent::ResumeFrom(path) => {
                    // Replace the current chat widget with a new one configured to resume
                    let mut cfg = self.config.clone();
                    cfg.experimental_resume = Some(path);
                    if let AppState::Chat { .. } = &self.app_state {
                        let mut new_widget = ChatWidget::new(
                            cfg,
                            self.app_event_tx.clone(),
                            None,
                            Vec::new(),
                            self.enhanced_keys_supported,
                            self.terminal_info.clone(),
                            self.show_order_overlay,
                        );
                        new_widget.enable_perf(self.timing_enabled);
                        self.app_state = AppState::Chat { widget: Box::new(new_widget) };
                        self.app_event_tx.send(AppEvent::RequestRedraw);
                    }
                }
                AppEvent::PrepareAgents => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.prepare_agents();
                    }
                }
                AppEvent::UpdateReasoningEffort(new_effort) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_reasoning_effort(new_effort);
                    }
                }
                AppEvent::UpdateTextVerbosity(new_verbosity) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_text_verbosity(new_verbosity);
                    }
                }
                AppEvent::UpdateGithubWatcher(enabled) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_github_watcher(enabled);
                    }
                }
                AppEvent::UpdateMcpServer { name, enable } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.toggle_mcp_server(&name, enable);
                    }
                }
                AppEvent::PrefillComposer(text) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.insert_str(&text);
                    }
                }
                AppEvent::DiffResult(text) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.add_diff_output(text);
                    }
                }
                AppEvent::UpdateTheme(new_theme) => {
                    // Switch the theme immediately
                    crate::theme::switch_theme(new_theme);

                    // Clear terminal with new theme colors
                    let theme_bg = crate::colors::background();
                    let theme_fg = crate::colors::text();
                    let _ = crossterm::execute!(
                        std::io::stdout(),
                        crossterm::style::SetColors(crossterm::style::Colors::new(
                            theme_fg.into(),
                            theme_bg.into()
                        )),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                        crossterm::cursor::MoveTo(0, 0),
                        crossterm::terminal::SetTitle("Code"),
                        crossterm::terminal::EnableLineWrap
                    );

                    // Update config and save to file
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_theme(new_theme);
                    }

                    // Force a full redraw on the next frame so the entire
                    // ratatui back buffer is cleared and repainted with the
                    // new theme. This avoids any stale cells lingering on
                    // terminals that preserve previous cell attributes.
                    self.clear_on_first_frame = true;
                    self.schedule_redraw();
                }
                AppEvent::PreviewTheme(new_theme) => {
                    // Switch the theme immediately for preview (no history event)
                    crate::theme::switch_theme(new_theme);

                    // Clear terminal with new theme colors
                    let theme_bg = crate::colors::background();
                    let theme_fg = crate::colors::text();
                    let _ = crossterm::execute!(
                        std::io::stdout(),
                        crossterm::style::SetColors(crossterm::style::Colors::new(
                            theme_fg.into(),
                            theme_bg.into()
                        )),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                        crossterm::cursor::MoveTo(0, 0),
                        crossterm::terminal::SetTitle("Code"),
                        crossterm::terminal::EnableLineWrap
                    );

                    // Retint pre-rendered history cells so the preview reflects immediately
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.retint_history_for_preview();
                    }

                    // Don't update config or add to history for previews
                    // Force a full redraw so previews repaint cleanly as you cycle
                    self.clear_on_first_frame = true;
                    self.schedule_redraw();
                }
                AppEvent::ComposerExpanded => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.on_composer_expanded();
                    }
                    self.schedule_redraw();
                }
                AppEvent::CycleAccessMode => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.cycle_access_mode();
                    }
                    self.schedule_redraw();
                }
                AppEvent::OnboardingAuthComplete(result) => {
                    if let AppState::Onboarding { screen } = &mut self.app_state {
                        screen.on_auth_complete(result);
                    }
                }
                AppEvent::OnboardingComplete(ChatWidgetArgs {
                    config,
                    enhanced_keys_supported,
                    initial_images,
                    initial_prompt,
                    terminal_info,
                    show_order_overlay,
                    enable_perf,
                }) => {
                    let mut w = ChatWidget::new(
                        config,
                        app_event_tx.clone(),
                        initial_prompt,
                        initial_images,
                        enhanced_keys_supported,
                        terminal_info,
                        show_order_overlay,
                    );
                    w.enable_perf(enable_perf);
                    self.app_state = AppState::Chat { widget: Box::new(w) }
                }
                AppEvent::StartFileSearch(query) => {
                    if !query.is_empty() {
                        self.file_search.on_user_query(query);
                    }
                }
                AppEvent::FileSearchResult { query, matches } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_file_search_result(query, matches);
                    }
                }
                AppEvent::ShowChromeOptions(port) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_chrome_options(port);
                    }
                }
                AppEvent::ChromeLaunchOptionSelected(option, port) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_chrome_launch_option(option, port);
                    }
                }
                AppEvent::JumpBack { nth, prefill } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        // Build response items from current UI history
                        let items = widget.export_response_items();
                        let cfg = widget.config_ref().clone();

                        // Compute prefix up to selected user message now
                        let prefix_items = {
                            let mut user_seen = 0usize;
                            let mut cut = items.len();
                            for (idx, it) in items.iter().enumerate().rev() {
                                if let codex_protocol::models::ResponseItem::Message { role, .. } = it {
                                    if role == "user" {
                                        user_seen += 1;
                                        if user_seen == nth { cut = idx; break; }
                                    }
                                }
                            }
                            items.iter().take(cut).cloned().collect::<Vec<_>>()
                        };

                        // Perform the fork off the UI thread to avoid nested runtimes
                        let server = self._server.clone();
                        let tx = self.app_event_tx.clone();
                        let prefill_clone = prefill.clone();
                        std::thread::spawn(move || {
                            let rt = tokio::runtime::Builder::new_multi_thread()
                                .enable_all()
                                .build()
                                .expect("build tokio runtime");
                            // Clone cfg for the async block to keep original for the event
                            let cfg_for_rt = cfg.clone();
                            let result = rt.block_on(async move {
                                server.fork_conversation(items, nth, cfg_for_rt).await
                            });
                            if let Ok(new_conv) = result {
                                tx.send(AppEvent::JumpBackForked { cfg, new_conv: crate::app_event::Redacted(new_conv), prefix_items, prefill: prefill_clone });
                            } else if let Err(e) = result {
                                tracing::error!("error forking conversation: {e:#}");
                            }
                        });
                    }
                }
                AppEvent::JumpBackForked { cfg, new_conv, prefix_items, prefill } => {
                    // Replace widget with a new one bound to the forked conversation
                    let session_conf = new_conv.0.session_configured.clone();
                    let conv = new_conv.0.conversation.clone();
                    let mut new_widget = ChatWidget::new_from_existing(
                        cfg,
                        conv,
                        session_conf,
                        self.app_event_tx.clone(),
                        self.enhanced_keys_supported,
                        self.terminal_info.clone(),
                        self.show_order_overlay,
                    );
                    new_widget.enable_perf(self.timing_enabled);
                    // Ensure any initial animations or status are set up on the fresh widget
                    new_widget.check_for_initial_animations();

                    self.app_state = AppState::Chat { widget: Box::new(new_widget) };
                    // Reset any transient state from the previous widget/session
                    self.commit_anim_running.store(false, Ordering::Release);
                    self.last_esc_time = None;
                    // Force a clean repaint of the new UI state
                    self.clear_on_first_frame = true;

                    // Replay prefix to the UI
                    let ev = codex_core::protocol::Event {
                        id: "fork".to_string(),
                        event_seq: 0,
                        msg: codex_core::protocol::EventMsg::ReplayHistory(
                            codex_core::protocol::ReplayHistoryEvent { items: prefix_items }
                        ),
                        order: None,
                    };
                    self.app_event_tx.send(AppEvent::CodexEvent(ev));

                    // Prefill composer with the edited text
                    if let AppState::Chat { widget } = &mut self.app_state {
                        if !prefill.is_empty() { widget.insert_str(&prefill); }
                    }
                    self.app_event_tx.send(AppEvent::RequestRedraw);
                }
                AppEvent::ScheduleFrameIn(duration) => {
                    // Schedule the next redraw with the requested duration
                    self.schedule_redraw_in(duration);
                }
            }
        }
        terminal.clear()?;

        Ok(())
    }

    /// Pull the next event with priority for interactive input.
    /// Never returns None due to idleness; only returns None if both channels disconnect.
    fn next_event_priority(&self) -> Option<AppEvent> {
        use std::sync::mpsc::RecvTimeoutError::{Timeout, Disconnected};
        loop {
            if let Ok(ev) = self.app_event_rx_high.try_recv() { return Some(ev); }
            if let Ok(ev) = self.app_event_rx_bulk.try_recv() { return Some(ev); }
            match self.app_event_rx_high.recv_timeout(Duration::from_millis(10)) {
                Ok(ev) => return Some(ev),
                Err(Timeout) => continue,
                Err(Disconnected) => break,
            }
        }
        // High channel disconnected; try blocking on bulk as a last resort
        self.app_event_rx_bulk.recv().ok()
    }

    #[cfg(unix)]
    fn suspend(&mut self, terminal: &mut tui::Tui) -> Result<()> {
        tui::restore()?;
        // SAFETY: Unix-only code path. We intentionally send SIGTSTP to the
        // current process group (pid 0) to trigger standard job-control
        // suspension semantics. This FFI does not involve any raw pointers,
        // is not called from a signal handler, and uses a constant signal.
        // Errors from kill are acceptable (e.g., if already stopped) — the
        // subsequent re-init path will still leave the terminal in a good state.
        // We considered `nix`, but didn't think it was worth pulling in for this one call.
        unsafe { libc::kill(0, libc::SIGTSTP) };
        let (new_terminal, new_terminal_info) = tui::init(&self.config)?;
        *terminal = new_terminal;
        self.terminal_info = new_terminal_info;
        terminal.clear()?;
        self.app_event_tx.send(AppEvent::RequestRedraw);
        Ok(())
    }

    pub(crate) fn token_usage(&self) -> codex_core::protocol::TokenUsage {
        let usage = match &self.app_state {
            AppState::Chat { widget } => widget.token_usage().clone(),
            AppState::Onboarding { .. } => codex_core::protocol::TokenUsage::default(),
        };
        // ensure background helpers stop before returning
        self.commit_anim_running.store(false, Ordering::Release);
        self.input_running.store(false, Ordering::Release);
        usage
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

    fn draw_next_frame(&mut self, terminal: &mut tui::Tui) -> Result<()> {
        if self.clear_on_first_frame || matches!(self.app_state, AppState::Onboarding { .. }) {
            terminal.clear()?;
            self.clear_on_first_frame = false;
        }

        // Terminal resize handling - simplified version since private fields aren't accessible
        // The terminal will handle resize events internally
        let _screen_size = terminal.size()?;

        terminal.draw(|frame| {
            match &mut self.app_state {
                AppState::Chat { widget } => {
                    if let Some((x, y)) = widget.cursor_pos(frame.area()) {
                        frame.set_cursor_position((x, y));
                    }
                    frame.render_widget_ref(&**widget, frame.area())
                }
                AppState::Onboarding { screen } => frame.render_widget_ref(&*screen, frame.area()),
            }
        })?;
        Ok(())
    }

    /// Dispatch a KeyEvent to the current view and let it decide what to do
    /// with it.
    fn dispatch_key_event(&mut self, key_event: KeyEvent) {
        match &mut self.app_state {
            AppState::Chat { widget } => {
                widget.handle_key_event(key_event);
            }
            AppState::Onboarding { screen } => match key_event.code {
                KeyCode::Char('q') => {
                    self.app_event_tx.send(AppEvent::ExitRequest);
                }
                _ => screen.handle_key_event(key_event),
            },
        }
    }

    fn dispatch_paste_event(&mut self, pasted: String) {
        match &mut self.app_state {
            AppState::Chat { widget } => widget.handle_paste(pasted),
            AppState::Onboarding { .. } => {}
        }
    }

    fn dispatch_mouse_event(&mut self, mouse_event: crossterm::event::MouseEvent) {
        match &mut self.app_state {
            AppState::Chat { widget } => {
                widget.handle_mouse_event(mouse_event);
            }
            AppState::Onboarding { .. } => {}
        }
    }

    fn dispatch_codex_event(&mut self, event: Event) {
        match &mut self.app_state {
            AppState::Chat { widget } => widget.handle_codex_event(event),
            AppState::Onboarding { .. } => {}
        }
    }

}

fn should_show_onboarding(
    _login_status: crate::LoginStatus,
    _config: &Config,
    show_trust_screen: bool,
) -> bool {
    if show_trust_screen {
        return true;
    }
    // Defer login screen visibility decision to onboarding screen logic.
    // Here we only gate on trust flow.
    false
}

fn should_show_login_screen(login_status: crate::LoginStatus, _config: &Config) -> bool {
    matches!(login_status, crate::LoginStatus::NotAuthenticated)
}

// (legacy tests removed)
#[derive(Default, Clone, Debug)]
struct TimingStats {
    frames_drawn: u64,
    redraw_events: u64,
    key_events: u64,
    draw_ns: Vec<u64>,
    key_to_frame_ns: Vec<u64>,
    last_key_event: Option<Instant>,
    key_waiting_for_frame: bool,
}

impl TimingStats {
    fn on_key(&mut self) {
        self.key_events = self.key_events.saturating_add(1);
        self.last_key_event = Some(Instant::now());
        self.key_waiting_for_frame = true;
    }
    fn on_redraw_begin(&mut self) { self.redraw_events = self.redraw_events.saturating_add(1); }
    fn on_redraw_end(&mut self, started: Instant) {
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
    fn summarize(&self) -> String {
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
