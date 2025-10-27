// Forbid accidental stdout/stderr writes in the *library* portion of the TUI.
// The standalone `codex-tui` binary prints a short help message before the
// alternate‑screen mode starts; that file opts‑out locally via `allow`.
#![deny(clippy::print_stdout, clippy::print_stderr)]
#![deny(clippy::disallowed_methods)]
use app::App;
use code_core::BUILT_IN_OSS_MODEL_PROVIDER_ID;
use code_core::config::set_cached_terminal_background;
use code_core::config::Config;
use code_core::config::ConfigOverrides;
use code_core::config::ConfigToml;
use code_core::config::find_code_home;
use code_core::config::load_config_as_toml;
use code_core::config::load_config_as_toml_with_cli_overrides;
use code_core::protocol::AskForApproval;
use code_core::protocol::SandboxPolicy;
use code_core::config_types::CachedTerminalBackground;
use code_core::config_types::ThemeColors;
use code_core::config_types::ThemeConfig;
use code_core::config_types::ThemeName;
use regex_lite::Regex;
use code_login::AuthMode;
use code_login::CodexAuth;
use code_ollama::DEFAULT_OSS_MODEL;
use code_protocol::config_types::SandboxMode;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::Once;
use std::sync::OnceLock;
use tracing_appender::non_blocking;
use tracing_appender::rolling;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

mod app;
mod app_event;
mod app_event_sender;
mod account_label;
mod bottom_pane;
mod chrome_launch;
mod chatwidget;
mod citation_regex;
mod cloud_tasks_service;
mod cli;
mod common;
mod colors;
pub mod card_theme;
mod diff_render;
mod exec_command;
mod file_search;
pub mod gradient_background;
mod get_git_diff;
mod glitch_animation;
mod auto_drive_strings;
mod auto_drive_style;
mod auto_review;
mod header_wave;
mod history_cell;
mod history;
mod insert_history;
pub mod live_wrap;
mod markdown;
mod markdown_render;
mod markdown_renderer;
mod markdown_stream;
mod syntax_highlight;
pub mod onboarding;
pub mod public_widgets;
mod render;
// mod scroll_view; // Orphaned after trait-based HistoryCell migration
mod session_log;
mod shimmer;
mod slash_command;
mod rate_limits_view;
mod resume;
mod streaming;
mod sanitize;
mod layout_consts;
mod terminal_info;
// mod text_block; // Orphaned after trait-based HistoryCell migration
mod text_formatting;
mod text_processing;
mod theme;
mod thread_spawner;
mod util {
    pub mod buffer;
    pub mod list_window;
}
mod spinner;
mod tui;
#[cfg(feature = "code-fork")]
mod tui_event_extensions;
#[cfg(feature = "code-fork")]
mod foundation;
mod ui_consts;
mod user_approval_widget;
mod height_manager;
mod clipboard_paste;
mod greeting;
// Upstream introduced a standalone status indicator widget. Our fork renders
// status within the composer title; keep the module private unless tests need it.
mod status_indicator_widget;
#[cfg(target_os = "macos")]
mod agent_install_helpers;

// Internal vt100-based replay tests live as a separate source file to keep them
// close to the widget code. Include them in unit tests.
mod updates;
#[cfg(any(test, feature = "test-helpers"))]
pub mod test_backend;

pub use cli::Cli;
pub use self::markdown_render::render_markdown_text;
pub use public_widgets::composer_input::{ComposerAction, ComposerInput};

#[cfg(feature = "test-helpers")]
pub mod test_helpers {
    pub use crate::chatwidget::smoke_helpers::AutoContinueModeFixture;
    pub use crate::chatwidget::smoke_helpers::ChatWidgetHarness;
    pub use crate::chatwidget::smoke_helpers::LayoutMetrics;
    #[cfg(test)]
    pub use crate::test_backend::VT100Backend;

    use crate::app_event::AppEvent;
    use code_core::history::state::HistoryRecord;
    use std::time::Duration;

    use std::io::Write;

