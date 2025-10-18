//! Consensus checking infrastructure for multi-agent spec-kit automation
//!
//! This module handles consensus validation across multiple AI agents,
//! artifact collection from local-memory, and synthesis result persistence.

use super::error::{Result, SpecKitError};
// FORK-SPECIFIC (just-every/code): LocalMemoryClient removed, using native MCP
use crate::spec_prompts::SpecStage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

// ============================================================================
// TYPES (moved from chatwidget/mod.rs)
// ============================================================================

#[derive(Debug, Clone)]
pub(in super::super) struct ConsensusArtifactData {
    pub memory_id: Option<String>,
    pub agent: String,
    pub version: Option<String>,
    pub content: Value,
}

#[derive(Clone)]
pub(in super::super) struct ConsensusEvidenceHandle {
    pub path: PathBuf,
    pub sha256: String,
}

pub(in super::super) struct ConsensusTelemetryPaths {
    pub agent_paths: Vec<PathBuf>,
    pub telemetry_path: PathBuf,
    pub synthesis_path: PathBuf,
}

#[derive(Clone, Serialize, Deserialize)]
pub(in super::super) struct ConsensusArtifactVerdict {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<String>,
    pub agent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub content: Value,
}

#[derive(Clone, Serialize, Deserialize)]
pub(in super::super) struct ConsensusVerdict {
    pub spec_id: String,
    pub stage: String,
    pub recorded_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_version: Option<String>,
    pub consensus_ok: bool,
    pub degraded: bool,
    pub required_fields_ok: bool,
    pub missing_agents: Vec<String>,
    pub agreements: Vec<String>,
    pub conflicts: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregator_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregator_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregator: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synthesis_path: Option<String>,
    pub artifacts: Vec<ConsensusArtifactVerdict>,
}

#[derive(Debug)]
pub(in super::super) struct ConsensusSynthesisSummary {
    pub status: String,
    pub missing_agents: Vec<String>,
    pub agreements: Vec<String>,
    pub conflicts: Vec<String>,
    pub prompt_version: Option<String>,
    pub path: PathBuf,
}

