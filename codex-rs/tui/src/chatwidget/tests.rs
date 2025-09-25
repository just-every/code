#![allow(clippy::unwrap_used, clippy::expect_used, unnameable_test_items)]

use super::*;
use crate::app_event::{AppEvent, BackgroundPlacement};
use crate::app_event_sender::AppEventSender;
use crate::slash_command::SlashCommand;
use codex_core::config::Config;
use codex_core::config::ConfigOverrides;
use codex_core::config::ConfigToml;
use codex_core::plan_tool::PlanItemArg;
use codex_core::plan_tool::StepStatus;
use codex_core::plan_tool::UpdatePlanArgs;
use codex_core::protocol::AgentMessageDeltaEvent;
use codex_core::protocol::AgentMessageEvent;
use codex_core::protocol::AgentStatusUpdateEvent;
use codex_core::protocol::AgentInfo as ProtocolAgentInfo;
use codex_core::protocol::AgentReasoningDeltaEvent;
use codex_core::protocol::AgentReasoningEvent;
use codex_core::protocol::ApplyPatchApprovalRequestEvent;
use codex_core::protocol::Event;
use codex_core::protocol::EventMsg;
use codex_core::protocol::ExecApprovalRequestEvent;
use codex_core::protocol::ExecCommandBeginEvent;
use codex_core::protocol::ExecCommandEndEvent;
use codex_core::protocol::FileChange;
use codex_core::protocol::RateLimitSnapshotEvent;
use codex_core::protocol::PatchApplyBeginEvent;
use codex_core::protocol::PatchApplyEndEvent;
use codex_core::protocol::ProAction;
use codex_core::protocol::StreamErrorEvent;
use codex_core::protocol::TaskCompleteEvent;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use chrono::{Duration as ChronoDuration, Local, Utc};
use insta::assert_snapshot;
use pretty_assertions::assert_eq;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use tokio::sync::mpsc::unbounded_channel;
use strip_ansi_escapes::strip as strip_ansi_bytes;

fn test_config() -> Config {
    // Use base defaults to avoid depending on host state.
    codex_core::config::Config::load_from_base_config_with_overrides(
        ConfigToml::default(),
        ConfigOverrides::default(),
        std::env::temp_dir(),
    )
    .expect("config")
}

#[test]
fn final_answer_without_newline_is_flushed_immediately() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    // Set up a VT100 test terminal to capture ANSI visual output
    let width: u16 = 80;
    let height: u16 = 2000;
    let viewport = ratatui::layout::Rect::new(0, height - 1, width, 1);
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = crate::custom_terminal::Terminal::with_options(backend)
        .expect("failed to construct terminal");
    terminal.set_viewport_area(viewport);

    // Simulate a streaming answer without any newline characters.
    chat.handle_codex_event(Event {
        id: "sub-a".into(),
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "Hi! How can I help with codex-rs or anything else today?".into(),
        }),
    });

    // Now simulate the final AgentMessage which should flush the pending line immediately.
    chat.handle_codex_event(Event {
        id: "sub-a".into(),
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "Hi! How can I help with codex-rs or anything else today?".into(),
        }),
    });

    // Drain history insertions and verify the final line is present.
    // We no longer emit a visible "codex" header during streaming.
    let cells = drain_insert_history(&rx);
    let found_final = cells.iter().any(|lines| {
        let s = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|sp| sp.content.clone())
            .collect::<String>();
        s.contains("Hi! How can I help with codex-rs or anything else today?")
    });
    assert!(
        found_final,
        "expected final answer text to be flushed to history"
    );
}

fn cell_texts(chat: &ChatWidget<'_>) -> Vec<String> {
    chat
        .history_cells
        .iter()
        .map(|cell| {
            let mut out = Vec::new();
            for line in cell.display_lines() {
                let mut buf = String::new();
                for span in line.spans {
                    buf.push_str(span.content.as_ref());
                }
                out.push(buf);
            }
            out.join("\n")
        })
        .collect()
}

#[test]
fn background_events_append_in_arrival_order() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual();

    chat.insert_background_event_with_placement(
        "first background".to_string(),
        BackgroundPlacement::Tail,
    );
    chat.insert_background_event_with_placement(
        "second background".to_string(),
        BackgroundPlacement::Tail,
    );

    let texts = cell_texts(&chat);
    assert_eq!(texts, vec!["first background".to_string(), "second background".to_string()]);
}

#[test]
fn background_event_before_next_output_precedes_later_cells() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual();

    chat.insert_background_event_with_placement(
        "initial".to_string(),
        BackgroundPlacement::Tail,
    );
    chat.insert_background_event_with_placement(
        "guard".to_string(),
        BackgroundPlacement::BeforeNextOutput,
    );
    chat.push_background_tail("tail".to_string());

    let texts = cell_texts(&chat);
    assert_eq!(texts, vec![
        "initial".to_string(),
        "guard".to_string(),
        "tail".to_string(),
    ]);
}

#[test]
fn limits_overlay_loading_when_snapshot_missing() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual();
    chat.rate_limit_snapshot = None;
    chat.rate_limit_fetch_inflight = true;
    chat.rate_limit_last_fetch_at = Some(Utc::now());

    chat.add_limits_output();

    let overlay = chat.limits.overlay.as_ref().expect("overlay present");
    let lines = overlay.lines_for_width(60);
    let text: Vec<String> = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect();

    assert!(
        text.iter()
            .any(|line| line.contains("Loading...")),
        "expected loading message in overlay: {text:?}"
    );
    assert!(text.iter().all(|line| !line.contains("/limits")));
    assert!(chat.rate_limit_fetch_inflight);
}

#[test]
fn limits_overlay_renders_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual();
    chat.rate_limit_snapshot = Some(RateLimitSnapshotEvent {
        primary_used_percent: 30.0,
        secondary_used_percent: 60.0,
        primary_to_secondary_ratio_percent: 0.0,
        primary_window_minutes: 300,
        secondary_window_minutes: 10_080,
        primary_reset_after_seconds: Some(600),
        secondary_reset_after_seconds: Some(3_600),
    });
    chat.update_rate_limit_resets(chat.rate_limit_snapshot.as_ref().unwrap());
    chat.rate_limit_last_fetch_at = Some(Utc::now());

    chat.add_limits_output();

    let overlay = chat.limits.overlay.as_ref().expect("overlay present");
    let lines = overlay.lines_for_width(80);
    let strings: Vec<String> = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect();

    let joined = strings.join("\n");
    assert!(joined.contains("Hourly Limit"), "overlay text: {joined}");
    assert!(joined.contains("Weekly Limit"), "overlay text: {joined}");
    assert!(joined.contains(" Type: "));
    assert!(joined.contains(" Plan: "));
    assert!(joined.contains("Resets"), "overlay text: {joined}");
    assert!(
        !joined.contains("awaiting reset timing"),
        "expected reset timings to render: {joined}"
    );
    assert!(joined.contains(" Total: "));
    assert!(joined.contains("Chart"));
    assert!(joined.contains("7 Day History"));
    assert!(!joined.contains("/limits"));
    assert!(!joined.contains("Within current limits"));

    let header_idx = strings
        .iter()
        .position(|line| line.contains("7 Day History"))
        .expect("expected usage header");
    let latest_label = Local::now().format("%b %d").to_string();
    let yesterday_label = (Local::now().date_naive() - ChronoDuration::days(1))
        .format("%b %d")
        .to_string();
    let latest_line = strings
        .get(header_idx + 1)
        .expect("expected latest usage line");
    let second_line = strings
        .get(header_idx + 2)
        .expect("expected second usage line");
    assert!(latest_line.contains(&latest_label));
    assert!(second_line.contains(&yesterday_label));
}

