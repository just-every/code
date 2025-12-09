use code_core::config_types::{AgentConfig, SubagentCommandConfig};
use code_core::protocol::{ReviewContextMetadata, ReviewRequest};

use code_core::slash_commands::format_subagent_command;

#[derive(Clone, Copy)]
pub struct SlashContext<'a> {
    pub agents: &'a [AgentConfig],
    pub subagent_commands: &'a [SubagentCommandConfig],
}

#[derive(Debug)]
pub enum SlashDispatch {
    NotSlash,
    ExpandedPrompt { prompt: String, summary: String },
    Review { request: ReviewRequest, summary: String },
}

pub fn process_exec_slash_command(message: &str, ctx: SlashContext<'_>) -> Result<SlashDispatch, String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Ok(SlashDispatch::NotSlash);
    }

    if !trimmed.starts_with('/') {
        return Ok(SlashDispatch::NotSlash);
    }

    let command_portion = trimmed.trim_start_matches('/');
    let mut parts = command_portion.splitn(2, |c: char| c.is_whitespace());
    let command_raw = parts.next().unwrap_or("");
    let args_raw = parts.next().unwrap_or("").trim();
    let command = command_raw.to_ascii_lowercase();

    if command.is_empty() {
        return Ok(SlashDispatch::NotSlash);
    }

    // Allow existing exec auto-drive handling to continue unchanged.
    if command == "auto" {
        return Ok(SlashDispatch::NotSlash);
    }

    match command.as_str() {
        "plan" | "solve" | "code" => handle_subagent(command.as_str(), args_raw, ctx),
        "review" => handle_review(args_raw),
        other => {
            // Custom subagents
            if ctx
                .subagent_commands
                .iter()
                .any(|c| c.name.eq_ignore_ascii_case(other))
            {
                return handle_subagent(other, args_raw, ctx);
            }

            Err(format!("Command '/{}' is not supported in exec mode.", other))
        }
    }
}

fn handle_subagent(
    name: &str,
    args: &str,
    ctx: SlashContext<'_>,
) -> Result<SlashDispatch, String> {
    if args.is_empty() {
        return Err(format!(
            "Error: /{name} requires a task description. Usage: /{name} <task>",
            name = name
        ));
    }

    let formatted = format_subagent_command(name, args, Some(ctx.agents), Some(ctx.subagent_commands));
    let summary_args = args.replace('\n', " ").trim().to_string();
    let summary = if summary_args.is_empty() {
        format!("/{name}")
    } else {
        format!("/{name} {summary_args}")
    };

    Ok(SlashDispatch::ExpandedPrompt {
        prompt: formatted.prompt,
        summary,
    })
}

fn handle_review(args_raw: &str) -> Result<SlashDispatch, String> {
    let (prompt, hint, metadata) = if args_raw.is_empty() {
        (
            "Review the current workspace changes and highlight bugs, regressions, risky patterns, and missing tests before merge.".to_string(),
            "current workspace changes".to_string(),
            ReviewContextMetadata {
                scope: Some("workspace".to_string()),
                ..Default::default()
            },
        )
    } else {
        let text = args_raw.trim().to_string();
        (
            text.clone(),
            text.clone(),
            ReviewContextMetadata {
                scope: Some("custom".to_string()),
                ..Default::default()
            },
        )
    };

    let summary = if args_raw.is_empty() {
        "/review".to_string()
    } else {
        let hint_clean = hint.replace('\n', " ").trim().to_string();
        if hint_clean.is_empty() {
            "/review".to_string()
        } else {
            format!("/review {hint_clean}")
        }
    };

    Ok(SlashDispatch::Review {
        request: ReviewRequest {
            prompt,
            user_facing_hint: hint,
            metadata: Some(metadata),
        },
        summary,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_core::config_types::{AgentConfig, SubagentCommandConfig};

    fn ctx<'a>(agents: &'a [AgentConfig], subagents: &'a [SubagentCommandConfig]) -> SlashContext<'a> {
        SlashContext { agents, subagent_commands: subagents }
    }

    #[test]
    fn plan_expands_to_prompt() {
        let result = process_exec_slash_command("/plan ship it", ctx(&[], &[])).unwrap();
        match result {
            SlashDispatch::ExpandedPrompt { prompt, summary } => {
                assert!(prompt.contains("Task for /plan"));
                assert_eq!(summary, "/plan ship it");
            }
            _ => panic!("expected expansion"),
        }
    }

    #[test]
    fn custom_subagent_is_supported() {
        let subagent = SubagentCommandConfig { name: "audit".to_string(), ..Default::default() };
        let result = process_exec_slash_command("/audit security pass", ctx(&[], &[subagent])).unwrap();
        match result {
            SlashDispatch::ExpandedPrompt { prompt, summary } => {
                assert!(prompt.contains("Task for /audit"));
                assert_eq!(summary, "/audit security pass");
            }
            other => panic!("unexpected result: {:?}", other),
        }
    }

    #[test]
    fn review_without_args_defaults_to_workspace_scope() {
        let result = process_exec_slash_command("/review", ctx(&[], &[])).unwrap();
        match result {
            SlashDispatch::Review { request, summary } => {
                assert_eq!(summary, "/review");
                assert_eq!(request.user_facing_hint, "current workspace changes");
                assert_eq!(request.metadata.unwrap().scope.unwrap(), "workspace");
            }
            _ => panic!("expected review"),
        }
    }

    #[test]
    fn unsupported_command_returns_error() {
        let result = process_exec_slash_command("/theme", ctx(&[], &[]));
        assert!(result.is_err());
    }
}
