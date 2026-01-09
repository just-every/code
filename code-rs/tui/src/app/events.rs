use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::{Duration, Instant};

use color_eyre::eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::SynchronizedUpdate;

use code_cloud_tasks_client::{CloudTaskError, TaskId};
use code_core::config::add_project_allowed_command;
use code_core::config_types::Notifications;
use code_core::protocol::{Event, Op, SandboxPolicy};
use code_login::{AuthManager, AuthMode, ServerOptions};
use portable_pty::PtySize;

use crate::app_event::AppEvent;
use crate::bottom_pane::SettingsSection;
use crate::chatwidget::ChatWidget;
use crate::cloud_tasks_service;
use crate::exec_command::strip_bash_lc_and_escape;
use crate::get_git_diff::get_git_diff;
use crate::history_cell;
use crate::slash_command::SlashCommand;
use crate::thread_spawner;
use crate::tui;

use super::render::flatten_draw_result;
use super::state::{
    App,
    AppState,
    ChatWidgetArgs,
    LoginFlowState,
    BACKPRESSURE_FORCED_DRAW_SKIPS,
    HIGH_EVENT_BURST_MAX,
};

impl App<'_> {
    fn handle_login_mode_change(&mut self, using_chatgpt_auth: bool) {
        self.config.using_chatgpt_auth = using_chatgpt_auth;
        if let AppState::Chat { widget } = &mut self.app_state {
            widget.set_using_chatgpt_auth(using_chatgpt_auth);
            let _ = widget.reload_auth();
        }

        self.spawn_remote_model_discovery();
    }

    fn spawn_remote_model_discovery(&self) {
        if crate::chatwidget::is_test_mode() {
            return;
        }
        let remote_tx = self.app_event_tx.clone();
        let remote_auth_manager = self._server.auth_manager();
        let remote_provider = self.config.model_provider.clone();
        let remote_code_home = self.config.code_home.clone();
        let remote_using_chatgpt_hint = self.config.using_chatgpt_auth;
        tokio::spawn(async move {
            let remote_manager = code_core::remote_models::RemoteModelsManager::new(
                remote_auth_manager.clone(),
                remote_provider,
                remote_code_home,
            );
            remote_manager.refresh_remote_models().await;
            let remote_models = remote_manager.remote_models_snapshot().await;
            if remote_models.is_empty() {
                return;
            }

            let auth_mode = remote_auth_manager
                .auth()
                .map(|auth| auth.mode)
                .or_else(|| {
                    if remote_using_chatgpt_hint {
                        Some(code_protocol::mcp_protocol::AuthMode::ChatGPT)
                    } else {
                        Some(code_protocol::mcp_protocol::AuthMode::ApiKey)
                    }
                });
            let presets = code_common::model_presets::builtin_model_presets(auth_mode);
            let presets = crate::remote_model_presets::merge_remote_models(remote_models, presets);
            let default_model = remote_manager.default_model_slug(auth_mode).await;
            remote_tx.send(AppEvent::ModelPresetsUpdated {
                presets,
                default_model,
            });
        });
    }

    pub(crate) fn run(&mut self, terminal: &mut tui::Tui) -> Result<()> {
        // Insert an event to trigger the first render.
        let app_event_tx = self.app_event_tx.clone();
        app_event_tx.send(AppEvent::RequestRedraw);
        // Some Windows/macOS terminals report an initial size that stabilizes
        // shortly after entering the alt screen. Schedule one follow‑up frame
        // to catch any late size change without polling.
        app_event_tx.send(AppEvent::ScheduleFrameIn(Duration::from_millis(120)));

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
                        if self.alt_screen_active {
                            widget.insert_history_lines(lines)
                        } else {
                            use std::io::stdout;
                            // Compute desired bottom height now, so growing/shrinking input
                            // adjusts the reserved region immediately even before the next frame.
                            let width = terminal.size().map(|s| s.width).unwrap_or(80);
                            let reserve = widget.desired_bottom_height(width).max(1);
                            let _ = execute!(stdout(), crossterm::terminal::BeginSynchronizedUpdate);
                            crate::insert_history::insert_history_lines_above(terminal, reserve, lines);
                            let _ = execute!(stdout(), crossterm::terminal::EndSynchronizedUpdate);
                            self.schedule_redraw();
                        }
                    },
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::InsertHistoryWithKind { id, kind, lines } => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        tracing::debug!("app: InsertHistoryWithKind kind={:?} id={:?} lines={}", kind, id, lines.len());
                        // Always update widget history, even in terminal mode.
                        // In terminal mode, the widget will emit an InsertHistory event
                        // which we will mirror to scrollback in the handler above.
                        let to_mirror = lines.clone();
                        widget.insert_history_lines_with_kind(kind, id, lines);
                        if !self.alt_screen_active {
                            use std::io::stdout;
                            let width = terminal.size().map(|s| s.width).unwrap_or(80);
                            let reserve = widget.desired_bottom_height(width).max(1);
                            let _ = execute!(stdout(), crossterm::terminal::BeginSynchronizedUpdate);
                            crate::insert_history::insert_history_lines_above(terminal, reserve, to_mirror);
                            let _ = execute!(stdout(), crossterm::terminal::EndSynchronizedUpdate);
                            self.schedule_redraw();
                        }
                    },
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::InsertFinalAnswer { id, lines, source } => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        tracing::debug!("app: InsertFinalAnswer id={:?} lines={} source_len={}", id, lines.len(), source.len());
                        let to_mirror = lines.clone();
                        widget.insert_final_answer_with_id(id, lines, source);
                        if !self.alt_screen_active {
                            use std::io::stdout;
                            let width = terminal.size().map(|s| s.width).unwrap_or(80);
                            let reserve = widget.desired_bottom_height(width).max(1);
                            let _ = execute!(stdout(), crossterm::terminal::BeginSynchronizedUpdate);
                            crate::insert_history::insert_history_lines_above(terminal, reserve, to_mirror);
                            let _ = execute!(stdout(), crossterm::terminal::EndSynchronizedUpdate);
                            self.schedule_redraw();
                        }
                    },
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::InsertBackgroundEvent { message, placement, order } => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        tracing::debug!(
                            "app: InsertBackgroundEvent placement={:?} len={}",
                            placement,
                            message.len()
                        );
                        widget.insert_background_event_with_placement(message, placement, order);
                    }
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::AutoUpgradeCompleted { version } => match &mut self.app_state {
                    AppState::Chat { widget } => widget.on_auto_upgrade_completed(version),
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::RateLimitFetchFailed { message } => match &mut self.app_state {
                    AppState::Chat { widget } => widget.on_rate_limit_refresh_failed(message),
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::RateLimitSnapshotStored { account_id } => match &mut self.app_state {
                    AppState::Chat { widget } => {
                        widget.on_rate_limit_snapshot_stored(account_id)
                    }
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::RequestRedraw => {
                    self.schedule_redraw();
                }
                AppEvent::ModelPresetsUpdated { presets, default_model } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.update_model_presets(presets, default_model);
                    }
                    self.schedule_redraw();
                }
                AppEvent::UpdatePlanningUseChatModel(use_chat) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_planning_use_chat_model(use_chat);
                    }
                    self.schedule_redraw();
                }
                AppEvent::FlushPendingExecEnds => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.flush_pending_exec_ends();
                    }
                    self.schedule_redraw();
                }
                AppEvent::SyncHistoryVirtualization => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.sync_history_virtualization();
                    }
                    self.schedule_redraw();
                }
                AppEvent::FlushInterruptsIfIdle => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.flush_interrupts_if_stream_idle();
                    }
                }
                AppEvent::Redraw => {
                    if self.timing_enabled { self.timing.on_redraw_begin(); }
                    let t0 = Instant::now();
                    let mut used_nonblocking = false;
                    let draw_result = if !tui::stdout_ready_for_writes() {
                        self.stdout_backpressure_skips = self.stdout_backpressure_skips.saturating_add(1);
                        if self.stdout_backpressure_skips == 1
                            || self.stdout_backpressure_skips % 25 == 0
                        {
                            tracing::warn!(
                                skips = self.stdout_backpressure_skips,
                                "stdout not writable; deferring redraw to avoid blocking"
                            );
                        }

                        if self.stdout_backpressure_skips < BACKPRESSURE_FORCED_DRAW_SKIPS {
                            self.redraw_inflight.store(false, Ordering::Release);
                            self.app_event_tx
                                .send(AppEvent::ScheduleFrameIn(Duration::from_millis(120)));
                            continue;
                        }

                        used_nonblocking = true;
                        tracing::warn!(
                            skips = self.stdout_backpressure_skips,
                            "stdout still blocked; forcing nonblocking redraw"
                        );
                        self.draw_frame_with_nonblocking_stdout(terminal)
                    } else {
                        self.stdout_backpressure_skips = 0;
                        std::io::stdout().sync_update(|_| self.draw_next_frame(terminal))
                    };

                    self.redraw_inflight.store(false, Ordering::Release);
                    let needs_follow_up = self.post_frame_redraw.swap(false, Ordering::AcqRel);
                    if needs_follow_up {
                        self.schedule_redraw();
                    }

                    match flatten_draw_result(draw_result) {
                        Ok(()) => {
                            self.stdout_backpressure_skips = 0;
                            if self.timing_enabled { self.timing.on_redraw_end(t0); }
                        }
                        Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            // A draw can fail after partially writing to the terminal. In that case,
                            // the terminal contents may no longer match ratatui's back buffer, and
                            // subsequent diff-based draws may not fully repair stale tail lines.
                            // Force a clear on the next successful frame to resynchronize.
                            self.clear_on_first_frame = true;

                            // Also force the next successful draw to repaint the entire screen by
                            // invalidating ratatui's notion of the "current" buffer. This avoids
                            // cases where a partially-applied frame leaves stale glyphs visible but
                            // the back buffer thinks the terminal is already up to date.
                            terminal.swap_buffers();
                            // Non‑blocking draw hit backpressure; try again shortly.
                            if used_nonblocking {
                                tracing::debug!("nonblocking redraw hit WouldBlock; rescheduling");
                            }
                            self.app_event_tx
                                .send(AppEvent::ScheduleFrameIn(Duration::from_millis(120)));
                            continue;
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                AppEvent::StartCommitAnimation => {
                    if self
                        .commit_anim_running
                        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                        .is_ok()
                    {
                        let tx = self.app_event_tx.clone();
                        let running = self.commit_anim_running.clone();
                        let running_for_thread = running.clone();
                        let tick_ms: u64 = self
                            .config
                            .tui
                            .stream
                            .commit_tick_ms
                            .or(if self.config.tui.stream.responsive { Some(30) } else { None })
                            .unwrap_or(50);
                        if thread_spawner::spawn_lightweight("commit-anim", move || {
                            while running_for_thread.load(Ordering::Relaxed) {
                                thread::sleep(Duration::from_millis(tick_ms));
                                tx.send(AppEvent::CommitTick);
                            }
                        })
                        .is_none()
                        {
                            running.store(false, Ordering::Release);
                        }
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
                    #[cfg(windows)]
                    {
                        use crossterm::event::KeyCode;
                        use crossterm::event::KeyEventKind;
                        if matches!(key_event.kind, KeyEventKind::Repeat) {
                            match key_event.code {
                                KeyCode::Left
                                | KeyCode::Right
                                | KeyCode::Up
                                | KeyCode::Down
                                | KeyCode::Home
                                | KeyCode::End
                                | KeyCode::Backspace
                                | KeyCode::Delete => {}
                                _ => continue,
                            }
                        }
                    }
                    // On terminals without keyboard enhancement flags (notably some Windows
                    // Git Bash/mintty setups), crossterm may emit duplicate key-up events or
                    // only report releases. Track which keys were seen as pressed so matching
                    // releases can be dropped, and synthesize a press when a release arrives
                    // without a prior press.
                    if !self.enhanced_keys_supported {
                        let key_code = key_event.code.clone();
                        match key_event.kind {
                            KeyEventKind::Press | KeyEventKind::Repeat => {
                                self.non_enhanced_pressed_keys.insert(key_code);
                            }
                            KeyEventKind::Release => {
                                if self.non_enhanced_pressed_keys.remove(&key_code) {
                                    continue;
                                }

                                let mut release_handled = false;
                                if let KeyCode::Char(ch) = key_code {
                                    let alts: Vec<char> = ch
                                        .to_lowercase()
                                        .chain(ch.to_uppercase())
                                        .filter(|&c| c != ch)
                                        .collect();

                                    for alt in alts {
                                        if self
                                            .non_enhanced_pressed_keys
                                            .remove(&KeyCode::Char(alt))
                                        {
                                            release_handled = true;
                                            break;
                                        }
                                    }
                                }

                                if release_handled {
                                    continue;
                                }

                                key_event = KeyEvent::new(
                                    Self::normalize_non_enhanced_release_code(key_event.code),
                                    key_event.modifiers,
                                );
                            }
                        }
                    }
                    // Reset double‑Esc timer on any non‑Esc key
                    if !matches!(key_event.code, KeyCode::Esc) {
                        self.last_esc_time = None;
                    }

                    match key_event {
                        KeyEvent { code: KeyCode::Esc, kind: KeyEventKind::Press | KeyEventKind::Repeat, .. } => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                if widget.handle_app_esc(key_event, &mut self.last_esc_time) {
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
                                match widget.on_ctrl_c() {
                                    crate::bottom_pane::CancellationEvent::Handled => {
                                        if widget.ctrl_c_requests_exit() {
                                            self.app_event_tx.send(AppEvent::ExitRequest);
                                        }
                                    }
                                    crate::bottom_pane::CancellationEvent::Ignored => {}
                                }
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
                        KeyEvent {
                            code: KeyCode::Char('r'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Press,
                            ..
                        }
                        | KeyEvent {
                            code: KeyCode::Char('r'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Repeat,
                            ..
                        } => {
                            // Toggle reasoning/thinking visibility (Ctrl+R)
                            match &mut self.app_state {
                                AppState::Chat { widget } => {
                                    widget.toggle_reasoning_visibility();
                                }
                                AppState::Onboarding { .. } => {}
                            }
                        }
                        KeyEvent {
                            code: KeyCode::Char('c'),
                            modifiers,
                            kind: KeyEventKind::Press,
                            ..
                        } if modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
                            && modifiers.contains(crossterm::event::KeyModifiers::SHIFT) =>
                        {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.toggle_context_expansion();
                            }
                        }
                        KeyEvent {
                            code: KeyCode::Char('t'),
                            modifiers: crossterm::event::KeyModifiers::CONTROL,
                            kind: KeyEventKind::Press | KeyEventKind::Repeat,
                            ..
                        } => {
                            let _ = self.toggle_screen_mode(terminal);
                            // Propagate mode to widget so it can adapt layout
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.set_standard_terminal_mode(!self.alt_screen_active);
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
                    self.dispatch_code_event(event);
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
                AppEvent::RegisterApprovedCommand { command, match_kind, persist, semantic_prefix } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.register_approved_command(
                            command.clone(),
                            match_kind.clone(),
                            semantic_prefix.clone(),
                        );
                        if persist {
                            if let Err(err) = add_project_allowed_command(
                                &self.config.code_home,
                                &self.config.cwd,
                                &command,
                                match_kind.clone(),
                            ) {
                                widget.history_push_plain_state(history_cell::new_error_event(format!(
                                    "Failed to persist always-allow command: {err:#}",
                                )));
                            } else {
                                let display = strip_bash_lc_and_escape(&command);
                                widget.push_background_tail(format!(
                                    "Always allowing `{display}` for this project.",
                                ));
                            }
                        }
                    }
                }
                AppEvent::MarkTaskIdle => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.mark_task_idle_after_denied();
                    }
                }
                AppEvent::OpenTerminal(launch) => {
                    let mut spawn = None;
                    let requires_immediate_command = !launch.command.is_empty();
                    let restricted = !matches!(self.config.sandbox_policy, SandboxPolicy::DangerFullAccess);
                    if let AppState::Chat { widget } = &mut self.app_state {
                        if restricted && requires_immediate_command {
                            widget.history_push_plain_state(history_cell::new_error_event(
                                "Terminal requires Full Access to auto-run install commands.".to_string(),
                            ));
                            widget.show_agents_overview_ui();
                        } else {
                            widget.terminal_open(&launch);
                            if requires_immediate_command {
                                spawn = Some((
                                    launch.id,
                                    launch.command.clone(),
                                    Some(launch.command_display.clone()),
                                    launch.controller.clone(),
                                ));
                            }
                        }
                    }
                    if let Some((id, command, display, controller)) = spawn {
                        self.start_terminal_run(id, command, display, controller);
                    }
                }
                AppEvent::TerminalChunk {
                    id,
                    chunk,
                    _is_stderr: is_stderr,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.terminal_append_chunk(id, &chunk, is_stderr);
                    }
                }
                AppEvent::TerminalExit {
                    id,
                    exit_code,
                    _duration: duration,
                } => {
                    let after = if let AppState::Chat { widget } = &mut self.app_state {
                        widget.terminal_finalize(id, exit_code, duration)
                    } else {
                        None
                    };
                    let controller_present = if let Some(run) = self.terminal_runs.get_mut(&id) {
                        run.running = false;
                        run.cancel_tx = None;
                        if let Some(writer_shared) = run.writer_tx.take() {
                            let mut guard = writer_shared.lock().unwrap();
                            guard.take();
                        }
                        run.pty = None;
                        run.controller.is_some()
                    } else {
                        false
                    };
                    if exit_code == Some(0) && !controller_present {
                        self.terminal_runs.remove(&id);
                    }
                    if let Some(after) = after {
                        self.app_event_tx.send(AppEvent::TerminalAfter(after));
                    }
                }
                AppEvent::TerminalCancel { id } => {
                    let mut remove_entry = false;
                    if let Some(run) = self.terminal_runs.get_mut(&id) {
                        let had_controller = run.controller.is_some();
                        if let Some(tx) = run.cancel_tx.take() {
                            if !tx.is_closed() {
                                let _ = tx.send(());
                            }
                        }
                        run.running = false;
                        run.controller = None;
                        if let Some(writer_shared) = run.writer_tx.take() {
                            let mut guard = writer_shared.lock().unwrap();
                            guard.take();
                        }
                        run.pty = None;
                        remove_entry = had_controller;
                    }
                    if remove_entry {
                        self.terminal_runs.remove(&id);
                    }
                }
                AppEvent::TerminalRerun { id } => {
                    let command_and_controller = self
                        .terminal_runs
                        .get(&id)
                        .and_then(|run| {
                            (!run.running).then(|| {
                                (
                                    run.command.clone(),
                                    run.display.clone(),
                                    run.controller.clone(),
                                )
                            })
                        });
                    if let Some((command, display, controller)) = command_and_controller {
                        if let AppState::Chat { widget } = &mut self.app_state {
                            widget.terminal_mark_running(id);
                        }
                        self.start_terminal_run(id, command, Some(display), controller);
                    }
                }
                AppEvent::TerminalRunCommand {
                    id,
                    command,
                    command_display,
                    controller,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.terminal_set_command_display(id, command_display.clone());
                        widget.terminal_mark_running(id);
                    }
                    self.start_terminal_run(id, command, Some(command_display), controller);
                }
                AppEvent::TerminalSendInput { id, data } => {
                    if let Some(run) = self.terminal_runs.get_mut(&id) {
                        if let Some(writer_shared) = run.writer_tx.as_ref() {
                            let mut guard = writer_shared.lock().unwrap();
                            if let Some(tx) = guard.as_ref() {
                                if tx.send(data).is_err() {
                                    guard.take();
                                }
                            }
                        }
                    }
                }
                AppEvent::TerminalResize { id, rows, cols } => {
                    if rows == 0 || cols == 0 {
                        continue;
                    }
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.terminal_apply_resize(id, rows, cols);
                    }
                    if let Some(run) = self.terminal_runs.get(&id) {
                        if let Some(pty) = run.pty.as_ref() {
                            if let Ok(guard) = pty.lock() {
                                let _ = guard.resize(PtySize {
                                    rows,
                                    cols,
                                    pixel_width: 0,
                                    pixel_height: 0,
                                });
                            }
                        }
                    }
                }
                AppEvent::TerminalUpdateMessage { id, message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.terminal_update_message(id, message);
                    }
                }
                AppEvent::TerminalSetAssistantMessage { id, message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.terminal_set_assistant_message(id, message);
                    }
                }
                AppEvent::TerminalAwaitCommand { id, suggestion, ack } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.terminal_prepare_command(id, suggestion, ack.0);
                    }
                }
                AppEvent::TerminalForceClose { id } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.close_terminal_overlay();
                    }
                    self.terminal_runs.remove(&id);
                }
                AppEvent::TerminalApprovalDecision { id, approved } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_terminal_approval_decision(id, approved);
                    }
                }
                AppEvent::StartAutoDriveCelebration { message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.start_auto_drive_card_celebration(message);
                    }
                }
                AppEvent::StopAutoDriveCelebration => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.stop_auto_drive_card_celebration();
                    }
                }
                AppEvent::TerminalAfter(after) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_terminal_after(after);
                    }
                }
                AppEvent::RequestValidationToolInstall { name, command } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        if let Some(launch) = widget.launch_validation_tool_install(&name, &command) {
                            self.app_event_tx.send(AppEvent::OpenTerminal(launch));
                        }
                    }
                }
                AppEvent::RunUpdateCommand { command, display, latest_version } => {
                    if crate::updates::upgrade_ui_enabled() {
                        if let AppState::Chat { widget } = &mut self.app_state {
                            if let Some(launch) = widget.launch_update_command(command, display, latest_version) {
                                self.app_event_tx.send(AppEvent::OpenTerminal(launch));
                            }
                        }
                    }
                }
                AppEvent::SetAutoUpgradeEnabled(enabled) => {
                    if crate::updates::upgrade_ui_enabled() {
                        if let AppState::Chat { widget } = &mut self.app_state {
                            widget.set_auto_upgrade_enabled(enabled);
                        }
                        self.config.auto_upgrade_enabled = enabled;
                    }
                }
                AppEvent::SetAutoSwitchAccountsOnRateLimit(enabled) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_auto_switch_accounts_on_rate_limit(enabled);
                    }
                    self.config.auto_switch_accounts_on_rate_limit = enabled;
                }
                AppEvent::SetApiKeyFallbackOnAllAccountsLimited(enabled) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_api_key_fallback_on_all_accounts_limited(enabled);
                    }
                    self.config.api_key_fallback_on_all_accounts_limited = enabled;
                }
                AppEvent::ShowAutoDriveSettings => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_auto_drive_settings();
                    }
                }
                AppEvent::CloseAutoDriveSettings => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.close_auto_drive_settings();
                    }
                }
                AppEvent::AutoDriveSettingsChanged {
                    review_enabled,
                    agents_enabled,
                    cross_check_enabled,
                    qa_automation_enabled,
                    continue_mode,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_auto_drive_settings(
                            review_enabled,
                            agents_enabled,
                            cross_check_enabled,
                            qa_automation_enabled,
                            continue_mode,
                        );
                    }
                }
                AppEvent::RequestAgentInstall { name, selected_index } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        if let Some(launch) = widget.launch_agent_install(name, selected_index) {
                            self.app_event_tx.send(AppEvent::OpenTerminal(launch));
                        }
                    }
                }
                AppEvent::AgentsOverviewSelectionChanged { index } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_agents_overview_selection(index);
                    }
                }
                // fallthrough handled by break
                AppEvent::CodexOp(op) => match &mut self.app_state {
                    AppState::Chat { widget } => widget.submit_op(op),
                    AppState::Onboarding { .. } => {}
                },
                AppEvent::AutoCoordinatorDecision {
                    seq,
                    status,
                    status_title,
                    status_sent_to_user,
                    goal,
                    cli,
                    agents_timing,
                    agents,
                    transcript,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_decision(
                            seq,
                            status,
                            status_title,
                            status_sent_to_user,
                            goal,
                            cli,
                            agents_timing,
                            agents,
                            transcript,
                        );
                    }
                }
                AppEvent::AutoCoordinatorUserReply {
                    user_response,
                    cli_command,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_user_reply(user_response, cli_command);
                    }
                }
                AppEvent::AutoCoordinatorThinking { delta, summary_index } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_thinking(delta, summary_index);
                    }
                }
                AppEvent::AutoCoordinatorAction { message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_action(message);
                    }
                }
                AppEvent::AutoCoordinatorTokenMetrics {
                    total_usage,
                    last_turn_usage,
                    turn_count,
                    duplicate_items,
                    replay_updates,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_token_metrics(
                            total_usage,
                            last_turn_usage,
                            turn_count,
                            duplicate_items,
                            replay_updates,
                        );
                    }
                }
                AppEvent::AutoCoordinatorStopAck => {
                    // Coordinator acknowledged stop; no additional action required currently.
                }
                AppEvent::AutoCoordinatorCompactedHistory { conversation, show_notice } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_compacted_history(conversation, show_notice);
                    }
                }
                AppEvent::AutoCoordinatorCountdown { countdown_id, seconds_left } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_countdown(countdown_id, seconds_left);
                    }
                }
                AppEvent::AutoCoordinatorRestart { token, attempt } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.auto_handle_restart(token, attempt);
                    }
                }
                AppEvent::PerformUndoRestore {
                    commit,
                    restore_files,
                    restore_conversation,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.perform_undo_restore(commit.as_deref(), restore_files, restore_conversation);
                    }
                }
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
                        SlashCommand::Undo => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_undo_command();
                            }
                        }
                        SlashCommand::Review => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                if command_args.is_empty() {
                                    widget.open_review_dialog();
                                } else {
                                    widget.handle_review_command(command_args);
                                }
                            }
                        }
                        SlashCommand::Cloud => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_cloud_command(command_args);
                            }
                        }
                        SlashCommand::Branch => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_branch_command(command_args);
                            }
                        }
                        SlashCommand::Merge => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_merge_command();
                            }
                        }
                        SlashCommand::Push => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_push_command();
                            }
                        }
                        SlashCommand::Resume => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.show_resume_picker();
                            }
                        }
                        SlashCommand::New => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.abort_active_turn_for_new_chat();
                            }
                            // Start a brand new conversation (core session) with no carried history.
                            // Replace the chat widget entirely, mirroring SwitchCwd flow but without import.
                            let mut new_widget = ChatWidget::new(
                                self.config.clone(),
                                self.app_event_tx.clone(),
                                None,
                                Vec::new(),
                                self.enhanced_keys_supported,
                                self.terminal_info.clone(),
                                self.show_order_overlay,
                                self.latest_upgrade_version.clone(),
                            );
                            new_widget.enable_perf(self.timing_enabled);
                            self.app_state = AppState::Chat { widget: Box::new(new_widget) };
                            self.terminal_runs.clear();
                            self.app_event_tx.send(AppEvent::RequestRedraw);
                        }
                        SlashCommand::Init => {
                            // Guard: do not run if a task is active.
                            if let AppState::Chat { widget } = &mut self.app_state {
                                const INIT_PROMPT: &str =
                                    include_str!("../../prompt_for_init_command.md");
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
                        SlashCommand::Login => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_login_command();
                            }
                        }
                        SlashCommand::Logout => {
                            if let Err(e) = code_login::logout(&self.config.code_home) { tracing::error!("failed to logout: {e}"); }
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
                        SlashCommand::Cmd => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_project_command(command_args);
                            }
                        }
                        SlashCommand::Auto => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                let goal = if command_args.is_empty() {
                                    None
                                } else {
                                    Some(command_args.clone())
                                };
                                widget.handle_auto_command(goal);
                            }
                        }
                        SlashCommand::Status => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.add_status_output();
                            }
                        }
                        SlashCommand::Limits => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_limits_command(command_args);
                            }
                        }
                        SlashCommand::Update => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_update_command(command_args.trim());
                            }
                        }
                        SlashCommand::Settings => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                let section = command
                                    .settings_section_from_args(&command_args)
                                    .and_then(ChatWidget::settings_section_from_hint);
                                widget.show_settings_overlay(section);
                            }
                        }
                        SlashCommand::Notifications => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_notifications_command(command_args);
                            }
                        }
                        SlashCommand::Agents => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_agents_command(command_args);
                            }
                        }
                        SlashCommand::Validation => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_validation_command(command_args);
                            }
                        }
                        SlashCommand::Mcp => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_mcp_command(command_args);
                            }
                        }
                        SlashCommand::Model => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                if command_args.trim().is_empty() {
                                    widget.show_settings_overlay(Some(SettingsSection::Model));
                                } else {
                                    widget.handle_model_command(command_args);
                                }
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
                                widget.show_settings_overlay(Some(SettingsSection::Theme));
                            }
                        }
                        SlashCommand::Prompts => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_prompts_command(command_args.as_str());
                            }
                        }
                        SlashCommand::Skills => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_skills_command(command_args.as_str());
                            }
                        }
                        SlashCommand::Perf => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_perf_command(command_args);
                            }
                        }
                        SlashCommand::Demo => {
                            if let AppState::Chat { widget } = &mut self.app_state {
                                widget.handle_demo_command(command_args);
                            }
                        }
                        // Prompt-expanding commands should have been handled in submit_user_message
                        // but add a fallback just in case. Use a helper that shows the original
                        // slash command in history while sending the expanded prompt to the model.
                        SlashCommand::Plan | SlashCommand::Solve | SlashCommand::Code => {
                            // These should have been expanded already, but handle them anyway
                            if let AppState::Chat { widget } = &mut self.app_state {
                                let expanded = command.expand_prompt(command_args.trim());
                                if let Some(prompt) = expanded {
                                    widget.submit_prompt_with_display(command_text.clone(), prompt);
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
                                if command_args.trim().is_empty() {
                                    widget.show_settings_overlay(Some(SettingsSection::Chrome));
                                } else {
                                    widget.handle_chrome_command(command_args);
                                }
                            }
                        }
                        #[cfg(debug_assertions)]
                        SlashCommand::TestApproval => {
                            use code_core::protocol::EventMsg;
                            use std::collections::HashMap;

                            use code_core::protocol::ApplyPatchApprovalRequestEvent;
                            use code_core::protocol::FileChange;

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
                                                    original_content: "test2".to_string(),
                                                    new_content: "test".to_string(),
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
                    let target = new_cwd.clone();
                    self.config.cwd = target.clone();
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.switch_cwd(target, initial_prompt);
                    }
                }
                AppEvent::ResumePickerLoaded { cwd, candidates } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.present_resume_picker(cwd, candidates);
                    }
                }
                AppEvent::ResumePickerLoadFailed { message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_resume_picker_load_failed(message);
                    }
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
                            self.latest_upgrade_version.clone(),
                        );
                        new_widget.enable_perf(self.timing_enabled);
                        self.app_state = AppState::Chat { widget: Box::new(new_widget) };
                        self.terminal_runs.clear();
                        self.app_event_tx.send(AppEvent::RequestRedraw);
                    }
                }
                AppEvent::PrepareAgents => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.prepare_agents();
                    }
                }
                AppEvent::ShowAgentEditor { name } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.ensure_settings_overlay_section(SettingsSection::Agents);
                        widget.show_agent_editor_ui(name);
                    }
                }
                AppEvent::ShowAgentEditorNew => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.ensure_settings_overlay_section(SettingsSection::Agents);
                        widget.show_agent_editor_new_ui();
                    }
                }
                AppEvent::UpdateModelSelection { model, effort } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_model_selection(model, effort);
                    }
                }
                AppEvent::UpdateReviewModelSelection { model, effort } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_review_model_selection(model, effort);
                    }
                }
                AppEvent::UpdateReviewResolveModelSelection { model, effort } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_review_resolve_model_selection(model, effort);
                    }
                }
                AppEvent::UpdateReviewUseChatModel(use_chat) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_review_use_chat_model(use_chat);
                    }
                }
                AppEvent::UpdateReviewResolveUseChatModel(use_chat) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_review_resolve_use_chat_model(use_chat);
                    }
                }
                AppEvent::UpdatePlanningModelSelection { model, effort } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_planning_model_selection(model, effort);
                    }
                }
                AppEvent::UpdateAutoDriveModelSelection { model, effort } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_auto_drive_model_selection(model, effort);
                    }
                }
                AppEvent::UpdateAutoDriveUseChatModel(use_chat) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_auto_drive_use_chat_model(use_chat);
                    }
                }
                AppEvent::UpdateAutoReviewModelSelection { model, effort } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_auto_review_model_selection(model, effort);
                    }
                }
                AppEvent::UpdateAutoReviewUseChatModel(use_chat) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_auto_review_use_chat_model(use_chat);
                    }
                }
                AppEvent::UpdateAutoReviewResolveModelSelection { model, effort } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_auto_review_resolve_model_selection(model, effort);
                    }
                }
                AppEvent::UpdateAutoReviewResolveUseChatModel(use_chat) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_auto_review_resolve_use_chat_model(use_chat);
                    }
                }
                AppEvent::ModelSelectionClosed { target, accepted } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_model_selection_closed(target, accepted);
                    }
                }
                AppEvent::UpdateTextVerbosity(new_verbosity) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_text_verbosity(new_verbosity);
                    }
                }
                AppEvent::UpdateTuiNotifications(enabled) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_tui_notifications(enabled);
                    }
                    self.config.tui.notifications = Notifications::Enabled(enabled);
                    self.config.tui_notifications = Notifications::Enabled(enabled);
                }
                AppEvent::UpdateValidationTool { name, enable } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.toggle_validation_tool(&name, enable);
                    }
                }
                AppEvent::UpdateValidationGroup { group, enable } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.toggle_validation_group(group, enable);
                    }
                }
                AppEvent::SetTerminalTitle { title } => {
                    self.terminal_title_override = title;
                    self.apply_terminal_title();
                }
                AppEvent::EmitTuiNotification { title, body } => {
                    if let Some(message) = Self::format_notification_message(&title, body.as_deref()) {
                        Self::emit_osc9_notification(&message);
                    }
                }
                AppEvent::UpdateMcpServer { name, enable } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.toggle_mcp_server(&name, enable);
                    }
                }
                AppEvent::UpdateSubagentCommand(cmd) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_subagent_update(cmd);
                    }
                }
                AppEvent::DeleteSubagentCommand(name) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.delete_subagent_by_name(&name);
                    }
                }
                // ShowAgentsSettings removed
                AppEvent::ShowAgentsOverview => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.ensure_settings_overlay_section(SettingsSection::Agents);
                        widget.show_agents_overview_ui();
                    }
                }
                // ShowSubagentEditor removed; use ShowSubagentEditorForName/ShowSubagentEditorNew
                AppEvent::ShowSubagentEditorForName { name } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.ensure_settings_overlay_section(SettingsSection::Agents);
                        widget.show_subagent_editor_for_name(name);
                    }
                }
                AppEvent::ShowSubagentEditorNew => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.ensure_settings_overlay_section(SettingsSection::Agents);
                        widget.show_new_subagent_editor();
                    }
                }
                AppEvent::UpdateAgentConfig { name, enabled, args_read_only, args_write, instructions, description, command } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.apply_agent_update(&name, enabled, args_read_only, args_write, instructions, description, command);
                    }
                }
                AppEvent::AgentValidationFinished { name, result, attempt_id } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_agent_validation_finished(&name, attempt_id, result);
                    }
                }
                AppEvent::PrefillComposer(text) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.insert_str(&text);
                    }
                }
                AppEvent::ConfirmGitInit { resume } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.confirm_git_init(resume);
                    }
                }
                AppEvent::DeclineGitInit => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.decline_git_init();
                    }
                }
                AppEvent::GitInitFinished { ok, message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_git_init_finished(ok, message);
                    }
                }
                AppEvent::SubmitTextWithPreface { visible, preface } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.submit_text_message_with_preface(visible, preface);
                    }
                }
                AppEvent::SubmitHiddenTextWithPreface {
                    agent_text,
                    preface,
                    surface_notice,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.submit_hidden_text_message_with_preface_and_notice(
                            agent_text,
                            preface,
                            surface_notice,
                        );
                    }
                }
                AppEvent::RunReviewCommand(args) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_review_command(args);
                    }
                }
                AppEvent::UpdateReviewAutoResolveEnabled(enabled) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_review_auto_resolve_enabled(enabled);
                    }
                }
                AppEvent::UpdateAutoReviewEnabled(enabled) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_auto_review_enabled(enabled);
                    }
                }
                AppEvent::UpdateReviewAutoResolveAttempts(attempts) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_review_auto_resolve_attempts(attempts);
                    }
                }
                AppEvent::UpdateAutoReviewFollowupAttempts(attempts) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_auto_review_followup_attempts(attempts);
                    }
                }
                AppEvent::ShowReviewModelSelector => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_review_model_selector();
                    }
                }
                AppEvent::ShowReviewResolveModelSelector => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_review_resolve_model_selector();
                    }
                }
                AppEvent::ShowPlanningModelSelector => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_planning_model_selector();
                    }
                }
                AppEvent::ShowAutoDriveModelSelector => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_auto_drive_model_selector();
                    }
                }
                AppEvent::ShowAutoReviewModelSelector => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_auto_review_model_selector();
                    }
                }
                AppEvent::ShowAutoReviewResolveModelSelector => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_auto_review_resolve_model_selector();
                    }
                }
                AppEvent::RunReviewWithScope {
                    prompt,
                    hint,
                    preparation_label,
                    metadata,
                    auto_resolve,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.start_review_with_scope(
                            prompt,
                            hint,
                            preparation_label,
                            metadata,
                            auto_resolve,
                        );
                    }
                }
                AppEvent::BackgroundReviewStarted { worktree_path, branch, agent_id, snapshot } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.on_background_review_started(worktree_path, branch, agent_id, snapshot);
                    }
                }
                AppEvent::BackgroundReviewFinished { worktree_path, branch, has_findings, findings, summary, error, agent_id, snapshot } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.on_background_review_finished(
                            worktree_path,
                            branch,
                            has_findings,
                            findings,
                            summary.clone(),
                            error.clone(),
                            agent_id.clone(),
                            snapshot.clone(),
                        );
                    }
                }
                AppEvent::OpenReviewCustomPrompt => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_review_custom_prompt();
                    }
                }
                AppEvent::FetchCloudTasks { environment } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_cloud_tasks_loading();
                    }
                    let tx = self.app_event_tx.clone();
                    let env_clone = environment.clone();
                    tokio::spawn(async move {
                        match cloud_tasks_service::fetch_tasks(environment).await {
                            Ok(tasks) => tx.send(AppEvent::PresentCloudTasks {
                                environment: env_clone,
                                tasks,
                            }),
                            Err(err) => tx.send(AppEvent::CloudTasksError {
                                message: err.to_string(),
                            }),
                        }
                    });
                }
                AppEvent::PresentCloudTasks { environment, tasks } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.present_cloud_tasks(environment, tasks);
                    }
                }
                AppEvent::CloudTasksError { message } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_cloud_tasks_error(message);
                    }
                }
                AppEvent::FetchCloudEnvironments => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_cloud_environment_loading();
                    }
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        match cloud_tasks_service::fetch_environments().await {
                            Ok(envs) => tx.send(AppEvent::PresentCloudEnvironments { environments: envs }),
                            Err(err) => tx.send(AppEvent::CloudTasksError { message: err.to_string() }),
                        }
                    });
                }
                AppEvent::PresentCloudEnvironments { environments } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.present_cloud_environment_picker(environments);
                    }
                }
                AppEvent::SetCloudEnvironment { environment } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_cloud_environment(environment);
                    }
                }
                AppEvent::ShowCloudTaskActions { task_id } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_cloud_task_actions(task_id);
                    }
                }
                AppEvent::FetchCloudTaskDiff { task_id } => {
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let task = TaskId(task_id.clone());
                        match cloud_tasks_service::fetch_task_diff(task.clone()).await {
                            Ok(Some(diff)) => {
                                tx.send(AppEvent::DiffResult(diff));
                            }
                            Ok(None) => tx.send(AppEvent::CloudTasksError {
                                message: format!("Task {} has no diff available", task.0),
                            }),
                            Err(err) => tx.send(AppEvent::CloudTasksError { message: err.to_string() }),
                        }
                    });
                }
                AppEvent::FetchCloudTaskMessages { task_id } => {
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let task = TaskId(task_id.clone());
                        match cloud_tasks_service::fetch_task_messages(task).await {
                            Ok(messages) if !messages.is_empty() => {
                                let joined = messages.join("\n\n");
                                tx.send(AppEvent::InsertBackgroundEvent {
                                    message: format!("Cloud task output for {task_id}:\n{joined}"),
                                    placement: crate::app_event::BackgroundPlacement::Tail,
                                    order: None,
                                });
                            }
                            Ok(_) => tx.send(AppEvent::CloudTasksError {
                                message: format!("Task {task_id} has no assistant messages"),
                            }),
                            Err(err) => tx.send(AppEvent::CloudTasksError { message: err.to_string() }),
                        }
                    });
                }
                AppEvent::ApplyCloudTask { task_id, preflight } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_cloud_task_apply_status(&task_id, preflight);
                    }
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let task = TaskId(task_id.clone());
                        let result = cloud_tasks_service::apply_task(task, preflight).await;
                        tx.send(AppEvent::CloudTaskApplyFinished {
                            task_id,
                            outcome: result.map_err(|err| CloudTaskError::Msg(err.to_string())),
                            preflight,
                        });
                    });
                }
                AppEvent::CloudTaskApplyFinished { task_id, outcome, preflight } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_cloud_task_apply_finished(task_id, outcome, preflight);
                    }
                }
                AppEvent::OpenCloudTaskCreate => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_cloud_task_create_prompt();
                    }
                }
                AppEvent::SubmitCloudTaskCreate { env_id, prompt, best_of_n } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_cloud_task_create_progress();
                    }
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let result = cloud_tasks_service::create_task(env_id.clone(), prompt.clone(), best_of_n).await;
                        tx.send(AppEvent::CloudTaskCreated {
                            env_id,
                            result: result.map_err(|err| CloudTaskError::Msg(err.to_string())),
                        });
                    });
                }
                AppEvent::CloudTaskCreated { env_id, result } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_cloud_task_created(env_id.clone(), result);
                    }
                }
                AppEvent::StartReviewCommitPicker => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_review_commit_loading();
                    }
                    let cwd = self.config.cwd.clone();
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let commits = code_core::git_info::recent_commits(&cwd, 60).await;
                        tx.send(AppEvent::PresentReviewCommitPicker { commits });
                    });
                }
                AppEvent::PresentReviewCommitPicker { commits } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.present_review_commit_picker(commits);
                    }
                }
                AppEvent::StartReviewBranchPicker => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_review_branch_loading();
                    }
                    let cwd = self.config.cwd.clone();
                    let tx = self.app_event_tx.clone();
                    tokio::spawn(async move {
                        let (branches, current_branch) = tokio::join!(
                            code_core::git_info::local_git_branches(&cwd),
                            code_core::git_info::current_branch_name(&cwd),
                        );
                        tx.send(AppEvent::PresentReviewBranchPicker {
                            current_branch,
                            branches,
                        });
                    });
                }
                AppEvent::PresentReviewBranchPicker {
                    current_branch,
                    branches,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.present_review_branch_picker(current_branch, branches);
                    }
                }
                AppEvent::DiffResult(text) => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.add_diff_output(text);
                    }
                }
                AppEvent::UpdateTheme(new_theme) => {
                    // Switch the theme immediately
                    if matches!(new_theme, code_core::config_types::ThemeName::Custom) {
                        // Prefer runtime custom colors; fall back to config on disk
                        if let Some(colors) = crate::theme::custom_theme_colors() {
                            crate::theme::init_theme(&code_core::config_types::ThemeConfig { name: new_theme, colors, label: crate::theme::custom_theme_label(), is_dark: crate::theme::custom_theme_is_dark() });
                        } else if let Ok(cfg) = code_core::config::Config::load_with_cli_overrides(vec![], code_core::config::ConfigOverrides::default()) {
                            crate::theme::init_theme(&cfg.tui.theme);
                        } else {
                            crate::theme::switch_theme(new_theme);
                        }
                    } else {
                        crate::theme::switch_theme(new_theme);
                    }

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
                        crossterm::terminal::EnableLineWrap
                    );
                    self.apply_terminal_title();

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
                    if matches!(new_theme, code_core::config_types::ThemeName::Custom) {
                        if let Some(colors) = crate::theme::custom_theme_colors() {
                            crate::theme::init_theme(&code_core::config_types::ThemeConfig { name: new_theme, colors, label: crate::theme::custom_theme_label(), is_dark: crate::theme::custom_theme_is_dark() });
                        } else if let Ok(cfg) = code_core::config::Config::load_with_cli_overrides(vec![], code_core::config::ConfigOverrides::default()) {
                            crate::theme::init_theme(&cfg.tui.theme);
                        } else {
                            crate::theme::switch_theme(new_theme);
                        }
                    } else {
                        crate::theme::switch_theme(new_theme);
                    }

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
                        crossterm::terminal::EnableLineWrap
                    );
                    self.apply_terminal_title();

                    // Retint pre-rendered history cells so the preview reflects immediately
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.retint_history_for_preview();
                    }

                    // Don't update config or add to history for previews
                    // Force a full redraw so previews repaint cleanly as you cycle
                    self.clear_on_first_frame = true;
                    self.schedule_redraw();
                }
                AppEvent::UpdateSpinner(name) => {
                    // Switch spinner immediately
                    crate::spinner::switch_spinner(&name);
                    // Update config and save to file
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.set_spinner(name.clone());
                    }
                    self.schedule_redraw();
                }
                AppEvent::PreviewSpinner(name) => {
                    // Switch spinner immediately for preview (no history event)
                    crate::spinner::switch_spinner(&name);
                    // No config change on preview
                    self.schedule_redraw();
                }
                AppEvent::ComposerExpanded => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.on_composer_expanded();
                    }
                    self.schedule_redraw();
                }
                AppEvent::ShowLoginAccounts => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_login_accounts_view();
                    }
                }
                AppEvent::ShowLoginAddAccount => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.show_login_add_account_view();
                    }
                }
                AppEvent::CycleAccessMode => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.cycle_access_mode();
                    }
                    self.schedule_redraw();
                }
                AppEvent::CycleAutoDriveVariant => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.cycle_auto_drive_variant();
                    }
                    self.schedule_redraw();
                }
                AppEvent::LoginStartChatGpt => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        if !widget.login_add_view_active() {
                            continue 'main;
                        }

                        if let Some(flow) = self.login_flow.take() {
                            if let Some(shutdown) = flow.shutdown {
                                shutdown.shutdown();
                            }
                            flow.join_handle.abort();
                        }

                        let opts = ServerOptions::new(
                            self.config.code_home.clone(),
                            code_login::CLIENT_ID.to_string(),
                            self.config.responses_originator_header.clone(),
                        );

                        match code_login::run_login_server(opts) {
                            Ok(server) => {
                                widget.notify_login_chatgpt_started(server.auth_url.clone());
                                let shutdown = server.cancel_handle();
                                let tx = self.app_event_tx.clone();
                                let join_handle = tokio::spawn(async move {
                                    let result = server
                                        .block_until_done()
                                        .await
                                        .map_err(|e| e.to_string());
                                    tx.send(AppEvent::LoginChatGptComplete { result });
                                });
                                self.login_flow = Some(LoginFlowState {
                                    shutdown: Some(shutdown),
                                    join_handle,
                                });
                            }
                            Err(err) => {
                                widget.notify_login_chatgpt_failed(format!(
                                    "Failed to start ChatGPT login: {err}"
                                ));
                            }
                        }
                    }
                }
                AppEvent::LoginStartDeviceCode => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        if !widget.login_add_view_active() {
                            continue 'main;
                        }

                        if let Some(flow) = self.login_flow.take() {
                            if let Some(shutdown) = flow.shutdown {
                                shutdown.shutdown();
                            }
                            flow.join_handle.abort();
                        }
                        widget.notify_login_device_code_pending();

                        let opts = ServerOptions::new(
                            self.config.code_home.clone(),
                            code_login::CLIENT_ID.to_string(),
                            self.config.responses_originator_header.clone(),
                        );
                        let tx = self.app_event_tx.clone();
                        let join_handle = tokio::spawn(async move {
                            match code_login::DeviceCodeSession::start(opts).await {
                                Ok(session) => {
                                    let authorize_url = session.authorize_url();
                                    let user_code = session.user_code().to_string();
                                    let _ = tx.send(AppEvent::LoginDeviceCodeReady { authorize_url, user_code });
                                    let result = session.wait_for_tokens().await.map_err(|e| e.to_string());
                                    let _ = tx.send(AppEvent::LoginDeviceCodeComplete { result });
                                }
                                Err(err) => {
                                    let _ = tx.send(AppEvent::LoginDeviceCodeFailed { message: err.to_string() });
                                }
                            }
                        });
                        self.login_flow = Some(LoginFlowState { shutdown: None, join_handle });
                    }
                }
                AppEvent::LoginCancelChatGpt => {
                    if let Some(flow) = self.login_flow.take() {
                        if let Some(shutdown) = flow.shutdown {
                            shutdown.shutdown();
                        }
                        flow.join_handle.abort();
                    }
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.notify_login_flow_cancelled();
                    }
                }
                AppEvent::LoginChatGptComplete { result } => {
                    if let Some(flow) = self.login_flow.take() {
                        if let Some(shutdown) = flow.shutdown {
                            shutdown.shutdown();
                        }
                        // Allow the task to finish naturally; if still running, abort.
                        if !flow.join_handle.is_finished() {
                            flow.join_handle.abort();
                        }
                    }

                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.notify_login_chatgpt_complete(result);
                    }
                }
                AppEvent::LoginDeviceCodeReady { authorize_url, user_code } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.notify_login_device_code_ready(authorize_url, user_code);
                    }
                }
                AppEvent::LoginDeviceCodeFailed { message } => {
                    if let Some(flow) = self.login_flow.take() {
                        if let Some(shutdown) = flow.shutdown {
                            shutdown.shutdown();
                        }
                        if !flow.join_handle.is_finished() {
                            flow.join_handle.abort();
                        }
                    }
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.notify_login_device_code_failed(message);
                    }
                }
                AppEvent::LoginDeviceCodeComplete { result } => {
                    if let Some(flow) = self.login_flow.take() {
                        if let Some(shutdown) = flow.shutdown {
                            shutdown.shutdown();
                        }
                        if !flow.join_handle.is_finished() {
                            flow.join_handle.abort();
                        }
                    }

                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.notify_login_device_code_complete(result);
                    }
                }
                AppEvent::LoginUsingChatGptChanged { using_chatgpt_auth } => {
                    self.handle_login_mode_change(using_chatgpt_auth);
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
                    resume_picker,
                    latest_upgrade_version,
                }) => {
                    let mut w = ChatWidget::new(
                        config,
                        app_event_tx.clone(),
                        initial_prompt,
                        initial_images,
                        enhanced_keys_supported,
                        terminal_info,
                        show_order_overlay,
                        latest_upgrade_version,
                    );
                    w.enable_perf(enable_perf);
                    if resume_picker {
                        w.show_resume_picker();
                    }
                    self.app_state = AppState::Chat { widget: Box::new(w) };
                    self.terminal_runs.clear();
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
                AppEvent::JumpBack {
                    nth,
                    prefill,
                    history_snapshot,
                } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        let ghost_state = widget.snapshot_ghost_state();
                        // Build response items from current UI history
                        let items = widget.export_response_items();
                        let cfg = widget.config_ref().clone();

                        // Compute prefix up to selected user message now
                        let prefix_items = {
                            let mut user_seen = 0usize;
                            let mut cut = items.len();
                            for (idx, it) in items.iter().enumerate().rev() {
                                if let code_protocol::models::ResponseItem::Message { role, .. } = it {
                                    if role == "user" {
                                        user_seen += 1;
                                        if user_seen == nth { cut = idx; break; }
                                    }
                                }
                            }
                            items.iter().take(cut).cloned().collect::<Vec<_>>()
                        };

                        self.pending_jump_back_ghost_state = Some(ghost_state);
                        self.pending_jump_back_history_snapshot = history_snapshot;

                        // Perform the fork off the UI thread to avoid nested runtimes
                        let server = self._server.clone();
                        let tx = self.app_event_tx.clone();
                        let prefill_clone = prefill.clone();
                        if let Err(err) = std::thread::Builder::new()
                            .name("jump-back-fork".to_string())
                            .spawn(move || {
                                let rt = tokio::runtime::Builder::new_multi_thread()
                                    .enable_all()
                                    .build()
                                    .expect("build tokio runtime");
                                // Clone cfg for the async block to keep original for the event
                                let cfg_for_rt = cfg.clone();
                                let result = rt.block_on(async move {
                                    // Fallback: start a new conversation instead of forking
                                    server.new_conversation(cfg_for_rt).await
                                });
                                if let Ok(new_conv) = result {
                                    tx.send(AppEvent::JumpBackForked { cfg, new_conv: crate::app_event::Redacted(new_conv), prefix_items, prefill: prefill_clone });
                                } else if let Err(e) = result {
                                    tracing::error!("error forking conversation: {e:#}");
                                }
                            })
                        {
                            tracing::error!("jump-back fork spawn failed: {err}");
                        }
                    }
                }
                AppEvent::JumpBackForked { cfg, new_conv, prefix_items, prefill } => {
                    // Replace widget with a new one bound to the forked conversation
                    let session_conf = new_conv.0.session_configured.clone();
                    let conv = new_conv.0.conversation.clone();

                    let mut ghost_state = self.pending_jump_back_ghost_state.take();
                    let history_snapshot = self.pending_jump_back_history_snapshot.take();
                    let emit_prefix = history_snapshot.is_none();

                    if let AppState::Chat { widget } = &mut self.app_state {
                        let auth_manager = widget.auth_manager();
                        let mut new_widget = ChatWidget::new_from_existing(
                            cfg,
                            conv,
                            session_conf,
                            self.app_event_tx.clone(),
                            self.enhanced_keys_supported,
                            self.terminal_info.clone(),
                            self.show_order_overlay,
                            self.latest_upgrade_version.clone(),
                            auth_manager,
                            false,
                        );
                        if let Some(state) = ghost_state.take() {
                            new_widget.adopt_ghost_state(state);
                        } else {
                            tracing::warn!("jump-back fork missing ghost snapshot state; redo may be unavailable");
                        }
                        if let Some(snapshot) = history_snapshot.as_ref() {
                            new_widget.restore_history_snapshot(snapshot);
                        }
                        new_widget.enable_perf(self.timing_enabled);
                        new_widget.check_for_initial_animations();
                        *widget = Box::new(new_widget);
                    } else {
                        let auth_manager = AuthManager::shared_with_mode_and_originator(
                            cfg.code_home.clone(),
                            AuthMode::ApiKey,
                            cfg.responses_originator_header.clone(),
                        );
                        let mut new_widget = ChatWidget::new_from_existing(
                            cfg,
                            conv,
                            session_conf,
                            self.app_event_tx.clone(),
                            self.enhanced_keys_supported,
                            self.terminal_info.clone(),
                            self.show_order_overlay,
                            self.latest_upgrade_version.clone(),
                            auth_manager,
                            false,
                        );
                        if let Some(state) = ghost_state.take() {
                            new_widget.adopt_ghost_state(state);
                        }
                        if let Some(snapshot) = history_snapshot.as_ref() {
                            new_widget.restore_history_snapshot(snapshot);
                        }
                        new_widget.enable_perf(self.timing_enabled);
                        new_widget.check_for_initial_animations();
                        self.app_state = AppState::Chat { widget: Box::new(new_widget) };
                    }
                    self.terminal_runs.clear();
                    // Reset any transient state from the previous widget/session
                    self.commit_anim_running.store(false, Ordering::Release);
                    self.last_esc_time = None;
                    // Force a clean repaint of the new UI state
                    self.clear_on_first_frame = true;

                    // Replay prefix to the UI
                    if emit_prefix {
                        let ev = code_core::protocol::Event {
                            id: "fork".to_string(),
                            event_seq: 0,
                            msg: code_core::protocol::EventMsg::ReplayHistory(
                                code_core::protocol::ReplayHistoryEvent {
                                    items: prefix_items,
                                    history_snapshot: None,
                                }
                            ),
                            order: None,
                        };
                        self.app_event_tx.send(AppEvent::CodexEvent(ev));
                    }

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
                AppEvent::GhostSnapshotFinished { job_id, result, elapsed } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_ghost_snapshot_finished(job_id, result, elapsed);
                    }
                }
                AppEvent::AutoReviewBaselineCaptured { turn_sequence, result } => {
                    if let AppState::Chat { widget } = &mut self.app_state {
                        widget.handle_auto_review_baseline_captured(turn_sequence, result);
                    }
                }
            }
        }
        if self.alt_screen_active {
            terminal.clear()?;
        }

        Ok(())
    }

    /// Pull the next event with priority for interactive input.
    /// Never returns None due to idleness; only returns None if both channels disconnect.
    fn next_event_priority(&mut self) -> Option<AppEvent> {
        next_event_priority_impl(
            &self.app_event_rx_high,
            &self.app_event_rx_bulk,
            &mut self.consecutive_high_events,
        )
    }

}