#[derive(Debug, Deserialize)]
pub(in super::super) struct ConsensusSynthesisRaw {
    pub stage: Option<String>,
    #[serde(rename = "specId")]
    pub spec_id: Option<String>,
    pub status: String,
    #[serde(default)]
    pub missing_agents: Vec<String>,
    #[serde(default)]
    pub consensus: ConsensusSynthesisConsensusRaw,
    pub prompt_version: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(in super::super) struct ConsensusSynthesisConsensusRaw {
    #[serde(default)]
    pub agreements: Vec<String>,
    #[serde(default)]
    pub conflicts: Vec<String>,
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

pub(in super::super) fn telemetry_value_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

pub(in super::super) fn telemetry_agent_slug(agent: &str) -> String {
    let mut slug = String::new();
    let mut last_was_sep = false;
    for ch in agent.chars() {
        let lower = ch.to_ascii_lowercase();
        let is_alnum = lower.is_ascii_alphanumeric();
        if is_alnum {
            slug.push(lower);
            last_was_sep = false;
        } else if !slug.is_empty() && !last_was_sep {
            slug.push('_');
            last_was_sep = true;
        }
    }
    let trimmed = slug.trim_matches('_');
    if trimmed.is_empty() {
        "agent".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Parse stage name from string (used by /spec-consensus command)
pub(in super::super) fn parse_consensus_stage(stage: &str) -> Option<SpecStage> {
    match stage.to_ascii_lowercase().as_str() {
        "plan" | "spec-plan" => Some(SpecStage::Plan),
        "tasks" | "spec-tasks" => Some(SpecStage::Tasks),
        "implement" | "spec-implement" => Some(SpecStage::Implement),
        "validate" | "spec-validate" => Some(SpecStage::Validate),
        "audit" | "review" | "spec-audit" | "spec-review" => Some(SpecStage::Audit),
        "unlock" | "spec-unlock" => Some(SpecStage::Unlock),
        _ => None,
    }
}

/// Get expected agent roster for a spec stage
// ARCH-006: Use SpecAgent enum instead of strings
pub(in super::super) fn expected_agents_for_stage(stage: SpecStage) -> Vec<crate::spec_prompts::SpecAgent> {
    use crate::spec_prompts::SpecAgent;
    match stage {
        SpecStage::Implement => vec![SpecAgent::Gemini, SpecAgent::Claude, SpecAgent::GptCodex, SpecAgent::GptPro],
        _ => vec![SpecAgent::Gemini, SpecAgent::Claude, SpecAgent::GptPro],
    }
}

/// Extract string array from JSON value
pub(in super::super) fn extract_string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Validate that summary has required fields for the stage
pub(in super::super) fn validate_required_fields(stage: SpecStage, summary: &Value) -> bool {
    let obj = match summary.as_object() {
        Some(o) => o,
        None => return false,
    };

    // Common fields
    if !obj.contains_key("stage") || !obj.contains_key("agent") {
        return false;
    }

    // Stage-specific required fields
    match stage {
        SpecStage::Plan => {
            obj.contains_key("work_breakdown") && obj.contains_key("acceptance_mapping")
        }
        SpecStage::Tasks => obj.contains_key("tasks"),
        SpecStage::Implement => obj.contains_key("implementation"),
        SpecStage::Validate => obj.contains_key("test_strategy"),
        SpecStage::Audit => obj.contains_key("audit_verdict"),
        SpecStage::Unlock => obj.contains_key("unlock_decision"),
    }
}

// ============================================================================
// CORE CONSENSUS LOGIC
// ============================================================================

use crate::local_memory_util::{self, LocalMemorySearchResult};
use std::fs;
use std::path::Path;

/// Collect consensus artifacts from local-memory (primary) or evidence files (fallback)
// FORK-SPECIFIC (just-every/code): Made async for native MCP with ARCH-002 fallback
pub(crate) async fn collect_consensus_artifacts(
    evidence_root: &Path,
    spec_id: &str,
    stage: SpecStage,
    mcp_manager: &codex_core::mcp_connection_manager::McpConnectionManager,
) -> Result<(Vec<ConsensusArtifactData>, Vec<String>)> {
    let mut warnings: Vec<String> = Vec::new();

    // ARCH-002: Try MCP first (faster, more current), fallback to files if unavailable
    match fetch_memory_entries(spec_id, stage, mcp_manager).await {
        Ok((entries, mut memory_warnings)) => {
            warnings.append(&mut memory_warnings);

            // Parse MCP results into artifacts
            let mut artifacts: Vec<ConsensusArtifactData> = Vec::new();

            for result in entries {
                let memory_id = result.memory.id.clone();
                let content_str = result.memory.content.trim();
                if content_str.is_empty() {
                    warnings.push("local-memory entry had empty content".to_string());
                    continue;
                }

                let value = match serde_json::from_str::<Value>(content_str) {
                    Ok(v) => v,
                    Err(err) => {
                        warnings.push(format!("unable to parse consensus artifact JSON: {err}"));
                        continue;
                    }
                };

                let agent = match value
                    .get("agent")
                    .or_else(|| value.get("model"))
                    .and_then(|v| v.as_str())
                {
                    Some(agent) if !agent.trim().is_empty() => agent.trim().to_string(),
                    _ => {
                        warnings.push("consensus artifact missing agent field".to_string());
                        continue;
                    }
                };

                let stage_matches = value
                    .get("stage")
                    .or_else(|| value.get("stage_name"))
                    .and_then(|v| v.as_str())
                    .and_then(parse_consensus_stage)
                    .map(|parsed| parsed == stage)
                    .unwrap_or(false);

                if !stage_matches {
                    warnings.push(format!(
                        "skipping local-memory entry for agent {} because stage did not match {}",
                        agent,
                        stage.command_name()
                    ));
                    continue;
                }

                let spec_matches = value
                    .get("spec_id")
                    .or_else(|| value.get("specId"))
                    .and_then(|v| v.as_str())
                    .map(|reported| reported.eq_ignore_ascii_case(spec_id))
                    .unwrap_or(true);

                if !spec_matches {
                    warnings.push(format!(
                        "skipping local-memory entry for agent {} because spec id did not match {}",
                        agent, spec_id
                    ));
                    continue;
                }

                let version = value
                    .get("prompt_version")
                    .or_else(|| value.get("promptVersion"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                artifacts.push(ConsensusArtifactData {
                    memory_id,
                    agent,
                    version,
                    content: value,
                });
            }

            return Ok((artifacts, warnings));
        }
        Err(mcp_err) => {
            // ARCH-002: Fallback to file-based evidence if MCP unavailable
            tracing::warn!("MCP fetch failed, falling back to file-based evidence: {}", mcp_err);
            warnings.push(format!(
                "⚠ Using file-based evidence (local-memory MCP unavailable: {})",
                mcp_err
            ));

            match load_artifacts_from_evidence(evidence_root, spec_id, stage) {
                Ok(Some((artifacts, mut evidence_warnings))) => {
                    warnings.append(&mut evidence_warnings);
                    return Ok((artifacts, warnings));
                }
                Ok(None) => {
                    warnings.push("No file-based evidence found either".to_string());
                }
                Err(err) => {
                    warnings.push(format!("File-based evidence also failed: {}", err));
                }
            }
        }
    }

    // If both MCP and file-based evidence failed
    Err(SpecKitError::NoConsensusFound {
        spec_id: spec_id.to_string(),
        stage: stage.command_name().to_string(),
        directory: evidence_root.to_path_buf(),
    })
}


fn load_artifacts_from_evidence(
    evidence_root: &Path,
    spec_id: &str,
    stage: SpecStage,
) -> Result<Option<(Vec<ConsensusArtifactData>, Vec<String>)>> {
    let consensus_dir = evidence_root.join(spec_id);
    if !consensus_dir.exists() {
        return Ok(None);
    }

    let stage_prefix = format!("{}_", stage.command_name());
    let suffix = "_artifact.json";

    let entries = fs::read_dir(&consensus_dir).map_err(|e| {
        format!(
            "Failed to read consensus evidence directory {}: {}",
            consensus_dir.display(),
            e
        )
    })?;

    let mut artifacts: Vec<ConsensusArtifactData> = Vec::new();
    let warnings: Vec<String> = Vec::new();

    for entry_res in entries {
        let entry = entry_res.map_err(|e| format!("Failed to read directory entry: {e}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(&stage_prefix) || !name.ends_with(suffix) {
            continue;
        }

        let contents = fs::read_to_string(&path).map_err(|e| {
            format!(
                "Failed to read consensus artifact {}: {}",
                path.display(),
                e
            )
        })?;

        let value: Value = serde_json::from_str(&contents).map_err(|e| {
            format!(
                "Failed to parse consensus artifact JSON {}: {}",
                path.display(),
                e
            )
        })?;

        let agent = value
            .get("agent")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let version = value
            .get("prompt_version")
            .or_else(|| value.get("promptVersion"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        artifacts.push(ConsensusArtifactData {
            memory_id: None,
            agent,
            version,
            content: value,
        });
    }

    if artifacts.is_empty() {
        Ok(None)
    } else {
        Ok(Some((artifacts, warnings)))
    }
}

// FORK-SPECIFIC (just-every/code): Native MCP for local-memory
async fn fetch_memory_entries(
    spec_id: &str,
    stage: SpecStage,
    mcp_manager: &codex_core::mcp_connection_manager::McpConnectionManager,
) -> Result<(Vec<LocalMemorySearchResult>, Vec<String>)> {
    use serde_json::json;

    let query = format!("{} {}", spec_id, stage.command_name());
    let args = json!({
        "query": query,
        "limit": 20,
        "tags": [format!("spec:{}", spec_id), format!("stage:{}", stage.command_name())],
        "search_type": "hybrid"
    });

    let result = mcp_manager
        .call_tool("local-memory", "search", Some(args), Some(std::time::Duration::from_secs(30)))
        .await
        .map_err(|e| SpecKitError::from_string(format!("MCP local-memory search failed: {}", e)))?;

    let results = parse_mcp_search_results(&result)
        .map_err(|e| SpecKitError::from_string(format!("Failed to parse MCP results: {}", e)))?;

    if results.is_empty() {
        Err(SpecKitError::NoConsensusFound {
            spec_id: spec_id.to_string(),
            stage: stage.command_name().to_string(),
            directory: std::path::PathBuf::from("local-memory"),
        })
    } else {
        Ok((results, Vec::new()))
    }
}

// FORK-SPECIFIC (just-every/code): Parse MCP search results
fn parse_mcp_search_results(result: &mcp_types::CallToolResult) -> Result<Vec<LocalMemorySearchResult>> {
    if result.content.is_empty() {
        return Ok(Vec::new());
    }

    let mut all_results = Vec::new();
    for content_item in &result.content {
        if let mcp_types::ContentBlock::TextContent(text_content) = content_item {
            let text = &text_content.text;
            if let Ok(json_results) = serde_json::from_str::<Vec<serde_json::Value>>(text) {
                for json_result in json_results {
                    if let Ok(parsed) = serde_json::from_value::<LocalMemorySearchResult>(json_result) {
                        all_results.push(parsed);
                    }
                }
            }
        }
    }

    Ok(all_results)
}

/// Load latest consensus synthesis file for spec/stage
pub(crate) fn load_latest_consensus_synthesis(
    cwd: &Path,
    spec_id: &str,
    stage: SpecStage,
) -> Result<Option<ConsensusSynthesisSummary>> {
    // MAINT-7: Use centralized path helper
    let base = super::evidence::consensus_dir(cwd).join(spec_id);
    if !base.exists() {
        return Ok(None);
    }

    let stage_prefix = format!("{}_", stage.command_name());
    let suffix = "_synthesis.json";

    let mut candidates: Vec<PathBuf> = fs::read_dir(&base)
        .map_err(|e| {
            format!(
                "Failed to read consensus synthesis directory {}: {}",
                base.display(),
                e
            )
        })?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&stage_prefix) && name.ends_with(suffix) {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    if candidates.is_empty() {
        return Ok(None);
    }

    candidates.sort();
    let latest_path = candidates.pop().unwrap();

    let contents = fs::read_to_string(&latest_path).map_err(|e| {
        format!(
            "Failed to read consensus synthesis {}: {}",
            latest_path.display(),
            e
        )
    })?;

    let raw: ConsensusSynthesisRaw = serde_json::from_str(&contents).map_err(|e| {
        format!(
            "Failed to parse consensus synthesis {}: {}",
            latest_path.display(),
            e
        )
    })?;

    if let Some(raw_stage) = raw.stage.as_deref() {
        if raw_stage != stage.command_name() {
            return Err(format!(
                "Consensus synthesis stage mismatch: expected {}, found {}",
                stage.command_name(),
                raw_stage
            ).into());
        }
    }

    if let Some(raw_spec) = raw.spec_id.as_deref() {
        if !raw_spec.eq_ignore_ascii_case(spec_id) {
            return Err(format!(
                "Consensus synthesis spec mismatch: expected {}, found {}",
                spec_id, raw_spec
            ).into());
        }
    }

    Ok(Some(ConsensusSynthesisSummary {
        status: raw.status,
        missing_agents: raw.missing_agents,
        agreements: raw.consensus.agreements,
        conflicts: raw.consensus.conflicts,
        prompt_version: raw.prompt_version,
        path: latest_path,
    }))
}

use std::collections::HashSet;

/// Run consensus check for spec stage
// FORK-SPECIFIC (just-every/code): Made async for native MCP
pub async fn run_spec_consensus(
    cwd: &Path,
    spec_id: &str,
    stage: SpecStage,
    telemetry_enabled: bool,
    mcp_manager: &codex_core::mcp_connection_manager::McpConnectionManager,
) -> Result<(Vec<ratatui::text::Line<'static>>, bool)> {
    // MAINT-7: Use centralized path helper
    let evidence_root = super::evidence::consensus_dir(cwd);

    let (artifacts, mut warnings) = collect_consensus_artifacts(&evidence_root, spec_id, stage, mcp_manager).await?;
    if artifacts.is_empty() {
        return Err(format!(
            "No structured local-memory entries found for {} stage '{}'. Ensure agents stored their JSON via local-memory remember.",
            spec_id,
            stage.command_name()
        ).into());
    }

    let synthesis_summary = match load_latest_consensus_synthesis(cwd, spec_id, stage) {
        Ok(summary) => summary,
        Err(err) => {
            warnings.push(format!("Failed to load consensus synthesis: {}", err));
            None
        }
    };

    let mut present_agents: HashSet<String> = HashSet::new();
    let mut aggregator_summary: Option<Value> = None;
    let mut aggregator_version: Option<String> = None;
    let mut aggregator_agent: Option<String> = None;
    let mut agreements: Vec<String> = Vec::new();
    let mut conflicts: Vec<String> = Vec::new();
    let mut required_fields_ok = false;

    for artifact in &artifacts {
        // ARCH-006: Use SpecAgent enum for type safety
        let agent_enum = crate::spec_prompts::SpecAgent::from_string(&artifact.agent);
        if let Some(agent) = agent_enum {
            present_agents.insert(agent.canonical_name().to_string());

            if matches!(agent, crate::spec_prompts::SpecAgent::GptPro) {
                let consensus_node = artifact
                    .content
                    .get("consensus")
                    .cloned()
                    .unwrap_or(Value::Null);
                agreements = extract_string_list(consensus_node.get("agreements"));
                conflicts = extract_string_list(consensus_node.get("conflicts"));
                required_fields_ok = validate_required_fields(stage, &artifact.content);
                aggregator_summary = Some(artifact.content.clone());
                aggregator_version = artifact.version.clone();
                aggregator_agent = Some(artifact.agent.clone());
            }
        } else {
            // Unknown agent name - keep old behavior (insert as-is)
            present_agents.insert(artifact.agent.to_ascii_lowercase());
        }
    }

    // ARCH-006: Use SpecAgent enum for expected agents
    let expected_agents = expected_agents_for_stage(stage);
    let expected_agent_names: Vec<String> = expected_agents
        .iter()
        .map(|agent| agent.canonical_name().to_string())
        .collect();

    let mut missing_agents: Vec<String> = expected_agent_names
        .into_iter()
        .filter(|agent| !present_agents.contains(agent))
        .collect();

    if aggregator_summary.is_none() {
        required_fields_ok = false;
    }

    let mut synthesis_evidence_path: Option<PathBuf> = None;
    let mut prompt_version =
        crate::spec_prompts::stage_version_enum(stage).unwrap_or_else(|| "unversioned".to_string());
    let has_conflict;
    let degraded;
    let consensus_ok;

    if let Some(summary) = &synthesis_summary {
        synthesis_evidence_path = Some(summary.path.clone());
        if let Some(version) = &summary.prompt_version {
            if !version.trim().is_empty() {
                prompt_version = version.clone();
            }
        }
        agreements = summary.agreements.clone();
        conflicts = summary.conflicts.clone();
        missing_agents = summary.missing_agents.clone();
        has_conflict = summary.status.eq_ignore_ascii_case("conflict") || !conflicts.is_empty();
        degraded = summary.status.eq_ignore_ascii_case("degraded")
            || (!missing_agents.is_empty() && !has_conflict);
        consensus_ok = summary.status.eq_ignore_ascii_case("ok");
    } else {
        has_conflict = !conflicts.is_empty();
        degraded = aggregator_summary.is_none() || !missing_agents.is_empty();
        consensus_ok = !aggregator_summary.is_none()
            && conflicts.is_empty()
            && missing_agents.is_empty()
            && required_fields_ok;
    }

    let consensus_ok = if consensus_ok { true } else { false };
    let has_conflict = if consensus_ok { false } else { has_conflict };
    let degraded = if consensus_ok { false } else { degraded };

    missing_agents.sort_unstable();
    missing_agents.dedup();
    conflicts.sort_unstable();
    conflicts.dedup();

    let consensus_status = if consensus_ok {
        "ok"
    } else if has_conflict {
        "conflict"
    } else if degraded {
        "degraded"
    } else {
        "unknown"
    };
    let consensus_status = consensus_status.to_string();

    let mut lines: Vec<ratatui::text::Line<'static>> = Vec::new();
    let status_label = if consensus_ok {
        "CONSENSUS OK"
    } else if has_conflict {
        "CONSENSUS CONFLICT"
    } else if degraded {
        "CONSENSUS DEGRADED"
    } else {
        "CONSENSUS UNKNOWN"
    };
    lines.push(ratatui::text::Line::from(format!(
        "[Spec Consensus] {} {} — {}",
        stage.display_name(),
        spec_id,
        status_label
    )));
    lines.push(ratatui::text::Line::from(format!(
        "  Prompt version: {}",
        prompt_version
    )));

    for warning in warnings.drain(..) {
        lines.push(ratatui::text::Line::from(format!("  Warning: {warning}")));
    }

    if let Some(path) = synthesis_evidence_path.as_ref() {
        lines.push(ratatui::text::Line::from(format!(
            "  Synthesis: {}",
            path.display()
        )));
    }

    if !missing_agents.is_empty() {
        lines.push(ratatui::text::Line::from(format!(
            "  Missing agents: {}",
            missing_agents.join(", ")
        )));
    }

    if aggregator_summary.is_none() {
        lines.push(ratatui::text::Line::from(
            "  Aggregator (gpt_pro) summary not found in local-memory.",
        ));
    }

    if !agreements.is_empty() {
        lines.push(ratatui::text::Line::from(format!(
            "  Agreements: {}",
            agreements.join("; ")
        )));
    }

    if !conflicts.is_empty() {
        lines.push(ratatui::text::Line::from(format!(
            "  Conflicts: {}",
            conflicts.join("; ")
        )));
    }

    lines.push(ratatui::text::Line::from(format!(
        "  Artifacts: {} agent(s)",
        present_agents.len()
    )));

    // Persistence: Write verdict, telemetry, and remember in local-memory
    if telemetry_enabled {
        let evidence_slug = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let verdict_obj = ConsensusVerdict {
            spec_id: spec_id.to_string(),
            stage: stage.command_name().to_string(),
            recorded_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            prompt_version: Some(prompt_version.clone()),
            consensus_ok,
            degraded,
            required_fields_ok,
            missing_agents: missing_agents.clone(),
            agreements: agreements.clone(),
            conflicts: conflicts.clone(),
            aggregator_agent: aggregator_agent.clone(),
            aggregator_version: aggregator_version.clone(),
            aggregator: aggregator_summary.clone(),
            synthesis_path: synthesis_evidence_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            artifacts: artifacts
                .iter()
                .map(|artifact| ConsensusArtifactVerdict {
                    memory_id: artifact.memory_id.clone(),
                    agent: artifact.agent.clone(),
                    version: artifact.version.clone(),
                    content: artifact.content.clone(),
                })
                .collect(),
        };

        match persist_consensus_verdict(cwd, spec_id, stage, &verdict_obj) {
            Ok(verdict_path) => {
                lines.push(ratatui::text::Line::from(format!(
                    "  Evidence: {}",
                    verdict_path.display()
                )));

                let verdict_handle = ConsensusEvidenceHandle {
                    path: verdict_path.clone(),
                    sha256: String::new(), // Not computing hash for now
                };

                // Persist telemetry bundle
                match persist_consensus_telemetry_bundle(
                    cwd,
                    spec_id,
                    stage,
                    &verdict_obj,
                    &verdict_handle,
                    &evidence_slug,
                    &consensus_status,
                ) {
                    Ok(_paths) => {
                        // Success - telemetry written
                    }
                    Err(err) => {
                        lines.push(ratatui::text::Line::from(format!(
                            "  Warning: failed to persist telemetry bundle: {}",
                            err
                        )));
                    }
                }

                // Remember in local-memory
                if let Err(err) = remember_consensus_verdict(spec_id, stage, &verdict_obj, mcp_manager).await {
                    lines.push(ratatui::text::Line::from(format!(
                        "  Warning: failed to store in local-memory: {}",
                        err
                    )));
                }
            }
            Err(err) => {
                lines.push(ratatui::text::Line::from(format!(
                    "  Warning: failed to persist consensus evidence: {}",
                    err
                )));
            }
        }
    }

    Ok((lines, consensus_ok))
}

use std::io::Write;

/// Persist consensus verdict to evidence directory
pub(crate) fn persist_consensus_verdict(
    cwd: &Path,
    spec_id: &str,
    stage: SpecStage,
    verdict: &ConsensusVerdict,
) -> Result<PathBuf> {
    // MAINT-7: Use centralized path helper
    let consensus_dir = super::evidence::consensus_dir(cwd).join(spec_id);
    fs::create_dir_all(&consensus_dir)
        .map_err(|e| format!("Failed to create consensus directory: {e}"))?;

    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H_%M_%S%.3fZ");
    let filename = format!("{}_{}_verdict.json", stage.command_name(), timestamp);
    let path = consensus_dir.join(&filename);

    let json = serde_json::to_string_pretty(verdict)
        .map_err(|e| format!("Failed to serialize verdict: {e}"))?;

    let mut file =
        fs::File::create(&path).map_err(|e| format!("Failed to create verdict file: {e}"))?;
    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write verdict: {e}"))?;

    Ok(path)
}

/// Persist consensus telemetry bundle with artifacts
pub(crate) fn persist_consensus_telemetry_bundle(
    cwd: &Path,
    spec_id: &str,
    stage: SpecStage,
    verdict: &ConsensusVerdict,
    verdict_handle: &ConsensusEvidenceHandle,
    slug: &str,
    consensus_status: &str,
) -> Result<ConsensusTelemetryPaths> {
    // MAINT-7: Use centralized path helper
    let base = super::evidence::consensus_dir(cwd).join(spec_id);
    fs::create_dir_all(&base).map_err(|e| {
        format!(
            "failed to create consensus evidence directory {}: {}",
            base.display(),
            e
        )
    })?;

    let stage_name = stage.command_name();

    let to_relative = |path: &Path| -> String {
        path.strip_prefix(cwd)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    };

    // Write individual agent artifacts
    let mut agent_paths: Vec<PathBuf> = Vec::new();
    for artifact in &verdict.artifacts {
        let agent_slug = telemetry_agent_slug(&artifact.agent);
        let filename = format!("{}_{agent_slug}_{slug}_artifact.json", stage_name);
        let agent_path = base.join(&filename);

        let json = serde_json::to_string_pretty(&artifact.content)
            .map_err(|e| format!("Failed to serialize agent artifact: {e}"))?;

        let mut file = fs::File::create(&agent_path)
            .map_err(|e| format!("Failed to create agent artifact file: {e}"))?;
        file.write_all(json.as_bytes())
            .map_err(|e| format!("Failed to write agent artifact: {e}"))?;

        agent_paths.push(agent_path);
    }

    // Write consensus telemetry bundle
    let telemetry_filename = format!("{}_{slug}_telemetry.json", stage_name);
    let telemetry_path = base.join(&telemetry_filename);

    let telemetry_bundle = serde_json::json!({
        "spec_id": spec_id,
        "stage": stage_name,
        "slug": slug,
        "consensus_status": consensus_status,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "verdict": to_relative(&verdict_handle.path),
        "artifacts": agent_paths.iter().map(|p| to_relative(p)).collect::<Vec<_>>(),
    });

    let json = serde_json::to_string_pretty(&telemetry_bundle)
        .map_err(|e| format!("Failed to serialize telemetry bundle: {e}"))?;

    let mut file = fs::File::create(&telemetry_path)
        .map_err(|e| format!("Failed to create telemetry file: {e}"))?;
    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write telemetry: {e}"))?;

    // Write synthesis metadata
    let synthesis_filename = format!("{}_{slug}_synthesis.json", stage_name);
    let synthesis_path = base.join(&synthesis_filename);

    let synthesis_data = serde_json::json!({
        "spec_id": spec_id,
        "stage": stage_name,
        "status": consensus_status,
        "missing_agents": verdict.missing_agents,
        "consensus": {
            "agreements": verdict.agreements,
            "conflicts": verdict.conflicts,
        },
        "prompt_version": verdict.prompt_version,
    });

    let json = serde_json::to_string_pretty(&synthesis_data)
        .map_err(|e| format!("Failed to serialize synthesis: {e}"))?;

    let mut file = fs::File::create(&synthesis_path)
        .map_err(|e| format!("Failed to create synthesis file: {e}"))?;
    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write synthesis: {e}"))?;

    Ok(ConsensusTelemetryPaths {
        agent_paths,
        telemetry_path,
        synthesis_path,
    })
}

/// Remember consensus verdict in local-memory
// FORK-SPECIFIC (just-every/code): Made async for native MCP
pub(crate) async fn remember_consensus_verdict(
    spec_id: &str,
    stage: SpecStage,
    verdict: &ConsensusVerdict,
    mcp_manager: &codex_core::mcp_connection_manager::McpConnectionManager,
) -> Result<()> {
    use serde_json::json;

    let mut summary_value = serde_json::json!({
        "spec_id": spec_id,
        "stage": stage.command_name(),
        "status": if verdict.consensus_ok {
            "ok"
        } else if !verdict.conflicts.is_empty() {
            "conflict"
        } else {
            "degraded"
        },
        "missing_agents": verdict.missing_agents,
        "agreements": verdict.agreements,
        "conflicts": verdict.conflicts,
    });

    if let Some(version) = &verdict.prompt_version {
        if let serde_json::Value::Object(obj) = &mut summary_value {
            obj.insert(
                "promptVersion".to_string(),
                serde_json::Value::String(version.clone()),
            );
        }
    }

    if let Some(path) = &verdict.synthesis_path {
        if let serde_json::Value::Object(obj) = &mut summary_value {
            obj.insert(
                "synthesisPath".to_string(),
                serde_json::Value::String(path.clone()),
            );
        }
    }

    let summary = serde_json::to_string(&summary_value)
        .map_err(|e| SpecKitError::JsonSerialize { source: e })?;

    // Call local-memory store_memory via MCP
    let args = json!({
        "content": summary,
        "domain": "spec-kit",
        "tags": [format!("spec:{}", spec_id), format!("stage:{}", stage.command_name()), "consensus-verdict"],
        "importance": 8
    });

    mcp_manager
        .call_tool("local-memory", "store_memory", Some(args), Some(std::time::Duration::from_secs(10)))
        .await
        .map_err(|e| SpecKitError::from_string(format!("MCP local-memory store failed: {}", e)))?;

    Ok(())
}
