//! Defaults for agent model slugs and their CLI launch configuration.
//!
//! The canonical catalog defined here is consumed by both the core executor
//! (to assemble argv when the user has not overridden a model) and by the TUI
//! (to surface the available sub-agent options).

use crate::config_types::AgentConfig;
use code_app_server_protocol::AuthMode;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

const CLAUDE_ALLOWED_TOOLS: &str = "Bash(ls:*), Bash(cat:*), Bash(grep:*), Bash(git status:*), Bash(git log:*), Bash(find:*), Read, Grep, Glob, LS, WebFetch, TodoRead, TodoWrite, WebSearch";
const CLOUD_MODEL_ENV_FLAG: &str = "CODE_ENABLE_CLOUD_AGENT_MODEL";

const CODE_GPT5_CODEX_READ_ONLY: &[&str] = &["-s", "read-only", "exec", "--skip-git-repo-check"];
const CODE_GPT5_CODEX_WRITE: &[&str] = &["-s", "workspace-write", "--dangerously-bypass-approvals-and-sandbox", "exec", "--skip-git-repo-check"];
const CODE_GPT5_READ_ONLY: &[&str] = &["-s", "read-only", "exec", "--skip-git-repo-check"];
const CODE_GPT5_WRITE: &[&str] = &["-s", "workspace-write", "--dangerously-bypass-approvals-and-sandbox", "exec", "--skip-git-repo-check"];
const CLAUDE_SONNET_READ_ONLY: &[&str] = &["--allowedTools", CLAUDE_ALLOWED_TOOLS];
const CLAUDE_SONNET_WRITE: &[&str] = &["--dangerously-skip-permissions"];
const CLAUDE_OPUS_READ_ONLY: &[&str] = &["--allowedTools", CLAUDE_ALLOWED_TOOLS];
const CLAUDE_OPUS_WRITE: &[&str] = &["--dangerously-skip-permissions"];
const CLAUDE_HAIKU_READ_ONLY: &[&str] = &["--allowedTools", CLAUDE_ALLOWED_TOOLS];
const CLAUDE_HAIKU_WRITE: &[&str] = &["--dangerously-skip-permissions"];
const GEMINI_PRO_READ_ONLY: &[&str] = &[];
const GEMINI_PRO_WRITE: &[&str] = &["-y"];
const ANTIGRAVITY_FLASH_READ_ONLY: &[&str] = &[];
const ANTIGRAVITY_FLASH_WRITE: &[&str] = &["--sandbox=false", "--dangerously-skip-permissions"];
const COPILOT_READ_ONLY: &[&str] = &["--autopilot", "--allow-all-tools", "--no-ask-user", "-s"];
const COPILOT_WRITE: &[&str] = &["--autopilot", "--yolo", "--no-ask-user", "-s"];
const QWEN_3_CODER_READ_ONLY: &[&str] = &[];
const QWEN_3_CODER_WRITE: &[&str] = &["-y"];
const CLOUD_GPT5_CODEX_READ_ONLY: &[&str] = &[];
const CLOUD_GPT5_CODEX_WRITE: &[&str] = &[];
const MODELS_MANIFEST: &str = include_str!("../../../codex-rs/models-manager/models.json");

