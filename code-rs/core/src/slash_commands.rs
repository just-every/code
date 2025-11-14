use crate::agent_defaults::enabled_agent_model_specs;
use crate::config_types::AgentConfig;
use crate::config_types::SubagentCommandConfig;

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
    enabled_agent_model_specs()
        .into_iter()
        .map(|spec| spec.slug.to_string())
        .collect()
}

/// Resolution result for a subagent command.
#[derive(Debug, Clone, PartialEq)]
pub struct SubagentResolution {
    pub name: String,
    pub read_only: bool,
    pub models: Vec<String>,
    pub orchestrator_instructions: Option<String>,
    pub agent_instructions: Option<String>,
    pub prompt: String,
}

/// Default read_only for built-in subagent commands.
pub fn default_read_only_for(name: &str) -> bool {
    match name {
        "plan" | "solve" => true,
        _ => name != "code",
    }
}

fn resolve_models(explicit: &[String], agents: Option<&[AgentConfig]>) -> Vec<String> {
    if !explicit.is_empty() {
        return explicit.to_vec();
    }
    if let Some(agents) = agents {
        let enabled = get_enabled_agents(agents);
        if !enabled.is_empty() {
            return enabled;
        }
    }
    get_default_models()
}

/// Format a subagent command (built-in or custom) using optional overrides
/// from `[[subagents.commands]]`. When a `plan|solve|code` entry exists, it
/// replaces the built-in defaults for that command.
/// Default multi-line instructions for built-in commands.
/// Returns None for custom subagent names.
pub fn default_instructions_for(name: &str) -> Option<String> {
    match name.to_ascii_lowercase().as_str() {
        "plan" => Some(r#"1. If you do not fully understand the context for the plan, very briefly research the code base. Do not come up with the plan yourself.
2. Start multiple agents working in parallel.
3. Wait for all agents to complete.
4. Analyze every agent's plans and recommendations. Identify common themes and best practices from each agent.
5. Think deeply and synthesize the best elements from each to create a final, comprehensive plan that incorporates the strongest recommendations from all agents.
6. Present the final plan with clear steps and rationale."#.to_string()),
        "solve" => Some(r#"Solve a complicated problem leveraging multiple state-of-the-art agents working in parallel.

1. If you do not fully understand the problem, research it briefly. Do not attempt to solve it yet, just understand what the problem is and what the desired result should be.
2. Provide full context to the agents so they can work on the problem themselves. You do not need to guide them on how to solve the problem - focus on describing the current issue and desired outcome. Allow each agent to come up with it's own path to the solution. If there have been previous attempts at the problem which have not worked, please explain these.
3. Wait for most agents to complete. If a couple of agents complete and one is still working, look at the completed agents first.
4. Go through each possible solution to the problem from each agent. If you're able to test each solution to compare them, you should do so. Utilize short helper scripts to do this.
5. If no solutions work, then start additional agents. You should always try to gather additional debugging information to feed to the agents.
6. Do no stop any agents prematurely - wait until problem is completely solved. Longer running agents may sometimes come up with unique solutions.
7. Once you have a working solution, check all running agents once again - see if there's any new solutions which might be optimal before completing the task."#.to_string()),
        "code" => Some(r#"Complete a coding task using multiple state-of-the-art agents working in parallel.

1. If you do not fully understand the task, research it briefly. Do not attempt to code or solve it, just understand the task in the context of the current code base.
2. Provide full context to the agents so they can work on the task themselves. You do not need to guide them on how to write the code - focus on describing the current task and desired outcome.
3. Start agents with write: true - each agent will work in a separate worktree and can:
- Read and analyze existing code
- Create new files
- Modify existing files
- Execute commands
- Run tests
- Install dependencies
4. Wait for all agents to complete.
5. View each agent's implementation in the worktree for each agent. You may use git to compare changes. Consider the different approaches and solutions
6. Bring the best parts of each solution into your own final implementation
7. If you are not satisfied the solution has been found, start a new round of agents with additional context"#.to_string()),
        _ => None,
    }
}

pub fn format_subagent_command(
    name: &str,
    task: &str,
    agents: Option<&[AgentConfig]>,
    commands: Option<&[SubagentCommandConfig]>,
) -> SubagentResolution {
    let (user_cmd, read_only_default) = {
        let ro = default_read_only_for(name);
        let found = commands
            .and_then(|list| list.iter().find(|c| c.name.eq_ignore_ascii_case(name)))
            .cloned();
        (found, ro)
    };

    let (read_only, models, orch_extra, agent_extra) = match user_cmd {
        Some(cfg) => (
            cfg.read_only, // User-provided read_only (defaults to false unless set)
            resolve_models(&cfg.agents, agents),
            cfg.orchestrator_instructions,
            cfg.agent_instructions,
        ),
        None => (read_only_default, resolve_models(&[], agents), None, None),
    };

    // Compose unified prompt used for all subagent commands (built-ins and custom)
    let models_str = models
        .iter()
        .map(|m| format!("\"{}\"", m))
        .collect::<Vec<_>>()
        .join(", ");

    let instr_text = orch_extra
        .clone()
        .or_else(|| default_instructions_for(name))
        .unwrap_or_default();

    let write_flag = !read_only;
    let prompt = format!(
        "Please perform /{name} using the <tools>, <instructions> and <task> below.\n<tools>\n    To perform /{name} you must use `agent {{\"action\":\"create\",\"create\":{{\"models\":[{models}],\"write\":{write_flag}}}}}` to start a batch of agents.\n    Provide a comprehensive description of the task and context. You may need to briefly research the code base first and to give the agents a head start of where to look. You can include one or two key files but also allow the models to look up the files they need themselves. Using the create action starts all agents at once and returns a `batch_id`.\n\n    Each agent uses a different LLM which allows you to gather diverse results.\n    Monitor progress using `agent {{\"action\":\"wait\",\"wait\":{{\"batch_id\":\"<batch_id>\",\"return_all\":true}}}}` to wait for all agents to complete.\n    If an agent fails or times out, you can ignore it and continue with the other results.\n    Use `agent {{\"action\":\"result\",\"result\":{{\"agent_id\":\"<agent_id>\"}}}}` to fetch detailed output, or inspect the worktree directly when `write` is true.\n</tools>\n<instructions>\n    Instructions for /{name}:\n    {instructions}\n</instructions>\n<task>\n    Task for /{name}:\n    {task}\n</task>",
        name = name,
        models = models_str,
        write_flag = write_flag,
        instructions = instr_text,
        task = task,
    );

    SubagentResolution {
        name: name.to_string(),
        read_only,
        models,
        orchestrator_instructions: orch_extra,
        agent_instructions: agent_extra,
        prompt,
    }
}

/// Format the /plan command into a prompt for the LLM
/// Legacy wrapper retained for compatibility; now delegates to unified formatter.
pub fn format_plan_command(
    task: &str,
    _models: Option<Vec<String>>,
    agents: Option<&[AgentConfig]>,
) -> String {
    let res = format_subagent_command("plan", task, agents, None);
    res.prompt
}

/// Format the /solve command into a prompt for the LLM
/// Legacy wrapper retained for compatibility; now delegates to unified formatter.
pub fn format_solve_command(
    task: &str,
    _models: Option<Vec<String>>,
    agents: Option<&[AgentConfig]>,
) -> String {
    let res = format_subagent_command("solve", task, agents, None);
    res.prompt
}

/// Format the /code command into a prompt for the LLM
/// Legacy wrapper retained for compatibility; now delegates to unified formatter.
pub fn format_code_command(
    task: &str,
    _models: Option<Vec<String>>,
    agents: Option<&[AgentConfig]>,
) -> String {
    let res = format_subagent_command("code", task, agents, None);
    res.prompt
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
        let plan_prompt = result.unwrap();
        assert!(plan_prompt.contains("final, comprehensive plan"));
        // Default agents list should include non-Codex providers when no [[agents]] configured
        assert!(plan_prompt.contains("code-gpt-5.1"));
        assert!(!plan_prompt.contains("cloud-gpt-5.1-codex"));

        // Test /solve command
        let result = handle_slash_command("/solve fix the bug in authentication", None);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Solve a complicated problem"));

        // Test /code command
        let result = handle_slash_command("/code refactor the database module", None);
        assert!(result.is_some());
        let code_prompt = result.unwrap();
        assert!(code_prompt.contains("Complete a coding task"));

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
                name: "claude-sonnet-4.5".to_string(),
                command: "claude".to_string(),
                args: vec![],
                read_only: false,
                enabled: true,
                description: None,
                env: None,
                args_read_only: None,
                args_write: None,
                instructions: None,
            },
            AgentConfig {
                name: "test-gemini".to_string(),
                command: "gemini".to_string(),
                args: vec![],
                read_only: false,
                enabled: false, // disabled
                description: None,
                env: None,
                args_read_only: None,
                args_write: None,
                instructions: None,
            },
        ];

        // Test that only enabled agents are included
        let result = handle_slash_command("/plan test task", Some(&agents));
        assert!(result.is_some());
        let prompt = result.unwrap();
        assert!(prompt.contains("claude-sonnet-4.5"));
        assert!(!prompt.contains("test-gemini"));
    }
}
