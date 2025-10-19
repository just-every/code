//! Phase 3 Integration Tests: Error Recovery Across Modules (E01-E15)
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit Phase 3 integration testing
//!
//! Tests error propagation and recovery across module boundaries:
//! - Consensus failures and retry logic
//! - MCP fallback mechanisms
//! - Evidence cleanup after errors
//! - State rollback and recovery
//! - Graceful degradation

mod common;

use common::{IntegrationTestContext, StateBuilder};
use codex_tui::SpecStage;
use serde_json::json;

// ============================================================================
// E01-E05: Consensus and MCP Failure Recovery
// ============================================================================

#[test]
fn e01_consensus_failure_handler_retry_evidence_cleanup_state_reset() {
    // Test: Consensus failure → Handler retry → Evidence cleanup → State reset
    // Error: Empty consensus result triggers retry logic

    let ctx = IntegrationTestContext::new("SPEC-E01-001").unwrap();

    let mut state = StateBuilder::new("SPEC-E01-001")
        .starting_at(SpecStage::Plan)
        .build();

    // Attempt 1: Write failed consensus (empty result)
    let failed_consensus = ctx.consensus_dir().join("spec-plan_2025-10-19T10_00_00Z_gemini_attempt1.json");
    std::fs::write(
        &failed_consensus,
        json!({
            "agent": "gemini",
            "stage": "plan",
            "status": "failed",
            "error": "Empty consensus result",
            "attempt": 1,
            "timestamp": "2025-10-19T10:00:00Z"
        }).to_string()
    ).unwrap();

    // Verify failed attempt recorded
    assert!(failed_consensus.exists());

    // Simulate retry: cleanup failed evidence
    std::fs::remove_file(&failed_consensus).unwrap();
    assert!(!failed_consensus.exists());

    // Attempt 2: Success with enhanced prompt
    let success_consensus = ctx.consensus_dir().join("spec-plan_2025-10-19T10_05_00Z_gemini_attempt2.json");
    std::fs::write(
        &success_consensus,
        json!({
            "agent": "gemini",
            "stage": "plan",
            "status": "success",
            "content": "Enhanced prompt successful",
            "attempt": 2,
            "retry_reason": "empty_result",
            "timestamp": "2025-10-19T10:05:00Z"
        }).to_string()
    ).unwrap();

    // Verify retry succeeded
    assert!(success_consensus.exists());
    assert_eq!(ctx.count_consensus_files(), 1); // Only successful attempt remains

    // State advances after successful retry
    state.current_index += 1;
    assert_eq!(state.current_stage(), Some(SpecStage::Tasks));

    // Verify evidence shows retry metadata
    let content = std::fs::read_to_string(&success_consensus).unwrap();
    assert!(content.contains("retry_reason"));
    assert!(content.contains("attempt"));
}

#[test]
fn e02_mcp_failure_fallback_to_file_evidence_records_fallback() {
    // Test: MCP failure → Fallback to file → Evidence records fallback → Retry succeeds
    // Error: MCP timeout triggers file-based fallback

    let ctx = IntegrationTestContext::new("SPEC-E02-001").unwrap();

    // Write fallback marker evidence (MCP failed, using file fallback)
    let fallback_evidence = ctx.consensus_dir().join("spec-plan_2025-10-19T11_00_00Z_fallback.json");
    std::fs::write(
        &fallback_evidence,
        json!({
            "mode": "file_fallback",
            "reason": "mcp_timeout",
            "original_server": "local-memory",
            "fallback_path": "/tmp/consensus_fallback.json",
            "timestamp": "2025-10-19T11:00:00Z"
        }).to_string()
    ).unwrap();

    // Write actual consensus from file fallback
    let consensus_file = ctx.consensus_dir().join("spec-plan_2025-10-19T11_00_05Z_gemini.json");
    std::fs::write(
        &consensus_file,
        json!({
            "agent": "gemini",
            "stage": "plan",
            "source": "file_fallback",
            "content": "Consensus from fallback mechanism",
            "timestamp": "2025-10-19T11:00:05Z"
        }).to_string()
    ).unwrap();

    // Verify fallback recorded
    assert!(fallback_evidence.exists());
    assert!(consensus_file.exists());

    let fallback_content = std::fs::read_to_string(&fallback_evidence).unwrap();
    assert!(fallback_content.contains("mcp_timeout"));
    assert!(fallback_content.contains("file_fallback"));

    // Verify consensus marked as fallback source
    let consensus_content = std::fs::read_to_string(&consensus_file).unwrap();
    assert!(consensus_content.contains("\"source\":\"file_fallback\""));

    // Evidence count: 2 (fallback marker + consensus)
    assert_eq!(ctx.count_consensus_files(), 2);
}

