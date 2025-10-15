//! Spec-Kit multi-agent automation framework
//!
//! This module isolates all spec-kit functionality from upstream TUI code
//! to minimize rebase conflict surface area.
//!
//! Uses free functions instead of methods to avoid Rust borrow checker issues
//! when accessing ChatWidget fields.

pub mod consensus;
pub mod guardrail;
pub mod handler;
pub mod state;

// Re-export key consensus functions
pub use consensus::{collect_consensus_artifacts, load_latest_consensus_synthesis, run_spec_consensus};

// Re-export guardrail functions
pub use guardrail::{evaluate_guardrail_value, validate_guardrail_schema};

// Re-export state types and helpers
pub use state::{
    GuardrailEvaluation, GuardrailOutcome, GuardrailWait, SpecAutoPhase, SpecAutoState,
    expected_guardrail_command, guardrail_for_stage, require_object, require_string_field,
    spec_ops_stage_prefix, validate_guardrail_evidence,
};

// Re-export handler functions
pub use handler::{
    advance_spec_auto, auto_submit_spec_stage_prompt, halt_spec_auto_with_error, handle_guardrail,
    handle_spec_auto, handle_spec_consensus, handle_spec_status, on_spec_auto_agents_complete,
    on_spec_auto_task_complete, on_spec_auto_task_started,
};
