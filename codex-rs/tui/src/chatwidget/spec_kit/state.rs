//! State management for spec-kit automation
//!
//! Extracted from chatwidget.rs to isolate spec-kit code from upstream

use crate::slash_command::{HalMode, SlashCommand};
use crate::spec_prompts::SpecStage;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
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

    // === Quality Gate Phases (T85) ===
    /// Executing quality gate agents
    QualityGateExecuting {
        checkpoint: QualityCheckpoint,
        gates: Vec<QualityGateType>,
        active_gates: HashSet<QualityGateType>,
        expected_agents: Vec<String>,
        completed_agents: HashSet<String>,
        results: HashMap<String, Value>,  // agent_id -> JSON result
    },

    /// Processing quality gate results (classification)
    QualityGateProcessing {
        checkpoint: QualityCheckpoint,
        auto_resolved: Vec<QualityIssue>,
        escalated: Vec<QualityIssue>,
    },

    /// Validating 2/3 majority answers with GPT-5 (async via agent system)
    QualityGateValidating {
        checkpoint: QualityCheckpoint,
        auto_resolved: Vec<QualityIssue>,  // Unanimous issues already resolved
        pending_validations: Vec<(QualityIssue, String)>,  // (issue, majority_answer)
        completed_validations: HashMap<usize, GPT5ValidationResult>,  // index -> validation result
    },

    /// Awaiting human answers for escalated questions
    QualityGateAwaitingHuman {
        checkpoint: QualityCheckpoint,
        escalated_issues: Vec<QualityIssue>,  // Store original issues
        escalated_questions: Vec<EscalatedQuestion>,  // For UI display
        answers: HashMap<String, String>,  // question_id -> human_answer
    },
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
#[derive(Debug)]
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

    // === Quality Gate State (T85) ===
    pub quality_gates_enabled: bool,
    pub completed_checkpoints: HashSet<QualityCheckpoint>,
    pub quality_modifications: Vec<String>,  // Track files modified by quality gates
    pub quality_auto_resolved: Vec<(QualityIssue, String)>,  // All auto-resolutions
    pub quality_escalated: Vec<(QualityIssue, String)>,  // All human-answered questions
    pub quality_checkpoint_outcomes: Vec<(QualityCheckpoint, usize, usize)>,  // (checkpoint, auto, escalated)

    // === Agent Lifecycle (T88) ===
    #[allow(dead_code)]  // Will be used when agent spawning is integrated
    pub agent_lifecycle: Option<super::agent_lifecycle::AgentLifecycleManager>,
}

impl SpecAutoState {
    #[allow(dead_code)]
    pub fn new(
        spec_id: String,
        goal: String,
        resume_from: SpecStage,
        hal_mode: Option<HalMode>,
    ) -> Self {
        Self::with_quality_gates(spec_id, goal, resume_from, hal_mode, true)
    }

    pub fn with_quality_gates(
        spec_id: String,
        goal: String,
        resume_from: SpecStage,
        hal_mode: Option<HalMode>,
        quality_gates_enabled: bool,
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

        // Always start with Guardrail phase
        // Quality checkpoints will be triggered by advance_spec_auto when needed
        let initial_phase = SpecAutoPhase::Guardrail;

        Self {
            spec_id,
            goal,
            stages,
            current_index: start_index,
            phase: initial_phase,
            waiting_guardrail: None,
            validate_retries: 0,
            pending_prompt_summary: None,
            hal_mode,
            quality_gates_enabled,
            completed_checkpoints: HashSet::new(),
            quality_modifications: Vec::new(),
            quality_auto_resolved: Vec::new(),
            quality_escalated: Vec::new(),
            quality_checkpoint_outcomes: Vec::new(),
            // T88: Agent lifecycle manager (automatic cleanup via Drop)
            agent_lifecycle: None,
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

// === Quality Gate Types (T85) ===

/// Quality checkpoint in the pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QualityCheckpoint {
    /// Before planning (runs clarify + checklist)
    PrePlanning,
    /// After plan created (runs analyze)
    PostPlan,
    /// After tasks created (runs analyze)
    PostTasks,
}

impl QualityCheckpoint {
    pub fn name(&self) -> &'static str {
        match self {
            Self::PrePlanning => "pre-planning",
            Self::PostPlan => "post-plan",
            Self::PostTasks => "post-tasks",
        }
    }

    pub fn gates(&self) -> &[QualityGateType] {
        match self {
            Self::PrePlanning => &[QualityGateType::Clarify, QualityGateType::Checklist],
            Self::PostPlan => &[QualityGateType::Analyze],
            Self::PostTasks => &[QualityGateType::Analyze],
        }
    }
}

/// Type of quality gate
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QualityGateType {
    /// Identify and resolve ambiguities
    Clarify,
    /// Score and improve requirements
    Checklist,
    /// Check consistency across artifacts
    Analyze,
}

impl QualityGateType {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Clarify => "clarify",
            Self::Checklist => "checklist",
            Self::Analyze => "analyze",
        }
    }
}

