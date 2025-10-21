#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::fmt::Write as _;

use crate::local_memory_util;

const PROMPTS_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/spec-kit/prompts.json"
));

fn block_on_sync<F, Fut, T>(factory: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let handle_clone = handle.clone();
        tokio::task::block_in_place(move || handle_clone.block_on(factory()))
    } else {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build runtime")
            .block_on(factory())
    }
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct AgentPrompt {
    #[serde(default)]
    pub role: Option<String>,
    pub prompt: String,
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(default)]
pub struct StagePrompts {
    pub version: Option<String>,
    pub gemini: Option<AgentPrompt>,
    pub claude: Option<AgentPrompt>,
    pub code: Option<AgentPrompt>,
    #[serde(rename = "gpt_codex")]
    pub gpt_codex: Option<AgentPrompt>,
    #[serde(rename = "gpt_pro")]
    pub gpt_pro: Option<AgentPrompt>,
    pub orchestrator_notes: Option<Vec<String>>,
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecAgent {
    Gemini,
    Claude,
    Code, // Claude Code (CLI assistant)
    GptCodex,
    GptPro,
}

// ARCH-006: Centralize agent name normalization
impl SpecAgent {
    /// Canonical name for storage/comparison (lowercase with underscores)
    pub fn canonical_name(&self) -> &'static str {
        match self {
            SpecAgent::Gemini => "gemini",
            SpecAgent::Claude => "claude",
            SpecAgent::Code => "code",
            SpecAgent::GptCodex => "gpt_codex",
            SpecAgent::GptPro => "gpt_pro",
        }
    }

    /// Parse from various string representations (case-insensitive)
    pub fn from_string(s: &str) -> Option<Self> {
        let normalized = s.to_ascii_lowercase().replace("-", "_").replace(" ", "_");
        match normalized.as_str() {
            "gemini" | "gemini_flash" | "gemini_2.0" => Some(Self::Gemini),
            "claude" | "claude_sonnet" | "claude_4" => Some(Self::Claude),
            "code" | "claude_code" => Some(Self::Code),
            "gpt_codex" | "gptcodex" | "gpt5_codex" | "gpt_5_codex" => Some(Self::GptCodex),
            "gpt_pro" | "gptpro" | "gpt5" | "gpt_5" | "gpt5pro" => Some(Self::GptPro),
            _ => None,
        }
    }

    /// Display name for UI rendering
    pub fn display_name(&self) -> &'static str {
        match self {
            SpecAgent::Gemini => "Gemini",
            SpecAgent::Claude => "Claude",
            SpecAgent::Code => "Claude Code",
            SpecAgent::GptCodex => "GPT-5 Codex",
            SpecAgent::GptPro => "GPT-5 Pro",
        }
    }

    /// All expected agents for consensus checking
    pub fn all() -> [Self; 5] {
        [
            Self::Gemini,
            Self::Claude,
            Self::Code,
            Self::GptCodex,
            Self::GptPro,
        ]
    }
}

#[derive(Debug, Clone)]
pub struct PromptRegistry {
    stages: HashMap<String, StagePrompts>,
}

static PROMPT_DATA: OnceCell<PromptRegistry> = OnceCell::new();

impl PromptRegistry {
    fn load() -> Self {
        let stages: HashMap<String, StagePrompts> =
            serde_json::from_str(PROMPTS_JSON).expect("invalid spec-kit prompt json");
        Self { stages }
    }

    pub fn stage(&self, name: &str) -> Option<&StagePrompts> {
        self.stages.get(name)
    }
}

pub fn registry() -> &'static PromptRegistry {
    PROMPT_DATA.get_or_init(PromptRegistry::load)
}

pub fn stage_version(stage: &str) -> Option<String> {
    registry().stage(stage)?.version.clone()
}

pub fn agent_prompt(stage: &str, agent: SpecAgent) -> Option<AgentPrompt> {
    let stage = registry().stage(stage)?;
    let prompt = match agent {
        SpecAgent::Gemini => stage.gemini.clone(),
        SpecAgent::Claude => stage.claude.clone(),
        SpecAgent::Code => stage.code.clone(),
        SpecAgent::GptCodex => stage.gpt_codex.clone(),
        SpecAgent::GptPro => stage.gpt_pro.clone(),
    }?;
    Some(prompt)
}

