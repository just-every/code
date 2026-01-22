use crate::codex::ApprovedCommandPattern;
use crate::protocol::ApprovedCommandMatchKind;
use crate::config_profile::ConfigProfile;
use crate::config_types::AgentConfig;
use std::collections::HashMap;
use crate::config_types::AutoDriveSettings;
use crate::config_types::AllowedCommand;
use crate::config_types::AllowedCommandMatchKind;
use crate::config_types::BrowserConfig;
use crate::config_types::ClientTools;
use crate::config_types::Notice;
use crate::config_types::History;
use crate::config_types::GithubConfig;
use crate::config_types::ValidationConfig;
use crate::config_types::McpServerConfig;
use crate::config_types::Notifications;
use crate::config_types::OtelConfig;
use crate::config_types::OtelConfigToml;
use crate::config_types::OtelExporterKind;
use crate::config_types::ProjectCommandConfig;
use crate::config_types::ProjectHookConfig;
use crate::config_types::SandboxWorkspaceWrite;
use crate::config_types::ShellEnvironmentPolicy;
use crate::config_types::ShellEnvironmentPolicyToml;
use crate::config_types::TextVerbosity;
use crate::config_types::Tui;
use crate::config_types::UriBasedFileOpener;
use crate::config_types::ConfirmGuardConfig;
use crate::config_types::Personality;
use crate::config_types::DEFAULT_OTEL_ENVIRONMENT;
use crate::git_info::resolve_root_git_project_for_trust;
use crate::model_family::ModelFamily;
use crate::model_family::derive_default_model_family;
use crate::model_family::find_family_for_model;
use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::built_in_model_providers;
use crate::reasoning::clamp_reasoning_effort_for_model;
use crate::protocol::AskForApproval;
use crate::protocol::SandboxPolicy;
use crate::config_types::ReasoningEffort;
use crate::config_types::ReasoningSummary;
use crate::project_features::{load_project_commands, ProjectCommand, ProjectHooks};
use code_app_server_protocol::AuthMode;
use code_protocol::config_types::SandboxMode;
use std::time::Instant;
use serde::Deserialize;
use serde::de::{self, Unexpected};
use std::path::Path;
use std::path::PathBuf;
use toml::Value as TomlValue;

mod builder;
mod defaults;
mod sources;
mod validation;

use defaults::{default_responses_originator, default_review_model, default_true_local};

pub use builder::ConfigBuilder;
pub use defaults::set_default_originator;
pub use sources::{
    add_mcp_server,
    add_project_allowed_command,
    find_code_home,
    list_mcp_servers,
    load_config_as_toml,
    load_global_mcp_servers,
    persist_model_selection,
    resolve_code_path_for_read,
    set_auto_drive_settings,
    set_auto_review_model,
    set_auto_review_resolve_model,
    set_cached_terminal_background,
    set_custom_spinner,
    set_custom_theme,
    set_github_actionlint_on_patch,
    set_github_check_on_push,
    set_mcp_server_enabled,
    set_planning_model,
    set_project_access_mode,
    set_project_trusted,
    set_review_model,
    set_review_resolve_model,
    set_tui_alternate_screen,
    set_tui_auto_review_enabled,
    set_tui_notifications,
    set_tui_review_auto_resolve,
    set_tui_spinner_name,
    set_tui_theme_name,
    set_validation_group_enabled,
    set_validation_tool_enabled,
    write_global_mcp_servers,
};

#[allow(deprecated)]
pub use sources::set_tui_auto_drive_settings;

pub use crate::config_constraint::Constrained;
pub use crate::config_constraint::ConstraintError;
pub use crate::config_constraint::ConstraintResult;

pub(crate) use defaults::merge_with_default_agents;
pub(crate) use validation::upgrade_legacy_model_slugs;

pub(crate) const OPENAI_DEFAULT_MODEL: &str = "gpt-5.2-codex";
const OPENAI_DEFAULT_REVIEW_MODEL: &str = "gpt-5.2-codex";
pub const GPT_5_CODEX_MEDIUM_MODEL: &str = "gpt-5.2-codex";

/// Maximum number of bytes of the documentation that will be embedded. Larger
/// files are *silently truncated* to this size so we do not take up too much of
/// the context window.
pub(crate) const PROJECT_DOC_MAX_BYTES: usize = 32 * 1024; // 32 KiB
pub(crate) const CONFIG_TOML_FILE: &str = "config.toml";

const DEFAULT_RESPONSES_ORIGINATOR_HEADER: &str = "code_cli_rs";

/// Application configuration loaded from disk and merged with overrides.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Optional override of model selection.
    pub model: String,

    /// True if the model was explicitly chosen by the user (via CLI args,
    /// config.toml, or a profile). When false, Code may adopt a server-provided
    /// default model (e.g. "codex-auto-balanced") when available.
    pub model_explicit: bool,

    /// Planning model (used when in Plan mode / Read Only access preset). Falls back to `model`.
    pub planning_model: String,

    /// Reasoning effort for planning model.
    pub planning_model_reasoning_effort: ReasoningEffort,

    /// Whether planning should inherit the chat model instead of using a dedicated override.
    pub planning_use_chat_model: bool,

    /// Model used specifically for review sessions. Defaults to "gpt-5.2-codex".
    pub review_model: String,

    /// Reasoning effort used when running review sessions.
    pub review_model_reasoning_effort: ReasoningEffort,

    /// Whether review should inherit the chat model instead of using a dedicated override.
    pub review_use_chat_model: bool,

    /// Model used to apply fixes during auto-resolve of `/review` flows.
    pub review_resolve_model: String,

    /// Reasoning effort used for the resolve model when auto-resolving `/review`.
    pub review_resolve_model_reasoning_effort: ReasoningEffort,

    /// Whether resolve steps should inherit the chat model instead of a dedicated override.
    pub review_resolve_use_chat_model: bool,

    /// Model used for background Auto Review runs.
    pub auto_review_model: String,

    /// Reasoning effort used when running Auto Review.
    pub auto_review_model_reasoning_effort: ReasoningEffort,

    /// Whether Auto Review should inherit the chat model instead of a dedicated override.
    pub auto_review_use_chat_model: bool,

    /// Model used to apply fixes during Auto Review follow-ups.
    pub auto_review_resolve_model: String,

    /// Reasoning effort used for Auto Review resolve steps.
    pub auto_review_resolve_model_reasoning_effort: ReasoningEffort,

    /// Whether Auto Review resolve steps should inherit the chat model.
    pub auto_review_resolve_use_chat_model: bool,

    pub model_family: ModelFamily,

    /// Size of the context window for the model, in tokens.
    pub model_context_window: Option<u64>,

    /// Maximum number of output tokens.
    pub model_max_output_tokens: Option<u64>,

    /// Token usage threshold triggering auto-compaction of conversation history.
    pub model_auto_compact_token_limit: Option<i64>,

    /// Key into the model_providers map that specifies which provider to use.
    pub model_provider_id: String,

    /// Info needed to make an API request to the model.
    pub model_provider: ModelProviderInfo,

    /// Name of the active profile, if any, that populated this configuration.
    pub active_profile: Option<String>,

    /// Approval policy for executing commands.
    pub approval_policy: AskForApproval,

    pub sandbox_policy: SandboxPolicy,

    /// Commands the user has permanently approved for this project/session.
    pub always_allow_commands: Vec<ApprovedCommandPattern>,

    /// Project-level lifecycle hooks configured for the active workspace.
    pub project_hooks: ProjectHooks,

    /// Project-specific commands available in the active workspace.
    pub project_commands: Vec<ProjectCommand>,

    pub shell_environment_policy: ShellEnvironmentPolicy,
    /// Patterns requiring an explicit confirm prefix before running.
    pub confirm_guard: ConfirmGuardConfig,

    /// When `true`, `AgentReasoning` events emitted by the backend will be
    /// suppressed from the frontend output. This can reduce visual noise when
    /// users are only interested in the final agent responses.
    pub hide_agent_reasoning: bool,

    /// When set to `true`, `AgentReasoningRawContentEvent` events will be shown in the UI/output.
    /// Defaults to `false`.
    pub show_raw_agent_reasoning: bool,

    /// Disable server-side response storage (sends the full conversation
    /// context with every request). Currently necessary for OpenAI customers
    /// who have opted into Zero Data Retention (ZDR).
    pub disable_response_storage: bool,

    /// OTEL configuration (exporter type, endpoint, headers, etc.).
    pub otel: crate::config_types::OtelConfig,

    /// When true, Code will silently install updates on startup whenever a newer
    /// release is available. Upgrades are performed using the package manager
    /// that originally installed the CLI (Homebrew or npm). Manual installs are
    /// never upgraded automatically.
    pub auto_upgrade_enabled: bool,

    /// User-provided instructions from AGENTS.md.
    pub user_instructions: Option<String>,

    /// Optional developer-role message to prepend to every turn for demos.
    /// Set by the CLI via `--demo`; not loaded from disk.
    pub demo_developer_message: Option<String>,

    /// Base instructions override.
    pub base_instructions: Option<String>,

    /// Optional personality used to template gpt-5.2-codex base instructions.
    pub model_personality: Option<Personality>,

    /// Optional override for the compaction prompt text.
    pub compact_prompt_override: Option<String>,

    /// Optional external notifier command. When set, Codex will spawn this
    /// program after each completed *turn* (i.e. when the agent finishes
    /// processing a user submission). The value must be the full command
    /// broken into argv tokens **without** the trailing JSON argument - Codex
    /// appends one extra argument containing a JSON payload describing the
    /// event.
    ///
    /// Example `~/.code/config.toml` snippet (Code also reads legacy
    /// `~/.codex/config.toml`):
    ///
    /// ```toml
    /// notify = ["notify-send", "Codex"]
    /// ```
    ///
    /// which will be invoked as:
    ///
    /// ```shell
    /// notify-send Codex '{"type":"agent-turn-complete","turn-id":"12345"}'
    /// ```
    ///
    /// If unset the feature is disabled.
    pub notify: Option<Vec<String>>,

    /// Record of which one-time notices the user has acknowledged.
    pub notices: Notice,

    /// TUI notifications preference. When set, the TUI will send OSC 9 notifications on approvals
    /// and turn completions when not focused.
    pub tui_notifications: Notifications,

    /// Cadence (in requests) for running the Auto Drive observer thread.
    pub auto_drive_observer_cadence: u32,

    /// The directory that should be treated as the current working directory
    /// for the session. All relative paths inside the business-logic layer are
    /// resolved against this path.
    pub cwd: PathBuf,

    /// Definition for MCP servers that Codex can reach out to for tool calls.
    pub mcp_servers: HashMap<String, McpServerConfig>,

    /// Optional ACP client tool identifiers supplied by the host IDE.
    pub experimental_client_tools: Option<ClientTools>,

    /// Configuration for available agent models
    pub agents: Vec<AgentConfig>,

    /// Combined provider map (defaults merged with user-defined overrides).
    pub model_providers: HashMap<String, ModelProviderInfo>,

    /// Maximum number of bytes to include from an AGENTS.md project doc file.
    pub project_doc_max_bytes: usize,

    /// Maximum number of bytes of tool output to include in a model request.
    pub tool_output_max_bytes: usize,

    /// Ordered list of fallback filenames to consider when loading project docs.
    pub project_doc_fallback_filenames: Vec<String>,

    /// Directory containing all Codex state (defaults to `~/.code`; can be
    /// overridden by the `CODE_HOME` or `CODEX_HOME` environment variables).
    pub code_home: PathBuf,

    /// Settings that govern if and what will be written to `~/.code/history.jsonl`
    /// (Code still reads legacy `~/.codex/history.jsonl`).
    pub history: History,

    /// Optional URI-based file opener. If set, citations to files in the model
    /// output will be hyperlinked using the specified URI scheme.
    pub file_opener: UriBasedFileOpener,

    /// Collection of settings that are specific to the TUI.
    pub tui: Tui,

    /// Shared Auto Drive defaults.
    pub auto_drive: AutoDriveSettings,
    /// Whether Auto Drive should inherit the chat model instead of a dedicated override.
    pub auto_drive_use_chat_model: bool,

    /// Path to the `codex-linux-sandbox` executable. This must be set if
    /// [`crate::exec::SandboxType::LinuxSeccomp`] is used. Note that this
    /// cannot be set in the config file: it must be set in code via
    /// [`ConfigOverrides`].
    ///
    /// When this program is invoked, arg0 will be set to `codex-linux-sandbox`.
    pub code_linux_sandbox_exe: Option<PathBuf>,

    /// The value to use for `reasoning.effort` when making a
    /// request using the Responses API. Allowed values: `minimal`, `low`, `medium`, `high`, `xhigh`.
    pub model_reasoning_effort: ReasoningEffort,

    /// Optional preferred reasoning effort for the chat model. When the active model
    /// does not support this level, Code will clamp the effective effort but keep
    /// the preference so switching back restores it.
    pub preferred_model_reasoning_effort: Option<ReasoningEffort>,

    /// If not "none", the value to use for `reasoning.summary` when making a
    /// request using the Responses API.
    pub model_reasoning_summary: ReasoningSummary,

    /// The value to use for `text.verbosity` when making a request using the Responses API.
    pub model_text_verbosity: TextVerbosity,

    /// Base URL for requests to ChatGPT (as opposed to the OpenAI API).
    pub chatgpt_base_url: String,

    /// Include an experimental plan tool that the model can use to update its current plan and status of each step.
    pub include_plan_tool: bool,
    /// Include the `apply_patch` tool for models that benefit from invoking
    /// file edits as a structured tool call. When unset, this falls back to the
    /// model family's default preference.
    pub include_apply_patch_tool: bool,
    /// Enable the native Responses web_search tool.
    pub tools_web_search_request: bool,
    /// Optional allow-list of domains for web_search filters.allowed_domains
    pub tools_web_search_allowed_domains: Option<Vec<String>>,
    /// Experimental: enable streamable shell tool selection (off by default).
    pub use_experimental_streamable_shell_tool: bool,
    /// Enable the `image_view` tool that lets the agent attach local images.
    pub include_view_image_tool: bool,

    /// Experimental: enable discovery and injection of skills.
    pub skills_enabled: bool,
    /// Experimental: enable JSON-based environment context snapshots and deltas (phase gated).
    pub env_ctx_v2: bool,
    /// Retention policy for env_ctx_v2 timeline management (gated by env_ctx_v2).
    pub retention: crate::config_types::RetentionConfig,
    /// The value for the `originator` header included with Responses API requests.
    pub responses_originator_header: String,

    /// Enable debug logging of LLM requests and responses
    pub debug: bool,
    
    /// Whether we're using ChatGPT authentication (affects feature availability)
    pub using_chatgpt_auth: bool,

    /// When true, automatically switch to another connected account when the
    /// current account hits a rate/usage limit.
    pub auto_switch_accounts_on_rate_limit: bool,

    /// When true, fall back to an API key account only if every connected
    /// ChatGPT account is rate/usage limited.
    pub api_key_fallback_on_all_accounts_limited: bool,

    /// GitHub integration configuration.
    pub github: GithubConfig,

    /// Validation harness configuration.
    pub validation: ValidationConfig,

    /// Resolved subagent command configurations (including custom ones).
    /// If a command with name `plan|solve|code` exists here, it overrides
    /// the built-in defaults for that slash command.
    pub subagent_commands: Vec<crate::config_types::SubagentCommandConfig>,
    /// Experimental: path to a rollout file to resume a prior session from.
    /// When set, the core will send this path in the initial ConfigureSession
    /// so the backend can attempt to resume.
    pub experimental_resume: Option<PathBuf>,

    /// Optional wall-clock time budget (seconds) for the current run.
    ///
    /// Intended for `code exec` / benchmarks where the CLI must finish within
    /// a hard deadline. This value is not loaded from `config.toml`; callers
    /// should set it explicitly.
    pub max_run_seconds: Option<u64>,

    /// Optional wall-clock deadline for the current run.
    ///
    /// When present, countdown nudges are anchored to this deadline instead of
    /// the session creation time so startup work doesn't delay warnings.
    pub max_run_deadline: Option<Instant>,

    /// True when exec is running Auto Drive with a max time budget.
    ///
    /// Set by exec; not loaded from config.toml.
    pub timeboxed_exec_mode: bool,
}

