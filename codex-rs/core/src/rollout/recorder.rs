//! Persist Codex session rollouts (.jsonl) so sessions can be replayed or inspected later.

use std::fs::File;
use std::fs::{self};
use std::io::Error as IoError;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde::Serialize;
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
use uuid::Uuid;

use super::SESSIONS_SUBDIR;
use super::list::ConversationsPage;
use super::list::Cursor;
use super::list::get_conversations;
use crate::config::Config;
use super::policy::is_persisted_response_item;
use crate::conversation_manager::InitialHistory;
use codex_protocol::models::ResponseItem;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SessionMeta {
    pub id: Uuid,
    pub timestamp: String,
    pub instructions: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SessionMetaWithGit {
    #[serde(flatten)]
    meta: SessionMeta,
    #[serde(skip_serializing_if = "Option::is_none")] 
    git: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct SessionStateSnapshot {}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct SavedSession {
    pub session: SessionMeta,
    #[serde(default)]
    pub items: Vec<ResponseItem>,
    #[serde(default)]
    pub state: SessionStateSnapshot,
    pub session_id: Uuid,
}

#[derive(Clone)]
pub struct RolloutRecorder {
    tx: Sender<RolloutCmd>,
}

enum RolloutCmd {
    AddItems(Vec<ResponseItem>),
    UpdateState(SessionStateSnapshot),
    Shutdown { ack: oneshot::Sender<()> },
}

impl RolloutRecorder {
    #[allow(dead_code)]
    pub async fn list_conversations(
        codex_home: &Path,
        page_size: usize,
        cursor: Option<&Cursor>,
    ) -> std::io::Result<ConversationsPage> {
        get_conversations(codex_home, page_size, cursor).await
    }

    pub async fn new(
        config: &Config,
        uuid: Uuid,
        instructions: Option<String>,
    ) -> std::io::Result<Self> {
        let LogFileInfo { file, session_id, timestamp, path } = create_log_file(config, uuid)?;

        let timestamp_format: &[FormatItem] = format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        );
        let timestamp = timestamp
            .format(timestamp_format)
            .map_err(|e| IoError::other(format!("failed to format timestamp: {e}")))?;

        let (tx, rx) = mpsc::channel::<RolloutCmd>(256);
        let index_ctx = Some(IndexContext::new(
            config.codex_home.clone(),
            config.cwd.clone(),
            path,
            Some(config.model.clone()),
        ));
        tokio::task::spawn(rollout_writer(
            tokio::fs::File::from_std(file),
            rx,
            Some(SessionMeta { timestamp, id: session_id, instructions }),
            index_ctx,
        ));
        Ok(Self { tx })
    }

    pub async fn resume(config: &Config, path: &Path) -> std::io::Result<(Self, SavedSession)> {
        info!("Resuming rollout from {path:?}");
        let text = tokio::fs::read_to_string(path).await?;
        let mut lines = text.lines();
        let first = lines
            .next()
            .ok_or_else(|| IoError::other("empty session file"))?;
        let meta: SessionMeta = serde_json::from_str::<SessionMetaWithGit>(first)
            .map(|m| m.meta)
            .map_err(|e| IoError::other(format!("failed to parse session header: {e}")))?;

        let mut items = Vec::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let v: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("record_type")
                .and_then(|rt| rt.as_str())
                .map(|s| s == "state")
                .unwrap_or(false)
            {
                continue;
            }
            if let Ok(item) = serde_json::from_value::<ResponseItem>(v.clone()) {
                items.push(item);
            }
        }

        let file = std::fs::OpenOptions::new().append(true).read(true).open(path)?;
        let (tx, rx) = mpsc::channel::<RolloutCmd>(256);
        let index_ctx = Some(IndexContext::new(
            config.codex_home.clone(),
            config.cwd.clone(),
            path.to_path_buf(),
            Some(config.model.clone()),
        ));
        tokio::task::spawn(rollout_writer(
            tokio::fs::File::from_std(file),
            rx,
            None,
            index_ctx,
        ));

        let saved = SavedSession {
            session: meta.clone(),
            items: items.clone(),
            state: SessionStateSnapshot::default(),
            session_id: meta.id,
        };
        info!("Resumed rollout successfully from {path:?}");
        Ok((Self { tx }, saved))
    }

    pub(crate) async fn record_items(&self, items: &[ResponseItem]) -> std::io::Result<()> {
        let filtered: Vec<ResponseItem> = items
            .iter()
            .filter(|item| is_persisted_response_item(item))
            .cloned()
            .collect();
        if filtered.is_empty() {
            return Ok(());
        }
        self.tx
            .send(RolloutCmd::AddItems(filtered))
            .await
            .map_err(|e| IoError::other(format!("failed to queue rollout items: {e}")))
    }

    pub(crate) async fn record_state(&self, state: SessionStateSnapshot) -> std::io::Result<()> {
        self.tx
            .send(RolloutCmd::UpdateState(state))
            .await
            .map_err(|e| IoError::other(format!("failed to queue rollout state: {e}")))
    }

    pub async fn get_rollout_history(path: &Path) -> std::io::Result<InitialHistory> {
        info!("Resuming rollout from {path:?}");
        let text = tokio::fs::read_to_string(path).await?;
        let mut lines = text.lines();
        let _ = lines
            .next()
            .ok_or_else(|| IoError::other("empty session file"))?;
        let mut items = Vec::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            let v: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("record_type")
                .and_then(|rt| rt.as_str())
                .map(|s| s == "state")
                .unwrap_or(false)
            {
                continue;
            }
            if let Ok(item) = serde_json::from_value::<ResponseItem>(v.clone()) {
                items.push(item);
            }
        }
        if items.is_empty() {
            Ok(InitialHistory::New)
        } else {
            Ok(InitialHistory::Resumed(items))
        }
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
    file: File,
    session_id: Uuid,
    timestamp: OffsetDateTime,
    path: PathBuf,
}

