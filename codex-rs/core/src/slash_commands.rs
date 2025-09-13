use crate::config_types::AgentConfig;

// NOTE: These are the prompt formatters for the prompt‑expanding slash commands
// (/plan, /solve, /code). If you add or change a slash command, please update
// the user documentation in `docs/slash-commands.md` so the list stays in sync
// with the UI and behavior.

/// Get the list of enabled agent names from the configuration
pub fn get_enabled_agents(agents: &[AgentConfig]) -> Vec<String> {
    agents
        .iter()
        .filter(|agent| agent.enabled)
        .map(|agent| agent.name.clone())
        .collect()
}

/// Get default models if no agents are configured
fn get_default_models() -> Vec<String> {
    vec![
        "claude".to_string(),
        "qwen".to_string(),
        "gemini".to_string(),
        "codex".to_string(),
    ]
}

/// Format the /plan command into a prompt for the LLM
pub fn format_plan_command(
    task: &str,
    models: Option<Vec<String>>,
    agents: Option<&[AgentConfig]>,
) -> String {
    let models_list = models.unwrap_or_else(|| {
        if let Some(agents) = agents {
            let enabled = get_enabled_agents(agents);
            if !enabled.is_empty() {
                enabled
            } else {
                get_default_models()
            }
        } else {
            get_default_models()
        }
    });
    let models_str = models_list
        .iter()
        .map(|m| format!("\"{}\"", m))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        r#"Create a comprehensive plan by leveraging multiple state-of-the-art LLMs working in parallel.

Use the agent tool to start a batch of agents using agent_run with:
- models: an array containing [{}]
- read_only: true (planning mode - no file modifications)
Provide a comprehensive description of the task and context. You should research the code base first and provide a general directory structure to give the models a head start of where to look. You can include one or two key files but also allow the models to look up the files they need themselves.

This will start all agents at once and return a batch_id.

IMPORTANT: Use agent_wait with the batch_id and return_all: true to wait for ALL agents to complete. This ensures you get all perspectives before formulating the final plan. If an agent fails or times out, you can ignore it and continue with the other results.

Once all models have completed:
1. Analyze all the different plans and recommendations
2. Identify common themes and best practices from each model
3. Synthesize the best elements from each plan
4. Create a final, comprehensive plan that incorporates the strongest recommendations from all models
5. Present the final plan with clear steps and rationale

Task to plan:
{}"#,
        models_str, task
    )
}

/// Format the /solve command into a prompt for the LLM
pub fn format_solve_command(
    task: &str,
    models: Option<Vec<String>>,
    agents: Option<&[AgentConfig]>,
) -> String {
    let models_list = models.unwrap_or_else(|| {
        if let Some(agents) = agents {
            let enabled = get_enabled_agents(agents);
            if !enabled.is_empty() {
                enabled
            } else {
                get_default_models()
            }
        } else {
            get_default_models()
        }
    });
    let models_str = models_list
        .iter()
        .map(|m| format!("\"{}\"", m))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        r#"Solve a complicated problem by starting multiple agents with state of the art LLMs.

Use the agent tool to start a batch of agents using a SINGLE agent_run with:
- models: an array containing [{}]
- read_only: true (so agents don't edit files but can read them)
Provide an extremely comprehensive description of the task and context. You should research the background information thoroughly and include any relevant details that could help the models understand the problem better. Include ALL relevant files you find.

This will start all agents at once and return a batch_id.

To monitor progress, you have two options:
1. Use agent_wait with the batch_id to block until the next agent completes (efficient, ignores already-completed agents)
2. Use agent_list with the batch_id to poll and check status manually

As soon as one completes you can try implementing the solution it proposes. IMPORTANT: You must thoroughly test and verify that the solution works correctly before considering the problem solved. This includes:
- Running any relevant tests
- Verifying the output is correct
- Checking for edge cases
- Ensuring no errors occur

If the solution does not work or only partially works, start a new agent with the same model/class and explain the problem, its suggested solution and why it didn't work. Check for any other completed agents and if they have a different solution try that. Keep multiple agents running in the background to explore different approaches.

**CRITICAL: Only cancel remaining agents AFTER you have:**
1. Implemented a complete solution
2. Thoroughly tested it works correctly
3. Verified the problem is 100% solved
4. Confirmed there are no errors or issues

**NEVER cancel agents if:**
- The solution is incomplete or untested
- You encounter errors during implementation
- The problem is only partially solved
- You haven't verified the solution works

Once you complete implement the solution from the first agent, check that no other agents have also completed before returning to the user. You may get a second opinion with a better result which is what the multi agent process is all about! Don't be too confident in any solution you implement.

Problem to solve:
{}

Remember: DO NOT cancel any running agents until you have 100% confirmed the problem is completely solved with a working, tested solution."#,
        models_str, task
    )
}

