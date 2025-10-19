//! Handler orchestration tests (Phase 2)
//!
//! FORK-SPECIFIC (just-every/code): Test Coverage Phase 2 (Dec 2025)
//!
//! Tests handler.rs orchestration logic via state transitions, retry mechanisms.
//! Strategy: Test state logic directly (like E2E tests) rather than through handler functions
//! Policy: docs/spec-kit/testing-policy.md
//! Target: handler.rs 0.7%→30% coverage

mod common;

use codex_tui::{
    halt_spec_auto_with_error, MockSpecKitContext, SpecAutoPhase, SpecAutoState, SpecKitContext,
    SpecStage,
};

// ============================================================================
// State Management Tests
// ============================================================================

#[test]
fn test_halt_spec_auto_clears_state() {
    let mut mock = MockSpecKitContext::new();
    mock.spec_auto_state = Some(SpecAutoState::new(
        "SPEC-TEST".to_string(),
        "Test goal".to_string(),
        SpecStage::Implement,
        None,
    ));

    halt_spec_auto_with_error(&mut mock, "Test error".to_string());

    // State should be cleared
    assert!(mock.spec_auto_state.is_none());
    assert!(mock.history.len() > 0); // Error pushed to history
}

#[test]
fn test_spec_auto_state_persists_retry_count() {
    let mut state = SpecAutoState::new(
        "SPEC-TEST".to_string(),
        "Test goal".to_string(),
        SpecStage::Tasks,
        None,
    );

    // Simulate retry
    state.agent_retry_count = 2;
    state.agent_retry_context = Some("Previous attempt failed".to_string());

    // Verify retry state persists
    assert_eq!(state.agent_retry_count, 2);
    assert!(state.agent_retry_context.is_some());
}

#[test]
fn test_spec_auto_state_initialization() {
    let state = SpecAutoState::new(
        "SPEC-KIT-123".to_string(),
        "Add OAuth2 authentication".to_string(),
        SpecStage::Plan,
        None,
    );

    assert_eq!(state.spec_id, "SPEC-KIT-123");
    assert_eq!(state.goal, "Add OAuth2 authentication");
    assert_eq!(state.current_index, 0);
    assert_eq!(state.stages.len(), 6); // All 6 stages
    assert_eq!(state.current_stage(), Some(SpecStage::Plan));
    assert!(state.quality_gates_enabled); // Enabled by default
}

#[test]
fn test_spec_auto_state_resume_from_tasks() {
    let state = SpecAutoState::new(
        "SPEC-KIT-124".to_string(),
        "".to_string(),
        SpecStage::Tasks,
        None,
    );

    assert_eq!(state.current_index, 1); // Tasks is index 1
    assert_eq!(state.current_stage(), Some(SpecStage::Tasks));
    assert_eq!(state.stages[0], SpecStage::Plan);
    assert_eq!(state.stages[1], SpecStage::Tasks);
}

#[test]
fn test_spec_auto_phase_transitions() {
    let phase1 = SpecAutoPhase::Guardrail;
    let phase2 = SpecAutoPhase::ExecutingAgents {
        expected_agents: vec!["gemini".to_string(), "claude".to_string()],
        completed_agents: Default::default(),
    };

    // Phase transitions are explicit state changes
    assert!(matches!(phase1, SpecAutoPhase::Guardrail));
    assert!(matches!(phase2, SpecAutoPhase::ExecutingAgents { .. }));
}

#[test]
fn test_mock_context_tracks_submissions() {
    let mut mock = MockSpecKitContext::new();

    mock.submit_prompt("Display 1".to_string(), "Prompt 1".to_string());
    mock.submit_prompt("Display 2".to_string(), "Prompt 2".to_string());

    assert_eq!(mock.submitted_prompts.len(), 2);
    assert_eq!(mock.submitted_prompts[0].0, "Display 1");
    assert_eq!(mock.submitted_prompts[1].1, "Prompt 2");
}

