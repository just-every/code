use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::error::ProtocolError;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

const META_FILE: &str = "code-bridge.json";
const HEARTBEAT_STALE_MS: i64 = 20_000;
const DEFAULT_CAPABILITIES: &[&str] = &[
    "error",
    "console",
    "pageview",
    "navigation",
    "network",
    "screenshot",
    "control",
];

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BridgeMeta {
    pub url: String,
    pub secret: String,
    #[allow(dead_code)]
    pub port: Option<u16>,
    pub workspace_path: Option<String>,
    #[allow(dead_code)]
    pub started_at: Option<String>,
    pub heartbeat_at: Option<String>,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct BridgeTarget {
    pub meta_path: PathBuf,
    pub meta: BridgeMeta,
    pub stale: bool,
    pub heartbeat_age_ms: Option<i64>,
}

#[derive(Debug)]
pub struct ControlOutcome {
    pub delivered: usize,
    pub result: Option<Value>,
    pub screenshot_bytes: Option<usize>,
    pub screenshot_mime: Option<String>,
}

pub fn discover_bridge_targets(cwd: &Path) -> Result<Vec<BridgeTarget>> {
    let mut seen = HashSet::new();
    let mut targets = Vec::new();
    let mut current = Some(cwd);

    while let Some(dir) = current {
        let candidate = dir.join(".code").join(META_FILE);
        if candidate.exists() && seen.insert(candidate.clone()) {
            let raw = fs::read_to_string(&candidate).context("read bridge metadata")?;
            let meta: BridgeMeta = serde_json::from_str(&raw).context("parse bridge metadata")?;
            let (stale, heartbeat_age_ms) = compute_staleness(&meta, &candidate);

            targets.push(BridgeTarget {
                meta_path: candidate,
                meta,
                stale,
                heartbeat_age_ms,
            });
        }

        current = dir.parent();
    }

    Ok(targets)
}

pub async fn list_control_capable(target: &BridgeTarget) -> Result<usize> {
    let (mut tx, mut rx) = connect_and_subscribe(target, &default_levels(), &[]).await?;
    let id = format!("code-cli-list-{}", Uuid::new_v4());
    let payload = serde_json::json!({
        "type": "control_request",
        "id": id,
        "action": "ping",
        "expectResult": false,
    });
    tx.send(Message::Text(payload.to_string()))
        .await
        .context("send ping control")?;

    let delivered = wait_for_forwarded(&mut rx, &id, Duration::from_secs(2)).await?;
    // Close politely so the host drops the consumer quickly
    let _ = tx
        .send(Message::Close(Some(CloseFrame {
            code: CloseCode::Normal,
            reason: "ok".into(),
        })))
        .await;
    Ok(delivered.unwrap_or(0))
}

pub async fn tail_events(target: &BridgeTarget, level: &str, raw: bool) -> Result<()> {
    let levels = vec![normalise_level(level)?];
    let caps = DEFAULT_CAPABILITIES
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let (mut tx, mut rx) = connect_and_subscribe(target, &levels, &caps).await?;

    println!(
        "Connected to bridge {}{}",
        target.meta.url,
        if target.stale {
            " (metadata stale, awaiting live data)"
        } else {
            ""
        }
    );
    println!("Subscribed to levels: {}", levels.join(", "));
    println!("Press Ctrl+C to stop.\n");

    loop {
        tokio::select! {
            msg = rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if raw {
                            println!("{}", text);
                            continue;
                        }
                        if let Ok(val) = serde_json::from_str::<Value>(&text) {
                            if let Some(line) = format_bridge_message(&val) {
                                println!("{}", line);
                            }
                        }
                    }
                    Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Close(_))) => break,
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        eprintln!("Bridge stream error: {err:?}");
                        break;
                    }
                    None => break,
                }
            }
            _ = tokio::signal::ctrl_c() => {
                let _ = tx
                    .send(Message::Close(Some(CloseFrame { code: CloseCode::Normal, reason: "interrupt".into() })))
                    .await;
                break;
            }
        }
    }

    Ok(())
}

pub async fn request_screenshot(
    target: &BridgeTarget,
    timeout_secs: u64,
) -> Result<ControlOutcome> {
    // Subscribe at info so screenshot events (level=info) are delivered
    let levels = vec!["info".to_string()];
    let caps = vec!["screenshot".to_string(), "control".to_string()];
    let (mut tx, mut rx) = connect_and_subscribe(target, &levels, &caps).await?;

    let id = format!("code-cli-screenshot-{}", Uuid::new_v4());
    let payload = serde_json::json!({
        "type": "control_request",
        "id": id,
        "action": "screenshot",
        "args": {},
        "timeoutMs": timeout_secs * 1000,
    });
    tx.send(Message::Text(payload.to_string()))
        .await
        .context("send screenshot control")?;

    let delivered = wait_for_forwarded(&mut rx, &id, Duration::from_secs(2)).await?;
    if delivered == Some(0) {
        bail!("No control-capable bridges are connected (advertising screenshot)");
    }

    let (result, screenshot_meta) =
        wait_for_control_and_screenshot(&mut rx, &id, Duration::from_secs(timeout_secs)).await?;
    let screenshot_bytes = screenshot_meta.as_ref().map(|m| m.0);
    let screenshot_mime = screenshot_meta.map(|m| m.1);

    let _ = tx
        .send(Message::Close(Some(CloseFrame {
            code: CloseCode::Normal,
            reason: "done".into(),
        })))
        .await;

    Ok(ControlOutcome {
        delivered: delivered.unwrap_or(0),
        result,
        screenshot_bytes,
        screenshot_mime,
    })
}

