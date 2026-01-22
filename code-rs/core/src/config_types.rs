//! Types used to define the fields of [`crate::config::Config`].

// Note this file should generally be restricted to simple struct/enum
// definitions that do not contain business logic.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use schemars::JsonSchema;
use wildmatch::WildMatchPattern;

use shlex::split as shlex_split;

use serde::de::{self, Deserializer, Error as SerdeError};
use serde::Deserialize;
use serde::Serialize;
use strum_macros::Display;

pub const DEFAULT_OTEL_ENVIRONMENT: &str = "dev";

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum OtelHttpProtocol {
    Binary,
    Json,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum OtelExporterKind {
    None,
    OtlpHttp {
        endpoint: String,
        headers: HashMap<String, String>,
        protocol: OtelHttpProtocol,
    },
    OtlpGrpc {
        endpoint: String,
        headers: HashMap<String, String>,
    },
}

#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct OtelConfigToml {
    pub log_user_prompt: Option<bool>,
    pub environment: Option<String>,
    pub exporter: Option<OtelExporterKind>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OtelConfig {
    pub log_user_prompt: bool,
    pub environment: String,
    pub exporter: OtelExporterKind,
}

impl Default for OtelConfig {
    fn default() -> Self {
        OtelConfig {
            log_user_prompt: false,
            environment: DEFAULT_OTEL_ENVIRONMENT.to_owned(),
            exporter: OtelExporterKind::None,
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct McpServerConfig {
    #[serde(flatten)]
    pub transport: McpServerTransportConfig,

    /// Startup timeout in seconds for initializing MCP server & initially listing tools.
    #[serde(
        default,
        with = "option_duration_secs",
        skip_serializing_if = "Option::is_none"
    )]
    pub startup_timeout_sec: Option<Duration>,

    /// Default timeout for MCP tool calls initiated via this server.
    #[serde(default, with = "option_duration_secs")]
    pub tool_timeout_sec: Option<Duration>,
}

impl<'de> Deserialize<'de> for McpServerConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawMcpServerConfig {
            command: Option<String>,
            #[serde(default)]
            args: Option<Vec<String>>,
            #[serde(default)]
            env: Option<HashMap<String, String>>,

            url: Option<String>,
            bearer_token: Option<String>,

            #[serde(default)]
            startup_timeout_sec: Option<f64>,
            #[serde(default)]
            startup_timeout_ms: Option<u64>,
            #[serde(default, with = "option_duration_secs")]
            tool_timeout_sec: Option<Duration>,
        }

        let raw = RawMcpServerConfig::deserialize(deserializer)?;

        let startup_timeout_sec = match (raw.startup_timeout_sec, raw.startup_timeout_ms) {
            (Some(sec), _) => {
                let duration = Duration::try_from_secs_f64(sec).map_err(SerdeError::custom)?;
                Some(duration)
            }
            (None, Some(ms)) => Some(Duration::from_millis(ms)),
            (None, None) => None,
        };

        fn throw_if_set<E, T>(transport: &str, field: &str, value: Option<&T>) -> Result<(), E>
        where
            E: SerdeError,
        {
            if value.is_none() {
                return Ok(());
            }
            Err(E::custom(format!(
                "{field} is not supported for {transport}",
            )))
        }

        let transport = match raw {
            RawMcpServerConfig {
                command: Some(command),
                args,
                env,
                url,
                bearer_token,
                ..
            } => {
                throw_if_set("stdio", "url", url.as_ref())?;
                throw_if_set("stdio", "bearer_token", bearer_token.as_ref())?;
                McpServerTransportConfig::Stdio {
                    command,
                    args: args.unwrap_or_default(),
                    env,
                }
            }
            RawMcpServerConfig {
                url: Some(url),
                bearer_token,
                command,
                args,
                env,
                ..
            } => {
                throw_if_set("streamable_http", "command", command.as_ref())?;
                throw_if_set("streamable_http", "args", args.as_ref())?;
                throw_if_set("streamable_http", "env", env.as_ref())?;
                McpServerTransportConfig::StreamableHttp { url, bearer_token }
            }
            _ => return Err(SerdeError::custom("invalid transport")),
        };

        Ok(Self {
            transport,
            startup_timeout_sec,
            tool_timeout_sec: raw.tool_timeout_sec,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged, deny_unknown_fields, rename_all = "snake_case")]
pub enum McpServerTransportConfig {
    /// https://modelcontextprotocol.io/specification/2025-06-18/basic/transports#stdio
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        env: Option<HashMap<String, String>>,
    },
    /// https://modelcontextprotocol.io/specification/2025-06-18/basic/transports#streamable-http
    StreamableHttp {
        url: String,
        /// A plain text bearer token to use for authentication.
        /// This bearer token will be included in the HTTP request header as an `Authorization: Bearer <token>` header.
        /// This should be used with caution because it lives on disk in clear text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bearer_token: Option<String>,
    },
}

mod option_duration_secs {
    use serde::Deserialize;
    use serde::Deserializer;
    use serde::Serializer;
    use std::time::Duration;

    pub fn serialize<S>(value: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(duration) => serializer.serialize_some(&duration.as_secs_f64()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = Option::<f64>::deserialize(deserializer)?;
        secs.map(|secs| Duration::try_from_secs_f64(secs).map_err(serde::de::Error::custom))
            .transpose()
    }
}

/// Configuration for commands that require an explicit `confirm:` prefix.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct ConfirmGuardConfig {
    /// List of regex patterns applied to the raw command (joined argv or shell script).
    #[serde(default)]
    pub patterns: Vec<ConfirmGuardPattern>,
}

impl Default for ConfirmGuardConfig {
    fn default() -> Self {
        Self { patterns: default_confirm_guard_patterns() }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct ConfirmGuardPattern {
    /// ECMA-style regular expression matched against the command string.
    pub regex: String,
    /// Optional custom guidance text surfaced when the guard triggers.
    #[serde(default)]
    pub message: Option<String>,
}

fn default_confirm_guard_patterns() -> Vec<ConfirmGuardPattern> {
    vec![
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+reset\b".to_string(),
            message: Some("Blocked git reset. Reset rewrites the working tree/index and may delete local work. Resend with 'confirm:' if you're certain.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+checkout\s+--\b".to_string(),
            message: Some("Blocked git checkout -- <paths>. This overwrites local modifications; resend with 'confirm:' to proceed.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+checkout\s+(?:-b|-B|--orphan|--detach)\b".to_string(),
            message: Some("Blocked git checkout with branch-changing flag. Switching branches can discard or hide in-progress changes.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+checkout\s+-\b".to_string(),
            message: Some("Blocked git checkout -. Confirm before switching back to the previous branch.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+switch\b.*(?:-c|--detach)".to_string(),
            message: Some("Blocked git switch creating or detaching a branch. Resend with 'confirm:' if requested.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+switch\s+[^\s-][^\s]*".to_string(),
            message: Some("Blocked git switch <branch>. Branch changes can discard or hide work; confirm before continuing.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+clean\b.*(?:-f|--force|-x|-X|-d)".to_string(),
            message: Some("Blocked git clean with destructive flags. This deletes untracked files or build artifacts.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*git\s+push\b.*(?:--force|-f)".to_string(),
            message: Some("Blocked git push --force. Force pushes rewrite remote history; only continue if explicitly requested.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?rm\s+-[a-z-]*rf[a-z-]*\s+(?:--\s+)?(?:\.|\.\.|\./|/|\*)(?:\s|$)".to_string(),
            message: Some("Blocked rm -rf targeting a broad path (., .., /, or *). Confirm before destructive delete.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?rm\s+-[a-z-]*r[a-z-]*\s+-[a-z-]*f[a-z-]*\s+(?:--\s+)?(?:\.|\.\.|\./|/|\*)(?:\s|$)".to_string(),
            message: Some("Blocked rm -r/-f combination targeting broad paths. Resend with 'confirm:' if you intend to wipe this tree.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?rm\s+-[a-z-]*f[a-z-]*\s+-[a-z-]*r[a-z-]*\s+(?:--\s+)?(?:\.|\.\.|\./|/|\*)(?:\s|$)".to_string(),
            message: Some("Blocked rm -f/-r combination targeting broad paths. Confirm before running.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?rm\b[^\n]*\s+-[a-z-]*rf[a-z-]*\b".to_string(),
            message: Some("Blocked rm -rf. Force-recursive delete requires explicit confirmation.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?rm\b[^\n]*\s+-[-0-9a-qs-z]*f[-0-9a-qs-z]*\b".to_string(),
            message: Some("Blocked rm -f. Force delete requires explicit confirmation.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?find\s+\.(?:\s|$).*\s-delete\b".to_string(),
            message: Some("Blocked find . ... -delete. Recursive deletes require confirmation.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?find\s+\.(?:\s|$).*\s-exec\s+rm\b".to_string(),
            message: Some("Blocked find . ... -exec rm. Confirm before running recursive rm.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?trash\s+-[a-z-]*r[a-z-]*f[a-z-]*\b".to_string(),
            message: Some("Blocked trash -rf. Bulk trash operations can delete large portions of the workspace.".to_string()),
        },
        ConfirmGuardPattern {
            regex: r"(?i)^\s*(?:sudo\s+)?fd\b.*(?:--exec|-x)\s+rm\b".to_string(),
            message: Some("Blocked fd … --exec rm. Confirm before piping search results into rm.".to_string()),
        },
    ]
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AllowedCommandMatchKind {
    Exact,
    Prefix,
}

impl Default for AllowedCommandMatchKind {
    fn default() -> Self { Self::Exact }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct AllowedCommand {
    #[serde(default)]
    pub argv: Vec<String>,
    #[serde(default)]
    pub match_kind: AllowedCommandMatchKind,
}

/// Configuration for a subagent slash command (e.g., plan/solve/code or custom)
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SubagentCommandConfig {
    /// Name of the command (e.g., "plan", "solve", "code", or custom)
    pub name: String,

    /// Whether agents launched for this command should run in read-only mode
    /// Defaults: plan/solve=true, code=false (applied if not specified here)
    #[serde(default)]
    pub read_only: bool,

    /// Agent names to enable for this command. If empty, falls back to
    /// enabled agents from `[[agents]]`, or built-in defaults.
    #[serde(default)]
    pub agents: Vec<String>,

    /// Extra instructions to append to the orchestrator (Code) prompt.
    #[serde(default)]
    pub orchestrator_instructions: Option<String>,

    /// Extra instructions that the orchestrator should append to the prompt
    /// given to each launched agent.
    #[serde(default)]
    pub agent_instructions: Option<String>,
}

/// Top-level subagents section containing a list of commands.
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "kebab-case")]
pub struct SubagentsToml {
    #[serde(default)]
    pub commands: Vec<SubagentCommandConfig>,
}

/// MCP tool identifiers that the client exposes to the agent.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ClientTools {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_permission: Option<McpToolId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_text_file: Option<McpToolId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_text_file: Option<McpToolId>,
}

/// Identifier for a client-hosted MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpToolId {
    pub mcp_server: String,
    pub tool_name: String,
}

/// Configuration for external agent models
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub struct AgentConfig {
    /// Name of the agent (e.g., "claude", "gemini", "gpt-4")
    pub name: String,

    /// Command to execute the agent (e.g., "claude", "gemini").
    /// If omitted, defaults to the agent `name` during config load.
    #[serde(default)]
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

    /// Optional arguments to pass only when the agent is executed in
    /// read-only mode. When present, these are preferred over `args` for
    /// read-only runs.
    #[serde(default)]
    pub args_read_only: Option<Vec<String>>,

    /// Optional arguments to pass only when the agent is executed with write
    /// permissions. When present, these are preferred over `args` for write
    /// runs.
    #[serde(default)]
    pub args_write: Option<Vec<String>>,

    /// Optional per-agent instructions. When set, these are prepended to the
    /// prompt provided to the agent whenever it runs.
    #[serde(default)]
    pub instructions: Option<String>,
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

    /// When true, run `actionlint` on modified workflows during apply_patch.
    #[serde(default)]
    pub actionlint_on_patch: bool,

    /// Optional explicit executable path for actionlint.
    #[serde(default)]
    pub actionlint_path: Option<PathBuf>,

    /// Treat actionlint findings as blocking when composing approval text.
    #[serde(default)]
    pub actionlint_strict: bool,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct ValidationConfig {
    /// Legacy master toggle for the validation harness (kept for config compatibility).
    /// `run_patch_harness` now relies solely on the functional/stylistic group toggles.
    #[serde(default)]
    pub patch_harness: bool,

    /// Optional allowlist restricting which external tools may run.
    #[serde(default)]
    pub tools_allowlist: Option<Vec<String>>,

    /// Timeout (seconds) for each external tool invocation.
    #[serde(default)]
    pub timeout_seconds: Option<u64>,

    /// Group toggles that control which classes of validation run.
    #[serde(default)]
    pub groups: ValidationGroups,

    /// Per-tool enable flags (unset implies enabled).
    #[serde(default)]
    pub tools: ValidationTools,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            patch_harness: false,
            tools_allowlist: None,
            timeout_seconds: None,
            groups: ValidationGroups::default(),
            tools: ValidationTools::default(),
        }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct ValidationGroups {
    /// Functional checks catch correctness regressions.
    #[serde(default = "default_true")]
    pub functional: bool,

    /// Stylistic checks enforce formatting and best practices.
    #[serde(default)]
    pub stylistic: bool,
}

impl Default for ValidationGroups {
    fn default() -> Self {
        Self { functional: false, stylistic: false }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ValidationTools {
    pub shellcheck: Option<bool>,
    pub markdownlint: Option<bool>,
    pub hadolint: Option<bool>,
    pub yamllint: Option<bool>,
    #[serde(rename = "cargo-check")]
    pub cargo_check: Option<bool>,
    pub shfmt: Option<bool>,
    pub prettier: Option<bool>,
    #[serde(rename = "tsc")]
    pub tsc: Option<bool>,
    pub eslint: Option<bool>,
    pub phpstan: Option<bool>,
    pub psalm: Option<bool>,
    pub mypy: Option<bool>,
    pub pyright: Option<bool>,
    #[serde(rename = "golangci-lint")]
    pub golangci_lint: Option<bool>,
}

/// Category groupings for validation checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationCategory {
    Functional,
    Stylistic,
}

impl ValidationCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            ValidationCategory::Functional => "functional",
            ValidationCategory::Stylistic => "stylistic",
        }
    }
}

/// Map a validation tool name to its category grouping.
pub fn validation_tool_category(name: &str) -> ValidationCategory {
    match name {
        "actionlint"
        | "shellcheck"
        | "cargo-check"
        | "tsc"
        | "eslint"
        | "phpstan"
        | "psalm"
        | "mypy"
        | "pyright"
        | "golangci-lint" => ValidationCategory::Functional,
        "markdownlint" | "hadolint" | "yamllint" | "shfmt" | "prettier" => {
            ValidationCategory::Stylistic
        }
        _ => ValidationCategory::Stylistic,
    }
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

/// Settings that govern if and what will be written to `~/.code/history.jsonl`
/// (Code still reads legacy `~/.codex/history.jsonl`).
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Notifications {
    Enabled(bool),
    Custom(Vec<String>),
}

impl Default for Notifications {
    fn default() -> Self {
        Self::Enabled(false)
    }
}

/// Collection of settings that are specific to the TUI.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub struct CachedTerminalBackground {
    pub is_dark: bool,
    #[serde(default)]
    pub term: Option<String>,
    #[serde(default)]
    pub term_program: Option<String>,
    #[serde(default)]
    pub term_program_version: Option<String>,
    #[serde(default)]
    pub colorfgbg: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub rgb: Option<String>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Tui {
    /// Theme configuration for the TUI
    #[serde(default)]
    pub theme: ThemeConfig,

    /// Auto Drive behavioral defaults (legacy location; prefer top-level `[auto_drive]`).
    #[serde(default)]
    pub auto_drive: Option<AutoDriveSettings>,

    /// Cached autodetect result so we can skip probing the terminal repeatedly.
    #[serde(default)]
    pub cached_terminal_background: Option<CachedTerminalBackground>,

    /// Syntax highlighting configuration (Markdown fenced code blocks)
    #[serde(default)]
    pub highlight: HighlightConfig,

    /// Whether to show reasoning content expanded by default (can be toggled with Ctrl+R/T)
    #[serde(default)]
    pub show_reasoning: bool,

    /// Streaming/animation behavior for assistant/reasoning output
    #[serde(default)]
    pub stream: StreamConfig,

    /// Loading spinner style selection
    #[serde(default)]
    pub spinner: SpinnerSelection,

    /// Enable desktop notifications from the TUI when the terminal is unfocused.
    /// Defaults to `false`.
    #[serde(default)]
    pub notifications: Notifications,

    /// Whether to use the terminal's Alternate Screen (full-screen) mode.
    /// When false, Codex renders nothing and leaves the standard terminal
    /// buffer visible; users can toggle back to Alternate Screen at runtime
    /// with Ctrl+T. Defaults to true.
    #[serde(default = "default_true")]
    pub alternate_screen: bool,

    /// Remember whether Auto Resolve is enabled for `/review` flows.
    #[serde(default = "default_true")]
    pub review_auto_resolve: bool,

    /// Run a background `/review` after turns that modify code.
    #[serde(default = "default_true")]
    pub auto_review_enabled: bool,
}

// Important: Provide a manual Default so that when no config file exists and we
// construct `Config` via `unwrap_or_default()`, we still honor the intended
// default of `alternate_screen = true`. Deriving `Default` would set booleans to
// `false`, which caused fresh installs (or a temporary CODEX_HOME) to start in
// standard-terminal mode until the user pressed Ctrl+T.
impl Default for Tui {
    fn default() -> Self {
        Self {
            theme: ThemeConfig::default(),
            auto_drive: None,
            cached_terminal_background: None,
            highlight: HighlightConfig::default(),
            show_reasoning: false,
            stream: StreamConfig::default(),
            spinner: SpinnerSelection::default(),
            notifications: Notifications::default(),
            alternate_screen: true,
            review_auto_resolve: true,
            auto_review_enabled: true,
        }
    }
}

/// User acknowledgements for in-product notices (distinct from notifications).
#[derive(Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Notice {
    pub hide_full_access_warning: Option<bool>,
    pub hide_gpt5_1_migration_prompt: Option<bool>,
    #[serde(rename = "hide_gpt-5.1-codex-max_migration_prompt")]
    pub hide_gpt_5_1_codex_max_migration_prompt: Option<bool>,
    pub hide_gpt5_2_migration_prompt: Option<bool>,
    pub hide_gpt5_2_codex_migration_prompt: Option<bool>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(transparent)]
pub struct AutoResolveAttemptLimit(u32);

impl AutoResolveAttemptLimit {
    pub const ALLOWED: [u32; 9] = [0, 1, 2, 3, 4, 5, 10, 20, 40];
    pub const DEFAULT: u32 = 10;

    pub fn get(self) -> u32 {
        self.0
    }

    pub fn try_new(value: u32) -> Result<Self, &'static str> {
        if Self::ALLOWED.contains(&value) {
            Ok(Self(value))
        } else {
            Err("auto_resolve_review_attempts must be one of {0,1,2,3,4,5,10,20,40}")
        }
    }
}

impl Default for AutoResolveAttemptLimit {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

impl<'de> Deserialize<'de> for AutoResolveAttemptLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = u32::deserialize(deserializer)?;
        Self::try_new(raw).map_err(D::Error::custom)
    }
}

/// Auto Drive behavioral defaults persisted via `config.toml`.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct AutoDriveSettings {
    #[serde(default = "default_true")]
    pub review_enabled: bool,

    #[serde(default = "default_true")]
    pub agents_enabled: bool,

    #[serde(default = "default_true")]
    pub qa_automation_enabled: bool,

    #[serde(default = "default_true")]
    pub cross_check_enabled: bool,

    #[serde(default = "default_true")]
    pub observer_enabled: bool,

    /// Enable coordinator routing of user prompts during Auto Drive turns.
    #[serde(default = "default_true")]
    pub coordinator_routing: bool,

    #[serde(default)]
    pub continue_mode: AutoDriveContinueMode,

    /// Model used for the Auto Drive coordinator and follow-on turns.
    #[serde(default = "default_auto_drive_model")]
    pub model: String,

    /// Reasoning effort applied to the Auto Drive model.
    #[serde(default = "default_auto_drive_reasoning_effort")]
    pub model_reasoning_effort: ReasoningEffort,

    #[serde(default)]
    pub auto_resolve_review_attempts: AutoResolveAttemptLimit,

    #[serde(default)]
    pub auto_review_followup_attempts: AutoResolveAttemptLimit,

    /// Maximum number of coordinator turns before stopping the session (0 = unlimited).
    #[serde(default = "default_auto_drive_coordinator_turn_cap")]
    pub coordinator_turn_cap: u32,
}

impl Default for AutoDriveSettings {
    fn default() -> Self {
        Self {
            review_enabled: true,
            agents_enabled: true,
            qa_automation_enabled: true,
            cross_check_enabled: true,
            observer_enabled: true,
            coordinator_routing: true,
            continue_mode: AutoDriveContinueMode::TenSeconds,
            model: default_auto_drive_model(),
            model_reasoning_effort: default_auto_drive_reasoning_effort(),
            auto_resolve_review_attempts: AutoResolveAttemptLimit::default(),
            auto_review_followup_attempts: AutoResolveAttemptLimit::default(),
            coordinator_turn_cap: default_auto_drive_coordinator_turn_cap(),
        }
    }
}

const fn default_auto_drive_coordinator_turn_cap() -> u32 {
    0
}

fn default_auto_drive_model() -> String {
    // Keep aligned with the coordinator's preferred model fallback.
    String::from("gpt-5.1")
}

const fn default_auto_drive_reasoning_effort() -> ReasoningEffort {
    ReasoningEffort::High
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AutoDriveContinueMode {
    Immediate,
    TenSeconds,
    SixtySeconds,
    Manual,
}

impl AutoDriveContinueMode {
    pub fn seconds(self) -> Option<u8> {
        match self {
            Self::Immediate => Some(0),
            Self::TenSeconds => Some(10),
            Self::SixtySeconds => Some(60),
            Self::Manual => None,
        }
    }
}

impl Default for AutoDriveContinueMode {
    fn default() -> Self {
        Self::TenSeconds
    }
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

    /// Optional display name when using a custom theme generated by the user.
    /// Not used for built-in themes. If `name == Custom` and this is set, the
    /// UI may display it in place of the generic "Custom" label.
    #[serde(default)]
    pub label: Option<String>,

    /// Optional hint whether the custom theme targets a dark background.
    /// When present and `name == Custom`, the UI can show "Dark - <label>"
    /// or "Light - <label>" in lists.
    #[serde(default)]
    pub is_dark: Option<bool>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: ThemeName::default(),
            colors: ThemeColors::default(),
            label: None,
            is_dark: None,
        }
    }
}

/// Selected loading spinner style.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct SpinnerSelection {
    /// Name of the spinner to use. Accepts one of the names from
    /// sindresorhus/cli-spinners (kebab-case), or custom names supported
    /// by Codex. Defaults to "diamond".
    #[serde(default = "default_spinner_name")] 
    pub name: String,
    /// Custom spinner definitions saved by the user
    #[serde(default)]
    pub custom: std::collections::HashMap<String, CustomSpinner>,
}

fn default_spinner_name() -> String { "diamond".to_string() }

impl Default for SpinnerSelection {
    fn default() -> Self {
        Self { name: default_spinner_name(), custom: Default::default() }
    }
}

/// User-defined custom spinner
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct CustomSpinner {
    pub interval: u64,
    pub frames: Vec<String>,
    /// Optional human-readable label to display in the UI
    #[serde(default)]
    pub label: Option<String>,
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
    LightPhotonAnsi16,
    LightPrismRainbow,
    LightVividTriad,
    LightPorcelain,
    LightSandbar,
    LightGlacier,
    // Dark themes (below)
    DarkCarbonNight,
    DarkCarbonAnsi16,
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
#[derive(Debug, Clone, PartialEq)]
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

impl Default for ShellEnvironmentPolicy {
    fn default() -> Self {
        Self {
            inherit: ShellEnvironmentPolicyInherit::All,
            ignore_default_excludes: true,
            exclude: Vec::new(),
            r#set: HashMap::new(),
            include_only: Vec::new(),
            use_profile: false,
        }
    }
}

impl From<ShellEnvironmentPolicyToml> for ShellEnvironmentPolicy {
    fn from(toml: ShellEnvironmentPolicyToml) -> Self {
        // Default to inheriting the full environment when not specified.
        let inherit = toml.inherit.unwrap_or(ShellEnvironmentPolicyInherit::All);
        let ignore_default_excludes = toml.ignore_default_excludes.unwrap_or(true);
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
#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq, Display, Hash)]
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
    XHigh,
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

#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Personality {
    Friendly,
    #[default]
    Pragmatic,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CommandField {
    List(Vec<String>),
    String(String),
}

fn deserialize_command_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = CommandField::deserialize(deserializer)?;
    match value {
        CommandField::List(items) => Ok(items),
        CommandField::String(text) => {
            if text.trim().is_empty() {
                Ok(Vec::new())
            } else {
                shlex_split(&text).ok_or_else(|| de::Error::custom("failed to parse command string"))
            }
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectHookEvent {
    #[serde(rename = "session.start")]
    SessionStart,
    #[serde(rename = "session.end")]
    SessionEnd,
    #[serde(rename = "tool.before")]
    ToolBefore,
    #[serde(rename = "tool.after")]
    ToolAfter,
    #[serde(rename = "file.before_write")]
    FileBeforeWrite,
    #[serde(rename = "file.after_write")]
    FileAfterWrite,
}

impl ProjectHookEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProjectHookEvent::SessionStart => "session.start",
            ProjectHookEvent::SessionEnd => "session.end",
            ProjectHookEvent::ToolBefore => "tool.before",
            ProjectHookEvent::ToolAfter => "tool.after",
            ProjectHookEvent::FileBeforeWrite => "file.before_write",
            ProjectHookEvent::FileAfterWrite => "file.after_write",
        }
    }

    pub fn slug(&self) -> &'static str {
        match self {
            ProjectHookEvent::SessionStart => "session_start",
            ProjectHookEvent::SessionEnd => "session_end",
            ProjectHookEvent::ToolBefore => "tool_before",
            ProjectHookEvent::ToolAfter => "tool_after",
            ProjectHookEvent::FileBeforeWrite => "file_before_write",
            ProjectHookEvent::FileAfterWrite => "file_after_write",
        }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct ProjectHookConfig {
    pub event: ProjectHookEvent,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(alias = "run", deserialize_with = "deserialize_command_vec")]
    pub command: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub run_in_background: Option<bool>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct ProjectCommandConfig {
    pub name: String,
    #[serde(alias = "run", deserialize_with = "deserialize_command_vec")]
    pub command: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Retention policy configuration for env_ctx_v2 timeline management.
#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct RetentionConfig {
    /// Maximum number of environment context deltas to retain (default: 3)
    #[serde(default = "default_max_env_deltas")]
    pub max_env_deltas: usize,
    /// Maximum number of browser snapshots to retain (default: 2)
    #[serde(default = "default_max_browser_snapshots")]
    pub max_browser_snapshots: usize,
    /// Maximum total bytes for all retained env_ctx items (default: 100KB)
    #[serde(default = "default_max_total_bytes")]
    pub max_total_bytes: usize,
    /// Always keep the most recent environment baseline snapshot (default: true)
    #[serde(default = "default_true_bool")]
    pub keep_latest_baseline: bool,
}

fn default_max_env_deltas() -> usize {
    3
}

fn default_max_browser_snapshots() -> usize {
    2
}

fn default_max_total_bytes() -> usize {
    100 * 1024
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            max_env_deltas: default_max_env_deltas(),
            max_browser_snapshots: default_max_browser_snapshots(),
            max_total_bytes: default_max_total_bytes(),
            keep_latest_baseline: true,
        }
    }
}

impl From<code_protocol::config_types::ReasoningEffort> for ReasoningEffort {
    fn from(v: code_protocol::config_types::ReasoningEffort) -> Self {
        match v {
            code_protocol::config_types::ReasoningEffort::Minimal => ReasoningEffort::Minimal,
            code_protocol::config_types::ReasoningEffort::Low => ReasoningEffort::Low,
            code_protocol::config_types::ReasoningEffort::Medium => ReasoningEffort::Medium,
            code_protocol::config_types::ReasoningEffort::High => ReasoningEffort::High,
            code_protocol::config_types::ReasoningEffort::XHigh => ReasoningEffort::XHigh,
        }
    }
}

impl From<ReasoningEffort> for code_protocol::config_types::ReasoningEffort {
    fn from(v: ReasoningEffort) -> Self {
        match v {
            ReasoningEffort::Minimal | ReasoningEffort::None => {
                code_protocol::config_types::ReasoningEffort::Minimal
            }
            ReasoningEffort::Low => code_protocol::config_types::ReasoningEffort::Low,
            ReasoningEffort::Medium => code_protocol::config_types::ReasoningEffort::Medium,
            ReasoningEffort::High => code_protocol::config_types::ReasoningEffort::High,
            ReasoningEffort::XHigh => code_protocol::config_types::ReasoningEffort::XHigh,
        }
    }
}

impl From<code_protocol::config_types::ReasoningSummary> for ReasoningSummary {
    fn from(v: code_protocol::config_types::ReasoningSummary) -> Self {
        match v {
            code_protocol::config_types::ReasoningSummary::Auto => ReasoningSummary::Auto,
            code_protocol::config_types::ReasoningSummary::Concise => ReasoningSummary::Concise,
            code_protocol::config_types::ReasoningSummary::Detailed => ReasoningSummary::Detailed,
            code_protocol::config_types::ReasoningSummary::None => ReasoningSummary::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn deserialize_stdio_command_server_config() {
        let cfg: McpServerConfig = toml::from_str(
            r#"
            command = "echo"
        "#,
        )
        .expect("should deserialize command config");

        assert_eq!(
            cfg.transport,
            McpServerTransportConfig::Stdio {
                command: "echo".to_string(),
                args: vec![],
                env: None
            }
        );
    }

    #[test]
    fn deserialize_stdio_command_server_config_with_args() {
        let cfg: McpServerConfig = toml::from_str(
            r#"
            command = "echo"
            args = ["hello", "world"]
        "#,
        )
        .expect("should deserialize command config");

        assert_eq!(
            cfg.transport,
            McpServerTransportConfig::Stdio {
                command: "echo".to_string(),
                args: vec!["hello".to_string(), "world".to_string()],
                env: None
            }
        );
    }

    #[test]
    fn deserialize_stdio_command_server_config_with_arg_with_args_and_env() {
        let cfg: McpServerConfig = toml::from_str(
            r#"
            command = "echo"
            args = ["hello", "world"]
            env = { "FOO" = "BAR" }
        "#,
        )
        .expect("should deserialize command config");

        assert_eq!(
            cfg.transport,
            McpServerTransportConfig::Stdio {
                command: "echo".to_string(),
                args: vec!["hello".to_string(), "world".to_string()],
                env: Some(HashMap::from([("FOO".to_string(), "BAR".to_string())]))
            }
        );
    }

    #[test]
    fn deserialize_streamable_http_server_config() {
        let cfg: McpServerConfig = toml::from_str(
            r#"
            url = "https://example.com/mcp"
        "#,
        )
        .expect("should deserialize http config");

        assert_eq!(
            cfg.transport,
            McpServerTransportConfig::StreamableHttp {
                url: "https://example.com/mcp".to_string(),
                bearer_token: None
            }
        );
    }

    #[test]
    fn deserialize_streamable_http_server_config_with_bearer_token() {
        let cfg: McpServerConfig = toml::from_str(
            r#"
            url = "https://example.com/mcp"
            bearer_token = "secret"
        "#,
        )
        .expect("should deserialize http config");

        assert_eq!(
            cfg.transport,
            McpServerTransportConfig::StreamableHttp {
                url: "https://example.com/mcp".to_string(),
                bearer_token: Some("secret".to_string())
            }
        );
    }

    #[test]
    fn deserialize_rejects_command_and_url() {
        toml::from_str::<McpServerConfig>(
            r#"
            command = "echo"
            url = "https://example.com"
        "#,
        )
        .expect_err("should reject command+url");
    }

    #[test]
    fn deserialize_rejects_env_for_http_transport() {
        toml::from_str::<McpServerConfig>(
            r#"
            url = "https://example.com"
            env = { "FOO" = "BAR" }
        "#,
        )
        .expect_err("should reject env for http transport");
    }

    #[test]
    fn deserialize_rejects_bearer_token_for_stdio_transport() {
        toml::from_str::<McpServerConfig>(
            r#"
            command = "echo"
            bearer_token = "secret"
        "#,
        )
        .expect_err("should reject bearer token for stdio transport");
    }
}
