//! State management for spec-kit automation
//!
//! Extracted from chatwidget.rs to isolate spec-kit code from upstream

use crate::slash_command::{HalMode, SlashCommand};
use crate::spec_prompts::SpecStage;
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;

/// Phase tracking for /speckit.auto pipeline
#[derive(Debug, Clone)]
pub enum SpecAutoPhase {
    Guardrail,
    ExecutingAgents {
        // Track which agents we're waiting for completion
        expected_agents: Vec<String>,
        // Track which agents have completed (populated from AgentStatusUpdateEvent)
        completed_agents: HashSet<String>,
    },
    CheckingConsensus,
}

/// Waiting state for guardrail execution
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GuardrailWait {
    pub stage: SpecStage,
    pub command: SlashCommand,
    pub task_id: Option<String>,
}

/// State for /speckit.auto pipeline automation
#[derive(Debug, Clone)]
pub struct SpecAutoState {
    pub spec_id: String,
    pub goal: String,
    pub stages: Vec<SpecStage>,
    pub current_index: usize,
    pub phase: SpecAutoPhase,
    pub waiting_guardrail: Option<GuardrailWait>,
    pub validate_retries: u32,
    pub pending_prompt_summary: Option<String>,
    pub hal_mode: Option<HalMode>,
}

impl SpecAutoState {
    #[allow(dead_code)]
    pub fn new(
        spec_id: String,
        goal: String,
        resume_from: SpecStage,
        hal_mode: Option<HalMode>,
    ) -> Self {
        let stages = vec![
            SpecStage::Plan,
            SpecStage::Tasks,
            SpecStage::Implement,
            SpecStage::Validate,
            SpecStage::Audit,
            SpecStage::Unlock,
        ];
        let start_index = stages
            .iter()
            .position(|stage| *stage == resume_from)
            .unwrap_or(0);
        Self {
            spec_id,
            goal,
            stages,
            current_index: start_index,
            phase: SpecAutoPhase::Guardrail,
            waiting_guardrail: None,
            validate_retries: 0,
            pending_prompt_summary: None,
            hal_mode,
        }
    }

    pub fn current_stage(&self) -> Option<SpecStage> {
        self.stages.get(self.current_index).copied()
    }

    #[allow(dead_code)]
    pub fn is_executing_agents(&self) -> bool {
        matches!(self.phase, SpecAutoPhase::ExecutingAgents { .. })
    }
}

/// Guardrail evaluation result
pub struct GuardrailEvaluation {
    pub success: bool,
    pub summary: String,
    pub failures: Vec<String>,
}

/// Guardrail outcome with telemetry
#[derive(Debug, Clone)]
pub struct GuardrailOutcome {
    pub success: bool,
    pub summary: String,
    pub telemetry_path: Option<PathBuf>,
    pub failures: Vec<String>,
}

// === Helper Functions ===

pub fn guardrail_for_stage(stage: SpecStage) -> SlashCommand {
    match stage {
        SpecStage::Plan => SlashCommand::SpecOpsPlan,
        SpecStage::Tasks => SlashCommand::SpecOpsTasks,
        SpecStage::Implement => SlashCommand::SpecOpsImplement,
        SpecStage::Validate => SlashCommand::SpecOpsValidate,
        SpecStage::Audit => SlashCommand::SpecOpsAudit,
        SpecStage::Unlock => SlashCommand::SpecOpsUnlock,
    }
}

pub fn spec_ops_stage_prefix(stage: SpecStage) -> &'static str {
    match stage {
        SpecStage::Plan => "plan_",
        SpecStage::Tasks => "tasks_",
        SpecStage::Implement => "implement_",
        SpecStage::Validate => "validate_",
        SpecStage::Audit => "audit_",
        SpecStage::Unlock => "unlock_",
    }
}

pub fn expected_guardrail_command(stage: SpecStage) -> &'static str {
    match stage {
        SpecStage::Plan => "spec-ops-plan",
        SpecStage::Tasks => "spec-ops-tasks",
        SpecStage::Implement => "spec-ops-implement",
        SpecStage::Validate => "spec-ops-validate",
        SpecStage::Audit => "spec-ops-audit",
        SpecStage::Unlock => "spec-ops-unlock",
    }
}

/// Validate that guardrail evidence artifacts exist on disk
pub fn validate_guardrail_evidence(
    cwd: &std::path::Path,
    stage: SpecStage,
    telemetry: &Value,
) -> (Vec<String>, usize) {
    if matches!(stage, SpecStage::Validate) {
        return (Vec::new(), 0);
    }

    let Some(artifacts_value) = telemetry.get("artifacts") else {
        return (vec!["No evidence artifacts recorded".to_string()], 0);
    };
    let Some(artifacts) = artifacts_value.as_array() else {
        return (
            vec!["Telemetry artifacts field is not an array".to_string()],
            0,
        );
    };
    if artifacts.is_empty() {
        return (vec!["Telemetry artifacts array is empty".to_string()], 0);
    }

    let mut failures = Vec::new();
    let mut ok_count = 0usize;
    for (idx, artifact_value) in artifacts.iter().enumerate() {
        let path_opt = match artifact_value {
            Value::String(s) => Some(s.as_str()),
            Value::Object(map) => map.get("path").and_then(|p| p.as_str()),
            _ => None,
        };
        let Some(path_str) = path_opt else {
            failures.push(format!("Artifact #{} missing path", idx + 1));
            continue;
        };

        let raw_path = PathBuf::from(path_str);
        let resolved = if raw_path.is_absolute() {
            raw_path.clone()
        } else {
            cwd.join(&raw_path)
        };
        if resolved.exists() {
            ok_count += 1;
        } else {
            failures.push(format!(
                "Artifact #{} not found at {}",
                idx + 1,
                resolved.display()
            ));
        }
    }

    if ok_count == 0 {
        failures.push("No evidence artifacts found on disk".to_string());
    }

    (failures, ok_count)
}

/// Get nested value from JSON object
pub fn get_nested<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = root;
    for segment in path {
        current = current.get(*segment)?;
    }
    Some(current)
}

/// Require a non-empty string field from JSON, adding error if missing
pub fn require_string_field<'a>(
    root: &'a Value,
    path: &[&str],
    errors: &mut Vec<String>,
) -> Option<&'a str> {
    let label = path.join(".");
    match get_nested(root, path).and_then(|value| value.as_str()) {
        Some(value) if !value.trim().is_empty() => Some(value),
        Some(_) => {
            errors.push(format!("Field {label} must be a non-empty string"));
            None
        }
        None => {
            errors.push(format!("Missing required string field {label}"));
            None
        }
    }
}

/// Require an object field from JSON, adding error if missing
pub fn require_object<'a>(
    root: &'a Value,
    path: &[&str],
    errors: &mut Vec<String>,
) -> Option<&'a serde_json::Map<String, Value>> {
    let label = path.join(".");
    match get_nested(root, path).and_then(|value| value.as_object()) {
        Some(map) => Some(map),
        None => {
            errors.push(format!("Missing required object field {label}"));
            None
        }
    }
}