    /// Render successive frames of the chat widget into a VT100-backed terminal.
    /// Each entry in `frames` specifies the `(width, height)` for that capture.
    /// Returns a vector of screen dumps, one per frame.
    pub fn render_chat_widget_frames_to_vt100(
        harness: &mut ChatWidgetHarness,
        frames: &[(u16, u16)],
    ) -> Vec<String> {
        use crate::test_backend::VT100Backend;

        frames
            .iter()
            .map(|&(width, height)| {
                harness.flush_into_widget();

                let backend = VT100Backend::new(width, height);
                let mut terminal = ratatui::Terminal::new(backend).expect("create terminal");

                terminal
                    .draw(|frame| {
                        let area = frame.area();
                        let chat_widget = harness.chat();
                        frame.render_widget_ref(&*chat_widget, area);
                    })
                    .expect("draw");

                terminal.backend_mut().flush().expect("flush");

                let screen = terminal.backend().vt100().screen();
                let (rows, cols) = screen.size();
                let snapshot = screen
                    .rows(0, cols)
                    .take(rows.into())
                    .map(|row| row.trim_end().to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                snapshot
            })
            .collect()
    }

    /// Convenience helper to capture a single VT100 frame at the provided size.
    pub fn render_chat_widget_to_vt100(
        harness: &mut ChatWidgetHarness,
        width: u16,
        height: u16,
    ) -> String {
        render_chat_widget_frames_to_vt100(harness, &[(width, height)])
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    pub fn assert_has_terminal_chunk_containing(
        harness: &mut ChatWidgetHarness,
        needle: &str,
    ) {
        let events = harness.poll_until(
            |events| {
                events.iter().any(|event| {
                    match event {
                        AppEvent::TerminalChunk { chunk, .. } => {
                            String::from_utf8_lossy(chunk).contains(needle)
                        }
                        _ => false,
                    }
                })
            },
            Duration::from_millis(200),
        );
        crate::chatwidget::smoke_helpers::assert_has_terminal_chunk_containing(&events, needle);
    }

    pub fn assert_has_background_event_containing(
        harness: &mut ChatWidgetHarness,
        needle: &str,
    ) {
        let events = harness.poll_until(
            |events| {
                events.iter().any(|event| {
                    match event {
                        AppEvent::InsertBackgroundEvent { message, .. } => {
                            message.contains(needle)
                        }
                        _ => false,
                    }
                })
            },
            Duration::from_millis(200),
        );
        crate::chatwidget::smoke_helpers::assert_has_background_event_containing(&events, needle);
    }

    pub fn assert_has_codex_event(harness: &mut ChatWidgetHarness) {
        let events = harness.poll_until(
            |events| events
                .iter()
                .any(|event| matches!(event, AppEvent::CodexEvent(_))),
            Duration::from_millis(200),
        );
        crate::chatwidget::smoke_helpers::assert_has_codex_event(&events);
    }

    pub fn layout_metrics(harness: &ChatWidgetHarness) -> LayoutMetrics {
        harness.layout_metrics()
    }

    pub fn assert_has_insert_history(harness: &mut ChatWidgetHarness) {
        let events = harness.poll_until(
            |events| {
                events.iter().any(|event| {
                    matches!(
                        event,
                        AppEvent::InsertHistory(_)
                            | AppEvent::InsertHistoryWithKind { .. }
                            | AppEvent::InsertFinalAnswer { .. }
                    )
                })
            },
            Duration::from_millis(200),
        );
        crate::chatwidget::smoke_helpers::assert_has_insert_history(&events);
    }

    pub fn assert_no_events(harness: &mut ChatWidgetHarness) {
        let events = harness.poll_until(
            |events| !events.is_empty(),
            Duration::from_millis(100),
        );
        crate::chatwidget::smoke_helpers::assert_no_events(&events);
    }

    pub fn history_records(harness: &mut ChatWidgetHarness) -> Vec<HistoryRecord> {
        harness.history_records()
    }

    pub fn set_standard_terminal_mode(harness: &mut ChatWidgetHarness, enabled: bool) {
        harness.set_standard_terminal_mode(enabled);
    }

    pub fn force_scroll_offset(harness: &mut ChatWidgetHarness, offset: u16) {
        harness.force_scroll_offset(offset);
    }

    pub fn scroll_offset(harness: &ChatWidgetHarness) -> u16 {
        harness.scroll_offset()
    }
}

fn theme_configured_in_config_file(code_home: &std::path::Path) -> bool {
    let config_path = code_home.join("config.toml");
    let Ok(contents) = std::fs::read_to_string(&config_path) else {
        return false;
    };

    let table_pattern = Regex::new(r"(?m)^\s*\[tui\.theme\]").expect("valid regex");
    if table_pattern.is_match(&contents) {
        return true;
    }

    let inline_pattern = Regex::new(r"(?m)^\s*tui\.theme\s*=").expect("valid regex");
    inline_pattern.is_match(&contents)
}
// (tests access modules directly within the crate)

#[derive(Debug)]
pub struct ExitSummary {
    pub token_usage: code_core::protocol::TokenUsage,
    pub session_id: Option<Uuid>,
}

pub fn resume_command_name() -> &'static str {
    static COMMAND: OnceLock<&'static str> = OnceLock::new();
    COMMAND.get_or_init(|| {
        let arg0 = std::env::args().next();
        let invoked = arg0
            .as_ref()
            .and_then(|value| std::path::Path::new(value).file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("");

        if invoked.eq_ignore_ascii_case("coder") {
            "coder"
        } else {
            "code"
        }
    })
}

pub async fn run_main(
    mut cli: Cli,
    code_linux_sandbox_exe: Option<PathBuf>,
) -> std::io::Result<ExitSummary> {
    cli.finalize_defaults();

    let (sandbox_mode, approval_policy) = if cli.full_auto {
        (
            Some(SandboxMode::WorkspaceWrite),
            Some(AskForApproval::OnRequest),
        )
    } else if cli.dangerously_bypass_approvals_and_sandbox {
        (
            Some(SandboxMode::DangerFullAccess),
            Some(AskForApproval::Never),
        )
    } else {
        (
            cli.sandbox_mode.map(Into::<SandboxMode>::into),
            cli.approval_policy.map(Into::into),
        )
    };

    // When using `--oss`, let the bootstrapper pick the model (defaulting to
    // gpt-oss:20b) and ensure it is present locally. Also, force the built‑in
    // `oss` model provider.
    let model = if let Some(model) = &cli.model {
        Some(model.clone())
    } else if cli.oss {
        Some(DEFAULT_OSS_MODEL.to_owned())
    } else {
        None // No model specified, will use the default.
    };

    let model_provider_override = if cli.oss {
        Some(BUILT_IN_OSS_MODEL_PROVIDER_ID.to_owned())
    } else {
        None
    };

    // canonicalize the cwd
    let cwd = cli.cwd.clone().map(|p| p.canonicalize().unwrap_or(p));

    let overrides = ConfigOverrides {
        model,
        review_model: None,
        approval_policy,
        sandbox_mode,
        cwd,
        model_provider: model_provider_override,
        config_profile: cli.config_profile.clone(),
        code_linux_sandbox_exe,
        base_instructions: None,
        include_plan_tool: Some(true),
        include_apply_patch_tool: None,
        include_view_image_tool: None,
        disable_response_storage: cli.oss.then_some(true),
        show_raw_agent_reasoning: cli.oss.then_some(true),
        debug: Some(cli.debug),
        tools_web_search_request: Some(cli.web_search),
        mcp_servers: None,
        experimental_client_tools: None,
    };

    // Parse `-c` overrides from the CLI.
    let cli_kv_overrides = match cli.config_overrides.parse_overrides() {
        Ok(v) => v,
        #[allow(clippy::print_stderr)]
        Err(e) => {
            eprintln!("Error parsing -c overrides: {e}");
            std::process::exit(1);
        }
    };
    let theme_override_in_cli = cli_kv_overrides
        .iter()
        .any(|(path, _)| path.starts_with("tui.theme"));

    let code_home = match find_code_home() {
        Ok(code_home) => code_home,
        #[allow(clippy::print_stderr)]
        Err(err) => {
            eprintln!("Error finding codex home: {err}");
            std::process::exit(1);
        }
    };

    let housekeeping_home = code_home.clone();
    let housekeeping_handle = thread_spawner::spawn_lightweight("housekeeping", move || {
        if let Err(err) = code_core::run_housekeeping_if_due(&housekeeping_home) {
            tracing::warn!("code home housekeeping failed: {err}");
        }
    });

    let workspace_write_network_access_explicit = {
        let cli_override = cli_kv_overrides
            .iter()
            .any(|(path, _)| path == "sandbox_workspace_write.network_access");

        if cli_override {
            true
        } else {
            match load_config_as_toml(&code_home) {
                Ok(raw) => raw
                    .get("sandbox_workspace_write")
                    .and_then(|value| value.as_table())
                    .map_or(false, |table| table.contains_key("network_access")),
                Err(_) => false,
            }
        }
    };

    let mut config = {
        // Load configuration and support CLI overrides.

        #[allow(clippy::print_stderr)]
        match Config::load_with_cli_overrides(cli_kv_overrides.clone(), overrides) {
            Ok(config) => config,
            Err(err) => {
                eprintln!("Error loading configuration: {err}");
                std::process::exit(1);
            }
        }
    };

    let startup_footer_notice = None;

    // we load config.toml here to determine project state.
    #[allow(clippy::print_stderr)]
    let (config_toml, theme_set_in_config_file) = {
        let theme_set_in_config_file = theme_configured_in_config_file(&code_home);

        match load_config_as_toml_with_cli_overrides(&code_home, cli_kv_overrides.clone()) {
            Ok(config_toml) => (config_toml, theme_set_in_config_file),
            Err(err) => {
                eprintln!("Error loading config.toml: {err}");
                std::process::exit(1);
            }
        }
    };

    let theme_configured_explicitly = theme_set_in_config_file || theme_override_in_cli;

    let should_show_trust_screen = determine_repo_trust_state(
        &mut config,
        &config_toml,
        approval_policy,
        sandbox_mode,
        cli.config_profile.clone(),
        workspace_write_network_access_explicit,
    )?;

    let log_dir = code_core::config::log_dir(&config)?;
    std::fs::create_dir_all(&log_dir)?;
    // Open (or create) your log file, appending to it.
    let mut log_file_opts = OpenOptions::new();
    log_file_opts.create(true).append(true);

    // Ensure the file is only readable and writable by the current user.
    // Doing the equivalent to `chmod 600` on Windows is quite a bit more code
    // and requires the Windows API crates, so we can reconsider that when
    // Code CLI is officially supported on Windows.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        log_file_opts.mode(0o600);
    }

    let log_file = log_file_opts.open(log_dir.join("codex-tui.log"))?;

    // Wrap file in non‑blocking writer.
    let (log_writer, _log_guard) = non_blocking(log_file);

    let critical_dir = {
        let mut path = code_home.clone();
        path.push("logs");
        path
    };
    std::fs::create_dir_all(&critical_dir)?;
    let critical_appender = rolling::daily(&critical_dir, "critical.log");
    let (critical_writer, _critical_guard) = non_blocking(critical_appender);

    let default_filter = if cli.debug {
        "code_core=info,code_tui=info,code_browser=info"
    } else {
        "code_core=warn,code_tui=warn,code_browser=warn"
    };

    // use RUST_LOG env var, defaulting based on debug flag.
    let env_filter = || {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter))
    };

    let env_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .with_ansi(false)
        .with_writer(log_writer)
        .with_filter(env_filter());

    let critical_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_ansi(false)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_writer(critical_writer)
        .with_filter(LevelFilter::ERROR);

    let _ = tracing_subscriber::registry()
        .with(env_layer)
        .with(critical_layer)
        .try_init();

    if cli.oss {
        code_ollama::ensure_oss_ready(&config)
            .await
            .map_err(|e| std::io::Error::other(format!("OSS setup failed: {e}")))?;
    }

    let _otel = code_core::otel_init::build_provider(&config, env!("CARGO_PKG_VERSION"));

    let latest_upgrade_version = if crate::updates::upgrade_ui_enabled() {
        updates::get_upgrade_version(&config)
    } else {
        None
    };

    let run_result = run_ratatui_app(
        cli,
        config,
        should_show_trust_screen,
        startup_footer_notice,
        latest_upgrade_version,
        theme_configured_explicitly,
    );

    if let Some(handle) = housekeeping_handle {
        if let Err(err) = handle.join() {
            tracing::warn!("code home housekeeping task panicked: {err:?}");
        }
    } else {
        tracing::warn!("housekeeping thread spawn skipped: background thread limit reached");
    }

    run_result.map_err(|err| std::io::Error::other(err.to_string()))
}