/// Agent confidence level (derived from agreement)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// All agents agree (3/3)
    High,
    /// Majority agree (2/3)
    Medium,
    /// No consensus (0-1/3)
    Low,
}

/// Issue magnitude/severity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Magnitude {
    /// Blocks progress, affects core functionality
    Critical,
    /// Significant but not blocking
    Important,
    /// Nice-to-have, cosmetic, minor
    Minor,
}

/// Whether agents can resolve the issue
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolvability {
    /// Straightforward fix, apply immediately
    AutoFix,
    /// Fix available but needs validation
    SuggestFix,
    /// Requires human judgment
    NeedHuman,
}

/// Quality issue identified by agents
#[derive(Debug, Clone)]
pub struct QualityIssue {
    pub id: String,
    pub gate_type: QualityGateType,
    pub issue_type: String,
    pub description: String,
    pub confidence: Confidence,
    pub magnitude: Magnitude,
    pub resolvability: Resolvability,
    pub suggested_fix: Option<String>,
    pub context: String,
    pub affected_artifacts: Vec<String>,
    pub agent_answers: HashMap<String, String>,
    pub agent_reasoning: HashMap<String, String>,
}

/// GPT-5 validation result for majority answers
#[derive(Debug, Clone)]
pub struct GPT5ValidationResult {
    pub agrees_with_majority: bool,
    pub reasoning: String,
    pub recommended_answer: Option<String>,
    pub confidence: Confidence,
}

/// Resolution decision for a quality issue
#[derive(Debug, Clone)]
pub enum Resolution {
    /// Auto-apply the answer
    AutoApply {
        answer: String,
        confidence: Confidence,
        reason: String,
        validation: Option<GPT5ValidationResult>,
    },
    /// Escalate to human
    Escalate {
        reason: String,
        all_answers: HashMap<String, String>,
        gpt5_reasoning: Option<String>,
        recommended: Option<String>,
    },
}

/// Escalated question requiring human input
#[derive(Debug, Clone)]
pub struct EscalatedQuestion {
    pub id: String,
    pub gate_type: QualityGateType,
    pub question: String,
    pub context: String,
    pub agent_answers: HashMap<String, String>,
    pub gpt5_reasoning: Option<String>,
    pub magnitude: Magnitude,
    pub suggested_options: Vec<String>,
}

/// Outcome of a quality checkpoint (one or more gates)
#[derive(Debug, Clone)]
pub struct QualityCheckpointOutcome {
    pub checkpoint: QualityCheckpoint,
    pub total_issues: usize,
    pub auto_resolved: usize,
    pub escalated: usize,
    pub escalated_questions: Vec<EscalatedQuestion>,
    pub auto_resolutions: Vec<(QualityIssue, String)>,  // (issue, applied_answer)
    pub telemetry_path: Option<PathBuf>,
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

use codex_core::config_types::ShellEnvironmentPolicy;

/// Check if spec-kit telemetry is enabled via env or config
pub fn spec_kit_telemetry_enabled(env_policy: &ShellEnvironmentPolicy) -> bool {
    if let Ok(value) = std::env::var("SPEC_KIT_TELEMETRY_ENABLED") {
        if super::consensus::telemetry_value_truthy(&value) {
            return true;
        }
    }

    if let Some(value) = env_policy.r#set.get("SPEC_KIT_TELEMETRY_ENABLED") {
        if super::consensus::telemetry_value_truthy(value) {
            return true;
        }
    }

    false
}
