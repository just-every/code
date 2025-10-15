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