pub(crate) fn install_unified_panic_hook() {
    static PANIC_HOOK_ONCE: Once = Once::new();

    PANIC_HOOK_ONCE.call_once(|| {
        let prev_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let current_thread = std::thread::current();
            let thread_name = current_thread.name().unwrap_or("unnamed");
            let thread_id = format!("{:?}", current_thread.id());

            if let Some(location) = info.location() {
                tracing::error!(
                    thread_name,
                    thread_id,
                    file = location.file(),
                    line = location.line(),
                    column = location.column(),
                    panic = %info,
                    "panic captured"
                );
            } else {
                tracing::error!(
                    thread_name,
                    thread_id,
                    panic = %info,
                    "panic captured"
                );
            }

            if let Err(err) = crate::tui::restore() {
                tracing::warn!("failed to restore terminal after panic: {err}");
            }

            prev_hook(info);
            std::process::exit(1);
        }));
    });
}

fn run_ratatui_app(
    cli: Cli,
    mut config: Config,
    should_show_trust_screen: bool,
    startup_footer_notice: Option<String>,
    latest_upgrade_version: Option<String>,
    theme_configured_explicitly: bool,
) -> color_eyre::Result<ExitSummary> {
    color_eyre::install()?;
    install_unified_panic_hook();
    maybe_apply_terminal_theme_detection(&mut config, theme_configured_explicitly);

    let (mut terminal, terminal_info) = tui::init(&config)?;
    if config.tui.alternate_screen {
        terminal.clear()?;
    } else {
        // Start in standard terminal mode: leave alt screen and DO NOT clear
        // the normal buffer. We want prior shell history to remain intact and
        // new chat output to append inline into scrollback. Ensure line wrap is
        // enabled and cursor is left where the shell put it.
        let _ = tui::leave_alt_screen_only();
        let _ = ratatui::crossterm::execute!(
            std::io::stdout(),
            ratatui::crossterm::terminal::EnableLineWrap
        );
    }

    // Initialize high-fidelity session event logging if enabled.
    session_log::maybe_init(&config);

    let Cli {
        prompt,
        images,
        debug,
        order,
        timing,
        resume_picker,
        resume_last: _,
        resume_session_id: _,
        ..
    } = cli;
    let mut app = App::new(
        config.clone(),
        prompt,
        images,
        should_show_trust_screen,
        debug,
        order,
        terminal_info,
        timing,
        resume_picker,
        startup_footer_notice,
        latest_upgrade_version,
    );

    let app_result = app.run(&mut terminal);
    let session_id = app.session_id();
    let usage = app.token_usage();

    // Optionally print timing summary to stderr after restoring the terminal.
    let timing_summary = app.perf_summary();

    restore();

    // After restoring the terminal, clean up any worktrees created by this process.
    cleanup_session_worktrees_and_print();
    // Mark the end of the recorded session.
    session_log::log_session_end();
    if let Some(summary) = timing_summary {
        print_timing_summary(&summary);
    }

    #[cfg(unix)]
    let sigterm_triggered = app.sigterm_triggered();
    #[cfg(unix)]
    app.clear_sigterm_guard();
    drop(app);
    #[cfg(unix)]
    if sigterm_triggered {
        unsafe {
            libc::raise(libc::SIGTERM);
        }
    }

    // ignore error when collecting usage – report underlying error instead
    app_result.map(|_| ExitSummary {
        token_usage: usage,
        session_id,
    })
}

