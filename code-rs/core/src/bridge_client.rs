use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use anyhow::bail;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures_util::{SinkExt, StreamExt};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::{sleep, sleep_until, Instant};
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
const BATCH_WINDOW: Duration = Duration::from_secs(3);
const MAX_EVENTS_PER_BATCH: usize = 50;
const MAX_EVENT_SUMMARY_CHARS: usize = 1200;

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct BridgeBatchEvent {
    summary: String,
    level: Option<String>,
    truncated: bool,
}

fn default_levels() -> Vec<String> {
    vec!["errors".to_string()]
}

fn default_filter() -> String {
    "off".to_string()
}

fn default_subscription() -> Subscription {
    Subscription {
        levels: default_levels(),
        capabilities: vec![
            "console".to_string(),
            "error".to_string(),
            "pageview".to_string(),
            "screenshot".to_string(),
            "control".to_string(),
        ],
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

fn parse_level(raw: &str) -> Option<String> {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|val| {
            val.get("level")
                .and_then(|v| v.as_str())
                .map(|s| s.to_lowercase())
                .or_else(|| {
                    val.get("type")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_lowercase())
                })
        })
}

fn is_error_level(level: &str) -> bool {
    matches!(level, "error" | "errors" | "err" | "fatal" | "critical" | "panic")
}

fn truncate_summary(text: &str) -> (String, bool) {
    let mut chars = text.chars();
    let truncated: String = chars.by_ref().take(MAX_EVENT_SUMMARY_CHARS).collect();
    if text.chars().count() > MAX_EVENT_SUMMARY_CHARS {
        let remaining = text.chars().count().saturating_sub(MAX_EVENT_SUMMARY_CHARS);
        (format!("{}... [truncated {remaining} chars]", truncated), true)
    } else {
        (truncated, false)
    }
}

fn summarize_event(raw: &str) -> BridgeBatchEvent {
    let level = parse_level(raw);
    let summary = summarize(raw);
    let (summary, truncated) = truncate_summary(&summary);

    BridgeBatchEvent {
        summary,
        level,
        truncated,
    }
}

#[derive(Debug)]
struct CoalescedBatch {
    entries: Vec<(String, usize)>,
    total_events: usize,
    truncated_events: usize,
    dropped_events: usize,
    saw_error: bool,
}

fn coalesce_events(events: Vec<BridgeBatchEvent>) -> CoalescedBatch {
    let mut entries: Vec<(String, usize)> = Vec::new();
    let mut truncated_events = 0;
    let mut dropped_events = 0;
    let mut saw_error = false;

    for event in events {
        let BridgeBatchEvent {
            summary,
            level,
            truncated,
        } = event;

        if truncated {
            truncated_events += 1;
        }

        if let Some(level) = level.as_deref() {
            if is_error_level(level) {
                saw_error = true;
            }
        }

        if let Some((_, count)) = entries.iter_mut().find(|(msg, _)| msg == &summary) {
            *count += 1;
            continue;
        }

        if entries.len() < MAX_EVENTS_PER_BATCH {
            entries.push((summary, 1));
        } else {
            dropped_events += 1;
        }
    }

    let total_events = entries.iter().map(|(_, count)| *count).sum::<usize>() + dropped_events;

    CoalescedBatch {
        entries,
        total_events,
        truncated_events,
        dropped_events,
        saw_error,
    }
}

fn format_batch_message(batch: &CoalescedBatch) -> String {
    if batch.entries.is_empty() {
        return "(no bridge events)".to_string();
    }

    let mut lines = Vec::new();
    let header = if batch.total_events == 1 {
        "Code Bridge event".to_string()
    } else {
        format!(
            "Code Bridge events ({} in last {}s)",
            batch.total_events,
            BATCH_WINDOW.as_secs()
        )
    };
    lines.push(header);

    for (msg, count) in batch.entries.iter() {
        let prefix = if *count > 1 {
            format!("[{count}x] ")
        } else {
            String::new()
        };
        let indented = msg.replace('\n', "\n  ");
        lines.push(format!("- {}{}", prefix, indented));
    }

    if batch.dropped_events > 0 {
        lines.push(format!(
            "(dropped {} events beyond batch limit of {})",
            batch.dropped_events, MAX_EVENTS_PER_BATCH
        ));
    }

    if batch.truncated_events > 0 {
        lines.push(format!(
            "(truncated {} event bodies to {} chars)",
            batch.truncated_events, MAX_EVENT_SUMMARY_CHARS
        ));
    }

    lines.join("\n")
}

