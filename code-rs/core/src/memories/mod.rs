mod citations;
mod storage;

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use code_protocol::models::{ContentItem, ResponseItem};
use code_protocol::protocol::RolloutItem;
use code_protocol::protocol::SessionSource;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt::Write as _;
use tokio::sync::Mutex as AsyncMutex;
use tracing::{debug, warn};
use uuid::Uuid;

use crate::Prompt;
use crate::RolloutRecorder;
use crate::codex::Session;
use crate::model_family::{derive_default_model_family, find_family_for_model};
use crate::rollout::catalog::{SessionCatalog, SessionIndexEntry};
use crate::truncate::truncate_middle;

pub(crate) use citations::strip_memory_citations;

const MEMORIES_DIR: &str = "memories";
const MEMORY_SUMMARY_FILENAME: &str = "memory_summary.md";
const MEMORY_FILENAME: &str = "MEMORY.md";
const STATE_DIR: &str = ".state";
const STAGE1_OUTPUTS_FILENAME: &str = "stage1_outputs.json";
const PHASE2_STATE_FILENAME: &str = "phase2_state.json";
const MAX_MEMORY_PROMPT_TOKENS: usize = 5_000;
const REFRESH_INTERVAL: Duration = Duration::from_secs(300);
const PHASE1_CONCURRENCY_LIMIT: usize = 4;
const DEFAULT_STAGE_ONE_MODEL: &str = "gpt-5.1-codex-mini";
const DEFAULT_STAGE_TWO_MODEL: &str = "gpt-5.3-codex";
const DEFAULT_STAGE_ONE_ROLLOUT_TOKEN_LIMIT: usize = 150_000;
const CONTEXT_WINDOW_PERCENT: u64 = 70;
const PHASE2_ENTRY_SNIPPET_BYTES: usize = 12_000;
const PHASE2_SELECTED_PAYLOAD_BYTES: usize = 80_000;
const READ_PATH_TEMPLATE: &str = include_str!("../../templates/memories/read_path.md");

const STAGE_ONE_SYSTEM_PROMPT: &str = include_str!("../../templates/memories/stage_one_system.md");
const CONSOLIDATION_SYSTEM_PROMPT: &str = include_str!("../../templates/memories/consolidation_system.md");
const MCP_TOOL_PREFIX: &str = "mcp__";