#[tokio::test(flavor = "current_thread")]
async fn helpers_are_available_and_do_not_panic() {
    let (tx_raw, _rx) = channel::<AppEvent>();
    let tx = AppEventSender::new(tx_raw);
    let cfg = test_config();
    let term = crate::tui::TerminalInfo {
        picker: None,
        font_size: (8, 16),
    };
    let mut w = ChatWidget::new(
        cfg,
        tx,
        None,
        Vec::new(),
        false,
        term,
        false,
        None,
    );
    // Basic construction sanity.
    let _ = &mut w;
}

// --- Helpers for tests that need direct construction and event draining ---
fn make_chatwidget_manual() -> (
    ChatWidget<'static>,
    std::sync::mpsc::Receiver<AppEvent>,
    tokio::sync::mpsc::UnboundedReceiver<Op>,
) {
    let (tx_raw, rx) = channel::<AppEvent>();
    let app_event_tx = AppEventSender::new(tx_raw);
    let (op_tx, op_rx) = unbounded_channel::<Op>();
    let cfg = test_config();
    let bottom = BottomPane::new(BottomPaneParams {
        app_event_tx: app_event_tx.clone(),
        has_input_focus: true,
        enhanced_keys_supported: false,
        using_chatgpt_auth: false,
    });
    let widget = ChatWidget {
        app_event_tx,
        codex_op_tx: op_tx,
        bottom_pane: bottom,
        active_exec_cell: None,
        config: cfg.clone(),
        latest_upgrade_version: None,
        initial_user_message: None,
        total_token_usage: TokenUsage::default(),
        last_token_usage: TokenUsage::default(),
        rate_limit_snapshot: None,
        rate_limit_warnings: Default::default(),
        rate_limit_fetch_inflight: false,
        rate_limit_fetch_placeholder: None,
        rate_limit_fetch_ack_pending: false,
        #[cfg(not(feature = "legacy_tests"))]
        ghost_snapshots: Vec::new(),
        #[cfg(not(feature = "legacy_tests"))]
        ghost_snapshots_disabled: false,
        #[cfg(not(feature = "legacy_tests"))]
        ghost_snapshots_disabled_reason: None,
        stream: StreamController::new(cfg),
        last_stream_kind: None,
        running_commands: HashMap::new(),
        pending_exec_completions: Vec::new(),
        task_complete_pending: false,
        interrupts: InterruptManager::new(),
        needs_redraw: false,
        agents_terminal: AgentsTerminalState::new(),
    };
    (widget, rx, op_rx)
}

pub(crate) fn make_chatwidget_manual_with_sender() -> (
    ChatWidget,
    AppEventSender,
    tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    tokio::sync::mpsc::UnboundedReceiver<Op>,
) {
    let (widget, rx, op_rx) = make_chatwidget_manual();
    let app_event_tx = widget.app_event_tx.clone();
    (widget, app_event_tx, rx, op_rx)
}

fn drain_insert_history(
    rx: &std::sync::mpsc::Receiver<AppEvent>,
) -> Vec<Vec<ratatui::text::Line<'static>>> {
    let mut out = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        if let AppEvent::InsertHistory(lines) = ev {
            out.push(lines);
        }
    }
    out
}

fn lines_to_single_string(lines: &[ratatui::text::Line<'static>]) -> String {
    let mut s = String::new();
    for line in lines {
        for span in &line.spans {
            s.push_str(&span.content);
        }
        s.push('\n');
    }
    s
}

#[derive(Clone, Copy)]
enum ScriptStep {
    Key(KeyCode, KeyModifiers),
}

impl ScriptStep {
    fn key_char(c: char) -> Self {
        ScriptStep::Key(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn enter() -> Self {
        ScriptStep::Key(KeyCode::Enter, KeyModifiers::NONE)
    }
}

fn run_script(chat: &mut ChatWidget<'_>, steps: &[ScriptStep], rx: &std::sync::mpsc::Receiver<AppEvent>) {
    for step in steps {
        if let ScriptStep::Key(code, modifiers) = step {
            chat.handle_key_event(KeyEvent::new(*code, *modifiers));
            pump_app_events(chat, rx);
        }
    }
    pump_app_events(chat, rx);
}

#[test]
fn agents_terminal_tracks_logs() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();
    chat.prepare_agents();

    let mut event = AgentStatusUpdateEvent {
        agents: vec![ProtocolAgentInfo {
            id: "agent-1".into(),
            name: "Gemini".into(),
            status: "running".into(),
            batch_id: None,
            model: Some("gemini-pro".into()),
            last_progress: Some("12:00:01: creating worktree".into()),
            result: None,
            error: None,
        }],
        context: Some("monorepo".into()),
        task: Some("upgrade agents UI".into()),
    };

    chat.handle_codex_event(Event {
        id: "agents".into(),
        msg: EventMsg::AgentStatusUpdate(event.clone()),
    });

    let entry = chat
        .agents_terminal
        .entries
        .get("agent-1")
        .expect("expected agent entry");
    assert_eq!(entry.logs.len(), 2, "expect status + initial progress logged");

    // Duplicate update should not add new logs
    chat.handle_codex_event(Event {
        id: "agents".into(),
        msg: EventMsg::AgentStatusUpdate(event.clone()),
    });
    let entry = chat
        .agents_terminal
        .entries
        .get("agent-1")
        .unwrap();
    assert_eq!(entry.logs.len(), 2, "duplicate progress should be deduped");

    // Completed update should append status + result
    event.agents[0].status = "completed".into();
    event.agents[0].last_progress = Some("12:04:12: finalizing".into());
    event.agents[0].result = Some("Plan delivered".into());

    chat.handle_codex_event(Event {
        id: "agents".into(),
        msg: EventMsg::AgentStatusUpdate(event),
    });

    let entry = chat
        .agents_terminal
        .entries
        .get("agent-1")
        .unwrap();
    assert!(
        entry
            .logs
            .iter()
            .any(|log| matches!(log.kind, AgentLogKind::Result) && log.message.contains("Plan delivered")),
        "result log expected"
    );
    assert!(
        entry
            .logs
            .iter()
            .any(|log| matches!(log.kind, AgentLogKind::Status) && log.message.contains("Completed")),
        "status transition expected"
    );

    drop(rx);
}

#[test]
fn agents_terminal_toggle_via_shortcuts() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();
    chat.prepare_agents();

    let event = AgentStatusUpdateEvent {
        agents: vec![ProtocolAgentInfo {
            id: "agent-1".into(),
            name: "Gemini".into(),
            status: "running".into(),
            batch_id: None,
            model: Some("gemini-pro".into()),
            last_progress: Some("progress".into()),
            result: None,
            error: None,
        }],
        context: None,
        task: None,
    };

    chat.handle_codex_event(Event {
        id: "agents".into(),
        msg: EventMsg::AgentStatusUpdate(event),
    });

    assert!(chat.agents_terminal.order.contains(&"agent-1".to_string()));
    assert!(!chat.agents_terminal.active);

    chat.handle_key_event(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    pump_app_events(&mut chat, &rx);
    assert!(
        chat.agents_terminal.active,
        "Ctrl+Shift+A should open terminal"
    );

    // Esc should exit the terminal view
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    pump_app_events(&mut chat, &rx);
    assert!(!chat.agents_terminal.active, "Esc should exit terminal");
}

#[test]
fn agents_terminal_focus_and_scroll_controls() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();
    chat.prepare_agents();

    let event = AgentStatusUpdateEvent {
        agents: vec![ProtocolAgentInfo {
            id: "agent-1".into(),
            name: "Gemini".into(),
            status: "running".into(),
            batch_id: None,
            model: Some("gemini-pro".into()),
            last_progress: Some("progress".into()),
            result: None,
            error: None,
        }],
        context: None,
        task: None,
    };

    chat.handle_codex_event(Event {
        id: "agents".into(),
        msg: EventMsg::AgentStatusUpdate(event),
    });

    chat.handle_key_event(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    pump_app_events(&mut chat, &rx);
    assert_eq!(chat.agents_terminal.focus(), AgentsTerminalFocus::Sidebar);

    chat.layout.last_history_viewport_height.set(5);
    chat.layout.last_max_scroll.set(5);

    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    pump_app_events(&mut chat, &rx);
    assert_eq!(chat.agents_terminal.focus(), AgentsTerminalFocus::Detail);

    chat.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    pump_app_events(&mut chat, &rx);
    assert_eq!(chat.layout.scroll_offset, 1, "Up should scroll output when focused");

    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    pump_app_events(&mut chat, &rx);
    assert!(chat.agents_terminal.active, "Overlay should remain open after first Esc");
    assert_eq!(chat.agents_terminal.focus(), AgentsTerminalFocus::Sidebar);

    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    pump_app_events(&mut chat, &rx);
    assert!(!chat.agents_terminal.active, "Second Esc should close overlay");
}

#[test]
fn agents_terminal_esc_closes_from_sidebar() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();
    chat.prepare_agents();

    let event = AgentStatusUpdateEvent {
        agents: vec![ProtocolAgentInfo {
            id: "agent-1".into(),
            name: "Gemini".into(),
            status: "running".into(),
            batch_id: None,
            model: Some("gemini-pro".into()),
            last_progress: None,
            result: None,
            error: None,
        }],
        context: None,
        task: None,
    };

    chat.handle_codex_event(Event {
        id: "agents".into(),
        msg: EventMsg::AgentStatusUpdate(event),
    });

    chat.handle_key_event(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    pump_app_events(&mut chat, &rx);
    assert!(chat.agents_terminal.active, "overlay should open");
    assert_eq!(chat.agents_terminal.focus(), AgentsTerminalFocus::Sidebar);

    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    pump_app_events(&mut chat, &rx);
    assert!(!chat.agents_terminal.active, "Esc should close from sidebar focus");
}

fn pump_app_events(chat: &mut ChatWidget<'_>, rx: &std::sync::mpsc::Receiver<AppEvent>) {
    while let Ok(event) = rx.try_recv() {
        match event {
            AppEvent::PrepareAgents => chat.prepare_agents(),
            AppEvent::DispatchCommand(SlashCommand::Agents, args) => {
                chat.handle_agents_command(args);
            }
            AppEvent::DispatchCommand(SlashCommand::Undo, _command_text) => {
                chat.handle_undo_command();
            }
            AppEvent::ShowUndoOptions { index } => {
                chat.show_undo_restore_options(index);
            }
            AppEvent::PerformUndoRestore { index, restore_files, restore_conversation } => {
                chat.perform_undo_restore(index, restore_files, restore_conversation);
            }
            AppEvent::ShowAgentsOverview => chat.show_agents_overview_ui(),
            AppEvent::RequestRedraw | AppEvent::Redraw | AppEvent::ScheduleFrameIn(_) => {}
            _ => {}
        }
    }
}

fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
    let area = buffer.area();
    let mut out = String::with_capacity((area.width as usize + 1) * area.height as usize);
    for y in 0..area.height {
        for x in 0..area.width {
            out.push_str(buffer.get(x, y).symbol());
        }
        out.push('\n');
    }
    match strip_ansi_bytes(out.as_bytes()) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => out,
    }
}

