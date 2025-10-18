//! Quality command implementations (clarify, analyze, checklist)
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit multi-agent automation framework

use super::super::command_registry::SpecKitCommand;
use super::super::super::ChatWidget;

/// Command: /speckit.clarify
/// Structured ambiguity resolution
pub struct SpecKitClarifyCommand;

impl SpecKitCommand for SpecKitClarifyCommand {
    fn name(&self) -> &'static str {
        "speckit.clarify"
    }

    fn aliases(&self) -> &[&'static str] {
        &[]
    }

    fn description(&self) -> &'static str {
        "resolve spec ambiguities (max 5 questions)"
    }

    fn execute(&self, _widget: &mut ChatWidget, _args: String) {
        // Prompt-expanding command
    }

    fn expand_prompt(&self, args: &str) -> Option<String> {
        Some(
            codex_core::slash_commands::format_subagent_command("clarify", args, None, None)
                .prompt,
        )
    }

    fn requires_args(&self) -> bool {
        true
    }
}

/// Command: /speckit.analyze
/// Cross-artifact consistency checking
pub struct SpecKitAnalyzeCommand;

impl SpecKitCommand for SpecKitAnalyzeCommand {
    fn name(&self) -> &'static str {
        "speckit.analyze"
    }

    fn aliases(&self) -> &[&'static str] {
        &[]
    }

    fn description(&self) -> &'static str {
        "check cross-artifact consistency"
    }

    fn execute(&self, _widget: &mut ChatWidget, _args: String) {
        // Prompt-expanding command
    }

    fn expand_prompt(&self, args: &str) -> Option<String> {
        Some(
            codex_core::slash_commands::format_subagent_command("analyze", args, None, None)
                .prompt,
        )
    }

    fn requires_args(&self) -> bool {
        true
    }
}

/// Command: /speckit.checklist
/// Requirement quality scoring
pub struct SpecKitChecklistCommand;

impl SpecKitCommand for SpecKitChecklistCommand {
    fn name(&self) -> &'static str {
        "speckit.checklist"
    }

    fn aliases(&self) -> &[&'static str] {
        &[]
    }

    fn description(&self) -> &'static str {
        "evaluate requirement quality (generates scores)"
    }

    fn execute(&self, _widget: &mut ChatWidget, _args: String) {
        // Prompt-expanding command
    }

    fn expand_prompt(&self, args: &str) -> Option<String> {
        Some(
            codex_core::slash_commands::format_subagent_command("checklist", args, None, None)
                .prompt,
        )
    }

    fn requires_args(&self) -> bool {
        true
    }
}