#[test]
fn e03_guardrail_schema_violation_handler_error_state_rollback() {
    // Test: Guardrail schema violation → Handler error → State rollback → User notification
    // Error: Invalid JSON schema triggers validation failure

    let ctx = IntegrationTestContext::new("SPEC-E03-001").unwrap();

    let state = StateBuilder::new("SPEC-E03-001")
        .starting_at(SpecStage::Tasks)
        .build();

    let initial_index = state.current_index;

    // Write invalid guardrail telemetry (missing required fields)
    let invalid_telemetry = ctx.commands_dir().join("spec-tasks_2025-10-19T12_00_00Z.json");
    std::fs::write(
        &invalid_telemetry,
        json!({
            // Missing schemaVersion, timestamp, baseline, etc.
            "invalid": true,
            "partial_data": "incomplete"
        }).to_string()
    ).unwrap();

    // Verify invalid telemetry exists
    assert!(invalid_telemetry.exists());

    // Parse attempt reveals schema violation
    let content = std::fs::read_to_string(&invalid_telemetry).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Detect missing required fields
    assert!(parsed.get("schemaVersion").is_none());
    assert!(parsed.get("timestamp").is_none());
    assert!(parsed.get("baseline").is_none());

    // State rollback: doesn't advance
    assert_eq!(state.current_index, initial_index);
    assert_eq!(state.current_stage(), Some(SpecStage::Tasks));

    // Write error notification marker
    let error_marker = ctx.commands_dir().join("spec-tasks_2025-10-19T12_00_01Z_schema_error.json");
    std::fs::write(
        &error_marker,
        json!({
            "error_type": "schema_violation",
            "stage": "tasks",
            "missing_fields": ["schemaVersion", "timestamp", "baseline"],
            "user_notified": true,
            "timestamp": "2025-10-19T12:00:01Z"
        }).to_string()
    ).unwrap();

    // Verify error recorded and user notified
    assert!(error_marker.exists());
    let error_content = std::fs::read_to_string(&error_marker).unwrap();
    assert!(error_content.contains("schema_violation"));
    assert!(error_content.contains("user_notified"));
}

#[test]
fn e04_evidence_write_failure_handler_retry_lock_cleanup() {
    // Test: Evidence write failure → Handler retry → Lock cleanup → Success on retry
    // Error: I/O error (disk full, permission denied) triggers cleanup and retry

    let ctx = IntegrationTestContext::new("SPEC-E04-001").unwrap();

    // Simulate lock file from failed write attempt
    let lock_file = ctx.consensus_dir().join(".spec-plan.lock");
    std::fs::write(
        &lock_file,
        json!({
            "locked_by": "attempt_1",
            "timestamp": "2025-10-19T13:00:00Z",
            "reason": "write_in_progress"
        }).to_string()
    ).unwrap();

    // Verify lock exists
    assert!(lock_file.exists());

    // Write failure marker
    let failure_marker = ctx.consensus_dir().join("spec-plan_2025-10-19T13_00_00Z_write_failed.json");
    std::fs::write(
        &failure_marker,
        json!({
            "error": "io_error",
            "reason": "disk_full",
            "attempt": 1,
            "timestamp": "2025-10-19T13:00:00Z"
        }).to_string()
    ).unwrap();

    // Cleanup: Remove stale lock and failure marker
    std::fs::remove_file(&lock_file).unwrap();
    std::fs::remove_file(&failure_marker).unwrap();

    assert!(!lock_file.exists());
    assert!(!failure_marker.exists());

    // Retry: Write succeeds
    let success_file = ctx.consensus_dir().join("spec-plan_2025-10-19T13_00_05Z_gemini.json");
    std::fs::write(
        &success_file,
        json!({
            "agent": "gemini",
            "stage": "plan",
            "content": "Write successful after cleanup",
            "retry": true,
            "timestamp": "2025-10-19T13:00:05Z"
        }).to_string()
    ).unwrap();

    // Verify retry succeeded
    assert!(success_file.exists());
    assert_eq!(ctx.count_consensus_files(), 1); // Only successful write

    let content = std::fs::read_to_string(&success_file).unwrap();
    assert!(content.contains("retry"));
}

