//! Tests for spec-kit state management module
//!
//! Covers SpecAutoState, phase management, quality gates, and helper functions.

use codex_tui::{
    SpecAutoState, SpecAutoPhase, QualityCheckpoint, QualityGateType,
    HalMode, SlashCommand, SpecStage,
    guardrail_for_stage, spec_ops_stage_prefix, expected_guardrail_command,
    validate_guardrail_evidence, get_nested, require_string_field, require_object,
};
use serde_json::json;
use std::path::PathBuf;

// ===== SpecAutoState Construction Tests =====

#[test]
fn test_spec_auto_state_new() {
    let state = SpecAutoState::new(
        "SPEC-KIT-001".to_string(),
        "Test goal".to_string(),
        SpecStage::Plan,
        None,
    );

    assert_eq!(state.spec_id, "SPEC-KIT-001");
    assert_eq!(state.goal, "Test goal");
    assert_eq!(state.current_index, 0);
    assert_eq!(state.stages.len(), 6);
    assert!(state.quality_gates_enabled);
    assert_eq!(state.validate_retries, 0);
    assert_eq!(state.agent_retry_count, 0);
}

#[test]
fn test_spec_auto_state_with_quality_gates_disabled() {
    let state = SpecAutoState::with_quality_gates(
        "SPEC-KIT-002".to_string(),
        "Test goal".to_string(),
        SpecStage::Plan,
        None,
        false,
    );

    assert!(!state.quality_gates_enabled);
    assert_eq!(state.completed_checkpoints.len(), 0);
}

#[test]
fn test_spec_auto_state_resume_from_tasks() {
    let state = SpecAutoState::new(
        "SPEC-KIT-003".to_string(),
        "Resume test".to_string(),
        SpecStage::Tasks,
        None,
    );

    assert_eq!(state.current_index, 1); // Tasks is index 1
    assert_eq!(state.current_stage(), Some(SpecStage::Tasks));
}

#[test]
fn test_spec_auto_state_resume_from_unlock() {
    let state = SpecAutoState::new(
        "SPEC-KIT-004".to_string(),
        "Final stage".to_string(),
        SpecStage::Unlock,
        None,
    );

    assert_eq!(state.current_index, 5); // Unlock is last (index 5)
    assert_eq!(state.current_stage(), Some(SpecStage::Unlock));
}

#[test]
fn test_spec_auto_state_with_hal_mode() {
    let state = SpecAutoState::new(
        "SPEC-KIT-005".to_string(),
        "HAL test".to_string(),
        SpecStage::Plan,
        Some(HalMode::Live),
    );

    assert_eq!(state.hal_mode, Some(HalMode::Live));
}

#[test]
fn test_spec_auto_state_initial_phase_is_guardrail() {
    let state = SpecAutoState::new(
        "SPEC-KIT-006".to_string(),
        "Phase test".to_string(),
        SpecStage::Plan,
        None,
    );

    assert!(matches!(state.phase, SpecAutoPhase::Guardrail));
}

#[test]
fn test_spec_auto_state_stage_sequence() {
    let state = SpecAutoState::new(
        "SPEC-KIT-007".to_string(),
        "Sequence test".to_string(),
        SpecStage::Plan,
        None,
    );

    assert_eq!(state.stages[0], SpecStage::Plan);
    assert_eq!(state.stages[1], SpecStage::Tasks);
    assert_eq!(state.stages[2], SpecStage::Implement);
    assert_eq!(state.stages[3], SpecStage::Validate);
    assert_eq!(state.stages[4], SpecStage::Audit);
    assert_eq!(state.stages[5], SpecStage::Unlock);
}

// ===== SpecAutoState Methods Tests =====

#[test]
fn test_current_stage_returns_correct_stage() {
    let state = SpecAutoState::new(
        "SPEC-KIT-008".to_string(),
        "Current stage test".to_string(),
        SpecStage::Implement,
        None,
    );

    assert_eq!(state.current_stage(), Some(SpecStage::Implement));
}

