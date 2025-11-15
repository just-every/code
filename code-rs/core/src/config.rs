use crate::codex::ApprovedCommandPattern;
use crate::protocol::ApprovedCommandMatchKind;
use crate::config_loader::{load_config_as_toml_blocking, LoaderOverrides};
use crate::config_profile::ConfigProfile;
use crate::config_types::AgentConfig;
use crate::agent_defaults::{agent_model_spec, default_agent_configs};
use std::collections::HashSet;
use crate::config_types::AutoDriveContinueMode;
use crate::config_types::AutoDriveSettings;
use crate::config_types::AllowedCommand;
use crate::config_types::AllowedCommandMatchKind;
use crate::config_types::BrowserConfig;
use crate::config_types::CachedTerminalBackground;
use crate::config_types::ClientTools;
use crate::config_types::History;
use crate::config_types::GithubConfig;
use crate::config_types::ValidationConfig;
use crate::config_types::ThemeName;
use crate::config_types::ThemeColors;
use crate::config_types::McpServerConfig;
use crate::config_types::McpServerTransportConfig;
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
use crate::config_types::DEFAULT_OTEL_ENVIRONMENT;
use crate::git_info::resolve_root_git_project_for_trust;
use crate::model_family::ModelFamily;
use crate::model_family::derive_default_model_family;
use crate::model_family::find_family_for_model;
use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::built_in_model_providers;
use crate::openai_model_info::get_model_info;
use crate::protocol::AskForApproval;
use crate::protocol::SandboxPolicy;
use crate::config_types::ReasoningEffort;
use std::sync::LazyLock;
use std::sync::Mutex;
use crate::config_types::ReasoningSummary;
use crate::project_features::{load_project_commands, ProjectCommand, ProjectHooks};
use code_app_server_protocol::AuthMode;
use code_protocol::config_types::SandboxMode;
use std::time::Duration;
use dirs::home_dir;
use serde::Deserialize;
use serde::de::{self, Unexpected};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use tempfile::NamedTempFile;
use toml::Value as TomlValue;
use toml_edit::Array as TomlArray;
use toml_edit::ArrayOfTables as TomlArrayOfTables;
use toml_edit::DocumentMut;
use toml_edit::Item as TomlItem;
use toml_edit::Table as TomlTable;
use which::which;

const OPENAI_DEFAULT_MODEL: &str = "gpt-5.1-codex";
const OPENAI_DEFAULT_REVIEW_MODEL: &str = "gpt-5.1-codex-mini";
pub const GPT_5_CODEX_MEDIUM_MODEL: &str = "gpt-5.1-codex";

/// Maximum number of bytes of the documentation that will be embedded. Larger
/// files are *silently truncated* to this size so we do not take up too much of
/// the context window.
pub(crate) const PROJECT_DOC_MAX_BYTES: usize = 32 * 1024; // 32 KiB

pub(crate) const CONFIG_TOML_FILE: &str = "config.toml";

const DEFAULT_RESPONSES_ORIGINATOR_HEADER: &str = "code_cli_rs";

static RESPONSES_ORIGINATOR_OVERRIDE: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));

pub fn set_default_originator(originator: &str) -> std::io::Result<()> {
    let mut guard = RESPONSES_ORIGINATOR_OVERRIDE
        .lock()
        .map_err(|_| std::io::Error::new(ErrorKind::Other, "originator override lock poisoned"))?;
    *guard = Some(originator.to_string());
    Ok(())
}

fn default_responses_originator() -> String {
    RESPONSES_ORIGINATOR_OVERRIDE
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
        .unwrap_or_else(|| DEFAULT_RESPONSES_ORIGINATOR_HEADER.to_owned())
}

/// Application configuration loaded from disk and merged with overrides.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Optional override of model selection.
    pub model: String,

    /// Model used specifically for review sessions. Defaults to "gpt-5.1-codex-mini".
    pub review_model: String,

    /// Reasoning effort used when running review sessions.
    pub review_model_reasoning_effort: ReasoningEffort,

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

    /// Base instructions override.
    pub base_instructions: Option<String>,

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

    /// Path to the `codex-linux-sandbox` executable. This must be set if
    /// [`crate::exec::SandboxType::LinuxSeccomp`] is used. Note that this
    /// cannot be set in the config file: it must be set in code via
    /// [`ConfigOverrides`].
    ///
    /// When this program is invoked, arg0 will be set to `codex-linux-sandbox`.
    pub code_linux_sandbox_exe: Option<PathBuf>,

    /// The value to use for `reasoning.effort` when making a
    /// request using the Responses API. Allowed values: `minimal`, `low`, `medium`, `high`.
    pub model_reasoning_effort: ReasoningEffort,

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
    /// Experimental: opt into the RMCP client implementation for MCP servers.
    pub use_experimental_use_rmcp_client: bool,
    /// Enable the `view_image` tool that lets the agent attach local images.
    pub include_view_image_tool: bool,
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
        // Resolve the directory that stores Codex state (e.g. ~/.code or the
        // value of $CODEX_HOME) so we can embed it into the resulting
        // `Config` instance.
        let code_home = find_code_home()?;

        // Step 1: parse `config.toml` into a generic JSON value.
        let mut root_value = load_config_as_toml(&code_home)?;

        // Step 2: apply the `-c` overrides.
        for (path, value) in cli_overrides.into_iter() {
            apply_toml_override(&mut root_value, &path, value);
        }

        // Step 3: deserialize into `ConfigToml` so that Serde can enforce the
        // correct types.
        let cfg: ConfigToml = root_value.try_into().map_err(|e| {
            tracing::error!("Failed to deserialize overridden config: {e}");
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;

        // Step 4: merge with the strongly-typed overrides.
        Self::load_from_base_config_with_overrides(cfg, overrides, code_home)
    }
}

pub fn load_config_as_toml_with_cli_overrides(
    code_home: &Path,
    cli_overrides: Vec<(String, TomlValue)>,
) -> std::io::Result<ConfigToml> {
    let mut root_value = load_config_as_toml(code_home)?;

    for (path, value) in cli_overrides.into_iter() {
        apply_toml_override(&mut root_value, &path, value);
    }

    let cfg: ConfigToml = root_value.try_into().map_err(|e| {
        tracing::error!("Failed to deserialize overridden config: {e}");
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
    })?;

    Ok(cfg)
}

/// Read `CODEX_HOME/config.toml` and return it as a generic TOML value. Returns
/// an empty TOML table when the file does not exist.
pub fn load_config_as_toml(code_home: &Path) -> std::io::Result<TomlValue> {
    load_config_as_toml_blocking(code_home, LoaderOverrides::default())
}