impl Config {
    /// Load configuration with *generic* CLI overrides (`-c key=value`) applied
    /// **in between** the values parsed from `config.toml` and the
    /// strongly-typed overrides specified via [`ConfigOverrides`].
    ///
    /// The precedence order is therefore: `config.toml` < `-c` overrides <
    /// `ConfigOverrides`.
    pub fn load_with_cli_overrides(
        cli_overrides: Vec<(String, TomlValue)>,
        overrides: ConfigOverrides,
    ) -> std::io::Result<Self> {
        ConfigBuilder::new()
            .with_cli_overrides(cli_overrides)
            .with_overrides(overrides)
            .load()
    }
}

pub fn load_config_as_toml_with_cli_overrides(
    code_home: &Path,
    cli_overrides: Vec<(String, TomlValue)>,
) -> std::io::Result<ConfigToml> {
    ConfigBuilder::new()
        .with_code_home(code_home.to_path_buf())
        .with_cli_overrides(cli_overrides)
        .load_toml()
}

/// Base config deserialized from ~/.code/config.toml (legacy ~/.codex/config.toml is still read).
#[derive(Deserialize, Debug, Clone, Default)]
pub struct ConfigToml {
    /// Optional override of model selection.
    pub model: Option<String>,
    /// Planning model override used when in Read Only (Plan Mode).
    pub planning_model: Option<String>,
    /// Reasoning effort override used for the planning model.
    pub planning_model_reasoning_effort: Option<ReasoningEffort>,
    /// Inherit chat model for planning mode when true.
    pub planning_use_chat_model: Option<bool>,
    /// Review model override used by the `/review` feature.
    pub review_model: Option<String>,
    /// Reasoning effort override used for the review model.
    pub review_model_reasoning_effort: Option<ReasoningEffort>,
    /// Inherit chat model for review flows when true.
    #[serde(default)]
    pub review_use_chat_model: bool,

    /// Resolve model override used during auto-resolve for `/review`.
    pub review_resolve_model: Option<String>,
    /// Reasoning effort override used for the resolve model.
    pub review_resolve_model_reasoning_effort: Option<ReasoningEffort>,
    /// Inherit chat model for resolve flows when true.
    #[serde(default = "default_true_local")]
    pub review_resolve_use_chat_model: bool,

    /// Auto Review model override used for background reviews.
    pub auto_review_model: Option<String>,
    /// Reasoning effort override used for the Auto Review model.
    pub auto_review_model_reasoning_effort: Option<ReasoningEffort>,
    /// Inherit chat model for Auto Review when true.
    #[serde(default)]
    pub auto_review_use_chat_model: bool,

    /// Resolve model override used during Auto Review follow-ups.
    pub auto_review_resolve_model: Option<String>,
    /// Reasoning effort override used for the Auto Review resolve model.
    pub auto_review_resolve_model_reasoning_effort: Option<ReasoningEffort>,
    /// Inherit chat model for Auto Review resolve flows when true.
    #[serde(default = "default_true_local")]
    pub auto_review_resolve_use_chat_model: bool,

    /// Provider to use from the model_providers map.
    pub model_provider: Option<String>,

    /// Size of the context window for the model, in tokens.
    pub model_context_window: Option<u64>,

    /// Maximum number of output tokens.
    pub model_max_output_tokens: Option<u64>,

    /// Token usage threshold triggering auto-compaction of conversation history.
    pub model_auto_compact_token_limit: Option<i64>,

    /// Default approval policy for executing commands.
    pub approval_policy: Option<AskForApproval>,

    #[serde(default)]
    pub shell_environment_policy: ShellEnvironmentPolicyToml,

    /// Sandbox mode to use.
    pub sandbox_mode: Option<SandboxMode>,

    /// Sandbox configuration to apply if `sandbox` is `WorkspaceWrite`.
    pub sandbox_workspace_write: Option<SandboxWorkspaceWrite>,

    #[serde(default)]
    pub confirm_guard: Option<ConfirmGuardConfig>,

    /// Disable server-side response storage (sends the full conversation
    /// context with every request). Currently necessary for OpenAI customers
    /// who have opted into Zero Data Retention (ZDR).
    pub disable_response_storage: Option<bool>,

    #[serde(default)]
    pub otel: Option<OtelConfigToml>,

    /// Enable silent upgrades during startup when a newer release is available.
    #[serde(default, deserialize_with = "deserialize_option_bool_from_maybe_string")]
    pub auto_upgrade_enabled: Option<bool>,

    /// Optional external command to spawn for end-user notifications.
    #[serde(default)]
    pub notify: Option<Vec<String>>,

    /// Stored acknowledgement flags for in-product notices.
    pub notice: Option<Notice>,

    /// System instructions.
    pub instructions: Option<String>,

    /// Definition for MCP servers that Codex can reach out to for tool calls.
    #[serde(default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,

    /// Optional ACP client tool identifiers supplied by the host IDE.
    #[serde(default)]
    pub experimental_client_tools: Option<ClientTools>,

    /// Configuration for available agent models
    #[serde(default)]
    pub agents: Vec<AgentConfig>,

    /// User-defined provider entries that extend/override the built-in list.
    #[serde(default)]
    pub model_providers: HashMap<String, ModelProviderInfo>,

    /// Maximum number of bytes to include from an AGENTS.md project doc file.
    pub project_doc_max_bytes: Option<usize>,

    /// Maximum number of bytes of tool output to include in a model request.
    pub tool_output_max_bytes: Option<usize>,

    /// Ordered list of fallback filenames to look for when AGENTS.md is missing.
    pub project_doc_fallback_filenames: Option<Vec<String>>,

    /// Profile to use from the `profiles` map.
    pub profile: Option<String>,

    /// Named profiles to facilitate switching between different configurations.
    #[serde(default)]
    pub profiles: HashMap<String, ConfigProfile>,

    /// Settings that govern if and what will be written to `~/.code/history.jsonl`
    /// (Code still reads legacy `~/.codex/history.jsonl`).
    #[serde(default)]
    pub history: Option<History>,

    /// Optional URI-based file opener. If set, citations to files in the model
    /// output will be hyperlinked using the specified URI scheme.
    pub file_opener: Option<UriBasedFileOpener>,

    /// Collection of settings that are specific to the TUI.
    pub tui: Option<Tui>,

    /// Auto Drive behavioral defaults.
    pub auto_drive: Option<AutoDriveSettings>,

    /// If true, Auto Drive inherits the chat model instead of a dedicated override.
    pub auto_drive_use_chat_model: Option<bool>,

    #[serde(default)]
    pub auto_drive_observer_cadence: Option<u32>,

    /// Browser configuration for integrated screenshot capabilities.
    pub browser: Option<BrowserConfig>,

    /// When set to `true`, `AgentReasoning` events will be hidden from the
    /// UI/output. Defaults to `false`.
    pub hide_agent_reasoning: Option<bool>,

    /// When set to `true`, `AgentReasoningRawContentEvent` events will be shown in the UI/output.
    /// Defaults to `false`.
    pub show_raw_agent_reasoning: Option<bool>,

    pub model_reasoning_effort: Option<ReasoningEffort>,
    pub preferred_model_reasoning_effort: Option<ReasoningEffort>,
    pub model_reasoning_summary: Option<ReasoningSummary>,
    pub model_text_verbosity: Option<TextVerbosity>,
    pub model_personality: Option<Personality>,

    /// Override to force-enable reasoning summaries for the configured model.
    pub model_supports_reasoning_summaries: Option<bool>,

    /// Base URL for requests to ChatGPT (as opposed to the OpenAI API).
    pub chatgpt_base_url: Option<String>,

    /// Experimental path to a file whose contents replace the built-in BASE_INSTRUCTIONS.
    pub experimental_instructions_file: Option<PathBuf>,

    /// Optional override string for the compaction prompt.
    pub compact_prompt_override: Option<String>,

    /// Path to a file whose contents should replace the compaction prompt template.
    pub compact_prompt_file: Option<PathBuf>,

    pub experimental_use_exec_command_tool: Option<bool>,

    pub use_experimental_reasoning_summary: Option<bool>,

    /// The value for the `originator` header included with Responses API requests.
    pub responses_originator_header_internal_override: Option<String>,

    pub projects: Option<HashMap<String, ProjectConfig>>,

    /// If set to `true`, the API key will be signed with the `originator` header.
    pub preferred_auth_method: Option<AuthMode>,