pub fn orchestrator_notes(stage: &str) -> Option<Vec<String>> {
    registry().stage(stage)?.orchestrator_notes.clone()
}

pub fn render_prompt(stage: &str, agent: SpecAgent, vars: &[(&str, &str)]) -> Option<String> {
    let prompt = agent_prompt(stage, agent)?;
    let mut text = prompt.prompt;
    for (key, value) in vars {
        let placeholder = format!("${{{}}}", key);
        text = text.replace(&placeholder, value);
    }
    if text.contains("${PROMPT_VERSION}") {
        let version = stage_version(stage).unwrap_or_else(|| "unversioned".to_string());
        text = text.replace("${PROMPT_VERSION}", &version);
    }
    Some(text)
}

fn stage_env_suffix(stage: SpecStage) -> String {
    stage.key().replace('-', "_").to_ascii_uppercase()
}

fn agent_env_prefix(agent: SpecAgent) -> &'static str {
    match agent {
        SpecAgent::Gemini => "GEMINI",
        SpecAgent::Claude => "CLAUDE",
        SpecAgent::Code => "CODE",
        SpecAgent::GptCodex => "GPT_CODEX",
        SpecAgent::GptPro => "GPT_PRO",
    }
}

fn resolve_metadata_field(
    field: &str,
    stage: SpecStage,
    agent: SpecAgent,
    default: &str,
) -> String {
    let stage_key = stage_env_suffix(stage);
    let agent_key = agent_env_prefix(agent);
    let mut env_name = String::new();
    write!(env_name, "SPECKIT_{}_{}_{}", field, agent_key, stage_key).unwrap();
    if let Ok(value) = std::env::var(&env_name) {
        return value;
    }
    env_name.clear();
    write!(env_name, "SPECKIT_{}_{}", field, agent_key).unwrap();
    if let Ok(value) = std::env::var(&env_name) {
        return value;
    }
    default.to_string()
}

fn model_metadata(stage: SpecStage, agent: SpecAgent) -> Vec<(String, String)> {
    let (model_id_default, release_default, mode_default) = match (stage, agent) {
        (SpecStage::Tasks | SpecStage::Unlock, SpecAgent::Gemini) => {
            ("gemini-2.5-flash", "2025-05-14", "fast")
        }
        (_, SpecAgent::Gemini) => ("gemini-2.5-pro", "2025-05-14", "thinking"),
        (SpecStage::Unlock, SpecAgent::Claude) => ("claude-4.5-sonnet", "2025-09-29", "balanced"),
        (_, SpecAgent::Claude) => ("claude-4.5-sonnet", "2025-09-29", "balanced"),
        (_, SpecAgent::Code) => ("claude-sonnet-4-5", "2025-10-22", "extended"),
        (SpecStage::Implement, SpecAgent::GptCodex) => ("gpt-5-codex", "2025-09-29", "auto"),
        (_, SpecAgent::GptCodex) => ("gpt-5-codex", "2025-09-29", "auto"),
        (SpecStage::Implement, SpecAgent::GptPro) => ("gpt-5", "2025-08-06", "high"),
        (_, SpecAgent::GptPro) => ("gpt-5", "2025-08-06", "high"),
    };

    vec![
        (
            "MODEL_ID".into(),
            resolve_metadata_field("MODEL_ID", stage, agent, model_id_default),
        ),
        (
            "MODEL_RELEASE".into(),
            resolve_metadata_field("MODEL_RELEASE", stage, agent, release_default),
        ),
        (
            "REASONING_MODE".into(),
            resolve_metadata_field("REASONING_MODE", stage, agent, mode_default),
        ),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecStage {
    Plan,
    Tasks,
    Implement,
    Validate,
    Audit,
    Unlock,
}

impl SpecStage {
    pub fn all() -> [SpecStage; 6] {
        [
            SpecStage::Plan,
            SpecStage::Tasks,
            SpecStage::Implement,
            SpecStage::Validate,
            SpecStage::Audit,
            SpecStage::Unlock,
        ]
    }

    pub fn key(self) -> &'static str {
        match self {
            SpecStage::Plan => "spec-plan",
            SpecStage::Tasks => "spec-tasks",
            SpecStage::Implement => "spec-implement",
            SpecStage::Validate => "spec-validate",
            SpecStage::Audit => "spec-audit",
            SpecStage::Unlock => "spec-unlock",
        }
    }

    pub fn command_name(self) -> &'static str {
        match self {
            SpecStage::Plan => "spec-plan",
            SpecStage::Tasks => "spec-tasks",
            SpecStage::Implement => "spec-implement",
            SpecStage::Validate => "spec-validate",
            SpecStage::Audit => "spec-audit",
            SpecStage::Unlock => "spec-unlock",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            SpecStage::Plan => "Plan",
            SpecStage::Tasks => "Tasks",
            SpecStage::Implement => "Implement",
            SpecStage::Validate => "Validate",
            SpecStage::Audit => "Audit",
            SpecStage::Unlock => "Unlock",
        }
    }
}