pub fn load_global_mcp_servers(
    code_home: &Path,
) -> std::io::Result<BTreeMap<String, McpServerConfig>> {
    let root_value = load_config_as_toml(code_home)?;
    let Some(servers_value) = root_value.get("mcp_servers") else {
        return Ok(BTreeMap::new());
    };

    let servers: BTreeMap<String, McpServerConfig> = servers_value
        .clone()
        .try_into()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    for (name, cfg) in &servers {
        if let McpServerTransportConfig::Stdio { command, .. } = &cfg.transport {
            let command_looks_like_path = {
                let path = Path::new(command);
                path.components().count() > 1 || path.is_absolute()
            };
            if !command_looks_like_path && which(command).is_err() {
                let msg = format!(
                    "MCP server `{name}` command `{command}` not found on PATH. If the server is an npm package, set command = \"npx\" and keep the package name in args."
                );
                return Err(std::io::Error::new(ErrorKind::NotFound, msg));
            }
        }
    }

    Ok(servers)
}

pub fn write_global_mcp_servers(
    code_home: &Path,
    servers: &BTreeMap<String, McpServerConfig>,
) -> std::io::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents
            .parse::<DocumentMut>()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e),
    };

    doc.as_table_mut().remove("mcp_servers");

    if !servers.is_empty() {
        let mut table = TomlTable::new();
        table.set_implicit(true);
        doc["mcp_servers"] = TomlItem::Table(table);

        for (name, config) in servers {
            let mut entry = TomlTable::new();
            entry.set_implicit(false);
            match &config.transport {
                McpServerTransportConfig::Stdio { command, args, env } => {
                    entry["command"] = toml_edit::value(command.clone());

                    if !args.is_empty() {
                        let mut args_array = TomlArray::new();
                        for arg in args {
                            args_array.push(arg.clone());
                        }
                        entry["args"] = TomlItem::Value(args_array.into());
                    }

                    if let Some(env) = env
                        && !env.is_empty()
                    {
                        let mut env_table = TomlTable::new();
                        env_table.set_implicit(false);
                        let mut pairs: Vec<_> = env.iter().collect();
                        pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
                        for (key, value) in pairs {
                            env_table.insert(key, toml_edit::value(value.clone()));
                        }
                        entry["env"] = TomlItem::Table(env_table);
                    }
                }
                McpServerTransportConfig::StreamableHttp { url, bearer_token } => {
                    entry["url"] = toml_edit::value(url.clone());
                    if let Some(token) = bearer_token {
                        entry["bearer_token"] = toml_edit::value(token.clone());
                    }
                }
            }

            if let Some(timeout) = config.startup_timeout_sec {
                entry["startup_timeout_sec"] = toml_edit::value(timeout.as_secs_f64());
            }

            if let Some(timeout) = config.tool_timeout_sec {
                entry["tool_timeout_sec"] = toml_edit::value(timeout.as_secs_f64());
            }

            doc["mcp_servers"][name.as_str()] = TomlItem::Table(entry);
        }
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path).map_err(|err| err.error)?;

    Ok(())
}

/// Persist the currently active model selection back to `config.toml` so that it
/// becomes the default for future sessions.
pub async fn persist_model_selection(
    code_home: &Path,
    profile: Option<&str>,
    model: &str,
    effort: Option<ReasoningEffort>,
) -> anyhow::Result<()> {
    use tokio::fs;

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let existing = match fs::read_to_string(&read_path).await {
        Ok(raw) => Some(raw),
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => return Err(err.into()),
    };

    let mut doc = match existing {
        Some(raw) if raw.trim().is_empty() => DocumentMut::new(),
        Some(raw) => raw
            .parse::<DocumentMut>()
            .map_err(|e| anyhow::anyhow!("failed to parse config.toml: {e}"))?,
        None => DocumentMut::new(),
    };

    {
        let root = doc.as_table_mut();
        if let Some(profile_name) = profile {
            let profiles_item = root
                .entry("profiles")
                .or_insert_with(|| {
                    let mut table = TomlTable::new();
                    table.set_implicit(true);
                    TomlItem::Table(table)
                });

            let profiles_table = profiles_item
                .as_table_mut()
                .expect("profiles table should be a table");

            let profile_item = profiles_table
                .entry(profile_name)
                .or_insert_with(|| {
                    let mut table = TomlTable::new();
                    table.set_implicit(false);
                    TomlItem::Table(table)
                });

            let profile_table = profile_item
                .as_table_mut()
                .expect("profile entry should be a table");

            profile_table["model"] = toml_edit::value(model.to_string());

            if let Some(effort) = effort {
                profile_table["model_reasoning_effort"] =
                    toml_edit::value(effort.to_string());
            } else {
                profile_table.remove("model_reasoning_effort");
            }
        } else {
            root["model"] = toml_edit::value(model.to_string());
            match effort {
                Some(effort) => {
                    root["model_reasoning_effort"] =
                        toml_edit::value(effort.to_string());
                }
                None => {
                    root.remove("model_reasoning_effort");
                }
            }
        }
    }

    fs::create_dir_all(code_home).await?;
    let tmp_path = config_path.with_extension("tmp");
    fs::write(&tmp_path, doc.to_string()).await?;
    fs::rename(&tmp_path, &config_path).await?;

    Ok(())
}

/// Patch `CODEX_HOME/config.toml` project state.
/// Use with caution.
pub fn set_project_trusted(code_home: &Path, project_path: &Path) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    set_project_trusted_inner(&mut doc, project_path)?;

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

fn set_project_trusted_inner(doc: &mut DocumentMut, project_path: &Path) -> anyhow::Result<()> {
    // Ensure we render a human-friendly structure:
    //
    // [projects]
    // [projects."/path/to/project"]
    // trust_level = "trusted"
    //
    // rather than inline tables like:
    //
    // [projects]
    // "/path/to/project" = { trust_level = "trusted" }
    let project_key = project_path.to_string_lossy().to_string();

    // Ensure top-level `projects` exists as a non-inline, explicit table. If it
    // exists but was previously represented as a non-table (e.g., inline),
    // replace it with an explicit table.
    let mut created_projects_table = false;
    {
        let root = doc.as_table_mut();
        let needs_table = !root.contains_key("projects")
            || root.get("projects").and_then(|i| i.as_table()).is_none();
        if needs_table {
            root.insert("projects", toml_edit::table());
            created_projects_table = true;
        }
    }
    let Some(projects_tbl) = doc["projects"].as_table_mut() else {
        return Err(anyhow::anyhow!(
            "projects table missing after initialization"
        ));
    };

    // If we created the `projects` table ourselves, keep it implicit so we
    // don't render a standalone `[projects]` header.
    if created_projects_table {
        projects_tbl.set_implicit(true);
    }

    // Ensure the per-project entry is its own explicit table. If it exists but
    // is not a table (e.g., an inline table), replace it with an explicit table.
    let needs_proj_table = !projects_tbl.contains_key(project_key.as_str())
        || projects_tbl
            .get(project_key.as_str())
            .and_then(|i| i.as_table())
            .is_none();
    if needs_proj_table {
        projects_tbl.insert(project_key.as_str(), toml_edit::table());
    }
    let Some(proj_tbl) = projects_tbl
        .get_mut(project_key.as_str())
        .and_then(|i| i.as_table_mut())
    else {
        return Err(anyhow::anyhow!("project table missing for {}", project_key));
    };
    proj_tbl.set_implicit(false);
    proj_tbl["trust_level"] = toml_edit::value("trusted");

    Ok(())
}

