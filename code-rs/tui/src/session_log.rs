use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::OnceLock;

use code_core::config::Config;
use code_core::protocol::Op;
use serde::Serialize;
use serde_json::json;

use crate::app_event::AppEvent;
use crate::history::state::HistorySnapshot;

static LOGGER: LazyLock<SessionLogger> = LazyLock::new(SessionLogger::new);

struct SessionLogger {
    file: OnceLock<Mutex<File>>,
    path: OnceLock<PathBuf>,
}

impl SessionLogger {
    fn new() -> Self {
        Self {
            file: OnceLock::new(),
            path: OnceLock::new(),
        }
    }

    fn open(&self, path: PathBuf) -> std::io::Result<()> {
        let mut opts = OpenOptions::new();
        opts.create(true).truncate(true).write(true);

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }

        let file = opts.open(&path)?;
        self.file.get_or_init(|| Mutex::new(file));
        let _ = self.path.set(path);
        Ok(())
    }

    fn write_json_line(&self, value: serde_json::Value) {
        let Some(mutex) = self.file.get() else {
            return;
        };
        let mut guard = match mutex.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        match serde_json::to_string(&value) {
            Ok(serialized) => {
                if let Err(e) = guard.write_all(serialized.as_bytes()) {
                    tracing::warn!("session log write error: {}", e);
                    return;
                }
                if let Err(e) = guard.write_all(b"\n") {
                    tracing::warn!("session log write error: {}", e);
                    return;
                }
                if let Err(e) = guard.flush() {
                    tracing::warn!("session log flush error: {}", e);
                }
            }
            Err(e) => tracing::warn!("session log serialize error: {}", e),
        }
    }

    fn is_enabled(&self) -> bool {
        self.file.get().is_some()
    }

    fn path(&self) -> Option<&PathBuf> {
        self.path.get()
    }
}

fn now_ts() -> String {
    // RFC3339 for readability; consumers can parse as needed.
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(crate) fn maybe_init(config: &Config) {
    let enabled = std::env::var("CODEX_TUI_RECORD_SESSION")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);
    if !enabled {
        return;
    }

    let path = if let Ok(path) = std::env::var("CODEX_TUI_SESSION_LOG_PATH") {
        PathBuf::from(path)
    } else {
        let mut p = match code_core::config::log_dir(config) {
            Ok(dir) => dir,
            Err(_) => std::env::temp_dir(),
        };
        let filename = format!(
            "session-{}.jsonl",
            chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
        );
        p.push(filename);
        p
    };

    if let Err(e) = LOGGER.open(path.clone()) {
        tracing::error!("failed to open session log {:?}: {}", path, e);
        return;
    }

    // Write a header record so we can attach context.
    let header = json!({
        "ts": now_ts(),
        "dir": "meta",
        "kind": "session_start",
        "pid": std::process::id(),
        "cwd": config.cwd,
        "model": config.model,
        "model_provider_id": config.model_provider_id,
        "model_provider_name": config.model_provider.name,
    });
    LOGGER.write_json_line(header);
}