static LAST_REFRESH_AT: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
static STAGE1_OUTPUTS_LOCK: OnceLock<AsyncMutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Stage1Output {
    session_id: Uuid,
    rollout_path: PathBuf,
    cwd: PathBuf,
    git_branch: Option<String>,
    source_updated_at: DateTime<Utc>,
    generated_at: DateTime<Utc>,
    raw_memory: String,
    rollout_summary: String,
    rollout_slug: Option<String>,
    usage_count: u64,
    last_usage: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
struct Stage1OutputRef {
    session_id: Uuid,
    source_updated_at: DateTime<Utc>,
    rollout_slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
struct Phase2State {
    completion_watermark: Option<DateTime<Utc>>,
    selected: Vec<Stage1OutputRef>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StageOneModelOutput {
    raw_memory: String,
    rollout_summary: String,
    #[serde(default)]
    rollout_slug: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConsolidationOutput {
    memory_summary: String,
    memory_body: String,
}

#[derive(Debug)]
struct Phase2Selection {
    selected: Vec<Stage1Output>,
    retained_artifacts: Vec<Stage1Output>,
    removed: Vec<Stage1OutputRef>,
}

pub(crate) fn memory_root(code_home: &Path) -> PathBuf {
    code_home.join(MEMORIES_DIR)
}

fn state_dir(code_home: &Path) -> PathBuf {
    memory_root(code_home).join(STATE_DIR)
}

fn stage1_outputs_path(code_home: &Path) -> PathBuf {
    state_dir(code_home).join(STAGE1_OUTPUTS_FILENAME)
}

fn phase2_state_path(code_home: &Path) -> PathBuf {
    state_dir(code_home).join(PHASE2_STATE_FILENAME)
}

fn memory_summary_path(code_home: &Path) -> PathBuf {
    memory_root(code_home).join(MEMORY_SUMMARY_FILENAME)
}

fn memory_md_path(code_home: &Path) -> PathBuf {
    memory_root(code_home).join(MEMORY_FILENAME)
}

fn should_refresh_now() -> bool {
    let now = Instant::now();
    let mutex = LAST_REFRESH_AT.get_or_init(|| Mutex::new(None));
    let mut guard = match mutex.lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };

    let should_refresh = guard
        .as_ref()
        .is_none_or(|last| now.duration_since(*last) >= REFRESH_INTERVAL);
    if should_refresh {
        *guard = Some(now);
    }
    should_refresh
}

pub(crate) fn maybe_spawn_memory_refresh(session: Arc<Session>) {
    if !should_refresh_now() {
        return;
    }

    tokio::spawn(async move {
        if let Err(err) = refresh_memories(&session).await {
            debug!("memory refresh skipped: {err}");
        }
    });
}

fn stage1_outputs_lock() -> &'static AsyncMutex<()> {
    STAGE1_OUTPUTS_LOCK.get_or_init(|| AsyncMutex::new(()))
}

pub(crate) async fn build_memory_tool_developer_instructions(code_home: &Path) -> Option<String> {
    let summary = tokio::fs::read_to_string(memory_summary_path(code_home)).await.ok()?;
    let summary = summary.trim();
    if summary.is_empty() {
        return None;
    }

    let max_bytes = MAX_MEMORY_PROMPT_TOKENS.saturating_mul(4);
    let (truncated, _) = truncate_middle(summary, max_bytes);
    if truncated.trim().is_empty() {
        return None;
    }

    let base_path = memory_root(code_home).display().to_string();
    Some(
        READ_PATH_TEMPLATE
            .replace("{{ base_path }}", &base_path)
            .replace("{{ memory_summary }}", truncated.trim()),
    )
}

pub(crate) fn sanitize_response_item(item: ResponseItem) -> (ResponseItem, Vec<Uuid>) {
    match item {
        ResponseItem::Message {
            id,
            role,
            content,
            end_turn,
            phase,
        } if role == "assistant" => {
            let mut rollout_ids = Vec::new();
            let content = content
                .into_iter()
                .map(|content_item| match content_item {
                    ContentItem::OutputText { text } => {
                        let (stripped, citation) = strip_memory_citations(&text);
                        rollout_ids.extend(citation.rollout_ids);
                        ContentItem::OutputText { text: stripped }
                    }
                    other => other,
                })
                .collect();
            (
                ResponseItem::Message {
                    id,
                    role,
                    content,
                    end_turn,
                    phase,
                },
                rollout_ids,
            )
        }
        other => (other, Vec::new()),
    }
}

pub(crate) async fn note_memory_usage(code_home: &Path, rollout_ids: &[Uuid]) {
    if rollout_ids.is_empty() {
        return;
    }

    let rollout_ids = rollout_ids.iter().copied().collect::<HashSet<_>>();
    let _guard = stage1_outputs_lock().lock().await;
    let path = stage1_outputs_path(code_home);
    let mut outputs: Vec<Stage1Output> = load_json_file(path.clone()).await.unwrap_or_default();
    let now = Utc::now();
    let mut changed = false;
    for output in &mut outputs {
        if rollout_ids.contains(&output.session_id) {
            output.usage_count = output.usage_count.saturating_add(1);
            output.last_usage = Some(now);
            changed = true;
        }
    }

    if changed && let Err(err) = save_json_file(path, &outputs).await {
        warn!("failed to persist memory usage: {err}");
    }
}

async fn refresh_memories(session: &Arc<Session>) -> anyhow::Result<()> {
    let code_home = session.client().code_home().to_path_buf();
    let root = memory_root(&code_home);
    tokio::fs::create_dir_all(&root).await?;
    tokio::fs::create_dir_all(state_dir(&code_home)).await?;

    let catalog = tokio::task::spawn_blocking({
        let code_home = code_home.clone();
        move || SessionCatalog::load(&code_home)
    })
    .await??;

    let mut outputs = load_stage1_outputs(&code_home).await.unwrap_or_default();
    prune_stage1_outputs(
        &mut outputs,
        &catalog,
        session.client().config().memories.max_rollout_age_days,
    );
    refresh_stage1_outputs(session, &catalog, &mut outputs).await?;
    save_stage1_outputs(&code_home, &outputs).await?;

    let mut phase2_state = load_phase2_state(&code_home).await.unwrap_or_default();
    run_phase2(session, &catalog, &mut outputs, &mut phase2_state).await?;
    save_stage1_outputs(&code_home, &outputs).await?;
    save_phase2_state(&code_home, &phase2_state).await?;
    Ok(())
}

async fn refresh_stage1_outputs(
    session: &Arc<Session>,
    catalog: &SessionCatalog,
    outputs: &mut Vec<Stage1Output>,
) -> anyhow::Result<()> {
    let existing = outputs
        .iter()
        .map(|output| (output.session_id, output.source_updated_at))
        .collect::<HashMap<_, _>>();

    let entries = catalog
        .all_ordered()
        .into_iter()
        .filter(|entry| !entry.deleted && !entry.archived)
        .filter(|entry| matches!(entry.session_source, SessionSource::Cli | SessionSource::VSCode))
        .filter_map(|entry| EligibleEntry::from_catalog_entry(session.client().code_home(), entry))
        .filter(|entry| {
            let max_age = chrono::Duration::days(
                session.client().config().memories.max_rollout_age_days.max(0),
            );
            Utc::now().signed_duration_since(entry.source_updated_at) <= max_age
        })
        .filter(|entry| is_entry_idle_enough(entry.source_updated_at, session.client().config().memories.min_rollout_idle_hours))
        .take(session.client().config().memories.max_rollouts_per_startup)
        .collect::<Vec<_>>();

    let futures = entries.into_iter().filter_map(|entry| {
        let source_updated_at = entry.source_updated_at;
        if existing
            .get(&entry.session_id)
            .is_some_and(|seen| *seen == source_updated_at)
        {
            return None;
        }
        Some(summarize_entry(Arc::clone(session), entry))
    });

    let results = futures::stream::iter(futures)
        .buffer_unordered(PHASE1_CONCURRENCY_LIMIT)
        .collect::<Vec<_>>()
        .await;

    for result in results {
        match result {
            Ok(Some(output)) => upsert_stage1_output(outputs, output),
            Ok(None) => {}
            Err(err) => debug!("memory stage1 extraction failed: {err}"),
        }
    }

    Ok(())
}

async fn run_phase2(
    session: &Arc<Session>,
    catalog: &SessionCatalog,
    outputs: &mut [Stage1Output],
    phase2_state: &mut Phase2State,
) -> anyhow::Result<()> {
    let code_home = session.client().code_home();
    let selection = build_phase2_selection(
        outputs,
        session.client().config().memories.max_raw_memories_for_global,
        session.client().config().memories.max_unused_days,
        &phase2_state.selected,
    );

    storage::sync_rollout_summaries(code_home, &selection.retained_artifacts).await?;
    storage::rebuild_raw_memories_file(
        code_home,
        &selection.retained_artifacts,
        session.client().config().memories.max_raw_memories_for_global,
    )
    .await?;

    if selection.selected.is_empty() {
        remove_consolidated_artifacts(code_home).await?;
        phase2_state.selected.clear();
        phase2_state.completion_watermark = None;
        return Ok(());
    }

    let consolidated = request_phase2_consolidation(session, catalog, &selection).await?;
    tokio::fs::write(memory_summary_path(code_home), consolidated.memory_summary.trim()).await?;
    tokio::fs::write(memory_md_path(code_home), consolidated.memory_body.trim()).await?;
    phase2_state.selected = selection.selected.iter().map(Stage1OutputRef::from).collect();
    phase2_state.completion_watermark = selection
        .selected
        .iter()
        .map(|item| item.source_updated_at)
        .max();
    Ok(())
}

async fn remove_consolidated_artifacts(code_home: &Path) -> io::Result<()> {
    for path in [memory_summary_path(code_home), memory_md_path(code_home)] {
        if let Err(err) = tokio::fs::remove_file(path).await
            && err.kind() != io::ErrorKind::NotFound
        {
            return Err(err);
        }
    }
    Ok(())
}

fn build_phase2_selection(
    outputs: &[Stage1Output],
    max_raw_memories: usize,
    max_unused_days: i64,
    previous_selection: &[Stage1OutputRef],
) -> Phase2Selection {
    let mut sorted = outputs.to_vec();
    let max_unused = chrono::Duration::days(max_unused_days.max(0));
    sorted.retain(|item| {
        let anchor = item.last_usage.unwrap_or(item.generated_at);
        Utc::now().signed_duration_since(anchor) <= max_unused
    });
    sorted.sort_by(|a, b| {
        b.usage_count
            .cmp(&a.usage_count)
            .then_with(|| {
                b.last_usage
                    .unwrap_or(b.generated_at)
                    .cmp(&a.last_usage.unwrap_or(a.generated_at))
            })
            .then_with(|| b.source_updated_at.cmp(&a.source_updated_at))
    });

    let selected = sorted
        .iter()
        .take(max_raw_memories)
        .cloned()
        .collect::<Vec<_>>();
    let selected_refs = selected.iter().map(Stage1OutputRef::from).collect::<HashSet<_>>();
    let previous_refs = previous_selection.iter().cloned().collect::<HashSet<_>>();

    let mut retained_artifacts = selected.clone();
    for output in &sorted {
        let output_ref = Stage1OutputRef::from(output);
        if previous_refs.contains(&output_ref) && !selected_refs.contains(&output_ref) {
            retained_artifacts.push(output.clone());
        }
    }
    retained_artifacts.sort_by(|a, b| b.source_updated_at.cmp(&a.source_updated_at));
    retained_artifacts.dedup_by(|a, b| {
        a.session_id == b.session_id && a.source_updated_at == b.source_updated_at
    });

    let removed = previous_selection
        .iter()
        .filter(|item| !selected_refs.contains(*item))
        .cloned()
        .collect();

    Phase2Selection {
        selected,
        retained_artifacts,
        removed,
    }
}

async fn request_phase2_consolidation(
    session: &Arc<Session>,
    catalog: &SessionCatalog,
    selection: &Phase2Selection,
) -> anyhow::Result<ConsolidationOutput> {
    let model = session
        .client()
        .config()
        .memories
        .phase_2_model
        .clone()
        .unwrap_or_else(|| DEFAULT_STAGE_TWO_MODEL.to_string());
    let family = resolve_model_family(session, &model).await;
    let prompt = Prompt {
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: build_phase2_user_prompt(session.client().code_home(), catalog, selection),
            }],
            end_turn: None,
            phase: None,
        }],
        include_additional_instructions: false,
        store: false,
        base_instructions_override: Some(CONSOLIDATION_SYSTEM_PROMPT.to_string()),
        output_schema: Some(phase2_output_schema()),
        model_override: Some(model),
        model_family_override: Some(family),
        ..Prompt::default()
    };

    let text = collect_json_response(session, &prompt).await?;
    Ok(serde_json::from_str(&text)?)
}

