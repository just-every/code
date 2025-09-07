//! Types used to define the fields of [`crate::config::Config`].

// Note this file should generally be restricted to simple struct/enum
// definitions that do not contain business logic.

use std::collections::HashMap;
use std::path::PathBuf;
use wildmatch::WildMatchPattern;

use serde::Deserialize;
use serde::Serialize;
use strum_macros::Display;

/// Configuration for external agent models
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct AgentConfig {
    /// Name of the agent (e.g., "claude", "gemini", "gpt-4")
    pub name: String,

    /// Command to execute the agent (e.g., "claude", "gemini")
    pub command: String,

    /// Optional arguments to pass to the agent command
    #[serde(default)]
    pub args: Vec<String>,

    /// Whether this agent can only run in read-only mode
    #[serde(default)]
    pub read_only: bool,

    /// Whether this agent is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Optional description of the agent
    #[serde(default)]
    pub description: Option<String>,

    /// Optional environment variables for the agent
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
}

fn default_true() -> bool {
    true
}

/// GitHub integration settings.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct GithubConfig {
    /// When true, Codex watches for GitHub Actions workflow runs after a
    /// successful `git push` and reports failures as background messages.
    /// Enabled by default; can be disabled via `~/.code/config.toml` under
    /// `[github]` with `check_workflows_on_push = false`.
    #[serde(default = "default_true")]
    pub check_workflows_on_push: bool,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct McpServerConfig {
    pub command: String,

    #[serde(default)]
    pub args: Vec<String>,

    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
}

#[derive(Deserialize, Debug, Copy, Clone, PartialEq)]
pub enum UriBasedFileOpener {
    #[serde(rename = "vscode")]
    VsCode,

    #[serde(rename = "vscode-insiders")]
    VsCodeInsiders,

    #[serde(rename = "windsurf")]
    Windsurf,

    #[serde(rename = "cursor")]
    Cursor,

    /// Option to disable the URI-based file opener.
    #[serde(rename = "none")]
    None,
}

impl UriBasedFileOpener {
    pub fn get_scheme(&self) -> Option<&str> {
        match self {
            UriBasedFileOpener::VsCode => Some("vscode"),
            UriBasedFileOpener::VsCodeInsiders => Some("vscode-insiders"),
            UriBasedFileOpener::Windsurf => Some("windsurf"),
            UriBasedFileOpener::Cursor => Some("cursor"),
            UriBasedFileOpener::None => None,
        }
    }
}

/// Settings that govern if and what will be written to `~/.codex/history.jsonl`.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct History {
    /// If true, history entries will not be written to disk.
    pub persistence: HistoryPersistence,

    /// If set, the maximum size of the history file in bytes.
    /// TODO(mbolin): Not currently honored.
    pub max_bytes: Option<usize>,
}

#[derive(Deserialize, Debug, Copy, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum HistoryPersistence {
    /// Save all history entries to disk.
    #[default]
    SaveAll,
    /// Do not write history to disk.
    None,
}

/// Collection of settings that are specific to the TUI.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Tui {
    /// Theme configuration for the TUI
    #[serde(default)]
    pub theme: ThemeConfig,

    /// Syntax highlighting configuration (Markdown fenced code blocks)
    #[serde(default)]
    pub highlight: HighlightConfig,

    /// Whether to show reasoning content expanded by default (can be toggled with Ctrl+R/T)
    #[serde(default)]
    pub show_reasoning: bool,

    /// Streaming/animation behavior for assistant/reasoning output
    #[serde(default)]
    pub stream: StreamConfig,
}

/// Streaming behavior configuration for the TUI.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct StreamConfig {
    /// Emit the Answer header immediately when a stream begins (before first newline).
    #[serde(default)]
    pub answer_header_immediate: bool,

    /// Show an ellipsis placeholder in the Answer body while waiting for first text.
    #[serde(default = "default_true")]
    pub show_answer_ellipsis: bool,

    /// Commit animation pacing in milliseconds (lines per CommitTick).
    /// If unset, defaults to 50ms; in responsive profile, defaults to 30ms.
    #[serde(default)]
    pub commit_tick_ms: Option<u64>,

    /// Soft-commit timeout (ms) when no newline arrives; commits partial content.
    /// If unset, disabled; in responsive profile, defaults to 400ms.
    #[serde(default)]
    pub soft_commit_timeout_ms: Option<u64>,

    /// Soft-commit when this many chars have streamed without a newline.
    /// If unset, disabled; in responsive profile, defaults to 160 chars.
    #[serde(default)]
    pub soft_commit_chars: Option<usize>,

    /// Relax list hold-back: allow list lines with content; only withhold bare markers.
    #[serde(default)]
    pub relax_list_holdback: bool,

    /// Relax code hold-back: allow committing inside an open fenced code block
    /// except the very last partial line.
    #[serde(default)]
    pub relax_code_holdback: bool,

    /// Convenience switch enabling a snappier preset for the above values.
    /// Explicit values above still take precedence if set.
    #[serde(default)]
    pub responsive: bool,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            answer_header_immediate: false,
            show_answer_ellipsis: true,
            commit_tick_ms: None,
            soft_commit_timeout_ms: None,
            soft_commit_chars: None,
            relax_list_holdback: false,
            relax_code_holdback: false,
            responsive: false,
        }
    }
}