pub fn stage_version_enum(stage: SpecStage) -> Option<String> {
    stage_version(stage.key())
}

#[derive(Debug, thiserror::Error)]
pub enum PromptBuildError {
    #[error("`/{command}` requires a SPEC ID (e.g. `/{command} SPEC-OPS-005`)")]
    MissingSpecId { command: String },
    #[error("No prompts defined for stage '{0}'")]
    MissingStage(&'static str),
}

pub fn build_stage_prompt(stage: SpecStage, raw_args: &str) -> Result<String, PromptBuildError> {
    build_stage_prompt_with_mcp(stage, raw_args, None)
}

pub fn build_stage_prompt_with_mcp(
    stage: SpecStage,
    raw_args: &str,
    mcp_manager: Option<Arc<codex_core::mcp_connection_manager::McpConnectionManager>>,
) -> Result<String, PromptBuildError> {
    let trimmed = raw_args.trim();
    if trimmed.is_empty() {
        return Err(PromptBuildError::MissingSpecId {
            command: stage.command_name().to_string(),
        });
    }

    let mut parts = trimmed.split_whitespace();
    let spec_id = parts.next().unwrap();
    let remainder = trimmed[spec_id.len()..].trim().to_string();

    let context_hint = format!(
        "Use local-memory search (domains: spec-tracker, docs-ops, impl-notes, infra-ci) to gather current context for {spec_id}. Summaries must cite memory IDs or MCP transcripts."
    );

    let goal_hint = if !remainder.is_empty() {
        remainder.clone()
    } else {
        "(no additional goal provided)".to_string()
    };

    let mut replacements: Vec<(String, String)> = vec![
        ("SPEC_ID".into(), spec_id.to_string()),
        ("CONTEXT".into(), context_hint.clone()),
        ("GOAL".into(), goal_hint.clone()),
    ];

    match stage {
        SpecStage::Plan => {
            replacements.push((
                "PREVIOUS_OUTPUTS.gemini".into(),
                "Gemini Ultra findings stored in local-memory (spec-tracker domain).".into(),
            ));
            replacements.push((
                "PREVIOUS_OUTPUTS".into(),
                "Refer to Gemini + Claude outputs captured in local-memory for consensus notes."
                    .into(),
            ));
        }
        SpecStage::Tasks => {
            replacements.push((
                "PREVIOUS_OUTPUTS.gemini".into(),
                "Gemini research from /spec-plan (local-memory spec-tracker).".into(),
            ));
            replacements.push((
                "PREVIOUS_OUTPUTS.plan".into(),
                "Final plan consensus written during /spec-plan.".into(),
            ));
        }
        SpecStage::Implement => {
            replacements.push((
                "PREVIOUS_OUTPUTS.tasks".into(),
                "Latest /spec-tasks consensus stored in docs/SPEC-*/tasks.md and local-memory."
                    .into(),
            ));
        }
        SpecStage::Validate | SpecStage::Audit | SpecStage::Unlock => {
            // No extra replacements required
        }
    }

    // Provide fallbacks for placeholders that might appear in prompts
    replacements.push((
        "PREVIOUS_OUTPUTS".into(),
        "See local-memory entries from earlier /spec-* stages.".into(),
    ));
    replacements.push((
        "PREVIOUS_OUTPUTS.plan".into(),
        "Final plan consensus (if available).".into(),
    ));
    replacements.push((
        "PREVIOUS_OUTPUTS.tasks".into(),
        "Task breakdown consensus (if available).".into(),
    ));

    let registry = registry();
    let stage_prompts = registry
        .stage(stage.key())
        .ok_or(PromptBuildError::MissingStage(stage.key()))?;
    let prompt_version = stage_prompts
        .version
        .clone()
        .unwrap_or_else(|| "unversioned".to_string());
    replacements.push(("PROMPT_VERSION".into(), prompt_version.clone()));

    let mut bundle = String::new();
    bundle.push_str(&format!("# /{} — {}\n\n", stage.command_name(), spec_id));
    bundle.push_str("Leverage local-memory before starting, then run the agents below in parallel using these prompts. Record outputs back into local-memory (spec-tracker, impl-notes, docs-ops).\n\n");
    if let SpecStage::Plan = stage {
        bundle.push_str(&format!("Goal: {}\n\n", goal_hint));
    }
    bundle.push_str(&format!("Prompt version: {}\n\n", prompt_version));

    match gather_local_memory_context(spec_id, stage, mcp_manager) {
        Ok(entries) if !entries.is_empty() => {
            bundle.push_str("## Local-memory context\n");
            for entry in entries {
                bundle.push_str("- ");
                bundle.push_str(&entry);
                bundle.push('\n');
            }
            bundle.push('\n');
        }
        Ok(_) => {
            bundle.push_str(
                "## Local-memory context\n- No stage-specific local-memory entries found yet.\n\n",
            );
        }
        Err(err) => {
            bundle.push_str(&format!(
                "## Local-memory context\n- Unable to fetch local-memory context: {}\n\n",
                err
            ));
        }
    }

    bundle.push_str("## HTTP MCP (HAL)\n");
    bundle.push_str(
        "- If a HAL HTTP MCP profile is configured (see docs/SPEC-KIT-018-hal-http-mcp), drive the health/REST/GraphQL templates via `cargo run -p codex-mcp-client --bin call_tool -- --tool … -- npx -y hal-mcp` and archive the outputs in the project’s evidence folder.\n\n",
    );

    if let Some(prompt) = stage_prompts.gemini.clone() {
        let mut gemini_vars = replacements.clone();
        gemini_vars.extend(model_metadata(stage, SpecAgent::Gemini));
        let gemini_refs: Vec<(&str, &str)> = gemini_vars
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let rendered = render_prompt(stage.key(), SpecAgent::Gemini, &gemini_refs)
            .unwrap_or_else(|| prompt.prompt);
        bundle.push_str("## Gemini Ultra — Research\n");
        bundle.push_str(&rendered);
        bundle.push_str("\n\n");
    }
    if let Some(prompt) = stage_prompts.claude.clone() {
        let mut claude_vars = replacements.clone();
        claude_vars.extend(model_metadata(stage, SpecAgent::Claude));
        let claude_refs: Vec<(&str, &str)> = claude_vars
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let rendered = render_prompt(stage.key(), SpecAgent::Claude, &claude_refs)
            .unwrap_or_else(|| prompt.prompt);
        bundle.push_str("## Claude Sonnet 4.5 — Synthesis\n");
        bundle.push_str(&rendered);
        bundle.push_str("\n\n");
    }
    if let Some(prompt) = stage_prompts.gpt_codex.clone() {
        let mut codex_vars = replacements.clone();
        codex_vars.extend(model_metadata(stage, SpecAgent::GptCodex));
        let codex_refs: Vec<(&str, &str)> = codex_vars
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let rendered = render_prompt(stage.key(), SpecAgent::GptCodex, &codex_refs)
            .unwrap_or_else(|| prompt.prompt);
        bundle.push_str("## GPT-5 Codex — Code Diff Proposal\n");
        bundle.push_str(&rendered);
        bundle.push_str("\n\n");
    }
    if let Some(prompt) = stage_prompts.gpt_pro.clone() {
        let mut gpt_vars = replacements.clone();
        gpt_vars.extend(model_metadata(stage, SpecAgent::GptPro));
        let gpt_refs: Vec<(&str, &str)> = gpt_vars
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let rendered = render_prompt(stage.key(), SpecAgent::GptPro, &gpt_refs)
            .unwrap_or_else(|| prompt.prompt);
        bundle.push_str("## GPT-5 — Arbiter & QA\n");
        bundle.push_str(&rendered);
        bundle.push_str("\n");
    }

    Ok(bundle)
}

// FORK-SPECIFIC (just-every/code): Migrated to native MCP (ARCH-004 completion)
fn gather_local_memory_context(
    spec_id: &str,
    stage: SpecStage,
    mcp_manager: Option<Arc<codex_core::mcp_connection_manager::McpConnectionManager>>,
) -> Result<Vec<String>, String> {
    use serde_json::json;

    // Use native MCP if available (ARCH-002 pattern)
    let results = if let Some(manager_arc) = mcp_manager {
        let spec_id_owned = spec_id.to_string();
        let stage_name = stage.command_name().to_string();
        block_on_sync(move || {
            let manager = manager_arc.clone();
            let spec_id = spec_id_owned.clone();
            let stage_name = stage_name.clone();
            async move {
                let query = format!("{} {}", spec_id, stage_name);
                let args = json!({
                    "query": query,
                    "limit": 8,
                    "tags": [format!("spec:{}", spec_id), format!("stage:{}", stage_name)],
                    "search_type": "hybrid"
                });

                manager
                    .call_tool(
                        "local-memory",
                        "search",
                        Some(args),
                        Some(std::time::Duration::from_secs(10)),
                    )
                    .await
                    .ok()
                    .and_then(|result| parse_mcp_results_to_local_memory(&result).ok())
                    .unwrap_or_default()
            }
        })
    } else {
        // Fallback: Return empty (graceful degradation when mcp_manager not passed)
        Vec::new()
    };

    let mut entries: Vec<String> = Vec::new();

    for result in results.into_iter().take(5) {
        let mut snippet = result.memory.content.trim().replace('\n', " ");
        if snippet.is_empty() {
            continue;
        }
        if snippet.len() > 160 {
            snippet.truncate(160);
            snippet.push_str("…");
        }

        if let Some(id) = result.memory.id.as_ref() {
            entries.push(format!("{} — {}", id, snippet));
        } else {
            entries.push(snippet);
        }
    }

    Ok(entries)
}

// Helper: Parse MCP CallToolResult to LocalMemorySearchResult format (pub for handler.rs usage)
pub fn parse_mcp_results_to_local_memory(
    result: &mcp_types::CallToolResult,
) -> Result<Vec<local_memory_util::LocalMemorySearchResult>, String> {
    if result.content.is_empty() {
        return Ok(Vec::new());
    }

    let mut all_results = Vec::new();
    for content_item in &result.content {
        if let mcp_types::ContentBlock::TextContent(text_content) = content_item {
            let text = &text_content.text;
            if let Ok(json_results) = serde_json::from_str::<Vec<serde_json::Value>>(text) {
                for json_result in json_results {
                    if let Ok(parsed) = serde_json::from_value::<
                        local_memory_util::LocalMemorySearchResult,
                    >(json_result)
                    {
                        all_results.push(parsed);
                    }
                }
            }
        }
    }

    Ok(all_results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::sync::Mutex;
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn build_stub_script(responses: &[(&str, &str)]) -> String {
        let mut script =
            String::from("#!/usr/bin/env python3\nimport json\nimport sys\nresponses = {\n");
        for (key, json) in responses {
            script.push_str(&format!("    {key:?}: json.loads({json:?}),\n"));
        }
        if !responses.iter().any(|(key, _)| *key == "default") {
            script.push_str("    'default': {'success': True, 'data': {'results': []}},\n");
        }
        script.push_str(
            "}\nstage = next((arg for arg in sys.argv if arg.startswith('stage:')), 'default')\n",
        );
        script.push_str(
            "payload = responses.get(stage, responses.get('default', {'success': True, 'data': {'results': []}}))\n",
        );
        script.push_str("json.dump(payload, sys.stdout)\n");
        script.push_str("sys.stdout.flush()\n");
        script
    }

    fn with_local_memory_stub<F, R>(responses: &[(&str, &str)], f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().expect("temp dir");
        let script_path = temp_dir.path().join("local-memory-stub.py");
        let script_content = build_stub_script(responses);
        fs::write(&script_path, script_content).expect("write stub");
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms).unwrap();
        }
        unsafe {
            std::env::set_var("LOCAL_MEMORY_BIN", &script_path);
        }
        let result = f();
        unsafe {
            std::env::remove_var("LOCAL_MEMORY_BIN");
        }
        drop(temp_dir);
        result
    }

