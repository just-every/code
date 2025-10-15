//! Spec-Kit multi-agent automation framework
//!
//! This module isolates all spec-kit functionality from upstream TUI code
//! to minimize rebase conflict surface area.

mod handler;
mod state;

pub use handler::SpecKitHandler;
pub use state::{
    GuardrailEvaluation, GuardrailOutcome, GuardrailWait, SpecAutoPhase, SpecAutoState,
    expected_guardrail_command, get_nested, guardrail_for_stage, require_object,
    require_string_field, spec_ops_stage_prefix, validate_guardrail_evidence,
};
