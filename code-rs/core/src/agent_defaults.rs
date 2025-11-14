//! Defaults for agent model slugs and their CLI launch configuration.
//!
//! The canonical catalog defined here is consumed by both the core executor
//! (to assemble argv when the user has not overridden a model) and by the TUI
//! (to surface the available sub-agent options).

use crate::config_types::AgentConfig;
use std::collections::{HashMap, HashSet};

const CLAUDE_ALLOWED_TOOLS: &str = "Bash(ls:*), Bash(cat:*), Bash(grep:*), Bash(git status:*), Bash(git log:*), Bash(find:*), Read, Grep, Glob, LS, WebFetch, TodoRead, TodoWrite, WebSearch";
const CLOUD_MODEL_ENV_FLAG: &str = "CODE_ENABLE_CLOUD_AGENT_MODEL";

const CODE_GPT5_CODEX_READ_ONLY: &[&str] = &["-s", "read-only", "-a", "never", "exec", "--skip-git-repo-check"];
const CODE_GPT5_CODEX_WRITE: &[&str] = &["-s", "workspace-write", "--dangerously-bypass-approvals-and-sandbox", "exec", "--skip-git-repo-check"];
const CODE_GPT5_READ_ONLY: &[&str] = &["-s", "read-only", "-a", "never", "exec", "--skip-git-repo-check"];
const CODE_GPT5_WRITE: &[&str] = &["-s", "workspace-write", "--dangerously-bypass-approvals-and-sandbox", "exec", "--skip-git-repo-check"];
const CLAUDE_SONNET_READ_ONLY: &[&str] = &["--allowedTools", CLAUDE_ALLOWED_TOOLS];
const CLAUDE_SONNET_WRITE: &[&str] = &["--dangerously-skip-permissions"];
const CLAUDE_OPUS_READ_ONLY: &[&str] = &["--allowedTools", CLAUDE_ALLOWED_TOOLS];
const CLAUDE_OPUS_WRITE: &[&str] = &["--dangerously-skip-permissions"];
const CLAUDE_HAIKU_READ_ONLY: &[&str] = &["--allowedTools", CLAUDE_ALLOWED_TOOLS];
const CLAUDE_HAIKU_WRITE: &[&str] = &["--dangerously-skip-permissions"];
const GEMINI_PRO_READ_ONLY: &[&str] = &[];
const GEMINI_PRO_WRITE: &[&str] = &["-y"];
const GEMINI_FLASH_READ_ONLY: &[&str] = &[];
const GEMINI_FLASH_WRITE: &[&str] = &["-y"];
const QWEN_3_CODER_READ_ONLY: &[&str] = &[];
const QWEN_3_CODER_WRITE: &[&str] = &["-y"];
const CLOUD_GPT5_CODEX_READ_ONLY: &[&str] = &[];
const CLOUD_GPT5_CODEX_WRITE: &[&str] = &[];

/// Canonical list of built-in agent model slugs used when no `[[agents]]`
/// entries are configured. The ordering here controls priority for legacy
/// CLI-name lookups.
pub const DEFAULT_AGENT_NAMES: &[&str] = &[
    "code-gpt-5.1-codex-mini",
    "code-gpt-5.1-codex",
    "code-gpt-5.1",
    "claude-sonnet-4.5",
    "claude-opus-4.1",
    "claude-haiku-4.5",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
    "qwen-3-coder",
    "cloud-gpt-5.1-codex",
];

#[derive(Debug, Clone)]
pub struct AgentModelSpec {
    pub slug: &'static str,
    pub family: &'static str,
    pub cli: &'static str,
    pub read_only_args: &'static [&'static str],
    pub write_args: &'static [&'static str],
    pub model_args: &'static [&'static str],
    pub description: &'static str,
    pub enabled_by_default: bool,
    pub aliases: &'static [&'static str],
    pub gating_env: Option<&'static str>,
}

impl AgentModelSpec {
    pub fn is_enabled(&self) -> bool {
        if self.enabled_by_default {
            return true;
        }
        if let Some(env) = self.gating_env {
            if let Ok(value) = std::env::var(env) {
                return matches!(value.as_str(), "1" | "true" | "TRUE" | "True");
            }
        }
        false
    }

    pub fn default_args(&self, read_only: bool) -> &'static [&'static str] {
        if read_only {
            self.read_only_args
        } else {
            self.write_args
        }
    }
}