/// Theme configuration for the TUI
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct ThemeConfig {
    /// Name of the predefined theme to use
    #[serde(default)]
    pub name: ThemeName,

    /// Custom color overrides (optional)
    #[serde(default)]
    pub colors: ThemeColors,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: ThemeName::default(),
            colors: ThemeColors::default(),
        }
    }
}

/// Configuration for syntax highlighting in Markdown code blocks.
///
/// `theme` accepts the following values:
/// - "auto" (default): choose a sensible built-in syntect theme based on
///   whether the current UI theme is light or dark.
/// - "<name>": use a specific syntect theme by name from the default ThemeSet.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct HighlightConfig {
    /// Theme selection preference (see docstring for accepted values)
    #[serde(default)]
    pub theme: Option<String>,
}

/// Available predefined themes
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeName {
    // Light themes (at top)
    #[default]
    LightPhoton,
    LightPrismRainbow,
    LightVividTriad,
    LightPorcelain,
    LightSandbar,
    LightGlacier,
    // Dark themes (below)
    DarkCarbonNight,
    DarkShinobiDusk,
    DarkOledBlackPro,
    DarkAmberTerminal,
    DarkAuroraFlux,
    DarkCharcoalRainbow,
    DarkZenGarden,
    DarkPaperLightPro,
    Custom,
}

/// Theme colors that can be customized
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ThemeColors {
    // Primary colors
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub background: Option<String>,
    pub foreground: Option<String>,

    // UI elements
    pub border: Option<String>,
    pub border_focused: Option<String>,
    pub selection: Option<String>,
    pub cursor: Option<String>,

    // Status colors
    pub success: Option<String>,
    pub warning: Option<String>,
    pub error: Option<String>,
    pub info: Option<String>,

    // Text colors
    pub text: Option<String>,
    pub text_dim: Option<String>,
    pub text_bright: Option<String>,

    // Syntax/special colors
    pub keyword: Option<String>,
    pub string: Option<String>,
    pub comment: Option<String>,
    pub function: Option<String>,

    // Animation colors
    pub spinner: Option<String>,
    pub progress: Option<String>,
}

/// Browser configuration for integrated screenshot capabilities.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct BrowserConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub viewport: Option<BrowserViewportConfig>,

    #[serde(default)]
    pub wait: Option<BrowserWaitStrategy>,

    #[serde(default)]
    pub fullpage: bool,

    #[serde(default)]
    pub segments_max: Option<usize>,

    #[serde(default)]
    pub idle_timeout_ms: Option<u64>,

    #[serde(default)]
    pub format: Option<BrowserImageFormat>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct BrowserViewportConfig {
    pub width: u32,
    pub height: u32,

    #[serde(default)]
    pub device_scale_factor: Option<f64>,

    #[serde(default)]
    pub mobile: bool,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum BrowserWaitStrategy {
    Event(String),
    Delay { delay_ms: u64 },
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum BrowserImageFormat {
    Png,
    Webp,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct SandboxWorkspaceWrite {
    #[serde(default)]
    pub writable_roots: Vec<PathBuf>,
    #[serde(default)]
    pub network_access: bool,
    #[serde(default)]
    pub exclude_tmpdir_env_var: bool,
    #[serde(default)]
    pub exclude_slash_tmp: bool,
    /// When true, do not protect the top-level `.git` folder under a writable
    /// root. Defaults to true (historical behavior allows Git writes).
    #[serde(default = "crate::config_types::default_true_bool")]
    pub allow_git_writes: bool,
}

// Serde helper: default to true for `allow_git_writes` when omitted.
pub(crate) const fn default_true_bool() -> bool { true }

#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ShellEnvironmentPolicyInherit {
    /// "Core" environment variables for the platform. On UNIX, this would
    /// include HOME, LOGNAME, PATH, SHELL, and USER, among others.
    Core,

    /// Inherits the full environment from the parent process.
    #[default]
    All,

    /// Do not inherit any environment variables from the parent process.
    None,
}

/// Policy for building the `env` when spawning a process via either the
/// `shell` or `local_shell` tool.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ShellEnvironmentPolicyToml {
    pub inherit: Option<ShellEnvironmentPolicyInherit>,

    pub ignore_default_excludes: Option<bool>,

    /// List of regular expressions.
    pub exclude: Option<Vec<String>>,

    pub r#set: Option<HashMap<String, String>>,

    /// List of regular expressions.
    pub include_only: Option<Vec<String>>,

    pub experimental_use_profile: Option<bool>,
}

pub type EnvironmentVariablePattern = WildMatchPattern<'*', '?'>;

/// Deriving the `env` based on this policy works as follows:
/// 1. Create an initial map based on the `inherit` policy.
/// 2. If `ignore_default_excludes` is false, filter the map using the default
///    exclude pattern(s), which are: `"*KEY*"` and `"*TOKEN*"`.
/// 3. If `exclude` is not empty, filter the map using the provided patterns.
/// 4. Insert any entries from `r#set` into the map.
/// 5. If non-empty, filter the map using the `include_only` patterns.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShellEnvironmentPolicy {
    /// Starting point when building the environment.
    pub inherit: ShellEnvironmentPolicyInherit,

    /// True to skip the check to exclude default environment variables that
    /// contain "KEY" or "TOKEN" in their name.
    pub ignore_default_excludes: bool,

    /// Environment variable names to exclude from the environment.
    pub exclude: Vec<EnvironmentVariablePattern>,

    /// (key, value) pairs to insert in the environment.
    pub r#set: HashMap<String, String>,

    /// Environment variable names to retain in the environment.
    pub include_only: Vec<EnvironmentVariablePattern>,

    /// If true, the shell profile will be used to run the command.
    pub use_profile: bool,
}

