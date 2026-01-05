use super::{tool_cards, BrowserSessionOrderKey, ChatWidget, OrderKey};
use super::tool_cards::ToolCardSlot;
use crate::history_cell::{
    BrowserSessionCell,
    PlainHistoryCell,
    HistoryCellType,
    plain_message_state_from_lines,
};
use code_core::protocol::OrderMeta;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const BROWSER_CONSOLE_EVENT_PREFIX: &str = "[browser console]";

pub(super) struct BrowserSessionTracker {
    pub slot: ToolCardSlot,
    pub cell: BrowserSessionCell,
    pub elapsed: Duration,
    pub anchor_inserted: bool,
    pub first_screenshot_instant: Option<Instant>,
    pub last_screenshot_timestamp: Duration,
}

pub(super) struct BrowserScreenshotUpdateResult {
    pub grouped: bool,
    pub session_key: Option<String>,
}

struct BrowserActionSummary {
    action: String,
    target: Option<String>,
    value: Option<String>,
    outcome: Option<String>,
    status_code: Option<String>,
    headless: Option<bool>,
}

impl BrowserSessionTracker {
    fn new(order_key: OrderKey) -> Self {
        Self {
            slot: ToolCardSlot::new(order_key),
            cell: BrowserSessionCell::new(),
            elapsed: Duration::default(),
            anchor_inserted: false,
            first_screenshot_instant: None,
            last_screenshot_timestamp: Duration::ZERO,
        }
    }
}

fn insert_browser_anchor(
    chat: &mut ChatWidget<'_>,
    order_key: OrderKey,
    tracker: &BrowserSessionTracker,
) {
    let line = browser_anchor_line(tracker);
    let state = plain_message_state_from_lines(vec![line], HistoryCellType::BackgroundEvent);
    let cell = PlainHistoryCell::from_state(state);
    let _ = chat.history_insert_with_key_global_tagged(Box::new(cell), order_key, "background", None);
}

fn browser_anchor_line(tracker: &BrowserSessionTracker) -> Line<'static> {
    let label = tracker.cell.summary_label();
    let url = tracker.cell.current_url().map(|value| value.to_string());

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw("Started "));

    let mut title = label.clone();
    if title.trim().is_empty() {
        title = url
            .as_ref()
            .cloned()
            .unwrap_or_else(|| "browser session".to_string());
    }
    spans.push(Span::styled(title.clone(), Style::new().bold()));

    if let Some(url) = url.as_ref() {
        if url != &title {
            spans.push(Span::raw(format!(" ({})", url)));
        }
    }

    spans.push(Span::raw(" — latest view is shown below."));

    Line::from(spans)
}

fn ensure_cell_picker(chat: &ChatWidget<'_>, cell: &BrowserSessionCell) {
    let picker = chat.terminal_info.picker.clone();
    let font_size = chat.terminal_info.font_size;
    cell.ensure_picker_initialized(picker, font_size);
}

pub(super) fn handle_custom_tool_begin(
    chat: &mut ChatWidget<'_>,
    order: Option<&OrderMeta>,
    call_id: &str,
    tool_name: &str,
    params: Option<Value>,
) -> bool {
    if !tool_name.starts_with("browser_") || tool_name == "browser_fetch" {
        return false;
    }

    let (order_key, _) = order_key_and_ordinal(chat, order);
    let key = select_session_key(chat, order, call_id, tool_name);
    let mut tracker = chat
        .tools_state
        .browser_sessions
        .remove(&key)
        .unwrap_or_else(|| BrowserSessionTracker::new(order_key));
    tracker.slot.set_order_key(order_key);

    if let Some(Value::Object(json)) = params.as_ref() {
        if tool_name == "browser_open" {
            if let Some(url) = json.get("url").and_then(|v| v.as_str()) {
                tracker.cell.set_url(url.to_string());
            }
            if let Some(headless) = json.get("headless").and_then(|v| v.as_bool()) {
                tracker.cell.set_headless(Some(headless));
            }
        }
    }

    ensure_cell_picker(chat, &tracker.cell);
    if tracker.slot.has_order_change() && !tracker.anchor_inserted {
        if let Some(previous) = tracker.slot.last_inserted_order() {
            insert_browser_anchor(chat, previous, &tracker);
            tracker.anchor_inserted = true;
        }
    }
    tool_cards::assign_tool_card_key(&mut tracker.slot, &mut tracker.cell, Some(key.clone()));
    tool_cards::ensure_tool_card::<BrowserSessionCell>(chat, &mut tracker.slot, &tracker.cell);

    chat
        .tools_state
        .browser_session_by_call
        .insert(call_id.to_string(), key.clone());
    if let Some(meta) = order {
        chat
            .tools_state
            .browser_session_by_order
            .insert(BrowserSessionOrderKey::from_order_meta(meta), key.clone());
    }
    chat.tools_state.browser_last_key = Some(key.clone());

    chat
        .tools_state
        .browser_sessions
        .insert(key, tracker);

    true
}