    /// When true, automatically switch to another connected account when the
    /// current account hits a rate/usage limit.
    #[serde(default)]
    pub auto_switch_accounts_on_rate_limit: Option<bool>,

    /// When true, fall back to an API key account only if every connected
    /// ChatGPT account is rate/usage limited.
    #[serde(default)]
    pub api_key_fallback_on_all_accounts_limited: Option<bool>,

    /// Nested tools section for feature toggles
    pub tools: Option<ToolsToml>,

    /// Experimental feature toggles.
    pub features: Option<FeaturesToml>,

    /// When true, disables burst-paste detection for typed input entirely.
    /// All characters are inserted as they are received, and no buffering
    /// or placeholder replacement will occur for fast keypress bursts.
    pub disable_paste_burst: Option<bool>,

    /// GitHub integration configuration.
    pub github: Option<GithubConfig>,

    /// Validation harness configuration.
    pub validation: Option<ValidationConfig>,

    /// Configuration for subagent commands (built-ins and custom).
    #[serde(default)]
    pub subagents: Option<crate::config_types::SubagentsToml>,
    /// Experimental path to a rollout file to resume from.
    pub experimental_resume: Option<PathBuf>,
}

fn deserialize_option_bool_from_maybe_string<'de, D>(
    deserializer: D,
) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrString {
        Bool(bool),
        String(String),
    }

    let value = Option::<BoolOrString>::deserialize(deserializer)?;
    match value {
        Some(BoolOrString::Bool(b)) => Ok(Some(b)),
        Some(BoolOrString::String(s)) => {
            let normalized = s.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "true" => Ok(Some(true)),
                "false" => Ok(Some(false)),
                _ => Err(de::Error::invalid_value(
                    Unexpected::Str(&s),
                    &"a boolean or string 'true'/'false'",
                )),
            }
        }
        None => Ok(None),
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct ProjectConfig {
    pub trust_level: Option<String>,
    pub approval_policy: Option<AskForApproval>,
    pub sandbox_mode: Option<SandboxMode>,
    #[serde(default)]
    pub always_allow_commands: Option<Vec<AllowedCommand>>,
    #[serde(default)]
    pub hooks: Vec<ProjectHookConfig>,
    #[serde(default)]
    pub commands: Vec<ProjectCommandConfig>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct ToolsToml {
    #[serde(default, alias = "web_search_request")]
    pub web_search: Option<bool>,

    /// Optional allow-list of domains used by the Responses API web_search tool.
    /// Example:
    ///
    /// [tools]
    /// web_search = true
    /// web_search_allowed_domains = ["openai.com", "arxiv.org"]
    #[serde(default)]
    pub web_search_allowed_domains: Option<Vec<String>>,

    /// Enable the `image_view` tool that lets the agent attach local images.
    #[serde(default)]
    pub view_image: Option<bool>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct FeaturesToml {
    /// Enable discovery and injection of skills.
    #[serde(default)]
    pub skills: Option<bool>,
}

impl ConfigToml {
    /// Derive the effective sandbox policy from the configuration.
    #[cfg(test)]
    fn derive_sandbox_policy(&self, sandbox_mode_override: Option<SandboxMode>) -> SandboxPolicy {
        let resolved_sandbox_mode = sandbox_mode_override
            .or(self.sandbox_mode)
            .unwrap_or_default();
        match resolved_sandbox_mode {
            SandboxMode::ReadOnly => SandboxPolicy::new_read_only_policy(),
            SandboxMode::WorkspaceWrite => match self.sandbox_workspace_write.as_ref() {
                Some(SandboxWorkspaceWrite {
                    writable_roots,
                    network_access,
                    exclude_tmpdir_env_var,
                    exclude_slash_tmp,
                    allow_git_writes,
                }) => SandboxPolicy::WorkspaceWrite {
                    writable_roots: writable_roots.clone(),
                    network_access: *network_access,
                    exclude_tmpdir_env_var: *exclude_tmpdir_env_var,
                    exclude_slash_tmp: *exclude_slash_tmp,
                    allow_git_writes: *allow_git_writes,
                },
                None => SandboxPolicy::new_workspace_write_policy(),
            },
            SandboxMode::DangerFullAccess => SandboxPolicy::DangerFullAccess,
        }
    }

    pub fn is_cwd_trusted(&self, resolved_cwd: &Path) -> bool {
        let projects = self.projects.clone().unwrap_or_default();

        let is_path_trusted = |path: &Path| {
            let path_str = path.to_string_lossy().to_string();
            projects
                .get(&path_str)
                .map(|p| p.trust_level.as_deref() == Some("trusted"))
                .unwrap_or(false)
        };

        // Fast path: exact cwd match
        if is_path_trusted(resolved_cwd) {
            return true;
        }

        // If cwd lives inside a git worktree, check whether the root git project
        // (the primary repository working directory) is trusted. This lets
        // worktrees inherit trust from the main project.
        if let Some(root_project) = resolve_root_git_project_for_trust(resolved_cwd) {
            return is_path_trusted(&root_project);
        }

        false
    }

    pub fn get_config_profile(
        &self,
        override_profile: Option<String>,
    ) -> Result<ConfigProfile, std::io::Error> {
        let profile = override_profile.or_else(|| self.profile.clone());

        match profile {
            Some(key) => {
                if let Some(profile) = self.profiles.get(key.as_str()) {
                    return Ok(profile.clone());
                }

                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("config profile `{key}` not found"),
                ))
            }
            None => Ok(ConfigProfile::default()),
        }
    }
}

/// Optional overrides for user configuration (e.g., from CLI flags).
#[derive(Default, Debug, Clone)]
pub struct ConfigOverrides {
    pub model: Option<String>,
    pub review_model: Option<String>,
    pub cwd: Option<PathBuf>,
    pub approval_policy: Option<AskForApproval>,
    pub sandbox_mode: Option<SandboxMode>,
    pub model_provider: Option<String>,
    pub config_profile: Option<String>,
    pub code_linux_sandbox_exe: Option<PathBuf>,
    pub base_instructions: Option<String>,
    pub include_plan_tool: Option<bool>,
    pub include_apply_patch_tool: Option<bool>,
    pub include_view_image_tool: Option<bool>,
    pub disable_response_storage: Option<bool>,
    pub show_raw_agent_reasoning: Option<bool>,
    pub debug: Option<bool>,
    pub tools_web_search_request: Option<bool>,
    pub mcp_servers: Option<HashMap<String, McpServerConfig>>,
    pub experimental_client_tools: Option<ClientTools>,
    pub compact_prompt_override: Option<String>,
    pub compact_prompt_override_file: Option<PathBuf>,
}

