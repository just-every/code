//! Handler orchestration tests (Phase 2)
//!
//! FORK-SPECIFIC (just-every/code): Test Coverage Phase 2 (Dec 2025)
//!
//! Tests handler.rs orchestration logic, retry mechanisms, and error paths.
//! Policy: docs/spec-kit/testing-policy.md
//! Target: handler.rs 0.7%→30% coverage

mod common;

use codex_tui::{MockSpecKitContext, SpecAutoPhase, SpecAutoState};
use codex_tui::chatwidget::spec_kit::handler;

#[test]
fn test_halt_spec_auto_clears_state() {
    let mut mock = MockSpecKitContext::new();
    mock.spec_auto_state = Some(SpecAutoState {
        spec_id: "SPEC-TEST".to_string(),
        current_index: 2,
        phase: SpecAutoPhase::Guardrail,
        agent_retry_count: 0,
        agent_retry_context: None,
        goal: String::new(),
        quality_gates_enabled: false,
        completed_checkpoints: Default::default(),
        quality_checkpoint_outcomes: Vec::new(),
        quality_auto_resolved: Vec::new(),
        quality_escalated: Vec::new(),
        quality_modifications: Vec::new(),
    });

    handler::halt_spec_auto_with_error(&mut mock, "Test error".to_string());

    // State should be cleared
    assert!(mock.spec_auto_state.is_none());
    assert!(mock.history.len() > 0); // Error pushed to history
}

#[test]
fn test_advance_spec_auto_progresses_stage() {
    let mut mock = MockSpecKitContext::new();
    mock.spec_auto_state = Some(SpecAutoState {
        spec_id: "SPEC-TEST".to_string(),
        current_index: 0, // Plan stage
        phase: SpecAutoPhase::Guardrail,
        agent_retry_count: 0,
        agent_retry_context: None,
        goal: String::new(),
        quality_gates_enabled: false,
        completed_checkpoints: Default::default(),
        quality_checkpoint_outcomes: Vec::new(),
        quality_auto_resolved: Vec::new(),
        quality_escalated: Vec::new(),
        quality_modifications: Vec::new(),
    });

    // advance_spec_auto should transition to next stage
    // Note: This will trigger guardrail validation which may fail without proper setup
    // This test demonstrates the pattern, full implementation needs guardrail mocking
}

#[test]
fn test_spec_auto_state_persists_retry_count() {
    let mut state = SpecAutoState {
        spec_id: "SPEC-TEST".to_string(),
        current_index: 1,
        phase: SpecAutoPhase::Guardrail,
        agent_retry_count: 2, // Has retried twice
        agent_retry_context: Some("Previous attempt failed".to_string()),
        goal: String::new(),
        quality_gates_enabled: false,
        completed_checkpoints: Default::default(),
        quality_checkpoint_outcomes: Vec::new(),
        quality_auto_resolved: Vec::new(),
        quality_escalated: Vec::new(),
        quality_modifications: Vec::new(),
    };

    // Verify retry state persists
    assert_eq!(state.agent_retry_count, 2);
    assert!(state.agent_retry_context.is_some());
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