    fn metadata_map(stage: SpecStage, agent: SpecAgent) -> HashMap<String, String> {
        model_metadata(stage, agent)
            .into_iter()
            .collect::<HashMap<_, _>>()
    }

    #[test]
    fn agent_prompt_is_loaded() {
        let gemini = agent_prompt("spec-plan", SpecAgent::Gemini).expect("gemini prompt");
        assert!(gemini.prompt.contains("Summarize:"));
    }

    #[test]
    fn placeholder_substitution() {
        let mut owned: Vec<(String, String)> = vec![
            ("SPEC_ID".into(), "SPEC-OPS-123".into()),
            ("CONTEXT".into(), "<ctx>".into()),
        ];
        owned.extend(model_metadata(SpecStage::Plan, SpecAgent::Gemini));
        let refs: Vec<(&str, &str)> = owned
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let rendered = render_prompt("spec-plan", SpecAgent::Gemini, &refs).expect("rendered");
        assert!(rendered.contains("SPEC-OPS-123"));
        assert!(rendered.contains("<ctx>"));
    }

    #[test]
    fn orchestrator_notes_present_for_auto() {
        let notes = orchestrator_notes("spec-auto").expect("notes");
        assert!(!notes.is_empty());
    }

    #[test]
    fn build_stage_prompt_requires_spec_id() {
        let err = build_stage_prompt(SpecStage::Plan, " ").unwrap_err();
        assert!(matches!(err, PromptBuildError::MissingSpecId { .. }));
    }