impl Config {
    /// Meant to be used exclusively for tests: `load_with_overrides()` should
    /// be used in all other cases.
    pub fn load_from_base_config_with_overrides(
        cfg: ConfigToml,
        overrides: ConfigOverrides,
        code_home: PathBuf,
    ) -> std::io::Result<Self> {
        let user_instructions = Self::load_instructions(Some(&code_home));

        let mut cfg = cfg;
        upgrade_legacy_model_slugs(&mut cfg);

        // Destructure ConfigOverrides fully to ensure all overrides are applied.
        let ConfigOverrides {
            model,
            review_model: override_review_model,
            cwd,
            approval_policy,
            sandbox_mode,
            model_provider,
            config_profile: config_profile_key,
            code_linux_sandbox_exe,
            base_instructions,
            include_plan_tool,
            include_apply_patch_tool,
            include_view_image_tool,
            disable_response_storage,
            show_raw_agent_reasoning,
            debug,
            tools_web_search_request: override_tools_web_search_request,
            mcp_servers,
            experimental_client_tools,
            compact_prompt_override,
            compact_prompt_override_file,
        } = overrides;

        if let Some(mcp_servers) = mcp_servers {
            cfg.mcp_servers = mcp_servers;
        }

        if let Some(client_tools) = experimental_client_tools {
            cfg.experimental_client_tools = Some(client_tools);
        }

        let (active_profile_name, config_profile) =
            match config_profile_key.as_ref().or(cfg.profile.as_ref()) {
                Some(key) => {
                    let profile = cfg
                        .profiles
                        .get(key)
                        .ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                format!("config profile `{key}` not found"),
                            )
                        })?
                        .clone();
                    (Some(key.to_string()), profile)
                }
                None => (None, ConfigProfile::default()),
            };

        // (removed placeholder) sandbox_policy computed below after resolving project overrides.

        let mut model_providers = built_in_model_providers();
        // Merge user-defined providers into the built-in list.
        for (key, provider) in cfg.model_providers.into_iter() {
            model_providers.entry(key).or_insert(provider);
        }

        let model_provider_id = model_provider
            .or(config_profile.model_provider)
            .or(cfg.model_provider)
            .unwrap_or_else(|| "openai".to_string());
        let model_provider = model_providers
            .get(&model_provider_id)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Model provider `{model_provider_id}` not found"),
                )
            })?
            .clone();

        // Capture workspace-write details early to avoid borrow after partial moves
        let cfg_workspace = cfg.sandbox_workspace_write.clone();

        let shell_environment_policy = cfg.shell_environment_policy.into();

        let resolved_cwd = {
            use std::env;

            match cwd {
                None => {
                    tracing::info!("cwd not set, using current dir");
                    env::current_dir()?
                }
                Some(p) if p.is_absolute() => p,
                Some(p) => {
                    // Resolve relative path against the current working directory.
                    tracing::info!("cwd is relative, resolving against current dir");
                    let mut current = env::current_dir()?;
                    current.push(p);
                    current
                }
            }
        };

        // Do NOT normalize to the Git repository root.
        // Honor the exact directory the program was started in (or provided via -C/--cd).
        // Any Git-aware features should resolve the repo root on demand.

        // Project-specific overrides based on final resolved cwd (exact match)
        let project_key = resolved_cwd.to_string_lossy().to_string();
        let project_override = cfg
            .projects
            .as_ref()
            .and_then(|m| m.get(&project_key));
        // Resolve sandbox mode with correct precedence:
        // CLI override > per-project override > global config.toml > default
        let effective_sandbox_mode = sandbox_mode
            .or(project_override.and_then(|p| p.sandbox_mode))
            .or(cfg.sandbox_mode)
            .unwrap_or_default();
        let sandbox_policy = match effective_sandbox_mode {
            SandboxMode::ReadOnly => SandboxPolicy::new_read_only_policy(),
            SandboxMode::WorkspaceWrite => match cfg_workspace {
                Some(SandboxWorkspaceWrite {
                    writable_roots,
                    network_access,
                    exclude_tmpdir_env_var,
                    exclude_slash_tmp,
                    allow_git_writes,
                }) => SandboxPolicy::WorkspaceWrite {
                    writable_roots,
                    network_access,
                    exclude_tmpdir_env_var,
                    exclude_slash_tmp,
                    allow_git_writes,
                },
                None => SandboxPolicy::new_workspace_write_policy(),
            },
            SandboxMode::DangerFullAccess => SandboxPolicy::DangerFullAccess,
        };
        // Resolve approval policy with precedence:
        // CLI override > profile override > per-project override > global config.toml > default
        let effective_approval = approval_policy
            .or(config_profile.approval_policy)
            .or(project_override.and_then(|p| p.approval_policy))
            .or(cfg.approval_policy)
            .unwrap_or_else(AskForApproval::default);

        let history = cfg.history.unwrap_or_default();

        let mut always_allow_commands: Vec<ApprovedCommandPattern> = Vec::new();
        if let Some(project_cfg) = project_override {
            if let Some(commands) = &project_cfg.always_allow_commands {
                for cmd in commands {
                    if cmd.argv.is_empty() {
                        continue;
                    }
                    let kind = match cmd.match_kind {
                        AllowedCommandMatchKind::Exact => ApprovedCommandMatchKind::Exact,
                        AllowedCommandMatchKind::Prefix => ApprovedCommandMatchKind::Prefix,
                    };
                    let semantic = if matches!(kind, ApprovedCommandMatchKind::Prefix) {
                        Some(cmd.argv.clone())
                    } else {
                        None
                    };
                    always_allow_commands.push(ApprovedCommandPattern::new(
                        cmd.argv.clone(),
                        kind,
                        semantic,
                    ));
                }
            }
        }

        let project_hooks = project_override
            .map(|cfg| ProjectHooks::from_configs(&cfg.hooks, &resolved_cwd))
            .unwrap_or_default();
        let project_commands = project_override
            .map(|cfg| load_project_commands(&cfg.commands, &resolved_cwd))
            .unwrap_or_default();

        let tools_web_search_request = override_tools_web_search_request
            .or(cfg.tools.as_ref().and_then(|t| t.web_search))
            .unwrap_or(false);
        let tools_web_search_allowed_domains = cfg
            .tools
            .as_ref()
            .and_then(|t| t.web_search_allowed_domains.clone());
        // View Image tool is enabled by default; can be disabled in config or overrides.
        let include_view_image_tool_flag = include_view_image_tool
            .or(cfg.tools.as_ref().and_then(|t| t.view_image))
            .unwrap_or(true);

        let skills_enabled = cfg
            .features
            .as_ref()
            .and_then(|features| features.skills)
            .unwrap_or(true);

        let env_ctx_v2_flag = *crate::flags::CTX_UI;

        // Determine auth mode early so defaults like model selection can depend on it.
        let using_chatgpt_auth = Self::is_using_chatgpt_auth(&code_home);

        let auto_switch_accounts_on_rate_limit = config_profile
            .auto_switch_accounts_on_rate_limit
            .or(cfg.auto_switch_accounts_on_rate_limit)
            .unwrap_or(true);

        let api_key_fallback_on_all_accounts_limited = config_profile
            .api_key_fallback_on_all_accounts_limited
            .or(cfg.api_key_fallback_on_all_accounts_limited)
            .unwrap_or(false);

        let default_model_slug = if using_chatgpt_auth {
            GPT_5_CODEX_MEDIUM_MODEL
        } else {
            OPENAI_DEFAULT_MODEL
        };

        let model_explicit = model.is_some() || config_profile.model.is_some() || cfg.model.is_some();

        let model = model
            .or(config_profile.model)
            .or(cfg.model)
            .unwrap_or_else(|| default_model_slug.to_string());

        let model_personality = config_profile
            .model_personality
            .or(cfg.model_personality);

        let model_family =
            find_family_for_model(&model).unwrap_or_else(|| derive_default_model_family(&model));
        let default_tool_output_max_bytes = model_family.tool_output_max_bytes();

        // Chat model reasoning effort (used when other flows follow the chat model).
        let preferred_model_reasoning_effort = config_profile
            .preferred_model_reasoning_effort
            .or(cfg.preferred_model_reasoning_effort)
            .or(config_profile.model_reasoning_effort)
            .or(cfg.model_reasoning_effort);

        let requested_chat_effort =
            preferred_model_reasoning_effort.unwrap_or(ReasoningEffort::Medium);
        let chat_reasoning_effort =
            clamp_reasoning_effort_for_model(&model, requested_chat_effort);

        let model_context_window = cfg
            .model_context_window
            .or(model_family.context_window);
        let model_max_output_tokens = cfg
            .model_max_output_tokens
            .or(model_family.max_output_tokens);
        let model_auto_compact_token_limit = cfg
            .model_auto_compact_token_limit
            .or_else(|| model_family.auto_compact_token_limit());

        // Load base instructions override from a file if specified. If the
        // path is relative, resolve it against the effective cwd so the
        // behaviour matches other path-like config values.
        let experimental_instructions_path = config_profile
            .experimental_instructions_file
            .as_ref()
            .or(cfg.experimental_instructions_file.as_ref());
        let file_base_instructions =
            Self::get_base_instructions(experimental_instructions_path, &resolved_cwd)?;
        let base_instructions = base_instructions.or(file_base_instructions);

        let compact_prompt_file = compact_prompt_override_file
            .or(config_profile.compact_prompt_override_file.clone())
            .or(cfg.compact_prompt_file.clone());
        let file_compact_prompt =
            Self::get_compact_prompt_override(compact_prompt_file.as_ref(), &resolved_cwd)?;
        let compact_prompt_override = compact_prompt_override
            .or(config_profile.compact_prompt_override.clone())
            .or(cfg.compact_prompt_override.clone())
            .or(file_compact_prompt);

        let responses_originator_header: String = cfg
            .responses_originator_header_internal_override
            .unwrap_or_else(|| default_responses_originator());

        let agents: Vec<AgentConfig> = merge_with_default_agents(cfg.agents);

        for agent in &agents {
            if agent.name.eq_ignore_ascii_case("code")
                || agent.name.eq_ignore_ascii_case("codex")
                || agent.name.eq_ignore_ascii_case("claude")
                || agent.name.eq_ignore_ascii_case("gemini")
                || agent.name.eq_ignore_ascii_case("qwen")
                || agent.name.eq_ignore_ascii_case("cloud")
            {
                tracing::warn!(
                    "legacy agent name '{}' detected; update config to use model slugs (e.g., code-gpt-5.2-codex)",
                    agent.name
                );
            }
        }

        let mut confirm_guard = ConfirmGuardConfig::default();
        if let Some(mut user_guard) = cfg.confirm_guard {
            confirm_guard.patterns.extend(user_guard.patterns.drain(..));
        }
        for pattern in &confirm_guard.patterns {
            if let Err(err) = regex_lite::Regex::new(&pattern.regex) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid confirm_guard pattern `{}`: {err}", pattern.regex),
                ));
            }
        }

        // Default review model when not set in config; allow CLI override to take precedence.
        let review_model = override_review_model
            .or(config_profile.review_model.clone())
            .or(cfg.review_model)
            .unwrap_or_else(default_review_model);

        let review_model_reasoning_effort = config_profile
            .review_model_reasoning_effort
            .or(cfg.review_model_reasoning_effort)
            .unwrap_or(ReasoningEffort::High);

        let review_resolve_use_chat_model = config_profile
            .review_resolve_use_chat_model
            .or(Some(cfg.review_resolve_use_chat_model))
            .unwrap_or(true);
        let review_resolve_model = if review_resolve_use_chat_model {
            model.clone()
        } else {
            config_profile
                .review_resolve_model
                .clone()
                .or(cfg.review_resolve_model.clone())
                .unwrap_or_else(|| model.clone())
        };
        let review_resolve_model_reasoning_effort = if review_resolve_use_chat_model {
            chat_reasoning_effort
        } else {
            config_profile
                .review_resolve_model_reasoning_effort
                .or(cfg.review_resolve_model_reasoning_effort)
                .unwrap_or(ReasoningEffort::High)
        };
        let review_resolve_model_reasoning_effort = clamp_reasoning_effort_for_model(
            &review_resolve_model,
            review_resolve_model_reasoning_effort,
        );

        let planning_use_chat_model = config_profile
            .planning_use_chat_model
            .or(cfg.planning_use_chat_model)
            .unwrap_or_else(|| {
                config_profile.planning_model.is_none() && cfg.planning_model.is_none()
            });
        let planning_model = if planning_use_chat_model {
            model.clone()
        } else {
            config_profile
                .planning_model
                .or(cfg.planning_model)
                .unwrap_or_else(|| model.clone())
        };
        let planning_model_reasoning_effort = if planning_use_chat_model {
            chat_reasoning_effort
        } else {
            config_profile
                .planning_model_reasoning_effort
                .or(cfg.planning_model_reasoning_effort)
                .unwrap_or(chat_reasoning_effort)
        };
        let planning_model_reasoning_effort = clamp_reasoning_effort_for_model(
            &planning_model,
            planning_model_reasoning_effort,
        );

        let review_use_chat_model = config_profile
            .review_use_chat_model
            .or(Some(cfg.review_use_chat_model))
            .unwrap_or(false);
        let review_model = if review_use_chat_model {
            model.clone()
        } else {
            review_model
        };
        let review_model_reasoning_effort = if review_use_chat_model {
            chat_reasoning_effort
        } else {
            review_model_reasoning_effort
        };
        let review_model_reasoning_effort = clamp_reasoning_effort_for_model(
            &review_model,
            review_model_reasoning_effort,
        );

        let auto_review_use_chat_model = config_profile
            .auto_review_use_chat_model
            .or(Some(cfg.auto_review_use_chat_model))
            .unwrap_or(false);

        let auto_review_model = if auto_review_use_chat_model {
            model.clone()
        } else {
            config_profile
                .auto_review_model
                .clone()
                .or(cfg.auto_review_model.clone())
                .unwrap_or_else(default_review_model)
        };

        let auto_review_model_reasoning_effort = if auto_review_use_chat_model {
            chat_reasoning_effort
        } else {
            config_profile
                .auto_review_model_reasoning_effort
                .or(cfg.auto_review_model_reasoning_effort)
                .unwrap_or(ReasoningEffort::High)
        };
        let auto_review_model_reasoning_effort = clamp_reasoning_effort_for_model(
            &auto_review_model,
            auto_review_model_reasoning_effort,
        );

        let auto_review_resolve_use_chat_model = config_profile
            .auto_review_resolve_use_chat_model
            .or(Some(cfg.auto_review_resolve_use_chat_model))
            .unwrap_or(true);

        let auto_review_resolve_model = if auto_review_resolve_use_chat_model {
            model.clone()
        } else {
            config_profile
                .auto_review_resolve_model
                .clone()
                .or(cfg.auto_review_resolve_model.clone())
                .unwrap_or_else(|| model.clone())
        };

        let auto_review_resolve_model_reasoning_effort = if auto_review_resolve_use_chat_model {
            chat_reasoning_effort
        } else {
            config_profile
                .auto_review_resolve_model_reasoning_effort
                .or(cfg.auto_review_resolve_model_reasoning_effort)
                .unwrap_or(ReasoningEffort::High)
        };
        let auto_review_resolve_model_reasoning_effort = clamp_reasoning_effort_for_model(
            &auto_review_resolve_model,
            auto_review_resolve_model_reasoning_effort,
        );

        let auto_drive_use_chat_model = cfg.auto_drive_use_chat_model.unwrap_or(false);

        let mut auto_drive = cfg
            .auto_drive
            .clone()
            .or_else(|| cfg.tui.as_ref().and_then(|t| t.auto_drive.clone()))
            .unwrap_or_else(|| {
                let mut defaults = AutoDriveSettings::default();
                if using_chatgpt_auth {
                    defaults.model = GPT_5_CODEX_MEDIUM_MODEL.to_string();
                    defaults.model_reasoning_effort = ReasoningEffort::XHigh;
                }
                defaults
            });
        if auto_drive_use_chat_model {
            auto_drive.model = model.clone();
            auto_drive.model_reasoning_effort = chat_reasoning_effort;
        }

        auto_drive.model_reasoning_effort = clamp_reasoning_effort_for_model(
            &auto_drive.model,
            auto_drive.model_reasoning_effort,
        );

        let config = Self {
            model,
            model_explicit,
            planning_model,
            planning_model_reasoning_effort,
            planning_use_chat_model,
            review_model,
            review_model_reasoning_effort,
            review_use_chat_model,
            review_resolve_model,
            review_resolve_model_reasoning_effort,
            review_resolve_use_chat_model,
            auto_review_model,
            auto_review_model_reasoning_effort,
            auto_review_use_chat_model,
            auto_review_resolve_model,
            auto_review_resolve_model_reasoning_effort,
            auto_review_resolve_use_chat_model,
            model_family,
            model_context_window,
            model_max_output_tokens,
            model_auto_compact_token_limit,
            model_provider_id,
            model_provider,
            cwd: resolved_cwd,
            approval_policy: effective_approval,
            sandbox_policy,
            always_allow_commands,
            project_hooks,
            project_commands,
            shell_environment_policy,
            confirm_guard,
            disable_response_storage: config_profile
                .disable_response_storage
                .or(cfg.disable_response_storage)
                .or(disable_response_storage)
                .unwrap_or(false),
            auto_upgrade_enabled: cfg.auto_upgrade_enabled.unwrap_or(false),
            notify: cfg.notify,
            notices: cfg.notice.unwrap_or_default(),
            user_instructions,
            demo_developer_message: None,
            base_instructions,
            model_personality,
            compact_prompt_override,
            mcp_servers: cfg.mcp_servers,
            experimental_client_tools: cfg.experimental_client_tools.clone(),
            agents,
            model_providers,
            project_doc_max_bytes: cfg.project_doc_max_bytes.unwrap_or(PROJECT_DOC_MAX_BYTES),
            tool_output_max_bytes: cfg
                .tool_output_max_bytes
                .unwrap_or(default_tool_output_max_bytes),
            project_doc_fallback_filenames: cfg
                .project_doc_fallback_filenames
                .unwrap_or_default()
                .into_iter()
                .filter_map(|name| {
                    let trimmed = name.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    }
                })
                .collect(),
            code_home,
            history,
            file_opener: cfg.file_opener.unwrap_or(UriBasedFileOpener::VsCode),
            tui: cfg.tui.clone().unwrap_or_default(),
            auto_drive,
            auto_drive_use_chat_model,
            code_linux_sandbox_exe,
            active_profile: active_profile_name,

            hide_agent_reasoning: cfg.hide_agent_reasoning.unwrap_or(false),
            show_raw_agent_reasoning: cfg
                .show_raw_agent_reasoning
                .or(show_raw_agent_reasoning)
                .unwrap_or(false),
            model_reasoning_effort: chat_reasoning_effort,
            preferred_model_reasoning_effort,
            model_reasoning_summary: config_profile
                .model_reasoning_summary
                .or(cfg.model_reasoning_summary)
                .unwrap_or_default(),
            model_text_verbosity: config_profile
                .model_text_verbosity
                .or(cfg.model_text_verbosity)
                .unwrap_or_default(),

            chatgpt_base_url: config_profile
                .chatgpt_base_url
                .or(cfg.chatgpt_base_url)
                .unwrap_or("https://chatgpt.com/backend-api/".to_string()),
            include_plan_tool: include_plan_tool.unwrap_or(false),
            include_apply_patch_tool: include_apply_patch_tool.unwrap_or(false),
            tools_web_search_request,
            tools_web_search_allowed_domains,
            // Honor upstream opt-in switch name for our experimental streamable shell tool.
            use_experimental_streamable_shell_tool: cfg
                .experimental_use_exec_command_tool
                .unwrap_or(false),
            include_view_image_tool: include_view_image_tool_flag,
            skills_enabled,
            env_ctx_v2: env_ctx_v2_flag,
            retention: crate::config_types::RetentionConfig::default(),
            responses_originator_header,
            debug: debug.unwrap_or(false),
            // Already computed before moving code_home
            using_chatgpt_auth,
            auto_switch_accounts_on_rate_limit,
            api_key_fallback_on_all_accounts_limited,
            github: cfg.github.unwrap_or_default(),
            validation: cfg.validation.unwrap_or_default(),
            subagent_commands: cfg
                .subagents
                .map(|s| s.commands)
                .unwrap_or_default(),
            experimental_resume: cfg.experimental_resume,
            max_run_seconds: None,
            max_run_deadline: None,
            timeboxed_exec_mode: false,
            // Surface TUI notifications preference from config when present.
            tui_notifications: cfg
                .tui
                .as_ref()
                .map(|t| t.notifications.clone())
                .unwrap_or_default(),
            auto_drive_observer_cadence: cfg.auto_drive_observer_cadence.unwrap_or(5),
            otel: {
                let t: OtelConfigToml = cfg.otel.unwrap_or_default();
                let log_user_prompt = t.log_user_prompt.unwrap_or(false);
                let environment = t
                    .environment
                    .unwrap_or(DEFAULT_OTEL_ENVIRONMENT.to_string());
                let exporter = t.exporter.unwrap_or(OtelExporterKind::None);
                OtelConfig {
                    log_user_prompt,
                    environment,
                    exporter,
                }
            },
        };
        Ok(config)
    }

    /// Check if we're using ChatGPT authentication
    fn is_using_chatgpt_auth(code_home: &Path) -> bool {
        use code_app_server_protocol::AuthMode;
        use crate::CodexAuth;
        
        // Prefer ChatGPT when both ChatGPT tokens and an API key are present.
        match CodexAuth::from_code_home(code_home, AuthMode::ChatGPT, "code_cli_rs") {
            Ok(Some(auth)) => auth.mode == AuthMode::ChatGPT,
            _ => false,
        }
    }
    
    fn load_instructions(code_dir: Option<&Path>) -> Option<String> {
        sources::load_instructions(code_dir)
    }

    fn get_base_instructions(
        path: Option<&PathBuf>,
        cwd: &Path,
    ) -> std::io::Result<Option<String>> {
        sources::get_base_instructions(path, cwd)
    }

    fn get_compact_prompt_override(
        path: Option<&PathBuf>,
        cwd: &Path,
    ) -> std::io::Result<Option<String>> {
        sources::get_compact_prompt_override(path, cwd)
    }
}

