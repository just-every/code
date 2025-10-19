//! Tests for spec-kit error types
//!
//! Covers SpecKitError variants, helper methods, and error conversions.

use codex_tui::{SpecKitError, SpecKitResult, SpecStage};
use std::path::PathBuf;

// Type alias for Result used in tests
type Result<T> = SpecKitResult<T>;

// ===== Error Variant Construction Tests =====

#[test]
fn test_directory_read_error() {
    let err = SpecKitError::DirectoryRead {
        path: PathBuf::from("/test/dir"),
        source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
    };

    assert!(err.to_string().contains("/test/dir"));
    assert!(err.to_string().contains("Failed to read directory"));
}

#[test]
fn test_directory_create_error() {
    let err = SpecKitError::DirectoryCreate {
        path: PathBuf::from("/test/newdir"),
        source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied"),
    };

    assert!(err.to_string().contains("/test/newdir"));
    assert!(err.to_string().contains("Failed to create directory"));
}

#[test]
fn test_file_create_error() {
    let err = SpecKitError::FileCreate {
        path: PathBuf::from("/test/file.txt"),
        source: std::io::Error::new(std::io::ErrorKind::AlreadyExists, "already exists"),
    };

    assert!(err.to_string().contains("/test/file.txt"));
    assert!(err.to_string().contains("Failed to create file"));
}

#[test]
fn test_file_write_helper() {
    let err = SpecKitError::file_write(
        "/test/output.json",
        std::io::Error::new(std::io::ErrorKind::WriteZero, "write failed"),
    );

    assert!(err.to_string().contains("/test/output.json"));
    assert!(err.to_string().contains("Failed to write file"));
}

#[test]
fn test_json_serialize_error() {
    // Create a mock serde error (can't construct directly, use from_str)
    let json_err = serde_json::from_str::<serde_json::Value>("invalid json");
    assert!(json_err.is_err());

    let err = SpecKitError::JsonSerialize {
        source: json_err.unwrap_err(),
    };

    assert!(err.to_string().contains("Failed to serialize to JSON"));
}

#[test]
fn test_json_parse_helper() {
    let json_err = serde_json::from_str::<serde_json::Value>("{bad}").unwrap_err();
    let err = SpecKitError::json_parse("/test/bad.json", json_err);

    assert!(err.to_string().contains("/test/bad.json"));
    assert!(err.to_string().contains("Failed to parse JSON"));
}

#[test]
fn test_no_consensus_found_error() {
    let err = SpecKitError::NoConsensusFound {
        spec_id: "SPEC-KIT-100".to_string(),
        stage: "plan".to_string(),
        directory: PathBuf::from("/evidence/consensus"),
    };

    assert!(err.to_string().contains("SPEC-KIT-100"));
    assert!(err.to_string().contains("plan"));
    assert!(err.to_string().contains("No consensus artifacts found"));
}

#[test]
fn test_missing_artifact_error() {
    let err = SpecKitError::MissingArtifact {
        spec_id: "SPEC-KIT-101".to_string(),
        stage: "tasks".to_string(),
        artifact: "plan.md".to_string(),
    };

    assert!(err.to_string().contains("SPEC-KIT-101"));
    assert!(err.to_string().contains("tasks"));
    assert!(err.to_string().contains("plan.md"));
    assert!(err.to_string().contains("Missing required artifact"));
}

#[test]
fn test_missing_field_error() {
    let err = SpecKitError::MissingField {
        field: "baseline.status".to_string(),
    };

    assert!(err.to_string().contains("baseline.status"));
    assert!(err.to_string().contains("Missing required field"));
}

#[test]
fn test_invalid_field_value_error() {
    let err = SpecKitError::InvalidFieldValue {
        field: "command".to_string(),
        value: "wrong-command".to_string(),
        expected: "spec-ops-plan".to_string(),
    };

    assert!(err.to_string().contains("command"));
    assert!(err.to_string().contains("wrong-command"));
    assert!(err.to_string().contains("spec-ops-plan"));
}

#[test]
fn test_evidence_validation_error() {
    let err = SpecKitError::EvidenceValidation {
        spec_id: "SPEC-KIT-102".to_string(),
        stage: "implement".to_string(),
        failures: vec!["Missing file: code.rs".to_string()],
    };

    assert!(err.to_string().contains("SPEC-KIT-102"));
    assert!(err.to_string().contains("implement"));
    assert!(err.to_string().contains("Evidence validation failed"));
}

