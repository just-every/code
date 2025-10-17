//! Error types for spec-kit operations
//!
//! This module provides structured error handling to replace String-based errors
//! throughout the spec-kit framework.

use crate::spec_prompts::SpecStage;
use std::path::PathBuf;
use thiserror::Error;

/// Structured error type for all spec-kit operations
#[derive(Debug, Error)]
pub enum SpecKitError {
    // === File I/O Errors ===
    #[error("Failed to read directory {path}: {source}")]
    DirectoryRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to read file {path}: {source}")]
    FileRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to create directory {path}: {source}")]
    DirectoryCreate {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to create file {path}: {source}")]
    FileCreate {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to write file {path}: {source}")]
    FileWrite {
        path: PathBuf,
        source: std::io::Error,
    },

    // === JSON Errors ===
    #[error("Failed to parse JSON from {path}: {source}")]
    JsonParse {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("Failed to serialize to JSON: {source}")]
    JsonSerialize { source: serde_json::Error },

    // === Missing Artifacts ===
    #[error("No telemetry files found for {spec_id} stage {stage} matching pattern {pattern} in {directory}")]
    NoTelemetryFound {
        spec_id: String,
        stage: String,
        pattern: String,
        directory: PathBuf,
    },

    #[error("No consensus artifacts found for {spec_id} stage {stage} in {directory}")]
    NoConsensusFound {
        spec_id: String,
        stage: String,
        directory: PathBuf,
    },

    #[error("Missing required artifact: {artifact} for {spec_id} stage {stage}")]
    MissingArtifact {
        spec_id: String,
        stage: String,
        artifact: String,
    },

    // === Validation Errors ===
    #[error("Telemetry schema validation failed for {spec_id} stage {stage}: {failures:?}")]
    SchemaValidation {
        spec_id: String,
        stage: String,
        failures: Vec<String>,
    },

    #[error("Missing required field in telemetry: {field}")]
    MissingField { field: String },

    #[error("Invalid field value in telemetry: {field} = {value} (expected {expected})")]
    InvalidFieldValue {
        field: String,
        value: String,
        expected: String,
    },

    #[error("Evidence validation failed for {spec_id} stage {stage}: {failures:?}")]
    EvidenceValidation {
        spec_id: String,
        stage: String,
        failures: Vec<String>,
    },

    // === Consensus Errors ===
    #[error("Missing agent artifacts: expected {expected:?}, found {found:?}")]
    MissingAgents {
        expected: Vec<String>,
        found: Vec<String>,
    },

    #[error("Consensus conflict detected: {reason}")]
    ConsensusConflict { reason: String },

    #[error("Failed to parse consensus synthesis: {reason}")]
    ConsensusParse { reason: String },

    // === Local Memory Errors ===
    #[error("Local memory search failed: {query}")]
    LocalMemorySearch { query: String },

    #[error("Local memory store failed: {content}")]
    LocalMemoryStore { content: String },

    // === Spec Auto Pipeline Errors ===
    #[error("Spec auto pipeline halted at stage {stage}: {reason}")]
    PipelineHalted { stage: String, reason: String },

    #[error("Invalid stage transition: {from} → {to}")]
    InvalidStageTransition { from: String, to: String },

    // === Configuration Errors ===
    #[error("Invalid SPEC ID format: {spec_id}")]
    InvalidSpecId { spec_id: String },

    #[error("Unknown stage: {stage}")]
    UnknownStage { stage: String },

    // === Generic Wrapper ===
    #[error("{0}")]
    Other(String),
}

impl SpecKitError {
    /// Create a file read error
    pub fn file_read(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::FileRead {
            path: path.into(),
            source,
        }
    }

    /// Create a file write error
    pub fn file_write(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::FileWrite {
            path: path.into(),
            source,
        }
    }

    /// Create a JSON parse error
    pub fn json_parse(path: impl Into<PathBuf>, source: serde_json::Error) -> Self {
        Self::JsonParse {
            path: path.into(),
            source,
        }
    }

    /// Create a no telemetry found error
    pub fn no_telemetry(
        spec_id: impl Into<String>,
        stage: SpecStage,
        pattern: impl Into<String>,
        directory: impl Into<PathBuf>,
    ) -> Self {
        Self::NoTelemetryFound {
            spec_id: spec_id.into(),
            stage: stage.command_name().to_string(),
            pattern: pattern.into(),
            directory: directory.into(),
        }
    }

    /// Create a schema validation error
    pub fn schema_validation(
        spec_id: impl Into<String>,
        stage: SpecStage,
        failures: Vec<String>,
    ) -> Self {
        Self::SchemaValidation {
            spec_id: spec_id.into(),
            stage: stage.command_name().to_string(),
            failures,
        }
    }

    /// Create a missing agents error
    pub fn missing_agents(expected: Vec<String>, found: Vec<String>) -> Self {
        Self::MissingAgents { expected, found }
    }

    /// Convert from a generic string error
    pub fn from_string(s: impl Into<String>) -> Self {
        Self::Other(s.into())
    }
}

/// Type alias for spec-kit results
pub type Result<T> = std::result::Result<T, SpecKitError>;

// Conversion traits for easier migration
impl From<String> for SpecKitError {
    fn from(s: String) -> Self {
        Self::Other(s)
    }
}

impl From<&str> for SpecKitError {
    fn from(s: &str) -> Self {
        Self::Other(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_read_error() {
        let err = SpecKitError::file_read(
            PathBuf::from("/test/path.json"),
            std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        );
        assert!(err.to_string().contains("/test/path.json"));
        assert!(err.to_string().contains("Failed to read file"));
    }

    #[test]
    fn test_no_telemetry_error() {
        let err = SpecKitError::no_telemetry(
            "SPEC-KIT-065",
            SpecStage::Plan,
            "plan_*.json",
            PathBuf::from("/evidence"),
        );
        assert!(err.to_string().contains("SPEC-KIT-065"));
        assert!(err.to_string().contains("plan"));
        assert!(err.to_string().contains("plan_*.json"));
    }

    #[test]
    fn test_missing_agents_error() {
        let err = SpecKitError::missing_agents(
            vec!["gemini".to_string(), "claude".to_string()],
            vec!["gemini".to_string()],
        );
        assert!(err.to_string().contains("gemini"));
        assert!(err.to_string().contains("claude"));
    }

    #[test]
    fn test_schema_validation_error() {
        let err = SpecKitError::schema_validation(
            "SPEC-KIT-065",
            SpecStage::Plan,
            vec!["Missing field: baseline.status".to_string()],
        );
        assert!(err.to_string().contains("SPEC-KIT-065"));
        assert!(err.to_string().contains("plan"));
        assert!(err.to_string().contains("Missing field"));
    }

    #[test]
    fn test_from_string() {
        let err = SpecKitError::from_string("custom error message");
        assert_eq!(err.to_string(), "custom error message");
    }
}