const AGENT_MODEL_SPECS: &[AgentModelSpec] = &[
    AgentModelSpec {
        slug: "code-gpt-5.1-codex-mini",
        family: "code",
        cli: "coder",
        read_only_args: CODE_GPT5_CODEX_READ_ONLY,
        write_args: CODE_GPT5_CODEX_WRITE,
        model_args: &["--model", "gpt-5.1-codex-mini"],
        description: "Your primary coding agent (along with claude-sonnet-4.5). Cheap (1/4 cost), fast and very capable. Excels at implementation, refactors, multi-file edits and code review.",
        enabled_by_default: true,
        aliases: &["code-gpt-5-codex-mini", "codex-mini", "coder-mini"],
        gating_env: None,
    },
    AgentModelSpec {
        slug: "code-gpt-5.1-codex",
        family: "code",
        cli: "coder",
        read_only_args: CODE_GPT5_CODEX_READ_ONLY,
        write_args: CODE_GPT5_CODEX_WRITE,
        model_args: &["--model", "gpt-5.1-codex"],
        description: "Backup for complex coding tasks (along with claude-opus-4.1). Slower and more expensive, but slightly more capable if code-gpt-5.1-codex-mini did not succeed.",
        enabled_by_default: true,
        aliases: &["code-gpt-5-codex", "coder", "code", "codex"],
        gating_env: None,
    },
    AgentModelSpec {
        slug: "code-gpt-5.1",
        family: "code",
        cli: "coder",
        read_only_args: CODE_GPT5_READ_ONLY,
        write_args: CODE_GPT5_WRITE,
        model_args: &["--model", "gpt-5.1"],
        description: "Use for UI/UX or mixed tasks where explanation, design judgment, or multi-domain reasoning is equally important as code.",
        enabled_by_default: true,
        aliases: &["code-gpt-5", "coder-gpt-5"],
        gating_env: None,
    },
    AgentModelSpec {
        slug: "claude-sonnet-4.5",
        family: "claude",
        cli: "claude",
        read_only_args: CLAUDE_SONNET_READ_ONLY,
        write_args: CLAUDE_SONNET_WRITE,
        model_args: &["--model", "sonnet"],
        description: "Front line for coding tasks (along with code-gpt-5.1-codex-mini). Excels at implementation, tool use, debugging, and testing.",
        enabled_by_default: true,
        aliases: &["claude", "claude-sonnet"],
        gating_env: None,
    },
    AgentModelSpec {
        slug: "claude-opus-4.1",
        family: "claude",
        cli: "claude",
        read_only_args: CLAUDE_OPUS_READ_ONLY,
        write_args: CLAUDE_OPUS_WRITE,
        model_args: &["--model", "opus"],
        description: "Backup for complex coding tasks (along with code-gpt-5.1-codex). Slower and more expensive, but slightly more capable if claude-sonnet-4.5 did not succeed.",
        enabled_by_default: true,
        aliases: &["claude-opus"],
        gating_env: None,
    },
    AgentModelSpec {
        slug: "claude-haiku-4.5",
        family: "claude",
        cli: "claude",
        read_only_args: CLAUDE_HAIKU_READ_ONLY,
        write_args: CLAUDE_HAIKU_WRITE,
        model_args: &["--model", "haiku"],
        description: "Very fast model for simple tasks. Similar to gemini-2.5-flash in capability.",
        enabled_by_default: true,
        aliases: &["claude-haiku"],
        gating_env: None,
    },
    AgentModelSpec {
        slug: "gemini-2.5-pro",
        family: "gemini",
        cli: "gemini",
        read_only_args: GEMINI_PRO_READ_ONLY,
        write_args: GEMINI_PRO_WRITE,
        model_args: &["--model", "gemini-2.5-pro"],
        description: "Use when you require huge context or multimodal grounding (repo-scale inputs, or search grounding); good for alternative architecture opinions.",
        enabled_by_default: true,
        aliases: &["gemini"],
        gating_env: None,
    },
    AgentModelSpec {
        slug: "gemini-2.5-flash",
        family: "gemini",
        cli: "gemini",
        read_only_args: GEMINI_FLASH_READ_ONLY,
        write_args: GEMINI_FLASH_WRITE,
        model_args: &["--model", "gemini-2.5-flash"],
        description: "Use for fast, high-volume scaffolding, creating minimal repros/tests, or budget-sensitive operations.",
        enabled_by_default: true,
        aliases: &["gemini-flash"],
        gating_env: None,
    },
    AgentModelSpec {
        slug: "qwen-3-coder",
        family: "qwen",
        cli: "qwen",
        read_only_args: QWEN_3_CODER_READ_ONLY,
        write_args: QWEN_3_CODER_WRITE,
        model_args: &["-m", "qwen-3-coder"],
        description: "Fast and reasonably effective. Good for providing an alternative opinion as it has quite different training data to other models.",
        enabled_by_default: true,
        aliases: &["qwen", "qwen3"],
        gating_env: None,
    },
    AgentModelSpec {
        slug: "cloud-gpt-5.1-codex",
        family: "cloud",
        cli: "cloud",
        read_only_args: CLOUD_GPT5_CODEX_READ_ONLY,
        write_args: CLOUD_GPT5_CODEX_WRITE,
        model_args: &["--model", "gpt-5.1-codex"],
        description: "Cloud-hosted gpt-5.1-codex agent. Requires the CODE_ENABLE_CLOUD_AGENT_MODEL flag and carries the latency of a remote run.",
        enabled_by_default: false,
        aliases: &["cloud-gpt-5-codex", "cloud"],
        gating_env: Some(CLOUD_MODEL_ENV_FLAG),
    },
];

