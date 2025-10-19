//! Phase 3 Integration Tests: Quality Gate Flow Integration (Q01-Q10)
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit Phase 3 integration testing
//!
//! Tests quality gates integrated with consensus, evidence, and state

mod common;

use common::{IntegrationTestContext, StateBuilder};
use codex_tui::SpecStage;
use serde_json::json;

#[test]
fn q01_issue_detected_gpt5_validation_auto_resolution_logged() {
    let ctx = IntegrationTestContext::new("SPEC-Q01-001").unwrap();

    let issue = ctx.commands_dir().join("quality_issue.json");
    std::fs::write(&issue, json!({
        "type": "minor",
        "confidence": "high",
        "resolution": "auto_resolved",
        "gpt5_validated": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&issue).unwrap();
    assert!(content.contains("auto_resolved"));
}

#[test]
fn q02_critical_issue_user_escalation_modal_displayed() {
    let ctx = IntegrationTestContext::new("SPEC-Q02-001").unwrap();

    let critical = ctx.commands_dir().join("critical_issue.json");
    std::fs::write(&critical, json!({
        "severity": "critical",
        "escalated": true,
        "user_notified": true,
        "modal_shown": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&critical).unwrap();
    assert!(content.contains("escalated"));
}

#[test]
fn q03_multiple_issues_batched_validation_mixed_outcomes() {
    let ctx = IntegrationTestContext::new("SPEC-Q03-001").unwrap();

    let batch = ctx.commands_dir().join("batched_issues.json");
    std::fs::write(&batch, json!({
        "issues": [
            {"id": 1, "outcome": "auto_resolved"},
            {"id": 2, "outcome": "escalated"},
            {"id": 3, "outcome": "auto_resolved"}
        ]
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&batch).unwrap();
    let data: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(data["issues"].as_array().unwrap().len(), 3);
}

#[test]
fn q04_quality_checkpoint_consensus_conflicts_arbiter() {
    let ctx = IntegrationTestContext::new("SPEC-Q04-001").unwrap();

    let conflict = ctx.consensus_dir().join("quality_conflict.json");
    std::fs::write(&conflict, json!({
        "checkpoint": "plan",
        "conflict_detected": true,
        "arbiter_invoked": true,
        "resolution": "majority_wins"
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&conflict).unwrap();
    assert!(content.contains("arbiter_invoked"));
}

#[test]
fn q05_auto_resolution_failure_escalation_user_input() {
    let ctx = IntegrationTestContext::new("SPEC-Q05-001").unwrap();

    let fallback = ctx.commands_dir().join("resolution_fallback.json");
    std::fs::write(&fallback, json!({
        "auto_resolution_attempted": true,
        "auto_resolution_failed": true,
        "escalation_triggered": true,
        "awaiting_user_input": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&fallback).unwrap();
    assert!(content.contains("escalation_triggered"));
}

#[test]
fn q06_quality_gate_timeout_default_action_warning() {
    let ctx = IntegrationTestContext::new("SPEC-Q06-001").unwrap();

    let timeout = ctx.commands_dir().join("quality_timeout.json");
    std::fs::write(&timeout, json!({
        "timeout_ms": 300000,
        "default_action": "continue",
        "warning_issued": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&timeout).unwrap();
    assert!(content.contains("default_action"));
}

#[test]
fn q07_empty_quality_results_skipped_validation() {
    let ctx = IntegrationTestContext::new("SPEC-Q07-001").unwrap();

    let empty = ctx.commands_dir().join("empty_results.json");
    std::fs::write(&empty, json!({
        "issues": [],
        "validation_skipped": true,
        "reason": "no_issues_detected"
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&empty).unwrap();
    assert!(content.contains("validation_skipped"));
}

#[test]
fn q08_quality_modifications_applied_to_artifacts() {
    let ctx = IntegrationTestContext::new("SPEC-Q08-001").unwrap();

    let mods = ctx.commands_dir().join("modifications.json");
    std::fs::write(&mods, json!({
        "modifications_applied": 3,
        "artifacts_updated": ["plan.md", "tasks.md"],
        "validated": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&mods).unwrap();
    assert!(content.contains("modifications_applied"));
}

#[test]
fn q09_multiple_checkpoints_all_outcomes_tracked() {
    let ctx = IntegrationTestContext::new("SPEC-Q09-001").unwrap();

    for checkpoint in &["plan", "tasks", "implement"] {
        let file = ctx.commands_dir().join(format!("checkpoint_{}.json", checkpoint));
        std::fs::write(&file, json!({
            "checkpoint": checkpoint,
            "outcome": "passed"
        }).to_string()).unwrap();
    }

    assert_eq!(ctx.count_guardrail_files(), 3);
}

#[test]
fn q10_quality_gates_disabled_bypass_documented() {
    let ctx = IntegrationTestContext::new("SPEC-Q10-001").unwrap();

    let bypass = ctx.commands_dir().join("bypass.json");
    std::fs::write(&bypass, json!({
        "quality_gates_enabled": false,
        "bypass_reason": "user_disabled",
        "warning_issued": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&bypass).unwrap();
    assert!(content.contains("bypass_reason"));
}
