//! Persist Codex session rollouts (.jsonl) so sessions can be replayed or inspected later.

use std::fs::File;
use std::fs::{self};
use std::io::Error as IoError;
use std::path::Path;
use std::path::PathBuf;

use code_protocol::ConversationId;
use code_protocol::models::{ContentItem, ResponseItem};
use code_protocol::protocol::EventMsg as ProtoEventMsg;
use code_protocol::protocol::SessionSource;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
use super::list::get_conversations;
use super::list::ConversationsPage;
use super::list::Cursor;
use super::policy::{should_persist_response_item, should_persist_rollout_item};
use crate::config::Config;
use crate::default_client::DEFAULT_ORIGINATOR;
use crate::git_info::collect_git_info;
use crate::history::HistorySnapshot;
use crate::protocol::event_msg_from_protocol;
use code_protocol::protocol::InitialHistory;
use code_protocol::protocol::ResumedHistory;
use code_protocol::protocol::RolloutItem;
use code_protocol::protocol::RolloutLine;
use code_protocol::protocol::SessionMeta;
use code_protocol::protocol::SessionMetaLine;

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct SessionStateSnapshot {}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct SavedSession {
    pub session: SessionMeta,
    #[serde(default)]
    pub items: Vec<RolloutItem>,
    #[serde(default)]
    pub history_snapshot: Option<HistorySnapshot>,
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
        source: SessionSource,
    },
    Resume {
        path: PathBuf,
    },
}

enum RolloutCmd {
    AddItems(Vec<RolloutItem>),
    SetSnapshot(serde_json::Value),
    Shutdown { ack: oneshot::Sender<()> },
}

/// Tracks context needed to update the session catalog after writes.
struct CatalogUpdateState {
    code_home: PathBuf,
    session_id: ConversationId,
    rollout_path: PathBuf,
    last_timestamp: String,
}

impl RolloutRecorderParams {
    pub fn new(
        conversation_id: ConversationId,
        instructions: Option<String>,
        source: SessionSource,
    ) -> Self {
        Self::Create {
            conversation_id,
            instructions,
            source,
        }
    }

    // Note: older APIs used a different resume entrypoint; prefer RolloutRecorder::resume.
}

impl RolloutRecorder {
    /// List conversations (rollout files) under the provided Codex home directory.
    pub async fn list_conversations(
        code_home: &Path,
        page_size: usize,
        cursor: Option<&Cursor>,
        allowed_sources: &[SessionSource],
    ) -> std::io::Result<ConversationsPage> {
        get_conversations(code_home, page_size, cursor, allowed_sources).await
    }

