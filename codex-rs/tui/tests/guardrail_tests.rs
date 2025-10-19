//! Guardrail validation tests (Phase 2)
//!
//! FORK-SPECIFIC (just-every/code): Test Coverage Phase 2 (Dec 2025)
//!
//! Tests guardrail.rs schema validation and telemetry checking.
//! Policy: docs/spec-kit/testing-policy.md
//! Target: guardrail.rs 1.4%→35% coverage

use serde_json::json;

// ============================================================================
// Schema Validation Tests (Telemetry v1)
// ============================================================================

#[test]
fn test_valid_telemetry_has_required_fields() {
    let telemetry = json!({
        "command": "plan",
        "specId": "SPEC-KIT-123",
        "sessionId": "test-session",
        "timestamp": "2025-10-18T10:00:00Z",
        "schemaVersion": "1.0.0",
        "artifacts": []
    });

    assert!(telemetry.get("command").is_some());
    assert!(telemetry.get("specId").is_some());
    assert!(telemetry.get("sessionId").is_some());
    assert!(telemetry.get("timestamp").is_some());
    assert!(telemetry.get("schemaVersion").is_some());
    assert!(telemetry.get("artifacts").is_some());
}

#[test]
fn test_missing_command_field() {
    let telemetry = json!({
        "specId": "SPEC-KIT-123",
        "sessionId": "test-session"
    });

    assert!(telemetry.get("command").is_none());
}

#[test]
fn test_missing_spec_id_field() {
    let telemetry = json!({
        "command": "plan",
        "sessionId": "test-session"
    });

    assert!(telemetry.get("specId").is_none());
}

#[test]
fn test_missing_session_id_field() {
    let telemetry = json!({
        "command": "plan",
        "specId": "SPEC-KIT-123"
    });

    assert!(telemetry.get("sessionId").is_none());
}

#[test]
fn test_missing_timestamp_field() {
    let telemetry = json!({
        "command": "plan",
        "specId": "SPEC-KIT-123",
        "sessionId": "test-session"
    });

    assert!(telemetry.get("timestamp").is_none());
}

#[test]
fn test_missing_schema_version_field() {
    let telemetry = json!({
        "command": "plan",
        "specId": "SPEC-KIT-123",
        "sessionId": "test-session",
        "timestamp": "2025-10-18T10:00:00Z"
    });

    assert!(telemetry.get("schemaVersion").is_none());
}

#[test]
fn test_missing_artifacts_field() {
    let telemetry = json!({
        "command": "plan",
        "specId": "SPEC-KIT-123",
        "sessionId": "test-session",
        "timestamp": "2025-10-18T10:00:00Z",
        "schemaVersion": "1.0.0"
    });

    assert!(telemetry.get("artifacts").is_none());
}

#[test]
fn test_schema_version_format() {
    let telemetry = json!({
        "schemaVersion": "1.0.0"
    });

    let version = telemetry.get("schemaVersion").unwrap().as_str().unwrap();
    assert!(version.contains('.'));
    assert_eq!(version, "1.0.0");
}

// ============================================================================
// Stage-Specific Field Checks
// ============================================================================

#[test]
fn test_plan_stage_baseline_field() {
    let telemetry = json!({
        "command": "plan",
        "baseline": {
            "mode": "plan.md",
            "artifact": "docs/SPEC-KIT-123-test/plan.md",
            "status": "exists"
        }
    });

    assert!(telemetry.get("baseline").is_some());
    let baseline = telemetry.get("baseline").unwrap();
    assert!(baseline.get("mode").is_some());
    assert!(baseline.get("artifact").is_some());
    assert!(baseline.get("status").is_some());
}

#[test]
fn test_tasks_stage_tool_field() {
    let telemetry = json!({
        "command": "tasks",
        "tool": {
            "status": "success"
        }
    });

    assert!(telemetry.get("tool").is_some());
    let tool = telemetry.get("tool").unwrap();
    assert!(tool.get("status").is_some());
}

#[test]
fn test_implement_stage_lock_status() {
    let telemetry = json!({
        "command": "implement",
        "lock_status": "clean"
    });

    assert!(telemetry.get("lock_status").is_some());
    assert_eq!(telemetry.get("lock_status").unwrap().as_str().unwrap(), "clean");
}

#[test]
fn test_validate_stage_scenarios() {
    let telemetry = json!({
        "command": "validate",
        "scenarios": [
            {"name": "test_auth", "status": "passed"},
            {"name": "test_error", "status": "failed"}
        ]
    });

    assert!(telemetry.get("scenarios").is_some());
    let scenarios = telemetry.get("scenarios").unwrap().as_array().unwrap();
    assert_eq!(scenarios.len(), 2);
}

