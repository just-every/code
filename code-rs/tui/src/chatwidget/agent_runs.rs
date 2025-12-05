use super::{tool_cards, ChatWidget, OrderKey};
use super::tool_cards::ToolCardSlot;
use crate::history_cell::{
    AgentDetail,
    AgentRunCell,
    AgentStatusKind,
    AgentStatusPreview,
    PlainHistoryCell,
    StepProgress,
    HistoryCellType,
    plain_message_state_from_lines,
    plain_message_state_from_paragraphs,
};
use crate::history::state::{PlainMessageKind, PlainMessageRole};
use code_core::protocol::{AgentInfo, AgentSourceKind, AgentStatusUpdateEvent, OrderMeta};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use tracing::error;

const AGENT_TOOL_NAMES: &[&str] = &[
    "agent",
    "agent_run",
    "agent_result",
    "agent_wait",
    "agent_cancel",
    "agent_check",
    "agent_list",
];

// Memory guards for agent run outputs. These mirror the exec-stream caps to
// keep long Auto Drive sessions from retaining unbounded sub-agent chatter.
const AGENT_DETAIL_MAX_BYTES: usize = 512 * 1024; // total per-agent detail payload
const AGENT_DETAIL_MAX_LINES: usize = 200; // total detail entries per agent
const AGENT_DETAIL_LINE_MAX_BYTES: usize = 4 * 1024; // per-line tail we keep
const MAX_AGENT_RUNS: usize = 48; // global limit on tracked agent runs

pub(super) fn is_agent_tool(tool_name: &str) -> bool {
    AGENT_TOOL_NAMES
        .iter()
        .copied()
        .any(|name| name.eq_ignore_ascii_case(tool_name))
}

fn is_primary_run_tool(tool_name: &str) -> bool {
    matches!(tool_name, "agent" | "agent_run")
}

fn format_elapsed_short(duration: Duration) -> String {
    let secs = duration.as_secs();
    let minutes = secs / 60;
    let seconds = secs % 60;
    if minutes > 0 {
        format!("{}m{:02}s", minutes, seconds)
    } else {
        format!("{:02}s", seconds)
    }
}

fn begin_action_for(tool_name: &str, metadata: &InvocationMetadata) -> Option<String> {
    let label = metadata
        .label
        .clone()
        .or_else(|| metadata.agent_ids.clone().into_iter().next())
        .unwrap_or_else(|| "agent".to_string());
    let action = metadata.action.as_deref().unwrap_or_else(|| match tool_name {
        "agent" | "agent_run" => "create",
        "agent_wait" => "wait",
        "agent_result" => "result",
        "agent_cancel" => "cancel",
        "agent_check" => "status",
        "agent_list" => "list",
        other => other,
    });

    match action {
        "create" => Some(format!("Started agent run for {}", label)),
        "wait" => Some("Waiting for agents".to_string()),
        "result" => Some(format!("Requested results for {}", label)),
        "cancel" => Some(format!("Cancelling agent batch for {}", label)),
        "status" => Some(format!("Checking agent status for {}", label)),
        "list" => Some("Listing available agents".to_string()),
        _ => None,
    }
}

fn end_action_for(
    tool_name: &str,
    metadata: &InvocationMetadata,
    duration: Duration,
    success: bool,
    message: Option<&str>,
) -> Option<String> {
    let elapsed = format_elapsed_short(duration);
    let action = metadata.action.as_deref().unwrap_or_else(|| match tool_name {
        "agent" | "agent_run" => "create",
        "agent_wait" => "wait",
        "agent_result" => "result",
        "agent_cancel" => "cancel",
        "agent_check" => "status",
        "agent_list" => "list",
        other => other,
    });

    match action {
        "create" => {
            if success {
                None
            } else {
                let detail = message.unwrap_or("unknown error");
                Some(format!("Agent run failed in {} — {}", elapsed, detail))
            }
        }
        "wait" => {
            if success {
                None
            } else {
                let detail = message.unwrap_or("wait failed");
                Some(format!("Wait failed in {} — {}", elapsed, detail))
            }
        }
        "result" => {
            if success {
                None
            } else {
                let detail = message.unwrap_or("result error");
                Some(format!("Result fetch failed in {} — {}", elapsed, detail))
            }
        }
        "cancel" => Some(format!("Cancel request completed in {}", elapsed)),
        "status" => Some(format!("Status check finished in {}", elapsed)),
        "list" => Some("Listed agents".to_string()),
        _ => None,
    }
}

fn friendly_agent_names(metadata: &InvocationMetadata, tracker: &AgentRunTracker) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for id in &metadata.agent_ids {
        if let Some(name) = tracker.cell.agent_name_for_id(id) {
            names.push(name);
        } else if !looks_like_uuid(id.as_str()) {
            names.push(id.clone());
        }
    }
    if names.is_empty() {
        if let Some(label) = metadata
            .label
            .as_ref()
            .and_then(|value| clean_label(value))
            .filter(|value| !looks_like_uuid(value))
        {
            names.push(label);
        }
    }
    names.sort_unstable();
    names.dedup();
    names
}

fn normalize_begin_action_text(
    tracker: &AgentRunTracker,
    metadata: &InvocationMetadata,
    action: String,
) -> Option<String> {
    if action.is_empty() {
        return None;
    }

    if action.starts_with("Waiting for agents") {
        return Some("Waiting for agents".to_string());
    }

    if action.starts_with("Requested results for ") {
        let names = friendly_agent_names(metadata, tracker);
        if names.is_empty() {
            return Some("Requested results".to_string());
        }
        return Some(format!("Requested results for {}", names.join(", ")));
    }

    if action.starts_with("Cancelling agent batch for ") {
        let names = friendly_agent_names(metadata, tracker);
        if names.is_empty() {
            return Some("Cancelling agent batch".to_string());
        }
        return Some(format!("Cancelling agents {}", names.join(", ")));
    }

    if action.starts_with("Checking agent status for ") {
        let names = friendly_agent_names(metadata, tracker);
        if names.is_empty() {
            return Some("Checking agent status".to_string());
        }
        return Some(format!("Checking agent status for {}", names.join(", ")));
    }

    if action.starts_with("Started agent run for ") {
        let names = friendly_agent_names(metadata, tracker);
        if names.is_empty() {
            if let Some(label) = metadata
                .label
                .as_ref()
                .and_then(|value| clean_label(value))
                .filter(|value| !looks_like_uuid(value))
            {
                return Some(format!("Started agent run for {}", label));
            }
            return Some(action);
        }
        return Some(format!("Started agent run for {}", names.join(", ")));
    }

    Some(action)
}