#[test]
fn test_current_stage_returns_none_when_out_of_bounds() {
    let mut state = SpecAutoState::new(
        "SPEC-KIT-009".to_string(),
        "Bounds test".to_string(),
        SpecStage::Plan,
        None,
    );

    state.current_index = 999; // Out of bounds
    assert_eq!(state.current_stage(), None);
}

#[test]
fn test_is_executing_agents_true() {
    let mut state = SpecAutoState::new(
        "SPEC-KIT-010".to_string(),
        "Executing test".to_string(),
        SpecStage::Plan,
        None,
    );

    state.phase = SpecAutoPhase::ExecutingAgents {
        expected_agents: vec!["gemini".to_string(), "claude".to_string()],
        completed_agents: Default::default(),
    };

    assert!(state.is_executing_agents());
}

#[test]
fn test_is_executing_agents_false() {
    let state = SpecAutoState::new(
        "SPEC-KIT-011".to_string(),
        "Not executing test".to_string(),
        SpecStage::Plan,
        None,
    );

    assert!(!state.is_executing_agents());
}

// ===== QualityCheckpoint Tests =====

#[test]
fn test_quality_checkpoint_names() {
    assert_eq!(QualityCheckpoint::PrePlanning.name(), "pre-planning");
    assert_eq!(QualityCheckpoint::PostPlan.name(), "post-plan");
    assert_eq!(QualityCheckpoint::PostTasks.name(), "post-tasks");
}

#[test]
fn test_quality_checkpoint_gates() {
    let pre_planning = QualityCheckpoint::PrePlanning.gates();
    assert_eq!(pre_planning.len(), 2);
    assert!(pre_planning.contains(&QualityGateType::Clarify));
    assert!(pre_planning.contains(&QualityGateType::Checklist));

    let post_plan = QualityCheckpoint::PostPlan.gates();
    assert_eq!(post_plan.len(), 1);
    assert!(post_plan.contains(&QualityGateType::Analyze));

    let post_tasks = QualityCheckpoint::PostTasks.gates();
    assert_eq!(post_tasks.len(), 1);
    assert!(post_tasks.contains(&QualityGateType::Analyze));
}

// ===== QualityGateType Tests =====

#[test]
fn test_quality_gate_command_names() {
    assert_eq!(QualityGateType::Clarify.command_name(), "clarify");
    assert_eq!(QualityGateType::Checklist.command_name(), "checklist");
    assert_eq!(QualityGateType::Analyze.command_name(), "analyze");
}

// ===== Helper Function Tests =====

#[test]
fn test_guardrail_for_stage() {
    assert_eq!(guardrail_for_stage(SpecStage::Plan), SlashCommand::SpecOpsPlan);
    assert_eq!(guardrail_for_stage(SpecStage::Tasks), SlashCommand::SpecOpsTasks);
    assert_eq!(guardrail_for_stage(SpecStage::Implement), SlashCommand::SpecOpsImplement);
    assert_eq!(guardrail_for_stage(SpecStage::Validate), SlashCommand::SpecOpsValidate);
    assert_eq!(guardrail_for_stage(SpecStage::Audit), SlashCommand::SpecOpsAudit);
    assert_eq!(guardrail_for_stage(SpecStage::Unlock), SlashCommand::SpecOpsUnlock);
}

#[test]
fn test_spec_ops_stage_prefix() {
    assert_eq!(spec_ops_stage_prefix(SpecStage::Plan), "plan_");
    assert_eq!(spec_ops_stage_prefix(SpecStage::Tasks), "tasks_");
    assert_eq!(spec_ops_stage_prefix(SpecStage::Implement), "implement_");
    assert_eq!(spec_ops_stage_prefix(SpecStage::Validate), "validate_");
    assert_eq!(spec_ops_stage_prefix(SpecStage::Audit), "audit_");
    assert_eq!(spec_ops_stage_prefix(SpecStage::Unlock), "unlock_");
}

