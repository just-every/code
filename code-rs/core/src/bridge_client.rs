use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use anyhow::bail;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures_util::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::sleep;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

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
const SUBSCRIPTION_OVERRIDE_FILE: &str = "code-bridge.subscription.json";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Subscription {
    #[serde(default = "default_levels")]
    pub levels: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_filter")]
    pub llm_filter: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SubscriptionState {
    workspace: Option<Subscription>,
    session: Option<Subscription>,
    last_sent: Option<Subscription>,
}

static SUBSCRIPTIONS: Lazy<Mutex<SubscriptionState>> = Lazy::new(|| Mutex::new(SubscriptionState::default()));

static CONTROL_SENDER: Lazy<Mutex<Option<tokio::sync::mpsc::UnboundedSender<String>>>> =
    Lazy::new(|| Mutex::new(None));
static LAST_OVERRIDE_FINGERPRINT: Lazy<Mutex<Option<u64>>> = Lazy::new(|| Mutex::new(None));
static BRIDGE_HINT_EMITTED: Lazy<AtomicBool> = Lazy::new(|| AtomicBool::new(false));

fn default_levels() -> Vec<String> {
    vec!["errors".to_string()]
}

fn default_filter() -> String {
    "off".to_string()
}

fn default_subscription() -> Subscription {
    Subscription {
        levels: default_levels(),
        capabilities: Vec::new(),
        llm_filter: default_filter(),
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SubscriptionOverride {
    #[serde(default = "default_levels")]
    levels: Vec<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default = "default_filter", alias = "llm_filter")]
    llm_filter: String,
}

impl SubscriptionOverride {
    fn fingerprint(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        let mut lvls = self.levels.clone();
        lvls.iter_mut().for_each(|l| *l = l.to_lowercase());
        lvls.sort();
        lvls.hash(&mut hasher);

        let mut caps = self.capabilities.clone();
        caps.iter_mut().for_each(|c| *c = c.to_lowercase());
        caps.sort();
        caps.hash(&mut hasher);

        self.llm_filter.to_lowercase().hash(&mut hasher);
        hasher.finish()
    }

    fn normalised(mut self) -> Self {
        self.levels = normalise_vec(self.levels);
        self.capabilities = normalise_vec(self.capabilities);
        self.llm_filter = self.llm_filter.to_lowercase();
        self
    }
}

fn normalise_vec(values: Vec<String>) -> Vec<String> {
    let mut vals: Vec<String> = values
        .into_iter()
        .map(|v| v.trim().to_lowercase())
        .filter(|v| !v.is_empty())
        .collect();
    vals.sort();
    vals.dedup();
    vals
}

pub(crate) fn merge_effective_subscription(state: &SubscriptionState) -> Subscription {
    // Start with defaults
    let mut effective = default_subscription();

    if let Some(ws) = &state.workspace {
        if !ws.levels.is_empty() {
            effective.levels = ws.levels.clone();
        }
        if !ws.capabilities.is_empty() {
            effective.capabilities = ws.capabilities.clone();
        }
        effective.llm_filter = ws.llm_filter.clone();
    }

    if let Some(sess) = &state.session {
        // Session overrides always win, even when the intent is to clear values
        effective.levels = sess.levels.clone();
        effective.capabilities = sess.capabilities.clone();
        effective.llm_filter = sess.llm_filter.clone();
    }

    effective
}

#[allow(dead_code)]
pub(crate) fn set_bridge_levels(levels: Vec<String>) {
    let mut state = SUBSCRIPTIONS.lock().unwrap();
    let mut sub = state
        .session
        .clone()
        .unwrap_or_else(|| merge_effective_subscription(&state));
    sub.levels = if levels.is_empty() { default_levels() } else { normalise_vec(levels) };
    state.session = Some(sub);
    maybe_resubscribe(&mut state);
}

#[allow(dead_code)]
pub(crate) fn set_bridge_subscription(levels: Vec<String>, capabilities: Vec<String>) {
    let mut state = SUBSCRIPTIONS.lock().unwrap();
    let mut sub = state
        .session
        .clone()
        .unwrap_or_else(|| merge_effective_subscription(&state));
    sub.levels = if levels.is_empty() { default_levels() } else { normalise_vec(levels) };
    sub.capabilities = normalise_vec(capabilities);
    state.session = Some(sub);
    maybe_resubscribe(&mut state);
}

#[allow(dead_code)]
pub(crate) fn set_bridge_filter(filter: &str) {
    let mut state = SUBSCRIPTIONS.lock().unwrap();
    let mut sub = state
        .session
        .clone()
        .unwrap_or_else(|| merge_effective_subscription(&state));
    sub.llm_filter = filter.trim().to_lowercase();
    state.session = Some(sub);
    maybe_resubscribe(&mut state);
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
        let mut last_override_seen: Option<u64> = None;
        loop {
            // Poll subscription override (if any) each loop so runtime changes apply quickly.
            if let Some(path) = subscription_override_path(&cwd) {
                match read_subscription_override(path.as_path()) {
                    Ok(sub) => {
                        let fp = sub.fingerprint();
                        if Some(fp) != last_override_seen {
                            set_workspace_subscription(Some(Subscription {
                                levels: sub.levels.clone(),
                                capabilities: sub.capabilities.clone(),
                                llm_filter: sub.llm_filter.clone(),
                            }));
                            session
                                .record_bridge_event(format!(
                                    "Code Bridge subscription updated from {} (levels: [{}], capabilities: [{}], filter: {})",
                                    path.display(),
                                    sub.levels.join(", "),
                                    sub.capabilities.join(", "),
                                    sub.llm_filter
                                ))
                                .await;
                            *LAST_OVERRIDE_FINGERPRINT.lock().unwrap() = Some(fp);
                            last_override_seen = Some(fp);
                        }
                    }
                    Err(_) => {
                        if last_override_seen.is_some() {
                            set_workspace_subscription(None);
                            session
                                .record_bridge_event("Code Bridge subscription override removed or invalid; reverted to defaults (errors only).".to_string())
                                .await;
                            *LAST_OVERRIDE_FINGERPRINT.lock().unwrap() = None;
                            last_override_seen = None;
                        }
                    }
                }
            } else if last_override_seen.is_some() {
                set_workspace_subscription(None);
                session
                    .record_bridge_event(
                        "Code Bridge subscription override removed; reverted to defaults (errors only)."
                            .to_string(),
                    )
                    .await;
                *LAST_OVERRIDE_FINGERPRINT.lock().unwrap() = None;
                last_override_seen = None;
            }

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
                        if let Err(err) = connect_and_listen(meta, &session, &cwd).await {
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

fn read_subscription_override(path: &Path) -> Result<SubscriptionOverride> {
    let data = fs::read_to_string(path)?;
    let sub: SubscriptionOverride = serde_json::from_str(&data)?;
    Ok(sub.normalised())
}

pub(crate) fn set_workspace_subscription(sub: Option<Subscription>) {
    let mut state = SUBSCRIPTIONS.lock().unwrap();
    state.workspace = sub;
    maybe_resubscribe(&mut state);
}

pub(crate) fn set_session_subscription(sub: Option<Subscription>) {
    let mut state = SUBSCRIPTIONS.lock().unwrap();
    state.session = sub;
    maybe_resubscribe(&mut state);
}

pub(crate) fn force_resubscribe() {
    let mut state = SUBSCRIPTIONS.lock().unwrap();
    state.last_sent = None;
    maybe_resubscribe(&mut state);
}

pub(crate) fn get_effective_subscription() -> Subscription {
    let state = SUBSCRIPTIONS.lock().unwrap();
    merge_effective_subscription(&state)
}

pub(crate) fn get_workspace_subscription() -> Option<Subscription> {
    SUBSCRIPTIONS.lock().unwrap().workspace.clone()
}

pub(crate) fn persist_workspace_subscription(cwd: &Path, sub: Option<Subscription>) -> anyhow::Result<()> {
    let path = resolve_subscription_override_path(cwd);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    if let Some(sub) = sub {
        let tmp = path.with_extension("tmp");
        let payload = serde_json::to_string_pretty(&SubscriptionOverride {
            levels: sub.levels.clone(),
            capabilities: sub.capabilities.clone(),
            llm_filter: sub.llm_filter.clone(),
        })?;
        fs::write(&tmp, payload)?;
        fs::rename(tmp, &path)?;
    } else {
        if path.exists() {
            fs::remove_file(&path)?;
        }
    }

    Ok(())
}

fn maybe_resubscribe(state: &mut SubscriptionState) {
    let effective = merge_effective_subscription(state);
    if state.last_sent.as_ref() == Some(&effective) {
        return;
    }

    let msg = serde_json::json!({
        "type": "subscribe",
        "levels": effective.levels,
        "capabilities": effective.capabilities,
        "llm_filter": effective.llm_filter,
    })
    .to_string();

    if let Some(sender) = CONTROL_SENDER.lock().unwrap().as_ref() {
        let _ = sender.send(msg);
    }

    state.last_sent = Some(effective);
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

fn subscription_override_path(start: &Path) -> Option<PathBuf> {
    if let Some(meta) = find_meta_path(start) {
        if let Some(dir) = meta.parent() {
            let candidate = dir.join(SUBSCRIPTION_OVERRIDE_FILE);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    let mut current = Some(start);
    while let Some(dir) = current {
        let candidate = dir.join(".code").join(SUBSCRIPTION_OVERRIDE_FILE);
        if candidate.exists() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

fn resolve_subscription_override_path(start: &Path) -> PathBuf {
    if let Some(path) = subscription_override_path(start) {
        return path;
    }

    if let Some(dir) = find_meta_dir(start) {
        return dir.join(SUBSCRIPTION_OVERRIDE_FILE);
    }

    if let Some(dir) = find_code_dir(start) {
        return dir.join(SUBSCRIPTION_OVERRIDE_FILE);
    }

    start.join(".code").join(SUBSCRIPTION_OVERRIDE_FILE)
}

fn find_meta_dir(start: &Path) -> Option<PathBuf> {
    find_meta_path(start).and_then(|p| p.parent().map(Path::to_path_buf))
}

fn find_code_dir(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        let candidate = dir.join(".code");
        if candidate.is_dir() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

fn find_package_json(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        let candidate = dir.join("package.json");
        if candidate.exists() {
            return Some(candidate);
        }
        current = dir.parent();
    }
    None
}

fn workspace_has_code_bridge(start: &Path) -> bool {
    let pkg = match find_package_json(start) {
        Some(p) => p,
        None => return false,
    };

    let Ok(data) = fs::read_to_string(pkg.as_path()) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) else {
        return false;
    };

    let contains_dep = |section: &str| -> bool {
        json.get(section)
            .and_then(|v| v.as_object())
            .map(|map| map.contains_key("@just-every/code-bridge"))
            .unwrap_or(false)
    };

    contains_dep("dependencies") || contains_dep("devDependencies") || contains_dep("peerDependencies")
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

async fn connect_and_listen(meta: BridgeMeta, session: &Session, cwd: &Path) -> Result<()> {
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

    // initial subscribe using effective merged subscription
    let initial = {
        let state = SUBSCRIPTIONS.lock().unwrap();
        merge_effective_subscription(&state)
    };
    let subscribe = serde_json::json!({
        "type": "subscribe",
        "levels": initial.levels,
        "capabilities": initial.capabilities,
        "llm_filter": initial.llm_filter,
    })
    .to_string();
    tx.send(Message::Text(subscribe)).await?;
    {
        let mut state = SUBSCRIPTIONS.lock().unwrap();
        state.last_sent = Some(initial);
    }

    // set up control sender channel and forwarder (moves tx)
    let (ctrl_tx, mut ctrl_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    {
        let mut guard = CONTROL_SENDER.lock().unwrap();
        *guard = Some(ctrl_tx);
    }

    // Ensure any pending session overrides are pushed via control channel after it is set up
    force_resubscribe();

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

    if !BRIDGE_HINT_EMITTED.swap(true, Ordering::SeqCst) && workspace_has_code_bridge(cwd) {
        session
            .record_bridge_event(
                "Tip: adjust Code Bridge subscription with the internal tool `code_bridge_subscription` (show|set|clear; session-only by default; use persist=true to update .code/code-bridge.subscription.json for workspace defaults)."
                    .to_string(),
            )
            .await;
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn reset_state() {
        *SUBSCRIPTIONS.lock().unwrap() = SubscriptionState::default();
        *CONTROL_SENDER.lock().unwrap() = None;
        *LAST_OVERRIDE_FINGERPRINT.lock().unwrap() = None;
    }

    #[test]
    fn merge_respects_session_over_workspace() {
        reset_state();
        set_workspace_subscription(Some(Subscription {
            levels: vec!["info".into()],
            capabilities: vec!["console".into()],
            llm_filter: "minimal".into(),
        }));

        set_session_subscription(Some(Subscription {
            levels: vec!["trace".into()],
            capabilities: vec!["screenshot".into()],
            llm_filter: "off".into(),
        }));

        let state = SUBSCRIPTIONS.lock().unwrap();
        let eff = merge_effective_subscription(&state);
        assert_eq!(eff.levels, vec!["trace"]);
        assert_eq!(eff.capabilities, vec!["screenshot"]);
        assert_eq!(eff.llm_filter, "off".to_string());
    }

    #[test]
    fn session_can_clear_workspace_capabilities() {
        reset_state();
        set_workspace_subscription(Some(Subscription {
            levels: vec!["info".into()],
            capabilities: vec!["screenshot".into(), "pageview".into()],
            llm_filter: "minimal".into(),
        }));

        set_session_subscription(Some(Subscription {
            levels: vec!["info".into()],
            capabilities: Vec::new(),
            llm_filter: "minimal".into(),
        }));

        let state = SUBSCRIPTIONS.lock().unwrap();
        let eff = merge_effective_subscription(&state);
        assert!(eff.capabilities.is_empty());
        assert_eq!(eff.levels, vec!["info"]);
        assert_eq!(eff.llm_filter, "minimal".to_string());
    }

    #[test]
    fn resubscribe_sends_message_on_change() {
        reset_state();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        *CONTROL_SENDER.lock().unwrap() = Some(tx);

        set_session_subscription(Some(Subscription {
            levels: vec!["trace".into()],
            capabilities: vec!["console".into()],
            llm_filter: "off".into(),
        }));

        let msg = rx.try_recv().expect("expected subscribe message");
        assert!(msg.contains("\"type\":\"subscribe\""));
        assert!(msg.contains("trace"));
    }
}