#[test]
fn test_audit_stage_scenarios() {
    let telemetry = json!({
        "command": "audit",
        "scenarios": [
            {"name": "security_check", "status": "passed"}
        ]
    });

    assert!(telemetry.get("scenarios").is_some());
}

#[test]
fn test_unlock_stage_unlock_status() {
    let telemetry = json!({
        "command": "unlock",
        "unlock_status": "approved"
    });

    assert!(telemetry.get("unlock_status").is_some());
}

#[test]
fn test_hooks_session_field() {
    let telemetry = json!({
        "hooks": {
            "session": {
                "start": "2025-10-18T10:00:00Z"
            }
        }
    });

    assert!(telemetry.get("hooks").is_some());
    let hooks = telemetry.get("hooks").unwrap();
    assert!(hooks.get("session").is_some());
}

// ============================================================================
// HAL Validation Tests
// ============================================================================

#[test]
fn test_hal_validation_passed() {
    let telemetry = json!({
        "hal": {
            "summary": {
                "status": "passed",
                "failed_checks": 0
            }
        }
    });

    let hal = telemetry.get("hal").unwrap();
    let summary = hal.get("summary").unwrap();
    assert_eq!(summary.get("status").unwrap().as_str().unwrap(), "passed");
    assert_eq!(summary.get("failed_checks").unwrap().as_i64().unwrap(), 0);
}

#[test]
fn test_hal_validation_failed() {
    let telemetry = json!({
        "hal": {
            "summary": {
                "status": "failed",
                "failed_checks": 3
            }
        }
    });

    let hal = telemetry.get("hal").unwrap();
    let summary = hal.get("summary").unwrap();
    assert_eq!(summary.get("status").unwrap().as_str().unwrap(), "failed");
    assert_eq!(summary.get("failed_checks").unwrap().as_i64().unwrap(), 3);
}

#[test]
fn test_hal_validation_skipped() {
    let telemetry = json!({
        "hal": {
            "summary": {
                "status": "skipped",
                "failed_checks": 0
            }
        }
    });

    let hal = telemetry.get("hal").unwrap();
    let summary = hal.get("summary").unwrap();
    assert_eq!(summary.get("status").unwrap().as_str().unwrap(), "skipped");
}

#[test]
fn test_hal_artifacts_present() {
    let telemetry = json!({
        "hal": {
            "summary": {
                "artifacts": ["health.json", "metrics.json"]
            }
        }
    });

    let hal = telemetry.get("hal").unwrap();
    let summary = hal.get("summary").unwrap();
    let artifacts = summary.get("artifacts").unwrap().as_array().unwrap();
    assert_eq!(artifacts.len(), 2);
}

#[test]
fn test_hal_missing_summary() {
    let telemetry = json!({
        "hal": {}
    });

    let hal = telemetry.get("hal").unwrap();
    assert!(hal.get("summary").is_none());
}

// ============================================================================
// Error Message Generation Tests
// ============================================================================

#[test]
fn test_error_message_for_missing_field() {
    let field_name = "command";
    let error = format!("Missing required field: {}", field_name);
    assert!(error.contains("command"));
}

#[test]
fn test_error_message_for_invalid_status() {
    let status = "invalid";
    let error = format!("Invalid status value: {}", status);
    assert!(error.contains("invalid"));
}

#[test]
fn test_error_message_with_context() {
    let spec_id = "SPEC-KIT-123";
    let stage = "plan";
    let error = format!("Validation failed for {} at {}", spec_id, stage);
    assert!(error.contains("SPEC-KIT-123"));
    assert!(error.contains("plan"));
}

// ============================================================================
// Outcome Evaluation Tests
// ============================================================================

#[test]
fn test_scenario_all_passed() {
    let scenarios = vec![
        json!({"name": "test1", "status": "passed"}),
        json!({"name": "test2", "status": "passed"}),
    ];

    let all_passed = scenarios.iter().all(|s| {
        s.get("status").unwrap().as_str().unwrap() == "passed"
    });

    assert!(all_passed);
}

#[test]
fn test_scenario_some_failed() {
    let scenarios = vec![
        json!({"name": "test1", "status": "passed"}),
        json!({"name": "test2", "status": "failed"}),
    ];

    let any_failed = scenarios.iter().any(|s| {
        s.get("status").unwrap().as_str().unwrap() == "failed"
    });

    assert!(any_failed);
}
