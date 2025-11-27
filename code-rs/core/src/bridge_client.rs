use std::path::PathBuf;
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

use crate::codex::Session;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct BridgeMeta {
    url: String,
    secret: String,
    #[allow(dead_code)]
    port: Option<u16>,
    #[allow(dead_code)]
    workspace_path: Option<String>,
    #[allow(dead_code)]
    started_at: Option<String>,
}

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
    let meta_path = session.get_cwd().join(".code/code-bridge.json");
    tokio::spawn(async move {
        loop {
            if let Ok(meta) = read_meta(&meta_path) {
                info!("[bridge] host metadata found, connecting");
                if let Err(err) = connect_and_listen(meta, &session).await {
                    warn!("[bridge] connect failed: {err:?}");
                }
            }
            sleep(Duration::from_secs(5)).await;
        }
    });
}

fn read_meta(path: &PathBuf) -> Result<BridgeMeta> {
    let data = std::fs::read_to_string(path)?;
    let meta: BridgeMeta = serde_json::from_str(&data)?;
    Ok(meta)
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