#[expect(
    clippy::print_stderr,
    reason = "TUI should no longer be displayed, so we can write to stderr."
)]
fn restore() {
    if let Err(err) = tui::restore() {
        eprintln!(
            "failed to restore terminal. Run `reset` or restart your terminal to recover: {err}"
        );
    }
}

#[allow(clippy::print_stderr)]
fn print_timing_summary(summary: &str) {
    eprintln!("\n== Timing Summary ==\n{}", summary);
}

#[allow(clippy::print_stdout, clippy::print_stderr)]
fn cleanup_session_worktrees_and_print() {
    let pid = std::process::id();
    let home = match std::env::var_os("HOME") { Some(h) => std::path::PathBuf::from(h), None => return };
    let session_dir = home.join(".code").join("working").join("_session");
    let file = session_dir.join(format!("pid-{}.txt", pid));
    reclaim_worktrees_from_file(&file, "current session");
}

fn reclaim_worktrees_from_file(path: &std::path::Path, label: &str) {
    use std::process::Command;

    let Ok(data) = std::fs::read_to_string(path) else {
        let _ = std::fs::remove_file(path);
        return;
    };

    let mut entries: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();
    for line in data.lines() {
        if line.trim().is_empty() { continue; }
        if let Some((root_s, path_s)) = line.split_once('\t') {
            entries.push((std::path::PathBuf::from(root_s), std::path::PathBuf::from(path_s)));
        }
    }

    use std::collections::HashSet;
    let mut seen = HashSet::new();
    entries.retain(|(_, p)| seen.insert(p.clone()));
    if entries.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }

    eprintln!("Cleaning remaining worktrees for {} ({}).", label, entries.len());
    for (git_root, worktree) in entries {
        let Some(wt_str) = worktree.to_str() else { continue };
        let _ = Command::new("git")
            .current_dir(&git_root)
            .args(["worktree", "remove", wt_str, "--force"])
            .output();
        let _ = std::fs::remove_dir_all(&worktree);
    }
    let _ = std::fs::remove_file(path);
}

