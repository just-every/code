use clap::Parser;
use clap::ValueEnum;
use code_common::CliConfigOverrides;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(version)]
pub struct Cli {
    /// Action to perform. If omitted, runs a new non-interactive session.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Run Auto Drive instead of a single non-interactive turn.
    #[arg(long = "auto", default_value_t = false)]
    pub auto_drive: bool,

    /// Use Auto Review models and limits for /review runs.
    #[arg(long = "auto-review", default_value_t = false)]
    pub auto_review: bool,

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

    #[arg(long = "oss", default_value_t = false)]
    pub oss: bool,

    /// Select the sandbox policy to use when executing model-generated shell
    /// commands.
    #[arg(long = "sandbox", short = 's', value_enum)]
    pub sandbox_mode: Option<code_common::SandboxModeCliArg>,

    /// Configuration profile from config.toml to specify default options.
    #[arg(long = "profile", short = 'p')]
    pub config_profile: Option<String>,

    /// Convenience alias for low-friction sandboxed automatic execution (-a on-failure, --sandbox workspace-write).
    #[arg(long = "full-auto", default_value_t = false)]
    pub full_auto: bool,

    /// Skip all confirmation prompts and execute commands without sandboxing.
    /// EXTREMELY DANGEROUS. Intended solely for running in environments that are externally sandboxed.
    #[arg(
        long = "dangerously-bypass-approvals-and-sandbox",
        alias = "yolo",
        default_value_t = false,
        conflicts_with = "full_auto"
    )]
    pub dangerously_bypass_approvals_and_sandbox: bool,

    /// Tell the agent to use the specified directory as its working root.
    #[clap(long = "cd", short = 'C', value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// Enable debug logging of all LLM requests and responses to files.
    #[clap(long = "debug", short = 'd', default_value_t = false)]
    pub debug: bool,

    /// Allow running Codex outside a Git repository.
    #[arg(long = "skip-git-repo-check", default_value_t = false)]
    pub skip_git_repo_check: bool,

    /// Path to a JSON Schema file describing the model's final response shape.
    #[arg(long = "output-schema", value_name = "FILE")]
    pub output_schema: Option<PathBuf>,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    /// Optional developer-role message to prepend to every turn for demos.
    /// Set by the multitool root CLI via `--demo`.
    #[clap(skip)]
    pub demo_developer_message: Option<String>,

    /// Specifies color settings for use in the output.
    #[arg(long = "color", value_enum, default_value_t = Color::Auto)]
    pub color: Color,

    /// Print events to stdout as JSONL.
    #[arg(long = "json", default_value_t = false)]
    pub json: bool,

    /// Maximum wall-clock time budget (seconds) before aborting the run.
    ///
    /// When this budget is at least 50% consumed, the coordinator receives
    /// increasingly frequent reminders to converge and finish.
    #[arg(
        long = "max-seconds",
        value_name = "SECONDS",
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub max_seconds: Option<u64>,

    /// Maximum number of Auto Drive coordinator turns before stopping (0 = unlimited).
    /// Only meaningful when --auto is enabled.
    #[arg(long = "turn-cap", value_name = "TURNS")]
    pub turn_cap: Option<u32>,

    /// Whether to include the plan tool in the conversation.
    #[arg(long = "include-plan-tool", default_value_t = false)]
    pub include_plan_tool: bool,

    /// Specifies file where the last message from the agent should be written.
    #[arg(long = "output-last-message")]
    pub last_message_file: Option<PathBuf>,

    /// When running /review, write the structured review output JSON to this file.
    /// Includes every review pass (e.g., auto-resolve loops) so findings are not
    /// lost when later passes report "no issues".
    #[arg(long = "review-output-json", value_name = "FILE")]
    pub review_output_json: Option<PathBuf>,

    /// Initial instructions for the agent. If not provided as an argument (or
    /// if `-` is used), instructions are read from stdin.
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Resume a previous session by id or pick the most recent with --last.
    Resume(ResumeArgs),
}

#[derive(Parser, Debug)]
pub struct ResumeArgs {
    /// Conversation/session id (UUID). When provided, resumes this session.
    /// If omitted, use --last to pick the most recent recorded session.
    #[arg(value_name = "SESSION_ID")]
    pub session_id: Option<String>,

    /// Resume the most recent recorded session (newest) without specifying an id.
    #[arg(long = "last", default_value_t = false, conflicts_with = "session_id")]
    pub last: bool,

    /// Prompt to send after resuming the session. If `-` is used, read from stdin.
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum Color {
    Always,
    Never,
    #[default]
    Auto,
}
