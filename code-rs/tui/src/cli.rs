use clap::ArgAction;
use clap::Parser;
use code_common::ApprovalModeCliArg;
use code_common::CliConfigOverrides;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version)]
pub struct Cli {
    /// Optional user prompt to start the session.
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,

    /// Optional image(s) to attach to the initial prompt.
    #[arg(
        long = "image",
        short = 'i',
        value_name = "FILE",
        value_delimiter = ','
    )]
    pub images: Vec<PathBuf>,

    /// Model the agent should use.
    #[arg(long, short = 'm')]
    pub model: Option<String>,

    /// Convenience flag to select the local open source model provider.
    /// Equivalent to -c model_provider=oss; verifies a local Ollama server is
    /// running.
    #[arg(long = "oss", default_value_t = false)]
    pub oss: bool,

    /// Configuration profile from config.toml to specify default options.
    #[arg(long = "profile", short = 'p')]
    pub config_profile: Option<String>,

    /// Select the sandbox policy to use when executing model-generated shell
    /// commands.
    #[arg(long = "sandbox", short = 's')]
    pub sandbox_mode: Option<code_common::SandboxModeCliArg>,

    /// Configure when the model requires human approval before executing a command.
    #[arg(long = "ask-for-approval", short = 'a')]
    pub approval_policy: Option<ApprovalModeCliArg>,

    /// Convenience alias for low-friction sandboxed automatic execution (-a on-failure, --sandbox workspace-write).
    #[arg(long = "full-auto", default_value_t = false)]
    pub full_auto: bool,

    /// Skip all confirmation prompts and execute commands without sandboxing.
    /// EXTREMELY DANGEROUS. Intended solely for running in environments that are externally sandboxed.
    #[arg(
        long = "dangerously-bypass-approvals-and-sandbox",
        alias = "yolo",
        default_value_t = false,
        conflicts_with_all = ["approval_policy", "full_auto"]
    )]
    pub dangerously_bypass_approvals_and_sandbox: bool,

    /// Tell the agent to use the specified directory as its working root.
    #[clap(long = "cd", short = 'C', value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// Enable web search support. Enabled by default; use --no-search to disable.
    #[arg(long = "search", action = ArgAction::SetTrue)]
    pub enable_web_search: bool,

    /// Disable web search support explicitly.
    #[arg(long = "no-search", action = ArgAction::SetTrue, hide = true)]
    pub disable_web_search: bool,

    /// Effective web search toggle after applying flags.
    #[clap(skip)]
    pub web_search: bool,

    /// Enable debug logging of all LLM requests and responses to files.
    #[clap(long = "debug", short = 'd', default_value_t = false)]
    pub debug: bool,

    /// Override the compaction prompt text for this session.
    #[arg(long = "compact-prompt", value_name = "TEXT")]
    pub compact_prompt_override: Option<String>,

    /// Read the compaction prompt text from a file.
    #[arg(long = "compact-prompt-file", value_name = "FILE")]
    pub compact_prompt_file: Option<PathBuf>,

    /// Show per-cell ordering overlays (request index, order key, window/position) to debug
    /// event ordering. Off by default.
    #[arg(long = "order", default_value_t = false)]
    pub order: bool,

    /// Enable lightweight in-app timing and print a summary report on exit.
    /// This records render/measurement hotspots while the UI runs and writes a
    /// short report to stderr when the program exits.
    #[arg(long = "timing", default_value_t = false)]
    pub timing: bool,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// Optional developer-role message to prepend to every turn for demos.
    /// Set by the multitool root CLI via `--demo`.
    #[clap(skip)]
    pub demo_developer_message: Option<String>,

    /// Start in resume picker mode when true (used by `code resume`).
    #[clap(skip)]
    pub resume_picker: bool,

    /// Resume the most recent session automatically when true.
    #[clap(skip)]
    pub resume_last: bool,

    /// Resume a specific session id when provided.
    #[clap(skip)]
    pub resume_session_id: Option<String>,
}

impl Cli {
    pub fn finalize_defaults(&mut self) {
        self.web_search = if self.disable_web_search {
            false
        } else if self.enable_web_search {
            true
        } else {
            true
        };
    }
}