fn maybe_apply_terminal_theme_detection(config: &mut Config, theme_configured_explicitly: bool) {
    if theme_configured_explicitly {
        tracing::info!(
            "Terminal theme autodetect skipped due to explicit theme configuration"
        );
        return;
    }

    let theme = &mut config.tui.theme;

    let autodetect_disabled = std::env::var("CODE_DISABLE_THEME_AUTODETECT")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "True"))
        .unwrap_or(false);
    if autodetect_disabled {
        tracing::info!("Terminal theme autodetect disabled via CODE_DISABLE_THEME_AUTODETECT");
        return;
    }

    if theme.name != ThemeName::LightPhoton {
        return;
    }

    if theme.label.is_some() || theme.is_dark.is_some() {
        return;
    }

    if theme.colors != ThemeColors::default() {
        return;
    }

    let term = std::env::var("TERM").ok().filter(|value| !value.is_empty());
    let term_program = std::env::var("TERM_PROGRAM").ok().filter(|value| !value.is_empty());
    let term_program_version =
        std::env::var("TERM_PROGRAM_VERSION").ok().filter(|value| !value.is_empty());
    let colorfgbg = std::env::var("COLORFGBG").ok().filter(|value| !value.is_empty());

    if let Some(cached) = config.tui.cached_terminal_background.as_ref() {
        if cached_background_matches_env(
            cached,
            &term,
            &term_program,
            &term_program_version,
            &colorfgbg,
        ) {
            tracing::debug!(
                source = cached.source.as_deref().unwrap_or("cached"),
                "Using cached terminal background detection result",
            );
            apply_detected_theme(theme, cached.is_dark);
            return;
        }
    }

    match crate::terminal_info::detect_dark_terminal_background() {
        Some(detection) => {
            apply_detected_theme(theme, detection.is_dark);

            let source = match detection.source {
                crate::terminal_info::TerminalBackgroundSource::Osc11 => "osc-11",
                crate::terminal_info::TerminalBackgroundSource::ColorFgBg => "colorfgbg",
            };

            let cache = CachedTerminalBackground {
                is_dark: detection.is_dark,
                term,
                term_program,
                term_program_version,
                colorfgbg,
                source: Some(source.to_string()),
                rgb: detection
                    .rgb
                    .map(|(r, g, b)| format!("{:02x}{:02x}{:02x}", r, g, b)),
            };

            config.tui.cached_terminal_background = Some(cache.clone());
            if let Err(err) = set_cached_terminal_background(&config.code_home, &cache) {
                tracing::warn!("Failed to persist terminal background autodetect result: {err}");
            }
        }
        None => {
            tracing::debug!(
                "Terminal theme autodetect unavailable; using configured default theme"
            );
        }
    }
}