fn open_fixture(name: &str) -> std::fs::File {
    // 1) Prefer fixtures within this crate
    {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("tests");
        p.push("fixtures");
        p.push(name);
        if let Ok(f) = File::open(&p) {
            return f;
        }
    }
    // 2) Fallback to parent (workspace root)
    {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("..");
        p.push(name);
        if let Ok(f) = File::open(&p) {
            return f;
        }
    }
    // 3) Last resort: CWD
    File::open(name).expect("open fixture file")
}

#[test]
fn slash_agents_opens_overview() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    let script = [
        ScriptStep::key_char('/'),
        ScriptStep::key_char('a'),
        ScriptStep::key_char('g'),
        ScriptStep::key_char('e'),
        ScriptStep::key_char('n'),
        ScriptStep::key_char('t'),
        ScriptStep::key_char('s'),
        ScriptStep::enter(),
    ];
    run_script(&mut chat, &script, &rx);

    let width: u16 = 120;
    let height = chat.desired_height(width).max(40);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
        .expect("create terminal");
    terminal
        .draw(|f| f.render_widget_ref(&chat, f.area()))
        .expect("draw agents overview");

    let plain = buffer_to_string(terminal.backend().buffer());
    let lower = plain.to_ascii_lowercase();
    assert!(lower.contains("agents"), "expected Agents heading\n{plain}");
    assert!(lower.contains("commands"), "expected Commands section\n{plain}");
    assert!(
        lower.contains("add new"),
        "expected Add new row in overview\n{plain}"
    );
}

#[test]
fn slash_undo_shows_no_snapshot_state() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    let script = [
        ScriptStep::key_char('/'),
        ScriptStep::key_char('u'),
        ScriptStep::key_char('n'),
        ScriptStep::key_char('d'),
        ScriptStep::key_char('o'),
        ScriptStep::enter(),
    ];
    run_script(&mut chat, &script, &rx);

    assert!(
        chat.bottom_pane.has_active_modal_view(),
        "expected undo modal to be active"
    );

    let width: u16 = 120;
    let height = chat.desired_height(width).max(40);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
        .expect("create terminal");
    terminal
        .draw(|f| f.render_widget_ref(&chat, f.area()))
        .expect("draw undo status");

    let plain = buffer_to_string(terminal.backend().buffer());
    let lower = plain.to_ascii_lowercase();
    assert!(
        lower.contains("no snapshots yet"),
        "expected undo status title\n{plain}"
    );
    assert!(
        lower.contains("no snapshot is available to restore"),
        "expected undo status detail\n{plain}"
    );
    assert!(
        lower.contains("chat history stays unchanged"),
        "expected scope hint to mention chat history\n{plain}"
    );
}

#[test]
fn undo_options_view_shows_toggles() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual();

    chat.history_push(history_cell::new_user_prompt("latest change".to_string()));

    let commit = codex_git_tooling::GhostCommit::new("abcdef1234567890".to_string(), None);
    chat.ghost_snapshots.push(GhostSnapshot::new(
        commit,
        Some("Initial checkpoint".to_string()),
        ConversationSnapshot::new(0, 0),
    ));

    chat.show_undo_restore_options(0);

    let width: u16 = 100;
    let height = chat.desired_height(width).max(24);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
        .expect("create terminal");
    terminal
        .draw(|f| f.render_widget_ref(&chat, f.area()))
        .expect("draw undo options");

    let plain = buffer_to_string(terminal.backend().buffer());
    let lower = plain.to_ascii_lowercase();
    assert!(
        lower.contains("restore workspace files"),
        "expected workspace toggle\n{plain}"
    );
    assert!(
        lower.contains("restore conversation"),
        "expected conversation toggle\n{plain}"
    );
}

#[test]
fn parse_pro_action_supports_core_variants() {
    let (chat, _rx, _op_rx) = make_chatwidget_manual();

    assert_eq!(chat.parse_pro_action(""), Ok(ProAction::Status));
    assert_eq!(chat.parse_pro_action("toggle"), Ok(ProAction::Toggle));
    assert_eq!(chat.parse_pro_action("on"), Ok(ProAction::On));
    assert_eq!(chat.parse_pro_action("off"), Ok(ProAction::Off));
    assert_eq!(chat.parse_pro_action("status"), Ok(ProAction::Status));
    assert_eq!(chat.parse_pro_action("auto"), Ok(ProAction::AutoToggle));
    assert_eq!(chat.parse_pro_action("auto on"), Ok(ProAction::AutoOn));
    assert_eq!(chat.parse_pro_action("auto off"), Ok(ProAction::AutoOff));
    assert_eq!(
        chat.parse_pro_action("auto status"),
        Ok(ProAction::AutoStatus)
    );

    let err = chat.parse_pro_action("auto maybe").unwrap_err();
    assert!(err.contains("Unknown /pro auto option"));
}

