//! Command registry for spec-kit slash commands
//!
//! This module implements a dynamic command registry to eliminate conflicts
//! in upstream SlashCommand enum. All spec-kit commands register here instead
//! of being enum variants.
//!
//! Design: docs/spec-kit/COMMAND_REGISTRY_DESIGN.md

use super::super::ChatWidget;
use super::commands::*;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

/// Trait for spec-kit commands that can be registered and executed dynamically
pub trait SpecKitCommand: Send + Sync {
    /// Primary command name (e.g., "speckit.new")
    fn name(&self) -> &'static str;

    /// Aliases for backward compatibility (e.g., ["new-spec"])
    fn aliases(&self) -> &[&'static str] {
        &[]
    }

    /// User-visible description shown in help/completion
    fn description(&self) -> &'static str;

    /// Execute the command with given arguments
    ///
    /// For prompt-expanding commands, this should not be called directly.
    /// Use expand_prompt() instead.
    fn execute(&self, widget: &mut ChatWidget, args: String);

    /// Expand prompt for multi-agent commands
    ///
    /// Returns None if command doesn't support prompt expansion.
    /// For prompt-expanding commands, this generates the full prompt text
    /// that should be submitted to the agent.
    fn expand_prompt(&self, _args: &str) -> Option<String> {
        None
    }

    /// Whether command requires arguments to execute
    fn requires_args(&self) -> bool {
        false
    }

    /// Whether command expands prompts for multi-agent execution
    fn is_prompt_expanding(&self) -> bool {
        self.expand_prompt("").is_some()
    }

    /// Whether command is a guardrail validation command
    fn is_guardrail(&self) -> bool {
        false
    }

    /// Get guardrail script metadata (for guardrail commands only)
    ///
    /// Returns (display_name, script_path) tuple for guardrail execution.
    /// Returns None for non-guardrail commands.
    fn guardrail_script(&self) -> Option<(&'static str, &'static str)> {
        None
    }
}

/// Registry for dynamically registered spec-kit commands
pub struct CommandRegistry {
    /// Commands indexed by primary name
    commands: HashMap<String, Box<dyn SpecKitCommand>>,
    /// Alias → primary name mapping for backward compatibility
    by_alias: HashMap<String, String>,
}