/// Canonical list of built-in agent model slugs used when no `[[agents]]`
/// entries are configured. The ordering here controls priority for legacy
/// CLI-name lookups.
pub const DEFAULT_AGENT_NAMES: &[&str] = &[
    // Frontline for moderate/challenging tasks
    "code-gpt-5.4",
    "code-gpt-5.4-mini",
    "code-gpt-5.3-codex",
    "code-gpt-5.3-codex-spark",
    "claude-opus-4.8",
    "gemini-3.1-pro",
    // Straightforward / cost-aware
    "claude-sonnet-4.5",
    "gemini-3.5-flash",
    "github-copilot",
    // Mixed/general and alternates
    "claude-haiku-4.5",
    "qwen-3-coder",
    "cloud-gpt-5.1-codex-max",
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
    pub is_frontline: bool,
    pub pro_only: bool,
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
        slug: "code-gpt-5.4",
        family: "code",
        cli: "coder",
        read_only_args: CODE_GPT5_READ_ONLY,
        write_args: CODE_GPT5_WRITE,
        model_args: &["--model", "gpt-5.4"],
        description: "Highest-capacity GPT option for tricky reasoning; use when correctness matters most.",
        enabled_by_default: true,
        aliases: &[
            "code-gpt-5.2",
            "gpt-5.4",
            "gpt-5.2",
            "code-gpt-5.1",
            "code-gpt-5",
            "gpt-5.1",
            "gpt-5",
            "coder-gpt-5",
        ],
        gating_env: None,
        is_frontline: true,
        pro_only: false,
    },
    AgentModelSpec {
        slug: "code-gpt-5.4-mini",
        family: "code",
        cli: "coder",
        read_only_args: CODE_GPT5_CODEX_READ_ONLY,
        write_args: CODE_GPT5_CODEX_WRITE,
        model_args: &["--model", "gpt-5.4-mini"],
        description: "Budget coding agent for small changes and quick refactors; use when speed and cost matter.",
        enabled_by_default: true,
        aliases: &[
            "gpt-5.4-mini",
            "code-gpt-5.1-codex-mini",
            "code-gpt-5-codex-mini",
            "gpt-5.1-codex-mini",
            "gpt-5-codex-mini",
            "codex-mini",
            "coder-mini",
        ],
        gating_env: None,
        is_frontline: false,
        pro_only: false,
    },
    AgentModelSpec {
        slug: "code-gpt-5.3-codex",
        family: "code",
        cli: "coder",
        read_only_args: CODE_GPT5_CODEX_READ_ONLY,
        write_args: CODE_GPT5_CODEX_WRITE,
        model_args: &["--model", "gpt-5.3-codex"],
        description: "Primary coding agent for implementation and multi-file edits; strong speed and reliability.",
        enabled_by_default: true,
        aliases: &[
            "code-gpt-5.2-codex",
            "code-gpt-5.1-codex-max",
            "code-gpt-5.1-codex",
            "code-gpt-5-codex",
            "gpt-5.3-codex",
            "gpt-5.2-codex",
            "gpt-5.1-codex-max",
            "gpt-5.1-codex",
            "gpt-5-codex",
            "coder",
            "code",
            "codex",
        ],
        gating_env: None,
        is_frontline: true,
        pro_only: false,
    },
    AgentModelSpec {
        slug: "code-gpt-5.3-codex-spark",
        family: "code",
        cli: "coder",
        read_only_args: CODE_GPT5_CODEX_READ_ONLY,
        write_args: CODE_GPT5_CODEX_WRITE,
        model_args: &["--model", "gpt-5.3-codex-spark"],
        description: "Fast codex variant tuned for responsive coding loops and smaller edits.",
        enabled_by_default: true,
        aliases: &["gpt-5.3-codex-spark", "code-gpt-5.3-spark", "codex-spark"],
        gating_env: None,
        is_frontline: false,
        pro_only: true,
    },
    AgentModelSpec {
        slug: "claude-opus-4.8",
        family: "claude",
        cli: "claude",
        read_only_args: CLAUDE_OPUS_READ_ONLY,
        write_args: CLAUDE_OPUS_WRITE,
        model_args: &["--model", "opus"],
        description: "Higher-capacity Claude model for complex reasoning; use when you want the strongest Claude.",
        enabled_by_default: true,
        aliases: &[
            "claude-opus",
            "claude-opus-4.1",
            "claude-opus-4.5",
            "claude-opus-4.6",
            "claude-opus-4.7",
        ],
        gating_env: None,
        is_frontline: true,
        pro_only: false,
    },
    AgentModelSpec {
        slug: "claude-sonnet-4.5",
        family: "claude",
        cli: "claude",
        read_only_args: CLAUDE_SONNET_READ_ONLY,
        write_args: CLAUDE_SONNET_WRITE,
        model_args: &["--model", "sonnet"],
        description: "Balanced Claude model for implementation and debugging; a solid default when you want Claude.",
        enabled_by_default: true,
        aliases: &["claude", "claude-sonnet"],
        gating_env: None,
        is_frontline: false,
        pro_only: false,
    },
    AgentModelSpec {
        slug: "claude-haiku-4.5",
        family: "claude",
        cli: "claude",
        read_only_args: CLAUDE_HAIKU_READ_ONLY,
        write_args: CLAUDE_HAIKU_WRITE,
        model_args: &["--model", "haiku"],
        description: "Fast Claude model for simple tasks, drafts, and quick iterations; pick when latency matters.",
        enabled_by_default: true,
        aliases: &["claude-haiku"],
        gating_env: None,
        is_frontline: false,
        pro_only: false,
    },
    AgentModelSpec {
        slug: "gemini-3.1-pro",
        family: "gemini",
        cli: "gemini",
        read_only_args: GEMINI_PRO_READ_ONLY,
        write_args: GEMINI_PRO_WRITE,
        model_args: &["--model", "gemini-3.1-pro-preview"],
        description: "Higher-capacity Gemini CLI model for harder tasks; use when Gemini Flash misses details.",
        enabled_by_default: true,
        aliases: &[
            "gemini-3-pro",
            "gemini-3-pro-preview",
            "gemini-3",
            "gemini3",
            "gemini-pro",
            "gemini-2.5-pro",
        ],
        gating_env: None,
        is_frontline: true,
        pro_only: false,
    },
    AgentModelSpec {
        slug: "gemini-3.5-flash",
        family: "antigravity",
        cli: "agy",
        read_only_args: ANTIGRAVITY_FLASH_READ_ONLY,
        write_args: ANTIGRAVITY_FLASH_WRITE,
        model_args: &[],
        description: "Antigravity CLI default powered by Gemini 3.5 Flash; fast forward path for Google agents.",
        enabled_by_default: true,
        aliases: &[
            "gemini",
            "gemini-flash",
            "gemini-3-flash",
            "gemini-3-flash-preview",
            "gemini-2.5-flash",
            "antigravity",
            "agy",
        ],
        gating_env: None,
        is_frontline: false,
        pro_only: false,
    },
    AgentModelSpec {
        slug: "github-copilot",
        family: "copilot",
        cli: "copilot",
        read_only_args: COPILOT_READ_ONLY,
        write_args: COPILOT_WRITE,
        model_args: &[],
        description: "GitHub Copilot CLI agent; uses your signed-in Copilot account and configured default model.",
        enabled_by_default: true,
        aliases: &["copilot", "github-copilot-cli"],
        gating_env: None,
        is_frontline: false,
        pro_only: false,
    },
    AgentModelSpec {
        slug: "qwen-3-coder",
        family: "qwen",
        cli: "qwen",
        read_only_args: QWEN_3_CODER_READ_ONLY,
        write_args: QWEN_3_CODER_WRITE,
        model_args: &["-m", "qwen3-coder-plus"],
        description: "Fast and capable alternative; useful as a second opinion or for cross-checking.",
        enabled_by_default: true,
        aliases: &["qwen", "qwen3"],
        gating_env: None,
        is_frontline: false,
        pro_only: false,
    },
    AgentModelSpec {
        slug: "cloud-gpt-5.1-codex-max",
        family: "cloud",
        cli: "cloud",
        read_only_args: CLOUD_GPT5_CODEX_READ_ONLY,
        write_args: CLOUD_GPT5_CODEX_WRITE,
        model_args: &["--model", "gpt-5.1-codex-max"],
        description: "Cloud-hosted gpt-5.1-codex-max agent; use for remote runs when enabled via CODE_ENABLE_CLOUD_AGENT_MODEL.",
        enabled_by_default: false,
        aliases: &["cloud-gpt-5.1-codex", "cloud-gpt-5-codex", "cloud"],
        gating_env: Some(CLOUD_MODEL_ENV_FLAG),
        is_frontline: false,
        pro_only: false,
    },
];