fn normalize_end_action_text(
    tracker: &AgentRunTracker,
    metadata: &InvocationMetadata,
    action: String,
) -> Option<String> {
    if action.is_empty() {
        return None;
    }

    if action.starts_with("Agent run failed") {
        let names = friendly_agent_names(metadata, tracker);
        if names.is_empty() {
            return Some(action);
        }
        return Some(format!("{} — {}", names.join(", "), action));
    }

    if action.starts_with("Wait failed") {
        return Some(action);
    }

    if action.starts_with("Result fetch failed") {
        let names = friendly_agent_names(metadata, tracker);
        if names.is_empty() {
            return Some(action);
        }
        let detail = action.trim_start_matches("Result fetch failed in ");
        return Some(format!("Result fetch failed for {} in {}", names.join(", "), detail));
    }

    Some(action)
}

pub(super) struct AgentRunTracker {
    pub slot: ToolCardSlot,
    pub cell: AgentRunCell,
    pub batch_id: Option<String>,
    pub batch_label: Option<String>,
    agent_ids: HashSet<String>,
    models: HashSet<String>,
    task: Option<String>,
    context: Option<String>,
    has_custom_name: bool,
    call_ids: HashSet<String>,
    agent_started_at: HashMap<String, Instant>,
    agent_elapsed: HashMap<String, Duration>,
    agent_token_counts: HashMap<String, u64>,
    agent_announced_status: HashMap<String, AgentStatusKind>,
    anchor_inserted: bool,
    write_enabled: Option<bool>,
}

impl AgentRunTracker {
    pub fn new(order_key: OrderKey) -> Self {
        Self {
            slot: ToolCardSlot::new(order_key),
            cell: AgentRunCell::new("(pending)".to_string()),
            batch_id: None,
            batch_label: None,
            agent_ids: HashSet::new(),
            models: HashSet::new(),
            task: None,
            context: None,
            has_custom_name: false,
            call_ids: HashSet::new(),
            agent_started_at: HashMap::new(),
            agent_elapsed: HashMap::new(),
            agent_token_counts: HashMap::new(),
            agent_announced_status: HashMap::new(),
            anchor_inserted: false,
            write_enabled: None,
        }
    }

    fn merge_agent_ids<I>(&mut self, ids: I)
    where
        I: IntoIterator<Item = String>,
    {
        for id in ids {
            self.agent_ids.insert(id);
        }
    }

    fn merge_models<I>(&mut self, models: I)
    where
        I: IntoIterator<Item = String>,
    {
        for model in models {
            let trimmed = model.trim();
            if trimmed.is_empty() {
                continue;
            }
            self.models.insert(trimmed.to_string());
        }
    }

    fn effective_label(&self) -> Option<String> {
        if let Some(label) = self.batch_label.as_ref() {
            let trimmed = label.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        self.cell.display_title()
    }

    fn set_task(&mut self, task: Option<String>) {
        if let Some(value) = task {
            self.task = Some(value);
        }
        self.cell.set_task(self.task.clone());
    }

    fn set_context(&mut self, context: Option<String>) {
        if let Some(value) = context {
            self.context = Some(value);
        }
        self.cell.set_context(self.context.clone());
    }

    fn set_agent_name(&mut self, name: Option<String>, override_existing: bool) {
        if let Some(name) = name {
            if override_existing || !self.has_custom_name {
                self.cell.set_agent_name(name);
                self.has_custom_name = true;
            }
        }
    }

    fn set_write_mode(&mut self, write_flag: Option<bool>) {
        if let Some(flag) = write_flag {
            self.write_enabled = Some(flag);
            self.cell.set_write_mode(Some(flag));
        }
    }

    pub(crate) fn overlay_display_label(&self) -> Option<String> {
        self
            .effective_label()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| self.batch_id.as_ref().map(|value| value.to_string()))
    }

    pub(crate) fn overlay_task(&self) -> Option<String> {
        self.task.as_ref().and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }

    pub(crate) fn overlay_context(&self) -> Option<String> {
        self.context.as_ref().and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }
}

fn agent_batch_key(batch_id: &str) -> String {
    format!("batch:{}", batch_id)
}

fn short_batch_id(batch_id: &str) -> String {
    let trimmed: String = batch_id.chars().filter(|c| *c != '-').collect();
    if trimmed.len() <= 8 {
        if trimmed.is_empty() {
            batch_id.to_string()
        } else {
            trimmed
        }
    } else {
        trimmed[..8].to_string()
    }
}

fn looks_like_uuid(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    parts.iter().all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_hexdigit()))
}

