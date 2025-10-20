//! Special command implementations (auto, new, specify, consensus)
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit multi-agent automation framework

use super::super::command_registry::SpecKitCommand;
use super::super::super::ChatWidget;
use super::super::handler;

/// Command: /speckit.auto (and /spec-auto)
/// Full 6-stage pipeline with auto-advancement
pub struct SpecKitAutoCommand;

impl SpecKitCommand for SpecKitAutoCommand {
    fn name(&self) -> &'static str {
        "speckit.auto"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-auto"]
    }

    fn description(&self) -> &'static str {
        "full 6-stage pipeline with auto-advancement"
    }

    fn execute(&self, widget: &mut ChatWidget, args: String) {
        // Parse spec-auto args and delegate to handler
        match crate::slash_command::parse_spec_auto_args(&args) {
            Ok(invocation) => {
                widget.handle_spec_auto_command(invocation);
            }
            Err(err) => {
                let error_msg = match err {
                    crate::slash_command::SpecAutoParseError::MissingSpecId => {
                        "Missing SPEC ID. Usage: /speckit.auto SPEC-KIT-### [--from stage]".to_string()
                    }
                    crate::slash_command::SpecAutoParseError::MissingFromStage => {
                        "`--from` flag requires a stage name".to_string()
                    }
                    crate::slash_command::SpecAutoParseError::UnknownStage(stage) => {
                        format!(
                            "Unknown stage '{}'. Valid stages: plan, tasks, implement, validate, audit, unlock",
                            stage
                        )
                    }
                    crate::slash_command::SpecAutoParseError::UnknownHalMode(mode) => {
                        format!(
                            "Unknown HAL mode '{}'. Expected 'mock' or 'live'",
                            mode
                        )
                    }
                };
                widget.history_push(crate::history_cell::new_error_event(error_msg));
                widget.request_redraw();
            }
        }
    }

    fn requires_args(&self) -> bool {
        true
    }
}

/// Command: /speckit.new (and /new-spec)
/// Create new SPEC from description with templates
pub struct SpecKitNewCommand;

impl SpecKitCommand for SpecKitNewCommand {
    fn name(&self) -> &'static str {
        "speckit.new"
    }

    fn aliases(&self) -> &[&'static str] {
        &["new-spec"]
    }

    fn description(&self) -> &'static str {
        "create new SPEC from description with templates (55% faster)"
    }

    fn execute(&self, widget: &mut ChatWidget, args: String) {
        // Routed to subagent orchestrators
        // Use format_subagent_command and submit
        let formatted = codex_core::slash_commands::format_subagent_command(
            "speckit.new",
            &args,
            Some(&widget.config.agents),
            Some(&widget.config.subagent_commands),
        );
        widget.submit_prompt_with_display(args, formatted.prompt);
    }

    fn requires_args(&self) -> bool {
        true
    }
}

/// Command: /speckit.specify
/// Generate PRD with multi-agent consensus
pub struct SpecKitSpecifyCommand;

impl SpecKitCommand for SpecKitSpecifyCommand {
    fn name(&self) -> &'static str {
        "speckit.specify"
    }

    fn aliases(&self) -> &[&'static str] {
        &[]
    }

    fn description(&self) -> &'static str {
        "generate PRD with multi-agent consensus"
    }

    fn execute(&self, widget: &mut ChatWidget, args: String) {
        // Routed to subagent orchestrators
        let formatted = codex_core::slash_commands::format_subagent_command(
            "speckit.specify",
            &args,
            Some(&widget.config.agents),
            Some(&widget.config.subagent_commands),
        );
        widget.submit_prompt_with_display(args, formatted.prompt);
    }

    fn requires_args(&self) -> bool {
        true
    }
}

/// Command: /spec-consensus
/// Check multi-agent consensus via local-memory
pub struct SpecConsensusCommand;

impl SpecKitCommand for SpecConsensusCommand {
    fn name(&self) -> &'static str {
        "spec-consensus"
    }

    fn aliases(&self) -> &[&'static str] {
        &[]
    }

    fn description(&self) -> &'static str {
        "check multi-agent consensus via local-memory (requires SPEC ID & stage)"
    }

    fn execute(&self, widget: &mut ChatWidget, args: String) {
        handler::handle_spec_consensus(widget, args);
    }

    fn requires_args(&self) -> bool {
        true
    }
}