    /// Attempt to create a new [`RolloutRecorder`]. If the sessions directory
    /// cannot be created or the rollout file cannot be opened we return the
    /// error so the caller can decide whether to disable persistence.
    pub async fn new(config: &Config, params: RolloutRecorderParams) -> std::io::Result<Self> {
        let (file, rollout_path, meta) = match params {
            RolloutRecorderParams::Create {
                conversation_id,
                instructions,
                source,
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
                        source,
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
        let snapshot_path = rollout_path.with_extension("snapshot.json");

        let catalog_state = meta.as_ref().map(|meta| CatalogUpdateState {
            code_home: config.code_home.clone(),
            session_id: meta.id,
            rollout_path: rollout_path.clone(),
            last_timestamp: meta.timestamp.clone(),
        });

        // A reasonably-sized bounded channel. If the buffer fills up the send
        // future will yield, which is fine – we only need to ensure we do not
        // perform *blocking* I/O on the caller's thread.
        let (tx, rx) = mpsc::channel::<RolloutCmd>(256);

        // Spawn a Tokio task that owns the file handle and performs async
        // writes. Using `tokio::fs::File` keeps everything on the async I/O
        // driver instead of blocking the runtime.
        tokio::task::spawn(rollout_writer(
            file,
            rx,
            meta,
            cwd,
            snapshot_path,
            catalog_state,
        ));

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
        events: &[code_protocol::protocol::RecordedEvent],
    ) -> std::io::Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        let mut filtered = Vec::new();
        for event in events {
            if event_msg_from_protocol(&event.msg)
                .is_some_and(|msg| crate::rollout::policy::should_persist_event_msg(&msg))
            {
                filtered.push(RolloutItem::Event(event.clone()));
            }
        }
        if filtered.is_empty() {
            return Ok(());
        }
        self.tx
            .send(RolloutCmd::AddItems(filtered))
            .await
            .map_err(|e| IoError::other(format!("failed to queue rollout events: {e}")))
    }

    pub(crate) async fn set_history_snapshot(
        &self,
        snapshot: serde_json::Value,
    ) -> std::io::Result<()> {
        self.tx
            .send(RolloutCmd::SetSnapshot(snapshot))
            .await
            .map_err(|e| IoError::other(format!("failed to queue history snapshot: {e}")))
    }

    /// No-op compatibility shim for older APIs expecting a state snapshot.
    pub async fn record_state(&self, _snapshot: SessionStateSnapshot) -> std::io::Result<()> {
        Ok(())
    }

    /// Compatibility wrapper for older resume API used by codex.rs
    pub async fn resume(config: &Config, path: &Path) -> std::io::Result<(Self, SavedSession)> {
        let recorder = Self::new(config, RolloutRecorderParams::Resume { path: path.to_path_buf() }).await?;
        let history = Self::get_rollout_history(path).await?;
        let (session_id, items) = match history {
            InitialHistory::Resumed(resumed) => {
                (uuid::Uuid::from(resumed.conversation_id), resumed.history)
            }
            _ => (uuid::Uuid::new_v4(), Vec::new()),
        };
        let snapshot_path = path.with_extension("snapshot.json");
        let history_snapshot = match tokio::fs::read_to_string(&snapshot_path).await {
            Ok(json) => match serde_json::from_str::<crate::history::HistorySnapshot>(&json) {
                Ok(snapshot) => Some(snapshot),
                Err(e) => {
                    warn!("failed to parse history snapshot from {:?}: {}", snapshot_path, e);
                    None
                }
            },
            Err(err) => {
                if err.kind() != std::io::ErrorKind::NotFound {
                    warn!("failed to read history snapshot from {:?}: {}", snapshot_path, err);
                }
                None
            }
        };
        let saved = SavedSession {
            session: SessionMeta::default(),
            items,
            history_snapshot,
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
                        match &ev.msg {
                            ProtoEventMsg::UserMessage(user_msg) => {
                                let mut content = Vec::new();
                                content.push(ContentItem::InputText {
                                    text: user_msg.message.clone(),
                                });
                                if let Some(images) = &user_msg.images {
                                    for image_url in images {
                                        content.push(ContentItem::InputImage {
                                            image_url: image_url.clone(),
                                        });
                                    }
                                }
                                items.push(RolloutItem::ResponseItem(ResponseItem::Message {
                                    id: Some(ev.id.clone()),
                                    role: "user".to_string(),
                                    content,
                                }));
                            }
                            ProtoEventMsg::AgentMessage(_) => items.push(RolloutItem::Event(ev)),
                            _ => items.push(RolloutItem::Event(ev)),
                        }
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
    let mut dir = config.code_home.clone();
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
    snapshot_path: PathBuf,
    mut catalog_state: Option<CatalogUpdateState>,
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
        let (timestamp, _) = writer
            .write_rollout_item(RolloutItem::SessionMeta(session_meta_line))
            .await?;

        if let Some(ref mut state) = catalog_state {
            state.last_timestamp = timestamp;
            if let Err(err) = super::catalog::update_catalog_entry(
                &state.code_home,
                &state.rollout_path,
                state.session_id.into(),
                &state.last_timestamp,
            )
            .await
            {
                warn!("failed to update session catalog after meta write: {err}");
            }
        }
    }

    // Process rollout commands
    while let Some(cmd) = rx.recv().await {
        match cmd {
            RolloutCmd::AddItems(items) => {
                for item in items {
                    if should_persist_rollout_item(&item) {
                        let (timestamp, _) = writer.write_rollout_item(item).await?;
                        if let Some(ref mut state) = catalog_state {
                            state.last_timestamp = timestamp;
                        }
                    }
                }

                if let Some(ref state) = catalog_state {
                    if let Err(err) = super::catalog::update_catalog_entry(
                        &state.code_home,
                        &state.rollout_path,
                        state.session_id.into(),
                        &state.last_timestamp,
                    )
                    .await
                    {
                        warn!("failed to update session catalog after AddItems: {err}");
                    }
                }
            }
            RolloutCmd::SetSnapshot(snapshot) => {
                if let Err(err) = write_snapshot(&snapshot_path, &snapshot).await {
                    warn!("failed to persist history snapshot: {err}");
                }
            }
            RolloutCmd::Shutdown { ack } => {
                let _ = ack.send(());
            }
        }
    }

    Ok(())
}

async fn write_snapshot(path: &Path, snapshot: &serde_json::Value) -> std::io::Result<()> {
    let json = serde_json::to_vec(snapshot)
        .map_err(|e| IoError::other(format!("failed to serialize history snapshot: {e}")))?;
    tokio::fs::write(path, json).await
}

struct JsonlWriter {
    file: tokio::fs::File,
}

impl JsonlWriter {
    async fn write_rollout_item(
        &mut self,
        rollout_item: RolloutItem,
    ) -> std::io::Result<(String, ())> {
        let timestamp_format: &[FormatItem] = format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        );
        let timestamp = OffsetDateTime::now_utc()
            .format(timestamp_format)
            .map_err(|e| IoError::other(format!("failed to format timestamp: {e}")))?;

        let line = RolloutLine {
            timestamp: timestamp.clone(),
            item: rollout_item,
        };
        self.write_line(&line).await?;
        Ok((timestamp, ()))
    }
    async fn write_line(&mut self, item: &impl serde::Serialize) -> std::io::Result<()> {
        let mut json = serde_json::to_string(item)?;
        json.push('\n');
        self.file.write_all(json.as_bytes()).await?;
        self.file.flush().await?;
        Ok(())
    }
}
