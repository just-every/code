//! Spec-Kit command routing
//!
//! This module handles routing slash commands to the spec-kit command registry,
//! isolating all routing logic from app.rs to minimize rebase conflicts.

use super::super::ChatWidget;
use super::command_registry::SPEC_KIT_REGISTRY;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use codex_core::protocol::Op;

/// Try to dispatch a command through the spec-kit command registry
///
/// Returns true if the command was handled by the registry, false otherwise.
/// This allows app.rs to fall through to upstream command handling if the
/// command is not a spec-kit command.
///
/// # Arguments
/// * `widget` - The chat widget to execute the command on
/// * `command_text` - The full command text (e.g., "/speckit.plan SPEC-KIT-065")
/// * `app_event_tx` - Event sender for history persistence
///
/// # Returns
/// - `true` if command was handled by spec-kit registry
/// - `false` if command should fall through to upstream routing
pub fn try_dispatch_spec_kit_command(
    widget: &mut ChatWidget,
    command_text: &str,
    app_event_tx: &AppEventSender,
) -> bool {
    // Extract command name (first token after /)
    let command_name = command_text
        .trim_start_matches('/')
        .split_whitespace()
        .next()
        .unwrap_or("");

    // Try to find command in registry
    let Ok(registry) = SPEC_KIT_REGISTRY.lock() else {
        // Registry mutex poisoned - fall through to upstream
        return false;
    };

    let Some(spec_cmd) = registry.find(command_name) else {
        // Not a spec-kit command - fall through to upstream
        return false;
    };

    // Extract arguments (everything after command name)
    let args = command_text
        .trim_start_matches('/')
        .trim_start_matches(command_name)
        .trim()
        .to_string();

    // Handle prompt-expanding vs direct execution
    if let Some(expanded) = spec_cmd.expand_prompt(&args) {
        // Prompt-expanding command: submit expanded prompt to agent
        widget.submit_prompt_with_display(command_text.to_string(), expanded);
    } else {
        // Direct execution: persist to history then execute
        let _ = app_event_tx.send(AppEvent::CodexOp(Op::AddToHistory {
            text: command_text.to_string(),
        }));
        spec_cmd.execute(widget, args);
    }

    // Command was handled
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_name_extraction() {
        // Test command name parsing
        let test_cases = vec![
            ("/speckit.status", "speckit.status"),
            ("/speckit.plan SPEC-KIT-065", "speckit.plan"),
            ("/guardrail.auto SPEC-KIT-065 --from plan", "guardrail.auto"),
            ("/spec-consensus SPEC-KIT-065 plan", "spec-consensus"),
        ];

        for (input, expected) in test_cases {
            let name = input
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("");
            assert_eq!(name, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_args_extraction() {
        // Test argument extraction
        let test_cases = vec![
            ("/speckit.status", "speckit.status", ""),
            ("/speckit.plan SPEC-KIT-065", "speckit.plan", "SPEC-KIT-065"),
            (
                "/guardrail.auto SPEC-KIT-065 --from plan",
                "guardrail.auto",
                "SPEC-KIT-065 --from plan",
            ),
        ];

        for (command_text, cmd_name, expected_args) in test_cases {
            let args = command_text
                .trim_start_matches('/')
                .trim_start_matches(cmd_name)
                .trim();
            assert_eq!(args, expected_args, "Failed for input: {}", command_text);
        }
    }

    #[test]
    fn test_registry_find_returns_true() {
        // Verify that known commands return true from try_dispatch
        let registry = SPEC_KIT_REGISTRY.lock().unwrap();

        // All registered commands should be findable
        assert!(registry.find("speckit.status").is_some());
        assert!(registry.find("speckit.plan").is_some());
        assert!(registry.find("guardrail.plan").is_some());
        assert!(registry.find("spec-status").is_some());

        // Unknown commands should not be found
        assert!(registry.find("unknown-command").is_none());
        assert!(registry.find("browser").is_none()); // upstream command
    }
}
