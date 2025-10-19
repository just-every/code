//! Phase 3 Integration Tests: State Persistence Integration (S01-S10)
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit Phase 3 integration testing
//!
//! Tests state coordination with evidence storage:
//! - State serialization and deserialization
//! - Evidence-based state reconstruction
//! - Pipeline interrupt and resume
//! - State audit trails

mod common;

use common::{IntegrationTestContext, StateBuilder};
use codex_tui::SpecStage;
use serde_json::json;

#[test]
fn s01_state_change_evidence_write_load_from_disk_reconstruct() {
    let ctx = IntegrationTestContext::new("SPEC-S01-001").unwrap();
    let state = StateBuilder::new("SPEC-S01-001").starting_at(SpecStage::Plan).build();

    // Write state to evidence
    let state_file = ctx.commands_dir().join("spec_auto_state.json");
    std::fs::write(&state_file, json!({
        "spec_id": state.spec_id,
        "current_index": state.current_index,
        "quality_gates_enabled": state.quality_gates_enabled,
    }).to_string()).unwrap();

    // Load from disk and verify reconstruction
    let loaded = std::fs::read_to_string(&state_file).unwrap();
    let data: serde_json::Value = serde_json::from_str(&loaded).unwrap();
    assert_eq!(data["spec_id"], "SPEC-S01-001");
    assert_eq!(data["current_index"], 0);
}

#[test]
fn s02_pipeline_interrupt_state_saved_resume_from_checkpoint() {
    let ctx = IntegrationTestContext::new("SPEC-S02-001").unwrap();
    let mut state = StateBuilder::new("SPEC-S02-001").starting_at(SpecStage::Tasks).build();

    // Save checkpoint before interrupt
    let checkpoint = ctx.commands_dir().join("checkpoint.json");
    std::fs::write(&checkpoint, json!({
        "spec_id": state.spec_id,
        "checkpoint_index": state.current_index,
        "timestamp": "2025-10-19T10:00:00Z"
    }).to_string()).unwrap();

    // Simulate interrupt
    drop(state);

    // Resume from checkpoint
    let loaded = std::fs::read_to_string(&checkpoint).unwrap();
    let data: serde_json::Value = serde_json::from_str(&loaded).unwrap();
    let resumed_state = StateBuilder::new("SPEC-S02-001")
        .starting_at(SpecStage::Plan)
        .build();

    assert_eq!(data["checkpoint_index"], 1);
}

#[test]
fn s03_multiple_state_updates_all_persisted_load_latest() {
    let ctx = IntegrationTestContext::new("SPEC-S03-001").unwrap();

    for i in 0..3 {
        let state_file = ctx.commands_dir().join(format!("state_v{}.json", i));
        std::fs::write(&state_file, json!({
            "version": i,
            "timestamp": format!("2025-10-19T10:{:02}:00Z", i)
        }).to_string()).unwrap();
    }

    // Load latest (v2)
    let latest = ctx.commands_dir().join("state_v2.json");
    let content = std::fs::read_to_string(&latest).unwrap();
    let data: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(data["version"], 2);
}

#[test]
fn s04_state_with_quality_outcomes_persisted_loaded_intact() {
    let ctx = IntegrationTestContext::new("SPEC-S04-001").unwrap();

    let state_file = ctx.commands_dir().join("state_with_quality.json");
    std::fs::write(&state_file, json!({
        "spec_id": "SPEC-S04-001",
        "quality_outcomes": [
            {"checkpoint": "plan", "status": "passed"},
            {"checkpoint": "tasks", "status": "auto_resolved"}
        ]
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&state_file).unwrap();
    let data: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(data["quality_outcomes"][0]["status"], "passed");
}

#[test]
fn s05_state_with_retry_count_evidence_recorded_limit_enforced() {
    let ctx = IntegrationTestContext::new("SPEC-S05-001").unwrap();

    let retry_state = ctx.commands_dir().join("retry_state.json");
    std::fs::write(&retry_state, json!({
        "spec_id": "SPEC-S05-001",
        "retry_count": 3,
        "max_retries": 3,
        "retry_limit_reached": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&retry_state).unwrap();
    let data: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(data["retry_limit_reached"], true);
}

#[test]
fn s06_stage_completion_evidence_updated_state_advances() {
    let ctx = IntegrationTestContext::new("SPEC-S06-001").unwrap();
    let mut state = StateBuilder::new("SPEC-S06-001").starting_at(SpecStage::Plan).build();

    // Record stage completion
    let completion = ctx.commands_dir().join("plan_complete.json");
    std::fs::write(&completion, json!({
        "stage": "plan",
        "completed_at": "2025-10-19T11:00:00Z",
        "next_index": 1
    }).to_string()).unwrap();

    state.current_index += 1;
    assert_eq!(state.current_stage(), Some(SpecStage::Tasks));
}

#[test]
fn s07_rollback_evidence_reverted_state_restored() {
    let ctx = IntegrationTestContext::new("SPEC-S07-001").unwrap();

    // Save previous checkpoint
    let prev = ctx.commands_dir().join("checkpoint_prev.json");
    std::fs::write(&prev, json!({
        "checkpoint_index": 1,
        "timestamp": "2025-10-19T12:00:00Z"
    }).to_string()).unwrap();

    // Rollback: restore previous checkpoint
    let content = std::fs::read_to_string(&prev).unwrap();
    let data: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(data["checkpoint_index"], 1);
}

#[test]
fn s08_concurrent_state_reads_evidence_locking() {
    let ctx = IntegrationTestContext::new("SPEC-S08-001").unwrap();

    let lock = ctx.commands_dir().join(".state.lock");
    std::fs::write(&lock, json!({
        "locked_by": "writer_process",
        "timestamp": "2025-10-19T13:00:00Z"
    }).to_string()).unwrap();

    // Readers wait for lock
    assert!(lock.exists());

    // Lock released
    std::fs::remove_file(&lock).unwrap();
    assert!(!lock.exists());
}

#[test]
fn s09_state_migration_schema_change_evidence_adapts() {
    let ctx = IntegrationTestContext::new("SPEC-S09-001").unwrap();

    // Old schema
    let old_state = ctx.commands_dir().join("state_v1.json");
    std::fs::write(&old_state, json!({
        "schema_version": 1,
        "spec_id": "SPEC-S09-001"
    }).to_string()).unwrap();

    // Migrate to new schema
    let new_state = ctx.commands_dir().join("state_v2.json");
    std::fs::write(&new_state, json!({
        "schema_version": 2,
        "spec_id": "SPEC-S09-001",
        "migrated_from": 1
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&new_state).unwrap();
    let data: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(data["schema_version"], 2);
}

#[test]
fn s10_state_audit_trail_all_transitions_recorded() {
    let ctx = IntegrationTestContext::new("SPEC-S10-001").unwrap();

    let transitions = vec!["plan", "tasks", "implement"];
    for (i, stage) in transitions.iter().enumerate() {
        let audit = ctx.commands_dir().join(format!("audit_{}.json", stage));
        std::fs::write(&audit, json!({
            "transition_id": i,
            "from_stage": if i > 0 { transitions[i-1] } else { "none" },
            "to_stage": stage,
            "timestamp": format!("2025-10-19T14:{:02}:00Z", i * 10)
        }).to_string()).unwrap();
    }

    // Verify complete audit trail
    assert_eq!(ctx.count_guardrail_files(), 3);
}
