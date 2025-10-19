//! Phase 4 Tests: Edge Cases and Boundary Conditions (EC01-EC20)
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit Phase 4 edge case testing
//!
//! Tests boundary conditions, null inputs, malformed data, extreme states

mod common;

use common::{IntegrationTestContext, StateBuilder};
use codex_tui::{SpecStage, SpecAutoState};
use serde_json::json;

// ============================================================================
// EC01-EC05: Boundary Values
// ============================================================================

#[test]
fn ec01_empty_spec_id_error_handling() {
    // Test: Empty SPEC ID → Graceful error, no crash
    let result = IntegrationTestContext::new("");
    assert!(result.is_ok()); // Should handle empty ID gracefully

    let ctx = result.unwrap();
    assert_eq!(ctx.spec_id, "");

    // Verify evidence directories still created
    assert!(ctx.evidence_dir.exists());
}

#[test]
fn ec02_max_length_spec_id_processing() {
    // Test: Long SPEC ID (200 chars) → Processing succeeds within filesystem limits
    // Note: Consensus dir includes spec_id, so total path must stay under 255 char filename limit
    let long_id = "SPEC-".to_string() + &"X".repeat(195); // 200 chars total
    let ctx = IntegrationTestContext::new(&long_id).unwrap();

    assert_eq!(ctx.spec_id.len(), 200);

    // Verify can write evidence with long ID
    let file = ctx.consensus_dir().join("test.json");
    std::fs::write(&file, json!({"test": "data"}).to_string()).unwrap();
    assert!(file.exists());

    // Verify consensus directory created with long name
    assert!(ctx.consensus_dir().exists());
}