fn next_event_priority_impl(
    high_rx: &Receiver<AppEvent>,
    bulk_rx: &Receiver<AppEvent>,
    consecutive_high_events: &mut u32,
) -> Option<AppEvent> {
    use std::sync::mpsc::RecvTimeoutError::{Disconnected, Timeout};

    loop {
        if *consecutive_high_events >= HIGH_EVENT_BURST_MAX {
            if let Ok(ev) = bulk_rx.try_recv() {
                *consecutive_high_events = 0;
                return Some(ev);
            }
        }

        if let Ok(ev) = high_rx.try_recv() {
            *consecutive_high_events = consecutive_high_events.saturating_add(1);
            return Some(ev);
        }

        *consecutive_high_events = 0;
        if let Ok(ev) = bulk_rx.try_recv() {
            return Some(ev);
        }

        match high_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(ev) => {
                *consecutive_high_events = 1;
                return Some(ev);
            }
            Err(Timeout) => continue,
            Err(Disconnected) => break,
        }
    }

    bulk_rx.recv().ok()
}

#[cfg(test)]
mod next_event_priority_tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn next_event_priority_serves_bulk_amid_high_burst() {
        let (high_tx, high_rx) = channel();
        let (bulk_tx, bulk_rx) = channel();

        for _ in 0..(HIGH_EVENT_BURST_MAX + 4) {
            high_tx
                .send(AppEvent::RequestRedraw)
                .expect("send high event");
        }

        bulk_tx
            .send(AppEvent::FlushPendingExecEnds)
            .expect("send bulk event");

        // Keep high non-empty beyond the burst window.
        for _ in 0..4 {
            high_tx
                .send(AppEvent::RequestRedraw)
                .expect("send high event");
        }

        let mut consecutive = 0;
        let mut saw_bulk = false;
        for _ in 0..(HIGH_EVENT_BURST_MAX + 2) {
            let ev = next_event_priority_impl(&high_rx, &bulk_rx, &mut consecutive)
                .expect("expected an event");
            if matches!(ev, AppEvent::FlushPendingExecEnds) {
                saw_bulk = true;
                break;
            }
        }

        assert!(
            saw_bulk,
            "bulk event should not be starved behind continuous high-priority events"
        );
    }
}
