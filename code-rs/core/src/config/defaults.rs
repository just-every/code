use crate::agent_defaults::{agent_model_spec, default_agent_configs};
use crate::config_types::AgentConfig;
use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::sync::{LazyLock, Mutex};

static RESPONSES_ORIGINATOR_OVERRIDE: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));

pub(crate) const fn default_true_local() -> bool {
    true
}

pub fn set_default_originator(originator: &str) -> std::io::Result<()> {
    let mut guard = RESPONSES_ORIGINATOR_OVERRIDE
        .lock()
        .map_err(|_| std::io::Error::new(ErrorKind::Other, "originator override lock poisoned"))?;
    *guard = Some(originator.to_string());
    Ok(())
}

pub(crate) fn default_responses_originator() -> String {
    RESPONSES_ORIGINATOR_OVERRIDE
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .unwrap_or_else(|| super::DEFAULT_RESPONSES_ORIGINATOR_HEADER.to_owned())
}

fn normalize_agent(mut agent: AgentConfig) -> AgentConfig {
    if let Some(spec) = agent_model_spec(&agent.name).or_else(|| agent_model_spec(&agent.command)) {
        agent.name = spec.slug.to_string();
        if agent.command.trim().is_empty() {
            agent.command = spec.cli.to_string();
        }
    } else if agent.command.trim().is_empty() {
        agent.command = agent.name.clone();
    }

    agent
}

fn canonical_agent_key(agent: &AgentConfig) -> String {
    agent_model_spec(&agent.name)
        .or_else(|| agent_model_spec(&agent.command))
        .map(|spec| spec.slug.to_ascii_lowercase())
        .unwrap_or_else(|| agent.name.to_ascii_lowercase())
}

pub(crate) fn merge_with_default_agents(agents: Vec<AgentConfig>) -> Vec<AgentConfig> {
    let mut deduped: Vec<AgentConfig> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();

    for agent in agents.into_iter().map(normalize_agent) {
        let key = canonical_agent_key(&agent);
        if let Some(idx) = index.get(&key).copied() {
            deduped[idx] = agent;
        } else {
            index.insert(key, deduped.len());
            deduped.push(agent);
        }
    }

    if deduped.is_empty() {
        return default_agent_configs();
    }

    let mut seen = HashSet::new();
    for agent in &deduped {
        seen.insert(canonical_agent_key(agent));
    }

    for default_agent in default_agent_configs() {
        let key = canonical_agent_key(&default_agent);
        if !seen.contains(&key) {
            seen.insert(key.clone());
            deduped.push(default_agent);
        }
    }

    deduped
}

pub(crate) fn default_review_model() -> String {
    super::OPENAI_DEFAULT_REVIEW_MODEL.to_string()
}