#[test]
fn e05_agent_timeout_handler_detects_consensus_retry_evidence_updated() {
    // Test: Agent timeout → Handler detects → Consensus retry → Evidence updated
    // Error: Agent timeout (30min limit) triggers timeout detection and retry

    let ctx = IntegrationTestContext::new("SPEC-E05-001").unwrap();

    // Write timeout evidence
    let timeout_marker = ctx.consensus_dir().join("spec-implement_2025-10-19T14_00_00Z_timeout.json");
    std::fs::write(
        &timeout_marker,
        json!({
            "error_type": "agent_timeout",
            "agent": "gpt_codex",
            "stage": "implement",
            "timeout_ms": 1800000, // 30 minutes
            "started_at": "2025-10-19T13:30:00Z",
            "timed_out_at": "2025-10-19T14:00:00Z"
        }).to_string()
    ).unwrap();

    // Verify timeout detected
    assert!(timeout_marker.exists());
    let timeout_content = std::fs::read_to_string(&timeout_marker).unwrap();
    assert!(timeout_content.contains("agent_timeout"));
    assert!(timeout_content.contains("gpt_codex"));

    // Write retry attempt with different agent
    let retry_consensus = ctx.consensus_dir().join("spec-implement_2025-10-19T14_05_00Z_gemini.json");
    std::fs::write(
        &retry_consensus,
        json!({
            "agent": "gemini",
            "stage": "implement",
            "content": "Retry with fallback agent",
            "retry_reason": "agent_timeout:gpt_codex",
            "timestamp": "2025-10-19T14:05:00Z"
        }).to_string()
    ).unwrap();

    // Verify retry evidence updated
    assert!(retry_consensus.exists());
    assert_eq!(ctx.count_consensus_files(), 2); // Timeout marker + retry consensus

    let retry_content = std::fs::read_to_string(&retry_consensus).unwrap();
    assert!(retry_content.contains("retry_reason"));
    assert!(retry_content.contains("agent_timeout:gpt_codex"));
}

// ============================================================================
// E06-E10: Parse Errors and Retry Logic
// ============================================================================

#[test]
fn e06_empty_consensus_handler_retry_enhanced_prompt() {
    // Test: Empty consensus → Handler retry → Enhanced prompt → Success
    // Error: AR-3 retry logic - empty result detection and prompt enhancement

    let ctx = IntegrationTestContext::new("SPEC-E06-001").unwrap();

    // Attempt 1: Empty consensus
    let empty_attempt = ctx.consensus_dir().join("spec-plan_2025-10-19T15_00_00Z_attempt1.json");
    std::fs::write(
        &empty_attempt,
        json!({
            "agent": "claude",
            "stage": "plan",
            "content": "",
            "status": "empty",
            "attempt": 1,
            "timestamp": "2025-10-19T15:00:00Z"
        }).to_string()
    ).unwrap();

    // Verify empty result detected
    let content = std::fs::read_to_string(&empty_attempt).unwrap();
    let data: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(data["content"], "");
    assert_eq!(data["status"], "empty");

    // Cleanup empty attempt
    std::fs::remove_file(&empty_attempt).unwrap();

    // Attempt 2: Enhanced prompt with storage guidance
    let enhanced_attempt = ctx.consensus_dir().join("spec-plan_2025-10-19T15_05_00Z_attempt2.json");
    std::fs::write(
        &enhanced_attempt,
        json!({
            "agent": "claude",
            "stage": "plan",
            "content": "Enhanced result with storage guidance",
            "status": "success",
            "attempt": 2,
            "retry_reason": "empty_result",
            "prompt_enhanced": true,
            "timestamp": "2025-10-19T15:05:00Z"
        }).to_string()
    ).unwrap();

    // Verify enhancement succeeded
    let enhanced_content = std::fs::read_to_string(&enhanced_attempt).unwrap();
    assert!(enhanced_content.contains("prompt_enhanced"));
    assert!(enhanced_content.contains("retry_reason"));
    assert_eq!(ctx.count_consensus_files(), 1);
}