fn clean_label(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn insert_agent_start_message(chat: &mut ChatWidget<'_>, order_key: OrderKey, tracker: &AgentRunTracker) {
    let line = agent_start_line(tracker);
    let state = plain_message_state_from_lines(vec![line], HistoryCellType::BackgroundEvent);
    let cell = PlainHistoryCell::from_state(state);
    let _ = chat.history_insert_with_key_global_tagged(Box::new(cell), order_key, "background", None);
}

fn agent_start_line(tracker: &AgentRunTracker) -> Line<'static> {
    let title = tracker
        .effective_label()
        .or_else(|| tracker.batch_id.as_ref().map(|id| short_batch_id(id)))
        .unwrap_or_else(|| "agent batch".to_string());

    let mut agents: Vec<String> = tracker.models.iter().cloned().collect();
    if agents.is_empty() {
        agents = tracker
            .agent_ids
            .iter()
            .filter_map(|id| {
                let trimmed = id.trim();
                if trimmed.is_empty() {
                    None
                } else if looks_like_uuid(trimmed) {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect();
    }
    agents.sort_unstable();
    agents.dedup();

    let agent_segment = if agents.is_empty() {
        None
    } else {
        Some(format!(" with agents {}", agents.join(", ")))
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw("Started "));
    spans.push(Span::styled(title, Style::new().bold()));
    if let Some(segment) = agent_segment {
        spans.push(Span::raw(segment));
    }

    Line::from(spans)
}

fn report_missing_batch(
    chat: &mut ChatWidget<'_>,
    context: &str,
    call_id: Option<&str>,
    tool_name: Option<&str>,
    extra: Option<&str>,
) {
    error!(
        %context,
        call_id,
        tool_name,
        extra,
        "missing batch_id for agent event"
    );

    let mut message = format!("⚠️ {context}: missing agent batch_id.");
    if let Some(tool) = tool_name {
        if !tool.is_empty() {
            message.push_str(&format!(" tool={tool}"));
        }
    }
    if let Some(cid) = call_id {
        if !cid.is_empty() {
            message.push_str(&format!(" call_id={cid}"));
        }
    }
    if let Some(detail) = extra {
        if !detail.is_empty() {
            message.push_str(&format!(" {detail}"));
        }
    }

    let state = plain_message_state_from_paragraphs(
        PlainMessageKind::Error,
        PlainMessageRole::Error,
        [message],
    );
    let key = chat.next_internal_key();
    let cell = PlainHistoryCell::from_state(state);
    let _ = chat.history_insert_with_key_global(Box::new(cell), key);
}

#[derive(Default)]
struct InvocationMetadata {
    batch_id: Option<String>,
    agent_ids: Vec<String>,
    models: Vec<String>,
    task: Option<String>,
    plan: Vec<String>,
    label: Option<String>,
    action: Option<String>,
    context: Option<String>,
    write: Option<bool>,
    read_only: Option<bool>,
}

impl InvocationMetadata {
    fn from(tool_name: &str, params: Option<&Value>) -> Self {
        let mut meta = InvocationMetadata::default();
        if let Some(Value::Object(map)) = params {
            if let Some(action) = map.get("action").and_then(|v| v.as_str()) {
                meta.action = Some(action.to_string());
            }
            if let Some(batch) = map.get("batch_id").and_then(|v| v.as_str()) {
                meta.batch_id = Some(batch.to_string());
            }
            if let Some(write_flag) = map.get("write").and_then(|v| v.as_bool()) {
                meta.write = Some(write_flag);
            }
            if let Some(ro_flag) = map.get("read_only").and_then(|v| v.as_bool()) {
                meta.read_only = Some(ro_flag);
            }
            if let Some(agent_id) = map.get("agent_id").and_then(|v| v.as_str()) {
                meta.agent_ids.push(agent_id.to_string());
            }
            if let Some(agent_name) = map.get("agent_name").and_then(|v| v.as_str()) {
                meta.label = Some(agent_name.to_string());
            }
            if let Some(task) = map.get("task").and_then(|v| v.as_str()) {
                meta.task = Some(task.to_string());
            }
            if let Some(context) = map.get("context").and_then(|v| v.as_str()) {
                meta.context = Some(context.to_string());
            }
            if let Some(plan) = map.get("plan").and_then(|v| v.as_array()) {
                meta.plan = plan
                    .iter()
                    .filter_map(|step| step.as_str().map(|s| s.to_string()))
                    .collect();
            }
            if let Some(models) = map.get("models").and_then(|v| v.as_array()) {
                for model in models {
                    if let Some(name) = model.as_str() {
                        meta.models.push(name.to_string());
                    }
                }
            }
            if let Some(agents) = map.get("agents").and_then(|v| v.as_array()) {
                for entry in agents {
                    if let Some(obj) = entry.as_object() {
                        if let Some(id) = obj.get("id").and_then(|v| v.as_str()) {
                            meta.agent_ids.push(id.to_string());
                        }
                        if meta.label.is_none() {
                            if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                                meta.label = Some(name.to_string());
                            }
                        }
                        if let Some(model) = obj.get("model").and_then(|v| v.as_str()) {
                            meta.models.push(model.to_string());
                        }
                        if let Some(backend) = obj.get("backend").and_then(|v| v.as_str()) {
                            meta.models.push(backend.to_string());
                        }
                        if meta.write.is_none() {
                            if let Some(write_flag) = obj.get("write").and_then(|v| v.as_bool()) {
                                meta.write = Some(write_flag);
                            }
                        }
                        if meta.read_only.is_none() {
                            if let Some(ro_flag) = obj.get("read_only").and_then(|v| v.as_bool()) {
                                meta.read_only = Some(ro_flag);
                            }
                        }
                    }
                }
            }
            if let Some(create) = map.get("create").and_then(|v| v.as_object()) {
                if meta.task.is_none() {
                    if let Some(task) = create.get("task").and_then(|v| v.as_str()) {
                        meta.task = Some(task.to_string());
                    }
                }
                if let Some(name) = create.get("name").and_then(|v| v.as_str()) {
                    meta.label = Some(name.to_string());
                }
                if meta.context.is_none() {
                    if let Some(context) = create.get("context").and_then(|v| v.as_str()) {
                        meta.context = Some(context.to_string());
                    }
                }
                if meta.write.is_none() {
                    if let Some(write_flag) = create.get("write").and_then(|v| v.as_bool()) {
                        meta.write = Some(write_flag);
                    }
                }
                if meta.read_only.is_none() {
                    if let Some(ro_flag) = create.get("read_only").and_then(|v| v.as_bool()) {
                        meta.read_only = Some(ro_flag);
                    }
                }
                if meta.plan.is_empty() {
                    if let Some(plan) = create.get("plan").and_then(|v| v.as_array()) {
                        meta.plan = plan
                            .iter()
                            .filter_map(|step| step.as_str().map(|s| s.to_string()))
                            .collect();
                    }
                }
            }
            if let Some(wait) = map.get("wait").and_then(|v| v.as_object()) {
                if meta.batch_id.is_none() {
                    if let Some(batch) = wait.get("batch_id").and_then(|v| v.as_str()) {
                        meta.batch_id = Some(batch.to_string());
                    }
                }
                if let Some(agent_id) = wait.get("agent_id").and_then(|v| v.as_str()) {
                    meta.agent_ids.push(agent_id.to_string());
                }
            }
            if let Some(status) = map.get("status").and_then(|v| v.as_object()) {
                if meta.batch_id.is_none() {
                    if let Some(batch) = status.get("batch_id").and_then(|v| v.as_str()) {
                        meta.batch_id = Some(batch.to_string());
                    }
                }
                if let Some(agent_id) = status.get("agent_id").and_then(|v| v.as_str()) {
                    meta.agent_ids.push(agent_id.to_string());
                }
            }
            if let Some(result) = map.get("result").and_then(|v| v.as_object()) {
                if meta.batch_id.is_none() {
                    if let Some(batch) = result.get("batch_id").and_then(|v| v.as_str()) {
                        meta.batch_id = Some(batch.to_string());
                    }
                }
                if let Some(agent_id) = result.get("agent_id").and_then(|v| v.as_str()) {
                    meta.agent_ids.push(agent_id.to_string());
                }
            }
            if let Some(cancel) = map.get("cancel").and_then(|v| v.as_object()) {
                if meta.batch_id.is_none() {
                    if let Some(batch) = cancel.get("batch_id").and_then(|v| v.as_str()) {
                        meta.batch_id = Some(batch.to_string());
                    }
                }
                if let Some(agent_id) = cancel.get("agent_id").and_then(|v| v.as_str()) {
                    meta.agent_ids.push(agent_id.to_string());
                }
            }
            if let Some(list) = map.get("list").and_then(|v| v.as_object()) {
                if meta.batch_id.is_none() {
                    if let Some(batch) = list.get("batch_id").and_then(|v| v.as_str()) {
                        meta.batch_id = Some(batch.to_string());
                    }
                }
            }
        }
        if meta.label.is_none() {
            if let Some(first) = meta.agent_ids.first() {
                meta.label = Some(first.clone());
            }
        }
        meta.agent_ids = dedup(meta.agent_ids);
        meta.models = dedup(meta.models);
        if meta.plan.is_empty() && is_primary_run_tool(tool_name) {
            // Leave plan empty; the UI will render a placeholder.
        }
        meta
    }

    fn resolved_write_flag(&self) -> Option<bool> {
        if let Some(flag) = self.write {
            Some(flag)
        } else if let Some(ro_flag) = self.read_only {
            Some(!ro_flag)
        } else {
            None
        }
    }
}

pub(super) fn handle_custom_tool_begin(
    chat: &mut ChatWidget<'_>,
    order: Option<&OrderMeta>,
    call_id: &str,
    tool_name: &str,
    params: Option<Value>,
) -> bool {
    if !is_agent_tool(tool_name) {
        return false;
    }

    let metadata = InvocationMetadata::from(tool_name, params.as_ref());
    let action_requires_batch = matches!(
        metadata.action.as_deref(),
        Some("create") | Some("wait") | Some("result") | Some("cancel")
    );
    if metadata.batch_id.is_none() {
        if action_requires_batch {
            report_missing_batch(
                chat,
                "custom_tool_begin",
                Some(call_id),
                Some(tool_name),
                metadata.action.as_deref(),
            );
        }
        return true;
    }

    let (order_key, ordinal) = order_key_and_ordinal(chat, order);
    let batch = metadata.batch_id.as_ref().expect("batch id required");

    let (mut tracker, resolved_key) = match chat
        .tools_state
        .agent_run_by_batch
        .get(batch)
        .cloned()
        .and_then(|key| chat.tools_state.agent_runs.remove(&key))
    {
        Some(existing) => (existing, agent_batch_key(batch)),
        None => (AgentRunTracker::new(order_key), agent_batch_key(batch)),
    };
    tracker.slot.set_order_key(order_key);

    tracker.batch_id.get_or_insert(batch.clone());

    let raw_label = metadata.label.as_ref().map(|value| value.to_string());
    let clean_label_value = raw_label.as_ref().and_then(|value| clean_label(value));
    if let Some(ref cleaned) = clean_label_value {
        if !looks_like_uuid(cleaned) {
            tracker.batch_label = Some(cleaned.clone());
        }
    }

    tracker.merge_agent_ids(metadata.agent_ids.clone());
    tracker.merge_models(metadata.models.clone());

    let name_for_cell = clean_label_value
        .clone()
        .or(raw_label.clone())
        .filter(|value| !looks_like_uuid(value));
    tracker.set_agent_name(name_for_cell, true);

    if matches!(metadata.action.as_deref(), Some("create")) && !tracker.anchor_inserted {
        insert_agent_start_message(chat, tracker.slot.order_key, &tracker);
        tracker.anchor_inserted = true;
    }

    let header_label = tracker
        .effective_label()
        .or_else(|| tracker.batch_id.clone());
    tracker.cell.set_batch_label(header_label);
    tracker.set_write_mode(metadata.resolved_write_flag());
    if !metadata.plan.is_empty() {
        tracker.cell.set_plan(metadata.plan.clone());
    }
    tracker.set_context(metadata.context.clone());
    tracker.set_task(metadata.task.clone());

    if let Some(action) = begin_action_for(tool_name, &metadata) {
        if let Some(message) = normalize_begin_action_text(&tracker, &metadata, action) {
            tracker.cell.record_action(message);
        }
    }

    let key = update_mappings(
        chat,
        resolved_key,
        order,
        Some(call_id),
        ordinal,
        tool_name,
        &mut tracker,
    );
    tool_cards::assign_tool_card_key(&mut tracker.slot, &mut tracker.cell, Some(key.clone()));
    let header_label = tracker
        .batch_label
        .as_ref()
        .map(|value| value.clone())
        .or_else(|| tracker.batch_id.clone());
    tracker.cell.set_batch_label(header_label);
    tool_cards::replace_tool_card::<AgentRunCell>(chat, &mut tracker.slot, &tracker.cell);
    chat.tools_state.agent_last_key = Some(key.clone());
    chat.tools_state.agent_runs.insert(key, tracker);
    prune_agent_runs(chat);

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
    if !is_agent_tool(tool_name) {
        return false;
    }

    let metadata = InvocationMetadata::from(tool_name, params.as_ref());
    let action_requires_batch = matches!(
        metadata.action.as_deref(),
        Some("create") | Some("wait") | Some("result") | Some("cancel")
    );
    if metadata.batch_id.is_none() {
        if action_requires_batch {
            report_missing_batch(
                chat,
                "custom_tool_end",
                Some(call_id),
                Some(tool_name),
                metadata.action.as_deref(),
            );
        }
        return true;
    }

    let order_key = order
        .map(|meta| chat.provider_order_key_from_order_meta(meta))
        .unwrap_or_else(|| chat.next_internal_key());
    let ordinal = order.map(|m| m.request_ordinal);
    let batch = metadata.batch_id.as_ref().expect("batch id required");

    let (mut tracker, resolved_key) = match chat
        .tools_state
        .agent_run_by_batch
        .get(batch)
        .cloned()
        .and_then(|key| chat.tools_state.agent_runs.remove(&key))
    {
        Some(existing) => (existing, agent_batch_key(batch)),
        None => return false,
    };

    tracker.slot.set_order_key(order_key);

    tracker.batch_id.get_or_insert(batch.clone());

    let raw_label = metadata.label.as_ref().map(|value| value.to_string());
    let clean_label_value = raw_label.as_ref().and_then(|value| clean_label(value));
    if let Some(ref cleaned) = clean_label_value {
        if !looks_like_uuid(cleaned) {
            tracker.batch_label = Some(cleaned.clone());
        }
    }

    tracker.merge_agent_ids(metadata.agent_ids.clone());
    tracker.merge_models(metadata.models.clone());

    let name_for_cell = clean_label_value
        .clone()
        .or(raw_label.clone())
        .filter(|value| !looks_like_uuid(value));
    tracker.set_agent_name(name_for_cell, true);
    if tracker.write_enabled.is_none() {
        tracker.set_write_mode(metadata.resolved_write_flag());
    }
    if !metadata.plan.is_empty() {
        tracker.cell.set_plan(metadata.plan.clone());
    }
    tracker.set_context(metadata.context.clone());
    tracker.set_task(metadata.task.clone());

    tracker.cell.set_duration(Some(duration));
    match result {
        Ok(text) => {
            let lines = lines_from(text);
            if !lines.is_empty() {
                tracker.cell.set_latest_result(lines);
            }
            tracker.cell.set_status_label("Completed");
            tracker.cell.mark_completed();
            if let Some(action) = end_action_for(tool_name, &metadata, duration, true, Some(text.as_str())) {
                if let Some(message) = normalize_end_action_text(&tracker, &metadata, action) {
                    tracker.cell.record_action(message);
                }
            }
        }
        Err(err) => {
            tracker.cell.set_latest_result(vec![err.clone()]);
            tracker.cell.mark_failed();
            if let Some(action) = end_action_for(tool_name, &metadata, duration, false, Some(err.as_str())) {
                if let Some(message) = normalize_end_action_text(&tracker, &metadata, action) {
                    tracker.cell.record_action(message);
                }
            }
        }
    }

    let key = update_mappings(
        chat,
        resolved_key,
        order,
        Some(call_id),
        ordinal,
        tool_name,
        &mut tracker,
    );
    tool_cards::assign_tool_card_key(&mut tracker.slot, &mut tracker.cell, Some(key.clone()));
    tool_cards::replace_tool_card::<AgentRunCell>(chat, &mut tracker.slot, &tracker.cell);
    chat.tools_state.agent_last_key = Some(key.clone());
    chat.tools_state.agent_runs.insert(key, tracker);
    prune_agent_runs(chat);

    true
}

pub(super) fn handle_status_update(chat: &mut ChatWidget<'_>, event: &AgentStatusUpdateEvent) {
    if event.agents.is_empty() {
        return;
    }

    let filtered_agents: Vec<AgentInfo> = event
        .agents
        .iter()
        .filter(|agent| {
            !matches!(agent.source_kind, Some(AgentSourceKind::AutoReview))
                && !agent
                    .batch_id
                    .as_deref()
                    .map(|b| b.eq_ignore_ascii_case("auto-review"))
                    .unwrap_or(false)
        })
        .cloned()
        .collect();

    if filtered_agents.is_empty() {
        return;
    }

    let mut grouped: Vec<(String, Vec<AgentInfo>)> = Vec::new();
    let mut missing: Vec<String> = Vec::new();

    for agent in filtered_agents {
        if let Some(batch_id) = agent.batch_id.clone() {
            if let Some((_, bucket)) = grouped.iter_mut().find(|(id, _)| id == &batch_id) {
                bucket.push(agent);
            } else {
                grouped.push((batch_id, vec![agent]));
            }
        } else {
            missing.push(agent.id.clone());
        }
    }

    if !missing.is_empty() {
        let detail_string = format!("agents={}", missing.join(","));
        let detail = detail_string.as_str();
        report_missing_batch(
            chat,
            "status_update",
            None,
            Some("agent_status"),
            Some(detail),
        );
    }

    if grouped.is_empty() {
        return;
    }

    for (batch_id, agents) in grouped {
        process_status_update_for_batch(chat, &batch_id, &agents, event);
    }
}

fn process_status_update_for_batch(
    chat: &mut ChatWidget<'_>,
    batch_id: &str,
    agents: &[AgentInfo],
    event: &AgentStatusUpdateEvent,
) {
    let (mut tracker, resolved_key) = match chat
        .tools_state
        .agent_run_by_batch
        .get(batch_id)
        .cloned()
        .and_then(|key| {
            chat
                .tools_state
                .agent_runs
                .remove(&key)
                .map(|tracker| (tracker, key))
        })
    {
        Some((tracker, key)) => (tracker, key),
        None => {
            let order_key = chat.next_internal_key();
            tracing::warn!(batch_id, "status_update received with no existing tracker; creating placeholder");
            (AgentRunTracker::new(order_key), agent_batch_key(batch_id))
        }
    };
    let order_key = tracker
        .slot
        .last_inserted_order()
        .unwrap_or(tracker.slot.order_key);
    tracker.slot.set_order_key(order_key);
    tracker.batch_id.get_or_insert(batch_id.to_string());

    if tracker.context.is_none() {
        if let Some(context) = event.context.clone() {
            tracker.set_context(Some(context));
        }
    }

    if tracker.task.is_none() {
        if let Some(task) = event.task.clone() {
            tracker.set_task(Some(task));
        }
    }

    let mut previews: Vec<AgentStatusPreview> = Vec::new();
    let mut status_collect = StatusSummary::default();
    let mut summary_lines: Option<Vec<String>> = None;

    for agent in agents {
        tracker.agent_ids.insert(agent.id.clone());
        if let Some(agent_batch) = agent.batch_id.as_ref() {
            tracker.batch_id.get_or_insert(agent_batch.clone());
        }
        if let Some(model) = agent.model.as_ref() {
            tracker.merge_models([model.to_string()]);
        }
        if tracker
            .batch_label
            .as_ref()
            .map(|label| label.trim().is_empty())
            .unwrap_or(true)
        {
            if let Some(cleaned) = clean_label(agent.name.as_str()).filter(|name| !looks_like_uuid(name)) {
                tracker.batch_label = Some(cleaned);
            }
        }

        let phase = classify_status(
            &agent.status,
            agent.result.is_some(),
            agent.error.is_some(),
        );

        let mut details: Vec<AgentDetail> = Vec::new();

        if let Some(result) = agent.result.as_ref() {
            let mut lines = lines_from(result);
            if lines.is_empty() {
                lines.push(result.clone());
            }
            let mut collected: Vec<String> = Vec::new();
            for line in lines {
                if !line.trim().is_empty() {
                    collected.push(line.clone());
                    details.push(AgentDetail::Result(line));
                }
            }
            if !collected.is_empty() {
                summary_lines = Some(collected);
            }
        }

        if details.is_empty() {
            if let Some(error_text) = agent.error.as_ref() {
                let mut lines = lines_from(error_text);
                if lines.is_empty() {
                    lines.push(error_text.clone());
                }
                let mut collected: Vec<String> = Vec::new();
                for line in lines {
                    if !line.trim().is_empty() {
                        collected.push(line.clone());
                        details.push(AgentDetail::Error(line));
                    }
                }
                if !collected.is_empty() {
                    summary_lines = Some(collected);
                }
            }
        }

        let step_progress = agent
            .last_progress
            .as_deref()
            .and_then(parse_progress);

        if details.is_empty() {
            if let Some(progress) = agent.last_progress.as_ref() {
                let mut lines = lines_from(progress);
                if lines.is_empty() {
                    lines.push(progress.clone());
                }
                for line in lines {
                    if !line.trim().is_empty() {
                        details.push(AgentDetail::Progress(line));
                    }
                }
            }
        }

        if details.is_empty() {
            details.push(AgentDetail::Info(agent.status.clone()));
        }

        truncate_agent_details(&mut details);

        let last_update = details
            .last()
            .map(|detail| match detail {
                AgentDetail::Progress(text)
                | AgentDetail::Result(text)
                | AgentDetail::Error(text)
                | AgentDetail::Info(text) => text.clone(),
            });

        let elapsed = compute_agent_elapsed(
            &mut tracker,
            agent.id.as_str(),
            agent.elapsed_ms,
            phase,
        );
        let elapsed_updated_at = elapsed.map(|_| Instant::now());
        let token_count = resolve_agent_token_count(
            &mut tracker,
            agent.id.as_str(),
            agent.token_count,
            &details,
        );

        let preview = AgentStatusPreview {
            id: agent.id.clone(),
            name: agent.name.clone(),
            status: agent.status.clone(),
            model: agent.model.clone(),
            details,
            status_kind: phase_to_status_kind(phase),
            step_progress,
            elapsed,
            token_count,
            last_update,
            elapsed_updated_at,
        };
        previews.push(preview);

        status_collect.observe(phase);

        if let Some(clean_name) = clean_label(agent.name.as_str()).filter(|name| !looks_like_uuid(name)) {
            tracker.set_agent_name(Some(clean_name), false);
        }
    }

    tracker.cell.set_agent_overview(previews.clone());
    let header_label = tracker
        .effective_label()
        .or_else(|| tracker.batch_id.clone());
    tracker.cell.set_batch_label(header_label);
    status_collect.apply(&mut tracker.cell);

    if let Some(lines) = summary_lines {
        tracker.cell.set_latest_result(lines);
    } else {
        tracker.cell.set_latest_result(Vec::new());
    }

    let mut status_updates: Vec<String> = Vec::new();
    for preview in &previews {
        let current_kind = preview.status_kind;
        let previous_kind = tracker
            .agent_announced_status
            .get(preview.id.as_str())
            .copied();
        tracker
            .agent_announced_status
            .insert(preview.id.clone(), current_kind);

        let should_emit = matches!(
            current_kind,
            AgentStatusKind::Completed | AgentStatusKind::Failed | AgentStatusKind::Cancelled
        ) && previous_kind != Some(current_kind);

        if !should_emit {
            continue;
        }

        let label = tracker
            .cell
            .agent_name_for_id(preview.id.as_str())
            .or_else(|| {
                clean_label(preview.name.as_str()).filter(|value| !looks_like_uuid(value))
            })
            .unwrap_or_else(|| {
                let trimmed = preview.name.trim();
                if trimmed.is_empty() || looks_like_uuid(trimmed) {
                    preview.id.clone()
                } else {
                    trimmed.to_string()
                }
            });

        match current_kind {
            AgentStatusKind::Completed => status_updates.push(format!("{} completed", label)),
            AgentStatusKind::Failed => status_updates.push(format!("{} failed", label)),
            AgentStatusKind::Cancelled => status_updates.push(format!("{} cancelled", label)),
            _ => {}
        }
    }

    if !status_updates.is_empty() {
        let message = if status_updates.len() == 1 {
            status_updates.remove(0)
        } else {
            status_updates.join("; ")
        };
        tracker.cell.record_action(message);
    }

    let mut current_key = resolved_key;
    current_key = update_mappings(
        chat,
        current_key,
        None,
        None,
        None,
        "agent_status",
        &mut tracker,
    );
    tool_cards::assign_tool_card_key(
        &mut tracker.slot,
        &mut tracker.cell,
        Some(current_key.clone()),
    );
    tool_cards::replace_tool_card::<AgentRunCell>(chat, &mut tracker.slot, &tracker.cell);
    chat.tools_state.agent_last_key = Some(current_key.clone());
    chat.tools_state.agent_runs.insert(current_key, tracker);
    prune_agent_runs(chat);
}

fn order_key_and_ordinal(chat: &mut ChatWidget<'_>, order: Option<&OrderMeta>) -> (OrderKey, Option<u64>) {
    match order {
        Some(meta) => (chat.provider_order_key_from_order_meta(meta), Some(meta.request_ordinal)),
        None => (chat.next_internal_key(), None),
    }
}

fn update_mappings(
    chat: &mut ChatWidget<'_>,
    mut key: String,
    order: Option<&OrderMeta>,
    call_id: Option<&str>,
    ordinal: Option<u64>,
    tool_name: &str,
    tracker: &mut AgentRunTracker,
) -> String {
    let original_key = key.clone();

    if let Some(batch) = tracker.batch_id.as_ref() {
        let batch_key = agent_batch_key(batch);
        if batch_key != key {
            key = batch_key;
        }
    } else if is_primary_run_tool(tool_name) {
        if let Some(ord) = ordinal {
            let ord_key = format!("req:{}:agent-run", ord);
            if ord_key != key {
                key = ord_key;
            }
        }
    }

    if let Some(cid) = call_id {
        tracker.call_ids.insert(cid.to_string());
        chat
            .tools_state
            .agent_run_by_call
            .insert(cid.to_string(), key.clone());
    }
    if let Some(meta) = order {
        chat
            .tools_state
            .agent_run_by_order
            .insert(meta.request_ordinal, key.clone());
    }
    if let Some(batch) = tracker.batch_id.as_ref() {
        chat
            .tools_state
            .agent_run_by_batch
            .insert(batch.clone(), key.clone());
    }
    for agent_id in &tracker.agent_ids {
        chat
            .tools_state
            .agent_run_by_agent
            .insert(agent_id.clone(), key.clone());
    }

    if key != original_key {
        for stored in chat.tools_state.agent_run_by_order.values_mut() {
            if *stored == original_key {
                *stored = key.clone();
            }
        }
        if let Some(batch) = tracker.batch_id.as_ref() {
            if let Some(stored) = chat.tools_state.agent_run_by_batch.get_mut(batch) {
                if *stored == original_key {
                    *stored = key.clone();
                }
            }
        }
        for agent_id in &tracker.agent_ids {
            if let Some(stored) = chat.tools_state.agent_run_by_agent.get_mut(agent_id) {
                if *stored == original_key {
                    *stored = key.clone();
                }
            }
        }
        for cid in &tracker.call_ids {
            chat
                .tools_state
                .agent_run_by_call
                .insert(cid.clone(), key.clone());
        }
    }

    key
}

#[derive(Default)]
struct StatusSummary {
    any_failed: bool,
    any_cancelled: bool,
    any_running: bool,
    any_pending: bool,
    total: usize,
    completed: usize,
}

impl StatusSummary {
    fn observe(&mut self, phase: AgentPhase) {
        self.total += 1;
        match phase {
            AgentPhase::Failed => {
                self.any_failed = true;
            }
            AgentPhase::Cancelled => {
                self.any_cancelled = true;
            }
            AgentPhase::Running => {
                self.any_running = true;
            }
            AgentPhase::Pending => {
                self.any_pending = true;
            }
            AgentPhase::Completed => {
                self.completed += 1;
            }
        }
    }

    fn apply(self, cell: &mut AgentRunCell) {
        if self.any_failed {
            cell.mark_failed();
            return;
        }
        if self.any_cancelled {
            cell.set_status_label("Cancelled");
            cell.mark_completed();
            return;
        }
        if self.total > 0 && self.completed == self.total {
            cell.set_status_label("Completed");
            cell.mark_completed();
            return;
        }
        if self.any_running {
            cell.set_status_label("Running");
            return;
        }
        if self.any_pending {
            cell.set_status_label("Pending");
            return;
        }
        cell.set_status_label("Running");
    }
}

#[derive(Clone, Copy)]
enum AgentPhase {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

fn classify_status(status: &str, has_result: bool, has_error: bool) -> AgentPhase {
    if has_error {
        return AgentPhase::Failed;
    }
    if has_result {
        return AgentPhase::Completed;
    }
    let token = status
        .split_whitespace()
        .next()
        .unwrap_or(status)
        .to_ascii_lowercase();
    match token.as_str() {
        "failed" | "error" | "errored" => AgentPhase::Failed,
        "cancelled" | "canceled" => AgentPhase::Cancelled,
        "completed" | "complete" | "done" | "success" | "succeeded" => AgentPhase::Completed,
        "pending" | "queued" | "waiting" | "starting" => AgentPhase::Pending,
        _ => AgentPhase::Running,
    }
}

fn phase_to_status_kind(phase: AgentPhase) -> AgentStatusKind {
    match phase {
        AgentPhase::Completed => AgentStatusKind::Completed,
        AgentPhase::Failed => AgentStatusKind::Failed,
        AgentPhase::Cancelled => AgentStatusKind::Cancelled,
        AgentPhase::Pending => AgentStatusKind::Pending,
        AgentPhase::Running => AgentStatusKind::Running,
    }
}

fn compute_agent_elapsed(
    tracker: &mut AgentRunTracker,
    agent_id: &str,
    elapsed_ms: Option<u64>,
    phase: AgentPhase,
) -> Option<Duration> {
    if let Some(ms) = elapsed_ms {
        let duration = Duration::from_millis(ms);
        tracker
            .agent_elapsed
            .insert(agent_id.to_string(), duration);
        if matches!(phase, AgentPhase::Completed | AgentPhase::Failed | AgentPhase::Cancelled) {
            tracker.agent_started_at.remove(agent_id);
        }
        return Some(duration);
    }

    let start_entry = tracker
        .agent_started_at
        .entry(agent_id.to_string())
        .or_insert_with(Instant::now);
    let duration = start_entry.elapsed();

    let entry = tracker
        .agent_elapsed
        .entry(agent_id.to_string())
        .or_insert(duration);
    if duration > *entry {
        *entry = duration;
    }

    if matches!(phase, AgentPhase::Completed | AgentPhase::Failed | AgentPhase::Cancelled) {
        tracker.agent_started_at.remove(agent_id);
    }

    tracker.agent_elapsed.get(agent_id).copied()
}

fn resolve_agent_token_count(
    tracker: &mut AgentRunTracker,
    agent_id: &str,
    explicit: Option<u64>,
    details: &[AgentDetail],
) -> Option<u64> {
    if let Some(value) = explicit {
        tracker
            .agent_token_counts
            .insert(agent_id.to_string(), value);
        return Some(value);
    }

    let inferred = details.iter().rev().find_map(|detail| match detail {
        AgentDetail::Progress(text)
        | AgentDetail::Result(text)
        | AgentDetail::Error(text)
        | AgentDetail::Info(text) => extract_token_count_from_text(text),
    });

    if let Some(value) = inferred {
        tracker
            .agent_token_counts
            .insert(agent_id.to_string(), value);
        return Some(value);
    }

    tracker.agent_token_counts.get(agent_id).copied()
}

fn extract_token_count_from_text(text: &str) -> Option<u64> {
    let lower = text.to_ascii_lowercase();
    if !lower.contains("token") && !lower.contains("tok") {
        return None;
    }

    let mut candidate = None;
    let mut fragment = String::new();

    for ch in text.chars() {
        if ch.is_ascii_digit()
            || matches!(ch, '.' | ',' | '_' | 'k' | 'K' | 'm' | 'M')
        {
            fragment.push(ch);
        } else {
            if let Some(value) = parse_token_fragment(&fragment) {
                candidate = Some(value);
            }
            fragment.clear();
        }
    }

    if let Some(value) = parse_token_fragment(&fragment) {
        candidate = Some(value);
    }

    candidate
}

fn parse_token_fragment(fragment: &str) -> Option<u64> {
    let trimmed = fragment.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut multiplier = 1f64;
    let mut base = trimmed;
    if let Some(last) = trimmed.chars().last() {
        match last {
            'k' | 'K' => {
                multiplier = 1_000f64;
                base = trimmed[..trimmed.len().saturating_sub(1)].trim();
            }
            'm' | 'M' => {
                multiplier = 1_000_000f64;
                base = trimmed[..trimmed.len().saturating_sub(1)].trim();
            }
            _ => {}
        }
    }

    let normalized = base.replace(',', "").replace('_', "");
    if normalized.is_empty() {
        return None;
    }

    if normalized.chars().all(|c| c.is_ascii_digit()) {
        let value: u64 = normalized.parse().ok()?;
        let computed = (value as f64 * multiplier).round();
        if computed > 0.0 {
            return Some(computed as u64);
        }
        return None;
    }

    if normalized.contains('.') {
        let value: f64 = normalized.parse().ok()?;
        let computed = (value * multiplier).round();
        if computed > 0.0 {
            return Some(computed as u64);
        }
        return None;
    }

    None
}

fn parse_progress(progress: &str) -> Option<StepProgress> {
    for token in progress.split_whitespace() {
        if let Some((done, total)) = token.split_once('/') {
            let completed = done.trim().parse::<u32>().ok()?;
            let total = total.trim().parse::<u32>().ok()?;
            if total > 0 {
                return Some(StepProgress { completed: completed.min(total), total });
            }
        }
    }
    None
}

fn lines_from(input: &str) -> Vec<String> {
    input.lines().map(|line| line.to_string()).collect()
}

fn dedup(mut values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(value.clone()));
    values
}

fn order_tuple(order_key: OrderKey) -> (u64, i32, u64) {
    (order_key.req, order_key.out, order_key.seq)
}

fn prune_agent_runs(chat: &mut ChatWidget<'_>) {
    while chat.tools_state.agent_runs.len() > MAX_AGENT_RUNS {
        let oldest_key = chat
            .tools_state
            .agent_runs
            .iter()
            .min_by_key(|(_, tracker)| order_tuple(tracker.slot.order_key))
            .map(|(key, _)| key.clone());

        let Some(key) = oldest_key else { break; };
        drop_agent_run(chat, key.as_str());
    }
}

fn drop_agent_run(chat: &mut ChatWidget<'_>, key: &str) {
    if let Some(tracker) = chat.tools_state.agent_runs.remove(key) {
        chat.tools_state.agent_run_by_call.retain(|_, v| v != key);
        chat.tools_state.agent_run_by_order.retain(|_, v| v != key);
        chat.tools_state.agent_run_by_batch.retain(|_, v| v != key);
        chat.tools_state.agent_run_by_agent.retain(|_, v| v != key);
        if chat.tools_state.agent_last_key.as_deref() == Some(key) {
            chat.tools_state.agent_last_key = None;
        }

        if let Some(id) = tracker.slot.history_id {
            if let Some(idx) = chat.cell_index_for_history_id(id) {
                chat.history_remove_at(idx);
            }
        }
    }
}

fn trim_to_tail(text: &str, max_bytes: usize) -> (String, bool) {
    let bytes = text.as_bytes();
    if bytes.len() <= max_bytes {
        return (text.to_string(), false);
    }

    // Keep the tail: most recent content is usually last.
    let start = bytes.len() - max_bytes;
    let mut start_idx = start;
    // Ensure we split on a UTF-8 boundary.
    while start_idx < bytes.len() && (bytes[start_idx] & 0b1100_0000) == 0b1000_0000 {
        start_idx += 1;
    }
    let trimmed = String::from_utf8_lossy(&bytes[start_idx..]).to_string();
    (format!("…{}", trimmed), true)
}

fn detail_text_len(detail: &AgentDetail) -> usize {
    match detail {
        AgentDetail::Progress(text)
        | AgentDetail::Result(text)
        | AgentDetail::Error(text)
        | AgentDetail::Info(text) => text.len(),
    }
}

fn truncate_agent_details(details: &mut Vec<AgentDetail>) {
    // First, cap individual lines to avoid single gigantic entries.
    for detail in details.iter_mut() {
        match detail {
            AgentDetail::Progress(text)
            | AgentDetail::Result(text)
            | AgentDetail::Error(text)
            | AgentDetail::Info(text) => {
                let (trimmed, was_trimmed) = trim_to_tail(text, AGENT_DETAIL_LINE_MAX_BYTES);
                if was_trimmed {
                    *text = trimmed;
                }
            }
        }
    }

    // Then enforce total line + byte budgets, dropping oldest first.
    let mut total_bytes: usize = details.iter().map(detail_text_len).sum();

    while details.len() > AGENT_DETAIL_MAX_LINES || total_bytes > AGENT_DETAIL_MAX_BYTES {
        if details.is_empty() {
            break;
        }
        let removed = details.remove(0);
        total_bytes = total_bytes.saturating_sub(detail_text_len(&removed));
    }
}