fn apply_detected_theme(theme: &mut ThemeConfig, is_dark: bool) {
    if is_dark {
        theme.name = ThemeName::DarkCarbonNight;
        tracing::info!(
            "Detected dark terminal background; switching default theme to Dark - Carbon Night"
        );
    } else {
        tracing::info!(
            "Detected light terminal background; keeping default Light - Photon theme"
        );
    }
}

fn cached_background_matches_env(
    cached: &CachedTerminalBackground,
    term: &Option<String>,
    term_program: &Option<String>,
    term_program_version: &Option<String>,
    colorfgbg: &Option<String>,
) -> bool {
    fn matches(expected: &Option<String>, actual: &Option<String>) -> bool {
        match expected {
            Some(expected) => actual.as_ref().map(|value| value == expected).unwrap_or(false),
            None => true,
        }
    }

    matches(&cached.term, term)
        && matches(&cached.term_program, term_program)
        && matches(&cached.term_program_version, term_program_version)
        && matches(&cached.colorfgbg, colorfgbg)
}

/// Minimal login status indicator for onboarding flow.
#[derive(Debug, Clone, Copy)]
pub enum LoginStatus {
    NotAuthenticated,
    AuthMode(AuthMode),
}

/// Determine current login status based on auth.json presence.
pub fn get_login_status(config: &Config) -> LoginStatus {
    let code_home = config.code_home.clone();
    match CodexAuth::from_code_home(&code_home, AuthMode::ChatGPT, &config.responses_originator_header) {
        Ok(Some(auth)) => LoginStatus::AuthMode(auth.mode),
        _ => LoginStatus::NotAuthenticated,
    }
}