pub fn log_dir(cfg: &Config) -> std::io::Result<PathBuf> {
    let mut p = cfg.code_home.clone();
    p.push("debug_logs");
    Ok(p)
}

pub fn migrate_legacy_log_dirs(code_home: &Path) {
    let target = code_home.join("debug_logs");
    let legacy_dirs = [code_home.join("log"), code_home.join("logs")];
    for legacy in legacy_dirs {
        let _ = migrate_dir_contents(&legacy, &target);
    }
}

fn migrate_dir_contents(source: &Path, target: &Path) -> std::io::Result<()> {
    if !source.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        migrate_entry(&entry.path(), target)?;
    }
    let _ = std::fs::remove_dir(source);
    Ok(())
}

fn migrate_entry(path: &Path, target_dir: &Path) -> std::io::Result<()> {
    let Some(file_name) = path.file_name() else {
        return Ok(());
    };
    let mut target = target_dir.join(file_name);
    if target.exists() {
        let base = file_name.to_string_lossy();
        let mut idx = 1u32;
        loop {
            let candidate = target_dir.join(format!("{base}.legacy.{idx}"));
            if !candidate.exists() {
                target = candidate;
                break;
            }
            idx = idx.saturating_add(1);
        }
    }

    if path.is_dir() {
        std::fs::create_dir_all(&target)?;
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            migrate_entry(&entry.path(), &target)?;
        }
        let _ = std::fs::remove_dir_all(path);
        return Ok(());
    }

    match std::fs::rename(path, &target) {
        Ok(()) => Ok(()),
        Err(_) => {
            std::fs::copy(path, &target)?;
            std::fs::remove_file(path)?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use crate::config_types::HistoryPersistence;
    use crate::config_types::McpServerTransportConfig;
    use crate::config_types::Notifications;

    use super::*;
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use tempfile::NamedTempFile;
    use tempfile::TempDir;

    struct EnvVarGuard {
        key: &'static str,
        original: Option<OsString>,
    }

    impl EnvVarGuard {
        fn new(key: &'static str) -> Self {
            Self {
                key,
                original: std::env::var_os(key),
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(val) => unsafe {
                    std::env::set_var(self.key, val)
                },
                None => unsafe {
                    std::env::remove_var(self.key)
                },
            }
        }
    }

    #[test]
    fn test_toml_parsing() {
        let history_with_persistence = r#"
[history]
persistence = "save-all"
"#;
        let history_with_persistence_cfg = toml::from_str::<ConfigToml>(history_with_persistence)
            .expect("TOML deserialization should succeed");
        assert_eq!(
            Some(History {
                persistence: HistoryPersistence::SaveAll,
                max_bytes: None,
            }),
            history_with_persistence_cfg.history
        );

        let history_no_persistence = r#"
[history]
persistence = "none"
"#;

        let history_no_persistence_cfg = toml::from_str::<ConfigToml>(history_no_persistence)
            .expect("TOML deserialization should succeed");
        assert_eq!(
            Some(History {
                persistence: HistoryPersistence::None,
                max_bytes: None,
            }),
            history_no_persistence_cfg.history
        );
    }

    #[test]
    fn auto_upgrade_enabled_accepts_string_boolean() {
        let cfg_true = r#"auto_upgrade_enabled = "true""#;
        let parsed_true = toml::from_str::<ConfigToml>(cfg_true)
            .expect("string boolean should deserialize");
        assert_eq!(parsed_true.auto_upgrade_enabled, Some(true));

        let cfg_false = r#"auto_upgrade_enabled = "false""#;
        let parsed_false = toml::from_str::<ConfigToml>(cfg_false)
            .expect("string boolean should deserialize");
        assert_eq!(parsed_false.auto_upgrade_enabled, Some(false));

        let cfg_bool = r#"auto_upgrade_enabled = true"#;
        let parsed_bool = toml::from_str::<ConfigToml>(cfg_bool)
            .expect("boolean should deserialize");
        assert_eq!(parsed_bool.auto_upgrade_enabled, Some(true));
    }

    #[test]
    fn tool_output_max_bytes_defaults_and_overrides() -> std::io::Result<()> {
        let code_home = TempDir::new()?;

        let default_config = Config::load_from_base_config_with_overrides(
            ConfigToml::default(),
            ConfigOverrides::default(),
            code_home.path().to_path_buf(),
        )?;
        assert_eq!(
            default_config.tool_output_max_bytes,
            default_config.model_family.tool_output_max_bytes()
        );

        let cfg = toml::from_str::<ConfigToml>("tool_output_max_bytes = 65536")
            .expect("TOML should deserialize");
        let overridden = Config::load_from_base_config_with_overrides(
            cfg,
            ConfigOverrides::default(),
            code_home.path().to_path_buf(),
        )?;
        assert_eq!(overridden.tool_output_max_bytes, 65_536);
        Ok(())
    }

    #[test]
    fn load_instructions_reads_from_code_home() -> anyhow::Result<()> {
        let code_home = TempDir::new()?;
        std::fs::write(code_home.path().join("AGENTS.md"), "  keep me  \n")?;

        let loaded = Config::load_instructions(Some(code_home.path()));

        assert_eq!(loaded.as_deref(), Some("keep me"));
        Ok(())
    }

    #[test]
    fn load_instructions_falls_back_to_legacy_codex_home() -> anyhow::Result<()> {
        let code_home = TempDir::new()?;
        let legacy_home = TempDir::new()?;
        let legacy_codex = legacy_home.path().join(".codex");
        std::fs::create_dir_all(&legacy_codex)?;
        std::fs::write(legacy_codex.join("AGENTS.md"), " legacy guidance \n")?;

        let _home_guard = EnvVarGuard::new("HOME");
        let _code_home_guard = EnvVarGuard::new("CODE_HOME");
        let _codex_home_guard = EnvVarGuard::new("CODEX_HOME");

        unsafe {
            std::env::set_var("HOME", legacy_home.path());
            std::env::remove_var("CODE_HOME");
            std::env::remove_var("CODEX_HOME");
        }

        let loaded = Config::load_instructions(Some(code_home.path()));

        assert_eq!(loaded.as_deref(), Some("legacy guidance"));
        Ok(())
    }

    #[test]
    fn tui_config_missing_notifications_field_defaults_to_disabled() {
        let cfg = r#"
[tui]
"#;

        let parsed = toml::from_str::<ConfigToml>(cfg)
            .expect("TUI config without notifications should succeed");
        let tui = parsed.tui.expect("config should include tui section");

        assert_eq!(tui.notifications, Notifications::Enabled(false));
    }

    #[test]
    fn test_sandbox_config_parsing() {
        let sandbox_full_access = r#"
sandbox_mode = "danger-full-access"

[sandbox_workspace_write]
network_access = false  # This should be ignored.
"#;
        let sandbox_full_access_cfg = toml::from_str::<ConfigToml>(sandbox_full_access)
            .expect("TOML deserialization should succeed");
        let sandbox_mode_override = None;
        assert_eq!(
            SandboxPolicy::DangerFullAccess,
            sandbox_full_access_cfg.derive_sandbox_policy(sandbox_mode_override)
        );

        let sandbox_read_only = r#"
sandbox_mode = "read-only"

[sandbox_workspace_write]
network_access = true  # This should be ignored.
"#;

        let sandbox_read_only_cfg = toml::from_str::<ConfigToml>(sandbox_read_only)
            .expect("TOML deserialization should succeed");
        let sandbox_mode_override = None;
        assert_eq!(
            SandboxPolicy::ReadOnly,
            sandbox_read_only_cfg.derive_sandbox_policy(sandbox_mode_override)
        );

        let sandbox_workspace_write = r#"
sandbox_mode = "workspace-write"

[sandbox_workspace_write]
writable_roots = [
    "/my/workspace",
]
exclude_tmpdir_env_var = true
exclude_slash_tmp = true
"#;

        let sandbox_workspace_write_cfg = toml::from_str::<ConfigToml>(sandbox_workspace_write)
            .expect("TOML deserialization should succeed");
        let sandbox_mode_override = None;
        assert_eq!(
            SandboxPolicy::WorkspaceWrite {
                writable_roots: vec![PathBuf::from("/my/workspace")],
                network_access: false,
                exclude_tmpdir_env_var: true,
                exclude_slash_tmp: true,
                allow_git_writes: true,
            },
            sandbox_workspace_write_cfg.derive_sandbox_policy(sandbox_mode_override)
        );
    }

    #[test]
    fn load_global_mcp_servers_returns_empty_if_missing() -> anyhow::Result<()> {
        let code_home = TempDir::new()?;

        let servers = load_global_mcp_servers(code_home.path())?;
        assert!(servers.is_empty());

        Ok(())
    }

    #[test]
    fn write_global_mcp_servers_round_trips_entries() -> anyhow::Result<()> {
        let code_home = TempDir::new()?;

        let mut servers = BTreeMap::new();
        servers.insert(
            "docs".to_string(),
            McpServerConfig {
                transport: McpServerTransportConfig::Stdio {
                    command: "echo".to_string(),
                    args: vec!["hello".to_string()],
                    env: None,
                },
                startup_timeout_sec: None,
                tool_timeout_sec: None,
            },
        );

        write_global_mcp_servers(code_home.path(), &servers)?;

        let loaded = load_global_mcp_servers(code_home.path())?;
        assert_eq!(loaded.len(), 1);
        let docs = loaded.get("docs").expect("docs entry");
        match &docs.transport {
            McpServerTransportConfig::Stdio { command, args, env } => {
                assert_eq!(command, "echo");
                assert_eq!(args, &vec!["hello".to_string()]);
                assert!(env.is_none());
            }
            _ => panic!("expected stdio transport"),
        }

        let empty = BTreeMap::new();
        write_global_mcp_servers(code_home.path(), &empty)?;
        let loaded = load_global_mcp_servers(code_home.path())?;
        assert!(loaded.is_empty());

        Ok(())
    }

    #[test]
    fn load_global_mcp_servers_rejects_missing_command() -> anyhow::Result<()> {
        let code_home = TempDir::new()?;
        let config_path = code_home.path().join(CONFIG_TOML_FILE);

        std::fs::write(
            &config_path,
            r#"[mcp_servers.context7-mcp]
command = "nonexistent-cmd"
args = ["-y", "@upstash/context7-mcp"]
"#,
        )?;

        let err = load_global_mcp_servers(code_home.path())
            .expect_err("missing executables should surface a readable error");
        let msg = err.to_string();

        assert!(
            msg.contains("context7-mcp"),
            "expected server name in error, got: {msg}"
        );
        assert!(
            msg.contains("command = \"npx\""),
            "expected hint suggesting command = \"npx\", got: {msg}"
        );

        Ok(())
    }
    #[tokio::test]
    async fn persist_model_selection_updates_defaults() -> anyhow::Result<()> {
        let code_home = TempDir::new()?;

        persist_model_selection(
            code_home.path(),
            None,
            "gpt-5.1-codex",
            Some(ReasoningEffort::High),
            None,
        )
        .await?;

        let serialized =
            tokio::fs::read_to_string(code_home.path().join(CONFIG_TOML_FILE)).await?;
        let parsed: ConfigToml = toml::from_str(&serialized)?;

        assert_eq!(parsed.model.as_deref(), Some("gpt-5.1-codex"));
        assert_eq!(parsed.model_reasoning_effort, Some(ReasoningEffort::High));

        Ok(())
    }

    #[tokio::test]
    async fn persist_model_selection_overwrites_existing_model() -> anyhow::Result<()> {
        let code_home = TempDir::new()?;
        let config_path = code_home.path().join(CONFIG_TOML_FILE);

        tokio::fs::write(
            &config_path,
            r#"
model = "gpt-5.1-codex"
model_reasoning_effort = "medium"

[profiles.dev]
model = "gpt-4.1"
"#,
        )
        .await?;

        persist_model_selection(
            code_home.path(),
            None,
            "o4-mini",
            Some(ReasoningEffort::High),
            None,
        )
        .await?;

        let serialized = tokio::fs::read_to_string(config_path).await?;
        let parsed: ConfigToml = toml::from_str(&serialized)?;

        assert_eq!(parsed.model.as_deref(), Some("o4-mini"));
        assert_eq!(parsed.model_reasoning_effort, Some(ReasoningEffort::High));
        assert_eq!(
            parsed
                .profiles
                .get("dev")
                .and_then(|profile| profile.model.as_deref()),
            Some("gpt-4.1"),
        );

        Ok(())
    }

    #[tokio::test]
    async fn persist_model_selection_updates_profile() -> anyhow::Result<()> {
        let code_home = TempDir::new()?;

        persist_model_selection(
            code_home.path(),
            Some("dev"),
            "gpt-5.1-codex",
            Some(ReasoningEffort::Medium),
            None,
        )
        .await?;

        let serialized =
            tokio::fs::read_to_string(code_home.path().join(CONFIG_TOML_FILE)).await?;
        let parsed: ConfigToml = toml::from_str(&serialized)?;
        let profile = parsed
            .profiles
            .get("dev")
            .expect("profile should be created");

        assert_eq!(profile.model.as_deref(), Some("gpt-5.1-codex"));
        assert_eq!(
            profile.model_reasoning_effort,
            Some(ReasoningEffort::Medium)
        );

        Ok(())
    }

    #[tokio::test]
    async fn persist_model_selection_updates_existing_profile() -> anyhow::Result<()> {
        let code_home = TempDir::new()?;
        let config_path = code_home.path().join(CONFIG_TOML_FILE);

        tokio::fs::write(
            &config_path,
            r#"
[profiles.dev]
model = "gpt-4"
model_reasoning_effort = "medium"

[profiles.prod]
model = "gpt-5.1-codex"
"#,
        )
        .await?;

        persist_model_selection(
            code_home.path(),
            Some("dev"),
            "o4-high",
            Some(ReasoningEffort::Medium),
            None,
        )
        .await?;

        let serialized = tokio::fs::read_to_string(config_path).await?;
        let parsed: ConfigToml = toml::from_str(&serialized)?;

        let dev_profile = parsed
            .profiles
            .get("dev")
            .expect("dev profile should survive updates");
        assert_eq!(dev_profile.model.as_deref(), Some("o4-high"));
        assert_eq!(
            dev_profile.model_reasoning_effort,
            Some(ReasoningEffort::Medium)
        );

        assert_eq!(
            parsed
                .profiles
                .get("prod")
                .and_then(|profile| profile.model.as_deref()),
            Some("gpt-5.1-codex"),
        );

        Ok(())
    }
    struct PrecedenceTestFixture {
        cwd: TempDir,
        code_home: TempDir,
        cfg: ConfigToml,
        model_provider_map: HashMap<String, ModelProviderInfo>,
        openai_provider: ModelProviderInfo,
        openai_chat_completions_provider: ModelProviderInfo,
    }

    impl PrecedenceTestFixture {
        fn cwd(&self) -> PathBuf {
            self.cwd.path().to_path_buf()
        }

        fn code_home(&self) -> PathBuf {
            self.code_home.path().to_path_buf()
        }
    }

    fn create_test_fixture() -> std::io::Result<PrecedenceTestFixture> {
        let toml = r#"
model = "o3"
approval_policy = "untrusted"
disable_response_storage = false

# Can be used to determine which profile to use if not specified by
# `ConfigOverrides`.
profile = "gpt3"

[model_providers.openai-chat-completions]
name = "OpenAI using Chat Completions"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "chat"
request_max_retries = 4            # retry failed HTTP requests
stream_max_retries = 10            # retry dropped SSE streams
stream_idle_timeout_ms = 300000    # 5m idle timeout

[profiles.o3]
model = "o3"
model_provider = "openai"
approval_policy = "never"
model_reasoning_effort = "high"
model_reasoning_summary = "detailed"

[profiles.gpt3]
model = "gpt-3.5-turbo"
model_provider = "openai-chat-completions"

[profiles.zdr]
model = "o3"
model_provider = "openai"
approval_policy = "on-failure"
disable_response_storage = true

[profiles.gpt5]
model = "gpt-5.1"
model_provider = "openai"
approval_policy = "on-failure"
model_reasoning_effort = "high"
model_reasoning_summary = "detailed"
model_verbosity = "high"
"#;

        let cfg: ConfigToml = toml::from_str(toml).expect("TOML deserialization should succeed");

        // Use a temporary directory for the cwd so it does not contain an
        // AGENTS.md file.
        let cwd_temp_dir = TempDir::new().unwrap();
        let cwd = cwd_temp_dir.path().to_path_buf();
        // Make it look like a Git repo so it does not search for AGENTS.md in
        // a parent folder, either.
        std::fs::write(cwd.join(".git"), "gitdir: nowhere")?;

        let code_home_temp_dir = TempDir::new().unwrap();

        let openai_chat_completions_provider = ModelProviderInfo {
            name: "OpenAI using Chat Completions".to_string(),
            base_url: Some("https://api.openai.com/v1".to_string()),
            env_key: Some("OPENAI_API_KEY".to_string()),
            wire_api: crate::WireApi::Chat,
            env_key_instructions: None,
            experimental_bearer_token: None,
            query_params: None,
            http_headers: None,
            env_http_headers: None,
            request_max_retries: Some(4),
            stream_max_retries: Some(10),
            stream_idle_timeout_ms: Some(300_000),
            requires_openai_auth: false,
            openrouter: None,
        };
        let model_provider_map = {
            let mut model_provider_map = built_in_model_providers();
            model_provider_map.insert(
                "openai-chat-completions".to_string(),
                openai_chat_completions_provider.clone(),
            );
            model_provider_map
        };

        let openai_provider = model_provider_map
            .get("openai")
            .expect("openai provider should exist")
            .clone();

        Ok(PrecedenceTestFixture {
            cwd: cwd_temp_dir,
            code_home: code_home_temp_dir,
            cfg,
            model_provider_map,
            openai_provider,
            openai_chat_completions_provider,
        })
    }

    /// Users can specify config values at multiple levels that have the
    /// following precedence:
    ///
    /// 1. custom command-line argument, e.g. `--model o3`
    /// 2. as part of a profile, where the `--profile` is specified via a CLI
    ///    (or in the config file itself)
    /// 3. as an entry in `config.toml`, e.g. `model = "o3"`
    /// 4. the default value for a required field defined in code, e.g.,
    ///    `crate::flags::OPENAI_DEFAULT_MODEL`
    ///
    /// Note that profiles are the recommended way to specify a group of
    /// configuration options together.
    #[test]
    fn test_precedence_fixture_with_o3_profile() -> std::io::Result<()> {
        let fixture = create_test_fixture()?;

        let o3_profile_overrides = ConfigOverrides {
            config_profile: Some("o3".to_string()),
            cwd: Some(fixture.cwd()),
            ..Default::default()
        };
        let o3_profile_config: Config = Config::load_from_base_config_with_overrides(
            fixture.cfg.clone(),
            o3_profile_overrides,
            fixture.code_home(),
        )?;
        assert_eq!("o3", o3_profile_config.model);
        assert_eq!(OPENAI_DEFAULT_REVIEW_MODEL, o3_profile_config.review_model);
        assert_eq!(
            ReasoningEffort::High,
            o3_profile_config.review_model_reasoning_effort
        );
        assert_eq!(
            find_family_for_model("o3").expect("known model slug"),
            o3_profile_config.model_family
        );
        assert_eq!(Some(200_000), o3_profile_config.model_context_window);
        assert_eq!(Some(100_000), o3_profile_config.model_max_output_tokens);
        assert_eq!("openai", o3_profile_config.model_provider_id);
        assert_eq!(fixture.openai_provider, o3_profile_config.model_provider);
        assert_eq!(AskForApproval::Never, o3_profile_config.approval_policy);
        assert_eq!(
            SandboxPolicy::new_read_only_policy(),
            o3_profile_config.sandbox_policy
        );
        assert_eq!(fixture.cwd(), o3_profile_config.cwd);
        assert_eq!(fixture.code_home(), o3_profile_config.code_home);
        assert_eq!(
            &fixture.model_provider_map,
            &o3_profile_config.model_providers
        );
        assert_eq!(ReasoningEffort::High, o3_profile_config.model_reasoning_effort);
        assert_eq!(ReasoningSummary::Detailed, o3_profile_config.model_reasoning_summary);
        assert_eq!(TextVerbosity::default(), o3_profile_config.model_text_verbosity);
        assert!(!o3_profile_config.disable_response_storage);
        assert_eq!(UriBasedFileOpener::VsCode, o3_profile_config.file_opener);
        assert_eq!(Tui::default(), o3_profile_config.tui);
        assert!(!o3_profile_config.hide_agent_reasoning);
        assert!(!o3_profile_config.show_raw_agent_reasoning);
        assert_eq!(
            crate::config_types::OtelConfig::default(),
            o3_profile_config.otel
        );
        Ok(())
    }

    #[test]
    fn test_precedence_fixture_with_gpt3_profile() -> std::io::Result<()> {
        let fixture = create_test_fixture()?;

        let gpt3_profile_overrides = ConfigOverrides {
            config_profile: Some("gpt3".to_string()),
            cwd: Some(fixture.cwd()),
            ..Default::default()
        };
        let gpt3_profile_config = Config::load_from_base_config_with_overrides(
            fixture.cfg.clone(),
            gpt3_profile_overrides,
            fixture.code_home(),
        )?;
        assert_eq!("gpt-3.5-turbo", gpt3_profile_config.model);
        assert_eq!(OPENAI_DEFAULT_REVIEW_MODEL, gpt3_profile_config.review_model);
        assert_eq!(
            ReasoningEffort::High,
            gpt3_profile_config.review_model_reasoning_effort
        );
        assert_eq!(
            find_family_for_model("gpt-3.5-turbo").expect("known model slug"),
            gpt3_profile_config.model_family
        );
        assert_eq!(Some(16_385), gpt3_profile_config.model_context_window);
        assert_eq!(Some(4_096), gpt3_profile_config.model_max_output_tokens);
        assert_eq!("openai-chat-completions", gpt3_profile_config.model_provider_id);
        assert_eq!(
            fixture.openai_chat_completions_provider,
            gpt3_profile_config.model_provider
        );
        assert_eq!(Some("gpt3".to_string()), gpt3_profile_config.active_profile);
        assert_eq!(AskForApproval::UnlessTrusted, gpt3_profile_config.approval_policy);
        assert_eq!(
            SandboxPolicy::new_read_only_policy(),
            gpt3_profile_config.sandbox_policy
        );
        assert_eq!(fixture.cwd(), gpt3_profile_config.cwd);
        assert_eq!(fixture.code_home(), gpt3_profile_config.code_home);
        assert_eq!(
            &fixture.model_provider_map,
            &gpt3_profile_config.model_providers
        );
        assert_eq!(ReasoningEffort::default(), gpt3_profile_config.model_reasoning_effort);
        assert_eq!(ReasoningSummary::default(), gpt3_profile_config.model_reasoning_summary);
        assert_eq!(TextVerbosity::default(), gpt3_profile_config.model_text_verbosity);
        assert!(!gpt3_profile_config.disable_response_storage);

        // Verify that loading without specifying a profile in ConfigOverrides
        // uses the default profile from the config file (which is "gpt3").
        let default_profile_overrides = ConfigOverrides {
            cwd: Some(fixture.cwd()),
            ..Default::default()
        };

        let default_profile_config = Config::load_from_base_config_with_overrides(
            fixture.cfg.clone(),
            default_profile_overrides,
            fixture.code_home(),
        )?;

        assert_eq!(gpt3_profile_config.model, default_profile_config.model);
        assert_eq!(gpt3_profile_config.active_profile, default_profile_config.active_profile);
        assert_eq!(gpt3_profile_config.model_provider_id, default_profile_config.model_provider_id);
        assert_eq!(gpt3_profile_config.approval_policy, default_profile_config.approval_policy);
        assert_eq!(gpt3_profile_config.sandbox_policy, default_profile_config.sandbox_policy);
        Ok(())
    }

    #[test]
    fn test_precedence_fixture_with_zdr_profile() -> std::io::Result<()> {
        let fixture = create_test_fixture()?;

        let zdr_profile_overrides = ConfigOverrides {
            config_profile: Some("zdr".to_string()),
            cwd: Some(fixture.cwd()),
            ..Default::default()
        };
        let zdr_profile_config = Config::load_from_base_config_with_overrides(
            fixture.cfg.clone(),
            zdr_profile_overrides,
            fixture.code_home(),
        )?;
        assert_eq!("o3", zdr_profile_config.model);
        assert_eq!(OPENAI_DEFAULT_REVIEW_MODEL, zdr_profile_config.review_model);
        assert_eq!(
            ReasoningEffort::High,
            zdr_profile_config.review_model_reasoning_effort
        );
        assert_eq!(
            find_family_for_model("o3").expect("known model slug"),
            zdr_profile_config.model_family
        );
        assert_eq!(Some(200_000), zdr_profile_config.model_context_window);
        assert_eq!(Some(100_000), zdr_profile_config.model_max_output_tokens);
        assert_eq!("openai", zdr_profile_config.model_provider_id);
        assert_eq!(fixture.openai_provider, zdr_profile_config.model_provider);
        assert_eq!(Some("zdr".to_string()), zdr_profile_config.active_profile);
        assert_eq!(AskForApproval::OnFailure, zdr_profile_config.approval_policy);
        assert_eq!(
            SandboxPolicy::new_read_only_policy(),
            zdr_profile_config.sandbox_policy
        );
        assert!(zdr_profile_config.disable_response_storage);
        assert_eq!(fixture.cwd(), zdr_profile_config.cwd);
        assert_eq!(fixture.code_home(), zdr_profile_config.code_home);
        assert_eq!(
            &fixture.model_provider_map,
            &zdr_profile_config.model_providers
        );

        Ok(())
    }

    #[test]
    fn test_precedence_fixture_with_gpt5_profile() -> std::io::Result<()> {
        let fixture = create_test_fixture()?;

        let gpt5_profile_overrides = ConfigOverrides {
            config_profile: Some("gpt5".to_string()),
            cwd: Some(fixture.cwd()),
            ..Default::default()
        };
        let gpt5_profile_config = Config::load_from_base_config_with_overrides(
            fixture.cfg.clone(),
            gpt5_profile_overrides,
            fixture.code_home(),
        )?;
        assert_eq!("gpt-5.2", gpt5_profile_config.model);
        assert_eq!(OPENAI_DEFAULT_REVIEW_MODEL, gpt5_profile_config.review_model);
        assert_eq!(
            ReasoningEffort::High,
            gpt5_profile_config.review_model_reasoning_effort
        );
        assert_eq!(
            find_family_for_model("gpt-5.2").expect("known model slug"),
            gpt5_profile_config.model_family
        );
        assert!(gpt5_profile_config.model_context_window.is_some());
        assert!(gpt5_profile_config.model_max_output_tokens.is_some());
        assert_eq!("openai", gpt5_profile_config.model_provider_id);
        assert_eq!(fixture.openai_provider, gpt5_profile_config.model_provider);
        assert_eq!(Some("gpt5".to_string()), gpt5_profile_config.active_profile);
        assert_eq!(AskForApproval::OnFailure, gpt5_profile_config.approval_policy);
        assert_eq!(
            SandboxPolicy::new_read_only_policy(),
            gpt5_profile_config.sandbox_policy
        );
        assert!(matches!(
            gpt5_profile_config.model_reasoning_effort,
            ReasoningEffort::Medium | ReasoningEffort::High
        ));
        assert_eq!(ReasoningSummary::Detailed, gpt5_profile_config.model_reasoning_summary);
        assert!(matches!(
            gpt5_profile_config.model_text_verbosity,
            TextVerbosity::Medium | TextVerbosity::High
        ));
        assert_eq!(fixture.cwd(), gpt5_profile_config.cwd);
        assert_eq!(fixture.code_home(), gpt5_profile_config.code_home);
        assert_eq!(
            &fixture.model_provider_map,
            &gpt5_profile_config.model_providers
        );

        Ok(())
    }

    #[test]
    fn planning_defaults_to_chat_model_when_not_overridden() -> std::io::Result<()> {
        let fixture = create_test_fixture()?;

        let overrides = ConfigOverrides {
            cwd: Some(fixture.cwd()),
            ..Default::default()
        };

        let config = Config::load_from_base_config_with_overrides(
            fixture.cfg.clone(),
            overrides,
            fixture.code_home(),
        )?;

        assert!(config.planning_use_chat_model);
        assert_eq!(config.planning_model, config.model);
        assert_eq!(config.planning_model_reasoning_effort, config.model_reasoning_effort);
        Ok(())
    }

    #[test]
    fn planning_follow_chat_overrides_reasoning_override() -> std::io::Result<()> {
        let fixture = create_test_fixture()?;
        let mut cfg = fixture.cfg.clone();
        cfg.planning_use_chat_model = Some(true);
        cfg.planning_model_reasoning_effort = Some(ReasoningEffort::High);
        cfg.model_reasoning_effort = Some(ReasoningEffort::Low);

        let overrides = ConfigOverrides {
            cwd: Some(fixture.cwd()),
            ..Default::default()
        };

        let config = Config::load_from_base_config_with_overrides(
            cfg,
            overrides,
            fixture.code_home(),
        )?;

        assert!(config.planning_use_chat_model);
        assert_eq!(config.planning_model, config.model);
        assert_eq!(config.model_reasoning_effort, ReasoningEffort::Low);
        assert_eq!(config.planning_model_reasoning_effort, ReasoningEffort::Low);
        Ok(())
    }

    #[test]
    fn review_follow_chat_preserves_override_on_toggle() -> anyhow::Result<()> {
        let code_home = tempfile::TempDir::new()?;
        std::fs::write(
            code_home.path().join(CONFIG_TOML_FILE),
            format!(
                "review_model = \"custom-review\"\nreview_model_reasoning_effort = \"{}\"\n",
                ReasoningEffort::High.to_string().to_ascii_lowercase()
            ),
        )?;

        set_review_model(
            code_home.path(),
            "chat-model-unused",
            ReasoningEffort::Low,
            true,
        )?;

        let written = std::fs::read_to_string(code_home.path().join(CONFIG_TOML_FILE))?;
        let parsed: ConfigToml = toml::from_str(&written)?;

        assert!(parsed.review_use_chat_model);
        assert_eq!(parsed.review_model.as_deref(), Some("custom-review"));
        assert_eq!(parsed.review_model_reasoning_effort, Some(ReasoningEffort::High));
        Ok(())
    }

    #[test]
    fn planning_follow_chat_preserves_override_on_toggle() -> anyhow::Result<()> {
        let code_home = tempfile::TempDir::new()?;
        std::fs::write(
            code_home.path().join(CONFIG_TOML_FILE),
            format!(
                "planning_model = \"custom-plan\"\nplanning_model_reasoning_effort = \"{}\"\n",
                ReasoningEffort::High.to_string().to_ascii_lowercase()
            ),
        )?;

        set_planning_model(
            code_home.path(),
            "chat-model-unused",
            ReasoningEffort::Low,
            true,
        )?;

        let written = std::fs::read_to_string(code_home.path().join(CONFIG_TOML_FILE))?;
        let parsed: ConfigToml = toml::from_str(&written)?;

        assert!(parsed.planning_use_chat_model.unwrap_or(false));
        assert_eq!(parsed.planning_model.as_deref(), Some("custom-plan"));
        assert_eq!(
            parsed.planning_model_reasoning_effort,
            Some(ReasoningEffort::High)
        );
        Ok(())
    }

    #[test]
    fn auto_drive_follow_chat_model_applied_on_load() -> std::io::Result<()> {
        let fixture = create_test_fixture()?;
        let mut cfg = fixture.cfg.clone();
        cfg.auto_drive_use_chat_model = Some(true);

        let overrides = ConfigOverrides {
            cwd: Some(fixture.cwd()),
            ..Default::default()
        };

        let config = Config::load_from_base_config_with_overrides(
            cfg,
            overrides,
            fixture.code_home(),
        )?;

        assert!(config.auto_drive_use_chat_model);
        assert_eq!(config.auto_drive.model, config.model);
        assert_eq!(config.auto_drive.model_reasoning_effort, config.model_reasoning_effort);
        Ok(())
    }

    #[test]
    fn review_follow_chat_model_applies_reasoning_on_load() -> std::io::Result<()> {
        let fixture = create_test_fixture()?;
        let mut cfg = fixture.cfg.clone();
        cfg.review_use_chat_model = true;
        cfg.review_model_reasoning_effort = Some(ReasoningEffort::High);

        let overrides = ConfigOverrides {
            cwd: Some(fixture.cwd()),
            ..Default::default()
        };

        let config = Config::load_from_base_config_with_overrides(
            cfg,
            overrides,
            fixture.code_home(),
        )?;

        assert!(config.review_use_chat_model);
        assert_eq!(config.review_model, config.model);
        assert_eq!(config.review_model_reasoning_effort, config.model_reasoning_effort);
        Ok(())
    }

    #[test]
    fn upgrade_legacy_model_slugs_updates_top_level() {
        let mut cfg = ConfigToml {
            model: Some("gpt-5-codex".to_string()),
            review_model: Some("gpt-5".to_string()),
            ..Default::default()
        };

        upgrade_legacy_model_slugs(&mut cfg);

        assert_eq!(cfg.model.as_deref(), Some("gpt-5.1-codex"));
        assert_eq!(cfg.review_model.as_deref(), Some("gpt-5.2"));
    }

    #[test]
    fn upgrade_legacy_model_slugs_updates_profiles() {
        let mut cfg = ConfigToml::default();
        cfg.profiles.insert(
            "legacy".to_string(),
            ConfigProfile {
                model: Some("test-gpt-5-codex".to_string()),
                review_model: Some("gpt-5-codex".to_string()),
                ..Default::default()
            },
        );

        upgrade_legacy_model_slugs(&mut cfg);

        let legacy = cfg.profiles.get("legacy").expect("profile exists");
        assert_eq!(legacy.model.as_deref(), Some("test-gpt-5.1-codex"));
        assert_eq!(legacy.review_model.as_deref(), Some("gpt-5.1-codex"));
    }

    #[test]
    fn test_compact_prompt_override_prefers_cli_string() -> std::io::Result<()> {
        let fixture = create_test_fixture()?;
        let mut cfg = fixture.cfg.clone();
        cfg.compact_prompt_override = Some("config prompt".to_string());

        let overrides = ConfigOverrides {
            cwd: Some(fixture.cwd()),
            compact_prompt_override: Some("cli prompt".to_string()),
            ..Default::default()
        };

        let resolved = Config::load_from_base_config_with_overrides(
            cfg,
            overrides,
            fixture.code_home(),
        )?;

        assert_eq!(resolved.compact_prompt_override.as_deref(), Some("cli prompt"));
        Ok(())
    }

    #[test]
    fn test_compact_prompt_override_file_populates_string() -> std::io::Result<()> {
        let fixture = create_test_fixture()?;
        let mut file = NamedTempFile::new()?;
        let file_contents = "file based compact prompt";
        std::io::Write::write_all(&mut file, file_contents.as_bytes())?;

        let overrides = ConfigOverrides {
            cwd: Some(fixture.cwd()),
            compact_prompt_override_file: Some(file.path().to_path_buf()),
            ..Default::default()
        };

        let resolved = Config::load_from_base_config_with_overrides(
            fixture.cfg.clone(),
            overrides,
            fixture.code_home(),
        )?;

        assert_eq!(
            resolved.compact_prompt_override.as_deref(),
            Some(file_contents)
        );
        Ok(())
    }

    #[test]
    fn test_custom_agent_entries_extend_defaults() -> std::io::Result<()> {
        let fixture = create_test_fixture()?;

        let mut cfg = fixture.cfg.clone();
        cfg.agents = vec![AgentConfig {
            name: "code-gpt-5.2-codex".to_string(),
            command: String::new(),
            args: Vec::new(),
            read_only: false,
            enabled: true,
            description: None,
            env: None,
            args_read_only: None,
            args_write: None,
            instructions: None,
        }];

        let overrides = ConfigOverrides {
            cwd: Some(fixture.cwd()),
            ..Default::default()
        };

        let loaded = Config::load_from_base_config_with_overrides(
            cfg,
            overrides,
            fixture.code_home(),
        )?;

        let enabled_names: std::collections::HashSet<String> = loaded
            .agents
            .into_iter()
            .filter(|agent| agent.enabled)
            .map(|agent| agent.name.to_ascii_lowercase())
            .collect();

        assert!(enabled_names.contains("code-gpt-5.2-codex"));
        assert!(enabled_names.contains("code-gpt-5.2"));
        assert!(enabled_names.contains("claude-sonnet-4.5"));
        assert!(enabled_names.contains("gemini-3-pro"));
        assert!(enabled_names.contains("qwen-3-coder"));
        Ok(())
    }

    #[test]
    fn test_set_project_trusted_writes_explicit_tables() -> anyhow::Result<()> {
        let code_home = TempDir::new().unwrap();
        let project_dir = TempDir::new().unwrap();

        // Call the function under test
        set_project_trusted(code_home.path(), project_dir.path())?;

        // Read back the generated config.toml and verify the trusted entry
        let config_path = code_home.path().join(CONFIG_TOML_FILE);
        let contents = std::fs::read_to_string(&config_path)?;

        let raw_path = project_dir.path().to_string_lossy();
        let parsed: toml::Value = toml::from_str(&contents)?;
        let table = parsed
            .as_table()
            .ok_or_else(|| anyhow::anyhow!("config should be a table"))?;
        let projects = table
            .get("projects")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| anyhow::anyhow!("projects table missing"))?;
        let entry = projects
            .get(raw_path.as_ref())
            .and_then(toml::Value::as_table)
            .ok_or_else(|| anyhow::anyhow!("project entry missing"))?;
        assert_eq!(
            entry
                .get("trust_level")
                .and_then(toml::Value::as_str),
            Some("trusted")
        );

        Ok(())
    }

    #[test]
    fn test_set_project_trusted_converts_inline_to_explicit() -> anyhow::Result<()> {
        let code_home = TempDir::new().unwrap();
        let project_dir = TempDir::new().unwrap();

        // Seed config.toml with an inline project entry under [projects]
        let config_path = code_home.path().join(CONFIG_TOML_FILE);
        let raw_path = project_dir.path().to_string_lossy();
        let path_str = if raw_path.contains('\\') {
            format!("'{raw_path}'")
        } else {
            format!("\"{raw_path}\"")
        };
        // Use a quoted key so backslashes don't require escaping on Windows
        let initial = format!(
            r#"[projects]
{path_str} = {{ trust_level = "untrusted" }}
"#
        );
        std::fs::create_dir_all(code_home.path())?;
        std::fs::write(&config_path, initial)?;

        // Run the function; it should convert to explicit tables and set trusted
        set_project_trusted(code_home.path(), project_dir.path())?;

        let contents = std::fs::read_to_string(&config_path)?;

        let parsed: toml::Value = toml::from_str(&contents)?;
        let table = parsed
            .as_table()
            .ok_or_else(|| anyhow::anyhow!("config should be a table"))?;
        let projects = table
            .get("projects")
            .and_then(toml::Value::as_table)
            .ok_or_else(|| anyhow::anyhow!("projects table missing"))?;
        let entry = projects
            .get(raw_path.as_ref())
            .and_then(toml::Value::as_table)
            .ok_or_else(|| anyhow::anyhow!("project entry missing"))?;
        assert_eq!(
            entry
                .get("trust_level")
                .and_then(toml::Value::as_str),
            Some("trusted")
        );

        Ok(())
    }

    // No test enforcing the presence of a standalone [projects] header.
}

