//! Persist Codex session rollouts (.jsonl) so sessions can be replayed or inspected later.

use std::fs::File;
use std::fs::{self};
use std::io::Error as IoError;
use std::path::Path;
use std::path::PathBuf;

use codex_protocol::mcp_protocol::ConversationId;
use serde_json::Value;
use serde::{Serialize, Deserialize};
use time::OffsetDateTime;
use time::format_description::FormatItem;
use time::macros::format_description;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::{self};
use tokio::sync::oneshot;
use tracing::info;
use tracing::warn;

use super::SESSIONS_SUBDIR;
use super::list::ConversationsPage;
use super::list::Cursor;
use super::list::get_conversations;
use super::policy::{should_persist_response_item, should_persist_rollout_item};
use crate::config::Config;
use crate::default_client::DEFAULT_ORIGINATOR;
use crate::git_info::collect_git_info;
use codex_protocol::protocol::InitialHistory;
use codex_protocol::protocol::ResumedHistory;
use codex_protocol::protocol::RolloutItem;
use codex_protocol::protocol::RolloutLine;
use codex_protocol::protocol::SessionMeta;
use codex_protocol::protocol::SessionMetaLine;
use codex_protocol::models::ResponseItem;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct SessionStateSnapshot {}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct SavedSession {
    pub session: SessionMeta,
    #[serde(default)]
    pub items: Vec<RolloutItem>,
    #[serde(default)]
    pub events: Vec<crate::protocol::RecordedEvent>,
    #[serde(default)]
    pub state: SessionStateSnapshot,
    pub session_id: uuid::Uuid,
}
/// Records all [`ResponseItem`]s for a session and flushes them to disk after
/// every update.
///
/// Rollouts are recorded as JSONL and can be inspected with tools such as:
///
/// ```ignore
/// $ jq -C . ~/.code/sessions/rollout-2025-05-07T17-24-21-5973b6c0-94b8-487b-a530-2aeb6098ae0e.jsonl
/// $ fx ~/.code/sessions/rollout-2025-05-07T17-24-21-5973b6c0-94b8-487b-a530-2aeb6098ae0e.jsonl
/// ```
#[derive(Clone)]
pub struct RolloutRecorder {
    tx: Sender<RolloutCmd>,
    #[allow(dead_code)]
    pub(crate) rollout_path: PathBuf,
}

#[derive(Clone)]
pub enum RolloutRecorderParams {
    Create {
        conversation_id: ConversationId,
        instructions: Option<String>,
    },
    Resume {
        path: PathBuf,
    },
}

enum RolloutCmd {
    AddItems(Vec<RolloutItem>),
    Shutdown { ack: oneshot::Sender<()> },
}

impl RolloutRecorderParams {
    pub fn new(conversation_id: ConversationId, instructions: Option<String>) -> Self {
        Self::Create {
            conversation_id,
            instructions,
        }
    }

    // Note: older APIs used a different resume entrypoint; prefer RolloutRecorder::resume.
}

impl RolloutRecorder {
    /// List conversations (rollout files) under the provided Codex home directory.
    #[allow(dead_code)]
    pub async fn list_conversations(
        codex_home: &Path,
        page_size: usize,
        cursor: Option<&Cursor>,
    ) -> std::io::Result<ConversationsPage> {
        get_conversations(codex_home, page_size, cursor).await
    }

    /// Attempt to create a new [`RolloutRecorder`]. If the sessions directory
    /// cannot be created or the rollout file cannot be opened we return the
    /// error so the caller can decide whether to disable persistence.
    pub async fn new(config: &Config, params: RolloutRecorderParams) -> std::io::Result<Self> {
        let (file, rollout_path, meta) = match params {
            RolloutRecorderParams::Create {
                conversation_id,
                instructions,
            } => {
                let LogFileInfo {
                    file,
                    path,
                    conversation_id: session_id,
                    timestamp,
                } = create_log_file(config, conversation_id)?;

                let timestamp_format: &[FormatItem] = format_description!(
                    "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
                );
                let timestamp = timestamp
                    .to_offset(time::UtcOffset::UTC)
                    .format(timestamp_format)
                    .map_err(|e| IoError::other(format!("failed to format timestamp: {e}")))?;

                (
                    tokio::fs::File::from_std(file),
                    path,
                    Some(SessionMeta {
                        id: session_id,
                        timestamp,
                        cwd: config.cwd.clone(),
                        originator: DEFAULT_ORIGINATOR.to_string(),
                        cli_version: env!("CARGO_PKG_VERSION").to_string(),
                        instructions,
                    }),
                )
            }
            RolloutRecorderParams::Resume { path } => (
                tokio::fs::OpenOptions::new()
                    .append(true)
                    .open(&path)
                    .await?,
                path,
                None,
            ),
        };

        // Clone the cwd for the spawned task to collect git info asynchronously
        let cwd = config.cwd.clone();

        // A reasonably-sized bounded channel. If the buffer fills up the send
        // future will yield, which is fine – we only need to ensure we do not
        // perform *blocking* I/O on the caller's thread.
        let (tx, rx) = mpsc::channel::<RolloutCmd>(256);

        // Spawn a Tokio task that owns the file handle and performs async
        // writes. Using `tokio::fs::File` keeps everything on the async I/O
        // driver instead of blocking the runtime.
        tokio::task::spawn(rollout_writer(file, rx, meta, cwd));

        Ok(Self { tx, rollout_path })
    }

