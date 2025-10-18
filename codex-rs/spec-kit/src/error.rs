//! Error types for spec-kit operations
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit multi-agent automation framework
//!
//! Migrated from tui/src/chatwidget/spec_kit/error.rs (MAINT-10)

use std::path::PathBuf;
use thiserror::Error;

/// Spec-kit result type alias
pub type Result<T> = std::result::Result<T, SpecKitError>;

/// Spec-kit error taxonomy
#[derive(Debug, Error)]
pub enum SpecKitError {
    #[error("Failed to write file {path}: {source}")]
    FileWrite {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to read file {path}: {source}")]
    FileRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to read directory {path}: {source}")]
    DirectoryRead {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to create directory {path}: {source}")]
    DirectoryCreate {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Failed to serialize JSON: {source}")]
    JsonSerialize { source: serde_json::Error },

    #[error("Failed to deserialize JSON: {source}")]
    JsonDeserialize { source: serde_json::Error },

    #[error("No consensus found for {spec_id} stage {stage} in {directory:?}")]
    NoConsensusFound {
        spec_id: String,
        stage: String,
        directory: PathBuf,
    },

    #[error("Agent execution failed: expected {expected:?}, completed {completed:?}")]
    AgentExecutionFailed {
        expected: Vec<String>,
        completed: Vec<String>,
    },

    #[error("MCP call failed: {0}")]
    McpCallFailed(String),

    #[error("Invalid SPEC ID format: {0}")]
    InvalidSpecId(String),

    #[error("Stage {stage} not valid for operation")]
    InvalidStage { stage: String },

    #[error("Configuration validation failed: {0}")]
    ConfigValidation(String),

    #[error("Evidence repository error: {0}")]
    EvidenceRepository(String),

    #[error("{0}")]
    Other(String),
}

impl From<String> for SpecKitError {
    fn from(s: String) -> Self {
        SpecKitError::Other(s)
    }
}

impl From<&str> for SpecKitError {
    fn from(s: &str) -> Self {
        SpecKitError::Other(s.to_string())
    }
}
