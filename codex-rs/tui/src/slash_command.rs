use std::sync::OnceLock;

use strum::IntoEnumIterator;
use strum_macros::AsRefStr;
use strum_macros::EnumIter;
use strum_macros::EnumString;
use strum_macros::IntoStaticStr;

use crate::spec_prompts;
use crate::spec_prompts::SpecStage;
use thiserror::Error;

const BUILD_PROFILE: Option<&str> = option_env!("CODEX_PROFILE");

fn demo_command_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let profile_matches = |
            profile: &str
        | {
            let normalized = profile.trim().to_ascii_lowercase();
            normalized == "perf" || normalized.starts_with("dev")
        };

        if let Some(profile) = BUILD_PROFILE.or(option_env!("PROFILE")) {
            if profile_matches(profile) {
                return true;
            }
        }

        if let Ok(exe_path) = std::env::current_exe() {
            let path = exe_path.to_string_lossy().to_ascii_lowercase();
            if path.contains("target/dev-fast/")
                || path.contains("target/dev/")
                || path.contains("target/perf/")
            {
                return true;
            }
        }

        cfg!(debug_assertions)
    })
}

fn pro_command_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        let profile_matches = |
            profile: &str
        | {
            let normalized = profile.trim().to_ascii_lowercase();
            normalized.starts_with("dev") || normalized == "pref" || normalized == "perf"
        };

        if let Some(profile) = BUILD_PROFILE.or(option_env!("PROFILE")) {
            if profile_matches(profile) {
                return true;
            }
        }

        if let Ok(exe_path) = std::env::current_exe() {
            let path = exe_path.to_string_lossy().to_ascii_lowercase();
            if path.contains("target/dev-fast/")
                || path.contains("target/dev/")
                || path.contains("target/pref/")
                || path.contains("target/perf/")
            {
                return true;
            }
        }

        false
    })
}

/// Commands that can be invoked by starting a message with a leading slash.
///
/// IMPORTANT: When adding or changing slash commands, also update
/// `docs/slash-commands.md` at the repo root so users can discover them easily.
/// This enum is the source of truth for the list and ordering shown in the UI.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, EnumIter, AsRefStr, IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum SlashCommand {
    // DO NOT ALPHA-SORT! Enum order is presentation order in the popup, so
    // more frequently used commands should be listed first.
    Browser,
    Chrome,
    New,
    Init,
    Compact,
    Undo,
    Review,
    Diff,
    Mention,
    Cmd,
    Status,
    Limits,
    #[strum(serialize = "update", serialize = "upgrade")]
    Update,
    Theme,
    Model,
    Reasoning,
    Verbosity,
    Prompts,
    Perf,
    Demo,
    Agents,
    Pro,
    Branch,
    Merge,
    Github,
    Validation,
    Mcp,
    Resume,
    Login,
    // Prompt-expanding commands
    Plan,
    Solve,
    Code,
    // === FORK-SPECIFIC: spec-kit slash commands ===
    // Upstream: Does not have spec-kit automation
    // Preserve: All spec-* commands during rebases
    // Migration: T30 will move these to Project Commands (config.toml)
    #[strum(serialize = "spec-plan")]
    SpecPlan,
    #[strum(serialize = "spec-tasks")]
    SpecTasks,
    #[strum(serialize = "spec-implement")]
    SpecImplement,
    #[strum(serialize = "spec-validate")]
    SpecValidate,
    #[strum(serialize = "spec-audit")]
    SpecAudit,
    #[strum(serialize = "spec-unlock")]
    SpecUnlock,
    #[strum(serialize = "spec-auto")]
    SpecAuto,
    #[strum(serialize = "spec-ops-plan")]
    SpecOpsPlan,
    #[strum(serialize = "spec-ops-tasks")]
    SpecOpsTasks,
    #[strum(serialize = "spec-ops-implement")]
    SpecOpsImplement,
    #[strum(serialize = "spec-ops-validate")]
    SpecOpsValidate,
    #[strum(serialize = "spec-ops-audit")]
    SpecOpsAudit,
    #[strum(serialize = "spec-ops-unlock")]
    SpecOpsUnlock,
    #[strum(serialize = "spec-ops-auto")]
    SpecOpsAuto,
    #[strum(serialize = "spec-evidence-stats")]
    SpecEvidenceStats,
    #[strum(serialize = "spec-consensus")]
    SpecConsensus,
    // === END FORK-SPECIFIC: spec-kit commands ===
    Logout,
    Quit,
    #[cfg(debug_assertions)]
    TestApproval,
}