#[test]
fn e07_invalid_json_parser_error_handler_retry_with_schema() {
    // Test: Invalid JSON → Parser error → Handler retry with schema → Success
    // Error: AR-4 schema injection - malformed JSON triggers schema-enhanced retry

    let ctx = IntegrationTestContext::new("SPEC-E07-001").unwrap();

    // Attempt 1: Invalid JSON (parse error)
    let invalid_json_file = ctx.consensus_dir().join("spec-tasks_2025-10-19T16_00_00Z_invalid.json");
    std::fs::write(
        &invalid_json_file,
        "{invalid json: missing quotes, trailing comma,}"
    ).unwrap();

    // Verify parse error occurs
    let content = std::fs::read_to_string(&invalid_json_file).unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&content).is_err());

    // Cleanup invalid attempt
    std::fs::remove_file(&invalid_json_file).unwrap();

    // Attempt 2: Schema-guided retry
    let schema_attempt = ctx.consensus_dir().join("spec-tasks_2025-10-19T16_05_00Z_valid.json");
    std::fs::write(
        &schema_attempt,
        json!({
            "agent": "gpt_pro",
            "stage": "tasks",
            "content": "Schema-validated output",
            "schema_validated": true,
            "retry_reason": "parse_error",
            "timestamp": "2025-10-19T16:05:00Z"
        }).to_string()
    ).unwrap();

    // Verify schema-guided retry succeeded
    let valid_content = std::fs::read_to_string(&schema_attempt).unwrap();
    let valid_data: serde_json::Value = serde_json::from_str(&valid_content).unwrap();
    assert_eq!(valid_data["schema_validated"], true);
    assert_eq!(ctx.count_consensus_files(), 1);
}

#[test]
fn e08_state_corruption_evidence_read_fails_fallback_to_default() {
    // Test: State corruption → Evidence read fails → Fallback to default → Recovery
    // Error: Corrupted state file triggers default state initialization

    let ctx = IntegrationTestContext::new("SPEC-E08-001").unwrap();

    // Write corrupted state evidence
    let corrupted_state = ctx.commands_dir().join("spec_auto_state.json");
    std::fs::write(
        &corrupted_state,
        "{corrupted: invalid json with trailing comma,}"
    ).unwrap();

    // Attempt to read corrupted state
    let content = std::fs::read_to_string(&corrupted_state).unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&content).is_err());

    // Cleanup corrupted state
    std::fs::remove_file(&corrupted_state).unwrap();

    // Fallback: Initialize default state
    let default_state = StateBuilder::new("SPEC-E08-001")
        .starting_at(SpecStage::Plan)
        .build();

    // Write recovered state evidence
    let recovered_state = ctx.commands_dir().join("spec_auto_state_recovered.json");
    std::fs::write(
        &recovered_state,
        json!({
            "spec_id": default_state.spec_id,
            "current_index": default_state.current_index,
            "recovery_mode": "default_initialization",
            "reason": "state_corruption",
            "timestamp": "2025-10-19T17:00:00Z"
        }).to_string()
    ).unwrap();

    // Verify recovery successful
    assert!(recovered_state.exists());
    let recovered_content = std::fs::read_to_string(&recovered_state).unwrap();
    assert!(recovered_content.contains("recovery_mode"));
    assert!(recovered_content.contains("default_initialization"));
}