static ALL_AGENT_MODEL_SPECS: LazyLock<Vec<AgentModelSpec>> =
    LazyLock::new(build_agent_model_specs);

#[derive(Debug, Deserialize)]
struct ModelsManifest {
    models: Vec<ManifestModel>,
}

#[derive(Debug, Deserialize)]
struct ManifestModel {
    slug: String,
    display_name: String,
    description: String,
    visibility: String,
    supported_in_api: bool,
}

fn build_agent_model_specs() -> Vec<AgentModelSpec> {
    let mut specs = AGENT_MODEL_SPECS.to_vec();
    specs.extend(dynamic_code_agent_specs());
    specs
}

fn dynamic_code_agent_specs() -> Vec<AgentModelSpec> {
    let Ok(manifest) = serde_json::from_str::<ModelsManifest>(MODELS_MANIFEST) else {
        return Vec::new();
    };

    manifest
        .models
        .into_iter()
        .filter(|model| model.supported_in_api)
        .filter(|model| model.visibility.eq_ignore_ascii_case("list"))
        .filter_map(|model| dynamic_code_agent_spec(model))
        .collect()
}

fn dynamic_code_agent_spec(model: ManifestModel) -> Option<AgentModelSpec> {
    let track = code_agent_track(&model.slug)?;
    if static_agent_model_spec(&model.slug).is_some() {
        return None;
    }

    let candidate_version = parse_model_version_components(&model.slug)?;
    let highest_static_version = highest_static_code_track_version(track)?;
    if candidate_version <= highest_static_version {
        return None;
    }

    let slug = leak_str(format!("code-{}", model.slug));
    let model_slug = leak_str(model.slug);
    let description = leak_str(model.description);
    let _display_name = leak_str(model.display_name);
    let aliases = leak_str_slice(vec![model_slug]);
    let model_args = leak_str_slice(vec!["--model", model_slug]);
    let pro_only = matches!(track, CodeAgentTrack::CodexSpark);
    let is_frontline = !matches!(track, CodeAgentTrack::Mini | CodeAgentTrack::CodexSpark);

    Some(AgentModelSpec {
        slug,
        family: "code",
        cli: "coder",
        read_only_args: CODE_GPT5_READ_ONLY,
        write_args: CODE_GPT5_WRITE,
        model_args,
        description,
        enabled_by_default: true,
        aliases,
        gating_env: None,
        is_frontline,
        pro_only,
    })
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum CodeAgentTrack {
    Base,
    Mini,
    Codex,
    CodexSpark,
}

fn code_agent_track(model: &str) -> Option<CodeAgentTrack> {
    let canonical = model.strip_prefix("code-").unwrap_or(model);
    if !canonical.starts_with("gpt-") {
        return None;
    }

    if canonical.contains("codex-spark") {
        Some(CodeAgentTrack::CodexSpark)
    } else if canonical.contains("codex") {
        Some(CodeAgentTrack::Codex)
    } else if canonical.ends_with("-mini") {
        Some(CodeAgentTrack::Mini)
    } else {
        Some(CodeAgentTrack::Base)
    }
}

fn highest_static_code_track_version(track: CodeAgentTrack) -> Option<Vec<u32>> {
    AGENT_MODEL_SPECS
        .iter()
        .filter(|spec| spec.family == "code")
        .filter(|spec| code_agent_track(spec.slug) == Some(track))
        .filter_map(|spec| parse_model_version_components(spec.slug))
        .max()
}

fn parse_model_version_components(model: &str) -> Option<Vec<u32>> {
    let canonical = model
        .strip_prefix("code-")
        .unwrap_or(model)
        .rsplit('/')
        .next()
        .unwrap_or(model);
    let mut components = Vec::new();

    for segment in canonical.split('-') {
        let first = segment.chars().next()?;
        if !first.is_ascii_digit() {
            continue;
        }

        for part in segment.split('.') {
            if part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            components.push(part.parse().ok()?);
        }

        return (!components.is_empty()).then_some(components);
    }

    None
}

fn leak_str(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

fn leak_str_slice(values: Vec<&'static str>) -> &'static [&'static str] {
    Box::leak(values.into_boxed_slice())
}

fn static_agent_model_spec(identifier: &str) -> Option<&'static AgentModelSpec> {
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

pub fn agent_model_specs() -> &'static [AgentModelSpec] {
    ALL_AGENT_MODEL_SPECS.as_slice()
}

pub fn enabled_agent_model_specs() -> Vec<&'static AgentModelSpec> {
    agent_model_specs()
        .iter()
        .filter(|spec| spec.is_enabled())
        .collect()
}

pub fn agent_model_available_for_auth(
    spec: &AgentModelSpec,
    auth_mode: Option<AuthMode>,
    supports_pro_only_models: bool,
) -> bool {
    let is_chatgpt_auth = auth_mode.is_some_and(AuthMode::is_chatgpt);
    !spec.pro_only || (is_chatgpt_auth && supports_pro_only_models)
}

pub fn enabled_agent_model_specs_for_auth(
    auth_mode: Option<AuthMode>,
    supports_pro_only_models: bool,
) -> Vec<&'static AgentModelSpec> {
    agent_model_specs()
        .iter()
        .filter(|spec| spec.is_enabled())
        .filter(|spec| agent_model_available_for_auth(spec, auth_mode, supports_pro_only_models))
        .collect()
}