/// Persist the selected TUI theme into `CODEX_HOME/config.toml` at `[tui.theme].name`.
pub fn set_tui_theme_name(code_home: &Path, theme: ThemeName) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Map enum to kebab-case string used in config
    let theme_str = match theme {
        ThemeName::LightPhoton => "light-photon",
        ThemeName::LightPhotonAnsi16 => "light-photon-ansi16",
        ThemeName::LightPrismRainbow => "light-prism-rainbow",
        ThemeName::LightVividTriad => "light-vivid-triad",
        ThemeName::LightPorcelain => "light-porcelain",
        ThemeName::LightSandbar => "light-sandbar",
        ThemeName::LightGlacier => "light-glacier",
        ThemeName::DarkCarbonNight => "dark-carbon-night",
        ThemeName::DarkCarbonAnsi16 => "dark-carbon-ansi16",
        ThemeName::DarkShinobiDusk => "dark-shinobi-dusk",
        ThemeName::DarkOledBlackPro => "dark-oled-black-pro",
        ThemeName::DarkAmberTerminal => "dark-amber-terminal",
        ThemeName::DarkAuroraFlux => "dark-aurora-flux",
        ThemeName::DarkCharcoalRainbow => "dark-charcoal-rainbow",
        ThemeName::DarkZenGarden => "dark-zen-garden",
        ThemeName::DarkPaperLightPro => "dark-paper-light-pro",
        ThemeName::Custom => "custom",
    };

    // Write `[tui.theme].name = "…"`
    doc["tui"]["theme"]["name"] = toml_edit::value(theme_str);
    // When switching away from the Custom theme, clear any lingering custom
    // overrides so built-in themes render true to spec on next startup.
    if theme != ThemeName::Custom {
        if let Some(tbl) = doc["tui"]["theme"].as_table_mut() {
            tbl.remove("label");
            tbl.remove("colors");
        }
    }

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Record the most recent terminal background autodetect result under `[tui.cached_terminal_background]`.
pub fn set_cached_terminal_background(
    code_home: &Path,
    cache: &CachedTerminalBackground,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let mut tbl = toml_edit::Table::new();
    tbl.set_implicit(false);
    tbl.insert("is_dark", toml_edit::value(cache.is_dark));
    if let Some(term) = &cache.term {
        tbl.insert("term", toml_edit::value(term.as_str()));
    }
    if let Some(term_program) = &cache.term_program {
        tbl.insert("term_program", toml_edit::value(term_program.as_str()));
    }
    if let Some(term_program_version) = &cache.term_program_version {
        tbl.insert(
            "term_program_version",
            toml_edit::value(term_program_version.as_str()),
        );
    }
    if let Some(colorfgbg) = &cache.colorfgbg {
        tbl.insert("colorfgbg", toml_edit::value(colorfgbg.as_str()));
    }
    if let Some(source) = &cache.source {
        tbl.insert("source", toml_edit::value(source.as_str()));
    }
    if let Some(rgb) = &cache.rgb {
        tbl.insert("rgb", toml_edit::value(rgb.as_str()));
    }

    doc["tui"]["cached_terminal_background"] = toml_edit::Item::Table(tbl);

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;
    Ok(())
}

/// Persist the selected spinner into `CODEX_HOME/config.toml` at `[tui.spinner].name`.
pub fn set_tui_spinner_name(code_home: &Path, spinner_name: &str) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Write `[tui.spinner].name = "…"`
    doc["tui"]["spinner"]["name"] = toml_edit::value(spinner_name);

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Save or update a custom spinner under `[tui.spinner.custom.<id>]` with a display `label`,
/// and set it active by writing `[tui.spinner].name = <id>`.
pub fn set_custom_spinner(
    code_home: &Path,
    id: &str,
    label: &str,
    interval: u64,
    frames: &[String],
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };
    // Write custom spinner
    let node = &mut doc["tui"]["spinner"]["custom"][id];
    node["interval"] = toml_edit::value(interval as i64);
    let mut arr = toml_edit::Array::default();
    for s in frames { arr.push(s.as_str()); }
    node["frames"] = toml_edit::value(arr);
    node["label"] = toml_edit::value(label);

    // Set as active
    doc["tui"]["spinner"]["name"] = toml_edit::value(id);

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Save or update a custom theme with a display `label` and color overrides
/// under `[tui.theme]`, and set it active by writing `[tui.theme].name = "custom"`.
pub fn set_custom_theme(
    code_home: &Path,
    label: &str,
    colors: &ThemeColors,
    set_active: bool,
    is_dark: Option<bool>,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Optionally activate custom theme and persist label
    if set_active {
        doc["tui"]["theme"]["name"] = toml_edit::value("custom");
    }
    doc["tui"]["theme"]["label"] = toml_edit::value(label);
    if let Some(d) = is_dark { doc["tui"]["theme"]["is_dark"] = toml_edit::value(d); }

    // Ensure colors table exists and write provided keys
    {
        use toml_edit::Item as It;
        if !doc["tui"]["theme"].is_table() {
            doc["tui"]["theme"] = It::Table(toml_edit::Table::new());
        }
        let theme_tbl = doc["tui"]["theme"].as_table_mut().unwrap();
        if !theme_tbl.contains_key("colors") {
            theme_tbl.insert("colors", It::Table(toml_edit::Table::new()));
        }
    let colors_tbl = theme_tbl["colors"].as_table_mut().unwrap();
        macro_rules! set_opt {
            ($key:ident) => {
                if let Some(ref v) = colors.$key { colors_tbl.insert(stringify!($key), toml_edit::value(v.clone())); }
            };
        }
        set_opt!(primary);
        set_opt!(secondary);
        set_opt!(background);
        set_opt!(foreground);
        set_opt!(border);
        set_opt!(border_focused);
        set_opt!(selection);
        set_opt!(cursor);
        set_opt!(success);
        set_opt!(warning);
        set_opt!(error);
        set_opt!(info);
        set_opt!(text);
        set_opt!(text_dim);
        set_opt!(text_bright);
        set_opt!(keyword);
        set_opt!(string);
        set_opt!(comment);
        set_opt!(function);
        set_opt!(spinner);
        set_opt!(progress);
    }

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Persist the alternate screen preference into `CODEX_HOME/config.toml` at `[tui].alternate_screen`.
pub fn set_tui_alternate_screen(code_home: &Path, enabled: bool) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Write `[tui].alternate_screen = true/false`
    doc["tui"]["alternate_screen"] = toml_edit::value(enabled);

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the TUI notifications preference into `CODEX_HOME/config.toml` at `[tui].notifications`.
pub fn set_tui_notifications(
    code_home: &Path,
    notifications: crate::config_types::Notifications,
) -> anyhow::Result<()> {
    use crate::config_types::Notifications;

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    match notifications {
        Notifications::Enabled(value) => {
            doc["tui"]["notifications"] = toml_edit::value(value);
        }
        Notifications::Custom(values) => {
            let mut array = TomlArray::default();
            for value in values {
                array.push(value);
            }
            doc["tui"]["notifications"] = TomlItem::Value(array.into());
        }
    }

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the review auto-resolve preference into `CODEX_HOME/config.toml` at `[tui].review_auto_resolve`.
pub fn set_tui_review_auto_resolve(code_home: &Path, enabled: bool) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));

    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["tui"]["review_auto_resolve"] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist the review model + reasoning effort into `CODEX_HOME/config.toml`.