#[cfg(test)]
mod agent_merge_tests {
    use super::merge_with_default_agents;
    use crate::config_types::AgentConfig;

    fn agent(name: &str, command: &str, enabled: bool) -> AgentConfig {
        AgentConfig {
            name: name.to_string(),
            command: command.to_string(),
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
    fn disabled_codex_mini_alias_is_preserved() {
        let agents = vec![agent("codex-mini", "coder", false)];
        let merged = merge_with_default_agents(agents);

        let mini = merged
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("code-gpt-5.1-codex-mini"))
            .expect("mini present");

        assert!(!mini.enabled, "disabled state should persist for alias");
        assert_eq!(
            merged
                .iter()
                .filter(|a| a.name.eq_ignore_ascii_case("code-gpt-5.1-codex-mini"))
                .count(),
            1,
            "should dedupe alias/canonical"
        );
    }

    #[test]
    fn disabled_codex_mini_slug_is_preserved_with_command() {
        let agents = vec![agent("code-gpt-5.1-codex-mini", "coder", false)];
        let merged = merge_with_default_agents(agents);

        let mini = merged
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("code-gpt-5.1-codex-mini"))
            .expect("mini present");

        assert!(!mini.enabled, "disabled state should persist for canonical slug");
        assert_eq!(
            merged
                .iter()
                .filter(|a| a.name.eq_ignore_ascii_case("code-gpt-5.1-codex-mini"))
                .count(),
            1,
            "should dedupe canonical entry"
        );
    }