#[test]
fn alt_up_edits_most_recent_queued_message() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual();

    // Simulate a running task so messages would normally be queued.
    chat.bottom_pane.set_task_running(true);

    // Seed two queued messages.
    chat.queued_user_messages
        .push_back(UserMessage::from("first queued".to_string()));
    chat.queued_user_messages
        .push_back(UserMessage::from("second queued".to_string()));
    chat.refresh_queued_user_messages();

    // Press Alt+Up to edit the most recent (last) queued message.
    chat.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT));

    // Composer should now contain the last queued message.
    assert_eq!(
        chat.bottom_pane.composer_text(),
        "second queued".to_string()
    );
    // And the queue should now contain only the remaining (older) item.
    assert_eq!(chat.queued_user_messages.len(), 1);
    assert_eq!(
        chat.queued_user_messages.front().unwrap().text,
        "first queued"
    );
}

#[test]
fn exec_history_cell_shows_working_then_completed() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    // Begin command
    chat.handle_codex_event(Event {
        id: "call-1".into(),
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: "call-1".into(),
            command: vec!["bash".into(), "-lc".into(), "echo done".into()],
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            parsed_cmd: vec![codex_core::parse_command::ParsedCommand::Unknown {
                cmd: "echo done".into(),
            }],
        }),
    });

    // End command successfully
    chat.handle_codex_event(Event {
        id: "call-1".into(),
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: "call-1".into(),
            stdout: "done".into(),
            stderr: String::new(),
            aggregated_output: "done".into(),
            exit_code: 0,
            duration: std::time::Duration::from_millis(5),
        }),
    });

    let cells = drain_insert_history(&rx);
    assert_eq!(
        cells.len(),
        1,
        "expected only the completed exec cell to be inserted into history"
    );
    let blob = lines_to_single_string(&cells[0]);
    assert!(
        blob.contains("Completed"),
        "expected completed exec cell to show Completed header: {blob:?}"
    );
}

#[test]
fn exec_history_cell_shows_working_then_failed() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    // Begin command
    chat.handle_codex_event(Event {
        id: "call-2".into(),
        msg: EventMsg::ExecCommandBegin(ExecCommandBeginEvent {
            call_id: "call-2".into(),
            command: vec!["bash".into(), "-lc".into(), "false".into()],
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            parsed_cmd: vec![codex_core::parse_command::ParsedCommand::Unknown {
                cmd: "false".into(),
            }],
        }),
    });

    // End command with failure
    chat.handle_codex_event(Event {
        id: "call-2".into(),
        msg: EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: "call-2".into(),
            stdout: String::new(),
            stderr: "error".into(),
            aggregated_output: "error".into(),
            exit_code: 2,
            duration: std::time::Duration::from_millis(7),
        }),
    });

    let cells = drain_insert_history(&rx);
    assert_eq!(
        cells.len(),
        1,
        "expected only the completed exec cell to be inserted into history"
    );
    let blob = lines_to_single_string(&cells[0]);
    assert!(
        blob.contains("Failed (exit 2)"),
        "expected completed exec cell to show Failed header with exit code: {blob:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn binary_size_transcript_matches_ideal_fixture() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    // Set up a VT100 test terminal to capture ANSI visual output
    let width: u16 = 80;
    let height: u16 = 2000;
    let viewport = ratatui::layout::Rect::new(0, height - 1, width, 1);
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = crate::custom_terminal::Terminal::with_options(backend)
        .expect("failed to construct terminal");
    terminal.set_viewport_area(viewport);

    // Replay the recorded session into the widget and collect transcript
    let file = open_fixture("binary-size-log.jsonl");
    let reader = BufReader::new(file);
    let mut transcript = String::new();
    let mut ansi: Vec<u8> = Vec::new();

    for line in reader.lines() {
        let line = line.expect("read line");
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let Ok(v): Result<serde_json::Value, _> = serde_json::from_str(&line) else {
            continue;
        };
        let Some(dir) = v.get("dir").and_then(|d| d.as_str()) else {
            continue;
        };
        if dir != "to_tui" {
            continue;
        }
        let Some(kind) = v.get("kind").and_then(|k| k.as_str()) else {
            continue;
        };

        match kind {
            "codex_event" => {
                if let Some(payload) = v.get("payload") {
                    let ev: Event = serde_json::from_value(payload.clone()).expect("parse");
                    chat.handle_codex_event(ev);
                    while let Ok(app_ev) = rx.try_recv() {
                        if let AppEvent::InsertHistory(lines) = app_ev {
                            transcript.push_str(&lines_to_single_string(&lines));
                            crate::insert_history::insert_history_lines_to_writer(
                                &mut terminal,
                                &mut ansi,
                                lines,
                            );
                        }
                    }
                }
            }
            "app_event" => {
                if let Some(variant) = v.get("variant").and_then(|s| s.as_str())
                    && variant == "CommitTick"
                {
                    chat.on_commit_tick();
                    while let Ok(app_ev) = rx.try_recv() {
                        if let AppEvent::InsertHistory(lines) = app_ev {
                            transcript.push_str(&lines_to_single_string(&lines));
                            crate::insert_history::insert_history_lines_to_writer(
                                &mut terminal,
                                &mut ansi,
                                lines,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Read the ideal fixture as-is
    let mut f = open_fixture("ideal-binary-response.txt");
    let mut ideal = String::new();
    f.read_to_string(&mut ideal)
        .expect("read ideal-binary-response.txt");
    // Normalize line endings for Windows vs. Unix checkouts
    let ideal = ideal.replace("\r\n", "\n");

    // Build the final VT100 visual by parsing the ANSI stream. Trim trailing spaces per line
    // and drop trailing empty lines so the shape matches the ideal fixture exactly.
    let mut parser = vt100::Parser::new(height, width, 0);
    parser.process(&ansi);
    let mut lines: Vec<String> = Vec::with_capacity(height as usize);
    for row in 0..height {
        let mut s = String::with_capacity(width as usize);
        for col in 0..width {
            if let Some(cell) = parser.screen().cell(row, col) {
                if let Some(ch) = cell.contents().chars().next() {
                    s.push(ch);
                } else {
                    s.push(' ');
                }
            } else {
                s.push(' ');
            }
        }
        // Trim trailing spaces to match plain text fixture
        lines.push(s.trim_end().to_string());
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    // Compare only after the last session banner marker, and start at the next 'thinking' line.
    const MARKER_PREFIX: &str = ">_ You are using OpenAI Code in ";
    let last_marker_line_idx = lines
        .iter()
        .rposition(|l| l.starts_with(MARKER_PREFIX))
        .expect("marker not found in visible output");
    let thinking_line_idx = (last_marker_line_idx + 1..lines.len())
        .find(|&idx| lines[idx].trim_start() == "thinking")
        .expect("no 'thinking' line found after marker");

    let mut compare_lines: Vec<String> = Vec::new();
    // Ensure the first line is exactly 'thinking' without leading spaces to match the fixture
    compare_lines.push(lines[thinking_line_idx].trim_start().to_string());
    compare_lines.extend(lines[(thinking_line_idx + 1)..].iter().cloned());
    let visible_after = compare_lines.join("\n");

    // Optionally update the fixture when env var is set
    if std::env::var("UPDATE_IDEAL").as_deref() == Ok("1") {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("tests");
        p.push("fixtures");
        p.push("ideal-binary-response.txt");
        std::fs::write(&p, &visible_after).expect("write updated ideal fixture");
        return;
    }

    // Exact equality with pretty diff on failure
    assert_eq!(visible_after, ideal);
}

//
// Snapshot test: command approval modal
//
// Synthesizes a Code ExecApprovalRequest event to trigger the approval modal
// and snapshots the visual output using the ratatui TestBackend.
#[test]
fn approval_modal_exec_snapshot() {
    // Build a chat widget with manual channels to avoid spawning the agent.
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual();
    // Ensure policy allows surfacing approvals explicitly (not strictly required for direct event).
    chat.config.approval_policy = codex_core::protocol::AskForApproval::OnRequest;
    // Inject an exec approval request to display the approval modal.
    let ev = ExecApprovalRequestEvent {
        call_id: "call-approve-cmd".into(),
        command: vec!["bash".into(), "-lc".into(), "echo hello world".into()],
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        reason: Some("Model wants to run a command".into()),
    };
    chat.handle_codex_event(Event {
        id: "sub-approve".into(),
        msg: EventMsg::ExecApprovalRequest(ev),
    });
    // Render to a fixed-size test terminal and snapshot.
    // Call desired_height first and use that exact height for rendering.
    let height = chat.desired_height(80);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, height))
        .expect("create terminal");
    terminal
        .draw(|f| f.render_widget_ref(&chat, f.area()))
        .expect("draw approval modal");
    assert_snapshot!("approval_modal_exec", terminal.backend());
}

// Snapshot test: patch approval modal
#[test]
fn approval_modal_patch_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual();
    chat.config.approval_policy = codex_core::protocol::AskForApproval::OnRequest;

    // Build a small changeset and a reason/grant_root to exercise the prompt text.
    let mut changes = std::collections::HashMap::new();
    changes.insert(
        PathBuf::from("README.md"),
        FileChange::Add {
            content: "hello\nworld\n".into(),
        },
    );
    let ev = ApplyPatchApprovalRequestEvent {
        call_id: "call-approve-patch".into(),
        changes,
        reason: Some("The model wants to apply changes".into()),
        grant_root: Some(PathBuf::from("/tmp")),
    };
    chat.handle_codex_event(Event {
        id: "sub-approve-patch".into(),
        msg: EventMsg::ApplyPatchApprovalRequest(ev),
    });

    // Render at the widget's desired height and snapshot.
    let height = chat.desired_height(80);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, height))
        .expect("create terminal");
    terminal
        .draw(|f| f.render_widget_ref(&chat, f.area()))
        .expect("draw patch approval modal");
    assert_snapshot!("approval_modal_patch", terminal.backend());
}

#[test]
fn interrupt_restores_queued_messages_into_composer() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual();

    // Simulate a running task to enable queuing of user inputs.
    chat.bottom_pane.set_task_running(true);

    // Queue two user messages while the task is running.
    chat.queued_user_messages
        .push_back(UserMessage::from("first queued".to_string()));
    chat.queued_user_messages
        .push_back(UserMessage::from("second queued".to_string()));
    chat.refresh_queued_user_messages();

    // Deliver a TurnAborted event with Interrupted reason (as if Esc was pressed).
    chat.handle_codex_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnAborted(codex_core::protocol::TurnAbortedEvent {
            reason: codex_core::protocol::TurnAbortReason::Interrupted,
        }),
    });

    // Composer should now contain the queued messages joined by newlines, in order.
    assert_eq!(
        chat.bottom_pane.composer_text(),
        "first queued\n\nsecond queued"
    );

    // Queue should be cleared and no new user input should have been auto-submitted.
    assert!(chat.queued_user_messages.is_empty());
    assert!(
        op_rx.try_recv().is_err(),
        "unexpected outbound op after interrupt"
    );

    // Drain rx to avoid unused warnings.
    let _ = drain_insert_history(&mut rx);
}