async fn flush_batch(session: &Arc<Session>, events: Vec<BridgeBatchEvent>) {
    let batch = coalesce_events(events);
    if batch.entries.is_empty() {
        return;
    }

    let message = format_batch_message(&batch);
    session.record_bridge_event(message).await;

    if batch.saw_error {
        session.start_pending_only_turn_if_idle().await;
    }
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
                    last_notice = Some("missing");
                }
                Some(meta_path) => match read_meta(meta_path.as_path()) {
                    Ok(meta) => {
                        last_notice = None;
                        info!("[bridge] host metadata found, connecting");
                        if let Err(err) = connect_and_listen(meta, Arc::clone(&session), &cwd).await {
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

#[allow(dead_code)]
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

async fn connect_and_listen(meta: BridgeMeta, session: Arc<Session>, cwd: &Path) -> Result<()> {
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
                "Code Bridge is a local, real-time debug stream (errors/console like Sentry, plus pageviews/screenshots and a control channel). Use the `code_bridge` tool: `action=subscribe` with level (errors|warn|info|trace) to persist full-capability logging, `action=screenshot` to request a capture, or `action=javascript` with `code` to run JS on the bridge client."
                    .to_string(),
            )
            .await;
    }

    let (batch_tx, mut batch_rx) = tokio::sync::mpsc::unbounded_channel::<BridgeBatchEvent>();
    let session_for_batch = Arc::clone(&session);

    let batch_handle = tokio::spawn(async move {
        let mut buffer: Vec<BridgeBatchEvent> = Vec::new();
        let mut deadline: Option<Instant> = None;

        loop {
            tokio::select! {
                Some(item) = batch_rx.recv() => {
                    buffer.push(item);

                    if buffer.len() >= MAX_EVENTS_PER_BATCH {
                        flush_batch(&session_for_batch, std::mem::take(&mut buffer)).await;
                        deadline = None;
                        continue;
                    }

                    if deadline.is_none() {
                        deadline = Some(Instant::now() + BATCH_WINDOW);
                    }
                }
                _ = async {
                    if let Some(when) = deadline {
                        sleep_until(when).await;
                    }
                }, if deadline.is_some() => {
                    if !buffer.is_empty() {
                        flush_batch(&session_for_batch, std::mem::take(&mut buffer)).await;
                    }
                    deadline = None;
                }
                else => {
                    break;
                }
            }
        }

        if !buffer.is_empty() {
            flush_batch(&session_for_batch, buffer).await;
        }
    });

    while let Some(msg) = rx.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let event = summarize_event(&text);
                let _ = batch_tx.send(event);
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

    drop(batch_tx);
    let _ = batch_handle.await;
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
        if let Some(platform) = val.get("platform").and_then(|v| v.as_str()) {
            parts.push(format!("platform: {platform}"));
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

    #[test]
    fn batch_coalesces_and_truncates() {
        let long_msg = "a".repeat(MAX_EVENT_SUMMARY_CHARS + 10);
        let events = vec![
            BridgeBatchEvent {
                summary: "alpha".to_string(),
                level: Some("info".to_string()),
                truncated: false,
            },
            BridgeBatchEvent {
                summary: "alpha".to_string(),
                level: Some("info".to_string()),
                truncated: false,
            },
            BridgeBatchEvent {
                summary: long_msg.clone(),
                level: Some("error".to_string()),
                truncated: true,
            },
        ];

        let batch = coalesce_events(events);
        assert_eq!(batch.entries.len(), 2);
        assert_eq!(batch.entries[0].0, "alpha");
        assert_eq!(batch.entries[0].1, 2);
        assert!(batch.saw_error);
        assert_eq!(batch.truncated_events, 1);
        assert_eq!(batch.dropped_events, 0);
    }

    #[test]
    fn batch_enforces_limit_and_marks_error() {
        let mut events = Vec::new();
        for idx in 0..(MAX_EVENTS_PER_BATCH + 5) {
            events.push(BridgeBatchEvent {
                summary: format!("msg-{idx}"),
                level: Some(if idx == 0 { "error" } else { "info" }.to_string()),
                truncated: false,
            });
        }

        let batch = coalesce_events(events);
        assert_eq!(batch.entries.len(), MAX_EVENTS_PER_BATCH);
        assert_eq!(batch.dropped_events, 5);
        assert!(batch.saw_error);
        assert_eq!(batch.total_events, MAX_EVENTS_PER_BATCH + 5);
    }

    #[test]
    fn format_batch_includes_multipliers() {
        let batch = CoalescedBatch {
            entries: vec![
                ("one".to_string(), 1),
                ("two".to_string(), 3),
            ],
            total_events: 4,
            truncated_events: 0,
            dropped_events: 0,
            saw_error: false,
        };

        let text = format_batch_message(&batch);
        assert!(text.contains("Code Bridge events (4 in last"));
        assert!(text.contains("- one"));
        assert!(text.contains("- [3x] two"));
    }

    #[test]
    fn summarize_includes_platform_when_present() {
        let raw = r#"{"type":"console","level":"info","platform":"roblox","message":"hi"}"#;
        let summary = summarize(raw);
        assert!(summary.contains("platform: roblox"));
        assert!(summary.contains("message: hi"));
    }
}