pub fn set_review_model(
    code_home: &Path,
    model: &str,
    effort: ReasoningEffort,
) -> anyhow::Result<()> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return Err(anyhow::anyhow!("review model cannot be empty"));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["review_model"] = toml_edit::value(trimmed);
    doc["review_model_reasoning_effort"] =
        toml_edit::value(effort.to_string().to_ascii_lowercase());

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist Auto Drive defaults under `[auto_drive]`.
pub fn set_auto_drive_settings(
    code_home: &Path,
    settings: &AutoDriveSettings,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));

    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(contents) => contents.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    if let Some(tui_tbl) = doc["tui"].as_table_mut() {
        tui_tbl.remove("auto_drive");
    }

    doc["auto_drive"]["review_enabled"] = toml_edit::value(settings.review_enabled);
    doc["auto_drive"]["agents_enabled"] = toml_edit::value(settings.agents_enabled);
    doc["auto_drive"]["qa_automation_enabled"] =
        toml_edit::value(settings.qa_automation_enabled);
    doc["auto_drive"]["cross_check_enabled"] =
        toml_edit::value(settings.cross_check_enabled);
    doc["auto_drive"]["observer_enabled"] =
        toml_edit::value(settings.observer_enabled);
    doc["auto_drive"]["coordinator_routing"] =
        toml_edit::value(settings.coordinator_routing);
    doc["auto_drive"]["auto_resolve_review_attempts"] =
        toml_edit::value(settings.auto_resolve_review_attempts.get() as i64);

    let mode_str = match settings.continue_mode {
        AutoDriveContinueMode::Immediate => "immediate",
        AutoDriveContinueMode::TenSeconds => "ten-seconds",
        AutoDriveContinueMode::SixtySeconds => "sixty-seconds",
        AutoDriveContinueMode::Manual => "manual",
    };
    doc["auto_drive"]["continue_mode"] = toml_edit::value(mode_str);

    std::fs::create_dir_all(code_home)?;
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Legacy helper: persist Auto Drive defaults under `[auto_drive]` while
/// accepting the former API surface.
#[deprecated(note = "use set_auto_drive_settings instead")]
pub fn set_tui_auto_drive_settings(
    code_home: &Path,
    settings: &AutoDriveSettings,
) -> anyhow::Result<()> {
    set_auto_drive_settings(code_home, settings)
}

/// Persist the GitHub workflow check preference under `[github].check_workflows_on_push`.
pub fn set_github_check_on_push(code_home: &Path, enabled: bool) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Write `[github].check_workflows_on_push = <enabled>`
    doc["github"]["check_workflows_on_push"] = toml_edit::value(enabled);

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;

    // create a tmp_file
    let tmp_file = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp_file.path(), doc.to_string())?;

    // atomically move the tmp file into config.toml
    tmp_file.persist(config_path)?;

    Ok(())
}