pub fn agent_model_specs() -> &'static [AgentModelSpec] {
    AGENT_MODEL_SPECS
}

pub fn enabled_agent_model_specs() -> Vec<&'static AgentModelSpec> {
    AGENT_MODEL_SPECS
        .iter()
        .filter(|spec| spec.is_enabled())
        .collect()
}

pub fn agent_model_spec(identifier: &str) -> Option<&'static AgentModelSpec> {
    let lower = identifier.to_ascii_lowercase();
    AGENT_MODEL_SPECS
        .iter()
        .find(|spec| spec.slug.eq_ignore_ascii_case(&lower))
        .or_else(|| {
            AGENT_MODEL_SPECS.iter().find(|spec| {
                spec.aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(&lower))
            })
        })
}

const MODEL_GUIDE_INTRO: &str =
    "Preferred agent models for this helper (choose from the valid agent list). Selection guide:";

fn model_guide_line(spec: &AgentModelSpec) -> String {
    format!("- `{}`: {}", spec.slug, spec.description)
}

fn custom_model_guide_line(name: &str, description: &str) -> String {
    format!("- `{}`: {}", name, description)
}

pub fn build_model_guide_description(active_agents: &[String]) -> String {
    let mut description = String::from(MODEL_GUIDE_INTRO);

    let mut canonical: HashSet<String> = HashSet::new();
    for name in active_agents {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(spec) = agent_model_spec(trimmed) {
            canonical.insert(spec.slug.to_ascii_lowercase());
        } else {
            canonical.insert(trimmed.to_ascii_lowercase());
        }
    }

    let lines: Vec<String> = AGENT_MODEL_SPECS
        .iter()
        .filter(|spec| canonical.contains(&spec.slug.to_ascii_lowercase()))
        .map(model_guide_line)
        .collect();

    if lines.is_empty() {
        description.push('\n');
        description.push_str("- No model guides available for the current configuration.");
    } else {
        for line in lines {
            description.push('\n');
            description.push_str(&line);
        }
    }

    description
}

pub fn model_guide_markdown() -> String {
    AGENT_MODEL_SPECS
        .iter()
        .filter(|spec| spec.is_enabled())
        .map(model_guide_line)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn model_guide_markdown_with_custom(configured_agents: &[AgentConfig]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();

    for spec in AGENT_MODEL_SPECS.iter().filter(|spec| spec.is_enabled()) {
        let idx = lines.len();
        positions.insert(spec.slug.to_ascii_lowercase(), idx);
        lines.push(model_guide_line(spec));
    }

    let mut saw_custom = false;
    for agent in configured_agents {
        if !agent.enabled {
            continue;
        }
        let Some(description) = agent.description.as_deref() else { continue };
        let trimmed = description.trim();
        if trimmed.is_empty() {
            continue;
        }
        let slug = agent.name.trim();
        if slug.is_empty() {
            continue;
        }
        saw_custom = true;
        let line = custom_model_guide_line(slug, trimmed);
        let key = slug.to_ascii_lowercase();
        if let Some(idx) = positions.get(&key).copied() {
            lines[idx] = line;
        } else {
            positions.insert(key, lines.len());
            lines.push(line);
        }
    }

    if saw_custom {
        Some(lines.join("\n"))
    } else {
        None
    }
}

pub fn default_agent_configs() -> Vec<AgentConfig> {
    enabled_agent_model_specs()
        .into_iter()
        .map(|spec| agent_config_from_spec(spec))
        .collect()
}

pub fn agent_config_from_spec(spec: &AgentModelSpec) -> AgentConfig {
    AgentConfig {
        name: spec.slug.to_string(),
        command: spec.cli.to_string(),
        args: Vec::new(),
        read_only: false,
        enabled: spec.is_enabled(),
        description: None,
        env: None,
        args_read_only: some_args(spec.read_only_args),
        args_write: some_args(spec.write_args),
        instructions: None,
    }
}

fn some_args(args: &[&str]) -> Option<Vec<String>> {
    if args.is_empty() {
        None
    } else {
        Some(args.iter().map(|arg| (*arg).to_string()).collect())
    }
}

/// Return default CLI arguments (excluding the prompt flag) for a given agent
/// identifier and access mode.
///
/// The identifier can be either the canonical slug or a legacy CLI alias
/// (`code`, `claude`, etc.) used prior to the model slug transition.
pub fn default_params_for(name: &str, read_only: bool) -> Vec<String> {
    if let Some(spec) = agent_model_spec(name) {
        return spec
            .default_args(read_only)
            .iter()
            .map(|arg| (*arg).to_string())
            .collect();
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_defaults_are_empty_both_modes() {
        assert!(default_params_for("cloud", true).is_empty());
        assert!(default_params_for("cloud", false).is_empty());
    }
}
