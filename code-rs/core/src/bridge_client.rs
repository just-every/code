use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use tokio::time::sleep;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};
use once_cell::sync::Lazy;
use std::sync::Mutex;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use anyhow::bail;

use crate::codex::Session;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct BridgeMeta {
    url: String,
    secret: String,
    #[allow(dead_code)]
    port: Option<u16>,
    #[allow(dead_code)]
    workspace_path: Option<String>,
    #[allow(dead_code)]
    started_at: Option<String>,
    #[allow(dead_code)]
    heartbeat_at: Option<String>,
}

const HEARTBEAT_STALE_MS: i64 = 20_000;

static DESIRED_LEVELS: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(vec!["errors".to_string()]));
static DESIRED_CAPABILITIES: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));
static CONTROL_SENDER: Lazy<Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>> =
    Lazy::new(|| Mutex::new(None));
static DESIRED_FILTER: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new("off".to_string()));

#[allow(dead_code)]
pub(crate) fn set_bridge_levels(levels: Vec<String>) {
    let mut guard = DESIRED_LEVELS.lock().unwrap();
    *guard = if levels.is_empty() { vec!["errors".to_string()] } else { levels };
}

#[allow(dead_code)]
pub(crate) fn set_bridge_subscription(levels: Vec<String>, capabilities: Vec<String>) {
    let mut lvl = DESIRED_LEVELS.lock().unwrap();
    *lvl = if levels.is_empty() { vec!["errors".to_string()] } else { levels };
    let mut caps = DESIRED_CAPABILITIES.lock().unwrap();
    *caps = capabilities;
}

#[allow(dead_code)]
pub(crate) fn set_bridge_filter(filter: &str) {
    let mut f = DESIRED_FILTER.lock().unwrap();
    *f = filter.to_string();
}

#[allow(dead_code)]
pub(crate) fn send_bridge_control(action: &str, args: serde_json::Value) {
    let msg = serde_json::json!({
        "type": "control",
        "action": action,
        "args": args,
    })
    .to_string();

    if let Some(sender) = CONTROL_SENDER.lock().unwrap().as_ref() {
        let _ = sender.send(msg);
    }
}

/// Spawn a background task that watches `.code/code-bridge.json` and
/// connects as a consumer to the external bridge host when available.
pub(crate) fn spawn_bridge_listener(session: std::sync::Arc<Session>) {
    let cwd = session.get_cwd().to_path_buf();
    tokio::spawn(async move {
        let mut last_notice: Option<&str> = None;
        loop {
            match find_meta_path(&cwd) {
                None => {
                    if last_notice != Some("missing") {
                        session
                            .record_bridge_event(
                                "Code Bridge metadata not found (.code/code-bridge.json); waiting for host..."
                                    .to_string(),
                            )
                            .await;
                        last_notice = Some("missing");
                    }
                }
                Some(meta_path) => match read_meta(meta_path.as_path()) {
                    Ok(meta) => {
                        last_notice = None;
                        info!("[bridge] host metadata found, connecting");
                        if let Err(err) = connect_and_listen(meta, &session).await {
                            warn!("[bridge] connect failed: {err:?}");
                        }
                    }
                    Err(err) => {
                        if last_notice != Some("stale") {
                            session
                                .record_bridge_event(format!(
                                    "Code Bridge metadata is stale at {} ({err}); waiting for a fresh host...",
                                    meta_path.display()
                                ))
                                .await;
                            last_notice = Some("stale");
                        }
                    }
                },
            }
            sleep(Duration::from_secs(5)).await;
        }
    });
}

fn read_meta(path: &Path) -> Result<BridgeMeta> {
    let data = std::fs::read_to_string(path)?;
    let meta: BridgeMeta = serde_json::from_str(&data)?;

    if is_meta_stale(&meta, path) {
        bail!("heartbeat missing or stale");
    }

    Ok(meta)
}