pub(super) fn handle_custom_tool_end(
    chat: &mut ChatWidget<'_>,
    order: Option<&OrderMeta>,
    call_id: &str,
    tool_name: &str,
    params: Option<Value>,
    duration: Duration,
    result: &Result<String, String>,
) -> bool {
    if !tool_name.starts_with("browser_") || tool_name == "browser_fetch" {
        return false;
    }

    let key = chat
        .tools_state
        .browser_session_by_call
        .remove(call_id)
        .or_else(|| {
            order.and_then(|meta| {
                chat
                    .tools_state
                    .browser_session_by_order
                    .get(&BrowserSessionOrderKey::from_order_meta(meta))
                    .cloned()
            })
        })
        .unwrap_or_else(|| browser_key(order, call_id));

    let mut tracker = match chat.tools_state.browser_sessions.remove(&key) {
        Some(tracker) => tracker,
        None => return false,
    };

    let order_key = order
        .map(|meta| chat.provider_order_key_from_order_meta(meta))
        .unwrap_or(tracker.slot.order_key);

    tracker.slot.set_order_key(order_key);

    let params_to_use = params.as_ref();
    if tool_name == "browser_open" {
        if let Some(Value::Object(json)) = params_to_use {
            if let Some(url) = json.get("url").and_then(|v| v.as_str()) {
                tracker.cell.set_url(url.to_string());
            }
        }
    }

    let summary = summarize_action(tool_name, params_to_use, result);
    let timestamp = tracker.elapsed;
    tracker.cell.record_action(
        timestamp,
        duration,
        summary.action.clone(),
        summary.target.clone(),
        summary.value.clone(),
        summary.outcome.clone(),
    );
    if tool_name == "browser_console" {
        for line in console_lines_from_result(result) {
            tracker.cell.add_console_message(line);
        }
    }
    if let Some(code) = summary.status_code {
        tracker.cell.set_status_code(Some(code));
    }
    if let Some(headless) = summary.headless {
        tracker.cell.set_headless(Some(headless));
    }
    tracker.elapsed = tracker.elapsed.saturating_add(duration);

    ensure_cell_picker(chat, &tracker.cell);
    if tracker.slot.has_order_change() && !tracker.anchor_inserted {
        if let Some(previous) = tracker.slot.last_inserted_order() {
            insert_browser_anchor(chat, previous, &tracker);
            tracker.anchor_inserted = true;
        }
    }
    tool_cards::assign_tool_card_key(&mut tracker.slot, &mut tracker.cell, Some(key.clone()));
    tool_cards::replace_tool_card::<BrowserSessionCell>(chat, &mut tracker.slot, &tracker.cell);

    chat
        .tools_state
        .browser_sessions
        .insert(key, tracker);

    true
}

pub(super) fn handle_background_event(
    chat: &mut ChatWidget<'_>,
    order: Option<&OrderMeta>,
    message: &str,
) -> bool {
    if chat.tools_state.browser_sessions.is_empty() {
        return false;
    }

    let payload = match message.strip_prefix(BROWSER_CONSOLE_EVENT_PREFIX) {
        Some(rest) => rest.trim(),
        None => return false,
    };
    if payload.is_empty() {
        return false;
    }

    let key = key_from_order_or_last(chat, order);
    let Some(key) = key else { return false; };

    let mut tracker = match chat.tools_state.browser_sessions.remove(&key) {
        Some(tracker) => tracker,
        None => return false,
    };

    let order_key = order
        .map(|meta| chat.provider_order_key_from_order_meta(meta))
        .unwrap_or(tracker.slot.order_key);
    tracker.slot.set_order_key(order_key);

    let console_lines = parse_console_output(payload);
    if console_lines.is_empty() {
        chat.tools_state.browser_sessions.insert(key, tracker);
        return false;
    }
    for line in console_lines {
        tracker.cell.add_console_message(line);
    }

    ensure_cell_picker(chat, &tracker.cell);
    if tracker.slot.has_order_change() && !tracker.anchor_inserted {
        if let Some(previous) = tracker.slot.last_inserted_order() {
            insert_browser_anchor(chat, previous, &tracker);
            tracker.anchor_inserted = true;
        }
    }
    tool_cards::assign_tool_card_key(&mut tracker.slot, &mut tracker.cell, Some(key.clone()));
    tool_cards::replace_tool_card::<BrowserSessionCell>(chat, &mut tracker.slot, &tracker.cell);

    chat
        .tools_state
        .browser_sessions
        .insert(key, tracker);

    true
}

