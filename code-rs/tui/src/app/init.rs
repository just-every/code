use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::KeyEventKind;
use crossterm::terminal::supports_keyboard_enhancement;
#[cfg(unix)]
use signal_hook::consts::signal::SIGTERM;
#[cfg(unix)]
use signal_hook::flag;

use code_core::config::Config;
use code_core::ConversationManager;
use code_login::{AuthManager, AuthMode};
use code_protocol::protocol::SessionSource;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::ChatWidget;
use crate::file_search::FileSearchManager;
use crate::get_login_status;
use crate::onboarding::onboarding_screen::{OnboardingScreen, OnboardingScreenArgs};
use crate::thread_spawner;
use crate::tui::TerminalInfo;

use super::state::{App, AppState, ChatWidgetArgs, FrameTimer};

impl App<'_> {
    pub(crate) fn new(
        config: Config,
        initial_prompt: Option<String>,
        initial_images: Vec<PathBuf>,
        show_trust_screen: bool,
        debug: bool,
        show_order_overlay: bool,
        terminal_info: TerminalInfo,
        enable_perf: bool,
        resume_picker: bool,
        startup_footer_notice: Option<String>,
        latest_upgrade_version: Option<String>,
    ) -> Self {
        let auth_manager = AuthManager::shared_with_mode_and_originator(
            config.code_home.clone(),
            AuthMode::ApiKey,
            config.responses_originator_header.clone(),
        );
        let conversation_manager = Arc::new(ConversationManager::new(
            auth_manager.clone(),
            SessionSource::Cli,
        ));

        // Split queues so interactive input never waits behind bulk updates.
        let (high_tx, app_event_rx_high) = channel();
        let (bulk_tx, app_event_rx_bulk) = channel();
        let app_event_tx = AppEventSender::new_dual(high_tx.clone(), bulk_tx.clone());

        {
            let remote_tx = app_event_tx.clone();
            let remote_auth_manager = auth_manager.clone();
            let remote_provider = config.model_provider.clone();
            let remote_code_home = config.code_home.clone();
            let remote_using_chatgpt_hint = config.using_chatgpt_auth;
            if !crate::chatwidget::is_test_mode() {
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
        }
        let pending_redraw = Arc::new(AtomicBool::new(false));
        let redraw_inflight = Arc::new(AtomicBool::new(false));
        let post_frame_redraw = Arc::new(AtomicBool::new(false));
        let frame_timer = Arc::new(FrameTimer::new());

        let enhanced_keys_supported = supports_keyboard_enhancement().unwrap_or(false)
            && crate::tui::should_enable_keyboard_enhancement();

        // Spawn a dedicated thread for reading the crossterm event loop and
        // re-publishing the events as AppEvents, as appropriate.
        // Create the input thread stop flag up front so we can store it on `Self`.
        let input_running = Arc::new(AtomicBool::new(true));
        #[cfg(unix)]
        let mut sigterm_guard = None;
        #[cfg(unix)]
        let sigterm_flag = Arc::new(AtomicBool::new(false));
        {
            let app_event_tx = app_event_tx.clone();
            let input_running_thread = input_running.clone();
            let drop_release_events = enhanced_keys_supported;
            if let Err(err) = std::thread::Builder::new()
                .name("tui-input-loop".to_string())
                .spawn(move || {
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
                    match crossterm::event::poll(poll_timeout) {
                        Ok(true) => match crossterm::event::read() {
                            Ok(event) => {
                                match event {
                                    crossterm::event::Event::Key(key_event) => {
                                        // Some Windows terminals (e.g., legacy conhost) only report
                                        // `Release` events when keyboard enhancement flags are not
                                        // supported. Preserve those events so onboarding works there.
                                        if !drop_release_events
                                            || matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat)
                                        {
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
                            Err(err) => {
                                if err.kind() == std::io::ErrorKind::Interrupted {
                                    continue;
                                }
                                tracing::error!("input thread failed to read event: {err}");
                                input_running_thread.store(false, Ordering::Release);
                                app_event_tx.send(AppEvent::ExitRequest);
                                break;
                            }
                        },
                        Ok(false) => {
                            // Timeout expired, no `Event` is available. If the user is typing
                            // keep the loop hot; otherwise sleep briefly to cut idle CPU.
                            if !hot_typing {
                                std::thread::sleep(Duration::from_millis(5));
                            }
                        }
                        Err(err) => {
                            if err.kind() == std::io::ErrorKind::Interrupted {
                                continue;
                            }
                            tracing::error!("input thread failed to poll events: {err}");
                            input_running_thread.store(false, Ordering::Release);
                            app_event_tx.send(AppEvent::ExitRequest);
                            break;
                        }
                    }
                }
                })
            {
                tracing::error!("input thread spawn failed: {err}");
            }
        }

        #[cfg(unix)]
        {
            let term_trigger = Arc::new(AtomicBool::new(false));
            let tx = app_event_tx.clone();
            let running_for_thread = input_running.clone();
            let trigger_for_thread = Arc::clone(&term_trigger);
            let flag_for_thread = sigterm_flag.clone();
            let listener = move || {
                while running_for_thread.load(Ordering::Relaxed) {
                    if trigger_for_thread.swap(false, Ordering::SeqCst) {
                        running_for_thread.store(false, Ordering::Release);
                        flag_for_thread.store(true, Ordering::Release);
                        tx.send(AppEvent::ExitRequest);
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            };

            if thread_spawner::spawn_lightweight("sigterm-listener", listener).is_some() {
                match flag::register(SIGTERM, Arc::clone(&term_trigger)) {
                    Ok(sig_id) => {
                        sigterm_guard = Some(sig_id);
                    }
                    Err(err) => {
                        tracing::warn!("failed to register SIGTERM handler: {err}");
                        input_running.store(false, Ordering::Release);
                    }
                }
            } else {
                tracing::warn!("SIGTERM listener spawn skipped: background thread limit reached");
            }
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
                resume_picker,
                latest_upgrade_version: latest_upgrade_version.clone(),
            };
            AppState::Onboarding {
                screen: OnboardingScreen::new(OnboardingScreenArgs {
                    event_tx: app_event_tx.clone(),
                    code_home: config.code_home.clone(),
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
                latest_upgrade_version.clone(),
            );
            chat_widget.enable_perf(enable_perf);
            if resume_picker {
                chat_widget.show_resume_picker();
            }
            // Check for initial animations after widget is created
            chat_widget.check_for_initial_animations();
            if let Some(notice) = startup_footer_notice {
                chat_widget.debug_notice(notice);
            }
            AppState::Chat {
                widget: Box::new(chat_widget),
            }
        };

        let file_search = FileSearchManager::new(config.cwd.clone(), app_event_tx.clone());
        let start_in_alt = config.tui.alternate_screen;
        Self {
            _server: conversation_manager,
            app_event_tx,
            app_event_rx_high,
            app_event_rx_bulk,
            consecutive_high_events: 0,
            app_state,
            config,
            latest_upgrade_version,
            file_search,
            pending_redraw,
            redraw_inflight,
            post_frame_redraw,
            stdout_backpressure_skips: 0,
            frame_timer,
            input_running,
            enhanced_keys_supported,
            non_enhanced_pressed_keys: HashSet::new(),
            _debug: debug,
            show_order_overlay,
            commit_anim_running: Arc::new(AtomicBool::new(false)),
            terminal_info,
            clear_on_first_frame: true,
            pending_jump_back_ghost_state: None,
            pending_jump_back_history_snapshot: None,
            last_frame_size: None,
            last_esc_time: None,
            timing_enabled: enable_perf,
            timing: super::state::TimingStats::default(),
            buffer_diff_profiler: super::state::BufferDiffProfiler::new_from_env(),
            alt_screen_active: start_in_alt,
            terminal_runs: HashMap::new(),
            terminal_title_override: None,
            login_flow: None,
            #[cfg(unix)]
            sigterm_guard,
            #[cfg(unix)]
            sigterm_flag,
        }
    }
}

fn should_show_onboarding(
    login_status: crate::LoginStatus,
    _config: &Config,
    show_trust_screen: bool,
) -> bool {
    if show_trust_screen {
        return true;
    }
    matches!(login_status, crate::LoginStatus::NotAuthenticated)
}

fn should_show_login_screen(login_status: crate::LoginStatus, _config: &Config) -> bool {
    matches!(login_status, crate::LoginStatus::NotAuthenticated)
}
