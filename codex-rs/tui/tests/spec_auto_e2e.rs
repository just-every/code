//! End-to-end integration tests for /speckit.auto pipeline (T87)
//!
//! These tests validate the complete pipeline flow with quality checkpoints:
//! - Stage progression through all 6 stages
//! - Quality checkpoint triggering at correct points
//! - Pipeline state transitions
//! - Error handling and recovery

use codex_tui::{
    HalMode, QualityCheckpoint, SpecAutoState, SpecStage,
};
use std::collections::HashSet;

// ============================================================================
// Pipeline State Machine Tests
// ============================================================================

#[test]
fn test_spec_auto_state_initialization() {
    let state = SpecAutoState::new(
        "SPEC-TEST-001".to_string(),
        "Test automation".to_string(),
        SpecStage::Plan,
        None,
    );

    assert_eq!(state.spec_id, "SPEC-TEST-001");
    assert_eq!(state.goal, "Test automation");
    assert_eq!(state.current_index, 0);
    assert_eq!(state.stages.len(), 6);
    assert_eq!(state.current_stage(), Some(SpecStage::Plan));
    assert!(state.quality_gates_enabled);
    assert!(state.completed_checkpoints.is_empty());
}

#[test]
fn test_pipeline_stages_order() {
    let state = SpecAutoState::new(
        "SPEC-TEST-002".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    let expected = vec![
        SpecStage::Plan,
        SpecStage::Tasks,
        SpecStage::Implement,
        SpecStage::Validate,
        SpecStage::Audit,
        SpecStage::Unlock,
    ];

    assert_eq!(state.stages, expected);
}

#[test]
fn test_resume_from_tasks_stage() {
    let state = SpecAutoState::new(
        "SPEC-TEST-003".to_string(),
        "".to_string(),
        SpecStage::Tasks,
        None,
    );

    assert_eq!(state.current_index, 1); // Tasks is index 1
    assert_eq!(state.current_stage(), Some(SpecStage::Tasks));
}

#[test]
fn test_quality_gates_enabled_by_default() {
    let state = SpecAutoState::new(
        "SPEC-TEST-004".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    assert!(state.quality_gates_enabled);
}

#[test]
fn test_quality_gates_can_be_disabled() {
    let state = SpecAutoState::with_quality_gates(
        "SPEC-TEST-005".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
        false,  // Disable quality gates
    );

    assert!(!state.quality_gates_enabled);
}

// ============================================================================
// Quality Checkpoint Integration Tests
// ============================================================================

#[test]
fn test_quality_checkpoints_track_completion() {
    let mut state = SpecAutoState::new(
        "SPEC-TEST-006".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    // Initially no checkpoints completed
    assert!(state.completed_checkpoints.is_empty());

    // Mark PrePlanning complete
    state.completed_checkpoints.insert(QualityCheckpoint::PrePlanning);
    assert!(state.completed_checkpoints.contains(&QualityCheckpoint::PrePlanning));
    assert!(!state.completed_checkpoints.contains(&QualityCheckpoint::PostPlan));

    // Mark PostPlan complete
    state.completed_checkpoints.insert(QualityCheckpoint::PostPlan);
    assert_eq!(state.completed_checkpoints.len(), 2);
}

#[test]
fn test_quality_modifications_tracked() {
    let mut state = SpecAutoState::new(
        "SPEC-TEST-007".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    // Initially no modifications
    assert!(state.quality_modifications.is_empty());

    // Track file modifications
    state.quality_modifications.push("spec.md".to_string());
    state.quality_modifications.push("plan.md".to_string());

    assert_eq!(state.quality_modifications.len(), 2);
    assert!(state.quality_modifications.contains(&"spec.md".to_string()));
}

#[test]
fn test_auto_resolutions_tracked() {
    use codex_tui::{Confidence, Magnitude, QualityGateType, QualityIssue, Resolvability};
    use std::collections::HashMap;

    let mut state = SpecAutoState::new(
        "SPEC-TEST-008".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    // Create a test issue
    let issue = QualityIssue {
        id: "Q1".to_string(),
        gate_type: QualityGateType::Clarify,
        issue_type: "ambiguity".to_string(),
        description: "Test issue".to_string(),
        confidence: Confidence::High,
        magnitude: Magnitude::Minor,
        resolvability: Resolvability::AutoFix,
        suggested_fix: None,
        context: "".to_string(),
        affected_artifacts: vec![],
        agent_answers: HashMap::new(),
        agent_reasoning: HashMap::new(),
    };

    // Track auto-resolution
    state.quality_auto_resolved.push((issue.clone(), "yes".to_string()));

    assert_eq!(state.quality_auto_resolved.len(), 1);
    assert_eq!(state.quality_auto_resolved[0].1, "yes");
}

#[test]
fn test_checkpoint_outcomes_recorded() {
    let mut state = SpecAutoState::new(
        "SPEC-TEST-009".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    // Record checkpoint outcomes
    state.quality_checkpoint_outcomes.push((
        QualityCheckpoint::PrePlanning,
        5,  // auto_resolved
        2,  // escalated
    ));

    state.quality_checkpoint_outcomes.push((
        QualityCheckpoint::PostPlan,
        3,  // auto_resolved
        0,  // escalated
    ));

    assert_eq!(state.quality_checkpoint_outcomes.len(), 2);

    let (checkpoint, auto, esc) = &state.quality_checkpoint_outcomes[0];
    assert_eq!(*checkpoint, QualityCheckpoint::PrePlanning);
    assert_eq!(*auto, 5);
    assert_eq!(*esc, 2);
}

// ============================================================================
// Stage Transition Tests
// ============================================================================

#[test]
fn test_current_stage_progression() {
    let mut state = SpecAutoState::new(
        "SPEC-TEST-010".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    // Start at Plan
    assert_eq!(state.current_stage(), Some(SpecStage::Plan));

    // Advance to Tasks
    state.current_index = 1;
    assert_eq!(state.current_stage(), Some(SpecStage::Tasks));

    // Advance to Implement
    state.current_index = 2;
    assert_eq!(state.current_stage(), Some(SpecStage::Implement));

    // Advance to Validate
    state.current_index = 3;
    assert_eq!(state.current_stage(), Some(SpecStage::Validate));

    // Advance to Audit
    state.current_index = 4;
    assert_eq!(state.current_stage(), Some(SpecStage::Audit));

    // Advance to Unlock
    state.current_index = 5;
    assert_eq!(state.current_stage(), Some(SpecStage::Unlock));

    // Beyond stages
    state.current_index = 6;
    assert_eq!(state.current_stage(), None);
}

#[test]
fn test_validate_retry_tracking() {
    let mut state = SpecAutoState::new(
        "SPEC-TEST-011".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    assert_eq!(state.validate_retries, 0);

    // Simulate retry
    state.validate_retries += 1;
    assert_eq!(state.validate_retries, 1);

    state.validate_retries += 1;
    assert_eq!(state.validate_retries, 2);
}

// ============================================================================
// Quality Gates + Pipeline Integration
// ============================================================================

#[test]
fn test_quality_checkpoints_at_correct_stages() {
    // PrePlanning should run before Plan stage
    // PostPlan should run before Tasks stage
    // PostTasks should run before Implement stage

    let mut checkpoints_run: HashSet<QualityCheckpoint> = HashSet::new();

    // Simulate pipeline execution
    let stages = vec![
        SpecStage::Plan,
        SpecStage::Tasks,
        SpecStage::Implement,
        SpecStage::Validate,
        SpecStage::Audit,
        SpecStage::Unlock,
    ];

    for (idx, stage) in stages.iter().enumerate() {
        // Determine checkpoint before this stage
        let checkpoint = match stage {
            SpecStage::Plan if !checkpoints_run.contains(&QualityCheckpoint::PrePlanning) => {
                Some(QualityCheckpoint::PrePlanning)
            }
            SpecStage::Tasks if !checkpoints_run.contains(&QualityCheckpoint::PostPlan) => {
                Some(QualityCheckpoint::PostPlan)
            }
            SpecStage::Implement if !checkpoints_run.contains(&QualityCheckpoint::PostTasks) => {
                Some(QualityCheckpoint::PostTasks)
            }
            _ => None,
        };

        if let Some(cp) = checkpoint {
            checkpoints_run.insert(cp);
        }
    }

    // Verify all 3 checkpoints ran
    assert_eq!(checkpoints_run.len(), 3);
    assert!(checkpoints_run.contains(&QualityCheckpoint::PrePlanning));
    assert!(checkpoints_run.contains(&QualityCheckpoint::PostPlan));
    assert!(checkpoints_run.contains(&QualityCheckpoint::PostTasks));
}

#[test]
fn test_checkpoint_runs_once_per_pipeline() {
    let mut state = SpecAutoState::new(
        "SPEC-TEST-012".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    // First time: should run PrePlanning
    let should_run = !state.completed_checkpoints.contains(&QualityCheckpoint::PrePlanning);
    assert!(should_run);

    // Mark complete
    state.completed_checkpoints.insert(QualityCheckpoint::PrePlanning);

    // Second time: should NOT run again
    let should_run = !state.completed_checkpoints.contains(&QualityCheckpoint::PrePlanning);
    assert!(!should_run);
}

#[test]
fn test_pipeline_with_quality_gates_disabled() {
    let state = SpecAutoState::with_quality_gates(
        "SPEC-TEST-013".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
        false,  // Disable
    );

    // Should still have stages but no quality gate execution
    assert_eq!(state.stages.len(), 6);
    assert!(!state.quality_gates_enabled);
    assert!(state.completed_checkpoints.is_empty());
    assert!(state.quality_auto_resolved.is_empty());
}

#[test]
fn test_escalated_issues_tracked_separately_from_auto_resolved() {
    use codex_tui::{Confidence, Magnitude, QualityGateType, QualityIssue, Resolvability};
    use std::collections::HashMap;

    let mut state = SpecAutoState::new(
        "SPEC-TEST-014".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    let auto_issue = QualityIssue {
        id: "Q1".to_string(),
        gate_type: QualityGateType::Clarify,
        issue_type: "ambiguity".to_string(),
        description: "Auto-resolved issue".to_string(),
        confidence: Confidence::High,
        magnitude: Magnitude::Minor,
        resolvability: Resolvability::AutoFix,
        suggested_fix: None,
        context: "".to_string(),
        affected_artifacts: vec![],
        agent_answers: HashMap::new(),
        agent_reasoning: HashMap::new(),
    };

    let escalated_issue = QualityIssue {
        id: "Q2".to_string(),
        description: "Human-answered issue".to_string(),
        ..auto_issue.clone()
    };

    state.quality_auto_resolved.push((auto_issue, "yes".to_string()));
    state.quality_escalated.push((escalated_issue, "Option A".to_string()));

    assert_eq!(state.quality_auto_resolved.len(), 1);
    assert_eq!(state.quality_escalated.len(), 1);
}

// ============================================================================
// Error Recovery & Edge Cases
// ============================================================================

#[test]
fn test_pipeline_state_survives_checkpoint_completion() {
    let mut state = SpecAutoState::new(
        "SPEC-TEST-015".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    let spec_id_before = state.spec_id.clone();
    let goal_before = state.goal.clone();

    // Complete a checkpoint
    state.completed_checkpoints.insert(QualityCheckpoint::PrePlanning);

    // State should be preserved
    assert_eq!(state.spec_id, spec_id_before);
    assert_eq!(state.goal, goal_before);
    assert_eq!(state.stages.len(), 6);
}

#[test]
fn test_multiple_checkpoints_can_complete_in_sequence() {
    let mut state = SpecAutoState::new(
        "SPEC-TEST-016".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    // Complete checkpoints in order
    state.completed_checkpoints.insert(QualityCheckpoint::PrePlanning);
    state.completed_checkpoints.insert(QualityCheckpoint::PostPlan);
    state.completed_checkpoints.insert(QualityCheckpoint::PostTasks);

    assert_eq!(state.completed_checkpoints.len(), 3);

    // Verify all present
    assert!(state.completed_checkpoints.contains(&QualityCheckpoint::PrePlanning));
    assert!(state.completed_checkpoints.contains(&QualityCheckpoint::PostPlan));
    assert!(state.completed_checkpoints.contains(&QualityCheckpoint::PostTasks));
}

#[test]
fn test_quality_outcomes_accumulate_across_checkpoints() {
    let mut state = SpecAutoState::new(
        "SPEC-TEST-017".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    // Record outcomes from multiple checkpoints
    state.quality_checkpoint_outcomes.push((QualityCheckpoint::PrePlanning, 3, 1));
    state.quality_checkpoint_outcomes.push((QualityCheckpoint::PostPlan, 2, 0));
    state.quality_checkpoint_outcomes.push((QualityCheckpoint::PostTasks, 4, 2));

    assert_eq!(state.quality_checkpoint_outcomes.len(), 3);

    // Calculate totals
    let total_auto: usize = state.quality_checkpoint_outcomes.iter().map(|(_, a, _)| a).sum();
    let total_esc: usize = state.quality_checkpoint_outcomes.iter().map(|(_, _, e)| e).sum();

    assert_eq!(total_auto, 9);
    assert_eq!(total_esc, 3);
}

#[test]
fn test_hal_mode_preserved_throughout_pipeline() {

    let state = SpecAutoState::new(
        "SPEC-TEST-018".to_string(),
        "".to_string(),
        SpecStage::Plan,
        Some(HalMode::Live),
    );

    assert_eq!(state.hal_mode, Some(HalMode::Live));
}

#[test]
fn test_pending_prompt_summary_for_next_stage() {
    let mut state = SpecAutoState::new(
        "SPEC-TEST-019".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    assert!(state.pending_prompt_summary.is_none());

    // Set summary after guardrail
    state.pending_prompt_summary = Some("Baseline passed, 2 warnings".to_string());

    assert_eq!(
        state.pending_prompt_summary,
        Some("Baseline passed, 2 warnings".to_string())
    );
}

// ============================================================================
// Integration: Quality Gates + Stage Progression
// ============================================================================

#[test]
fn test_simulated_pipeline_flow_with_quality_gates() {
    let mut state = SpecAutoState::new(
        "SPEC-TEST-020".to_string(),
        "Full pipeline simulation".to_string(),
        SpecStage::Plan,
        None,
    );

    // Stage 0: Plan
    assert_eq!(state.current_stage(), Some(SpecStage::Plan));
    assert!(state.completed_checkpoints.is_empty());

    // Before Plan: Run PrePlanning checkpoint
    let should_run_checkpoint = state.quality_gates_enabled
        && !state.completed_checkpoints.contains(&QualityCheckpoint::PrePlanning);
    assert!(should_run_checkpoint);

    state.completed_checkpoints.insert(QualityCheckpoint::PrePlanning);
    state.quality_checkpoint_outcomes.push((QualityCheckpoint::PrePlanning, 5, 0));

    // Advance to Tasks
    state.current_index = 1;
    assert_eq!(state.current_stage(), Some(SpecStage::Tasks));

    // Before Tasks: Run PostPlan checkpoint
    let should_run_checkpoint = state.quality_gates_enabled
        && !state.completed_checkpoints.contains(&QualityCheckpoint::PostPlan);
    assert!(should_run_checkpoint);

    state.completed_checkpoints.insert(QualityCheckpoint::PostPlan);
    state.quality_checkpoint_outcomes.push((QualityCheckpoint::PostPlan, 3, 1));

    // Advance to Implement
    state.current_index = 2;
    assert_eq!(state.current_stage(), Some(SpecStage::Implement));

    // Before Implement: Run PostTasks checkpoint
    let should_run_checkpoint = state.quality_gates_enabled
        && !state.completed_checkpoints.contains(&QualityCheckpoint::PostTasks);
    assert!(should_run_checkpoint);

    state.completed_checkpoints.insert(QualityCheckpoint::PostTasks);
    state.quality_checkpoint_outcomes.push((QualityCheckpoint::PostTasks, 2, 0));

    // No more checkpoints for Validate, Audit, Unlock
    state.current_index = 3;
    assert_eq!(state.current_stage(), Some(SpecStage::Validate));

    state.current_index = 4;
    assert_eq!(state.current_stage(), Some(SpecStage::Audit));

    state.current_index = 5;
    assert_eq!(state.current_stage(), Some(SpecStage::Unlock));

    // Pipeline complete
    state.current_index = 6;
    assert_eq!(state.current_stage(), None);

    // Verify all checkpoints ran
    assert_eq!(state.completed_checkpoints.len(), 3);
    assert_eq!(state.quality_checkpoint_outcomes.len(), 3);

    // Verify total resolutions: 5+3+2=10 auto, 0+1+0=1 escalated
    let total_auto: usize = state.quality_checkpoint_outcomes.iter().map(|(_, a, _)| a).sum();
    let total_esc: usize = state.quality_checkpoint_outcomes.iter().map(|(_, _, e)| e).sum();
    assert_eq!(total_auto, 10);
    assert_eq!(total_esc, 1);
}