impl SlashCommand {
    /// User-visible description shown in the popup.
    pub fn description(self) -> &'static str {
        match self {
            SlashCommand::Chrome => "connect to Chrome",
            SlashCommand::Browser => "open internal browser",
            SlashCommand::Resume => "resume a past session for this folder",
            SlashCommand::Plan => "create a comprehensive plan (multiple agents)",
            SlashCommand::Solve => "solve a challenging problem (multiple agents)",
            SlashCommand::Code => "perform a coding task (multiple agents)",
            SlashCommand::Reasoning => "change reasoning effort (minimal/low/medium/high)",
            SlashCommand::Verbosity => "change text verbosity (high/medium/low)",
            SlashCommand::New => "start a new chat during a conversation",
            SlashCommand::Init => "create an AGENTS.md file with instructions for Code",
            SlashCommand::Compact => "summarize conversation to prevent hitting the context limit",
            SlashCommand::Undo => "restore the workspace to the last Code snapshot",
            SlashCommand::Review => "review your changes for potential issues",
            SlashCommand::Quit => "exit Code",
            SlashCommand::Diff => "show git diff (including untracked files)",
            SlashCommand::Mention => "mention a file",
            SlashCommand::Cmd => "run a project command",
            SlashCommand::Status => "show current session configuration and token usage",
            SlashCommand::Limits => "visualize weekly and hourly rate limits",
            SlashCommand::Update => "check for updates and optionally upgrade",
            SlashCommand::Theme => "switch between color themes",
            SlashCommand::Prompts => "show example prompts",
            SlashCommand::Model => "choose model & reasoning effort",
            SlashCommand::Agents => "create and configure agents",
            SlashCommand::SpecPlan => "multi-agent planning flow (requires SPEC ID and goal)",
            SlashCommand::SpecTasks => "multi-agent task synthesis (requires SPEC ID)",
            SlashCommand::SpecImplement => "multi-agent implementation design (requires SPEC ID)",
            SlashCommand::SpecValidate => "multi-agent validation consensus (requires SPEC ID)",
            SlashCommand::SpecAudit => "multi-agent audit/go-no-go (requires SPEC ID)",
            SlashCommand::SpecUnlock => "multi-agent unlock justification (requires SPEC ID)",
            SlashCommand::SpecAuto => "full automated pipeline with visible agents (orchestrator-driven)",
            SlashCommand::SpecOpsPlan => "run Spec Ops plan automation (requires SPEC ID)",
            SlashCommand::SpecOpsTasks => "run Spec Ops tasks automation (requires SPEC ID)",
            SlashCommand::SpecOpsImplement => {
                "run Spec Ops implement automation (requires SPEC ID)"
            }
            SlashCommand::SpecOpsValidate => {
                "run Spec Ops validate automation (requires SPEC ID)"
            }
            SlashCommand::SpecOpsAudit => {
                "run Spec Ops audit automation (requires SPEC ID)"
            }
            SlashCommand::SpecOpsUnlock => {
                "unlock SPEC.md copy-on-write lock (requires SPEC ID)"
            }
            SlashCommand::SpecOpsAuto => {
                "run Spec Ops guardrail sequence (requires SPEC ID; optional --from)"
            }
            SlashCommand::SpecEvidenceStats => {
                "summarize guardrail/consensus evidence sizes (optional --spec)"
            }
            SlashCommand::SpecConsensus => "check multi-agent consensus via local-memory (requires SPEC ID & stage)",
            SlashCommand::Pro => "manage Pro mode (toggle/status/auto)",
            SlashCommand::Branch => {
                "work in an isolated /branch then /merge when done (great for parallel work)"
            }
            SlashCommand::Merge => "merge current worktree branch back to default",
            SlashCommand::Github => "GitHub Actions watcher (status/on/off)",
            SlashCommand::Validation => "control validation harness (status/on/off)",
            SlashCommand::Mcp => "manage MCP servers (status/on/off/add)",
            SlashCommand::Perf => "performance tracing (on/off/show/reset)",
            SlashCommand::Demo => "populate history with demo cells (dev/perf only)",
            SlashCommand::Login => "manage Code sign-ins (add/select/disconnect)",
            SlashCommand::Logout => "log out of Code",
            #[cfg(debug_assertions)]
            SlashCommand::TestApproval => "test approval request",
        }
    }

    /// Command string without the leading '/'. Provided for compatibility with
    /// existing code that expects a method named `command()`.
    pub fn command(self) -> &'static str {
        self.into()
    }

    /// Returns true if this command should expand into a prompt for the LLM.
    pub fn is_prompt_expanding(self) -> bool {
        matches!(self, SlashCommand::Plan | SlashCommand::Solve | SlashCommand::Code)
    }

    /// Returns true if this command requires additional arguments after the command.
    pub fn requires_arguments(self) -> bool {
        matches!(self, SlashCommand::Plan | SlashCommand::Solve | SlashCommand::Code)
    }

    /// Returns true when this command maps to Spec Ops automation.
    pub fn is_spec_ops(self) -> bool {
        matches!(
            self,
            SlashCommand::SpecOpsPlan
                | SlashCommand::SpecOpsTasks
                | SlashCommand::SpecOpsImplement
                | SlashCommand::SpecOpsValidate
                | SlashCommand::SpecOpsAudit
                | SlashCommand::SpecOpsUnlock
                | SlashCommand::SpecOpsAuto
                | SlashCommand::SpecEvidenceStats
        )
    }

    /// Returns Spec Ops metadata for the command.
    pub fn spec_ops(self) -> Option<SpecOpsCommand> {
        match self {
            SlashCommand::SpecOpsPlan => Some(SpecOpsCommand {
                display: "plan",
                script: "spec_ops_plan.sh",
            }),
            SlashCommand::SpecOpsTasks => Some(SpecOpsCommand {
                display: "tasks",
                script: "spec_ops_tasks.sh",
            }),
            SlashCommand::SpecOpsImplement => Some(SpecOpsCommand {
                display: "implement",
                script: "spec_ops_implement.sh",
            }),
            SlashCommand::SpecOpsValidate => Some(SpecOpsCommand {
                display: "validate",
                script: "spec_ops_validate.sh",
            }),
            SlashCommand::SpecOpsAudit => Some(SpecOpsCommand {
                display: "audit",
                script: "spec_ops_audit.sh",
            }),
            SlashCommand::SpecOpsUnlock => Some(SpecOpsCommand {
                display: "unlock",
                script: "spec_ops_unlock.sh",
            }),
            SlashCommand::SpecOpsAuto => Some(SpecOpsCommand {
                display: "auto",
                script: "spec_auto.sh",
            }),
            SlashCommand::SpecEvidenceStats => Some(SpecOpsCommand {
                display: "evidence-stats",
                script: "evidence_stats.sh",
            }),
            _ => None,
        }
    }

    pub fn spec_stage(self) -> Option<SpecStage> {
        match self {
            SlashCommand::SpecPlan => Some(SpecStage::Plan),
            SlashCommand::SpecTasks => Some(SpecStage::Tasks),
            SlashCommand::SpecImplement => Some(SpecStage::Implement),
            SlashCommand::SpecValidate => Some(SpecStage::Validate),
            SlashCommand::SpecAudit => Some(SpecStage::Audit),
            SlashCommand::SpecUnlock => Some(SpecStage::Unlock),
            _ => None,
        }
    }

    pub fn is_available(self) -> bool {
        match self {
            SlashCommand::Pro => pro_command_enabled(),
            SlashCommand::Demo => demo_command_enabled(),
            _ => true,
        }
    }

    /// Expands a prompt-expanding command into a full prompt for the LLM.
    /// Returns None if the command is not a prompt-expanding command.
    pub fn expand_prompt(self, args: &str) -> Option<String> {
        if !self.is_prompt_expanding() {
            return None;
        }

        // Use the slash_commands module from core to generate the prompts
        // Note: We pass None for agents here as the TUI doesn't have access to the session config
        // The actual agents will be determined when the agent tool is invoked
        match self {
            SlashCommand::Plan => Some(codex_core::slash_commands::format_plan_command(
                args, None, None,
            )),
            SlashCommand::Solve => Some(codex_core::slash_commands::format_solve_command(
                args, None, None,
            )),
            SlashCommand::Code => Some(codex_core::slash_commands::format_code_command(
                args, None, None,
            )),
            _ => None,
        }
    }
}