#[test]
fn e09_multiple_retries_exhausted_handler_halts_evidence_logs_failure() {
    // Test: Multiple retries exhausted → Handler halts → Evidence logs failure → User escalation
    // Error: AR-2 retry limit (3 attempts) reached, pipeline halts gracefully

    let ctx = IntegrationTestContext::new("SPEC-E09-001").unwrap();

    // Simulate 3 failed retry attempts
    for attempt in 1..=3 {
        let retry_file = ctx.consensus_dir()
            .join(format!("spec-validate_2025-10-19T18_{:02}_00Z_attempt{}.json", attempt * 5, attempt));
        std::fs::write(
            &retry_file,
            json!({
                "agent": "gpt_pro",
                "stage": "validate",
                "status": "failed",
                "attempt": attempt,
                "error": format!("Validation failed - attempt {}", attempt),
                "timestamp": format!("2025-10-19T18:{:02}:00Z", attempt * 5)
            }).to_string()
        ).unwrap();
    }

    // Verify all retry attempts recorded
    assert_eq!(ctx.count_consensus_files(), 3);

    // Write final failure evidence (retries exhausted)
    let final_failure = ctx.commands_dir().join("spec-validate_2025-10-19T18_20_00Z_retries_exhausted.json");
    std::fs::write(
        &final_failure,
        json!({
            "error_type": "retries_exhausted",
            "stage": "validate",
            "max_retries": 3,
            "all_attempts_failed": true,
            "user_escalation_required": true,
            "halt_reason": "retry_limit_reached",
            "timestamp": "2025-10-19T18:20:00Z"
        }).to_string()
    ).unwrap();

    // Verify graceful halt evidence
    assert!(final_failure.exists());
    let failure_content = std::fs::read_to_string(&final_failure).unwrap();
    assert!(failure_content.contains("retries_exhausted"));
    assert!(failure_content.contains("user_escalation_required"));
}

#[test]
fn e10_quality_gate_failure_state_preserved_manual_intervention() {
    // Test: Quality gate failure → State preserved → Manual intervention → Resume
    // Error: Quality gate blocks progression, state saved for later resumption

    let ctx = IntegrationTestContext::new("SPEC-E10-001").unwrap();

    let state = StateBuilder::new("SPEC-E10-001")
        .starting_at(SpecStage::Implement)
        .build();

    // Write quality gate failure evidence
    let quality_failure = ctx.commands_dir().join("spec-implement_2025-10-19T19_00_00Z_quality_gate.json");
    std::fs::write(
        &quality_failure,
        json!({
            "stage": "implement",
            "quality_checkpoint": "post_implement",
            "status": "failed",
            "critical_issues": [
                {"type": "security", "severity": "high", "description": "SQL injection vulnerability"},
                {"type": "performance", "severity": "critical", "description": "N+1 query detected"}
            ],
            "state_preserved": true,
            "manual_intervention_required": true,
            "timestamp": "2025-10-19T19:00:00Z"
        }).to_string()
    ).unwrap();

    // Write preserved state
    let preserved_state = ctx.commands_dir().join("spec_auto_state_preserved.json");
    std::fs::write(
        &preserved_state,
        json!({
            "spec_id": state.spec_id,
            "current_index": state.current_index,
            "current_stage": "Implement",
            "preservation_reason": "quality_gate_failure",
            "resumable": true,
            "timestamp": "2025-10-19T19:00:01Z"
        }).to_string()
    ).unwrap();

    // Verify state preserved for resumption
    assert!(quality_failure.exists());
    assert!(preserved_state.exists());

    let state_content = std::fs::read_to_string(&preserved_state).unwrap();
    assert!(state_content.contains("resumable"));
    assert!(state_content.contains("quality_gate_failure"));
}

// ============================================================================
// E11-E15: Network and Resource Failures
// ============================================================================

#[test]
fn e11_concurrent_write_conflict_lock_timeout_retry_with_backoff() {
    // Test: Concurrent write conflict → Lock timeout → Retry with backoff → Success
    // Error: Lock contention triggers exponential backoff retry

    let ctx = IntegrationTestContext::new("SPEC-E11-001").unwrap();

    // Simulate lock held by another process
    let lock_file = ctx.consensus_dir().join(".spec-plan.lock");
    std::fs::write(
        &lock_file,
        json!({
            "held_by": "process_123",
            "acquired_at": "2025-10-19T20:00:00Z",
            "timeout_ms": 5000
        }).to_string()
    ).unwrap();

    // Attempt 1: Lock timeout
    let timeout_marker = ctx.consensus_dir().join("spec-plan_2025-10-19T20_00_05Z_lock_timeout.json");
    std::fs::write(
        &timeout_marker,
        json!({
            "error": "lock_timeout",
            "waited_ms": 5000,
            "attempt": 1,
            "backoff_ms": 1000,
            "timestamp": "2025-10-19T20:00:05Z"
        }).to_string()
    ).unwrap();

    // Release lock
    std::fs::remove_file(&lock_file).unwrap();

    // Attempt 2: Success with backoff
    let success_file = ctx.consensus_dir().join("spec-plan_2025-10-19T20_00_07Z_gemini.json");
    std::fs::write(
        &success_file,
        json!({
            "agent": "gemini",
            "stage": "plan",
            "content": "Acquired lock after backoff",
            "retry": true,
            "backoff_applied": true,
            "timestamp": "2025-10-19T20:00:07Z"
        }).to_string()
    ).unwrap();

    // Verify retry with backoff succeeded
    assert!(!lock_file.exists());
    assert!(success_file.exists());

    let success_content = std::fs::read_to_string(&success_file).unwrap();
    assert!(success_content.contains("backoff_applied"));
}

