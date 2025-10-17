//! Configuration validation for spec-kit operations (T83)
//!
//! Validates config.toml settings before spec-kit pipeline execution:
//! - Agent configurations (enabled agents, commands)
//! - Subagent command definitions
//! - Spec-kit specific settings
//!
//! REBASE-SAFE: New file, 100% isolation, no upstream changes

use super::error::{Result, SpecKitError};
use codex_core::config::Config;
use codex_core::config_types::{AgentConfig, SubagentCommandConfig};
use std::collections::HashSet;

/// Validation errors found in configuration
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub issue: String,
    pub severity: ValidationSeverity,
}

/// Severity levels for validation issues
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationSeverity {
    /// Blocks execution
    Error,
    /// Should be fixed but allows execution
    Warning,
    /// Best practice suggestion
    Info,
}

/// Validates spec-kit configuration before pipeline execution
pub struct SpecKitConfigValidator;

impl SpecKitConfigValidator {
    /// Validate configuration for spec-kit operations
    ///
    /// Returns Ok(warnings) if config is usable, Err if blocking issues exist
    pub fn validate(config: &Config) -> Result<Vec<ValidationError>> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Validate agent configuration
        Self::validate_agents(&config.agents, &mut errors, &mut warnings);

        // Validate subagent commands
        Self::validate_subagent_commands(&config.subagent_commands, &mut errors, &mut warnings);

        // Check for spec-kit specific requirements
        Self::validate_spec_kit_requirements(config, &mut errors, &mut warnings);

        if !errors.is_empty() {
            let error_messages: Vec<String> = errors
                .iter()
                .map(|e| format!("{}: {}", e.field, e.issue))
                .collect();

            return Err(SpecKitError::from_string(format!(
                "Configuration validation failed:\n  - {}",
                error_messages.join("\n  - ")
            )));
        }