// Snapshot test: ChatWidget at very small heights (idle)
// Ensures overall layout behaves when terminal height is extremely constrained.
#[test]
fn ui_snapshots_small_heights_idle() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let (chat, _rx, _op_rx) = make_chatwidget_manual();
    for h in [1u16, 2, 3] {
        let name = format!("chat_small_idle_h{h}");
        let mut terminal = Terminal::new(TestBackend::new(40, h)).expect("create terminal");
        terminal
            .draw(|f| f.render_widget_ref(&chat, f.area()))
            .expect("draw chat idle");
        assert_snapshot!(name, terminal.backend());
    }
}

// Snapshot test: ChatWidget at very small heights (task running)
// Validates how status + composer are presented within tight space.
#[test]
fn ui_snapshots_small_heights_task_running() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual();
    // Activate status line
    chat.handle_codex_event(Event {
        id: "task-1".into(),
        msg: EventMsg::TaskStarted(TaskStartedEvent {
            model_context_window: None,
        }),
    });
    chat.handle_codex_event(Event {
        id: "task-1".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "**Thinking**".into(),
        }),
    });
    for h in [1u16, 2, 3] {
        let name = format!("chat_small_running_h{h}");
        let mut terminal = Terminal::new(TestBackend::new(40, h)).expect("create terminal");
        terminal
            .draw(|f| f.render_widget_ref(&chat, f.area()))
            .expect("draw chat running");
        assert_snapshot!(name, terminal.backend());
    }
}

// Snapshot test: status widget + approval modal active together
// The modal takes precedence visually; this captures the layout with a running
// task (status indicator active) while an approval request is shown.
#[test]
fn status_widget_and_approval_modal_snapshot() {
    use codex_core::protocol::ExecApprovalRequestEvent;

    let (mut chat, _rx, _op_rx) = make_chatwidget_manual();
    // Begin a running task so the status indicator would be active.
    chat.handle_codex_event(Event {
        id: "task-1".into(),
        msg: EventMsg::TaskStarted(TaskStartedEvent {
            model_context_window: None,
        }),
    });
    // Provide a deterministic header for the status line.
    chat.handle_codex_event(Event {
        id: "task-1".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "**Analyzing**".into(),
        }),
    });

    // Now show an approval modal (e.g. exec approval).
    let ev = ExecApprovalRequestEvent {
        call_id: "call-approve-exec".into(),
        command: vec!["echo".into(), "hello world".into()],
        cwd: std::path::PathBuf::from("/tmp"),
        reason: Some("Code wants to run a command".into()),
    };
    chat.handle_codex_event(Event {
        id: "sub-approve-exec".into(),
        msg: EventMsg::ExecApprovalRequest(ev),
    });

    // Render at the widget's desired height and snapshot.
    let height = chat.desired_height(80);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, height))
        .expect("create terminal");
    terminal
        .draw(|f| f.render_widget_ref(&chat, f.area()))
        .expect("draw status + approval modal");
    assert_snapshot!("status_widget_and_approval_modal", terminal.backend());
}

// Snapshot test: status widget active (StatusIndicatorView)
// Ensures the VT100 rendering of the status indicator is stable when active.
#[test]
fn status_widget_active_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual();
    // Activate the status indicator by simulating a task start.
    chat.handle_codex_event(Event {
        id: "task-1".into(),
        msg: EventMsg::TaskStarted(TaskStartedEvent {
            model_context_window: None,
        }),
    });
    // Provide a deterministic header via a bold reasoning chunk.
    chat.handle_codex_event(Event {
        id: "task-1".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "**Analyzing**".into(),
        }),
    });
    // Render and snapshot.
    let height = chat.desired_height(80);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, height))
        .expect("create terminal");
    terminal
        .draw(|f| f.render_widget_ref(&chat, f.area()))
        .expect("draw status widget");
    assert_snapshot!("status_widget_active", terminal.backend());
}