/// Persist `github.actionlint_on_patch = <enabled>`.
pub fn set_github_actionlint_on_patch(
    code_home: &Path,
    enabled: bool,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["github"]["actionlint_on_patch"] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Persist `[validation.groups.<group>] = <enabled>`.
pub fn set_validation_group_enabled(
    code_home: &Path,
    group: &str,
    enabled: bool,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["validation"]["groups"][group] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Persist `[validation.tools.<tool>] = <enabled>`.
pub fn set_validation_tool_enabled(
    code_home: &Path,
    tool: &str,
    enabled: bool,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    doc["validation"]["tools"][tool] = toml_edit::value(enabled);

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Persist per-project access mode under `[projects."<path>"]` with
/// `approval_policy` and `sandbox_mode`.
pub fn set_project_access_mode(
    code_home: &Path,
    project_path: &Path,
    approval: AskForApproval,
    sandbox_mode: SandboxMode,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);

    // Parse existing config if present; otherwise start a new document.
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Ensure projects table and the per-project table exist
    let project_key = project_path.to_string_lossy().to_string();
    // Ensure `projects` is a table; if key exists but is not a table, replace it.
    let has_projects_table = doc
        .as_table()
        .get("projects")
        .and_then(|i| i.as_table())
        .is_some();
    if !has_projects_table {
        doc["projects"] = TomlItem::Table(toml_edit::Table::new());
    }
    let Some(projects_tbl) = doc["projects"].as_table_mut() else {
        return Err(anyhow::anyhow!("failed to prepare projects table"));
    };
    // Ensure per-project entry exists and is a table; replace if wrong type.
    let needs_proj_table = projects_tbl
        .get(project_key.as_str())
        .and_then(|i| i.as_table())
        .is_none();
    if needs_proj_table {
        projects_tbl.insert(project_key.as_str(), TomlItem::Table(toml_edit::Table::new()));
    }
    let proj_tbl = projects_tbl
        .get_mut(project_key.as_str())
        .and_then(|i| i.as_table_mut())
        .ok_or_else(|| anyhow::anyhow!(format!("failed to create projects.{} table", project_key)))?;

    // Write fields
    proj_tbl.insert(
        "approval_policy",
        TomlItem::Value(toml_edit::Value::from(format!("{}", approval))),
    );
    proj_tbl.insert(
        "sandbox_mode",
        TomlItem::Value(toml_edit::Value::from(format!("{}", sandbox_mode))),
    );

    // Harmonize trust_level with selected access mode:
    // - Full Access (Never + DangerFullAccess): set trust_level = "trusted" so future runs
    //   default to non-interactive behavior when no overrides are present.
    // - Other modes: remove trust_level to avoid conflicting with per-project policy.
    let full_access = matches!(
        (approval, sandbox_mode),
        (AskForApproval::Never, SandboxMode::DangerFullAccess)
    );
    if full_access {
        proj_tbl.insert(
            "trust_level",
            TomlItem::Value(toml_edit::Value::from("trusted")),
        );
    } else {
        proj_tbl.remove("trust_level");
    }

    // Ensure home exists; write atomically
    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;

    Ok(())
}

/// Append a command pattern to `[projects."<path>"].always_allow_commands`.
pub fn add_project_allowed_command(
    code_home: &Path,
    project_path: &Path,
    command: &[String],
    match_kind: ApprovedCommandMatchKind,
) -> anyhow::Result<()> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    let project_key = project_path.to_string_lossy().to_string();
    if doc
        .as_table()
        .get("projects")
        .and_then(|i| i.as_table())
        .is_none()
    {
        doc["projects"] = TomlItem::Table(TomlTable::new());
    }

    let Some(projects_tbl) = doc["projects"].as_table_mut() else {
        return Err(anyhow::anyhow!("failed to prepare projects table"));
    };

    if projects_tbl
        .get(project_key.as_str())
        .and_then(|i| i.as_table())
        .is_none()
    {
        projects_tbl.insert(project_key.as_str(), TomlItem::Table(TomlTable::new()));
    }

    let project_tbl = projects_tbl
        .get_mut(project_key.as_str())
        .and_then(|i| i.as_table_mut())
        .ok_or_else(|| anyhow::anyhow!(format!("failed to create projects.{} table", project_key)))?;

    let mut argv_array = TomlArray::new();
    for arg in command {
        argv_array.push(arg.clone());
    }

    let mut table = TomlTable::new();
    table.insert("argv", TomlItem::Value(toml_edit::Value::Array(argv_array)));
    let match_str = match match_kind {
        ApprovedCommandMatchKind::Exact => "exact",
        ApprovedCommandMatchKind::Prefix => "prefix",
    };
    table.insert(
        "match_kind",
        TomlItem::Value(toml_edit::Value::from(match_str)),
    );

    if let Some(existing) = project_tbl
        .get_mut("always_allow_commands")
        .and_then(|item| item.as_array_of_tables_mut())
    {
        let exists = existing.iter().any(|tbl| {
            let argv_match = tbl
                .get("argv")
                .and_then(|item| item.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(ToString::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let match_kind = tbl
                .get("match_kind")
                .and_then(|item| item.as_str())
                .unwrap_or("exact");
            argv_match == command && match_kind.eq_ignore_ascii_case(match_str)
        });
        if !exists {
            existing.push(table);
        }
    } else {
        let mut arr = TomlArrayOfTables::new();
        arr.push(table);
        project_tbl.insert("always_allow_commands", TomlItem::ArrayOfTables(arr));
    }

    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;

    Ok(())
}

/// List MCP servers from `CODEX_HOME/config.toml`.
/// Returns `(enabled, disabled)` lists of `(name, McpServerConfig)`.
pub fn list_mcp_servers(code_home: &Path) -> anyhow::Result<(
    Vec<(String, McpServerConfig)>,
    Vec<(String, McpServerConfig)>,
)> {
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let doc_str = std::fs::read_to_string(&read_path).unwrap_or_default();
    let doc = doc_str.parse::<DocumentMut>().unwrap_or_else(|_| DocumentMut::new());

    fn table_to_list(tbl: &toml_edit::Table) -> Vec<(String, McpServerConfig)> {
        let mut out = Vec::new();
        for (name, item) in tbl.iter() {
            if let Some(t) = item.as_table() {
                let transport = if let Some(command) = t.get("command").and_then(|v| v.as_str()) {
                    let args: Vec<String> = t
                        .get("args")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|i| i.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let env = t
                        .get("env")
                        .and_then(|v| {
                            if let Some(tbl) = v.as_inline_table() {
                                Some(
                                    tbl.iter()
                                        .filter_map(|(k, v)| {
                                            v.as_str().map(|s| (k.to_string(), s.to_string()))
                                        })
                                        .collect::<HashMap<_, _>>(),
                                )
                            } else if let Some(table) = v.as_table() {
                                Some(
                                    table
                                        .iter()
                                        .filter_map(|(k, v)| {
                                            v.as_str().map(|s| (k.to_string(), s.to_string()))
                                        })
                                        .collect::<HashMap<_, _>>(),
                                )
                            } else {
                                None
                            }
                        });

                    McpServerTransportConfig::Stdio {
                        command: command.to_string(),
                        args,
                        env,
                    }
                } else if let Some(url) = t.get("url").and_then(|v| v.as_str()) {
                    let bearer_token = t
                        .get("bearer_token")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    McpServerTransportConfig::StreamableHttp {
                        url: url.to_string(),
                        bearer_token,
                    }
                } else {
                    continue;
                };

                let startup_timeout_sec = t
                    .get("startup_timeout_sec")
                    .and_then(|v| {
                        v.as_float()
                            .map(|f| Duration::try_from_secs_f64(f).ok())
                            .or_else(|| {
                                Some(v.as_integer().map(|i| Duration::from_secs(i as u64)))
                            })
                    })
                    .flatten()
                    .or_else(|| {
                        t.get("startup_timeout_ms")
                            .and_then(|v| v.as_integer())
                            .map(|ms| Duration::from_millis(ms as u64))
                    });

                let tool_timeout_sec = t
                    .get("tool_timeout_sec")
                    .and_then(|v| {
                        v.as_float()
                            .map(|f| Duration::try_from_secs_f64(f).ok())
                            .or_else(|| {
                                Some(v.as_integer().map(|i| Duration::from_secs(i as u64)))
                            })
                    })
                    .flatten();

                out.push((
                    name.to_string(),
                    McpServerConfig {
                        transport,
                        startup_timeout_sec,
                        tool_timeout_sec,
                    },
                ));
            }
        }
        out
    }

    let enabled = doc
        .as_table()
        .get("mcp_servers")
        .and_then(|i| i.as_table())
        .map(table_to_list)
        .unwrap_or_default();

    let disabled = doc
        .as_table()
        .get("mcp_servers_disabled")
        .and_then(|i| i.as_table())
        .map(table_to_list)
        .unwrap_or_default();

    Ok((enabled, disabled))
}

/// Add or update an MCP server under `[mcp_servers.<name>]`. If the same
/// server exists under `mcp_servers_disabled`, it will be removed from there.
pub fn add_mcp_server(
    code_home: &Path,
    name: &str,
    cfg: McpServerConfig,
) -> anyhow::Result<()> {
    // Validate server name for safety and compatibility with MCP tool naming.
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(anyhow::anyhow!(
            "invalid server name '{}': must match ^[a-zA-Z0-9_-]+$",
            name
        ));
    }

    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Ensure target tables exist
    if !doc.as_table().contains_key("mcp_servers") {
        doc["mcp_servers"] = TomlItem::Table(toml_edit::Table::new());
    }
    let tbl = doc["mcp_servers"].as_table_mut().unwrap();

    let McpServerConfig {
        transport,
        startup_timeout_sec,
        tool_timeout_sec,
    } = cfg;

    // Build table for this server
    let mut server_tbl = toml_edit::Table::new();
    match transport {
        McpServerTransportConfig::Stdio { command, args, env } => {
            server_tbl.insert("command", toml_edit::value(command));
            if !args.is_empty() {
                let mut arr = toml_edit::Array::new();
                for a in args {
                    arr.push(toml_edit::Value::from(a));
                }
                server_tbl.insert("args", TomlItem::Value(toml_edit::Value::Array(arr)));
            }
            if let Some(env) = env {
                let mut it = toml_edit::InlineTable::new();
                for (k, v) in env {
                    it.insert(&k, toml_edit::Value::from(v));
                }
                server_tbl.insert("env", TomlItem::Value(toml_edit::Value::InlineTable(it)));
            }
        }
        McpServerTransportConfig::StreamableHttp { url, bearer_token } => {
            server_tbl.insert("url", toml_edit::value(url));
            if let Some(token) = bearer_token {
                server_tbl.insert("bearer_token", toml_edit::value(token));
            }
        }
    }

    if let Some(duration) = startup_timeout_sec {
        server_tbl.insert("startup_timeout_sec", toml_edit::value(duration.as_secs_f64()));
    }
    if let Some(duration) = tool_timeout_sec {
        server_tbl.insert("tool_timeout_sec", toml_edit::value(duration.as_secs_f64()));
    }

    // Write into enabled table
    tbl.insert(name, TomlItem::Table(server_tbl));

    // Remove from disabled if present
    if let Some(disabled_tbl) = doc["mcp_servers_disabled"].as_table_mut() {
        disabled_tbl.remove(name);
    }

    // ensure code_home exists
    std::fs::create_dir_all(code_home)?;
    let tmp = NamedTempFile::new_in(code_home)?;
    std::fs::write(tmp.path(), doc.to_string())?;
    tmp.persist(config_path)?;
    Ok(())
}

/// Enable/disable an MCP server by moving it between `[mcp_servers]` and
/// `[mcp_servers_disabled]`. Returns `true` if a change was made.
pub fn set_mcp_server_enabled(
    code_home: &Path,
    name: &str,
    enabled: bool,
) -> anyhow::Result<bool> {
    let config_path = code_home.join(CONFIG_TOML_FILE);
    let read_path = resolve_code_path_for_read(code_home, Path::new(CONFIG_TOML_FILE));
    let mut doc = match std::fs::read_to_string(&read_path) {
        Ok(s) => s.parse::<DocumentMut>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => DocumentMut::new(),
        Err(e) => return Err(e.into()),
    };

    // Helper to ensure table exists
    fn ensure_table<'a>(doc: &'a mut DocumentMut, key: &'a str) -> &'a mut toml_edit::Table {
        if !doc.as_table().contains_key(key) {
            doc[key] = TomlItem::Table(toml_edit::Table::new());
        }
        doc[key].as_table_mut().unwrap()
    }

    let mut changed = false;
    if enabled {
        // Move from disabled -> enabled
        let moved = {
            let disabled_tbl = ensure_table(&mut doc, "mcp_servers_disabled");
            disabled_tbl.remove(name)
        };
        if let Some(item) = moved {
            let enabled_tbl = ensure_table(&mut doc, "mcp_servers");
            enabled_tbl.insert(name, item);
            changed = true;
        }
    } else {
        // Move from enabled -> disabled
        let moved = {
            let enabled_tbl = ensure_table(&mut doc, "mcp_servers");
            enabled_tbl.remove(name)
        };
        if let Some(item) = moved {
            let disabled_tbl = ensure_table(&mut doc, "mcp_servers_disabled");
            disabled_tbl.insert(name, item);
            changed = true;
        }
    }

    if changed {
        std::fs::create_dir_all(code_home)?;
        let tmp = NamedTempFile::new_in(code_home)?;
        std::fs::write(tmp.path(), doc.to_string())?;
        tmp.persist(config_path)?;
    }

    Ok(changed)
}

/// Apply a single dotted-path override onto a TOML value.
fn apply_toml_override(root: &mut TomlValue, path: &str, value: TomlValue) {
    use toml::value::Table;

    let segments: Vec<&str> = path.split('.').collect();
    let mut current = root;

    for (idx, segment) in segments.iter().enumerate() {
        let is_last = idx == segments.len() - 1;

        if is_last {
            match current {
                TomlValue::Table(table) => {
                    table.insert(segment.to_string(), value);
                }
                _ => {
                    let mut table = Table::new();
                    table.insert(segment.to_string(), value);
                    *current = TomlValue::Table(table);
                }
            }
            return;
        }

        // Traverse or create intermediate object.
        match current {
            TomlValue::Table(table) => {
                current = table
                    .entry(segment.to_string())
                    .or_insert_with(|| TomlValue::Table(Table::new()));
            }
            _ => {
                *current = TomlValue::Table(Table::new());
                if let TomlValue::Table(tbl) = current {
                    current = tbl
                        .entry(segment.to_string())
                        .or_insert_with(|| TomlValue::Table(Table::new()));
                }
            }
        }
    }
}

/// Base config deserialized from ~/.code/config.toml (legacy ~/.codex/config.toml is still read).
#[derive(Deserialize, Debug, Clone, Default)]
pub struct ConfigToml {
    /// Optional override of model selection.
    pub model: Option<String>,
    /// Review model override used by the `/review` feature.
    pub review_model: Option<String>,
    /// Reasoning effort override used for the review model.
    pub review_model_reasoning_effort: Option<ReasoningEffort>,

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

    #[serde(default)]
    pub experimental_use_rmcp_client: Option<bool>,

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
    pub model_reasoning_summary: Option<ReasoningSummary>,
    pub model_text_verbosity: Option<TextVerbosity>,

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

    /// Nested tools section for feature toggles
    pub tools: Option<ToolsToml>,

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

    /// Enable the `view_image` tool that lets the agent attach local images.
    #[serde(default)]
    pub view_image: Option<bool>,
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

fn upgrade_legacy_model_slugs(cfg: &mut ConfigToml) {
    fn maybe_upgrade(field: &mut Option<String>) {
        if let Some(old) = field.clone() {
            if let Some(new) = upgrade_legacy_model_slug(&old) {
                tracing::info!(
                    target: "code.config",
                    old,
                    new,
                    "upgrading legacy gpt-5 model slug to gpt-5.1",
                );
                *field = Some(new);
            }
        }
    }

    maybe_upgrade(&mut cfg.model);
    maybe_upgrade(&mut cfg.review_model);

    for profile in cfg.profiles.values_mut() {
        maybe_upgrade(&mut profile.model);
        maybe_upgrade(&mut profile.review_model);
    }
}

fn upgrade_legacy_model_slug(slug: &str) -> Option<String> {
    if slug.starts_with("gpt-5.1") || slug.starts_with("test-gpt-5.1") {
        return None;
    }

    if let Some(rest) = slug.strip_prefix("test-gpt-5-codex") {
        return Some(format!("test-gpt-5.1-codex{rest}"));
    }

    if let Some(rest) = slug.strip_prefix("test-gpt-5") {
        return Some(format!("test-gpt-5.1{rest}"));
    }

    if let Some(rest) = slug.strip_prefix("gpt-5-codex") {
        return Some(format!("gpt-5.1-codex{rest}"));
    }

    if let Some(rest) = slug.strip_prefix("gpt-5") {
        return Some(format!("gpt-5.1{rest}"));
    }

    None
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

        let env_ctx_v2_flag = *crate::flags::CTX_UI;

        // Determine auth mode early so defaults like model selection can depend on it.
        let using_chatgpt_auth = Self::is_using_chatgpt_auth(&code_home);

        let default_model_slug = if using_chatgpt_auth {
            GPT_5_CODEX_MEDIUM_MODEL
        } else {
            OPENAI_DEFAULT_MODEL
        };

        let model = model
            .or(config_profile.model)
            .or(cfg.model)
            .unwrap_or_else(|| default_model_slug.to_string());

        let model_family =
            find_family_for_model(&model).unwrap_or_else(|| derive_default_model_family(&model));

        let openai_model_info = get_model_info(&model_family);
        let model_context_window = cfg
            .model_context_window
            .or_else(|| openai_model_info.as_ref().map(|info| info.context_window));
        let model_max_output_tokens = cfg.model_max_output_tokens.or_else(|| {
            openai_model_info
                .as_ref()
                .map(|info| info.max_output_tokens)
        });
        let model_auto_compact_token_limit = cfg.model_auto_compact_token_limit.or_else(|| {
            openai_model_info
                .as_ref()
                .and_then(|info| info.auto_compact_token_limit)
        });

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

        // Normalize agents: when `command` is missing/empty, default to `name`.
        let mut agents: Vec<AgentConfig> = cfg
            .agents
            .into_iter()
            .map(|mut a| {
                let command_trimmed = a.command.trim();
                if command_trimmed.is_empty() {
                    if let Some(spec) = agent_model_spec(&a.name) {
                        a.command = spec.cli.to_string();
                    } else {
                        a.command = a.name.clone();
                    }
                }

                a
            })
            .collect();

        if agents.is_empty() {
            agents = default_agent_configs();
        } else {
            let mut seen = HashSet::new();
            for agent in &agents {
                let canonical = agent_model_spec(&agent.name)
                    .map(|spec| spec.slug.to_ascii_lowercase())
                    .unwrap_or_else(|| agent.name.to_ascii_lowercase());
                seen.insert(canonical);
            }

            for default_agent in default_agent_configs() {
                let key = default_agent.name.to_ascii_lowercase();
                if !seen.contains(&key) {
                    seen.insert(key);
                    agents.push(default_agent);
                }
            }
        }

        for agent in &agents {
            if agent.name.eq_ignore_ascii_case("code")
                || agent.name.eq_ignore_ascii_case("codex")
                || agent.name.eq_ignore_ascii_case("claude")
                || agent.name.eq_ignore_ascii_case("gemini")
                || agent.name.eq_ignore_ascii_case("qwen")
                || agent.name.eq_ignore_ascii_case("cloud")
            {
                tracing::warn!(
                    "legacy agent name '{}' detected; update config to use model slugs (e.g., code-gpt-5.1-codex)",
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

        let config = Self {
            model,
            review_model,
            review_model_reasoning_effort,
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
            user_instructions,
            base_instructions,
            compact_prompt_override,
            mcp_servers: cfg.mcp_servers,
            experimental_client_tools: cfg.experimental_client_tools.clone(),
            agents,
            model_providers,
            project_doc_max_bytes: cfg.project_doc_max_bytes.unwrap_or(PROJECT_DOC_MAX_BYTES),
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
            auto_drive: cfg
                .auto_drive
                .clone()
                .or_else(|| cfg.tui.as_ref().and_then(|t| t.auto_drive.clone()))
                .unwrap_or_default(),
            code_linux_sandbox_exe,
            active_profile: active_profile_name,

            hide_agent_reasoning: cfg.hide_agent_reasoning.unwrap_or(false),
            show_raw_agent_reasoning: cfg
                .show_raw_agent_reasoning
                .or(show_raw_agent_reasoning)
                .unwrap_or(false),
            model_reasoning_effort: config_profile
                .model_reasoning_effort
                .or(cfg.model_reasoning_effort)
                .unwrap_or(ReasoningEffort::Medium),
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
            use_experimental_use_rmcp_client: cfg
                .experimental_use_rmcp_client
                .unwrap_or(false),
            include_view_image_tool: include_view_image_tool_flag,
            env_ctx_v2: env_ctx_v2_flag,
            retention: crate::config_types::RetentionConfig::default(),
            responses_originator_header,
            debug: debug.unwrap_or(false),
            // Already computed before moving code_home
            using_chatgpt_auth,
            github: cfg.github.unwrap_or_default(),
            validation: cfg.validation.unwrap_or_default(),
            subagent_commands: cfg
                .subagents
                .map(|s| s.commands)
                .unwrap_or_default(),
            experimental_resume: cfg.experimental_resume,
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
        let code_home = code_dir?;
        let read_path = resolve_code_path_for_read(code_home, Path::new("AGENTS.md"));

        let contents = match std::fs::read_to_string(&read_path) {
            Ok(s) => s,
            Err(_) => {
                if env_overrides_present() {
                    return None;
                }
                let Some(legacy_home) = legacy_code_home_dir() else {
                    return None;
                };
                let legacy_path = legacy_home.join("AGENTS.md");
                match std::fs::read_to_string(&legacy_path) {
                    Ok(s) => s,
                    Err(_) => return None,
                }
            }
        };

        let trimmed = contents.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    fn read_override_file(
        path: Option<&PathBuf>,
        cwd: &Path,
        description: &str,
    ) -> std::io::Result<Option<String>> {
        let p = match path.as_ref() {
            None => return Ok(None),
            Some(p) => p,
        };

        // Resolve relative paths against the provided cwd to make CLI
        // overrides consistent regardless of where the process was launched
        // from.
        let full_path = if p.is_relative() {
            cwd.join(p)
        } else {
            p.to_path_buf()
        };

        let contents = std::fs::read_to_string(&full_path).map_err(|e| {
            std::io::Error::new(
                e.kind(),
                format!("failed to read {description} {}: {e}", full_path.display()),
            )
        })?;

        let s = contents.trim().to_string();
        if s.is_empty() {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{description} is empty: {}", full_path.display()),
            ))
        } else {
            Ok(Some(s))
        }
    }

    fn get_base_instructions(
        path: Option<&PathBuf>,
        cwd: &Path,
    ) -> std::io::Result<Option<String>> {
        Self::read_override_file(path, cwd, "experimental instructions file")
    }

    fn get_compact_prompt_override(
        path: Option<&PathBuf>,
        cwd: &Path,
    ) -> std::io::Result<Option<String>> {
        Self::read_override_file(path, cwd, "compact prompt override file")
    }
}

fn default_review_model() -> String {
    OPENAI_DEFAULT_REVIEW_MODEL.to_string()
}

fn env_path(var: &str) -> std::io::Result<Option<PathBuf>> {
    match std::env::var(var) {
        Ok(val) if !val.trim().is_empty() => {
            let canonical = PathBuf::from(val).canonicalize()?;
            Ok(Some(canonical))
        }
        _ => Ok(None),
    }
}

fn env_overrides_present() -> bool {
    matches!(std::env::var("CODE_HOME"), Ok(ref v) if !v.trim().is_empty())
        || matches!(std::env::var("CODEX_HOME"), Ok(ref v) if !v.trim().is_empty())
}

fn default_code_home_dir() -> Option<PathBuf> {
    let mut path = home_dir()?;
    path.push(".code");
    Some(path)
}

fn compute_legacy_code_home_dir() -> Option<PathBuf> {
    if env_overrides_present() {
        return None;
    }
    let Some(home) = home_dir() else {
        return None;
    };
    let candidate = home.join(".codex");
    if path_exists(&candidate) {
        Some(candidate)
    } else {
        None
    }
}

fn legacy_code_home_dir() -> Option<PathBuf> {
    #[cfg(test)]
    {
        return compute_legacy_code_home_dir();
    }

    #[cfg(not(test))]
    {
        static LEGACY: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
        LEGACY
            .get_or_init(compute_legacy_code_home_dir)
            .clone()
    }
}

fn path_exists(path: &Path) -> bool {
    std::fs::metadata(path).is_ok()
}

/// Resolve the filesystem path used for *reading* Codex state that may live in
/// a legacy `~/.codex` directory. Writes should continue targeting `code_home`.
pub fn resolve_code_path_for_read(code_home: &Path, relative: &Path) -> PathBuf {
    let default_path = code_home.join(relative);

    if env_overrides_present() {
        return default_path;
    }

    if path_exists(&default_path) {
        return default_path;
    }

    if let Some(default_home) = default_code_home_dir() {
        if default_home != code_home {
            return default_path;
        }
    }

    if let Some(legacy) = legacy_code_home_dir() {
        let candidate = legacy.join(relative);
        if path_exists(&candidate) {
            return candidate;
        }
    }

    default_path
}

/// Returns the path to the Code/Codex configuration directory, which can be
/// specified by the `CODE_HOME` or `CODEX_HOME` environment variables. If not set,
/// defaults to `~/.code` for the fork.
///
/// - If `CODE_HOME` or `CODEX_HOME` is set, the value will be canonicalized and this
///   function will Err if the path does not exist.
/// - If neither is set, this function does not verify that the directory exists.
pub fn find_code_home() -> std::io::Result<PathBuf> {
    if let Some(path) = env_path("CODE_HOME")? {
        return Ok(path);
    }

    if let Some(path) = env_path("CODEX_HOME")? {
        return Ok(path);
    }

    let home = home_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not find home directory",
        )
    })?;

    let mut write_path = home;
    write_path.push(".code");
    Ok(write_path)
}

/// Returns the path to the folder where Codex logs are stored. Does not verify
/// that the directory exists.
pub fn log_dir(cfg: &Config) -> std::io::Result<PathBuf> {
    let mut p = cfg.code_home.clone();
    p.push("log");
    Ok(p)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use crate::config_types::HistoryPersistence;
    use crate::config_types::Notifications;

    use super::*;
    use pretty_assertions::assert_eq;
    use std::ffi::OsString;
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
        assert_eq!("gpt-5.1", gpt5_profile_config.model);
        assert_eq!(OPENAI_DEFAULT_REVIEW_MODEL, gpt5_profile_config.review_model);
        assert_eq!(
            ReasoningEffort::High,
            gpt5_profile_config.review_model_reasoning_effort
        );
        assert_eq!(
            find_family_for_model("gpt-5.1").expect("known model slug"),
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
    fn upgrade_legacy_model_slugs_updates_top_level() {
        let mut cfg = ConfigToml {
            model: Some("gpt-5-codex".to_string()),
            review_model: Some("gpt-5".to_string()),
            ..Default::default()
        };

        upgrade_legacy_model_slugs(&mut cfg);

        assert_eq!(cfg.model.as_deref(), Some("gpt-5.1-codex"));
        assert_eq!(cfg.review_model.as_deref(), Some("gpt-5.1"));
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
            name: "code-gpt-5.1-codex".to_string(),
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

        assert!(enabled_names.contains("code-gpt-5.1-codex"));
        assert!(enabled_names.contains("claude-sonnet-4.5"));
        assert!(enabled_names.contains("gemini-2.5-pro"));
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