async fn summarize_entry(
    session: Arc<Session>,
    entry: EligibleEntry,
) -> anyhow::Result<Option<Stage1Output>> {
    let history = RolloutRecorder::get_rollout_history(&entry.rollout_path).await?;
    let rollout_items = history.get_rollout_items();
    if session.client().config().memories.no_memories_if_mcp_or_web_search
        && rollout_items.iter().any(rollout_uses_mcp_or_web_search)
    {
        return Ok(None);
    }
    let rollout_contents = serialize_memory_relevant_rollout(&rollout_items)?;
    if rollout_contents.trim().is_empty() {
        return Ok(None);
    }

    let model = session
        .client()
        .config()
        .memories
        .phase_1_model
        .clone()
        .unwrap_or_else(|| DEFAULT_STAGE_ONE_MODEL.to_string());
    let family = resolve_model_family(&session, &model).await;
    let prompt = Prompt {
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: build_stage1_user_prompt(&entry, &family, &rollout_contents),
            }],
            end_turn: None,
            phase: None,
        }],
        include_additional_instructions: false,
        store: false,
        base_instructions_override: Some(STAGE_ONE_SYSTEM_PROMPT.to_string()),
        output_schema: Some(stage1_output_schema()),
        model_override: Some(model),
        model_family_override: Some(family),
        ..Prompt::default()
    };

    let text = collect_json_response(&session, &prompt).await?;
    let parsed: StageOneModelOutput = serde_json::from_str(&text)?;
    if parsed.raw_memory.trim().is_empty() || parsed.rollout_summary.trim().is_empty() {
        return Ok(None);
    }

    Ok(Some(Stage1Output {
        session_id: entry.session_id,
        rollout_path: entry.rollout_path,
        cwd: entry.cwd,
        git_branch: entry.git_branch,
        source_updated_at: entry.source_updated_at,
        generated_at: Utc::now(),
        raw_memory: parsed.raw_memory.trim().to_string(),
        rollout_summary: parsed.rollout_summary.trim().to_string(),
        rollout_slug: parsed.rollout_slug.map(|slug| slug.trim().to_string()).filter(|slug| !slug.is_empty()),
        usage_count: 0,
        last_usage: None,
    }))
}