#[test]
fn apply_patch_events_emit_history_cells() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    // 1) Approval request -> proposed patch summary cell
    let mut changes = HashMap::new();
    changes.insert(
        PathBuf::from("foo.txt"),
        FileChange::Add {
            content: "hello\n".to_string(),
        },
    );
    let ev = ApplyPatchApprovalRequestEvent {
        call_id: "c1".into(),
        changes,
        reason: None,
        grant_root: None,
    };
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::ApplyPatchApprovalRequest(ev),
    });
    let cells = drain_insert_history(&rx);
    assert!(!cells.is_empty(), "expected pending patch cell to be sent");
    let blob = lines_to_single_string(cells.last().unwrap());
    assert!(
        blob.contains("proposed patch"),
        "missing proposed patch header: {blob:?}"
    );

    // 2) Begin apply -> applying patch cell
    let mut changes2 = HashMap::new();
    changes2.insert(
        PathBuf::from("foo.txt"),
        FileChange::Add {
            content: "hello\n".to_string(),
        },
    );
    let begin = PatchApplyBeginEvent {
        call_id: "c1".into(),
        auto_approved: true,
        changes: changes2,
    };
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::PatchApplyBegin(begin),
    });
    let cells = drain_insert_history(&rx);
    assert!(!cells.is_empty(), "expected applying patch cell to be sent");
    let blob = lines_to_single_string(cells.last().unwrap());
    assert!(
        blob.contains("Applying patch"),
        "missing applying patch header: {blob:?}"
    );

    // 3) End apply success -> success cell
    let end = PatchApplyEndEvent {
        call_id: "c1".into(),
        stdout: "ok\n".into(),
        stderr: String::new(),
        success: true,
    };
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::PatchApplyEnd(end),
    });
    let cells = drain_insert_history(&rx);
    assert!(!cells.is_empty(), "expected applied patch cell to be sent");
    let blob = lines_to_single_string(cells.last().unwrap());
    assert!(
        blob.contains("Applied patch"),
        "missing applied patch header: {blob:?}"
    );
}

#[test]
fn apply_patch_approval_sends_op_with_submission_id() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();
    // Simulate receiving an approval request with a distinct submission id and call id
    let mut changes = HashMap::new();
    changes.insert(
        PathBuf::from("file.rs"),
        FileChange::Add {
            content: "fn main(){}\n".into(),
        },
    );
    let ev = ApplyPatchApprovalRequestEvent {
        call_id: "call-999".into(),
        changes,
        reason: None,
        grant_root: None,
    };
    chat.handle_codex_event(Event {
        id: "sub-123".into(),
        msg: EventMsg::ApplyPatchApprovalRequest(ev),
    });

    // Approve via key press 'y'
    chat.handle_key_event(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));

    // Expect a CodexOp with PatchApproval carrying the submission id, not call id
    let mut found = false;
    while let Ok(app_ev) = rx.try_recv() {
        if let AppEvent::CodexOp(Op::PatchApproval { id, decision }) = app_ev {
            assert_eq!(id, "sub-123");
            assert!(matches!(
                decision,
                codex_core::protocol::ReviewDecision::Approved
            ));
            found = true;
            break;
        }
    }
    assert!(found, "expected PatchApproval op to be sent");
}

#[test]
fn apply_patch_full_flow_integration_like() {
    let (mut chat, rx, mut op_rx) = make_chatwidget_manual();

    // 1) Backend requests approval
    let mut changes = HashMap::new();
    changes.insert(
        PathBuf::from("pkg.rs"),
        FileChange::Add { content: "".into() },
    );
    chat.handle_codex_event(Event {
        id: "sub-xyz".into(),
        msg: EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
            call_id: "call-1".into(),
            changes,
            reason: None,
            grant_root: None,
        }),
    });

    // 2) User approves via 'y' and App receives a CodexOp
    chat.handle_key_event(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
    let mut maybe_op: Option<Op> = None;
    while let Ok(app_ev) = rx.try_recv() {
        if let AppEvent::CodexOp(op) = app_ev {
            maybe_op = Some(op);
            break;
        }
    }
    let op = maybe_op.expect("expected CodexOp after key press");

    // 3) App forwards to widget.submit_op, which pushes onto codex_op_tx
    chat.submit_op(op);
    let forwarded = op_rx
        .try_recv()
        .expect("expected op forwarded to codex channel");
    match forwarded {
        Op::PatchApproval { id, decision } => {
            assert_eq!(id, "sub-xyz");
            assert!(matches!(
                decision,
                codex_core::protocol::ReviewDecision::Approved
            ));
        }
        other => panic!("unexpected op forwarded: {other:?}"),
    }

    // 4) Simulate patch begin/end events from backend; ensure history cells are emitted
    let mut changes2 = HashMap::new();
    changes2.insert(
        PathBuf::from("pkg.rs"),
        FileChange::Add { content: "".into() },
    );
    chat.handle_codex_event(Event {
        id: "sub-xyz".into(),
        msg: EventMsg::PatchApplyBegin(PatchApplyBeginEvent {
            call_id: "call-1".into(),
            auto_approved: false,
            changes: changes2,
        }),
    });
    chat.handle_codex_event(Event {
        id: "sub-xyz".into(),
        msg: EventMsg::PatchApplyEnd(PatchApplyEndEvent {
            call_id: "call-1".into(),
            stdout: String::from("ok"),
            stderr: String::new(),
            success: true,
        }),
    });
}

#[test]
fn apply_patch_untrusted_shows_approval_modal() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual();
    // Ensure approval policy is untrusted (OnRequest)
    chat.config.approval_policy = codex_core::protocol::AskForApproval::OnRequest;

    // Simulate a patch approval request from backend
    let mut changes = HashMap::new();
    changes.insert(
        PathBuf::from("a.rs"),
        FileChange::Add { content: "".into() },
    );
    chat.handle_codex_event(Event {
        id: "sub-1".into(),
        msg: EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
            call_id: "call-1".into(),
            changes,
            reason: None,
            grant_root: None,
        }),
    });

    // Render and ensure the approval modal title is present
    let area = ratatui::layout::Rect::new(0, 0, 80, 12);
    let mut buf = ratatui::buffer::Buffer::empty(area);
    (&chat).render_ref(area, &mut buf);

    let mut contains_title = false;
    for y in 0..area.height {
        let mut row = String::new();
        for x in 0..area.width {
            row.push(buf[(x, y)].symbol().chars().next().unwrap_or(' '));
        }
        if row.contains("Apply changes?") {
            contains_title = true;
            break;
        }
    }
    assert!(
        contains_title,
        "expected approval modal to be visible with title 'Apply changes?'"
    );
}

#[test]
fn apply_patch_request_shows_diff_summary() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    // Ensure we are in OnRequest so an approval is surfaced
    chat.config.approval_policy = codex_core::protocol::AskForApproval::OnRequest;

    // Simulate backend asking to apply a patch adding two lines to README.md
    let mut changes = HashMap::new();
    changes.insert(
        PathBuf::from("README.md"),
        FileChange::Add {
            // Two lines (no trailing empty line counted)
            content: "line one\nline two\n".into(),
        },
    );
    chat.handle_codex_event(Event {
        id: "sub-apply".into(),
        msg: EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
            call_id: "call-apply".into(),
            changes,
            reason: None,
            grant_root: None,
        }),
    });

    // Drain history insertions and verify the diff summary is present
    let cells = drain_insert_history(&rx);
    assert!(
        !cells.is_empty(),
        "expected a history cell with the proposed patch summary"
    );
    let blob = lines_to_single_string(cells.last().unwrap());

    // Header should summarize totals
    assert!(
        blob.contains("proposed patch to 1 file (+2 -0)"),
        "missing or incorrect diff header: {blob:?}"
    );

    // Per-file summary line should include the file path and counts
    assert!(
        blob.contains("README.md"),
        "missing per-file diff summary: {blob:?}"
    );
}