#[test]
fn test_expected_guardrail_command() {
    assert_eq!(expected_guardrail_command(SpecStage::Plan), "spec-ops-plan");
    assert_eq!(expected_guardrail_command(SpecStage::Tasks), "spec-ops-tasks");
    assert_eq!(expected_guardrail_command(SpecStage::Implement), "spec-ops-implement");
    assert_eq!(expected_guardrail_command(SpecStage::Validate), "spec-ops-validate");
    assert_eq!(expected_guardrail_command(SpecStage::Audit), "spec-ops-audit");
    assert_eq!(expected_guardrail_command(SpecStage::Unlock), "spec-ops-unlock");
}

// ===== JSON Helper Tests =====

#[test]
fn test_get_nested_valid_path() {
    let json = json!({
        "level1": {
            "level2": {
                "value": "found"
            }
        }
    });

    let result = get_nested(&json, &["level1", "level2", "value"]);
    assert_eq!(result, Some(&json!("found")));
}

#[test]
fn test_get_nested_invalid_path() {
    let json = json!({
        "level1": {
            "level2": "value"
        }
    });

    let result = get_nested(&json, &["level1", "nonexistent", "value"]);
    assert_eq!(result, None);
}

#[test]
fn test_require_string_field_valid() {
    let json = json!({
        "field": "valid value"
    });

    let mut errors = Vec::new();
    let result = require_string_field(&json, &["field"], &mut errors);

    assert_eq!(result, Some("valid value"));
    assert!(errors.is_empty());
}

#[test]
fn test_require_string_field_missing() {
    let json = json!({});

    let mut errors = Vec::new();
    let result = require_string_field(&json, &["field"], &mut errors);

    assert_eq!(result, None);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Missing required string field"));
}

#[test]
fn test_require_string_field_empty() {
    let json = json!({
        "field": "   "
    });

    let mut errors = Vec::new();
    let result = require_string_field(&json, &["field"], &mut errors);

    assert_eq!(result, None);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("must be a non-empty string"));
}

#[test]
fn test_require_object_valid() {
    let json = json!({
        "obj": {
            "key": "value"
        }
    });

    let mut errors = Vec::new();
    let result = require_object(&json, &["obj"], &mut errors);

    assert!(result.is_some());
    assert!(errors.is_empty());
}

#[test]
fn test_require_object_missing() {
    let json = json!({});

    let mut errors = Vec::new();
    let result = require_object(&json, &["obj"], &mut errors);

    assert_eq!(result, None);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("Missing required object field"));
}

// ===== Validation Tests =====

#[test]
fn test_validate_guardrail_evidence_validate_stage_skips() {
    let cwd = PathBuf::from("/test");
    let telemetry = json!({});

    let (failures, ok_count) = validate_guardrail_evidence(&cwd, SpecStage::Validate, &telemetry);

    assert_eq!(failures.len(), 0);
    assert_eq!(ok_count, 0);
}

#[test]
fn test_validate_guardrail_evidence_no_artifacts() {
    let cwd = PathBuf::from("/test");
    let telemetry = json!({});

    let (failures, ok_count) = validate_guardrail_evidence(&cwd, SpecStage::Plan, &telemetry);

    assert_eq!(failures.len(), 1);
    assert!(failures[0].contains("No evidence artifacts recorded"));
    assert_eq!(ok_count, 0);
}

#[test]
fn test_validate_guardrail_evidence_empty_array() {
    let cwd = PathBuf::from("/test");
    let telemetry = json!({
        "artifacts": []
    });

    let (failures, ok_count) = validate_guardrail_evidence(&cwd, SpecStage::Plan, &telemetry);

    assert_eq!(failures.len(), 1);
    assert!(failures[0].contains("artifacts array is empty"));
    assert_eq!(ok_count, 0);
}
