//! /speckit.plan command implementation
//!
//! FORK-SPECIFIC (just-every/code): Spec-kit multi-agent automation framework

use super::super::command_registry::SpecKitCommand;
use super::super::super::ChatWidget;

/// Command: /speckit.plan
/// Creates work breakdown with multi-agent consensus
pub struct SpecKitPlanCommand;

impl SpecKitCommand for SpecKitPlanCommand {
    fn name(&self) -> &'static str {
        "speckit.plan"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-plan", "spec-ops-plan"]
    }

    fn description(&self) -> &'static str {
        "create work breakdown with multi-agent consensus"
    }

    fn execute(&self, _widget: &mut ChatWidget, _args: String) {
        // Prompt-expanding commands don't execute directly
        // They expand via expand_prompt() and submit to agent
    }

    fn expand_prompt(&self, args: &str) -> Option<String> {
        Some(
            codex_core::slash_commands::format_subagent_command("plan", args, None, None).prompt,
        )
    }

    fn requires_args(&self) -> bool {
        true
    }
}

/// Command: /speckit.tasks
/// Generates task list with validation mapping
pub struct SpecKitTasksCommand;

impl SpecKitCommand for SpecKitTasksCommand {
    fn name(&self) -> &'static str {
        "speckit.tasks"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-tasks", "spec-ops-tasks"]
    }

    fn description(&self) -> &'static str {
        "generate task list with validation mapping"
    }

    fn execute(&self, _widget: &mut ChatWidget, _args: String) {
        // Prompt-expanding command
    }

    fn expand_prompt(&self, args: &str) -> Option<String> {
        Some(
            codex_core::slash_commands::format_subagent_command("tasks", args, None, None).prompt,
        )
    }

    fn requires_args(&self) -> bool {
        true
    }
}

/// Command: /speckit.implement
/// Code generation with multi-agent consensus
pub struct SpecKitImplementCommand;

impl SpecKitCommand for SpecKitImplementCommand {
    fn name(&self) -> &'static str {
        "speckit.implement"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-implement", "spec-ops-implement"]
    }

    fn description(&self) -> &'static str {
        "write code with multi-agent consensus"
    }

    fn execute(&self, _widget: &mut ChatWidget, _args: String) {
        // Prompt-expanding command
    }

    fn expand_prompt(&self, args: &str) -> Option<String> {
        Some(
            codex_core::slash_commands::format_subagent_command("implement", args, None, None)
                .prompt,
        )
    }

    fn requires_args(&self) -> bool {
        true
    }
}

/// Command: /speckit.validate
/// Test strategy with multi-agent validation
pub struct SpecKitValidateCommand;

impl SpecKitCommand for SpecKitValidateCommand {
    fn name(&self) -> &'static str {
        "speckit.validate"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-validate", "spec-ops-validate"]
    }

    fn description(&self) -> &'static str {
        "run test strategy with validation"
    }

    fn execute(&self, _widget: &mut ChatWidget, _args: String) {
        // Prompt-expanding command
    }

    fn expand_prompt(&self, args: &str) -> Option<String> {
        Some(
            codex_core::slash_commands::format_subagent_command("validate", args, None, None)
                .prompt,
        )
    }

    fn requires_args(&self) -> bool {
        true
    }
}

/// Command: /speckit.audit
/// Compliance review with multi-agent
pub struct SpecKitAuditCommand;

impl SpecKitCommand for SpecKitAuditCommand {
    fn name(&self) -> &'static str {
        "speckit.audit"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-audit", "spec-ops-audit"]
    }

    fn description(&self) -> &'static str {
        "compliance review with multi-agent"
    }

    fn execute(&self, _widget: &mut ChatWidget, _args: String) {
        // Prompt-expanding command
    }

    fn expand_prompt(&self, args: &str) -> Option<String> {
        Some(
            codex_core::slash_commands::format_subagent_command("audit", args, None, None).prompt,
        )
    }

    fn requires_args(&self) -> bool {
        true
    }
}

/// Command: /speckit.unlock
/// Final approval for merge
pub struct SpecKitUnlockCommand;

impl SpecKitCommand for SpecKitUnlockCommand {
    fn name(&self) -> &'static str {
        "speckit.unlock"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-unlock", "spec-ops-unlock"]
    }

    fn description(&self) -> &'static str {
        "final approval for merge"
    }

    fn execute(&self, _widget: &mut ChatWidget, _args: String) {
        // Prompt-expanding command
    }

    fn expand_prompt(&self, args: &str) -> Option<String> {
        Some(
            codex_core::slash_commands::format_subagent_command("unlock", args, None, None).prompt,
        )
    }

    fn requires_args(&self) -> bool {
        true
    }
}
