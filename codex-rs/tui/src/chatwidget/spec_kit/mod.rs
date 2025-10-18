//! Spec-Kit multi-agent automation framework
//!
//! This module isolates all spec-kit functionality from upstream TUI code
//! to minimize rebase conflict surface area.
//!
//! Uses free functions instead of methods to avoid Rust borrow checker issues
//! when accessing ChatWidget fields.

pub mod agent_lifecycle;
pub mod command_registry;
pub mod commands;
pub mod config_validator;
pub mod consensus;
pub mod context;
pub mod error;
pub mod evidence;
pub mod file_modifier;
pub mod guardrail;
pub mod handler;
pub mod local_memory_client;
pub mod mcp_registry;
pub mod metrics;
pub mod quality;
pub mod routing;
pub mod schemas;
pub mod state;

// Re-export context types
pub use context::SpecKitContext;

// Re-export error types
pub use error::{Result, SpecKitError};

// Re-export evidence types

// Re-export key consensus functions (pub(crate) since types are private)
pub(crate) use consensus::collect_consensus_artifacts;

// Re-export guardrail functions
pub use guardrail::{evaluate_guardrail_value, validate_guardrail_schema};

// Re-export routing functions
pub use routing::try_dispatch_spec_kit_command;

// Re-export state types and helpers
pub use state::{
    GuardrailOutcome, SpecAutoState, spec_ops_stage_prefix, validate_guardrail_evidence,
    // Quality gate types (T85)
    Confidence, EscalatedQuestion, Magnitude, QualityCheckpoint, QualityGateType, QualityIssue, Resolvability, Resolution,
};

// Re-export handler functions
pub use handler::{
    advance_spec_auto, auto_submit_spec_stage_prompt, halt_spec_auto_with_error, handle_guardrail,
    handle_spec_auto, handle_spec_consensus, handle_spec_status, on_spec_auto_agents_complete,
    on_spec_auto_task_complete, on_spec_auto_task_started, on_quality_gate_answers,
    on_quality_gate_cancelled,
};

// Re-export quality gate functions
pub use quality::{
    classify_issue_agreement,
    merge_agent_issues, parse_quality_issue_from_agent, resolve_quality_issue,
    should_auto_resolve,
};

// Re-export file modification functions