pub async fn run_javascript(
    target: &BridgeTarget,
    code: &str,
    timeout_secs: u64,
) -> Result<ControlOutcome> {
    let levels = vec!["errors".to_string()];
    let caps = vec!["control".to_string()];
    let (mut tx, mut rx) = connect_and_subscribe(target, &levels, &caps).await?;

    let id = format!("code-cli-js-{}", Uuid::new_v4());
    let payload = serde_json::json!({
        "type": "control_request",
        "id": id,
        // The bridge library handles `eval` by default; keep action aligned with the tool naming.
        "action": "eval",
        "code": code,
        "timeoutMs": timeout_secs * 1000,
        "expectResult": true,
    });
    tx.send(Message::Text(payload.to_string()))
        .await
        .context("send javascript control")?;

    let delivered = wait_for_forwarded(&mut rx, &id, Duration::from_secs(2)).await?;
    if delivered == Some(0) {
        bail!("No control-capable bridges are connected");
    }

    let (result, _shot_meta) =
        wait_for_control_and_screenshot(&mut rx, &id, Duration::from_secs(timeout_secs)).await?;

    let _ = tx
        .send(Message::Close(Some(CloseFrame {
            code: CloseCode::Normal,
            reason: "done".into(),
        })))
        .await;

    Ok(ControlOutcome {
        delivered: delivered.unwrap_or(0),
        result,
        screenshot_bytes: None,
        screenshot_mime: None,
    })
}

fn default_levels() -> Vec<String> {
    vec!["errors".to_string(), "warn".to_string(), "info".to_string()]
}

fn compute_staleness(meta: &BridgeMeta, path: &Path) -> (bool, Option<i64>) {
    if let Some(hb) = &meta.heartbeat_at {
        if let Ok(ts) = DateTime::parse_from_rfc3339(hb) {
            let age = Utc::now().signed_duration_since(ts.with_timezone(&Utc));
            return (age.num_milliseconds() > HEARTBEAT_STALE_MS, Some(age.num_milliseconds()));
        }
    }

    if let Ok(stat) = std::fs::metadata(path) {
        if let Ok(modified) = stat.modified() {
            let modified: DateTime<Utc> = modified.into();
            let age = Utc::now().signed_duration_since(modified);
            return (age.num_milliseconds() > HEARTBEAT_STALE_MS, Some(age.num_milliseconds()));
        }
    }

    (false, None)
}

async fn connect_and_subscribe(
    target: &BridgeTarget,
    levels: &[String],
    capabilities: &[String],
) -> Result<(SplitSink<WsStream, Message>, SplitStream<WsStream>)> {
    // connect_async returns owned WebSocketStream; splitting after auth allows reuse of the stream for waits
    let (ws, _) = connect_async(&target.meta.url)
        .await
        .with_context(|| format!("connect to {}", target.meta.url))?;
    let (mut tx, mut rx) = ws.split();

    let client_id = format!(
        "code-cli-{}",
        target
            .meta
            .workspace_path
            .as_deref()
            .unwrap_or("workspace")
            .rsplit_once('/')
            .map(|(_, tail)| tail)
            .unwrap_or("workspace"),
    );

    let auth = serde_json::json!({
        "type": "auth",
        "role": "consumer",
        "secret": target.meta.secret,
        "clientId": client_id,
    });
    tx.send(Message::Text(auth.to_string()))
        .await
        .context("send auth")?;
    if wait_for_type(&mut rx, &["auth_success"], Duration::from_secs(5))
        .await?
        .is_none()
    {
        bail!("bridge authentication timed out");
    }

    let subscribe = serde_json::json!({
        "type": "subscribe",
        "levels": levels,
        "capabilities": capabilities,
        "llm_filter": "off",
    });
    tx.send(Message::Text(subscribe.to_string()))
        .await
        .context("send subscribe")?;
    if wait_for_type(&mut rx, &["subscribe_ack"], Duration::from_secs(5))
        .await?
        .is_none()
    {
        bail!("bridge subscribe timed out");
    }

    Ok((tx, rx))
}