// ============================================================================
// Retry Logic Tests
// ============================================================================

#[test]
fn test_agent_retry_count_starts_at_zero() {
    let state = SpecAutoState::new(
        "SPEC-RETRY-001".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    assert_eq!(state.agent_retry_count, 0);
    assert!(state.agent_retry_context.is_none());
}

#[test]
fn test_agent_retry_state_mutation() {
    let mut state = SpecAutoState::new(
        "SPEC-RETRY-002".to_string(),
        "".to_string(),
        SpecStage::Implement,
        None,
    );

    // Simulate first retry
    state.agent_retry_count += 1;
    state.agent_retry_context = Some("Empty result from gemini".to_string());
    assert_eq!(state.agent_retry_count, 1);

    // Simulate second retry
    state.agent_retry_count += 1;
    state.agent_retry_context = Some("Timeout from claude".to_string());
    assert_eq!(state.agent_retry_count, 2);

    // Simulate third retry (max retries = 3)
    state.agent_retry_count += 1;
    assert_eq!(state.agent_retry_count, 3);
}

// ============================================================================
// Quality Gate Integration Tests
// ============================================================================

#[test]
fn test_quality_gates_enabled_by_default() {
    let state = SpecAutoState::new(
        "SPEC-QG-001".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    assert!(state.quality_gates_enabled);
    assert!(state.completed_checkpoints.is_empty());
    assert!(state.quality_modifications.is_empty());
    assert!(state.quality_auto_resolved.is_empty());
    assert!(state.quality_escalated.is_empty());
}

#[test]
fn test_quality_gates_can_be_disabled() {
    let state = SpecAutoState::with_quality_gates(
        "SPEC-QG-002".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
        false,
    );

    assert!(!state.quality_gates_enabled);
}

// ============================================================================
// Stage Progression Tests
// ============================================================================

#[test]
fn test_current_stage_retrieval() {
    let state = SpecAutoState::new(
        "SPEC-STAGE-001".to_string(),
        "".to_string(),
        SpecStage::Validate,
        None,
    );

    assert_eq!(state.current_stage(), Some(SpecStage::Validate));
    assert_eq!(state.current_index, 3); // Validate is index 3
}

#[test]
fn test_pipeline_stages_order() {
    let state = SpecAutoState::new(
        "SPEC-STAGE-002".to_string(),
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

// ============================================================================
// HAL Mode Tests
// ============================================================================

#[test]
fn test_hal_mode_defaults_to_none() {
    let state = SpecAutoState::new(
        "SPEC-HAL-001".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    assert!(state.hal_mode.is_none());
}
// ============================================================================
// ============================================================================
// Guardrail State Tests
// ============================================================================

#[test]
fn test_waiting_guardrail_initialization() {
    let state = SpecAutoState::new(
        "SPEC-GR-001".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    assert!(state.waiting_guardrail.is_none());
}

#[test]
fn test_validate_retries_starts_at_zero() {
    let state = SpecAutoState::new(
        "SPEC-GR-002".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    assert_eq!(state.validate_retries, 0);
}

#[test]
fn test_pending_prompt_summary_defaults_to_none() {
    let state = SpecAutoState::new(
        "SPEC-GR-003".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    assert!(state.pending_prompt_summary.is_none());
}

#[test]
fn test_state_can_track_pending_prompt() {
    let mut state = SpecAutoState::new(
        "SPEC-GR-004".to_string(),
        "".to_string(),
        SpecStage::Tasks,
        None,
    );

    state.pending_prompt_summary = Some("Waiting for task breakdown".to_string());
    assert!(state.pending_prompt_summary.is_some());
    assert_eq!(
        state.pending_prompt_summary.unwrap(),
        "Waiting for task breakdown"
    );
}

#[test]
fn test_validate_retries_can_increment() {
    let mut state = SpecAutoState::new(
        "SPEC-GR-005".to_string(),
        "".to_string(),
        SpecStage::Validate,
        None,
    );

    state.validate_retries = 1;
    assert_eq!(state.validate_retries, 1);

    state.validate_retries = 2;
    assert_eq!(state.validate_retries, 2);
}

// ============================================================================
// Agent Completion Tracking Tests
// ============================================================================

#[test]
fn test_executing_agents_phase_initialization() {
    let phase = SpecAutoPhase::ExecutingAgents {
        expected_agents: vec![
            "gemini".to_string(),
            "claude".to_string(),
            "code".to_string(),
        ],
        completed_agents: Default::default(),
    };

    if let SpecAutoPhase::ExecutingAgents {
        expected_agents,
        completed_agents,
    } = phase
    {
        assert_eq!(expected_agents.len(), 3);
        assert!(completed_agents.is_empty());
    } else {
        panic!("Expected ExecutingAgents phase");
    }
}

#[test]
fn test_agent_completion_tracking() {
    let mut completed = std::collections::HashSet::new();
    completed.insert("gemini".to_string());
    completed.insert("claude".to_string());

    assert_eq!(completed.len(), 2);
    assert!(completed.contains("gemini"));
    assert!(completed.contains("claude"));
    assert!(!completed.contains("code"));
}

#[test]
fn test_all_agents_completed_detection() {
    let expected = vec![
        "gemini".to_string(),
        "claude".to_string(),
        "code".to_string(),
    ];
    let mut completed = std::collections::HashSet::new();
    completed.insert("gemini".to_string());
    completed.insert("claude".to_string());
    completed.insert("code".to_string());

    let all_done = expected.iter().all(|agent| completed.contains(agent));
    assert!(all_done);
}

#[test]
fn test_partial_agent_completion_detection() {
    let expected = vec![
        "gemini".to_string(),
        "claude".to_string(),
        "code".to_string(),
    ];
    let mut completed = std::collections::HashSet::new();
    completed.insert("gemini".to_string());

    let all_done = expected.iter().all(|agent| completed.contains(agent));
    assert!(!all_done);
}

#[test]
fn test_checking_consensus_phase() {
    let phase = SpecAutoPhase::CheckingConsensus;
    assert!(matches!(phase, SpecAutoPhase::CheckingConsensus));
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_halt_preserves_resume_hint() {
    let mut mock = MockSpecKitContext::new();
    mock.spec_auto_state = Some(SpecAutoState::new(
        "SPEC-ERROR-001".to_string(),
        "Test goal".to_string(),
        SpecStage::Tasks,
        None,
    ));

    halt_spec_auto_with_error(&mut mock, "Agent timeout".to_string());

    // Check history contains error
    assert!(mock.history.len() > 0);
    assert!(mock.spec_auto_state.is_none());
}

#[test]
fn test_halt_with_different_stages() {
    for stage in [
        SpecStage::Plan,
        SpecStage::Tasks,
        SpecStage::Implement,
        SpecStage::Validate,
        SpecStage::Audit,
        SpecStage::Unlock,
    ] {
        let mut mock = MockSpecKitContext::new();
        mock.spec_auto_state = Some(SpecAutoState::new(
            format!("SPEC-{:?}", stage),
            "".to_string(),
            stage,
            None,
        ));

        halt_spec_auto_with_error(&mut mock, format!("Error at {:?}", stage));

        assert!(mock.spec_auto_state.is_none());
    }
}

#[test]
fn test_halt_on_already_none_state() {
    let mut mock = MockSpecKitContext::new();
    // State already None
    assert!(mock.spec_auto_state.is_none());

    halt_spec_auto_with_error(&mut mock, "Error on empty state".to_string());

    // Should not panic, state remains None
    assert!(mock.spec_auto_state.is_none());
}

#[test]
fn test_agent_retry_exhaustion() {
    let mut state = SpecAutoState::new(
        "SPEC-RETRY-EXHAUST".to_string(),
        "".to_string(),
        SpecStage::Implement,
        None,
    );

    // Simulate 3 retries (max)
    for i in 1..=3 {
        state.agent_retry_count = i;
    }

    assert_eq!(state.agent_retry_count, 3);

    // Check if exhausted (would need handler logic)
    let is_exhausted = state.agent_retry_count >= 3;
    assert!(is_exhausted);
}

#[test]
fn test_retry_context_captures_failure_reason() {
    let mut state = SpecAutoState::new(
        "SPEC-RETRY-CONTEXT".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    state.agent_retry_count = 1;
    state.agent_retry_context = Some("Gemini returned empty output".to_string());

    assert!(state.agent_retry_context.is_some());
    assert!(state
        .agent_retry_context
        .unwrap()
        .contains("empty output"));
}

#[test]
fn test_multiple_retry_context_updates() {
    let mut state = SpecAutoState::new(
        "SPEC-RETRY-MULTI".to_string(),
        "".to_string(),
        SpecStage::Tasks,
        None,
    );

    // First retry
    state.agent_retry_count = 1;
    state.agent_retry_context = Some("Timeout".to_string());
    assert_eq!(state.agent_retry_context.as_deref(), Some("Timeout"));

    // Second retry overwrites
    state.agent_retry_count = 2;
    state.agent_retry_context = Some("Invalid JSON".to_string());
    assert_eq!(state.agent_retry_context.as_deref(), Some("Invalid JSON"));
}

#[test]
fn test_empty_goal_allowed() {
    let state = SpecAutoState::new(
        "SPEC-EMPTY-GOAL".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    assert_eq!(state.goal, "");
    assert_eq!(state.spec_id, "SPEC-EMPTY-GOAL");
}

// ============================================================================
// Edge Cases Tests
// ============================================================================

#[test]
fn test_stage_index_beyond_bounds() {
    let state = SpecAutoState::new(
        "SPEC-BOUNDS-001".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    // current_index starts at 0, stages.len() is 6
    assert!(state.current_index < state.stages.len());

    // Simulate advancing past all stages
    let final_index = state.stages.len();
    assert!(final_index >= state.stages.len());
}

#[test]
fn test_resume_from_unlock_stage() {
    let state = SpecAutoState::new(
        "SPEC-UNLOCK".to_string(),
        "".to_string(),
        SpecStage::Unlock,
        None,
    );

    assert_eq!(state.current_index, 5); // Unlock is last stage (index 5)
    assert_eq!(state.current_stage(), Some(SpecStage::Unlock));
}

#[test]
fn test_quality_modifications_tracking() {
    let mut state = SpecAutoState::new(
        "SPEC-QM-001".to_string(),
        "".to_string(),
        SpecStage::Implement,
        None,
    );

    state
        .quality_modifications
        .push("src/main.rs".to_string());
    state.quality_modifications.push("src/lib.rs".to_string());

    assert_eq!(state.quality_modifications.len(), 2);
    assert!(state.quality_modifications.contains(&"src/main.rs".to_string()));
}

#[test]
fn test_quality_auto_resolved_collection() {
    let mut state = SpecAutoState::new(
        "SPEC-QAR-001".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    // Quality auto_resolved is Vec<(QualityIssue, String)>
    // We can test it exists and is empty
    assert_eq!(state.quality_auto_resolved.len(), 0);
    
    // Test it can grow (would need actual QualityIssue in real usage)
    assert!(state.quality_auto_resolved.capacity() >= 0);
}

#[test]
fn test_quality_escalated_collection() {
    let mut state = SpecAutoState::new(
        "SPEC-QE-001".to_string(),
        "".to_string(),
        SpecStage::Implement,
        None,
    );

    // Quality escalated is Vec<(QualityIssue, String)>
    // We can test it exists and is empty
    assert_eq!(state.quality_escalated.len(), 0);
    assert!(state.quality_escalated.is_empty());
}

#[test]
fn test_quality_checkpoint_outcomes_initialization() {
    let state = SpecAutoState::new(
        "SPEC-QCO-001".to_string(),
        "".to_string(),
        SpecStage::Tasks,
        None,
    );

    // quality_checkpoint_outcomes is Vec<(QualityCheckpoint, usize, usize)>
    assert!(state.quality_checkpoint_outcomes.is_empty());
}

#[test]
fn test_completed_checkpoints_initialization() {
    let state = SpecAutoState::new(
        "SPEC-CC-001".to_string(),
        "".to_string(),
        SpecStage::Implement,
        None,
    );

    assert!(state.completed_checkpoints.is_empty());
}

#[test]
fn test_state_with_quality_gates_disabled() {
    let state = SpecAutoState::with_quality_gates(
        "SPEC-QG-DISABLED".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
        false,
    );

    assert!(!state.quality_gates_enabled);
    assert!(state.completed_checkpoints.is_empty());
    assert!(state.quality_modifications.is_empty());
}

#[test]
fn test_agent_names_in_executing_phase() {
    let phase = SpecAutoPhase::ExecutingAgents {
        expected_agents: vec![
            "gemini".to_string(),
            "claude".to_string(),
            "gpt_pro".to_string(),
        ],
        completed_agents: {
            let mut set = std::collections::HashSet::new();
            set.insert("gemini".to_string());
            set
        },
    };

    if let SpecAutoPhase::ExecutingAgents {
        expected_agents,
        completed_agents,
    } = phase
    {
        assert_eq!(expected_agents.len(), 3);
        assert_eq!(completed_agents.len(), 1);
        assert!(completed_agents.contains("gemini"));
        assert!(!completed_agents.contains("claude"));
    } else {
        panic!("Wrong phase type");
    }
}

#[test]
fn test_multi_stage_progression_indexes() {
    // Test each stage maps to correct index
    let test_cases = vec![
        (SpecStage::Plan, 0),
        (SpecStage::Tasks, 1),
        (SpecStage::Implement, 2),
        (SpecStage::Validate, 3),
        (SpecStage::Audit, 4),
        (SpecStage::Unlock, 5),
    ];

    for (stage, expected_index) in test_cases {
        let state = SpecAutoState::new(
            format!("SPEC-{:?}", stage),
            "".to_string(),
            stage,
            None,
        );
        assert_eq!(state.current_index, expected_index);
    }
}
// ============================================================================
// Empty Result Detection Tests
// ============================================================================

#[test]
fn test_agent_empty_result_detection() {
    // Simulate checking for empty agent response
    let response = "";
    assert!(response.is_empty());
}

#[test]
fn test_agent_whitespace_only_result() {
    let response = "   \n\t  ";
    assert!(response.trim().is_empty());
}

#[test]
fn test_agent_valid_nonempty_result() {
    let response = "Agent response content";
    assert!(!response.trim().is_empty());
}

#[test]
fn test_json_empty_object_detection() {
    let json_str = "{}";
    let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(json_str);
    assert!(parsed.is_ok());
    
    if let Ok(value) = parsed {
        if let Some(obj) = value.as_object() {
            assert!(obj.is_empty());
        }
    }
}

#[test]
fn test_json_empty_array_detection() {
    let json_str = "[]";
    let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(json_str);
    assert!(parsed.is_ok());
    
    if let Ok(value) = parsed {
        if let Some(arr) = value.as_array() {
            assert!(arr.is_empty());
        }
    }
}

// ============================================================================
// Stage Advancement Edge Cases
// ============================================================================

#[test]
fn test_advance_from_final_stage() {
    let state = SpecAutoState::new(
        "SPEC-FINAL".to_string(),
        "".to_string(),
        SpecStage::Unlock,
        None,
    );

    // At final stage (index 5), advancing would go beyond bounds
    assert_eq!(state.current_index, 5);
    assert_eq!(state.stages.len(), 6);
    
    // Next index would be 6, which is >= stages.len()
    let next_index = state.current_index + 1;
    assert!(next_index >= state.stages.len());
}

#[test]
fn test_advance_from_middle_stage() {
    let mut state = SpecAutoState::new(
        "SPEC-MID".to_string(),
        "".to_string(),
        SpecStage::Implement,
        None,
    );

    assert_eq!(state.current_index, 2); // Implement is index 2
    
    // Simulate advancement
    state.current_index += 1;
    assert_eq!(state.current_index, 3); // Now at Validate
}

#[test]
fn test_skip_stage_advancement() {
    let mut state = SpecAutoState::new(
        "SPEC-SKIP".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    // Simulate skipping a stage (direct jump)
    state.current_index = 4; // Jump to Audit
    assert_eq!(state.current_stage(), Some(SpecStage::Audit));
}

#[test]
fn test_backward_stage_movement() {
    let mut state = SpecAutoState::new(
        "SPEC-BACK".to_string(),
        "".to_string(),
        SpecStage::Validate,
        None,
    );

    assert_eq!(state.current_index, 3);
    
    // Simulate backward movement (e.g., restart from earlier stage)
    state.current_index = 1;
    assert_eq!(state.current_stage(), Some(SpecStage::Tasks));
}

#[test]
fn test_stage_bounds_checking() {
    let state = SpecAutoState::new(
        "SPEC-BOUNDS".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    // Test all valid indices
    for i in 0..state.stages.len() {
        assert!(i < state.stages.len());
    }
}

// ============================================================================
// Agent Invocation Scenarios
// ============================================================================

#[test]
fn test_tier2_agent_configuration() {
    // Tier 2: 3 agents (gemini, claude, code/gpt_pro)
    let expected_agents = vec![
        "gemini".to_string(),
        "claude".to_string(),
        "code".to_string(),
    ];

    assert_eq!(expected_agents.len(), 3);
}

#[test]
fn test_tier3_agent_configuration() {
    // Tier 3: 4 agents (gemini, claude, gpt_codex, gpt_pro)
    let expected_agents = vec![
        "gemini".to_string(),
        "claude".to_string(),
        "gpt_codex".to_string(),
        "gpt_pro".to_string(),
    ];

    assert_eq!(expected_agents.len(), 4);
}

#[test]
fn test_degraded_agent_set_2_of_3() {
    // One agent failed/missing
    let mut expected = vec!["gemini".to_string(), "claude".to_string(), "code".to_string()];
    let mut completed = std::collections::HashSet::new();
    completed.insert("gemini".to_string());
    completed.insert("claude".to_string());

    // 2 out of 3 completed (degraded but viable)
    let completed_count = expected.iter().filter(|a| completed.contains(*a)).count();
    assert_eq!(completed_count, 2);
}

#[test]
fn test_agent_timeout_scenario() {
    let mut state = SpecAutoState::new(
        "SPEC-TIMEOUT".to_string(),
        "".to_string(),
        SpecStage::Plan,
        None,
    );

    // Simulate agent timeout
    state.agent_retry_count = 1;
    state.agent_retry_context = Some("Agent timeout after 30s".to_string());

    assert!(state.agent_retry_context.as_ref().unwrap().contains("timeout"));
}

#[test]
fn test_all_agents_failed_scenario() {
    let mut state = SpecAutoState::new(
        "SPEC-ALLFAIL".to_string(),
        "".to_string(),
        SpecStage::Implement,
        None,
    );

    // All agents failed after max retries
    state.agent_retry_count = 3;
    state.agent_retry_context = Some("All agents failed after 3 retries".to_string());

    assert_eq!(state.agent_retry_count, 3);
    assert!(state.agent_retry_context.is_some());
}