pub(super) fn handle_screenshot_update(
    chat: &mut ChatWidget<'_>,
    order: Option<&OrderMeta>,
    screenshot_path: &PathBuf,
    url: &str,
) -> BrowserScreenshotUpdateResult {
    let mut result = BrowserScreenshotUpdateResult {
        grouped: false,
        session_key: None,
    };

    if chat.tools_state.browser_sessions.is_empty() {
        return result;
    }

    let key = key_from_order_or_last(chat, order);
    let Some(key) = key else { return result; };

    let mut tracker = match chat.tools_state.browser_sessions.remove(&key) {
        Some(tracker) => tracker,
        None => return result,
    };

    let order_key = order
        .map(|meta| chat.provider_order_key_from_order_meta(meta))
        .unwrap_or(tracker.slot.order_key);
    tracker.slot.set_order_key(order_key);

    tracker.cell.set_url(url.to_string());

    let now = Instant::now();
    let base = tracker
        .first_screenshot_instant
        .get_or_insert(now);
    let mut relative = now.duration_since(*base);
    if relative <= tracker.last_screenshot_timestamp {
        let bump = tracker.last_screenshot_timestamp - relative + Duration::from_millis(1);
        relative = relative.saturating_add(bump);
    }
    tracker.last_screenshot_timestamp = relative;

    tracker
        .cell
        .record_screenshot(relative, screenshot_path.clone(), Some(url.to_string()));

    ensure_cell_picker(chat, &tracker.cell);
    if tracker.slot.has_order_change() && !tracker.anchor_inserted {
        if let Some(previous) = tracker.slot.last_inserted_order() {
            insert_browser_anchor(chat, previous, &tracker);
            tracker.anchor_inserted = true;
        }
    }
    tool_cards::assign_tool_card_key(&mut tracker.slot, &mut tracker.cell, Some(key.clone()));
    tool_cards::replace_tool_card::<BrowserSessionCell>(chat, &mut tracker.slot, &tracker.cell);

    result.session_key = Some(key.clone());

    chat
        .tools_state
        .browser_sessions
        .insert(key, tracker);

    result.grouped = true;
    result
}

fn order_key_and_ordinal(chat: &mut ChatWidget<'_>, order: Option<&OrderMeta>) -> (OrderKey, Option<u64>) {
    match order {
        Some(meta) => (chat.provider_order_key_from_order_meta(meta), Some(meta.request_ordinal)),
        None => (chat.next_internal_key(), None),
    }
}

fn browser_key(order: Option<&OrderMeta>, call_id: &str) -> String {
    if let Some(meta) = order {
        format!("req:{}:{}", meta.request_ordinal, call_id)
    } else {
        format!("call:{}", call_id)
    }
}

fn select_session_key(
    chat: &mut ChatWidget<'_>,
    order: Option<&OrderMeta>,
    call_id: &str,
    tool_name: &str,
) -> String {
    if let Some(meta) = order {
        if let Some(existing) = chat
            .tools_state
            .browser_session_by_order
            .get(&BrowserSessionOrderKey::from_order_meta(meta))
            .cloned()
        {
            if chat.tools_state.browser_sessions.contains_key(&existing) {
                return existing;
            }
        }
        if let Some(last) = chat.tools_state.browser_last_key.clone() {
            if chat.tools_state.browser_sessions.contains_key(&last) {
                return last;
            }
        }
    }

    let mut key = browser_key(order, call_id);

    if order.is_none() && tool_name != "browser_open" {
        if let Some(last) = chat.tools_state.browser_last_key.clone() {
            if chat.tools_state.browser_sessions.contains_key(&last) {
                key = last;
            }
        }
    }

    key
}

fn key_from_order_or_last(
    chat: &mut ChatWidget<'_>,
    order: Option<&OrderMeta>,
) -> Option<String> {
    if let Some(meta) = order {
        return chat
            .tools_state
            .browser_session_by_order
            .get(&BrowserSessionOrderKey::from_order_meta(meta))
            .cloned();
    }
    chat.tools_state.browser_last_key.clone()
}