async fn collect_json_response(session: &Session, prompt: &Prompt) -> anyhow::Result<String> {
    let mut stream = session.client().stream(prompt).await?;
    let mut result = String::new();

    while let Some(event) = stream.next().await {
        match event? {
            crate::ResponseEvent::OutputTextDelta { delta, .. } => result.push_str(&delta),
            crate::ResponseEvent::OutputItemDone { item, .. } => {
                if result.is_empty()
                    && let ResponseItem::Message { content, .. } = item
                    && let Some(text) = crate::content_items_to_text(&content)
                {
                    result.push_str(&text);
                }
            }
            crate::ResponseEvent::Completed { .. } => break,
            _ => {}
        }
    }

    Ok(result)
}

async fn resolve_model_family(session: &Session, model: &str) -> crate::model_family::ModelFamily {
    let base = find_family_for_model(model).unwrap_or_else(|| derive_default_model_family(model));
    if let Some(remote) = session.remote_models_manager() {
        remote.apply_remote_overrides(model, base).await
    } else {
        base
    }
}

fn build_stage1_user_prompt(
    entry: &EligibleEntry,
    family: &crate::model_family::ModelFamily,
    rollout_contents: &str,
) -> String {
    let rollout_token_limit = family
        .context_window
        .map(|limit| limit.saturating_mul(CONTEXT_WINDOW_PERCENT) / 100)
        .and_then(|limit| usize::try_from(limit.max(1)).ok())
        .unwrap_or(DEFAULT_STAGE_ONE_ROLLOUT_TOKEN_LIMIT);
    let (truncated_rollout_contents, _) =
        truncate_middle(rollout_contents, rollout_token_limit.saturating_mul(4));

    format!(
        "rollout_path: {}\nrollout_cwd: {}\ngit_branch: {}\nupdated_at: {}\n\nRollout:\n{}",
        entry.rollout_path.display(),
        entry.cwd.display(),
        entry.git_branch.as_deref().unwrap_or("(none)"),
        entry.source_updated_at.to_rfc3339(),
        truncated_rollout_contents,
    )
}