#[test]
fn test_consensus_conflict_error() {
    let err = SpecKitError::ConsensusConflict {
        reason: "Agent disagreement on architecture".to_string(),
    };

    assert!(err.to_string().contains("Consensus conflict"));
    assert!(err.to_string().contains("architecture"));
}

#[test]
fn test_consensus_parse_error() {
    let err = SpecKitError::ConsensusParse {
        reason: "Invalid JSON structure".to_string(),
    };

    assert!(err.to_string().contains("Failed to parse consensus synthesis"));
    assert!(err.to_string().contains("Invalid JSON"));
}

#[test]
fn test_local_memory_search_error() {
    let err = SpecKitError::LocalMemorySearch {
        query: "find consensus artifacts".to_string(),
    };

    assert!(err.to_string().contains("Local memory search failed"));
    assert!(err.to_string().contains("find consensus"));
}

#[test]
fn test_local_memory_store_error() {
    let err = SpecKitError::LocalMemoryStore {
        content: "consensus result".to_string(),
    };

    assert!(err.to_string().contains("Local memory store failed"));
    assert!(err.to_string().contains("consensus result"));
}

#[test]
fn test_pipeline_halted_error() {
    let err = SpecKitError::PipelineHalted {
        stage: "validate".to_string(),
        reason: "Tests failed".to_string(),
    };

    assert!(err.to_string().contains("Spec auto pipeline halted"));
    assert!(err.to_string().contains("validate"));
    assert!(err.to_string().contains("Tests failed"));
}

#[test]
fn test_invalid_stage_transition_error() {
    let err = SpecKitError::InvalidStageTransition {
        from: "plan".to_string(),
        to: "unlock".to_string(),
    };

    assert!(err.to_string().contains("Invalid stage transition"));
    assert!(err.to_string().contains("plan"));
    assert!(err.to_string().contains("unlock"));
}

#[test]
fn test_invalid_spec_id_error() {
    let err = SpecKitError::InvalidSpecId {
        spec_id: "INVALID-FORMAT".to_string(),
    };

    assert!(err.to_string().contains("Invalid SPEC ID format"));
    assert!(err.to_string().contains("INVALID-FORMAT"));
}

#[test]
fn test_unknown_stage_error() {
    let err = SpecKitError::UnknownStage {
        stage: "unknown-stage".to_string(),
    };

    assert!(err.to_string().contains("Unknown stage"));
    assert!(err.to_string().contains("unknown-stage"));
}

// ===== Helper Method Tests =====

#[test]
fn test_missing_agents_helper() {
    let err = SpecKitError::missing_agents(
        vec!["gemini".to_string(), "claude".to_string(), "code".to_string()],
        vec!["gemini".to_string()],
    );

    let msg = err.to_string();
    assert!(msg.contains("Missing agent artifacts"));
    assert!(msg.contains("gemini"));
    assert!(msg.contains("claude"));
    assert!(msg.contains("code"));
}

#[test]
fn test_schema_validation_helper() {
    let failures = vec![
        "Missing field: specId".to_string(),
        "Missing field: command".to_string(),
    ];

    let err = SpecKitError::schema_validation("SPEC-KIT-103", SpecStage::Audit, failures);

    let msg = err.to_string();
    assert!(msg.contains("SPEC-KIT-103"));
    assert!(msg.contains("audit"));
    assert!(msg.contains("Missing field: specId"));
    assert!(msg.contains("Missing field: command"));
}

#[test]
fn test_from_string_helper() {
    let err = SpecKitError::from_string("custom error description");

    assert_eq!(err.to_string(), "custom error description");
}

// ===== Conversion Trait Tests =====

#[test]
fn test_from_string_trait() {
    let err: SpecKitError = String::from("error from String").into();
    assert_eq!(err.to_string(), "error from String");
}

#[test]
fn test_from_str_trait() {
    let err: SpecKitError = "error from &str".into();
    assert_eq!(err.to_string(), "error from &str");
}

// ===== Result Type Test =====

#[test]
fn test_result_type_ok() {
    let result: Result<i32> = Ok(42);
    assert_eq!(result.unwrap(), 42);
}

#[test]
fn test_result_type_err() {
    let result: Result<i32> = Err(SpecKitError::from_string("test error"));
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "test error");
}