fn summarize_action(
    tool_name: &str,
    params: Option<&Value>,
    result: &Result<String, String>,
) -> BrowserActionSummary {
    let mut summary = BrowserActionSummary {
        action: summarize_action_label(tool_name),
        target: None,
        value: None,
        outcome: None,
        status_code: None,
        headless: None,
    };

    let params = params.and_then(|value| value.as_object());

    match tool_name {
        "browser_open" => {
            if let Some(url) = params.and_then(|value| value.get("url")).and_then(Value::as_str) {
                summary.target = Some(url.to_string());
            }
            if let Some(headless) = params
                .and_then(|value| value.get("headless"))
                .and_then(Value::as_bool)
            {
                summary.headless = Some(headless);
            }
        }
        "browser_click" => {
            let description = params
                .and_then(|value| value.get("description"))
                .and_then(Value::as_str)
                .map(|s| s.to_string());
            let selector = params
                .and_then(|value| value.get("selector"))
                .and_then(Value::as_str)
                .map(|s| s.to_string());
            summary.target = description.clone().or_else(|| selector.clone());
            if let (Some(_), Some(sel)) = (description.as_ref(), selector.as_ref()) {
                summary.value = Some(sel.clone());
            }
            if summary.target.is_none() {
                summary.target = params
                    .and_then(|value| value.get("x"))
                    .and_then(Value::as_f64)
                    .zip(params.and_then(|value| value.get("y")).and_then(Value::as_f64))
                    .map(|(x, y)| format!("({:.0}, {:.0})", x, y));
            }
        }
        "browser_scroll" => {
            let dx = params
                .and_then(|value| value.get("dx"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let dy = params
                .and_then(|value| value.get("dy"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let label = if dx == 0 {
                format!("dy={}", dy)
            } else {
                format!("dx={} dy={}", dx, dy)
            };
            if !(dx == 0 && dy == 0) {
                summary.value = Some(label);
            }
        }
        "browser_type" => {
            if let Some(text) = params
                .and_then(|value| value.get("text"))
                .and_then(Value::as_str)
            {
                summary.value = Some(truncate(text, 48));
            }
            if let Some(selector) = params
                .and_then(|value| value.get("selector"))
                .and_then(Value::as_str)
            {
                summary.target = Some(selector.to_string());
            }
        }
        "browser_key" => {
            if let Some(key) = params
                .and_then(|value| value.get("key"))
                .and_then(Value::as_str)
            {
                summary.value = Some(key.to_string());
            }
        }
        "browser_history" => {
            if let Some(direction) = params
                .and_then(|value| value.get("direction"))
                .and_then(Value::as_str)
            {
                summary.value = Some(direction.to_string());
            }
        }
        "browser_move" => {
            let absolute = params
                .and_then(|value| value.get("x"))
                .and_then(Value::as_f64)
                .zip(params.and_then(|value| value.get("y")).and_then(Value::as_f64))
                .map(|(x, y)| format!("to ({:.0}, {:.0})", x, y));
            let relative = params
                .and_then(|value| value.get("dx"))
                .and_then(Value::as_f64)
                .zip(params.and_then(|value| value.get("dy")).and_then(Value::as_f64))
                .map(|(dx, dy)| format!("by ({:.0}, {:.0})", dx, dy));
            summary.value = absolute.or(relative);
        }
        "browser_console" => {
            if let Some(lines) = params
                .and_then(|value| value.get("lines"))
                .and_then(Value::as_u64)
            {
                summary.value = Some(format!("last {}", lines));
            }
        }
        "browser_javascript" => {
            if let Some(code) = params
                .and_then(|value| value.get("code"))
                .and_then(Value::as_str)
            {
                summary.value = Some(truncate(code, 48));
            }
        }
        "browser_cdp" => {
            if let Some(method) = params
                .and_then(|value| value.get("method"))
                .and_then(Value::as_str)
            {
                summary.target = Some(method.to_string());
            }
        }
        "browser_inspect" => {
            summary.target = params
                .and_then(|value| value.get("selector"))
                .and_then(Value::as_str)
                .map(|s| s.to_string());
        }
        "browser_close" => {
            summary.action = "Close".to_string();
        }
        _ => {
            summary.target = params
                .and_then(|value| value.get("target"))
                .and_then(Value::as_str)
                .map(|s| s.to_string());
            summary.value = params
                .and_then(|value| value.get("value"))
                .and_then(Value::as_str)
                .map(|s| s.to_string());
        }
    }

    let (outcome, status_code) = summarize_action_result(result);
    summary.outcome = outcome;
    summary.status_code = status_code;

    summary
}

fn summarize_action_label(tool_name: &str) -> String {
    match tool_name {
        "browser_open" => "Nav".to_string(),
        "browser_click" => "Click".to_string(),
        "browser_scroll" => "Scroll".to_string(),
        "browser_type" => "Type".to_string(),
        "browser_key" => "Key".to_string(),
        "browser_move" => "Move".to_string(),
        "browser_history" => "History".to_string(),
        "browser_console" => "Console".to_string(),
        "browser_javascript" => "Script".to_string(),
        "browser_cdp" => "CDP".to_string(),
        "browser_status" => "Status".to_string(),
        "browser_inspect" => "Inspect".to_string(),
        "browser_cleanup" => "Cleanup".to_string(),
        "browser_close" => "Close".to_string(),
        other => other
            .trim_start_matches("browser_")
            .split('_')
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn summarize_action_result(result: &Result<String, String>) -> (Option<String>, Option<String>) {
    match result {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return (None, None);
            }

            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(trimmed) {
                let status = map
                    .get("status")
                    .and_then(Value::as_str)
                    .map(|s| s.to_string());
                let message = map
                    .get("message")
                    .and_then(Value::as_str)
                    .map(|s| s.to_string());
                let summary_text = status.or(message).unwrap_or_else(|| truncate(trimmed, 64));

                let status_code = map
                    .get("status_code")
                    .and_then(|value| match value {
                        Value::Number(num) => num.as_u64().map(|n| n.to_string()),
                        Value::String(s) => Some(s.to_string()),
                        _ => None,
                    });

                return (Some(summary_text), status_code);
            }

            (Some(truncate(trimmed, 64)), extract_leading_status_code(trimmed))
        }
        Err(err) => {
            let text = format!("error: {}", truncate(err, 48));
            (Some(text), None)
        }
    }
}

fn extract_leading_status_code(text: &str) -> Option<String> {
    let digits: String = text
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.len() == 3 {
        Some(digits)
    } else {
        None
    }
}

fn console_lines_from_result(result: &Result<String, String>) -> Vec<String> {
    match result {
        Ok(output) => parse_console_output(output),
        Err(err) => {
            let trimmed = err.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![format_warning_line(trimmed)]
            }
        }
    }
}

fn parse_console_output(output: &str) -> Vec<String> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(trimmed) {
        return format_console_entries(&entries);
    }

    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(trimmed) {
        if let Some(Value::Array(entries)) = map.get("logs") {
            return format_console_entries(entries);
        }
    }

    let mut lines = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.eq_ignore_ascii_case("console logs:") {
            continue;
        }
        let normalized = normalize_console_line(line);
        if !normalized.is_empty() {
            lines.push(normalized);
        }
    }
    lines
}

fn format_console_entries(entries: &[Value]) -> Vec<String> {
    let mut lines = Vec::new();
    for entry in entries {
        let line = match entry {
            Value::Object(map) => format_console_entry(map),
            Value::String(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        };
        if let Some(line) = line {
            let normalized = normalize_console_line(line.as_str());
            if !normalized.is_empty() {
                lines.push(normalized);
            }
        }
    }
    lines
}

fn format_console_entry(entry: &serde_json::Map<String, Value>) -> Option<String> {
    let message = entry
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if message.is_empty() {
        return None;
    }

    let level = entry
        .get("level")
        .and_then(Value::as_str)
        .unwrap_or("log")
        .to_uppercase();

    let timestamp = entry
        .get("timestamp")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .or_else(|| {
            entry.get("ts_unix_ms").and_then(|value| match value {
                Value::Number(num) => Some(num.to_string()),
                Value::String(s) => Some(s.clone()),
                _ => None,
            })
        })
        .unwrap_or_default();

    if timestamp.is_empty() {
        Some(format!("[{level}] {message}"))
    } else {
        Some(format!("[{timestamp}] [{level}] {message}"))
    }
}

fn normalize_console_line(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let upper = trimmed.to_ascii_uppercase();
    let is_warning = upper.contains("[WARN]")
        || upper.contains("[WARNING]")
        || upper.contains("[ERROR]")
        || upper.contains("[EXCEPTION]")
        || upper.contains("[UNHANDLEDREJECTION]");

    if is_warning {
        format_warning_line(trimmed)
    } else {
        trimmed.to_string()
    }
}

fn format_warning_line(line: &str) -> String {
    if line.contains('⚠') {
        line.to_string()
    } else {
        format!("⚠️ {}", line)
    }
}

fn truncate(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        input.to_string()
    } else {
        let truncated: String = input.chars().take(max).collect();
        format!("{}…", truncated)
    }
}