    pub(crate) async fn record_response_items(
        &self,
        items: &[ResponseItem],
    ) -> std::io::Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let mut rollout_items: Vec<RolloutItem> = Vec::new();
        for item in items {
            if should_persist_response_item(item) {
                rollout_items.push(RolloutItem::ResponseItem(item.clone()));
            }
        }
        if rollout_items.is_empty() {
            return Ok(());
        }
        self.record_items(&rollout_items).await
    }

    pub(crate) async fn record_items(&self, items: &[RolloutItem]) -> std::io::Result<()> {
        let mut filtered: Vec<RolloutItem> = Vec::new();
        for item in items {
            if should_persist_rollout_item(item) {
                filtered.push(item.clone());
            }
        }
        if filtered.is_empty() {
            return Ok(());
        }
        self.tx
            .send(RolloutCmd::AddItems(filtered))
            .await
            .map_err(|e| IoError::other(format!("failed to queue rollout items: {e}")))
    }

    pub(crate) async fn record_events(
        &self,
        events: &[codex_protocol::protocol::RecordedEvent],
    ) -> std::io::Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        let filtered = events
            .iter()
            .cloned()
            .map(RolloutItem::Event)
            .collect::<Vec<_>>();
        self.tx
            .send(RolloutCmd::AddItems(filtered))
            .await
            .map_err(|e| IoError::other(format!("failed to queue rollout events: {e}")))
    }

    /// No-op compatibility shim for older APIs expecting a state snapshot.
    pub async fn record_state(&self, _snapshot: SessionStateSnapshot) -> std::io::Result<()> {
        Ok(())
    }

    /// Compatibility wrapper for older resume API used by codex.rs
    pub async fn resume(config: &Config, path: &Path) -> std::io::Result<(Self, SavedSession)> {
        let recorder = Self::new(config, RolloutRecorderParams::Resume { path: path.to_path_buf() }).await?;
        let history = Self::get_rollout_history(path).await?;
        let (session_id, items, events) = match history {
            InitialHistory::Resumed(resumed) => {
                let events = resumed
                    .history
                    .iter()
                    .filter_map(|entry| match entry {
                        RolloutItem::Event(ev) => crate::protocol::recorded_event_from_protocol(ev.clone()),
                        _ => None,
                    })
                    .collect();
                (uuid::Uuid::from(resumed.conversation_id), resumed.history, events)
            }
            _ => (uuid::Uuid::new_v4(), Vec::new(), Vec::new()),
        };
        let saved = SavedSession {
            session: SessionMeta::default(),
            items,
            events,
            state: SessionStateSnapshot::default(),
            session_id,
        };
        Ok((recorder, saved))
    }

    pub(crate) async fn get_rollout_history(path: &Path) -> std::io::Result<InitialHistory> {
        info!("Resuming rollout from {path:?}");
        let text = tokio::fs::read_to_string(path).await?;
        if text.trim().is_empty() {
            return Err(IoError::other("empty session file"));
        }

        let mut items: Vec<RolloutItem> = Vec::new();
        let mut conversation_id: Option<ConversationId> = None;
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let v: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(e) => {
                    warn!("failed to parse line as JSON: {line:?}, error: {e}");
                    continue;
                }
            };

            // Parse the rollout line structure
            match serde_json::from_value::<RolloutLine>(v.clone()) {
                Ok(rollout_line) => match rollout_line.item {
                    RolloutItem::SessionMeta(session_meta_line) => {
                        tracing::error!(
                            "Parsed conversation ID from rollout file: {:?}",
                            session_meta_line.meta.id
                        );
                        conversation_id = Some(session_meta_line.meta.id);
                        items.push(RolloutItem::SessionMeta(session_meta_line));
                    }
                    RolloutItem::ResponseItem(item) => {
                        items.push(RolloutItem::ResponseItem(item));
                    }
                    RolloutItem::Event(ev) => {
                        items.push(RolloutItem::Event(ev));
                    }
                    RolloutItem::Compacted(compacted) => {
                        items.push(RolloutItem::Compacted(compacted));
                    }
                    // Ignore variants not used by this fork when resuming.
                    RolloutItem::TurnContext(_) => {
                        // Skip
                    }
                },
                Err(e) => {
                    warn!("failed to parse rollout line: {v:?}, error: {e}");
                }
            }
        }

        info!(
            "Resumed rollout with {} items, conversation ID: {:?}",
            items.len(),
            conversation_id
        );
        let conversation_id = conversation_id
            .ok_or_else(|| IoError::other("failed to parse conversation ID from rollout file"))?;

        if items.is_empty() {
            return Ok(InitialHistory::New);
        }

        info!("Resumed rollout successfully from {path:?}");
        Ok(InitialHistory::Resumed(ResumedHistory {
            conversation_id,
            history: items,
            rollout_path: path.to_path_buf(),
        }))
    }

    pub async fn shutdown(&self) -> std::io::Result<()> {
        let (tx_done, rx_done) = oneshot::channel();
        match self.tx.send(RolloutCmd::Shutdown { ack: tx_done }).await {
            Ok(_) => rx_done
                .await
                .map_err(|e| IoError::other(format!("failed waiting for rollout shutdown: {e}"))),
            Err(e) => {
                warn!("failed to send rollout shutdown command: {e}");
                Err(IoError::other(format!(
                    "failed to send rollout shutdown command: {e}"
                )))
            }
        }
    }
}

