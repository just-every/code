use std::collections::HashMap;

use code_core::config_types::AgentConfig;

pub(super) fn agent_summary_counts(config_agents: &[AgentConfig]) -> (usize, usize) {
    let mut enabled = 0usize;
    let mut total = 0usize;

    let mut config_by_name: HashMap<String, &AgentConfig> = HashMap::new();
    for cfg in config_agents {
        config_by_name.insert(cfg.name.to_ascii_lowercase(), cfg);
    }

    for spec in code_core::agent_defaults::agent_model_specs() {
        total += 1;
        let key = spec.slug.to_ascii_lowercase();
        if let Some(cfg) = config_by_name.get(&key) {
            if cfg.enabled {
                enabled += 1;
            }
        }
    }

    for cfg in config_agents {
        let key = cfg.name.to_ascii_lowercase();
        if code_core::agent_defaults::agent_model_spec(&key).is_some() {
            continue;
        }
        total += 1;
        if cfg.enabled {
            enabled += 1;
        }
    }

    (enabled, total)
}

#[cfg(test)]
mod agent_summary_counts_tests {
    use super::agent_summary_counts;
    use code_core::config_types::AgentConfig;

    fn make_agent(name: &str, enabled: bool) -> AgentConfig {
        AgentConfig {
            name: name.to_string(),
            command: name.to_string(),
            args: Vec::new(),
            read_only: false,
            enabled,
            description: None,
            env: None,
            args_read_only: None,
            args_write: None,
            instructions: None,
        }
    }

    #[test]
    fn missing_builtins_default_to_disabled() {
        let agents = vec![
            make_agent("code-gpt-5.2-codex", true),
            make_agent("code-gpt-5.2", true),
        ];

        let (enabled, total) = agent_summary_counts(&agents);
        let builtins = code_core::agent_defaults::agent_model_specs().len();

        assert_eq!(enabled, 2);
        assert_eq!(total, builtins);
    }

    #[test]
    fn custom_agents_are_counted() {
        let agents = vec![
            make_agent("code-gpt-5.2-codex", true),
            make_agent("my-custom-agent", false),
        ];

        let (enabled, total) = agent_summary_counts(&agents);
        let builtins = code_core::agent_defaults::agent_model_specs().len();

        assert_eq!(enabled, 1);
        assert_eq!(total, builtins + 1);
    }
}