/// Format the /code command into a prompt for the LLM
pub fn format_code_command(
    task: &str,
    models: Option<Vec<String>>,
    agents: Option<&[AgentConfig]>,
) -> String {
    let models_list = models.unwrap_or_else(|| {
        if let Some(agents) = agents {
            let enabled = get_enabled_agents(agents);
            if !enabled.is_empty() {
                enabled
            } else {
                get_default_models()
            }
        } else {
            get_default_models()
        }
    });
    let models_str = models_list
        .iter()
        .map(|m| format!("\"{}\"", m))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        r#"Perform a coding task with multiple LLMs and compare the results.

Use the agent tool to start a batch of agents using agent_run with:
- models: an array containing [{}]
- read_only: false (allow file modifications and code execution)
Provide a comprehensive description of the task and context. You should research the code base first to give the model a head start of where to look. You can include one or two key files but also allow the models to look up the files they need themselves.

The agents in separate worktrees will execute with full permissions to:
- Read and analyze existing code
- Create new files
- Modify existing files
- Execute commands
- Run tests
- Install dependencies

IMPORTANT: When agents complete, the response will include the worktree_path and branch_name for each agent. This shows you exactly where each model's work is located:
- worktree_path: The full path to the git worktree where the model made its changes
- branch_name: The git branch name created for that model's work

Monitor the agent progress using agent_check and wait for completion with agent_wait.

Once the agents are complete:
1. Check the worktree paths returned for each model
2. View their implementations in their respective worktrees
3. Compare their different approaches and solutions
4. Use git to examine the changes each model made
5. Bring the best parts of each solution into your own final implementation

Coding task to perform:
{}"#,
        models_str, task
    )
}

/// Parse a slash command and return the formatted prompt
pub fn handle_slash_command(input: &str, agents: Option<&[AgentConfig]>) -> Option<String> {
    let input = input.trim();

    // Check if it starts with a slash
    if !input.starts_with('/') {
        return None;
    }

    // Parse the command and arguments
    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    let command = parts[0];
    let args = parts.get(1).map(|s| s.to_string()).unwrap_or_default();

    match command {
        "/plan" => {
            if args.is_empty() {
                Some("Error: /plan requires a task description. Usage: /plan <task>".to_string())
            } else {
                Some(format_plan_command(&args, None, agents))
            }
        }
        "/solve" => {
            if args.is_empty() {
                Some(
                    "Error: /solve requires a problem description. Usage: /solve <problem>"
                        .to_string(),
                )
            } else {
                Some(format_solve_command(&args, None, agents))
            }
        }
        "/code" => {
            if args.is_empty() {
                Some("Error: /code requires a task description. Usage: /code <task>".to_string())
            } else {
                Some(format_code_command(&args, None, agents))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slash_command_parsing() {
        // Test /plan command
        let result = handle_slash_command("/plan implement a new feature", None);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Create a comprehensive plan"));

        // Test /solve command
        let result = handle_slash_command("/solve fix the bug in authentication", None);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Solve a complicated problem"));

        // Test /code command
        let result = handle_slash_command("/code refactor the database module", None);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Perform a coding task"));

        // Test invalid command
        let result = handle_slash_command("/invalid test", None);
        assert!(result.is_none());

        // Test non-slash command
        let result = handle_slash_command("regular message", None);
        assert!(result.is_none());

        // Test empty arguments
        let result = handle_slash_command("/plan", None);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Error"));
    }

    #[test]
    fn test_slash_commands_with_agents() {
        // Create test agent configurations
        let agents = vec![
            AgentConfig {
                name: "test-claude".to_string(),
                command: "claude".to_string(),
                args: vec![],
                read_only: false,
                enabled: true,
                description: None,
                env: None,
            },
            AgentConfig {
                name: "test-gemini".to_string(),
                command: "gemini".to_string(),
                args: vec![],
                read_only: false,
                enabled: false, // disabled
                description: None,
                env: None,
            },
        ];

        // Test that only enabled agents are included
        let result = handle_slash_command("/plan test task", Some(&agents));
        assert!(result.is_some());
        let prompt = result.unwrap();
        assert!(prompt.contains("test-claude"));
        assert!(!prompt.contains("test-gemini")); // Should not include disabled agent
    }
}