struct LogFileInfo {
    /// Opened file handle to the rollout file.
    file: File,

    /// Full path to the rollout file.
    path: PathBuf,

    /// Session ID (also embedded in filename).
    conversation_id: ConversationId,

    /// Timestamp for the start of the session.
    timestamp: OffsetDateTime,
}

fn create_log_file(
    config: &Config,
    conversation_id: ConversationId,
) -> std::io::Result<LogFileInfo> {
    // Resolve ~/.code/sessions/YYYY/MM/DD and create it if missing (Code still
    // reads legacy ~/.codex/sessions/ paths).
    let timestamp = OffsetDateTime::now_local()
        .map_err(|e| IoError::other(format!("failed to get local time: {e}")))?;
    let mut dir = config.codex_home.clone();
    dir.push(SESSIONS_SUBDIR);
    dir.push(timestamp.year().to_string());
    dir.push(format!("{:02}", u8::from(timestamp.month())));
    dir.push(format!("{:02}", timestamp.day()));
    fs::create_dir_all(&dir)?;

    // Custom format for YYYY-MM-DDThh-mm-ss. Use `-` instead of `:` for
    // compatibility with filesystems that do not allow colons in filenames.
    let format: &[FormatItem] =
        format_description!("[year]-[month]-[day]T[hour]-[minute]-[second]");
    let date_str = timestamp
        .format(format)
        .map_err(|e| IoError::other(format!("failed to format timestamp: {e}")))?;

    let filename = format!("rollout-{date_str}-{conversation_id}.jsonl");

    let path = dir.join(filename);
    let file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)?;

    Ok(LogFileInfo {
        file,
        path,
        conversation_id,
        timestamp,
    })
}

async fn rollout_writer(
    file: tokio::fs::File,
    mut rx: mpsc::Receiver<RolloutCmd>,
    mut meta: Option<SessionMeta>,
    cwd: std::path::PathBuf,
) -> std::io::Result<()> {
    let mut writer = JsonlWriter { file };

    // If we have a meta, collect git info asynchronously and write meta first
    if let Some(session_meta) = meta.take() {
        let git_info = collect_git_info(&cwd).await;
        let session_meta_line = SessionMetaLine {
            meta: session_meta,
            git: git_info,
        };

        // Write the SessionMeta as the first item in the file, wrapped in a rollout line
        writer
            .write_rollout_item(RolloutItem::SessionMeta(session_meta_line))
            .await?;
    }

    // Process rollout commands
    while let Some(cmd) = rx.recv().await {
        match cmd {
            RolloutCmd::AddItems(items) => {
                for item in items {
                    if should_persist_rollout_item(&item) {
                        writer.write_rollout_item(item).await?;
                    }
                }
            }
            RolloutCmd::Shutdown { ack } => {
                let _ = ack.send(());
            }
        }
    }

    Ok(())
}

struct JsonlWriter {
    file: tokio::fs::File,
}

impl JsonlWriter {
    async fn write_rollout_item(&mut self, rollout_item: RolloutItem) -> std::io::Result<()> {
        let timestamp_format: &[FormatItem] = format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        );
        let timestamp = OffsetDateTime::now_utc()
            .format(timestamp_format)
            .map_err(|e| IoError::other(format!("failed to format timestamp: {e}")))?;

        let line = RolloutLine {
            timestamp,
            item: rollout_item,
        };
        self.write_line(&line).await
    }
    async fn write_line(&mut self, item: &impl serde::Serialize) -> std::io::Result<()> {
        let mut json = serde_json::to_string(item)?;
        json.push('\n');
        self.file.write_all(json.as_bytes()).await?;
        self.file.flush().await?;
        Ok(())
    }
}