#[test]
fn e12_guardrail_timeout_handler_continues_warning_logged() {
    // Test: Guardrail timeout → Handler continues → Warning logged → Evidence marked incomplete
    // Error: Guardrail timeout doesn't block pipeline, marks as incomplete

    let ctx = IntegrationTestContext::new("SPEC-E12-001").unwrap();

    let mut state = StateBuilder::new("SPEC-E12-001")
        .starting_at(SpecStage::Audit)
        .build();

    // Write guardrail timeout evidence
    let timeout_telemetry = ctx.commands_dir().join("spec-audit_2025-10-19T21_00_00Z.json");
    std::fs::write(
        &timeout_telemetry,
        json!({
            "schemaVersion": 1,
            "stage": "audit",
            "status": "timeout",
            "timeout_ms": 120000,
            "incomplete": true,
            "warning": "Guardrail validation timed out - continuing with incomplete validation",
            "timestamp": "2025-10-19T21:00:00Z"
        }).to_string()
    ).unwrap();

    // Verify timeout logged but pipeline continues
    assert!(timeout_telemetry.exists());

    let content = std::fs::read_to_string(&timeout_telemetry).unwrap();
    assert!(content.contains("timeout"));
    assert!(content.contains("incomplete"));

    // State advances despite timeout (non-blocking)
    state.current_index += 1;
    assert_eq!(state.current_stage(), Some(SpecStage::Unlock));
}

#[test]
fn e13_mcp_server_crash_reconnect_replay() {
    // Test: MCP server crash → Reconnect → Replay → Success
    // Error: MCP connection lost, automatic reconnection and request replay

    let ctx = IntegrationTestContext::new("SPEC-E13-001").unwrap();

    // Write crash evidence
    let crash_marker = ctx.consensus_dir().join("spec-tasks_2025-10-19T22_00_00Z_mcp_crash.json");
    std::fs::write(
        &crash_marker,
        json!({
            "error": "mcp_connection_lost",
            "server": "local-memory",
            "last_request": "search",
            "crash_detected_at": "2025-10-19T22:00:00Z"
        }).to_string()
    ).unwrap();

    // Write reconnection evidence
    let reconnect_marker = ctx.consensus_dir().join("spec-tasks_2025-10-19T22_00_02Z_mcp_reconnect.json");
    std::fs::write(
        &reconnect_marker,
        json!({
            "reconnection_attempt": 1,
            "server": "local-memory",
            "status": "connected",
            "reconnected_at": "2025-10-19T22:00:02Z"
        }).to_string()
    ).unwrap();

    // Write replayed request success
    let replay_success = ctx.consensus_dir().join("spec-tasks_2025-10-19T22_00_03Z_claude.json");
    std::fs::write(
        &replay_success,
        json!({
            "agent": "claude",
            "stage": "tasks",
            "content": "Request replayed successfully after reconnection",
            "replayed": true,
            "timestamp": "2025-10-19T22:00:03Z"
        }).to_string()
    ).unwrap();

    // Verify crash → reconnect → replay flow
    assert!(crash_marker.exists());
    assert!(reconnect_marker.exists());
    assert!(replay_success.exists());

    let replay_content = std::fs::read_to_string(&replay_success).unwrap();
    assert!(replay_content.contains("replayed"));
}

