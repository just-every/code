use std::io::{Read, Write};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::FutureExt;
use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use shlex::try_join;

use color_eyre::eyre::Result;

use crate::app_event::{AppEvent, TerminalRunController, TerminalRunEvent};
use crate::tui;

use super::state::{
    App,
    AppState,
    DEFAULT_PTY_COLS,
    DEFAULT_PTY_ROWS,
    TerminalRunState,
};

impl App<'_> {
    pub(super) fn apply_terminal_title(&self) {
        let title = self
            .terminal_title_override
            .as_deref()
            .unwrap_or(Self::DEFAULT_TERMINAL_TITLE);
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::SetTitle(title.to_string())
        );
    }

    fn sanitize_notification_text(input: &str) -> String {
        let mut sanitized = String::with_capacity(input.len());
        for ch in input.chars() {
            match ch {
                '\u{00}'..='\u{08}' | '\u{0B}' | '\u{0C}' | '\u{0E}'..='\u{1F}' | '\u{7F}' => {}
                '\n' | '\r' | '\t' => {
                    if !sanitized.ends_with(' ') {
                        sanitized.push(' ');
                    }
                }
                _ => sanitized.push(ch),
            }
        }
        sanitized
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub(super) fn format_notification_message(title: &str, body: Option<&str>) -> Option<String> {
        let title = Self::sanitize_notification_text(title);
        let body = body.map(Self::sanitize_notification_text);
        let mut message = match body {
            Some(ref b) if !b.is_empty() => {
                if title.is_empty() {
                    b.clone()
                } else {
                    format!("{}: {}", title, b)
                }
            }
            _ => title.clone(),
        };

        if message.is_empty() {
            return None;
        }

        const MAX_LEN: usize = 160;
        if message.chars().count() > MAX_LEN {
            let mut truncated = String::new();
            for ch in message.chars() {
                if truncated.chars().count() >= MAX_LEN.saturating_sub(3) {
                    break;
                }
                truncated.push(ch);
            }
            truncated.push_str("...");
            message = truncated;
        }

        Some(message)
    }

    pub(super) fn emit_osc9_notification(message: &str) {
        let payload = format!("\u{1b}]9;{}\u{7}", message);
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(payload.as_bytes());
        let _ = stdout.flush();
    }

    pub(super) fn start_terminal_run(
        &mut self,
        id: u64,
        command: Vec<String>,
        display: Option<String>,
        controller: Option<TerminalRunController>,
    ) {
        if command.is_empty() {
            self.app_event_tx.send(AppEvent::TerminalChunk {
                id,
                chunk: b"Install command not resolved".to_vec(),
                _is_stderr: true,
            });
            self.app_event_tx.send(AppEvent::TerminalExit {
                id,
                exit_code: Some(1),
                _duration: Duration::from_millis(0),
            });
            return;
        }

        let joined_display = try_join(command.iter().map(|s| s.as_str()))
            .ok()
            .unwrap_or_else(|| command.join(" "));

        let display_line = display.clone().unwrap_or_else(|| joined_display.clone());

        if !display_line.trim().is_empty() {
            let line = format!("$ {display_line}\n");
            self.app_event_tx.send(AppEvent::TerminalChunk {
                id,
                chunk: line.into_bytes(),
                _is_stderr: false,
            });
        }

        let stored_command = command.clone();
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        let (writer_tx_raw, writer_rx) = channel::<Vec<u8>>();
        let writer_tx_shared = Arc::new(Mutex::new(Some(writer_tx_raw)));
        let controller_clone = controller.clone();
        let cwd = self.config.cwd.clone();
        let controller_tx = controller.map(|c| c.tx);

        let (pty_rows, pty_cols) = match &self.app_state {
            AppState::Chat { widget } => widget
                .terminal_dimensions_hint()
                .unwrap_or((DEFAULT_PTY_ROWS, DEFAULT_PTY_COLS)),
            _ => (DEFAULT_PTY_ROWS, DEFAULT_PTY_COLS),
        };

        let pty_system = native_pty_system();
        let pair = match pty_system.openpty(PtySize {
            rows: pty_rows,
            cols: pty_cols,
            pixel_width: 0,
            pixel_height: 0,
        }) {
            Ok(pair) => pair,
            Err(err) => {
                let msg = format!("Failed to open PTY: {err}\n");
                self.app_event_tx.send(AppEvent::TerminalChunk {
                    id,
                    chunk: msg.clone().into_bytes(),
                    _is_stderr: true,
                });
                if let Some(ref ctrl) = controller_tx {
                    let _ = ctrl.send(TerminalRunEvent::Chunk {
                        data: msg.clone().into_bytes(),
                        _is_stderr: true,
                    });
                    let _ = ctrl.send(TerminalRunEvent::Exit {
                        exit_code: Some(1),
                        _duration: Duration::from_millis(0),
                    });
                }
                self.app_event_tx.send(AppEvent::TerminalExit {
                    id,
                    exit_code: Some(1),
                    _duration: Duration::from_millis(0),
                });
                return;
            }
        };

        let PtyPair { master, slave } = pair;
        let master = Arc::new(Mutex::new(master));

        let writer = {
            let guard = match master.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    let msg = "Failed to acquire terminal writer: poisoned lock\n".to_string();
                    self.app_event_tx.send(AppEvent::TerminalChunk {
                        id,
                        chunk: msg.clone().into_bytes(),
                        _is_stderr: true,
                    });
                    if let Some(ref ctrl) = controller_tx {
                        let _ = ctrl.send(TerminalRunEvent::Chunk {
                            data: msg.clone().into_bytes(),
                            _is_stderr: true,
                        });
                        let _ = ctrl.send(TerminalRunEvent::Exit {
                            exit_code: Some(1),
                            _duration: Duration::from_millis(0),
                        });
                    }
                    self.app_event_tx.send(AppEvent::TerminalExit {
                        id,
                        exit_code: Some(1),
                        _duration: Duration::from_millis(0),
                    });
                    return;
                }
            };
            let result = guard.take_writer();
            drop(guard);
            match result {
                Ok(writer) => writer,
                Err(err) => {
                    let msg = format!("Failed to acquire terminal writer: {err}\n");
                    self.app_event_tx.send(AppEvent::TerminalChunk {
                        id,
                        chunk: msg.clone().into_bytes(),
                        _is_stderr: true,
                    });
                    if let Some(ref ctrl) = controller_tx {
                        let _ = ctrl.send(TerminalRunEvent::Chunk {
                            data: msg.clone().into_bytes(),
                            _is_stderr: true,
                        });
                        let _ = ctrl.send(TerminalRunEvent::Exit {
                            exit_code: Some(1),
                            _duration: Duration::from_millis(0),
                        });
                    }
                    self.app_event_tx.send(AppEvent::TerminalExit {
                        id,
                        exit_code: Some(1),
                        _duration: Duration::from_millis(0),
                    });
                    return;
                }
            }
        };

        let reader = {
            let guard = match master.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    let msg = "Failed to read terminal output: poisoned lock\n".to_string();
                    self.app_event_tx.send(AppEvent::TerminalChunk {
                        id,
                        chunk: msg.clone().into_bytes(),
                        _is_stderr: true,
                    });
                    if let Some(ref ctrl) = controller_tx {
                        let _ = ctrl.send(TerminalRunEvent::Chunk {
                            data: msg.clone().into_bytes(),
                            _is_stderr: true,
                        });
                        let _ = ctrl.send(TerminalRunEvent::Exit {
                            exit_code: Some(1),
                            _duration: Duration::from_millis(0),
                        });
                    }
                    self.app_event_tx.send(AppEvent::TerminalExit {
                        id,
                        exit_code: Some(1),
                        _duration: Duration::from_millis(0),
                    });
                    return;
                }
            };
            let result = guard.try_clone_reader();
            drop(guard);
            match result {
                Ok(reader) => reader,
                Err(err) => {
                    let msg = format!("Failed to read terminal output: {err}\n");
                    self.app_event_tx.send(AppEvent::TerminalChunk {
                        id,
                        chunk: msg.clone().into_bytes(),
                        _is_stderr: true,
                    });
                    if let Some(ref ctrl) = controller_tx {
                        let _ = ctrl.send(TerminalRunEvent::Chunk {
                            data: msg.clone().into_bytes(),
                            _is_stderr: true,
                        });
                        let _ = ctrl.send(TerminalRunEvent::Exit {
                            exit_code: Some(1),
                            _duration: Duration::from_millis(0),
                        });
                    }
                    self.app_event_tx.send(AppEvent::TerminalExit {
                        id,
                        exit_code: Some(1),
                        _duration: Duration::from_millis(0),
                    });
                    return;
                }
            }
        };

        let mut command_builder = CommandBuilder::new(command[0].clone());
        for arg in &command[1..] {
            command_builder.arg(arg);
        }
        command_builder.cwd(&cwd);

        let mut child = match slave.spawn_command(command_builder) {
            Ok(child) => child,
            Err(err) => {
                let msg = format!("Failed to spawn command: {err}\n");
                self.app_event_tx.send(AppEvent::TerminalChunk {
                    id,
                    chunk: msg.clone().into_bytes(),
                    _is_stderr: true,
                });
                if let Some(ref ctrl) = controller_tx {
                    let _ = ctrl.send(TerminalRunEvent::Chunk {
                        data: msg.clone().into_bytes(),
                        _is_stderr: true,
                    });
                    let _ = ctrl.send(TerminalRunEvent::Exit {
                        exit_code: Some(1),
                        _duration: Duration::from_millis(0),
                    });
                }
                self.app_event_tx.send(AppEvent::TerminalExit {
                    id,
                    exit_code: Some(1),
                    _duration: Duration::from_millis(0),
                });
                return;
            }
        };

        let mut killer = child.clone_killer();

        let master_for_state = Arc::clone(&master);
        self.terminal_runs.insert(
            id,
            TerminalRunState {
                command: stored_command,
                display: display_line.clone(),
                cancel_tx: Some(cancel_tx),
                running: true,
                controller: controller_clone,
                writer_tx: Some(writer_tx_shared.clone()),
                pty: Some(master_for_state),
            },
        );

        let tx = self.app_event_tx.clone();
        let controller_tx_task = controller_tx.clone();
        let master_for_task = Arc::clone(&master);
        let writer_tx_for_task = writer_tx_shared.clone();
        tokio::spawn(async move {
            let start_time = Instant::now();
            let controller_tx = controller_tx_task;
            let _master = master_for_task;

            let writer_handle = tokio::task::spawn_blocking(move || {
                let mut writer = writer;
                while let Ok(bytes) = writer_rx.recv() {
                    if writer.write_all(&bytes).is_err() {
                        break;
                    }
                    if writer.flush().is_err() {
                        break;
                    }
                }
            });

            let tx_reader = tx.clone();
            let controller_tx_reader = controller_tx.clone();
            let reader_handle = tokio::task::spawn_blocking(move || {
                let mut buf = [0u8; 8192];
                let mut reader = reader;
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            let chunk = buf[..n].to_vec();
                            tx_reader.send(AppEvent::TerminalChunk {
                                id,
                                chunk: chunk.clone(),
                                _is_stderr: false,
                            });
                            if let Some(ref ctrl) = controller_tx_reader {
                                let _ = ctrl.send(TerminalRunEvent::Chunk {
                                    data: chunk,
                                    _is_stderr: false,
                                });
                            }
                        }
                        Err(err) => {
                            let msg = format!("Error reading terminal output: {err}\n");
                            tx_reader.send(AppEvent::TerminalChunk {
                                id,
                                chunk: msg.clone().into_bytes(),
                                _is_stderr: true,
                            });
                            if let Some(ref ctrl) = controller_tx_reader {
                                let _ = ctrl.send(TerminalRunEvent::Chunk {
                                    data: msg.into_bytes(),
                                    _is_stderr: true,
                                });
                            }
                            break;
                        }
                    }
                }
            });

            let mut cancel_rx = cancel_rx.fuse();
            let mut cancel_triggered = false;
            let wait_handle = tokio::task::spawn_blocking(move || child.wait());
            futures::pin_mut!(wait_handle);
            let wait_status = loop {
                tokio::select! {
                    res = &mut wait_handle => break res,
                    res = &mut cancel_rx, if !cancel_triggered => {
                        if res.is_ok() {
                            cancel_triggered = true;
                            let _ = killer.kill();
                        }
                    }
                }
            };

            {
                let mut guard = writer_tx_for_task.lock().unwrap();
                guard.take();
            }

            let _ = reader_handle.await;
            let _ = writer_handle.await;

            let (exit_code, duration) = match wait_status {
                Ok(Ok(status)) => (Some(status.exit_code() as i32), start_time.elapsed()),
                Ok(Err(err)) => {
                    let msg = format!("Process wait failed: {err}\n");
                    tx.send(AppEvent::TerminalChunk {
                        id,
                        chunk: msg.clone().into_bytes(),
                        _is_stderr: true,
                    });
                    if let Some(ref ctrl) = controller_tx {
                        let _ = ctrl.send(TerminalRunEvent::Chunk {
                            data: msg.clone().into_bytes(),
                            _is_stderr: true,
                        });
                    }
                    (None, start_time.elapsed())
                }
                Err(err) => {
                    let msg = format!("Process join failed: {err}\n");
                    tx.send(AppEvent::TerminalChunk {
                        id,
                        chunk: msg.clone().into_bytes(),
                        _is_stderr: true,
                    });
                    if let Some(ref ctrl) = controller_tx {
                        let _ = ctrl.send(TerminalRunEvent::Chunk {
                            data: msg.clone().into_bytes(),
                            _is_stderr: true,
                        });
                    }
                    (None, start_time.elapsed())
                }
            };

            if let Some(ref ctrl) = controller_tx {
                let _ = ctrl.send(TerminalRunEvent::Exit {
                    exit_code,
                    _duration: duration,
                });
            }
            tx.send(AppEvent::TerminalExit {
                id,
                exit_code,
                _duration: duration,
            });
        });
    }

    #[cfg(unix)]
    pub(super) fn suspend(&mut self, terminal: &mut tui::Tui) -> Result<()> {
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

    /// Toggle between alternate-screen TUI and standard terminal buffer (Ctrl+T).
    pub(super) fn toggle_screen_mode(&mut self, _terminal: &mut tui::Tui) -> Result<()> {
        if self.alt_screen_active {
            // Leave alt screen only; keep raw mode enabled for key handling.
            let _ = crate::tui::leave_alt_screen_only();
            // Clear the normal buffer so our buffered transcript starts at a clean screen
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::style::ResetColor,
                crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                crossterm::cursor::MoveTo(0, 0),
                crossterm::terminal::EnableLineWrap
            );
            self.alt_screen_active = false;
            // Persist preference
            let _ = code_core::config::set_tui_alternate_screen(&self.config.code_home, false);
            // Immediately mirror the entire transcript into the terminal scrollback so
            // the user sees full history when entering standard mode.
            if let AppState::Chat { widget } = &self.app_state {
                let transcript = widget.export_transcript_lines_for_buffer();
                if !transcript.is_empty() {
                    // Best-effort: compute current width and bottom reservation.
                    // We don't have `terminal` here; schedule a one-shot redraw event
                    // that carries the transcript via InsertHistory to reuse the normal path.
                    self.app_event_tx.send(AppEvent::InsertHistory(transcript));
                }
            }
            // Ensure the input is painted in its reserved region immediately.
            self.schedule_redraw();
        } else {
            // Re-enter alt screen and force a clean repaint.
            let fg = crate::colors::text();
            let bg = crate::colors::background();
            let _ = crate::tui::enter_alt_screen_only(fg, bg);
            self.clear_on_first_frame = true;
            self.alt_screen_active = true;
            // Persist preference
            let _ = code_core::config::set_tui_alternate_screen(&self.config.code_home, true);
            // Request immediate redraw
            self.schedule_redraw();
        }
        Ok(())
    }
}