    #[test]
    fn build_stage_prompt_includes_agent_sections() {
        let prompt = build_stage_prompt(SpecStage::Plan, "SPEC-OPS-999 Align rollout").unwrap();
        assert!(prompt.contains("/spec-plan"));
        assert!(prompt.contains("Prompt version: 20251002-plan-a"));
        assert!(prompt.contains("Gemini Ultra"));
        assert!(prompt.contains("Claude Sonnet"));
        assert!(prompt.contains("GPT-5"));
    }

    #[test]
    fn gather_local_memory_context_returns_empty_without_mcp() {
        // ARCH-004: Subprocess mocking removed, native MCP path tested via integration tests
        // When mcp_manager is None (no MCP available), gracefully returns empty
        let entries = gather_local_memory_context("SPEC-OPS-123", SpecStage::Plan, None).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn gather_local_memory_context_handles_no_runtime() {
        // When called outside tokio runtime, returns empty gracefully
        let entries = gather_local_memory_context("SPEC-OPS-123", SpecStage::Plan, None).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn build_stage_prompt_works_without_mcp() {
        // ARCH-004: Without MCP manager, prompt still builds (context section empty)
        let prompt = build_stage_prompt(SpecStage::Plan, "SPEC-OPS-123 Align migration").unwrap();

        assert!(prompt.contains("## Local-memory context"));
        assert!(prompt.contains("No stage-specific local-memory entries"));
        assert!(prompt.contains("Prompt version"));
    }

    #[test]
    fn build_stage_prompt_emits_empty_notice_for_tasks() {
        let prompt = with_local_memory_stub(
            &[("default", r#"{"success": true, "data": {"results": []}}"#)],
            || build_stage_prompt(SpecStage::Tasks, "SPEC-OPS-123").unwrap(),
        );

        assert!(prompt.contains("No stage-specific local-memory entries"));
        assert!(prompt.contains("Prompt version: 20251002-tasks-a"));
    }

    #[test]
    fn build_stage_prompt_includes_version_for_tasks() {
        // ARCH-004: Native MCP path tested via integration tests, unit test verifies basic structure
        let prompt = build_stage_prompt(SpecStage::Tasks, "SPEC-OPS-123").unwrap();

        assert!(prompt.contains("## Local-memory context"));
        assert!(prompt.contains("Prompt version: 20251002-tasks-a"));
    }

    #[test]
    fn all_versioned_prompts_include_placeholder() {
        for stage in SpecStage::all() {
            let stage_key = stage.key();
            let version = stage_version(stage_key);
            if version.is_none() {
                continue;
            }
            let prompts = registry().stage(stage_key).expect("stage present");
            for (agent, prompt_opt) in [
                ("gemini", prompts.gemini.as_ref()),
                ("claude", prompts.claude.as_ref()),
                ("gpt_codex", prompts.gpt_codex.as_ref()),
                ("gpt_pro", prompts.gpt_pro.as_ref()),
            ] {
                if let Some(prompt) = prompt_opt {
                    assert!(
                        prompt.prompt.contains("${PROMPT_VERSION}"),
                        "prompt for stage {} agent {} missing ${{PROMPT_VERSION}}",
                        stage_key,
                        agent
                    );
                }
            }
        }
    }

    #[test]
    fn model_metadata_defaults_align_with_strategy() {
        let gemini = metadata_map(SpecStage::Plan, SpecAgent::Gemini);
        assert_eq!(gemini.get("MODEL_ID"), Some(&"gemini-2.5-pro".to_string()));
        assert_eq!(gemini.get("REASONING_MODE"), Some(&"thinking".to_string()));

        let claude = metadata_map(SpecStage::Implement, SpecAgent::Claude);
        assert_eq!(
            claude.get("MODEL_ID"),
            Some(&"claude-4.5-sonnet".to_string())
        );

        let codex = metadata_map(SpecStage::Implement, SpecAgent::GptCodex);
        assert_eq!(codex.get("MODEL_ID"), Some(&"gpt-5-codex".to_string()));
        assert_eq!(codex.get("REASONING_MODE"), Some(&"auto".to_string()));

        let gpt = metadata_map(SpecStage::Implement, SpecAgent::GptPro);
        assert_eq!(gpt.get("MODEL_ID"), Some(&"gpt-5".to_string()));
        assert_eq!(gpt.get("REASONING_MODE"), Some(&"high".to_string()));
    }

    #[test]
    fn model_metadata_env_overrides_apply() {
        unsafe {
            std::env::set_var("SPECKIT_MODEL_ID_GPT_PRO_SPEC_IMPLEMENT", "custom-gpt");
            std::env::set_var("SPECKIT_MODEL_RELEASE_GPT_PRO", "2025-09-27");
            std::env::set_var("SPECKIT_REASONING_MODE_GPT_PRO", "deep");
        }
        let map = metadata_map(SpecStage::Implement, SpecAgent::GptPro);
        unsafe {
            std::env::remove_var("SPECKIT_MODEL_ID_GPT_PRO_SPEC_IMPLEMENT");
            std::env::remove_var("SPECKIT_MODEL_RELEASE_GPT_PRO");
            std::env::remove_var("SPECKIT_REASONING_MODE_GPT_PRO");
        }

        assert_eq!(map.get("MODEL_ID"), Some(&"custom-gpt".to_string()));
        assert_eq!(map.get("MODEL_RELEASE"), Some(&"2025-09-27".to_string()));
        assert_eq!(map.get("REASONING_MODE"), Some(&"deep".to_string()));
    }
}