        Ok(warnings)
    }

    /// Validate agent configurations
    fn validate_agents(
        agents: &[AgentConfig],
        errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationError>,
    ) {
        if agents.is_empty() {
            warnings.push(ValidationError {
                field: "agents".to_string(),
                issue: "No agents configured, will use built-in defaults".to_string(),
                severity: ValidationSeverity::Info,
            });
            return;
        }

        // Check for enabled agents
        let enabled_agents: Vec<_> = agents.iter().filter(|a| a.enabled).collect();
        if enabled_agents.is_empty() {
            errors.push(ValidationError {
                field: "agents".to_string(),
                issue: "No agents are enabled. Spec-kit requires at least 1 enabled agent.".to_string(),
                severity: ValidationSeverity::Error,
            });
        }

        // Check for duplicate agent names
        let mut seen_names = HashSet::new();
        for agent in agents {
            if !seen_names.insert(&agent.name) {
                errors.push(ValidationError {
                    field: format!("agents.{}", agent.name),
                    issue: "Duplicate agent name".to_string(),
                    severity: ValidationSeverity::Error,
                });
            }
        }

        // Validate agent commands exist (non-empty)
        for agent in agents {
            if agent.command.is_empty() {
                errors.push(ValidationError {
                    field: format!("agents.{}.command", agent.name),
                    issue: "Agent command cannot be empty".to_string(),
                    severity: ValidationSeverity::Error,
                });
            }
        }

        // Warn if critical agents are disabled
        let critical_agents = ["gemini", "claude", "code"];
        for critical in &critical_agents {
            if let Some(agent) = agents.iter().find(|a| a.name.eq_ignore_ascii_case(critical)) {
                if !agent.enabled {
                    warnings.push(ValidationError {
                        field: format!("agents.{}", critical),
                        issue: format!("Critical agent '{}' is disabled. Multi-agent consensus requires 3 agents.", critical),
                        severity: ValidationSeverity::Warning,
                    });
                }
            }
        }
    }

    /// Validate subagent command configurations
    fn validate_subagent_commands(
        commands: &[SubagentCommandConfig],
        _errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationError>,
    ) {
        // Check for spec-kit commands
        let spec_kit_commands = ["plan", "tasks", "implement", "validate", "audit", "unlock"];
        let configured_commands: HashSet<_> = commands.iter().map(|c| c.name.as_str()).collect();

        for cmd in &spec_kit_commands {
            if !configured_commands.contains(cmd) {
                warnings.push(ValidationError {
                    field: format!("subagent_commands.{}", cmd),
                    issue: format!("Spec-kit command '{}' not configured, will use defaults", cmd),
                    severity: ValidationSeverity::Info,
                });
            }
        }
    }

    /// Validate spec-kit specific requirements
    fn validate_spec_kit_requirements(
        config: &Config,
        errors: &mut Vec<ValidationError>,
        warnings: &mut Vec<ValidationError>,
    ) {
        // Check working directory exists and is writable
        if !config.cwd.exists() {
            errors.push(ValidationError {
                field: "cwd".to_string(),
                issue: format!("Working directory does not exist: {}", config.cwd.display()),
                severity: ValidationSeverity::Error,
            });
        }

        // Check for git repository (spec-kit requires git)
        let git_dir = config.cwd.join(".git");
        if !git_dir.exists() {
            warnings.push(ValidationError {
                field: "cwd".to_string(),
                issue: "Not a git repository. Spec-kit requires git for commit generation.".to_string(),
                severity: ValidationSeverity::Warning,
            });
        }

        // Validate docs directory exists or can be created
        let docs_dir = config.cwd.join("docs");
        if !docs_dir.exists() {
            warnings.push(ValidationError {
                field: "docs".to_string(),
                issue: "docs/ directory does not exist. Will be created on first SPEC.".to_string(),
                severity: ValidationSeverity::Info,
            });
        }
    }

    /// Quick validation - returns true if config is usable
    pub fn is_valid(config: &Config) -> bool {
        Self::validate(config).is_ok()
    }

    /// Get all validation issues (errors + warnings) for reporting
    pub fn validate_all(config: &Config) -> (Vec<ValidationError>, Vec<ValidationError>) {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        Self::validate_agents(&config.agents, &mut errors, &mut warnings);
        Self::validate_subagent_commands(&config.subagent_commands, &mut errors, &mut warnings);
        Self::validate_spec_kit_requirements(config, &mut errors, &mut warnings);

        (errors, warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_core::config_types::AgentConfig;

    // Helper to create test agents
    fn make_agent(name: &str, enabled: bool) -> AgentConfig {
        AgentConfig {
            name: name.to_string(),
            command: name.to_string(),
            args: vec![],
            args_read_only: None,
            args_write: None,
            read_only: false,
            enabled,
            description: None,
            env: None,
            instructions: None,
        }
    }

    #[test]
    fn test_validate_agents_no_enabled_fails() {
        let agents = vec![make_agent("gemini", false)];
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        SpecKitConfigValidator::validate_agents(&agents, &mut errors, &mut warnings);

        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.issue.contains("No agents are enabled")));
    }

    #[test]
    fn test_validate_agents_duplicate_names_fails() {
        let agents = vec![make_agent("gemini", true), make_agent("gemini", true)];
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        SpecKitConfigValidator::validate_agents(&agents, &mut errors, &mut warnings);

        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.issue.contains("Duplicate")));
    }

    #[test]
    fn test_validate_agents_empty_command_fails() {
        let mut agent = make_agent("test", true);
        agent.command = String::new();

        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        SpecKitConfigValidator::validate_agents(&[agent], &mut errors, &mut warnings);

        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.issue.contains("command cannot be empty")));
    }

    #[test]
    fn test_validate_agents_disabled_critical_warns() {
        let agents = vec![
            make_agent("gemini", true),
            make_agent("claude", false),  // Critical agent disabled
        ];
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        SpecKitConfigValidator::validate_agents(&agents, &mut errors, &mut warnings);

        assert!(errors.is_empty());  // Not an error, just warning
        assert!(warnings.iter().any(|w| w.field.contains("claude")));
    }

    #[test]
    fn test_validate_agents_empty_list_warns() {
        let agents: Vec<AgentConfig> = vec![];
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        SpecKitConfigValidator::validate_agents(&agents, &mut errors, &mut warnings);

        assert!(errors.is_empty());  // Empty is OK (uses defaults)
        assert!(!warnings.is_empty());
    }

    #[test]
    fn test_validation_severity_levels() {
        let error = ValidationError {
            field: "test".to_string(),
            issue: "critical".to_string(),
            severity: ValidationSeverity::Error,
        };
        assert_eq!(error.severity, ValidationSeverity::Error);

        let warning = ValidationError {
            field: "test".to_string(),
            issue: "advisory".to_string(),
            severity: ValidationSeverity::Warning,
        };
        assert_eq!(warning.severity, ValidationSeverity::Warning);
    }
}