pub fn filter_agent_model_names_for_auth(
    model_names: Vec<String>,
    auth_mode: Option<AuthMode>,
    supports_pro_only_models: bool,
) -> Vec<String> {
    model_names
        .into_iter()
        .filter(|name| {
            if let Some(spec) = agent_model_spec(name) {
                return agent_model_available_for_auth(spec, auth_mode, supports_pro_only_models);
            }
            true
        })
        .collect()
}

pub fn agent_model_spec(identifier: &str) -> Option<&'static AgentModelSpec> {
    let lower = identifier.to_ascii_lowercase();
    agent_model_specs()
        .iter()
        .find(|spec| spec.slug.eq_ignore_ascii_case(&lower))
        .or_else(|| {
            agent_model_specs().iter().find(|spec| {
                spec.aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(&lower))
            })
        })
}

fn model_guide_intro(active_agents: &[String]) -> String {
    let mut present_frontline: Vec<String> = active_agents
        .iter()
        .filter_map(|id| {
            agent_model_spec(id)
                .filter(|spec| spec.is_frontline)
                .map(|spec| spec.slug.to_string())
        })
        .collect();

    if present_frontline.is_empty() {
        present_frontline.push("code-gpt-5.4".to_string());
    }
    let frontline_str = present_frontline.join(", ");

    format!("Preferred agent models: use {frontline_str} for challenging coding/agentic work.")
}

