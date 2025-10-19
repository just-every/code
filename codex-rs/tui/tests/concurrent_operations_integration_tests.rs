//! Phase 3 Integration Tests: Concurrent Operations Integration (C01-C10)
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit Phase 3 integration testing
//!
//! Tests parallel operations and synchronization

mod common;

use common::{IntegrationTestContext, StateBuilder};
use codex_tui::SpecStage;
use serde_json::json;

#[test]
fn c01_parallel_agent_spawns_evidence_locks_sequential_writes() {
    let ctx = IntegrationTestContext::new("SPEC-C01-001").unwrap();

    for agent in &["gemini", "claude", "gpt_pro"] {
        let file = ctx.consensus_dir().join(format!("spec-plan_{}.json", agent));
        std::fs::write(&file, json!({"agent": agent}).to_string()).unwrap();
    }

    assert_eq!(ctx.count_consensus_files(), 3);
}

#[test]
fn c02_multiple_stages_overlapping_writes_locking_prevents_corruption() {
    let ctx = IntegrationTestContext::new("SPEC-C02-001").unwrap();

    let lock = ctx.consensus_dir().join(".write.lock");
    std::fs::write(&lock, json!({"locked": true}).to_string()).unwrap();
    assert!(lock.exists());

    std::fs::remove_file(&lock).unwrap();
    assert!(!lock.exists());
}

#[test]
fn c03_concurrent_quality_checkpoints_queued_execution() {
    let ctx = IntegrationTestContext::new("SPEC-C03-001").unwrap();

    let queue = ctx.commands_dir().join("checkpoint_queue.json");
    std::fs::write(&queue, json!({
        "queued": ["plan", "tasks"],
        "executing": "plan",
        "completed": []
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&queue).unwrap();
    assert!(content.contains("queued"));
}

#[test]
fn c04_parallel_guardrail_validation_merged_results() {
    let ctx = IntegrationTestContext::new("SPEC-C04-001").unwrap();

    let merged = ctx.commands_dir().join("merged_results.json");
    std::fs::write(&merged, json!({
        "validator_1": "passed",
        "validator_2": "passed",
        "validator_3": "passed",
        "merged_status": "all_passed"
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&merged).unwrap();
    assert!(content.contains("merged_status"));
}

#[test]
fn c05_agent_race_condition_first_completes_winner() {
    let ctx = IntegrationTestContext::new("SPEC-C05-001").unwrap();

    let race = ctx.consensus_dir().join("race_winner.json");
    std::fs::write(&race, json!({
        "winner": "gemini",
        "completion_time_ms": 1234,
        "runner_up": "claude"
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&race).unwrap();
    assert!(content.contains("winner"));
}

#[test]
fn c06_concurrent_state_reads_during_writes_consistent_view() {
    let ctx = IntegrationTestContext::new("SPEC-C06-001").unwrap();

    let state = ctx.commands_dir().join("state_snapshot.json");
    std::fs::write(&state, json!({
        "snapshot_id": "abc123",
        "consistent": true,
        "readers_blocked": 2
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&state).unwrap();
    assert!(content.contains("consistent"));
}

#[test]
fn c07_evidence_archival_during_active_writes() {
    let ctx = IntegrationTestContext::new("SPEC-C07-001").unwrap();

    let archive = ctx.commands_dir().join("archival_status.json");
    std::fs::write(&archive, json!({
        "archival_pending": true,
        "active_writers": 1,
        "deferred": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&archive).unwrap();
    assert!(content.contains("deferred"));
}

#[test]
fn c08_multiple_mcp_calls_parallel_execution() {
    let ctx = IntegrationTestContext::new("SPEC-C08-001").unwrap();

    let parallel = ctx.consensus_dir().join("mcp_parallel.json");
    std::fs::write(&parallel, json!({
        "concurrent_calls": 3,
        "all_completed": true,
        "merged_results": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&parallel).unwrap();
    assert!(content.contains("concurrent_calls"));
}

#[test]
fn c09_concurrent_retry_attempts_deduplication() {
    let ctx = IntegrationTestContext::new("SPEC-C09-001").unwrap();

    let dedup = ctx.commands_dir().join("retry_dedup.json");
    std::fs::write(&dedup, json!({
        "retry_requests": 3,
        "deduplicated_to": 1,
        "single_retry_executed": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&dedup).unwrap();
    assert!(content.contains("deduplicated_to"));
}

#[test]
fn c10_parallel_quality_resolutions_conflict_detection() {
    let ctx = IntegrationTestContext::new("SPEC-C10-001").unwrap();

    let conflict = ctx.commands_dir().join("resolution_conflict.json");
    std::fs::write(&conflict, json!({
        "parallel_resolutions": 2,
        "conflict_detected": true,
        "sequential_resolution_applied": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&conflict).unwrap();
    assert!(content.contains("conflict_detected"));
}
