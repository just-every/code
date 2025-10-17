//! Guardrail command implementations

use super::super::command_registry::SpecKitCommand;
use super::super::super::ChatWidget;
use crate::slash_command::HalMode;

/// Command: /guardrail.plan (and /spec-ops-plan)
pub struct GuardrailPlanCommand;

impl SpecKitCommand for GuardrailPlanCommand {
    fn name(&self) -> &'static str {
        "guardrail.plan"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-ops-plan"]
    }

    fn description(&self) -> &'static str {
        "run guardrail validation for plan stage"
    }

    fn execute(&self, widget: &mut ChatWidget, args: String) {
        // Delegate to guardrail handler with script metadata
        self.execute_guardrail(widget, args, None);
    }

    fn is_guardrail(&self) -> bool {
        true
    }

    fn guardrail_script(&self) -> Option<(&'static str, &'static str)> {
        Some(("plan", "spec_ops_plan.sh"))
    }
}

impl GuardrailPlanCommand {
    fn execute_guardrail(
        &self,
        widget: &mut ChatWidget,
        args: String,
        hal_override: Option<HalMode>,
    ) {
        // For now, we still need to call the existing handler
        // In Phase 3, we'll refactor the handler to not need SlashCommand
        // TODO: Refactor handler to accept script metadata directly
        widget.handle_spec_ops_command(
            crate::slash_command::SlashCommand::GuardrailPlan,
            args,
            hal_override,
        );
    }
}

/// Command: /guardrail.tasks (and /spec-ops-tasks)
pub struct GuardrailTasksCommand;

impl SpecKitCommand for GuardrailTasksCommand {
    fn name(&self) -> &'static str {
        "guardrail.tasks"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-ops-tasks"]
    }

    fn description(&self) -> &'static str {
        "run guardrail validation for tasks stage"
    }

    fn execute(&self, widget: &mut ChatWidget, args: String) {
        widget.handle_spec_ops_command(
            crate::slash_command::SlashCommand::GuardrailTasks,
            args,
            None,
        );
    }

    fn is_guardrail(&self) -> bool {
        true
    }

    fn guardrail_script(&self) -> Option<(&'static str, &'static str)> {
        Some(("tasks", "spec_ops_tasks.sh"))
    }
}

/// Command: /guardrail.implement (and /spec-ops-implement)
pub struct GuardrailImplementCommand;

impl SpecKitCommand for GuardrailImplementCommand {
    fn name(&self) -> &'static str {
        "guardrail.implement"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-ops-implement"]
    }

    fn description(&self) -> &'static str {
        "run guardrail validation for implement stage"
    }

    fn execute(&self, widget: &mut ChatWidget, args: String) {
        widget.handle_spec_ops_command(
            crate::slash_command::SlashCommand::GuardrailImplement,
            args,
            None,
        );
    }

    fn is_guardrail(&self) -> bool {
        true
    }

    fn guardrail_script(&self) -> Option<(&'static str, &'static str)> {
        Some(("implement", "spec_ops_implement.sh"))
    }
}

/// Command: /guardrail.validate (and /spec-ops-validate)
pub struct GuardrailValidateCommand;

impl SpecKitCommand for GuardrailValidateCommand {
    fn name(&self) -> &'static str {
        "guardrail.validate"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-ops-validate"]
    }

    fn description(&self) -> &'static str {
        "run guardrail validation for validate stage"
    }

    fn execute(&self, widget: &mut ChatWidget, args: String) {
        widget.handle_spec_ops_command(
            crate::slash_command::SlashCommand::GuardrailValidate,
            args,
            None,
        );
    }

    fn is_guardrail(&self) -> bool {
        true
    }

    fn guardrail_script(&self) -> Option<(&'static str, &'static str)> {
        Some(("validate", "spec_ops_validate.sh"))
    }
}

/// Command: /guardrail.audit (and /spec-ops-audit)
pub struct GuardrailAuditCommand;

impl SpecKitCommand for GuardrailAuditCommand {
    fn name(&self) -> &'static str {
        "guardrail.audit"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-ops-audit"]
    }

    fn description(&self) -> &'static str {
        "run guardrail validation for audit stage"
    }

    fn execute(&self, widget: &mut ChatWidget, args: String) {
        widget.handle_spec_ops_command(
            crate::slash_command::SlashCommand::GuardrailAudit,
            args,
            None,
        );
    }

    fn is_guardrail(&self) -> bool {
        true
    }

    fn guardrail_script(&self) -> Option<(&'static str, &'static str)> {
        Some(("audit", "spec_ops_audit.sh"))
    }
}

/// Command: /guardrail.unlock (and /spec-ops-unlock)
pub struct GuardrailUnlockCommand;

impl SpecKitCommand for GuardrailUnlockCommand {
    fn name(&self) -> &'static str {
        "guardrail.unlock"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-ops-unlock"]
    }

    fn description(&self) -> &'static str {
        "run guardrail validation for unlock stage"
    }

    fn execute(&self, widget: &mut ChatWidget, args: String) {
        widget.handle_spec_ops_command(
            crate::slash_command::SlashCommand::GuardrailUnlock,
            args,
            None,
        );
    }

    fn is_guardrail(&self) -> bool {
        true
    }

    fn guardrail_script(&self) -> Option<(&'static str, &'static str)> {
        Some(("unlock", "spec_ops_unlock.sh"))
    }
}

/// Command: /guardrail.auto (and /spec-ops-auto)
pub struct GuardrailAutoCommand;

impl SpecKitCommand for GuardrailAutoCommand {
    fn name(&self) -> &'static str {
        "guardrail.auto"
    }

    fn aliases(&self) -> &[&'static str] {
        &["spec-ops-auto"]
    }

    fn description(&self) -> &'static str {
        "run full guardrail pipeline with telemetry"
    }

    fn execute(&self, widget: &mut ChatWidget, args: String) {
        widget.handle_spec_ops_command(
            crate::slash_command::SlashCommand::GuardrailAuto,
            args,
            None,
        );
    }

    fn is_guardrail(&self) -> bool {
        true
    }

    fn guardrail_script(&self) -> Option<(&'static str, &'static str)> {
        Some(("auto", "spec_auto.sh"))
    }
}

/// Command: /spec-evidence-stats
pub struct SpecEvidenceStatsCommand;

impl SpecKitCommand for SpecEvidenceStatsCommand {
    fn name(&self) -> &'static str {
        "spec-evidence-stats"
    }

    fn aliases(&self) -> &[&'static str] {
        &[]
    }

    fn description(&self) -> &'static str {
        "summarize guardrail/consensus evidence sizes (optional --spec)"
    }

    fn execute(&self, widget: &mut ChatWidget, args: String) {
        widget.handle_spec_ops_command(
            crate::slash_command::SlashCommand::SpecEvidenceStats,
            args,
            None,
        );
    }

    fn is_guardrail(&self) -> bool {
        true
    }

    fn guardrail_script(&self) -> Option<(&'static str, &'static str)> {
        Some(("evidence-stats", "evidence_stats.sh"))
    }
}