    #[test]
    fn codex_mini_alias_then_canonical_last_wins_disabled() {
        let agents = vec![
            agent("codex-mini", "coder", true),
            agent("code-gpt-5.1-codex-mini", "coder", false),
        ];
        let merged = merge_with_default_agents(agents);

        let mini = merged
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("code-gpt-5.1-codex-mini"))
            .expect("mini present");

        assert!(!mini.enabled, "later canonical disable should win");
        assert_eq!(
            merged
                .iter()
                .filter(|a| a.name.eq_ignore_ascii_case("code-gpt-5.1-codex-mini"))
                .count(),
            1,
            "should dedupe alias and canonical"
        );
    }

    #[test]
    fn codex_mini_canonical_then_alias_last_wins_disabled() {
        let agents = vec![
            agent("code-gpt-5.1-codex-mini", "coder", true),
            agent("codex-mini", "coder", false),
        ];
        let merged = merge_with_default_agents(agents);

        let mini = merged
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("code-gpt-5.1-codex-mini"))
            .expect("mini present");

        assert!(!mini.enabled, "later alias disable should win");
        assert_eq!(
            merged
                .iter()
                .filter(|a| a.name.eq_ignore_ascii_case("code-gpt-5.1-codex-mini"))
                .count(),
            1,
            "should dedupe alias and canonical"
        );
    }

    #[test]
    fn gemini_alias_and_canonical_dedupe_prefers_last_state() {
        let agents = vec![
            agent("gemini-2.5-pro", "gemini", true),
            agent("gemini-3-pro", "gemini", false),
        ];
        let merged = merge_with_default_agents(agents);

        let gemini = merged
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("gemini-3-pro"))
            .expect("gemini present");

        assert!(!gemini.enabled, "later canonical disable should win");
        assert_eq!(
            merged
                .iter()
                .filter(|a| a.name.eq_ignore_ascii_case("gemini-3-pro"))
                .count(),
            1,
            "should dedupe gemini alias/canonical"
        );
    }

    #[test]
    fn gemini_alias_disable_overrides_prior_canonical_enable() {
        let agents = vec![
            agent("gemini-3-pro", "gemini", true),
            agent("gemini-2.5-pro", "gemini", false),
        ];
        let merged = merge_with_default_agents(agents);

        let gemini = merged
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case("gemini-3-pro"))
            .expect("gemini present");

        assert!(!gemini.enabled, "later alias disable should win");
        assert_eq!(
            merged
                .iter()
                .filter(|a| a.name.eq_ignore_ascii_case("gemini-3-pro"))
                .count(),
            1,
            "should dedupe gemini alias/canonical"
        );
    }
}

#[cfg(test)]
mod notifications_tests {
    use crate::config_types::Notifications;
    use serde::Deserialize;

    #[derive(Deserialize, Debug, PartialEq)]
    struct TuiTomlTest {
        notifications: Notifications,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct RootTomlTest {
        tui: TuiTomlTest,
    }

    #[test]
    fn test_tui_notifications_true() {
        let toml = r#"
            [tui]
            notifications = true
        "#;
        let parsed: RootTomlTest = toml::from_str(toml).expect("deserialize notifications=true");
        assert!(matches!(
            parsed.tui.notifications,
            Notifications::Enabled(true)
        ));
    }

    #[test]
    fn test_tui_notifications_custom_array() {
        let toml = r#"
            [tui]
            notifications = ["foo"]
        "#;
        let parsed: RootTomlTest =
            toml::from_str(toml).expect("deserialize notifications=[\"foo\"]");
        assert!(matches!(
            parsed.tui.notifications,
            Notifications::Custom(ref v) if v == &vec!["foo".to_string()]
        ));
    }
}