#[test]
fn plan_update_renders_history_cell() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();
    let update = UpdatePlanArgs {
        name: Some("Feature rollout plan".to_string()),
        plan: vec![
            PlanItemArg {
                step: "Explore codebase".into(),
                status: StepStatus::Completed,
            },
            PlanItemArg {
                step: "Implement feature".into(),
                status: StepStatus::InProgress,
            },
            PlanItemArg {
                step: "Write tests".into(),
                status: StepStatus::Pending,
            },
        ],
    };
    chat.handle_codex_event(Event {
        id: "sub-1".into(),
        msg: EventMsg::PlanUpdate(update),
    });
    let cells = drain_insert_history(&rx);
    assert!(!cells.is_empty(), "expected plan update cell to be sent");
    let blob = lines_to_single_string(cells.last().unwrap());
    assert!(
        blob.contains("Feature rollout plan"),
        "missing plan header: {blob:?}"
    );
    assert!(blob.contains("Explore codebase"));
    assert!(blob.contains("Implement feature"));
    assert!(blob.contains("Write tests"));
}

#[test]
fn headers_emitted_on_stream_begin_for_answer_and_reasoning() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    // Answer: no header until a newline commit
    chat.handle_codex_event(Event {
        id: "sub-a".into(),
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "Hello".into(),
        }),
    });
    let mut saw_codex_pre = false;
    while let Ok(ev) = rx.try_recv() {
        if let AppEvent::InsertHistory(lines) = ev {
            let s = lines
                .iter()
                .flat_map(|l| l.spans.iter())
                .map(|sp| sp.content.clone())
                .collect::<Vec<_>>()
                .join("");
            if s.contains("codex") {
                saw_codex_pre = true;
                break;
            }
        }
    }
    assert!(
        !saw_codex_pre,
        "answer header should not be emitted before first newline commit"
    );

    // Newline arrives; no visible header should be emitted for Answer
    chat.handle_codex_event(Event {
        id: "sub-a".into(),
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "!\n".into(),
        }),
    });
    chat.on_commit_tick();
    let mut saw_codex_post = false;
    while let Ok(ev) = rx.try_recv() {
        if let AppEvent::InsertHistory(lines) = ev {
            let s = lines
                .iter()
                .flat_map(|l| l.spans.iter())
                .map(|sp| sp.content.clone())
                .collect::<Vec<_>>()
                .join("");
            if s.contains("codex") {
                saw_codex_post = true;
                break;
            }
        }
    }
    assert!(
        !saw_codex_post,
        "did not expect a visible 'codex' header to be emitted after first newline commit"
    );

    // Reasoning: header immediately
    let (mut chat2, rx2, _op_rx2) = make_chatwidget_manual();
    chat2.handle_codex_event(Event {
        id: "sub-b".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "Thinking".into(),
        }),
    });
    let mut saw_thinking = false;
    while let Ok(ev) = rx2.try_recv() {
        if let AppEvent::InsertHistory(lines) = ev {
            let s = lines
                .iter()
                .flat_map(|l| l.spans.iter())
                .map(|sp| sp.content.clone())
                .collect::<Vec<_>>()
                .join("");
            if s.contains("thinking") {
                saw_thinking = true;
                break;
            }
        }
    }
    assert!(
        saw_thinking,
        "expected 'thinking' header to be emitted at stream start"
    );
}

#[test]
fn multiple_agent_messages_in_single_turn_emit_multiple_headers() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    // Begin turn
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::TaskStarted(TaskStartedEvent {
            model_context_window: None,
        }),
    });

    // First finalized assistant message
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "First message".into(),
        }),
    });

    // Second finalized assistant message in the same turn
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "Second message".into(),
        }),
    });

    // End turn
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::TaskComplete(TaskCompleteEvent {
            last_agent_message: None,
        }),
    });

    let cells = drain_insert_history(&rx);
    let mut combined = String::new();
    for lines in &cells {
        for l in lines {
            for sp in &l.spans {
                let s = &sp.content;
                combined.push_str(s);
            }
            combined.push('\n');
        }
    }
    assert!(
        combined.contains("First message"),
        "missing first message: {combined}"
    );
    assert!(
        combined.contains("Second message"),
        "missing second message: {combined}"
    );
    let first_idx = combined.find("First message").unwrap();
    let second_idx = combined.find("Second message").unwrap();
    assert!(first_idx < second_idx, "messages out of order: {combined}");
}

#[test]
fn two_final_answers_append_not_overwrite_when_no_deltas() {
    // Directly exercise ChatWidget::insert_final_answer to validate we do not
    // overwrite a prior finalized assistant message when a new final arrives
    // without any streaming deltas (regression guard for overwrite bug).
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    // First finalized assistant message (no streaming cell exists yet)
    chat.insert_final_answer_with_id(None, Vec::new(), "First message".to_string());
    // Second finalized assistant message (also without streaming)
    chat.insert_final_answer_with_id(None, Vec::new(), "Second message".to_string());

    // Drain any history insert side-effects (not strictly required here)
    let _ = drain_insert_history(&rx);

    // Verify via exported ResponseItems so we don't reach into private fields
    let items = chat.export_response_items();
    let assistants: Vec<String> = items
        .into_iter()
        .filter_map(|it| match it {
            codex_protocol::models::ResponseItem::Message { role, content, .. } if role == "assistant" => {
                let text = content.into_iter().filter_map(|c| match c {
                    codex_protocol::models::ContentItem::OutputText { text } => Some(text),
                    codex_protocol::models::ContentItem::InputText { text } => Some(text),
                    _ => None,
                }).collect::<Vec<_>>().join("\n");
                Some(text)
            }
            _ => None,
        })
        .collect();

    assert_eq!(assistants.len(), 2, "expected two assistant messages, got {}", assistants.len());
    assert!(assistants.iter().any(|s| s.contains("First message")), "missing first message");
    assert!(assistants.iter().any(|s| s.contains("Second message")), "missing second message");
}

#[test]
fn second_final_that_is_superset_replaces_first() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    let base = "• Behavior\n  ◦ One\n\nNotes\n  ◦ Alpha\n".to_string();
    let extended = format!("{}  ◦ Beta\n", base);

    chat.insert_final_answer_with_id(None, Vec::new(), base);
    chat.insert_final_answer_with_id(None, Vec::new(), extended.clone());

    // Drain history events
    let _ = drain_insert_history(&rx);

    // Expect exactly one assistant message containing the extended text
    let items = chat.export_response_items();
    let assistants: Vec<String> = items
        .into_iter()
        .filter_map(|it| match it {
            codex_protocol::models::ResponseItem::Message { role, content, .. } if role == "assistant" => {
                let text = content.into_iter().filter_map(|c| match c {
                    codex_protocol::models::ContentItem::OutputText { text } => Some(text),
                    codex_protocol::models::ContentItem::InputText { text } => Some(text),
                    _ => None,
                }).collect::<Vec<_>>().join("\n");
                Some(text)
            }
            _ => None,
        })
        .collect();

    assert_eq!(assistants.len(), 1, "expected single assistant after superset replace");
    assert!(assistants[0].contains("Beta"), "expected extended content present");
}

#[test]
fn identical_content_with_unicode_bullets_dedupes() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    let a = "• Behavior\n  ◦ One\n\nNotes\n  ◦ Alpha".to_string();
    let b = "- Behavior\n  - One\n\nNotes\n  - Alpha".to_string();

    // Two finals that only differ in bullet glyphs should dedupe into one cell
    chat.insert_final_answer_with_id(None, Vec::new(), a);
    chat.insert_final_answer_with_id(None, Vec::new(), b);

    // Drain history events
    let _ = drain_insert_history(&rx);

    let items = chat.export_response_items();
    let assistants: Vec<String> = items
        .into_iter()
        .filter_map(|it| match it {
            codex_protocol::models::ResponseItem::Message { role, content, .. } if role == "assistant" => {
                let text = content.into_iter().filter_map(|c| match c {
                    codex_protocol::models::ContentItem::OutputText { text } => Some(text),
                    codex_protocol::models::ContentItem::InputText { text } => Some(text),
                    _ => None,
                }).collect::<Vec<_>>().join("\n");
                Some(text)
            }
            _ => None,
        })
        .collect();

    assert_eq!(assistants.len(), 1, "expected deduped single assistant cell");
}

