//! Spec-Kit multi-agent automation framework
//!
//! This module isolates all spec-kit functionality from upstream TUI code
//! to minimize rebase conflict surface area.
//!
//! Uses free functions instead of methods to avoid Rust borrow checker issues
//! when accessing ChatWidget fields.

pub mod handler;
pub mod state;

// Re-export state types and helpers
pub use state::{
    GuardrailEvaluation, GuardrailOutcome, GuardrailWait, SpecAutoPhase, SpecAutoState,
    expected_guardrail_command, guardrail_for_stage, require_object,
    require_string_field, spec_ops_stage_prefix, validate_guardrail_evidence,
};

// Re-export handler functions
pub use handler::{halt_spec_auto_with_error, handle_spec_consensus, handle_spec_status};
