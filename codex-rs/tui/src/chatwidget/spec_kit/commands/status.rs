//! /speckit.status command implementation

use super::super::command_registry::SpecKitCommand;
use super::super::super::ChatWidget;
use super::super::handler;

/// Command: /speckit.status
/// Shows comprehensive SPEC status dashboard (native Rust, <1s, $0)
pub struct SpecKitStatusCommand;

impl SpecKitCommand for SpecKitStatusCommand {
    fn name(&self) -> &'static str {
        "speckit.status"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-status"]
    }

    fn description(&self) -> &'static str {
        "show SPEC progress dashboard"
    }

    fn execute(&self, widget: &mut ChatWidget, args: String) {
        handler::handle_spec_status(widget, args);
    }

    fn requires_args(&self) -> bool {
        false
    }

    fn is_prompt_expanding(&self) -> bool {
        false
    }
}