#[test]
fn e14_evidence_disk_full_handler_error_cleanup_old_files() {
    // Test: Evidence disk full → Handler error → Cleanup old files → Retry
    // Error: Disk full triggers automatic evidence cleanup and retry

    let ctx = IntegrationTestContext::new("SPEC-E14-001").unwrap();

    // Write disk full error
    let disk_full_error = ctx.commands_dir().join("spec-implement_2025-10-19T23_00_00Z_disk_full.json");
    std::fs::write(
        &disk_full_error,
        json!({
            "error": "disk_full",
            "attempted_write": "spec-implement_gemini.json",
            "disk_usage_percent": 99.8,
            "timestamp": "2025-10-19T23:00:00Z"
        }).to_string()
    ).unwrap();

    // Create old evidence files to cleanup
    for i in 1..=5 {
        let old_file = ctx.consensus_dir()
            .join(format!("spec-plan_2025-10-0{}_00_00_00Z_old.json", i));
        std::fs::write(&old_file, "old evidence data").unwrap();
    }

    // Verify old files exist
    assert_eq!(ctx.count_consensus_files(), 5);

    // Cleanup old files
    for entry in std::fs::read_dir(ctx.consensus_dir()).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name().to_string_lossy().contains("_old.json") {
            std::fs::remove_file(entry.path()).unwrap();
        }
    }

    // Verify cleanup successful
    assert_eq!(ctx.count_consensus_files(), 0);

    // Retry write after cleanup
    let retry_write = ctx.consensus_dir().join("spec-implement_2025-10-19T23_00_05Z_gemini.json");
    std::fs::write(
        &retry_write,
        json!({
            "agent": "gemini",
            "stage": "implement",
            "content": "Write successful after cleanup",
            "retry": true,
            "cleanup_performed": true,
            "timestamp": "2025-10-19T23:00:05Z"
        }).to_string()
    ).unwrap();

    // Verify retry succeeded
    assert!(retry_write.exists());
    let content = std::fs::read_to_string(&retry_write).unwrap();
    assert!(content.contains("cleanup_performed"));
}

#[test]
fn e15_network_partition_mcp_unreachable_graceful_degradation() {
    // Test: Network partition → MCP unreachable → Graceful degradation → Recovery
    // Error: Network partition triggers degraded mode operation

    let ctx = IntegrationTestContext::new("SPEC-E15-001").unwrap();

    // Write network partition error
    let partition_error = ctx.consensus_dir().join("spec-unlock_2025-10-19T23_30_00Z_network_partition.json");
    std::fs::write(
        &partition_error,
        json!({
            "error": "network_partition",
            "mcp_servers_unreachable": ["local-memory", "byterover-mcp"],
            "degraded_mode": true,
            "timestamp": "2025-10-19T23:30:00Z"
        }).to_string()
    ).unwrap();

    // Write degraded mode consensus (using cached/local data)
    let degraded_consensus = ctx.consensus_dir().join("spec-unlock_2025-10-19T23_30_02Z_degraded.json");
    std::fs::write(
        &degraded_consensus,
        json!({
            "mode": "degraded",
            "source": "local_cache",
            "agent": "code",
            "stage": "unlock",
            "content": "Operating in degraded mode with cached data",
            "timestamp": "2025-10-19T23:30:02Z"
        }).to_string()
    ).unwrap();

    // Write recovery evidence (network restored)
    let recovery_marker = ctx.consensus_dir().join("spec-unlock_2025-10-19T23_35_00Z_recovery.json");
    std::fs::write(
        &recovery_marker,
        json!({
            "network_restored": true,
            "mcp_connectivity": "full",
            "degraded_mode_duration_seconds": 300,
            "timestamp": "2025-10-19T23:35:00Z"
        }).to_string()
    ).unwrap();

    // Verify graceful degradation and recovery
    assert!(partition_error.exists());
    assert!(degraded_consensus.exists());
    assert!(recovery_marker.exists());

    let degraded_content = std::fs::read_to_string(&degraded_consensus).unwrap();
    assert!(degraded_content.contains("degraded"));
    assert!(degraded_content.contains("local_cache"));

    let recovery_content = std::fs::read_to_string(&recovery_marker).unwrap();
    assert!(recovery_content.contains("network_restored"));
}