/// Return all built-in commands in a Vec paired with their command string.
pub fn built_in_slash_commands() -> Vec<(&'static str, SlashCommand)> {
    SlashCommand::iter()
        .filter(|c| c.is_available())
        .map(|c| (c.command(), c))
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub struct SpecOpsCommand {
    pub display: &'static str,
    pub script: &'static str,
}

/// Process a message that might contain a slash command.
/// Returns either the expanded prompt (for prompt-expanding commands) or the original message.
pub fn process_slash_command_message(message: &str) -> ProcessedCommand {
    let trimmed = message.trim();

    if trimmed.is_empty() {
        return ProcessedCommand::NotCommand(message.to_string());
    }

    let has_slash = trimmed.starts_with('/');
    let command_portion = if has_slash { &trimmed[1..] } else { trimmed };
    let parts: Vec<&str> = command_portion.splitn(2, ' ').collect();
    let command_str = parts.first().copied().unwrap_or("");
    let args_raw = parts.get(1).map(|s| s.trim()).unwrap_or("");
    let canonical_command = command_str.to_ascii_lowercase();

    if matches!(canonical_command.as_str(), "quit" | "exit") {
        if !has_slash && !args_raw.is_empty() {
            return ProcessedCommand::NotCommand(message.to_string());
        }

        let command_text = if args_raw.is_empty() {
            format!("/{}", SlashCommand::Quit.command())
        } else {
            format!("/{} {}", SlashCommand::Quit.command(), args_raw)
        };

        return ProcessedCommand::RegularCommand {
            command: SlashCommand::Quit,
            command_text,
            notice: None,
        };
    }

    if !has_slash {
        return ProcessedCommand::NotCommand(message.to_string());
    }

    // Try to parse the command
    if let Ok(command) = canonical_command.parse::<SlashCommand>() {
        if !command.is_available() {
            let command_name = command.command();
            let message = match command {
                SlashCommand::Pro => {
                    "Error: /pro is only available in dev, dev-fast, or pref builds.".to_string()
                }
                SlashCommand::Demo => {
                    format!("Error: /{command_name} is only available in dev or perf builds.")
                }
                _ => format!("Error: /{command_name} is not available in this build."),
            };
            return ProcessedCommand::Error(message);
        }

        if let Some(stage) = command.spec_stage() {
            match spec_prompts::build_stage_prompt(stage, args_raw) {
                Ok(prompt) => return ProcessedCommand::ExpandedPrompt(prompt),
                Err(err) => return ProcessedCommand::Error(err.to_string()),
            }
        }

        if command == SlashCommand::SpecAuto {
            match parse_spec_auto_args(args_raw) {
                Ok(auto) => {
                    return ProcessedCommand::SpecAuto(auto);
                }
                Err(err) => {
                    return ProcessedCommand::Error(err.to_string());
                }
            }
        }

        // Check if it's a prompt-expanding command
        if command.is_prompt_expanding() {
            if args_raw.is_empty() && command.requires_arguments() {
                return ProcessedCommand::Error(format!(
                    "Error: /{} requires a task description. Usage: /{} <task>",
                    command.command(),
                    command.command()
                ));
            }

            if let Some(expanded) = command.expand_prompt(args_raw) {
                return ProcessedCommand::ExpandedPrompt(expanded);
            }
        }

        let command_text = if args_raw.is_empty() {
            format!("/{}", command.command())
        } else {
            format!("/{} {}", command.command(), args_raw)
        };

        ProcessedCommand::RegularCommand {
            command,
            command_text,
            notice: None,
        }
    } else {
        // Unknown command
        ProcessedCommand::NotCommand(message.to_string())
    }
}

#[derive(Debug, Clone)]
pub enum ProcessedCommand {
    /// The message was expanded from a prompt-expanding slash command
    ExpandedPrompt(String),
    /// A regular slash command that should be handled by the TUI. The `String`
    /// contains the canonical command text (with leading slash and trimmed args).
    RegularCommand {
        command: SlashCommand,
        command_text: String,
        notice: Option<String>,
    },
    SpecAuto(SpecAutoInvocation),
    /// Not a slash command, just a regular message
    #[allow(dead_code)]
    NotCommand(String),
    /// Error processing the command
    Error(String),
}

#[derive(Debug, Clone)]
pub struct SpecAutoInvocation {
    pub spec_id: String,
    pub goal: String,
    pub resume_from: SpecStage,
}

#[derive(Debug, Error)]
pub enum SpecAutoParseError {
    #[error("`/spec-auto` requires a SPEC ID (e.g. `/spec-auto SPEC-OPS-005`)")]
    MissingSpecId,
    #[error("`/spec-auto --from` requires a stage name")]
    MissingFromStage,
    #[error("Unknown stage '{0}'. Expected plan, tasks, implement, validate, review, or unlock.")]
    UnknownStage(String),
}

fn parse_spec_auto_args(args: &str) -> Result<SpecAutoInvocation, SpecAutoParseError> {
    let mut tokens = args.trim().split_whitespace();
    let Some(spec_token) = tokens.next() else {
        return Err(SpecAutoParseError::MissingSpecId);
    };

    let mut resume_from = SpecStage::Plan;
    let mut goal_tokens: Vec<String> = Vec::new();
    let mut pending_from = false;

    for token in tokens {
        if pending_from {
            resume_from = parse_stage_name(token)
                .ok_or_else(|| SpecAutoParseError::UnknownStage(token.to_string()))?;
            pending_from = false;
            continue;
        }

        match token {
            "--from" | "--resume-from" => {
                pending_from = true;
            }
            _ => goal_tokens.push(token.to_string()),
        }
    }

    if pending_from {
        return Err(SpecAutoParseError::MissingFromStage);
    }

    Ok(SpecAutoInvocation {
        spec_id: spec_token.to_string(),
        goal: goal_tokens.join(" "),
        resume_from,
    })
}

fn parse_stage_name(value: &str) -> Option<SpecStage> {
    match value.to_ascii_lowercase().as_str() {
        "plan" | "spec-plan" => Some(SpecStage::Plan),
        "tasks" | "spec-tasks" => Some(SpecStage::Tasks),
        "implement" | "spec-implement" => Some(SpecStage::Implement),
        "validate" | "spec-validate" => Some(SpecStage::Validate),
        "review" | "spec-review" | "audit" | "spec-ops-audit" => Some(SpecStage::Audit),
        "unlock" | "spec-unlock" => Some(SpecStage::Unlock),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_spec_alias_emits_notice() {
        let message = process_slash_command_message("/spec-plan SPEC-OPS-999");
        match message {
            ProcessedCommand::RegularCommand { command, command_text, notice } => {
                assert_eq!(command, SlashCommand::SpecOpsPlan);
                assert_eq!(command_text, "/spec-ops-plan SPEC-OPS-999");
                let notice = notice.expect("expected deprecation notice");
                assert!(notice.contains("/spec-plan"));
                assert!(notice.contains("/spec-ops-plan"));
            }
            other => panic!("expected RegularCommand with notice, got {other:?}"),
        }
    }

    #[test]
    fn parse_spec_auto_args_supports_from_flag() {
        let auto = parse_spec_auto_args("SPEC-OPS-007 --from tasks align checkout").unwrap();
       assert_eq!(auto.spec_id, "SPEC-OPS-007");
       assert_eq!(auto.goal, "align checkout");
       assert_eq!(auto.resume_from, SpecStage::Tasks);
    }

    #[test]
    fn spec_ops_auto_maps_to_spec_auto_script() {
        let command = SlashCommand::SpecOpsAuto;
        assert!(command.is_spec_ops(), "/spec-ops-auto should be recognized as Spec Ops command");
        let meta = command.spec_ops().expect("Spec Ops metadata");
        assert_eq!(meta.display, "auto");
        assert_eq!(meta.script, "spec_auto.sh");
    }

    #[test]
    fn spec_evidence_stats_maps_to_evidence_stats_script() {
        let command = SlashCommand::SpecEvidenceStats;
        assert!(
            command.is_spec_ops(),
            "/spec-evidence-stats should be recognized as Spec Ops command"
        );
        let meta = command.spec_ops().expect("Spec Ops metadata");
        assert_eq!(meta.display, "evidence-stats");
        assert_eq!(meta.script, "evidence_stats.sh");
    }
}