fn build_phase2_user_prompt(
    code_home: &Path,
    catalog: &SessionCatalog,
    selection: &Phase2Selection,
) -> String {
    let memory_root = memory_root(code_home);
    let selected_lines = selection
        .selected
        .iter()
        .map(|item| {
            let retained = selection
                .removed
                .iter()
                .all(|removed| removed.session_id != item.session_id);
            format!(
                "- [{}] session_id={}, rollout_summary_file=rollout_summaries/{}.md",
                if retained { "selected" } else { "retained" },
                item.session_id,
                storage::rollout_summary_file_stem(item),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let removed_lines = if selection.removed.is_empty() {
        "- none".to_string()
    } else {
        selection
            .removed
            .iter()
            .map(|item| {
                format!(
                    "- session_id={}, rollout_summary_file=rollout_summaries/{}.md",
                    item.session_id,
                    storage::rollout_summary_file_stem_ref(item),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let recent_sessions = catalog
        .all_ordered()
        .into_iter()
        .take(10)
        .map(|entry| {
            format!(
                "- {} | {} | {}",
                entry.last_event_at,
                entry.session_id,
                entry.last_user_snippet.as_deref().unwrap_or("(no user snippet)"),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut selected_payload = String::new();
    for item in &selection.selected {
        let (summary_snippet, _) =
            truncate_middle(item.rollout_summary.trim(), PHASE2_ENTRY_SNIPPET_BYTES);
        let (raw_memory_snippet, _) =
            truncate_middle(item.raw_memory.trim(), PHASE2_ENTRY_SNIPPET_BYTES);
        let _ = writeln!(selected_payload, "## Session {}", item.session_id);
        let _ = writeln!(selected_payload, "updated_at: {}", item.source_updated_at.to_rfc3339());
        let _ = writeln!(selected_payload, "cwd: {}", item.cwd.display());
        if let Some(branch) = item.git_branch.as_deref() {
            let _ = writeln!(selected_payload, "git_branch: {branch}");
        }
        let _ = writeln!(selected_payload, "rollout_path: {}", item.rollout_path.display());
        let _ = writeln!(selected_payload, "rollout_summary_file: {}.md", storage::rollout_summary_file_stem(item));
        let _ = writeln!(selected_payload);
        let _ = writeln!(selected_payload, "Rollout summary:");
        let _ = writeln!(selected_payload, "{summary_snippet}");
        let _ = writeln!(selected_payload);
        let _ = writeln!(selected_payload, "Raw memory:");
        let _ = writeln!(selected_payload, "{raw_memory_snippet}");
        let _ = writeln!(selected_payload);
    }
    let (selected_payload, _) =
        truncate_middle(&selected_payload, PHASE2_SELECTED_PAYLOAD_BYTES);

    let removed_payload = selection
        .removed
        .iter()
        .filter_map(|removed| {
            selection
                .retained_artifacts
                .iter()
                .find(|item| Stage1OutputRef::from(*item) == *removed)
                .map(|item| {
                    format!(
                        "- session_id={}\n  rollout_summary_file={}.md\n  rollout_summary={}\n",
                        item.session_id,
                        storage::rollout_summary_file_stem(item),
                        item.rollout_summary.trim(),
                    )
                })
        })
        .collect::<Vec<_>>()
        .join("\n");
    let (removed_payload, _) = truncate_middle(&removed_payload, PHASE2_SELECTED_PAYLOAD_BYTES / 2);

    format!(
        "memory_root: {}\n\nSelection diff:\n{}\n\nRemoved from previous selection:\n{}\n\nRecent session context:\n{}\n\nSelected memory inputs:\n{}\n\nUse the selected inputs to write a concise, searchable MEMORY.md and a compact memory_summary.md.",
        memory_root.display(),
        selected_lines,
        if removed_payload.is_empty() { removed_lines } else { removed_payload },
        recent_sessions,
        selected_payload,
    )
}

fn stage1_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "raw_memory": { "type": "string" },
            "rollout_summary": { "type": "string" },
            "rollout_slug": { "type": ["string", "null"] }
        },
        "required": ["raw_memory", "rollout_summary", "rollout_slug"],
        "additionalProperties": false
    })
}

fn phase2_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "memory_summary": { "type": "string" },
            "memory_body": { "type": "string" }
        },
        "required": ["memory_summary", "memory_body"],
        "additionalProperties": false
    })
}

fn serialize_memory_relevant_rollout(items: &[code_protocol::protocol::RolloutItem]) -> anyhow::Result<String> {
    let mut out = Vec::new();
    for item in items {
        if crate::rollout::policy::should_persist_response_item_for_memories(item) {
            out.push(serde_json::to_string(item)?);
        }
    }
    Ok(out.join("\n"))
}

fn rollout_uses_mcp_or_web_search(item: &RolloutItem) -> bool {
    match item {
        RolloutItem::ResponseItem(ResponseItem::WebSearchCall { .. }) => true,
        RolloutItem::ResponseItem(ResponseItem::FunctionCall { name, .. }) => {
            name.starts_with(MCP_TOOL_PREFIX)
        }
        RolloutItem::Event(event) => crate::protocol::event_msg_from_protocol(&event.msg)
            .is_some_and(|event| event_uses_mcp_or_web_search(&event)),
        RolloutItem::EventMsg(msg) => crate::protocol::event_msg_from_protocol(msg)
            .is_some_and(|event| event_uses_mcp_or_web_search(&event)),
        _ => false,
    }
}

fn event_uses_mcp_or_web_search(event: &crate::protocol::EventMsg) -> bool {
    matches!(
        event,
        crate::protocol::EventMsg::McpToolCallBegin(_)
            | crate::protocol::EventMsg::McpToolCallEnd(_)
            | crate::protocol::EventMsg::WebSearchBegin(_)
            | crate::protocol::EventMsg::WebSearchComplete(_)
    )
}

fn is_entry_idle_enough(updated_at: DateTime<Utc>, min_idle_hours: i64) -> bool {
    let threshold = chrono::Duration::hours(min_idle_hours.max(0));
    Utc::now().signed_duration_since(updated_at) >= threshold
}

fn prune_stage1_outputs(
    outputs: &mut Vec<Stage1Output>,
    catalog: &SessionCatalog,
    max_rollout_age_days: i64,
) {
    let max_age = chrono::Duration::days(max_rollout_age_days.max(0));
    let allowed_sessions = catalog
        .all_ordered()
        .into_iter()
        .filter(|entry| !entry.deleted && !entry.archived)
        .filter(|entry| matches!(entry.session_source, SessionSource::Cli | SessionSource::VSCode))
        .filter_map(|entry| {
            DateTime::parse_from_rfc3339(&entry.last_event_at)
                .ok()
                .map(|ts| (entry.session_id, ts.with_timezone(&Utc)))
        })
        .collect::<HashMap<_, _>>();

    outputs.retain(|output| {
        allowed_sessions
            .get(&output.session_id)
            .is_some_and(|updated_at| Utc::now().signed_duration_since(*updated_at) <= max_age)
    });
}

fn upsert_stage1_output(outputs: &mut Vec<Stage1Output>, mut next: Stage1Output) {
    if let Some(existing) = outputs.iter().find(|output| output.session_id == next.session_id) {
        next.usage_count = existing.usage_count;
        next.last_usage = existing.last_usage;
    }
    outputs.retain(|output| output.session_id != next.session_id);
    outputs.push(next);
}

async fn load_stage1_outputs(code_home: &Path) -> io::Result<Vec<Stage1Output>> {
    let _guard = stage1_outputs_lock().lock().await;
    load_json_file(stage1_outputs_path(code_home)).await
}

async fn save_stage1_outputs(code_home: &Path, outputs: &[Stage1Output]) -> io::Result<()> {
    let _guard = stage1_outputs_lock().lock().await;
    let mut outputs = outputs.to_vec();
    merge_usage_metadata_from_persisted(code_home, &mut outputs).await?;
    save_json_file(stage1_outputs_path(code_home), &outputs).await
}

async fn merge_usage_metadata_from_persisted(
    code_home: &Path,
    outputs: &mut [Stage1Output],
) -> io::Result<()> {
    let path = stage1_outputs_path(code_home);
    let existing_outputs: Vec<Stage1Output> = match tokio::fs::read_to_string(&path).await {
        Ok(raw) => match serde_json::from_str(&raw) {
            Ok(outputs) => outputs,
            Err(err) => {
                warn!(
                    path = %path.display(),
                    "ignoring malformed persisted stage1 outputs while saving refreshed memories: {err}"
                );
                return Ok(());
            }
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    let usage_by_session = existing_outputs
        .into_iter()
        .map(|output| (output.session_id, (output.usage_count, output.last_usage)))
        .collect::<HashMap<_, _>>();

    for output in outputs {
        let Some((usage_count, last_usage)) = usage_by_session.get(&output.session_id) else {
            continue;
        };
        output.usage_count = output.usage_count.max(*usage_count);
        output.last_usage = max_optional_datetime(output.last_usage.clone(), last_usage.clone());
    }

    Ok(())
}

fn max_optional_datetime(
    left: Option<DateTime<Utc>>,
    right: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

async fn load_phase2_state(code_home: &Path) -> io::Result<Phase2State> {
    load_json_file(phase2_state_path(code_home)).await
}

async fn save_phase2_state(code_home: &Path, state: &Phase2State) -> io::Result<()> {
    save_json_file(phase2_state_path(code_home), state).await
}

async fn load_json_file<T>(path: PathBuf) -> io::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let raw = tokio::fs::read_to_string(path).await?;
    serde_json::from_str(&raw).map_err(|err| io::Error::other(format!("parse json: {err}")))
}

async fn save_json_file<T>(path: PathBuf, value: &T) -> io::Result<()>
where
    T: Serialize + ?Sized,
{
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let raw = serde_json::to_string_pretty(value)
        .map_err(|err| io::Error::other(format!("serialize json: {err}")))?;
    tokio::fs::write(path, raw).await
}

#[derive(Debug, Clone)]
struct EligibleEntry {
    session_id: Uuid,
    rollout_path: PathBuf,
    cwd: PathBuf,
    git_branch: Option<String>,
    source_updated_at: DateTime<Utc>,
}

impl EligibleEntry {
    fn from_catalog_entry(code_home: &Path, entry: &SessionIndexEntry) -> Option<Self> {
        let source_updated_at = DateTime::parse_from_rfc3339(&entry.last_event_at)
            .ok()?
            .with_timezone(&Utc);
        Some(Self {
            session_id: entry.session_id,
            rollout_path: code_home.join(&entry.rollout_path),
            cwd: entry.cwd_real.clone(),
            git_branch: entry.git_branch.clone(),
            source_updated_at,
        })
    }
}

impl From<&Stage1Output> for Stage1OutputRef {
    fn from(value: &Stage1Output) -> Self {
        Self {
            session_id: value.session_id,
            source_updated_at: value.source_updated_at,
            rollout_slug: value.rollout_slug.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::EventMsg;
    use crate::protocol::McpInvocation;
    use crate::protocol::McpToolCallBeginEvent;
    use crate::protocol::event_msg_to_protocol;
    use chrono::TimeDelta;

    fn output(session_id: Uuid, usage_count: u64, ts: &str) -> Stage1Output {
        Stage1Output {
            session_id,
            rollout_path: PathBuf::from("/tmp/rollout.jsonl"),
            cwd: PathBuf::from("/tmp"),
            git_branch: None,
            source_updated_at: DateTime::parse_from_rfc3339(ts).unwrap().with_timezone(&Utc),
            generated_at: DateTime::parse_from_rfc3339(ts).unwrap().with_timezone(&Utc),
            raw_memory: "raw".to_string(),
            rollout_summary: "summary".to_string(),
            rollout_slug: Some("slug".to_string()),
            usage_count,
            last_usage: None,
        }
    }

    #[test]
    fn phase2_selection_prioritizes_usage() {
        let older = output(Uuid::new_v4(), 10, "2026-01-01T00:00:00Z");
        let newer = output(Uuid::new_v4(), 1, "2026-02-01T00:00:00Z");
        let selection = build_phase2_selection(&[newer, older.clone()], 1, 365, &[]);
        assert_eq!(selection.selected, vec![older]);
    }

    #[test]
    fn phase2_selection_drops_unused_memories_outside_window() {
        let stale_ts = (Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        let fresh_ts = (Utc::now() - chrono::Duration::days(5)).to_rfc3339();
        let stale = output(Uuid::new_v4(), 1, &stale_ts);
        let fresh = output(Uuid::new_v4(), 1, &fresh_ts);
        let selection = build_phase2_selection(&[stale, fresh.clone()], 5, 30, &[]);
        assert_eq!(selection.selected, vec![fresh]);
    }

    #[test]
    fn phase2_prompt_payload_is_truncated() {
        let mut item = output(Uuid::new_v4(), 1, "2026-02-01T00:00:00Z");
        item.raw_memory = "x".repeat(PHASE2_SELECTED_PAYLOAD_BYTES * 2);
        item.rollout_summary = "y".repeat(PHASE2_ENTRY_SNIPPET_BYTES * 2);
        let selection = Phase2Selection {
            selected: vec![item.clone()],
            retained_artifacts: vec![item],
            removed: Vec::new(),
        };

        let prompt =
            build_phase2_user_prompt(Path::new("/tmp"), &SessionCatalog::default(), &selection);
        assert!(prompt.len() < PHASE2_SELECTED_PAYLOAD_BYTES * 2);
        assert!(prompt.contains("tokens truncated"));
    }

    #[test]
    fn rollout_skip_flag_detects_mcp_function_calls() {
        let item = RolloutItem::ResponseItem(ResponseItem::FunctionCall {
            name: "mcp__context7__resolve".to_string(),
            arguments: "{}".to_string(),
            call_id: "call-1".to_string(),
            id: None,
        });

        assert!(rollout_uses_mcp_or_web_search(&item));
    }

    #[test]
    fn rollout_skip_flag_detects_mcp_events() {
        let event = EventMsg::McpToolCallBegin(McpToolCallBeginEvent {
            call_id: "call-1".to_string(),
            invocation: McpInvocation {
                server: "context7".to_string(),
                tool: "resolve".to_string(),
                arguments: None,
            },
        });
        let event = event_msg_to_protocol(&event).expect("protocol event");
        let item = RolloutItem::EventMsg(event);

        assert!(rollout_uses_mcp_or_web_search(&item));
    }

    #[tokio::test]
    async fn save_stage1_outputs_preserves_newer_usage_metadata() {
        let code_home = tempfile::tempdir().expect("tempdir");
        let session_id = Uuid::new_v4();
        let persisted_last_usage = Some(Utc::now());
        let refreshed_last_usage = persisted_last_usage.map(|ts| ts - TimeDelta::hours(2));

        save_json_file(
            stage1_outputs_path(code_home.path()),
            &[Stage1Output {
                last_usage: persisted_last_usage,
                usage_count: 9,
                ..output(session_id, 1, "2026-02-01T00:00:00Z")
            }],
        )
        .await
        .expect("seed stage1 outputs");

        save_stage1_outputs(
            code_home.path(),
            &[Stage1Output {
                last_usage: refreshed_last_usage,
                usage_count: 2,
                ..output(session_id, 1, "2026-02-01T00:00:00Z")
            }],
        )
        .await
        .expect("save stage1 outputs");

        let saved = load_stage1_outputs(code_home.path())
            .await
            .expect("load stage1 outputs");
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].usage_count, 9);
        assert_eq!(saved[0].last_usage, persisted_last_usage);
    }

    #[tokio::test]
    async fn save_stage1_outputs_recovers_from_malformed_existing_file() {
        let code_home = tempfile::tempdir().expect("tempdir");
        let session_id = Uuid::new_v4();
        tokio::fs::create_dir_all(state_dir(code_home.path()))
            .await
            .expect("create state dir");
        tokio::fs::write(stage1_outputs_path(code_home.path()), "{not json")
            .await
            .expect("write malformed json");

        save_stage1_outputs(
            code_home.path(),
            &[output(session_id, 3, "2026-02-01T00:00:00Z")],
        )
        .await
        .expect("save stage1 outputs");

        let saved = load_stage1_outputs(code_home.path())
            .await
            .expect("load repaired stage1 outputs");
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].session_id, session_id);
        assert_eq!(saved[0].usage_count, 3);
    }
}