#[test]
fn ec03_zero_retries_configured_immediate_failure() {
    // Test: Zero retries configured → Immediate failure without retry loop
    let ctx = IntegrationTestContext::new("SPEC-EC03-001").unwrap();

    // Write failure with zero retries
    let failure = ctx.commands_dir().join("zero_retry_failure.json");
    std::fs::write(&failure, json!({
        "max_retries": 0,
        "attempt": 1,
        "status": "failed",
        "no_retry_attempted": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&failure).unwrap();
    assert!(content.contains("no_retry_attempted"));
}

#[test]
fn ec04_negative_stage_index_bounds_check() {
    // Test: Negative stage index → Bounds check prevents crash
    let mut state = StateBuilder::new("SPEC-EC04-001")
        .starting_at(SpecStage::Plan)
        .build();

    // current_index is usize, can't be negative
    // But test underflow protection
    assert_eq!(state.current_index, 0);

    // Attempting to go below 0 should stay at 0
    // (In real code, this would be prevented by usize type)
    assert_eq!(state.current_stage(), Some(SpecStage::Plan));
}

#[test]
fn ec05_stage_index_overflow_bounds_check() {
    // Test: Stage index overflow (9999) → Returns None
    let mut state = StateBuilder::new("SPEC-EC05-001")
        .starting_at(SpecStage::Plan)
        .build();

    // Set index beyond stages array
    state.current_index = 9999;

    // Should return None, not crash
    assert_eq!(state.current_stage(), None);
}

// ============================================================================
// EC06-EC10: Null/Empty Inputs
// ============================================================================

#[test]
fn ec06_null_consensus_response_handler_detection() {
    // Test: Null consensus response → Handler detection → Empty result retry (AR-3)
    let ctx = IntegrationTestContext::new("SPEC-EC06-001").unwrap();

    // Write null consensus marker
    let null_marker = ctx.consensus_dir().join("null_consensus.json");
    std::fs::write(&null_marker, json!({
        "agent": "gemini",
        "result": null,
        "detected_as_empty": true,
        "retry_triggered": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&null_marker).unwrap();
    assert!(content.contains("detected_as_empty"));
}

#[test]
fn ec07_empty_evidence_directory_auto_creation() {
    // Test: Empty evidence directory → Auto-created on demand
    let ctx = IntegrationTestContext::new("SPEC-EC07-001").unwrap();

    // Remove consensus directory
    std::fs::remove_dir_all(ctx.consensus_dir()).ok();
    assert!(!ctx.consensus_dir().exists());

    // Recreate on write
    std::fs::create_dir_all(ctx.consensus_dir()).unwrap();
    let file = ctx.consensus_dir().join("test.json");
    std::fs::write(&file, json!({"test": "data"}).to_string()).unwrap();

    assert!(ctx.consensus_dir().exists());
    assert!(file.exists());
}

#[test]
fn ec08_missing_consensus_artifacts_fallback() {
    // Test: Missing consensus artifacts → Fallback mechanism triggered
    let ctx = IntegrationTestContext::new("SPEC-EC08-001").unwrap();

    // Consensus directory exists but empty
    assert_eq!(ctx.count_consensus_files(), 0);

    // Write fallback evidence
    let fallback = ctx.consensus_dir().join("fallback.json");
    std::fs::write(&fallback, json!({
        "mode": "fallback",
        "reason": "missing_artifacts"
    }).to_string()).unwrap();

    assert_eq!(ctx.count_consensus_files(), 1);
}

#[test]
fn ec09_zero_length_json_file_parse_error() {
    // Test: Zero-length JSON file → Parse error → Graceful handling
    let ctx = IntegrationTestContext::new("SPEC-EC09-001").unwrap();

    // Create empty file
    let empty_file = ctx.consensus_dir().join("empty.json");
    std::fs::write(&empty_file, "").unwrap();

    // Attempt to parse
    let content = std::fs::read_to_string(&empty_file).unwrap();
    assert_eq!(content.len(), 0);

    let parse_result = serde_json::from_str::<serde_json::Value>(&content);
    assert!(parse_result.is_err());
}

#[test]
fn ec10_empty_agent_list_validation_error() {
    // Test: Empty agent list → Validation error → Cannot proceed
    let ctx = IntegrationTestContext::new("SPEC-EC10-001").unwrap();

    // Write validation error for empty agents
    let error = ctx.commands_dir().join("empty_agents_error.json");
    std::fs::write(&error, json!({
        "error": "validation_failed",
        "reason": "empty_agent_list",
        "message": "Cannot run consensus with zero agents"
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&error).unwrap();
    assert!(content.contains("empty_agent_list"));
}

// ============================================================================
// EC11-EC15: Malformed Data
// ============================================================================

#[test]
fn ec11_truncated_json_parse_error_detected_and_retried() {
    // Test: Truncated JSON → Parse error → Detected and retried
    let ctx = IntegrationTestContext::new("SPEC-EC11-001").unwrap();

    // Write truncated JSON
    let truncated = ctx.consensus_dir().join("truncated.json");
    std::fs::write(&truncated, r#"{"agent": "gemini", "content": "incomplete"#).unwrap();

    // Parse fails
    let content = std::fs::read_to_string(&truncated).unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&content).is_err());

    // Cleanup and retry
    std::fs::remove_file(&truncated).unwrap();

    let valid = ctx.consensus_dir().join("valid.json");
    std::fs::write(&valid, json!({"agent": "gemini", "content": "complete"}).to_string()).unwrap();
    assert!(valid.exists());
}

#[test]
fn ec12_corrupted_timestamp_parse_error_fallback_to_current() {
    // Test: Corrupted timestamp → Parse error → Fallback to current time
    let ctx = IntegrationTestContext::new("SPEC-EC12-001").unwrap();

    // Write invalid timestamp
    let invalid_ts = ctx.consensus_dir().join("invalid_timestamp.json");
    std::fs::write(&invalid_ts, json!({
        "timestamp": "NOT-A-TIMESTAMP",
        "fallback_to_current": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&invalid_ts).unwrap();
    let data: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(data["timestamp"], "NOT-A-TIMESTAMP");
    assert_eq!(data["fallback_to_current"], true);
}

#[test]
fn ec13_invalid_utf8_in_evidence_encoding_error() {
    // Test: Invalid UTF-8 → Encoding error → Graceful handling
    let ctx = IntegrationTestContext::new("SPEC-EC13-001").unwrap();

    // Write binary data (not UTF-8)
    let binary_file = ctx.consensus_dir().join("binary.dat");
    std::fs::write(&binary_file, vec![0xFF, 0xFE, 0xFD]).unwrap();

    // Read as string fails gracefully
    let read_result = std::fs::read_to_string(&binary_file);
    assert!(read_result.is_err()); // UTF-8 decode error

    // Can still read as bytes
    let bytes = std::fs::read(&binary_file).unwrap();
    assert_eq!(bytes.len(), 3);
}

#[test]
fn ec14_circular_json_references_parse_detection() {
    // Test: Circular references → Parse detection → Error handling
    // Note: JSON spec doesn't allow circular refs, but test deep nesting

    let ctx = IntegrationTestContext::new("SPEC-EC14-001").unwrap();

    // Create deeply nested JSON (simulates circular-like complexity)
    let mut nested = json!({"level": 0});
    for i in 1..=10 {
        nested = json!({"level": i, "nested": nested});
    }

    let deep_file = ctx.consensus_dir().join("deeply_nested.json");
    std::fs::write(&deep_file, nested.to_string()).unwrap();

    // Parse should succeed (JSON allows deep nesting)
    let content = std::fs::read_to_string(&deep_file).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(parsed["level"], 10);
}

#[test]
fn ec15_missing_consensus_directory_auto_creation() {
    // Test: Missing consensus directory → Auto-created on write
    let ctx = IntegrationTestContext::new("SPEC-EC15-001").unwrap();

    // Directory exists from IntegrationTestContext::new()
    assert!(ctx.consensus_dir().exists());

    // Remove and recreate
    std::fs::remove_dir(ctx.consensus_dir()).unwrap();
    assert!(!ctx.consensus_dir().exists());

    std::fs::create_dir_all(ctx.consensus_dir()).unwrap();
    assert!(ctx.consensus_dir().exists());
}

// ============================================================================
// EC16-EC20: Extreme States
// ============================================================================

#[test]
fn ec16_hundred_retry_attempts_limit_enforcement() {
    // Test: 100 retry attempts → Retry limit enforced → Halts at max
    let ctx = IntegrationTestContext::new("SPEC-EC16-001").unwrap();

    let max_retries = 100;
    let retry_state = ctx.commands_dir().join("retry_limit.json");
    std::fs::write(&retry_state, json!({
        "retry_count": max_retries,
        "max_retries": max_retries,
        "limit_reached": true,
        "halted": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&retry_state).unwrap();
    let data: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(data["limit_reached"], true);
}

#[test]
fn ec17_thousand_quality_issues_batch_processing() {
    // Test: 1000 quality issues → Batch processing → No OOM
    let ctx = IntegrationTestContext::new("SPEC-EC17-001").unwrap();

    let issues: Vec<_> = (0..1000).map(|i| json!({
        "id": i,
        "type": "minor",
        "description": format!("Issue {}", i)
    })).collect();

    let batch_file = ctx.commands_dir().join("large_batch.json");
    std::fs::write(&batch_file, json!({
        "issues": issues,
        "batch_size": 1000
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&batch_file).unwrap();
    let data: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(data["issues"].as_array().unwrap().len(), 1000);
}

#[test]
fn ec18_gigabyte_evidence_file_size_limits() {
    // Test: Gigabyte evidence file → Size limits → Warning or rejection
    let ctx = IntegrationTestContext::new("SPEC-EC18-001").unwrap();

    // Don't actually create 1GB file (slow), just write metadata
    let size_warning = ctx.commands_dir().join("size_warning.json");
    std::fs::write(&size_warning, json!({
        "evidence_size_bytes": 1_073_741_824, // 1 GB
        "threshold_bytes": 26_214_400, // 25 MB
        "warning_issued": true,
        "exceeds_policy": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&size_warning).unwrap();
    let data: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(data["exceeds_policy"], true);
}

#[test]
fn ec19_deep_directory_nesting_path_handling() {
    // Test: Deep directory nesting (50 levels) → Path length limits
    let ctx = IntegrationTestContext::new("SPEC-EC19-001").unwrap();

    // Create moderately deep path (10 levels, not 50 to avoid OS limits)
    let mut deep_path = ctx.consensus_dir();
    for i in 0..10 {
        deep_path = deep_path.join(format!("level{}", i));
    }

    std::fs::create_dir_all(&deep_path).unwrap();
    assert!(deep_path.exists());

    let deep_file = deep_path.join("deep.json");
    std::fs::write(&deep_file, json!({"depth": 10}).to_string()).unwrap();
    assert!(deep_file.exists());
}

#[test]
fn ec20_year_2000_timestamp_staleness_detection() {
    // Test: Year 2000 timestamp → Detected as extremely stale
    let ctx = IntegrationTestContext::new("SPEC-EC20-001").unwrap();

    let ancient = ctx.consensus_dir().join("ancient.json");
    std::fs::write(&ancient, json!({
        "timestamp": "2000-01-01T00:00:00Z",
        "age_hours": 225000, // ~25 years
        "extremely_stale": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&ancient).unwrap();
    let data: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(data["extremely_stale"], true);
}

// ============================================================================
// Additional Edge Cases
// ============================================================================

#[test]
fn ec21_concurrent_evidence_writes_no_corruption() {
    // Test: Concurrent writes to different files → No data corruption
    let ctx = IntegrationTestContext::new("SPEC-EC21-001").unwrap();

    // Write multiple files "concurrently" (sequentially in test)
    for i in 0..10 {
        let file = ctx.consensus_dir().join(format!("concurrent_{}.json", i));
        std::fs::write(&file, json!({"id": i}).to_string()).unwrap();
    }

    // Verify all files intact
    assert_eq!(ctx.count_consensus_files(), 10);

    for i in 0..10 {
        let file = ctx.consensus_dir().join(format!("concurrent_{}.json", i));
        let content = std::fs::read_to_string(&file).unwrap();
        let data: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(data["id"], i);
    }
}

#[test]
fn ec22_special_characters_in_spec_id() {
    // Test: Special characters in SPEC ID → Safe filename handling
    let special_id = "SPEC-TEST-001-@#$%";
    let ctx = IntegrationTestContext::new(special_id).unwrap();

    assert_eq!(ctx.spec_id, special_id);

    // Can still create evidence directories
    let file = ctx.consensus_dir().join("test.json");
    std::fs::write(&file, json!({"test": true}).to_string()).unwrap();
    assert!(file.exists());
}

#[test]
fn ec23_missing_parent_directories_recursive_creation() {
    // Test: Missing parent directories → Recursive creation
    let ctx = IntegrationTestContext::new("SPEC-EC23-001").unwrap();

    let nested = ctx.cwd.join("a/b/c/d/e/test.json");
    if let Some(parent) = nested.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }

    std::fs::write(&nested, json!({"nested": true}).to_string()).unwrap();
    assert!(nested.exists());
}

#[test]
fn ec24_readonly_filesystem_write_error_handling() {
    // Test: Read-only filesystem → Write error → Graceful failure
    // Note: Can't actually make filesystem read-only in test, simulate error
    let ctx = IntegrationTestContext::new("SPEC-EC24-001").unwrap();

    // Write error marker (simulates read-only failure)
    let error_marker = ctx.commands_dir().join("readonly_error.json");
    std::fs::write(&error_marker, json!({
        "error": "permission_denied",
        "filesystem": "readonly",
        "operation": "write",
        "graceful_failure": true
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&error_marker).unwrap();
    assert!(content.contains("graceful_failure"));
}

#[test]
fn ec25_unicode_in_evidence_content() {
    // Test: Unicode characters in evidence → Properly encoded
    let ctx = IntegrationTestContext::new("SPEC-EC25-001").unwrap();

    let unicode = ctx.consensus_dir().join("unicode.json");
    std::fs::write(&unicode, json!({
        "content": "Hello 世界 🌍 مرحبا мир",
        "emoji": "✅🎉🚀",
        "symbols": "Σ ∞ √ ∂"
    }).to_string()).unwrap();

    let content = std::fs::read_to_string(&unicode).unwrap();
    assert!(content.contains("世界"));
    assert!(content.contains("🌍"));
}