async fn wait_for_type(
    rx: &mut SplitStream<WsStream>,
    expected: &[&str],
    dur: Duration,
) -> Result<Option<Value>> {
    let expected_lower: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    let found = timeout(dur, async {
        while let Some(msg) = rx.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(val) = serde_json::from_str::<Value>(&text) {
                        if val
                            .get("type")
                            .and_then(|t| t.as_str())
                            .map(|t| expected_lower.contains(&t.to_string()))
                            .unwrap_or(false)
                        {
                            return Some(val);
                        }
                    }
                }
                Ok(Message::Binary(_)) => {}
                Ok(Message::Close(frame)) => {
                    let reason = frame.map(|f| f.reason.to_string()).unwrap_or_default();
                    return Some(serde_json::json!({"type":"close","reason":reason}));
                }
                Ok(_) => {}
                Err(WsError::Protocol(ProtocolError::ResetWithoutClosingHandshake)) => return None,
                Err(err) => {
                    eprintln!("bridge socket error: {err:?}");
                    return None;
                }
            }
        }
        None
    })
    .await
    .unwrap_or(None);

    Ok(found)
}

async fn wait_for_forwarded(
    rx: &mut SplitStream<WsStream>,
    id: &str,
    dur: Duration,
) -> Result<Option<usize>> {
    let found = timeout(dur, async {
        while let Some(msg) = rx.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(val) = serde_json::from_str::<Value>(&text) {
                        if val.get("type").and_then(|t| t.as_str()) == Some("control_forwarded")
                            && val.get("id").and_then(|v| v.as_str()) == Some(id)
                        {
                            return val.get("delivered").and_then(|v| v.as_u64()).map(|v| v as usize);
                        }
                    }
                }
                Ok(Message::Binary(_)) => {}
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        None
    })
    .await
    .unwrap_or(None);

    Ok(found)
}

async fn wait_for_control_and_screenshot(
    rx: &mut SplitStream<WsStream>,
    id: &str,
    dur: Duration,
) -> Result<(Option<Value>, Option<(usize, String)>)> {
    let mut result: Option<Value> = None;
    let mut screenshot: Option<(usize, String)> = None;

    let _ = timeout(dur, async {
        while let Some(msg) = rx.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(val) = serde_json::from_str::<Value>(&text) {
                        match val.get("type").and_then(|t| t.as_str()) {
                            Some("control_result") if val.get("id").and_then(|v| v.as_str()) == Some(id) => {
                                result = Some(val.clone());
                                if screenshot.is_some() {
                                    break;
                                }
                            }
                            Some("screenshot") if val.get("id").and_then(|v| v.as_str()) == Some(id) => {
                                let data_len = val
                                    .get("data")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.len());
                                let mime = val
                                    .get("mime")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                if let Some(len) = data_len {
                                    screenshot = Some((len, mime));
                                    if result.is_some() {
                                        break;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Message::Binary(_)) => {}
                Ok(_) => {}
                Err(_) => break,
            }
        }
    })
    .await;

    Ok((result, screenshot))
}

fn format_bridge_message(val: &Value) -> Option<String> {
    let t = val.get("type").and_then(|v| v.as_str())?;
    match t {
        "subscribe_ack" | "control_forwarded" => None,
        "rate_limit_notice" => {
            let reason = val
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("rate_limit");
            let msg = val
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or(reason);
            Some(format!("⚠ drop/rate-limit: {msg}"))
        }
        "control_result" => {
            let ok = val.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
            let id = val.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let summary = if ok {
                val.get("result")
                    .map(|r| format_result(r))
                    .unwrap_or_else(|| "ok".to_string())
            } else {
                val.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("error")
                    .to_string()
            };
            Some(format!("[control:{id}] {summary}"))
        }
        _ => {
            let level = val.get("level").and_then(|v| v.as_str()).unwrap_or("info");
            let ts = val
                .get("timestamp")
                .and_then(|v| v.as_i64())
                .map(|ms| format_ts(ms))
                .unwrap_or_else(|| "--:--:--".to_string());

            let body = match t {
                "screenshot" => {
                    let mime = val.get("mime").and_then(|v| v.as_str()).unwrap_or("?");
                    let bytes = val
                        .get("data")
                        .and_then(|v| v.as_str())
                        .map(|s| s.len())
                        .unwrap_or(0);
                    format!("screenshot {mime} ({} KB)", bytes / 1024)
                }
                "navigation" => {
                    let to = val
                        .get("navigation")
                        .and_then(|n| n.get("to"))
                        .and_then(|v| v.as_str())
                        .or_else(|| val.get("route").and_then(|v| v.as_str()))
                        .unwrap_or("");
                    format!("navigation -> {to}")
                }
                _ => val
                    .get("message")
                    .and_then(|m| m.as_str())
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| t.to_string()),
            };

            Some(format!("{ts} [{level}/{t}] {body}"))
        }
    }
}

fn format_result(val: &Value) -> String {
    if let Some(s) = val.as_str() {
        return s.to_string();
    }
    if val.is_object() || val.is_array() {
        return serde_json::to_string(val).unwrap_or_else(|_| "ok".to_string());
    }
    val.to_string()
}

fn format_ts(ms: i64) -> String {
    let dt = DateTime::from_timestamp_millis(ms).unwrap_or_else(|| Utc::now());
    dt.format("%H:%M:%S").to_string()
}

fn normalise_level(raw: &str) -> Result<String> {
    let lvl = raw.trim().to_lowercase();
    match lvl.as_str() {
        "errors" | "warn" | "info" | "trace" => Ok(lvl),
        _ => bail!("invalid level (use errors|warn|info|trace)"),
    }
}