fn model_guide_line(spec: &AgentModelSpec) -> String {
    format!("- `{}`: {}", spec.slug, spec.description)
}

fn custom_model_guide_line(name: &str, description: &str) -> String {
    format!("- `{}`: {}", name, description)
}

pub fn build_model_guide_description(active_agents: &[String]) -> String {
    let mut description = model_guide_intro(active_agents);

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

    let lines: Vec<String> = agent_model_specs()
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
    agent_model_specs()
        .iter()
        .filter(|spec| spec.is_enabled())
        .map(model_guide_line)
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn model_guide_markdown_with_custom(configured_agents: &[AgentConfig]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();

    for spec in agent_model_specs().iter().filter(|spec| spec.is_enabled()) {
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

    #[test]
    fn github_copilot_defaults_match_cli_contract() {
        assert_eq!(
            default_params_for("github-copilot", true),
            vec!["--autopilot", "--allow-all-tools", "--no-ask-user", "-s"]
        );
        assert_eq!(
            default_params_for("github-copilot", false),
            vec!["--autopilot", "--yolo", "--no-ask-user", "-s"]
        );

        let spec = agent_model_spec("copilot").expect("copilot alias should resolve");
        assert_eq!(spec.slug, "github-copilot");
    }

    #[test]
    fn gpt_codex_aliases_resolve() {
        let codex = agent_model_spec("gpt-5.1-codex").expect("alias for codex present");
        assert_eq!(codex.slug, "code-gpt-5.3-codex");

        let codex_direct = agent_model_spec("gpt-5.1-codex-max").expect("codex present");
        assert_eq!(codex_direct.slug, "code-gpt-5.3-codex");

        let codex_upgrade = agent_model_spec("gpt-5.2-codex").expect("upgrade alias present");
        assert_eq!(codex_upgrade.slug, "code-gpt-5.3-codex");

        let codex_slug_upgrade =
            agent_model_spec("code-gpt-5.2-codex").expect("slug upgrade alias present");
        assert_eq!(codex_slug_upgrade.slug, "code-gpt-5.3-codex");

        let spark =
            agent_model_spec("gpt-5.3-codex-spark").expect("spark alias for codex present");
        assert_eq!(spark.slug, "code-gpt-5.3-codex-spark");

        let mini = agent_model_spec("gpt-5.1-codex-mini").expect("mini alias present");
        assert_eq!(mini.slug, "code-gpt-5.4-mini");

        let mini_direct = agent_model_spec("gpt-5.4-mini").expect("mini direct alias present");
        assert_eq!(mini_direct.slug, "code-gpt-5.4-mini");

        let mid = agent_model_spec("gpt-5.1").expect("mid alias present");
        assert_eq!(mid.slug, "code-gpt-5.4");

        let mid_upgrade = agent_model_spec("code-gpt-5.2").expect("mid upgrade alias present");
        assert_eq!(mid_upgrade.slug, "code-gpt-5.4");
    }

    #[test]
    fn qwen_uses_dashscope_model_id() {
        let qwen = agent_model_spec("qwen-3-coder").expect("qwen spec present");
        assert_eq!(qwen.model_args, &["-m", "qwen3-coder-plus"]);
    }

    #[test]
    fn refreshed_provider_agent_aliases_resolve_to_current_defaults() {
        let opus = agent_model_spec("claude-opus-4.6").expect("legacy opus alias resolves");
        assert_eq!(opus.slug, "claude-opus-4.8");
        assert_eq!(opus.model_args, &["--model", "opus"]);

        let gemini = agent_model_spec("gemini").expect("gemini alias resolves");
        assert_eq!(gemini.slug, "gemini-3.5-flash");
        assert_eq!(gemini.cli, "agy");
        assert_eq!(gemini.model_args, &[] as &[&str]);
        assert_eq!(
            default_params_for("gemini", false),
            vec!["--sandbox=false", "--dangerously-skip-permissions"]
        );

        let legacy_flash =
            agent_model_spec("gemini-3-flash").expect("legacy gemini flash alias resolves");
        assert_eq!(legacy_flash.slug, "gemini-3.5-flash");

        let pro = agent_model_spec("gemini-3-pro").expect("legacy gemini pro alias resolves");
        assert_eq!(pro.slug, "gemini-3.1-pro");
        assert_eq!(pro.cli, "gemini");
    }

    #[test]
    fn spark_agent_model_requires_pro_chatgpt_auth() {
        let pro_specs = enabled_agent_model_specs_for_auth(Some(AuthMode::Chatgpt), true);
        assert!(
            pro_specs
                .iter()
                .any(|spec| spec.slug == "code-gpt-5.3-codex-spark")
        );

        let non_pro_specs = enabled_agent_model_specs_for_auth(Some(AuthMode::Chatgpt), false);
        assert!(
            non_pro_specs
                .iter()
                .all(|spec| spec.slug != "code-gpt-5.3-codex-spark")
        );

        let api_key_specs = enabled_agent_model_specs_for_auth(Some(AuthMode::ApiKey), false);
        assert!(
            api_key_specs
                .iter()
                .all(|spec| spec.slug != "code-gpt-5.3-codex-spark")
        );
    }

    #[test]
    fn filter_agent_model_names_removes_spark_for_non_pro_auth() {
        let filtered = filter_agent_model_names_for_auth(
            vec![
                "code-gpt-5.3-codex".to_string(),
                "code-gpt-5.3-codex-spark".to_string(),
                "gpt-5.3-codex-spark".to_string(),
            ],
            Some(AuthMode::ApiKey),
            false,
        );

        assert!(filtered.contains(&"code-gpt-5.3-codex".to_string()));
        assert!(!filtered.contains(&"code-gpt-5.3-codex-spark".to_string()));
        assert!(!filtered.contains(&"gpt-5.3-codex-spark".to_string()));
    }

    #[test]
    fn dynamic_agent_specs_include_newer_manifest_models() {
        let spec = agent_model_spec("gpt-5.5").expect("gpt-5.5 spec should be present");
        assert_eq!(spec.slug, "code-gpt-5.5");
        assert_eq!(spec.cli, "coder");
        assert_eq!(
            default_params_for("gpt-5.5", true),
            CODE_GPT5_READ_ONLY
                .iter()
                .map(|arg| (*arg).to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn dynamic_agent_specs_skip_older_manifest_models() {
        let gpt_5_2 = agent_model_spec("gpt-5.2").expect("gpt-5.2 should resolve via upgrade alias");
        assert_eq!(gpt_5_2.slug, "code-gpt-5.4");
        assert!(
            enabled_agent_model_specs()
                .iter()
                .all(|spec| spec.slug != "code-gpt-5.2")
        );
    }
}