fn find_meta_path(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        let candidate = dir.join(".code/code-bridge.json");
        if candidate.exists() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

fn is_meta_stale(meta: &BridgeMeta, path: &Path) -> bool {
    if let Some(hb) = &meta.heartbeat_at {
        if let Ok(ts) = DateTime::parse_from_rfc3339(hb) {
            let age = Utc::now().signed_duration_since(ts.with_timezone(&Utc));
            return age.num_milliseconds() > HEARTBEAT_STALE_MS;
        }
    }

    // Fallback for hosts that don't emit heartbeat: use file mtime as staleness signal
    if let Ok(stat) = std::fs::metadata(path) {
        if let Ok(modified) = stat.modified() {
            let modified: DateTime<Utc> = modified.into();
            let age = Utc::now().signed_duration_since(modified);
            return age > ChronoDuration::milliseconds(HEARTBEAT_STALE_MS);
        }
    }
    false
}

async fn connect_and_listen(meta: BridgeMeta, session: &Session) -> Result<()> {
    let (ws, _) = connect_async(&meta.url).await?;
    let (mut tx, mut rx) = ws.split();

    // auth frame
    let auth = serde_json::json!({
        "type": "auth",
        "role": "consumer",
        "secret": meta.secret,
        "clientId": format!("code-consumer-{}", session.session_uuid()),
    })
    .to_string();
    tx.send(Message::Text(auth)).await?;

    // default subscription: errors only, no extra capabilities
    let desired_levels = DESIRED_LEVELS.lock().unwrap().clone();
    let desired_caps = DESIRED_CAPABILITIES.lock().unwrap().clone();
    let desired_filter = DESIRED_FILTER.lock().unwrap().clone();
    let subscribe = serde_json::json!({
        "type": "subscribe",
        "levels": desired_levels,
        "capabilities": desired_caps,
        "llm_filter": desired_filter,
    })
    .to_string();
    tx.send(Message::Text(subscribe)).await?;

    // set up control sender channel and forwarder (moves tx)
    let (ctrl_tx, mut ctrl_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    {
        let mut guard = CONTROL_SENDER.lock().unwrap();
        *guard = Some(ctrl_tx);
    }

    tokio::spawn(async move {
        while let Some(msg) = ctrl_rx.recv().await {
            if let Err(err) = tx.send(Message::Text(msg)).await {
                warn!("[bridge] control send error: {err:?}");
                break;
            }
        }
    });

    // announce developer message
    let announce = format!(
        "Code Bridge host available.\n- url: {url}\n- secret: {secret}\n",
        url = meta.url,
        secret = meta.secret
    );
    session.record_bridge_event(announce).await;

    while let Some(msg) = rx.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let summary = summarize(&text);
                session.record_bridge_event(summary).await;
            }
            Ok(Message::Binary(_)) => {}
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(_)) => {}
            Ok(Message::Pong(_)) => {}
            Ok(Message::Frame(_)) => {}
            Err(err) => {
                warn!("[bridge] websocket error: {err:?}");
                break;
            }
        }
    }
    // clear sender on exit
    {
        let mut guard = CONTROL_SENDER.lock().unwrap();
        *guard = None;
    }
    Ok(())
}

fn summarize(raw: &str) -> String {
    if let Ok(val) = serde_json::from_str::<Value>(raw) {
        let mut parts = Vec::new();
        if let Some(t) = val.get("type").and_then(|v| v.as_str()) {
            parts.push(format!("type: {t}"));
        }
        if let Some(level) = val.get("level").and_then(|v| v.as_str()) {
            parts.push(format!("level: {level}"));
        }
        if let Some(msg) = val.get("message").and_then(|v| v.as_str()) {
            parts.push(format!("message: {msg}"));
        }
        return format!("<code_bridge_event>\n{}\n</code_bridge_event>", parts.join("\n"));
    }
    raw.to_string()
}