#[test]
fn final_reasoning_then_message_without_deltas_are_rendered() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    // No deltas; only final reasoning followed by final message.
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentReasoning(AgentReasoningEvent {
            text: "I will first analyze the request.".into(),
        }),
    });
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "Here is the result.".into(),
        }),
    });

    // Drain history and snapshot the combined visible content.
    let cells = drain_insert_history(&rx);
    let combined = cells
        .iter()
        .map(|lines| lines_to_single_string(lines))
        .collect::<String>();
    assert_snapshot!(combined);
}

#[test]
fn deltas_then_same_final_message_are_rendered_snapshot() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    // Stream some reasoning deltas first.
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "I will ".into(),
        }),
    });
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "first analyze the ".into(),
        }),
    });
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentReasoningDelta(AgentReasoningDeltaEvent {
            delta: "request.".into(),
        }),
    });
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentReasoning(AgentReasoningEvent {
            text: "request.".into(),
        }),
    });

    // Then stream answer deltas, followed by the exact same final message.
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "Here is the ".into(),
        }),
    });
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "result.".into(),
        }),
    });

    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "Here is the result.".into(),
        }),
    });

    // Snapshot the combined visible content to ensure we render as expected
    // when deltas are followed by the identical final message.
    let cells = drain_insert_history(&rx);
    let combined = cells
        .iter()
        .map(|lines| lines_to_single_string(lines))
        .collect::<String>();
    assert_snapshot!(combined);
}

#[test]
fn late_final_does_not_duplicate_when_stream_finalized_early() {
    use codex_core::protocol::*;

    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    // Stream some answer deltas for id "s1"
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "What I Can Do\n".into(),
        }),
    });
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "- Explore/Modify Code\n".into(),
        }),
    });

    // Simulate a side event that forces finalization (e.g., tool start)
    chat.finalize_active_stream();

    // Now a late final AgentMessage arrives with the full text
    let final_text = "What I Can Do\n- Explore/Modify Code\n".to_string();
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: final_text.clone(),
        }),
    });

    // Drain any history insert side-effects (not strictly required here)
    let _ = drain_insert_history(&rx);

    // Export and assert that only a single assistant message exists
    let items = chat.export_response_items();
    let assistants: Vec<String> = items
        .into_iter()
        .filter_map(|it| match it {
            codex_protocol::models::ResponseItem::Message { role, content, .. } if role == "assistant" => {
                let text = content.into_iter().filter_map(|c| match c {
                    codex_protocol::models::ContentItem::OutputText { text } => Some(text),
                    codex_protocol::models::ContentItem::InputText { text } => Some(text),
                    _ => None,
                }).collect::<Vec<_>>().join("\n");
                Some(text)
            }
            _ => None,
        })
        .collect();

    assert_eq!(assistants.len(), 1, "late final should replace, not duplicate");
    assert!(assistants[0].contains("Explore/Modify Code"));
}

#[test]
fn streaming_answer_then_finalize_does_not_truncate() {
    let (mut chat, rx, _op_rx) = make_chatwidget_manual();

    // Stream an assistant message in chunks, without sending a final AgentMessage.
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "Files changed\n".into(),
        }),
    });
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "- codex-rs/tui/src/markdown.rs: Guard against list markers...\n".into(),
        }),
    });
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "\nWhat to expect\n".into(),
        }),
    });
    chat.handle_codex_event(Event {
        id: "s1".into(),
        msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent {
            delta: "- Third-level bullets render correctly now.\n".into(),
        }),
    });

    // Simulate lifecycle location that finalizes active stream (e.g., new event)
    chat.finalize_active_stream();

    // Drain and combine visible content
    let cells = drain_insert_history(&rx);
    let combined = cells
        .iter()
        .map(|lines| lines_to_single_string(lines))
        .collect::<String>();

    assert!(combined.contains("Files changed"), "missing header: {combined}");
    assert!(combined.contains("What to expect"), "missing section header: {combined}");
    assert!(combined.contains("Third-level bullets render correctly now"),
        "missing tail content after finalize: {combined}");
}

// E2E vt100 snapshot for complex markdown with indented and nested fenced code blocks
#[test]
fn chatwidget_markdown_code_blocks_vt100_snapshot() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual();

    // Simulate a final agent message via streaming deltas instead of a single message

    chat.handle_codex_event(Event {
        id: "t1".into(),
        msg: EventMsg::TaskStarted(TaskStartedEvent {
            model_context_window: None,
        }),
    });
    // Build a vt100 visual from the history insertions only (no UI overlay)
    let width: u16 = 80;
    let height: u16 = 50;
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut term = crate::custom_terminal::Terminal::with_options(backend).expect("terminal");
    // Place viewport at the last line so that history lines insert above it
    term.set_viewport_area(Rect::new(0, height - 1, width, 1));

    let mut ansi: Vec<u8> = Vec::new();

    // Simulate streaming via AgentMessageDelta in 2-character chunks (no final AgentMessage).
    let source: &str = r#"

    -- Indented code block (4 spaces)
    SELECT *
    FROM "users"
    WHERE "email" LIKE '%@example.com';

````markdown
```sh
printf 'fenced within fenced\n'
```
````

```jsonc
{
  // comment allowed in jsonc
  "path": "C:\\Program Files\\App",
  "regex": "^foo.*(bar)?$"
}
```
"#;

    let mut it = source.chars();
    loop {
        let mut delta = String::new();
        match it.next() {
            Some(c) => delta.push(c),
            None => break,
        }
        if let Some(c2) = it.next() {
            delta.push(c2);
        }

        chat.handle_codex_event(Event {
            id: "t1".into(),
            msg: EventMsg::AgentMessageDelta(AgentMessageDeltaEvent { delta }),
        });
        // Drive commit ticks and drain emitted history lines into the vt100 buffer.
        loop {
            chat.on_commit_tick();
            let mut inserted_any = false;
            while let Ok(app_ev) = rx.try_recv() {
                if let AppEvent::InsertHistoryCell(cell) = app_ev {
                    let lines = cell.display_lines(width);
                    crate::insert_history::insert_history_lines_to_writer(
                        &mut term, &mut ansi, lines,
                    );
                    inserted_any = true;
                }
            }
            if !inserted_any {
                break;
            }
        }
    }

    // Finalize the stream without sending a final AgentMessage, to flush any tail.
    chat.handle_codex_event(Event {
        id: "t1".into(),
        msg: EventMsg::TaskComplete(TaskCompleteEvent {
            last_agent_message: None,
        }),
    });
    for lines in drain_insert_history(&mut rx) {
        crate::insert_history::insert_history_lines_to_writer(&mut term, &mut ansi, lines);
    }

    let mut parser = vt100::Parser::new(height, width, 0);
    parser.process(&ansi);

    let mut vt_lines: Vec<String> = (0..height)
        .map(|row| {
            let mut s = String::with_capacity(width as usize);
            for col in 0..width {
                if let Some(cell) = parser.screen().cell(row, col) {
                    if let Some(ch) = cell.contents().chars().next() {
                        s.push(ch);
                    } else {
                        s.push(' ');
                    }
                } else {
                    s.push(' ');
                }
            }
            s.trim_end().to_string()
        })
        .collect();

    // Compact trailing blank rows for a stable snapshot
    while matches!(vt_lines.last(), Some(l) if l.trim().is_empty()) {
        vt_lines.pop();
    }
    let visual = vt_lines.join("\n");
    assert_snapshot!(visual);
}