/// Determine if user has configured a sandbox / approval policy,
/// or if the current cwd project is trusted, and updates the config
/// accordingly.
fn determine_repo_trust_state(
    config: &mut Config,
    config_toml: &ConfigToml,
    approval_policy_overide: Option<AskForApproval>,
    sandbox_mode_override: Option<SandboxMode>,
    config_profile_override: Option<String>,
    workspace_write_network_access_explicit: bool,
) -> std::io::Result<bool> {
    let config_profile = config_toml.get_config_profile(config_profile_override)?;

    // If this project has explicit per-project overrides for approval and/or sandbox,
    // honor them and skip the trust screen entirely.
    let proj_key = config.cwd.to_string_lossy().to_string();
    let has_per_project_overrides = config_toml
        .projects
        .as_ref()
        .and_then(|m| m.get(&proj_key))
        .map(|p| p.approval_policy.is_some() || p.sandbox_mode.is_some())
        .unwrap_or(false);
    if has_per_project_overrides {
        return Ok(false);
    }

    if approval_policy_overide.is_some() || sandbox_mode_override.is_some() {
        // if the user has overridden either approval policy or sandbox mode,
        // skip the trust flow
        Ok(false)
    } else if config_profile.approval_policy.is_some() {
        // if the user has specified settings in a config profile, skip the trust flow
        // todo: profile sandbox mode?
        Ok(false)
    } else if config_toml.approval_policy.is_some() || config_toml.sandbox_mode.is_some() {
        // if the user has specified either approval policy or sandbox mode in config.toml
        // skip the trust flow
        Ok(false)
    } else if config_toml.is_cwd_trusted(&config.cwd) {
        // If the current cwd project is trusted and no explicit config has been set,
        // default to fully trusted, non‑interactive execution to match expected behavior.
        // This restores the previous semantics before the recent trust‑flow refactor.
        if let Some(workspace_write) = config_toml.sandbox_workspace_write.as_ref() {
            // Honor explicit sandbox WorkspaceWrite protections (like allow_git_writes = false)
            // even when the project is marked as trusted.
            if !workspace_write.allow_git_writes {
                // Maintain the historical networking behaviour from DangerFullAccess: allow outbound
                // network even when we pivot into WorkspaceWrite solely to protect `.git`.
                let network_access = if workspace_write.network_access {
                    true
                } else if workspace_write_network_access_explicit {
                    false
                } else {
                    true
                };
                config.approval_policy = AskForApproval::Never;
                config.sandbox_policy = SandboxPolicy::WorkspaceWrite {
                    writable_roots: workspace_write.writable_roots.clone(),
                    network_access,
                    exclude_tmpdir_env_var: workspace_write.exclude_tmpdir_env_var,
                    exclude_slash_tmp: workspace_write.exclude_slash_tmp,
                    allow_git_writes: workspace_write.allow_git_writes,
                };
                return Ok(false);
            }
        }

        config.approval_policy = AskForApproval::Never;
        config.sandbox_policy = SandboxPolicy::DangerFullAccess;
        Ok(false)
    } else {
        // if none of the above conditions are met (and no per‑project overrides), show the trust screen
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_core::config::ProjectConfig;
    use code_core::config_types::SandboxWorkspaceWrite;
    use code_core::protocol::AskForApproval;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn make_trusted_config(
        sandbox_workspace_write: Option<SandboxWorkspaceWrite>,
    ) -> std::io::Result<(Config, ConfigToml)> {
        let code_home = TempDir::new()?;
        let workspace = TempDir::new()?;

        let mut config_toml = ConfigToml::default();
        config_toml.sandbox_workspace_write = sandbox_workspace_write;

        let mut projects = HashMap::new();
        projects.insert(
            workspace.path().to_string_lossy().to_string(),
            ProjectConfig {
                trust_level: Some("trusted".to_string()),
                approval_policy: None,
                sandbox_mode: None,
                always_allow_commands: None,
                hooks: vec![],
                commands: vec![],
            },
        );
        config_toml.projects = Some(projects);

        let overrides = ConfigOverrides {
            cwd: Some(workspace.path().to_path_buf()),
            ..Default::default()
        };

        let config = Config::load_from_base_config_with_overrides(
            config_toml.clone(),
            overrides,
            code_home.path().to_path_buf(),
        )?;

        Ok((config, config_toml))
    }

    #[test]
    fn trusted_workspace_honors_allow_git_writes_override() -> std::io::Result<()> {
        let (mut config, config_toml) = make_trusted_config(Some(SandboxWorkspaceWrite {
            allow_git_writes: false,
            ..Default::default()
        }))?;

        let show_trust = determine_repo_trust_state(
            &mut config,
            &config_toml,
            None,
            None,
            None,
            false,
        )?;
        assert!(!show_trust);

        match &config.sandbox_policy {
            SandboxPolicy::WorkspaceWrite {
                allow_git_writes,
                network_access,
                ..
            } => {
                assert!(!allow_git_writes);
                assert!(*network_access, "trusted WorkspaceWrite should retain network access");
            }
            other => panic!("expected workspace-write sandbox, got {other:?}"),
        }

        assert!(matches!(config.approval_policy, AskForApproval::Never));

        Ok(())
    }

    #[test]
    fn trusted_workspace_default_stays_danger_full_access() -> std::io::Result<()> {
        let (mut config, config_toml) = make_trusted_config(None)?;

        let show_trust = determine_repo_trust_state(
            &mut config,
            &config_toml,
            None,
            None,
            None,
            false,
        )?;
        assert!(!show_trust);

        assert!(matches!(config.sandbox_policy, SandboxPolicy::DangerFullAccess));
        assert!(matches!(config.approval_policy, AskForApproval::Never));

        Ok(())
    }

    #[test]
    fn trusted_workspace_respects_explicit_network_disable() -> std::io::Result<()> {
        let (mut config, config_toml) = make_trusted_config(Some(SandboxWorkspaceWrite {
            allow_git_writes: false,
            network_access: false,
            ..Default::default()
        }))?;

        let show_trust = determine_repo_trust_state(
            &mut config,
            &config_toml,
            None,
            None,
            None,
            true,
        )?;
        assert!(!show_trust);

        match &config.sandbox_policy {
            SandboxPolicy::WorkspaceWrite {
                allow_git_writes,
                network_access,
                ..
            } => {
                assert!(!allow_git_writes);
                assert!(!network_access, "explicit opt-out should disable network access");
            }
            other => panic!("expected workspace-write sandbox, got {other:?}"),
        }

        Ok(())
    }
}