fn create_log_file(config: &Config, session_id: Uuid) -> std::io::Result<LogFileInfo> {
    let timestamp = OffsetDateTime::now_local()
        .map_err(|e| IoError::other(format!("failed to get local time: {e}")))?;
    let mut dir = config.codex_home.clone();
    dir.push(SESSIONS_SUBDIR);
    dir.push(timestamp.year().to_string());
    dir.push(format!("{:02}", u8::from(timestamp.month())));
    dir.push(format!("{:02}", timestamp.day()));
    fs::create_dir_all(&dir)?;
    let format: &[FormatItem] = format_description!("[year]-[month]-[day]T[hour]-[minute]-[second]");
    let date_str = timestamp
        .format(format)
        .map_err(|e| IoError::other(format!("failed to format timestamp: {e}")))?;
    let filename = format!("rollout-{date_str}-{session_id}.jsonl");
    let path = dir.join(filename);
    let file = std::fs::OpenOptions::new().append(true).create(true).open(&path)?;
    Ok(LogFileInfo { file, session_id, timestamp, path })
}

async fn rollout_writer(
    mut file: tokio::fs::File,
    mut rx: mpsc::Receiver<RolloutCmd>,
    mut meta: Option<SessionMeta>,
    index_ctx: Option<IndexContext>,
) -> std::io::Result<()> {
    if let Some(session_meta) = meta.take() {
        let mut json = serde_json::to_string(&SessionMetaWithGit { meta: session_meta, git: None })?;
        json.push('\n');
        let _ = file.write_all(json.as_bytes()).await;
        file.flush().await?;

        // Write initial index line so the session appears under /resume once messages arrive
        if let Some(ctx) = index_ctx.as_ref() {
            let _ = append_dir_index_line(ctx, Some("0"), Some(&ctx.get_timestamp_now()), None).await;
        }
    }

    while let Some(cmd) = rx.recv().await {
        match cmd {
            RolloutCmd::AddItems(items) => {
                for item in &items {
                    let mut json = serde_json::to_string(&item)?;
                    json.push('\n');
                    let _ = file.write_all(json.as_bytes()).await;
                }
                file.flush().await?;

                // Update the per-directory index with message deltas and optional last user snippet
                if let Some(ctx) = index_ctx.as_ref() {
                    use codex_protocol::models::{ContentItem, ResponseItem};
                    let mut msg_count_delta: usize = 0;
                    let mut last_user_snippet: Option<String> = None;
                    for it in &items {
                        if let ResponseItem::Message { role, content, .. } = it {
                            if role == "user" || role == "assistant" {
                                msg_count_delta = msg_count_delta.saturating_add(1);
                            }
                            if role == "user" {
                                for c in content {
                                    match c {
                                        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                                            if !text.trim().is_empty() {
                                                // Keep a short, single-line snippet
                                                let mut s = text.trim().replace('\n', " ");
                                                const MAX: usize = 120;
                                                if s.chars().count() > MAX {
                                                    s = s.chars().take(MAX).collect::<String>() + "…";
                                                }
                                                last_user_snippet = Some(s);
                                                break;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    if msg_count_delta > 0 || last_user_snippet.is_some() {
                        let ts = ctx.get_timestamp_now();
                        let _ = append_dir_index_line(ctx, None, Some(&ts), last_user_snippet.as_deref())
                            .await
                            .ok();
                        if msg_count_delta > 0 {
                            // Separate line to carry the count delta so aggregator can sum
                            let _ = append_dir_index_count_delta(ctx, msg_count_delta).await.ok();
                        }
                    }
                }
            }
            RolloutCmd::UpdateState(state) => {
                #[derive(Serialize)]
                struct StateLine<'a> {
                    record_type: &'static str,
                    #[serde(flatten)]
                    state: &'a SessionStateSnapshot,
                }
                let mut json = serde_json::to_string(&StateLine { record_type: "state", state: &state })?;
                json.push('\n');
                let _ = file.write_all(json.as_bytes()).await;
                file.flush().await?;
            }
            RolloutCmd::Shutdown { ack } => { let _ = ack.send(()); }
        }
    }

    Ok(())
}

// --- Fast per-directory index writing ---

struct IndexContext {
    codex_home: PathBuf,
    cwd: PathBuf,
    session_path: PathBuf,
    model: Option<String>,
}

impl IndexContext {
    fn new(codex_home: PathBuf, cwd: PathBuf, session_path: PathBuf, model: Option<String>) -> Self {
        Self { codex_home, cwd, session_path, model }
    }

    fn index_file_path(&self) -> PathBuf {
        // Mirror tui::resume::discovery::super_sanitize_dir_index_path
        let mut name = self.cwd.to_string_lossy().to_string();
        name = name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        if name.len() > 160 { name.truncate(160); }
        let mut p = self.codex_home.clone();
        p.push("sessions");
        p.push("index");
        p.push("by-dir");
        p.push(format!("{}.jsonl", name));
        p
    }

    fn get_timestamp_now(&self) -> String {
        let now = OffsetDateTime::now_utc();
        let fmt: &[FormatItem] = format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        );
        now.format(fmt).unwrap_or_else(|e| format!("format error: {e}")).to_string()
    }

    fn git_branch(&self) -> Option<String> {
        let head_path = self.cwd.join(".git/HEAD");
        if let Ok(contents) = std::fs::read_to_string(&head_path) {
            if let Some(rest) = contents.trim().strip_prefix("ref: ") {
                if let Some(branch) = rest.trim().rsplit('/').next() {
                    return Some(branch.to_string());
                }
            }
        }
        None
    }
}

#[derive(Serialize)]
struct DirIndexLine<'a> {
    record_type: &'static str,
    cwd: &'a str,
    session_file: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_ts: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    modified_ts: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_count_delta: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_user_snippet: Option<&'a str>,
}

async fn append_dir_index_line(
    ctx: &IndexContext,
    created_ts: Option<&str>,
    modified_ts: Option<&str>,
    last_user_snippet: Option<&str>,
) -> std::io::Result<()> {
    let index_path = ctx.index_file_path();
    if let Some(parent) = index_path.parent() { tokio::fs::create_dir_all(parent).await.ok(); }
    let mut f = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_path)
        .await?;
    let cwd_str = ctx.cwd.to_string_lossy();
    let path_str = ctx.session_path.to_string_lossy();
    let model_str = ctx.model.as_deref();
    let branch = ctx.git_branch();
    let line = DirIndexLine {
        record_type: "dir_index",
        cwd: &cwd_str,
        session_file: &path_str,
        created_ts,
        modified_ts,
        message_count_delta: None,
        model: model_str,
        branch: branch.as_deref(),
        last_user_snippet,
    };
    let mut json = serde_json::to_string(&line)?;
    json.push('\n');
    let _ = f.write_all(json.as_bytes()).await;
    f.flush().await
}

async fn append_dir_index_count_delta(ctx: &IndexContext, delta: usize) -> std::io::Result<()> {
    let index_path = ctx.index_file_path();
    if let Some(parent) = index_path.parent() { tokio::fs::create_dir_all(parent).await.ok(); }
    let mut f = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_path)
        .await?;
    let cwd_str = ctx.cwd.to_string_lossy();
    let path_str = ctx.session_path.to_string_lossy();
    let ts = ctx.get_timestamp_now();
    let model_str = ctx.model.as_deref();
    let branch = ctx.git_branch();
    let line = DirIndexLine {
        record_type: "dir_index",
        cwd: &cwd_str,
        session_file: &path_str,
        created_ts: None,
        modified_ts: Some(&ts),
        message_count_delta: Some(delta),
        model: model_str,
        branch: branch.as_deref(),
        last_user_snippet: None,
    };
    let mut json = serde_json::to_string(&line)?;
    json.push('\n');
    let _ = f.write_all(json.as_bytes()).await;
    f.flush().await
}