impl From<ShellEnvironmentPolicyToml> for ShellEnvironmentPolicy {
    fn from(toml: ShellEnvironmentPolicyToml) -> Self {
        // Default to inheriting the full environment when not specified.
        let inherit = toml.inherit.unwrap_or(ShellEnvironmentPolicyInherit::All);
        let ignore_default_excludes = toml.ignore_default_excludes.unwrap_or(false);
        let exclude = toml
            .exclude
            .unwrap_or_default()
            .into_iter()
            .map(|s| EnvironmentVariablePattern::new_case_insensitive(&s))
            .collect();
        let r#set = toml.r#set.unwrap_or_default();
        let include_only = toml
            .include_only
            .unwrap_or_default()
            .into_iter()
            .map(|s| EnvironmentVariablePattern::new_case_insensitive(&s))
            .collect();
        let use_profile = toml.experimental_use_profile.unwrap_or(false);

        Self {
            inherit,
            ignore_default_excludes,
            exclude,
            r#set,
            include_only,
            use_profile,
        }
    }
}

/// See https://platform.openai.com/docs/guides/reasoning?api-mode=responses#get-started-with-reasoning
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq, Display)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ReasoningEffort {
    /// Minimal reasoning. Accepts legacy value "none" for backwards compatibility.
    #[serde(alias = "none")]
    Minimal,
    Low,
    #[default]
    Medium,
    High,
    /// Deprecated: previously disabled reasoning. Kept for internal use only.
    #[serde(skip)]
    None,
}

/// A summary of the reasoning performed by the model. This can be useful for
/// debugging and understanding the model's reasoning process.
/// See https://platform.openai.com/docs/guides/reasoning?api-mode=responses#reasoning-summaries
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq, Display)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ReasoningSummary {
    #[default]
    Auto,
    Concise,
    Detailed,
    /// Option to disable reasoning summaries.
    None,
}

/// Text verbosity level for OpenAI API responses.
/// Controls the level of detail in the model's text responses.
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq, Display)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum TextVerbosity {
    Low,
    #[default]
    Medium,
    High,
}

impl From<codex_protocol::config_types::ReasoningEffort> for ReasoningEffort {
    fn from(v: codex_protocol::config_types::ReasoningEffort) -> Self {
        match v {
            codex_protocol::config_types::ReasoningEffort::Minimal => ReasoningEffort::Minimal,
            codex_protocol::config_types::ReasoningEffort::Low => ReasoningEffort::Low,
            codex_protocol::config_types::ReasoningEffort::Medium => ReasoningEffort::Medium,
            codex_protocol::config_types::ReasoningEffort::High => ReasoningEffort::High,
        }
    }
}

impl From<codex_protocol::config_types::ReasoningSummary> for ReasoningSummary {
    fn from(v: codex_protocol::config_types::ReasoningSummary) -> Self {
        match v {
            codex_protocol::config_types::ReasoningSummary::Auto => ReasoningSummary::Auto,
            codex_protocol::config_types::ReasoningSummary::Concise => ReasoningSummary::Concise,
            codex_protocol::config_types::ReasoningSummary::Detailed => ReasoningSummary::Detailed,
            codex_protocol::config_types::ReasoningSummary::None => ReasoningSummary::None,
        }
    }
}