pub(crate) fn log_inbound_app_event(event: &AppEvent) {
    // Log only if enabled
    if !LOGGER.is_enabled() {
        return;
    }

    match event {
        AppEvent::CodexEvent(ev) => {
            write_record("to_tui", "code_event", ev);
        }
        AppEvent::KeyEvent(k) => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "key_event",
                "event": format!("{:?}", k),
            });
            LOGGER.write_json_line(value);
        }
        AppEvent::Paste(s) => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "paste",
                "text": s,
            });
            LOGGER.write_json_line(value);
        }
        AppEvent::DispatchCommand(cmd, _text) => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "slash_command",
                "command": format!("{:?}", cmd),
            });
            LOGGER.write_json_line(value);
        }
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
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "auto_coordinator_decision",
                "seq": seq,
                "status": format!("{status:?}"),
                "status_title": status_title,
                "status_sent_to_user": status_sent_to_user,
                "goal": goal,
                "cli": cli.as_ref().map(|action| json!({
                    "prompt": action.prompt,
                    "context": action.context,
                    "suppress_ui_context": action.suppress_ui_context,
                    "model_override": action.model_override,
                    "reasoning_effort_override": action
                        .reasoning_effort_override
                        .map(|effort| effort.to_string()),
                })),
                "agents_timing": agents_timing.map(|timing| format!("{timing:?}")),
                "agents": agents
                    .iter()
                    .map(|agent| {
                        json!({
                            "prompt": agent.prompt,
                            "context": agent.context,
                            "write": agent.write,
                            "write_requested": agent.write_requested,
                            "models": agent.models,
                        })
                    })
                    .collect::<Vec<_>>(),
                "transcript": transcript,
            });
            LOGGER.write_json_line(value);
        }
        AppEvent::AutoCoordinatorUserReply {
            user_response,
            cli_command,
        } => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "auto_coordinator_user_reply",
                "user_response": user_response,
                "cli_command": cli_command,
            });
            LOGGER.write_json_line(value);
        }
        AppEvent::AutoCoordinatorThinking {
            delta,
            summary_index,
        } => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "auto_coordinator_thinking",
                "delta": delta,
                "summary_index": summary_index,
            });
            LOGGER.write_json_line(value);
        }
        AppEvent::AutoCoordinatorAction { message } => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "auto_coordinator_action",
                "message": message,
            });
            LOGGER.write_json_line(value);
        }
        AppEvent::AutoCoordinatorTokenMetrics {
            total_usage,
            last_turn_usage,
            turn_count,
            duplicate_items,
            replay_updates,
        } => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "auto_coordinator_token_metrics",
                "total_usage": total_usage,
                "last_turn_usage": last_turn_usage,
                "turn_count": turn_count,
                "duplicate_items": duplicate_items,
                "replay_updates": replay_updates,
            });
            LOGGER.write_json_line(value);
        }
        AppEvent::AutoCoordinatorCompactedHistory {
            conversation,
            show_notice,
        } => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "auto_coordinator_compacted_history",
                "conversation": conversation,
                "show_notice": show_notice,
            });
            LOGGER.write_json_line(value);
        }
        AppEvent::AutoCoordinatorStopAck => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "auto_coordinator_stop_ack",
            });
            LOGGER.write_json_line(value);
        }
        AppEvent::AutoCoordinatorCountdown {
            countdown_id,
            seconds_left,
        } => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "auto_coordinator_countdown",
                "countdown_id": countdown_id,
                "seconds_left": seconds_left,
            });
            LOGGER.write_json_line(value);
        }
        AppEvent::AutoCoordinatorRestart { token, attempt } => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "auto_coordinator_restart",
                "token": token,
                "attempt": attempt,
            });
            LOGGER.write_json_line(value);
        }
        // Internal UI events; still log for fidelity, but avoid heavy payloads.
        AppEvent::InsertHistory(lines) => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "insert_history",
                "lines": lines.len(),
            });
            LOGGER.write_json_line(value);
        }
        AppEvent::StartFileSearch(query) => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "file_search_start",
                "query": query,
            });
            LOGGER.write_json_line(value);
        }
        AppEvent::FileSearchResult { query, matches } => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "file_search_result",
                "query": query,
                "matches": matches.len(),
            });
            LOGGER.write_json_line(value);
        }
        // Noise or control flow – record variant only
        other => {
            let value = json!({
                "ts": now_ts(),
                "dir": "to_tui",
                "kind": "app_event",
                "variant": format!("{other:?}").split('(').next().unwrap_or("app_event"),
            });
            LOGGER.write_json_line(value);
        }
    }
}

#[allow(dead_code)]
pub(crate) fn log_outbound_op(op: &Op) {
    if !LOGGER.is_enabled() {
        return;
    }
    write_record("from_tui", "op", op);
}

pub(crate) fn log_session_end() {
    if !LOGGER.is_enabled() {
        return;
    }
    let value = json!({
        "ts": now_ts(),
        "dir": "meta",
        "kind": "session_end",
    });
    LOGGER.write_json_line(value);
}

pub(crate) fn log_panic(
    panic_message: &str,
    thread_name: &str,
    thread_id: &str,
    location: Option<(&str, u32, u32)>,
    backtrace: &str,
) {
    if !LOGGER.is_enabled() {
        return;
    }
    let (file, line, column) = match location {
        Some((file, line, column)) => (Some(file), Some(line), Some(column)),
        None => (None, None, None),
    };
    let value = json!({
        "ts": now_ts(),
        "dir": "meta",
        "kind": "panic",
        "thread_name": thread_name,
        "thread_id": thread_id,
        "panic": panic_message,
        "file": file,
        "line": line,
        "column": column,
        "backtrace": backtrace,
    });
    LOGGER.write_json_line(value);
}

pub(crate) fn log_path() -> Option<PathBuf> {
    LOGGER.path().cloned()
}

fn write_record<T>(dir: &str, kind: &str, obj: &T)
where
    T: Serialize,
{
    let value = json!({
        "ts": now_ts(),
        "dir": dir,
        "kind": kind,
        "payload": obj,
    });
    LOGGER.write_json_line(value);
}

fn make_history_snapshot_value(
    commit_id: &str,
    summary: Option<&str>,
    history: &HistorySnapshot,
) -> serde_json::Value {
    let record_count = history.records.len();
    let order_len = history.order.len();
    let order_debug_len = history.order_debug.len();
    json!({
        "ts": now_ts(),
        "dir": "meta",
        "kind": "history_snapshot",
        "commit": commit_id,
        "summary": summary,
        "record_count": record_count,
        "order_len": order_len,
        "order_debug_len": order_debug_len,
        "history": history,
    })
}

pub(crate) fn log_history_snapshot(
    commit_id: &str,
    summary: Option<&str>,
    history: &HistorySnapshot,
) {
    if !LOGGER.is_enabled() {
        return;
    }
    LOGGER.write_json_line(make_history_snapshot_value(commit_id, summary, history));
}
