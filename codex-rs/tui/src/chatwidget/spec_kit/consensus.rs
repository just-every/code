//! Consensus checking infrastructure for multi-agent spec-kit automation
//!
//! This module handles consensus validation across multiple AI agents,
//! artifact collection from local-memory, and synthesis result persistence.

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
pub(in super::super) fn expected_agents_for_stage(stage: SpecStage) -> Vec<&'static str> {
    match stage {
        SpecStage::Implement => vec!["gemini", "claude", "gpt_codex", "gpt_pro"],
        _ => vec!["gemini", "claude", "gpt_pro"],
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

/// Collect consensus artifacts from evidence files or local-memory
pub fn collect_consensus_artifacts(
    evidence_root: &Path,
    spec_id: &str,
    stage: SpecStage,
) -> Result<(Vec<ConsensusArtifactData>, Vec<String>), String> {
    let mut warnings: Vec<String> = Vec::new();

    match load_artifacts_from_evidence(evidence_root, spec_id, stage) {
        Ok(Some((artifacts, mut evidence_warnings))) => {
            warnings.append(&mut evidence_warnings);
            return Ok((artifacts, warnings));
        }
        Ok(None) => {}
        Err(err) => warnings.push(err),
    }

    let (entries, mut memory_warnings) = fetch_memory_entries(spec_id, stage)?;
    warnings.append(&mut memory_warnings);

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

    Ok((artifacts, warnings))
}

fn load_artifacts_from_evidence(
    evidence_root: &Path,
    spec_id: &str,
    stage: SpecStage,
) -> Result<Option<(Vec<ConsensusArtifactData>, Vec<String>)>, String> {
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
    let mut warnings: Vec<String> = Vec::new();

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
            format!("Failed to read consensus artifact {}: {}", path.display(), e)
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

fn fetch_memory_entries(
    spec_id: &str,
    stage: SpecStage,
) -> Result<(Vec<LocalMemorySearchResult>, Vec<String>), String> {
    let results = local_memory_util::search_by_stage(spec_id, stage.command_name(), 20)?;
    if results.is_empty() {
        // TODO: After Oct 2 migration, implement Byterover fallback fetching here and persist results into local-memory.
        Err(format!(
            "No local-memory entries found for {} stage '{}'",
            spec_id,
            stage.command_name()
        ))
    } else {
        Ok((results, Vec::new()))
    }
}

/// Load latest consensus synthesis file for spec/stage
pub fn load_latest_consensus_synthesis(
    cwd: &Path,
    spec_id: &str,
    stage: SpecStage,
) -> Result<Option<ConsensusSynthesisSummary>, String> {
    let base = cwd
        .join("docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus")
        .join(spec_id);
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
            ));
        }
    }

    if let Some(raw_spec) = raw.spec_id.as_deref() {
        if !raw_spec.eq_ignore_ascii_case(spec_id) {
            return Err(format!(
                "Consensus synthesis spec mismatch: expected {}, found {}",
                spec_id, raw_spec
            ));
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