impl CommandRegistry {
    /// Create a new empty command registry
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
            by_alias: HashMap::new(),
        }
    }

    /// Register a command and all its aliases
    pub fn register(&mut self, command: Box<dyn SpecKitCommand>) {
        let name = command.name().to_string();

        // Register all aliases pointing to primary name
        for alias in command.aliases() {
            self.by_alias.insert(alias.to_string(), name.clone());
        }

        // Store command by primary name
        self.commands.insert(name, command);
    }

    /// Find a command by name or alias
    pub fn find(&self, name: &str) -> Option<&dyn SpecKitCommand> {
        // Try primary name first
        if let Some(cmd) = self.commands.get(name) {
            return Some(cmd.as_ref());
        }

        // Try aliases
        if let Some(primary) = self.by_alias.get(name) {
            return self.commands.get(primary).map(|cmd| cmd.as_ref());
        }

        None
    }

    /// Iterate over all registered commands
    pub fn iter(&self) -> impl Iterator<Item = &dyn SpecKitCommand> {
        self.commands.values().map(|cmd| cmd.as_ref())
    }

    /// Count of registered commands
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Get all command names (primary + aliases)
    pub fn all_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.commands.keys().cloned().collect();
        names.extend(self.by_alias.keys().cloned());
        names.sort();
        names
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global spec-kit command registry
///
/// All spec-kit commands are registered here on first access.
/// This eliminates the need for enum variants in SlashCommand.
pub static SPEC_KIT_REGISTRY: Lazy<Mutex<CommandRegistry>> = Lazy::new(|| {
    let mut registry = CommandRegistry::new();

    // Special commands
    registry.register(Box::new(SpecKitNewCommand));
    registry.register(Box::new(SpecKitSpecifyCommand));
    registry.register(Box::new(SpecKitAutoCommand));
    registry.register(Box::new(SpecKitStatusCommand));
    registry.register(Box::new(SpecConsensusCommand));

    // Stage commands (plan → unlock)
    registry.register(Box::new(SpecKitPlanCommand));
    registry.register(Box::new(SpecKitTasksCommand));
    registry.register(Box::new(SpecKitImplementCommand));
    registry.register(Box::new(SpecKitValidateCommand));
    registry.register(Box::new(SpecKitAuditCommand));
    registry.register(Box::new(SpecKitUnlockCommand));

    // Quality commands
    registry.register(Box::new(SpecKitClarifyCommand));
    registry.register(Box::new(SpecKitAnalyzeCommand));
    registry.register(Box::new(SpecKitChecklistCommand));

    // Guardrail commands
    registry.register(Box::new(GuardrailPlanCommand));
    registry.register(Box::new(GuardrailTasksCommand));
    registry.register(Box::new(GuardrailImplementCommand));
    registry.register(Box::new(GuardrailValidateCommand));
    registry.register(Box::new(GuardrailAuditCommand));
    registry.register(Box::new(GuardrailUnlockCommand));
    registry.register(Box::new(GuardrailAutoCommand));

    // Evidence/stats command
    registry.register(Box::new(SpecEvidenceStatsCommand));

    Mutex::new(registry)
});

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCommand {
        name: &'static str,
        aliases: Vec<&'static str>,
    }

    impl SpecKitCommand for TestCommand {
        fn name(&self) -> &'static str {
            self.name
        }

        fn aliases(&self) -> &[&'static str] {
            &self.aliases
        }

        fn description(&self) -> &'static str {
            "test command"
        }

        fn execute(&self, _widget: &mut ChatWidget, _args: String) {
            // No-op for testing
        }
    }

    #[test]
    fn test_registry_basic() {
        let mut registry = CommandRegistry::new();
        assert!(registry.is_empty());

        registry.register(Box::new(TestCommand {
            name: "test.cmd",
            aliases: vec!["test-cmd"],
        }));

        assert_eq!(registry.len(), 1);
        assert!(registry.find("test.cmd").is_some());
        assert!(registry.find("test-cmd").is_some());
        assert!(registry.find("unknown").is_none());
    }

    #[test]
    fn test_registry_aliases() {
        let mut registry = CommandRegistry::new();

        registry.register(Box::new(TestCommand {
            name: "speckit.new",
            aliases: vec!["new-spec", "spec-new"],
        }));

        // Primary name works
        let cmd1 = registry.find("speckit.new");
        assert!(cmd1.is_some());
        assert_eq!(cmd1.unwrap().name(), "speckit.new");

        // Both aliases work
        let cmd2 = registry.find("new-spec");
        assert!(cmd2.is_some());
        assert_eq!(cmd2.unwrap().name(), "speckit.new");

        let cmd3 = registry.find("spec-new");
        assert!(cmd3.is_some());
        assert_eq!(cmd3.unwrap().name(), "speckit.new");
    }

    #[test]
    fn test_registry_iteration() {
        let mut registry = CommandRegistry::new();

        registry.register(Box::new(TestCommand {
            name: "cmd1",
            aliases: vec![],
        }));
        registry.register(Box::new(TestCommand {
            name: "cmd2",
            aliases: vec![],
        }));

        let names: Vec<&str> = registry.iter().map(|cmd| cmd.name()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"cmd1"));
        assert!(names.contains(&"cmd2"));
    }

    #[test]
    fn test_all_names() {
        let mut registry = CommandRegistry::new();

        registry.register(Box::new(TestCommand {
            name: "speckit.plan",
            aliases: vec!["spec-plan", "spec-ops-plan"],
        }));

        let all = registry.all_names();
        assert_eq!(all.len(), 3);
        assert!(all.contains(&"speckit.plan".to_string()));
        assert!(all.contains(&"spec-plan".to_string()));
        assert!(all.contains(&"spec-ops-plan".to_string()));
    }

    #[test]
    fn test_global_registry_populated() {
        // Test that the global registry has all expected commands
        let registry = SPEC_KIT_REGISTRY.lock().unwrap();

        // Should have all 22 commands
        assert_eq!(registry.len(), 22, "Registry should have 22 commands");

        // Verify key commands are registered
        assert!(registry.find("speckit.status").is_some());
        assert!(registry.find("speckit.new").is_some());
        assert!(registry.find("speckit.plan").is_some());
        assert!(registry.find("speckit.auto").is_some());
        assert!(registry.find("guardrail.plan").is_some());
    }

    #[test]
    fn test_legacy_aliases_work() {
        // Test that all legacy command names still work
        let registry = SPEC_KIT_REGISTRY.lock().unwrap();

        // Legacy spec-* commands
        assert!(registry.find("spec-plan").is_some());
        assert!(registry.find("spec-tasks").is_some());
        assert!(registry.find("spec-implement").is_some());
        assert!(registry.find("spec-auto").is_some());
        assert!(registry.find("new-spec").is_some());

        // Legacy spec-ops-* commands
        assert!(registry.find("spec-ops-plan").is_some());
        assert!(registry.find("spec-ops-tasks").is_some());
        assert!(registry.find("spec-ops-implement").is_some());
        assert!(registry.find("spec-ops-validate").is_some());
        assert!(registry.find("spec-ops-audit").is_some());
        assert!(registry.find("spec-ops-unlock").is_some());
        assert!(registry.find("spec-ops-auto").is_some());

        // Legacy status commands
        assert!(registry.find("spec-status").is_some());
    }

    #[test]
    fn test_all_stage_commands_registered() {
        let registry = SPEC_KIT_REGISTRY.lock().unwrap();

        // Verify all 6 stage commands
        let stages = vec![
            "speckit.plan",
            "speckit.tasks",
            "speckit.implement",
            "speckit.validate",
            "speckit.audit",
            "speckit.unlock",
        ];

        for stage in stages {
            let cmd = registry
                .find(stage)
                .expect(&format!("{} should be registered", stage));
            assert_eq!(cmd.name(), stage);
        }
    }

    #[test]
    fn test_all_guardrail_commands_registered() {
        let registry = SPEC_KIT_REGISTRY.lock().unwrap();

        // Verify all 7 guardrail commands
        let guardrails = vec![
            "guardrail.plan",
            "guardrail.tasks",
            "guardrail.implement",
            "guardrail.validate",
            "guardrail.audit",
            "guardrail.unlock",
            "guardrail.auto",
        ];

        for guardrail in guardrails {
            let cmd = registry
                .find(guardrail)
                .expect(&format!("{} should be registered", guardrail));
            assert_eq!(cmd.name(), guardrail);
            assert!(
                cmd.is_guardrail(),
                "{} should be marked as guardrail",
                guardrail
            );
        }
    }

    #[test]
    fn test_quality_commands_registered() {
        let registry = SPEC_KIT_REGISTRY.lock().unwrap();

        // Verify all 3 quality commands
        let quality = vec!["speckit.clarify", "speckit.analyze", "speckit.checklist"];

        for cmd_name in quality {
            assert!(
                registry.find(cmd_name).is_some(),
                "{} should be registered",
                cmd_name
            );
        }
    }

    #[test]
    fn test_special_commands_registered() {
        let registry = SPEC_KIT_REGISTRY.lock().unwrap();

        // Verify special commands
        assert!(registry.find("speckit.new").is_some());
        assert!(registry.find("speckit.specify").is_some());
        assert!(registry.find("speckit.auto").is_some());
        assert!(registry.find("speckit.status").is_some());
        assert!(registry.find("spec-consensus").is_some());
        assert!(registry.find("spec-evidence-stats").is_some());
    }

    #[test]
    fn test_command_descriptions_not_empty() {
        let registry = SPEC_KIT_REGISTRY.lock().unwrap();

        // All commands should have non-empty descriptions
        for cmd in registry.iter() {
            assert!(
                !cmd.description().is_empty(),
                "Command {} should have a description",
                cmd.name()
            );
        }
    }

    #[test]
    fn test_prompt_expanding_commands() {
        let registry = SPEC_KIT_REGISTRY.lock().unwrap();

        // Stage commands should be prompt-expanding
        let expanding = vec![
            "speckit.plan",
            "speckit.tasks",
            "speckit.implement",
            "speckit.validate",
            "speckit.audit",
            "speckit.unlock",
        ];

        for cmd_name in expanding {
            let cmd = registry.find(cmd_name).unwrap();
            let expanded = cmd.expand_prompt("SPEC-TEST-001");
            assert!(
                expanded.is_some(),
                "{} should expand prompts",
                cmd_name
            );
            assert!(
                !expanded.unwrap().is_empty(),
                "{} expanded prompt should not be empty",
                cmd_name
            );
        }
    }

    #[test]
    fn test_non_expanding_commands() {
        let registry = SPEC_KIT_REGISTRY.lock().unwrap();

        // Guardrail and status commands should NOT expand prompts
        let non_expanding = vec![
            "guardrail.plan",
            "speckit.status",
            "spec-consensus",
            "spec-evidence-stats",
        ];

        for cmd_name in non_expanding {
            let cmd = registry.find(cmd_name).unwrap();
            assert!(
                cmd.expand_prompt("test").is_none(),
                "{} should not expand prompts",
                cmd_name
            );
        }
    }

    #[test]
    fn test_guardrail_script_metadata() {
        let registry = SPEC_KIT_REGISTRY.lock().unwrap();

        // Guardrail commands should have script metadata
        let cmd = registry.find("guardrail.plan").unwrap();
        let script = cmd.guardrail_script();
        assert!(script.is_some(), "guardrail.plan should have script metadata");
        let (display, script_path) = script.unwrap();
        assert_eq!(display, "plan");
        assert_eq!(script_path, "spec_ops_plan.sh");

        // Non-guardrail commands should not have script metadata
        let cmd = registry.find("speckit.status").unwrap();
        assert!(
            cmd.guardrail_script().is_none(),
            "speckit.status should not have script metadata"
        );
    }

    #[test]
    fn test_all_names_count() {
        let registry = SPEC_KIT_REGISTRY.lock().unwrap();

        // 22 primary names + 16 aliases = 38 total names
        let all_names = registry.all_names();
        assert_eq!(
            all_names.len(),
            38,
            "Should have 38 total command names (22 primary + 16 aliases)"
        );
    }

    #[test]
    fn test_no_duplicate_aliases() {
        let registry = SPEC_KIT_REGISTRY.lock().unwrap();

        let all_names = registry.all_names();
        let unique_count = all_names.iter().collect::<std::collections::HashSet<_>>().len();

        assert_eq!(
            all_names.len(),
            unique_count,
            "All command names should be unique (no duplicate aliases)"
        );
    }
}
