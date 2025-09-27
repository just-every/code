use anyhow::anyhow;
use anyhow::Context;
use clap::CommandFactory;
use clap::Parser;
use clap_complete::Shell;
use clap_complete::generate;
use codex_arg0::arg0_dispatch_or_else;
use codex_chatgpt::apply_command::ApplyCommand;
use codex_chatgpt::apply_command::run_apply_command;
use codex_cli::LandlockCommand;
use codex_cli::SeatbeltCommand;
use codex_cli::login::run_login_status;
use codex_cli::login::run_login_with_api_key;
use codex_cli::login::run_login_with_chatgpt;
use codex_cli::login::run_logout;
use codex_cli::proto;
mod llm;
mod local_memory_cmd;
use llm::{LlmCli, run_llm};
use codex_common::CliConfigOverrides;
use codex_core::find_conversation_path_by_id_str;
use codex_core::RolloutRecorder;
use codex_exec::Cli as ExecCli;
use codex_tui::Cli as TuiCli;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use tokio::runtime::{Builder as TokioRuntimeBuilder, Handle as TokioHandle};

mod mcp_cmd;

use crate::local_memory_cmd::LocalMemoryCli;
use crate::mcp_cmd::McpCli;
use crate::proto::ProtoCli;

const CLI_COMMAND_NAME: &str = "code";

/// Codex CLI
///
/// If no subcommand is specified, options will be forwarded to the interactive CLI.
#[derive(Debug, Parser)]
#[clap(
    author,
    name = "code",
    version = codex_version::version(),
    // If a sub‑command is given, ignore requirements of the default args.
    subcommand_negates_reqs = true,
    // The executable is sometimes invoked via a platform‑specific name like
    // `codex-x86_64-unknown-linux-musl`, but the help output should always use
    // the generic `code` command name that users run.
    bin_name = "code"
)]
struct MultitoolCli {
    #[clap(flatten)]
    pub config_overrides: CliConfigOverrides,

    #[clap(flatten)]
    interactive: TuiCli,

    #[clap(subcommand)]
    subcommand: Option<Subcommand>,
}

#[derive(Debug, clap::Subcommand)]
enum Subcommand {
    /// Run Codex non-interactively.
    #[clap(visible_alias = "e")]
    Exec(ExecCli),

    /// Manage login.
    Login(LoginCommand),

    /// Remove stored authentication credentials.
    Logout(LogoutCommand),

    /// [experimental] Run Codex as an MCP server and manage MCP servers.
    #[clap(visible_alias = "acp")]
    Mcp(McpCli),

    /// Local-memory utilities.
    LocalMemory(LocalMemoryCli),

    /// Run the Protocol stream via stdin/stdout
    #[clap(visible_alias = "p")]
    Proto(ProtoCli),

    /// Generate shell completion scripts.
    Completion(CompletionCommand),

    /// Internal debugging commands.
    Debug(DebugArgs),

    /// Debug: replay ordering from response.json and codex-tui.log
    #[clap(hide = false)]
    OrderReplay(OrderReplayArgs),

    /// Apply the latest diff produced by Codex agent as a `git apply` to your local working tree.
    #[clap(visible_alias = "a")]
    Apply(ApplyCommand),

    /// Resume a previous interactive session (picker by default; use --last to continue the most recent).
    Resume(ResumeCommand),

    /// Internal: generate TypeScript protocol bindings.
    #[clap(hide = true)]
    GenerateTs(GenerateTsCommand),

    /// Diagnose PATH, binary collisions, and versions.
    Doctor,

    /// Download and run preview artifact by slug.
    Preview(PreviewArgs),

    /// Side-channel LLM utilities (no TUI events).
    Llm(LlmCli),
}

#[derive(Debug, Parser)]
struct CompletionCommand {
    /// Shell to generate completions for
    #[clap(value_enum, default_value_t = Shell::Bash)]
    shell: Shell,
}

#[derive(Debug, Parser)]
struct ResumeCommand {
    /// Conversation/session id (UUID). When provided, resumes this session.
    /// If omitted, use --last to pick the most recent recorded session.
    #[arg(value_name = "SESSION_ID")]
    session_id: Option<String>,

    /// Continue the most recent session without showing the picker.
    #[arg(long = "last", default_value_t = false, conflicts_with = "session_id")]
    last: bool,

    #[clap(flatten)]
    config_overrides: TuiCli,
}

#[derive(Debug, Parser)]
struct DebugArgs {
    #[command(subcommand)]
    cmd: DebugCommand,
}

#[derive(Debug, clap::Subcommand)]
enum DebugCommand {
    /// Run a command under Seatbelt (macOS only).
    Seatbelt(SeatbeltCommand),

    /// Run a command under Landlock+seccomp (Linux only).
    Landlock(LandlockCommand),
}

#[derive(Debug, Parser)]
struct LoginCommand {
    #[clap(skip)]
    config_overrides: CliConfigOverrides,

    #[arg(long = "api-key", value_name = "API_KEY")]
    api_key: Option<String>,

    #[command(subcommand)]
    action: Option<LoginSubcommand>,
}

#[derive(Debug, clap::Subcommand)]
enum LoginSubcommand {
    /// Show login status.
    Status,
}

#[derive(Debug, Parser)]
struct LogoutCommand {
    #[clap(skip)]
    config_overrides: CliConfigOverrides,
}

#[derive(Debug, Parser)]
struct GenerateTsCommand {
    /// Output directory where .ts files will be written
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    out_dir: PathBuf,

    /// Optional path to the Prettier executable to format generated files
    #[arg(short = 'p', long = "prettier", value_name = "PRETTIER_BIN")]
    prettier: Option<PathBuf>,
}

#[derive(Debug, Parser)]
struct OrderReplayArgs {
    /// Path to a response.json captured under ~/.code/debug_logs/*_response.json
    /// (legacy ~/.codex/debug_logs/ is still read).
    response_json: std::path::PathBuf,
    /// Path to codex-tui.log (typically ~/.code/log/codex-tui.log; legacy
    /// ~/.codex/log/codex-tui.log is still read).
    tui_log: std::path::PathBuf,
}

#[derive(Debug, Parser)]
struct PreviewArgs {
    /// Slug identifier (e.g., faster-downloads)
    slug: String,
    /// Optional owner/repo to override (defaults to just-every/code or $GITHUB_REPOSITORY)
    #[arg(long = "repo", value_name = "OWNER/REPO")]
    repo: Option<String>,
    /// Output directory where the binary will be extracted
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    out_dir: Option<PathBuf>,
    /// Additional args to pass to the downloaded binary
    #[arg(trailing_var_arg = true)]
    extra: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|codex_linux_sandbox_exe| async move {
        cli_main(codex_linux_sandbox_exe).await?;
        Ok(())
    })
}

async fn cli_main(codex_linux_sandbox_exe: Option<PathBuf>) -> anyhow::Result<()> {
    let MultitoolCli {
        config_overrides: root_config_overrides,
        mut interactive,
        subcommand,
    } = MultitoolCli::parse();

    interactive.finalize_defaults();

    match subcommand {
        None => {
            prepend_config_flags(
                &mut interactive.config_overrides,
                root_config_overrides.clone(),
            );
            let usage = codex_tui::run_main(interactive, codex_linux_sandbox_exe).await?;
            if !usage.is_zero() {
                println!("{}", codex_core::protocol::FinalOutput::from(usage));
            }
        }
        Some(Subcommand::Exec(mut exec_cli)) => {
            prepend_config_flags(
                &mut exec_cli.config_overrides,
                root_config_overrides.clone(),
            );
            codex_exec::run_main(exec_cli, codex_linux_sandbox_exe).await?;
        }
        Some(Subcommand::Mcp(mut mcp_cli)) => {
            // Propagate any root-level config overrides (e.g. `-c key=value`).
            prepend_config_flags(&mut mcp_cli.config_overrides, root_config_overrides.clone());
            mcp_cli.run(codex_linux_sandbox_exe).await?;
        }
        Some(Subcommand::LocalMemory(local_memory_cli)) => {
            local_memory_cli.run().await?;
        }
        Some(Subcommand::Resume(ResumeCommand {
            session_id,
            last,
            mut config_overrides,
        })) => {
            config_overrides.finalize_defaults();
            interactive = finalize_resume_interactive(
                interactive,
                root_config_overrides.clone(),
                session_id,
                last,
                config_overrides,
            );
            codex_tui::run_main(interactive, codex_linux_sandbox_exe).await?;
        }
        Some(Subcommand::Login(mut login_cli)) => {
            prepend_config_flags(
                &mut login_cli.config_overrides,
                root_config_overrides.clone(),
            );
            match login_cli.action {
                Some(LoginSubcommand::Status) => {
                    run_login_status(login_cli.config_overrides).await;
                }
                None => {
                    if let Some(api_key) = login_cli.api_key {
                        run_login_with_api_key(login_cli.config_overrides, api_key).await;
                    } else {
                        run_login_with_chatgpt(login_cli.config_overrides).await;
                    }
                }
            }
        }
        Some(Subcommand::Logout(mut logout_cli)) => {
            prepend_config_flags(
                &mut logout_cli.config_overrides,
                root_config_overrides.clone(),
            );
            run_logout(logout_cli.config_overrides).await;
        }
        Some(Subcommand::Proto(mut proto_cli)) => {
            prepend_config_flags(
                &mut proto_cli.config_overrides,
                root_config_overrides.clone(),
            );
            proto::run_main(proto_cli).await?;
        }
        Some(Subcommand::Completion(completion_cli)) => {
            print_completion(completion_cli);
        }
        Some(Subcommand::Debug(debug_args)) => match debug_args.cmd {
            DebugCommand::Seatbelt(mut seatbelt_cli) => {
                prepend_config_flags(
                    &mut seatbelt_cli.config_overrides,
                    root_config_overrides.clone(),
                );
                codex_cli::debug_sandbox::run_command_under_seatbelt(
                    seatbelt_cli,
                    codex_linux_sandbox_exe,
                )
                .await?;
            }
            DebugCommand::Landlock(mut landlock_cli) => {
                prepend_config_flags(
                    &mut landlock_cli.config_overrides,
                    root_config_overrides.clone(),
                );
                codex_cli::debug_sandbox::run_command_under_landlock(
                    landlock_cli,
                    codex_linux_sandbox_exe,
                )
                .await?;
            }
        },
        Some(Subcommand::Apply(mut apply_cli)) => {
            prepend_config_flags(
                &mut apply_cli.config_overrides,
                root_config_overrides.clone(),
            );
            run_apply_command(apply_cli, None).await?;
        }
        Some(Subcommand::GenerateTs(gen_cli)) => {
            codex_protocol_ts::generate_ts(&gen_cli.out_dir, gen_cli.prettier.as_deref())?;
        }
        Some(Subcommand::OrderReplay(args)) => {
            order_replay_main(args)?;
        }
        Some(Subcommand::Doctor) => {
            doctor_main().await?;
        }
        Some(Subcommand::Preview(args)) => {
            preview_main(args).await?;
        }
        Some(Subcommand::Llm(mut llm_cli)) => {
            prepend_config_flags(
                &mut llm_cli.config_overrides,
                root_config_overrides.clone(),
            );
            run_llm(llm_cli).await?;
        }
    }

    Ok(())
}

/// Prepend root-level overrides so they have lower precedence than
/// CLI-specific ones specified after the subcommand (if any).
fn prepend_config_flags(
    subcommand_config_overrides: &mut CliConfigOverrides,
    cli_config_overrides: CliConfigOverrides,
) {
    subcommand_config_overrides
        .raw_overrides
        .splice(0..0, cli_config_overrides.raw_overrides);
}

/// Build the final `TuiCli` for a `codex resume` invocation.
fn finalize_resume_interactive(
    mut interactive: TuiCli,
    root_config_overrides: CliConfigOverrides,
    session_id: Option<String>,
    last: bool,
    mut resume_cli: TuiCli,
) -> TuiCli {
    // Our fork does not expose explicit resume fields on the TUI CLI.
    // We simply merge resume-scoped flags and root overrides and run the TUI.

    interactive.finalize_defaults();
    resume_cli.finalize_defaults();

    // Merge resume-scoped flags and overrides with highest precedence.
    merge_resume_cli_flags(&mut interactive, resume_cli);

    if let Err(err) = apply_resume_directives(&mut interactive, session_id, last) {
        eprintln!("{}", err);
        process::exit(1);
    }

    // Propagate any root-level config overrides (e.g. `-c key=value`).
    prepend_config_flags(&mut interactive.config_overrides, root_config_overrides);

    interactive
}

/// Merge flags provided to `codex resume` so they take precedence over any
/// root-level flags. Only overrides fields explicitly set on the resume-scoped
/// CLI. Also appends `-c key=value` overrides with highest precedence.
fn merge_resume_cli_flags(interactive: &mut TuiCli, resume_cli: TuiCli) {
    if let Some(model) = resume_cli.model {
        interactive.model = Some(model);
    }
    if resume_cli.oss {
        interactive.oss = true;
    }
    if let Some(profile) = resume_cli.config_profile {
        interactive.config_profile = Some(profile);
    }
    if let Some(sandbox) = resume_cli.sandbox_mode {
        interactive.sandbox_mode = Some(sandbox);
    }
    if let Some(approval) = resume_cli.approval_policy {
        interactive.approval_policy = Some(approval);
    }
    if resume_cli.full_auto {
        interactive.full_auto = true;
    }
    if resume_cli.dangerously_bypass_approvals_and_sandbox {
        interactive.dangerously_bypass_approvals_and_sandbox = true;
    }
    if let Some(cwd) = resume_cli.cwd {
        interactive.cwd = Some(cwd);
    }
    if !resume_cli.images.is_empty() {
        interactive.images = resume_cli.images;
    }
    if let Some(prompt) = resume_cli.prompt {
        interactive.prompt = Some(prompt);
    }

    if resume_cli.enable_web_search || resume_cli.disable_web_search {
        interactive.enable_web_search = resume_cli.enable_web_search;
        interactive.disable_web_search = resume_cli.disable_web_search;
        interactive.web_search = resume_cli.web_search;
    }

    interactive
        .config_overrides
        .raw_overrides
        .extend(resume_cli.config_overrides.raw_overrides);
}

fn apply_resume_directives(
    interactive: &mut TuiCli,
    session_id: Option<String>,
    last: bool,
) -> anyhow::Result<()> {
    interactive.resume_picker = false;
    interactive.resume_last = false;
    interactive.resume_session_id = None;

    match (session_id, last) {
        (Some(id), _) => {
            match resolve_resume_path(Some(id.as_str()), false)? {
                Some(path) => {
                    interactive.resume_session_id = Some(id);
                    push_experimental_resume_override(interactive, &path);
                }
                None => {
                    eprintln!("No recorded session found with id {id}");
                    interactive.resume_session_id = Some(id);
                }
            }
        }
        (None, true) => {
            match resolve_resume_path(None, true)? {
                Some(path) => {
                    interactive.resume_last = true;
                    push_experimental_resume_override(interactive, &path);
                }
                None => {
                    eprintln!(
                        "No recent sessions found to resume. Start a session with `code` first."
                    );
                    interactive.resume_picker = true;
                }
            }
        }
        (None, false) => {
            interactive.resume_picker = true;
        }
    }

    Ok(())
}

fn resolve_resume_path(session_id: Option<&str>, last: bool) -> anyhow::Result<Option<PathBuf>> {
    if session_id.is_none() && !last {
        return Ok(None);
    }

    let codex_home = codex_core::config::find_codex_home()
        .context("failed to locate Codex home directory")?;

    // Build the async work once, then execute it either on the existing
    // runtime (from a helper thread) or a fresh current-thread runtime.
    // Clone borrowed inputs so the async task can be 'static when spawned.
    let sess = session_id.map(|s| s.to_string());
    let fetch = async move {
        if let Some(id) = sess.as_deref() {
            let maybe = find_conversation_path_by_id_str(&codex_home, id)
                .await
                .context("failed to look up session by id")?;
            Ok(maybe)
        } else if last {
            let page = RolloutRecorder::list_conversations(&codex_home, 1, None)
                .await
                .context("failed to list recorded sessions")?;
            Ok(page.items.first().map(|it| it.path.clone()))
        } else {
            Ok(None)
        }
    };

    match TokioHandle::try_current() {
        Ok(handle) => {
            let handle = handle.clone();
            std::thread::spawn(move || handle.block_on(fetch))
                .join()
                .map_err(|_| anyhow!("resume lookup thread panicked"))?
        }
        Err(_) => TokioRuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .context("failed to create async runtime for resume lookup")?
            .block_on(fetch),
    }
}

fn push_experimental_resume_override(interactive: &mut TuiCli, path: &Path) {
    let raw = path.to_string_lossy();
    let escaped = raw.replace('\\', "\\\\").replace('"', "\\\"");
    interactive
        .config_overrides
        .raw_overrides
        .push(format!("experimental_resume=\"{escaped}\""));
}

fn write_completion<W: std::io::Write>(shell: Shell, out: &mut W) {
    let mut app = MultitoolCli::command();
    generate(shell, &mut app, CLI_COMMAND_NAME, out);
}

fn print_completion(cmd: CompletionCommand) {
    write_completion(cmd.shell, &mut std::io::stdout());
}

fn order_replay_main(args: OrderReplayArgs) -> anyhow::Result<()> {
    use anyhow::{Context, Result};
    use regex::Regex;
    use serde_json::Value;
    use std::fs;

    fn parse_response_expected(path: &std::path::Path) -> Result<Vec<(u64, u64)>> {
        let data = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let v: Value = serde_json::from_str(&data)?;
        let events = v.get("events").and_then(|e| e.as_array()).cloned().unwrap_or_default();
        let mut items: Vec<(u64, u64)> = Vec::new();
        for ev in events {
            let data = ev.get("data");
            if let Some(d) = data {
                let out = d.get("output_index").and_then(|x| x.as_u64());
                let seq = d.get("sequence_number").and_then(|x| x.as_u64());
                if let (Some(out), Some(seq)) = (out, seq) {
                    items.push((out, seq));
                }
            }
        }
        items.sort();
        Ok(items)
    }

    #[derive(Debug)]
    struct InsertLog { ordered: bool, req: u64, out: u64, item_seq: u64, raw: u64 }

    fn parse_tui_inserts(path: &std::path::Path) -> Result<Vec<InsertLog>> {
        let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let re = Regex::new(r"insert window: seq=(?P<seq>\d+) \((?P<kind>[OU]):(?:req=(?P<req>\d+) out=(?P<out>\d+) seq=(?P<iseq>\d+)|(?P<uval>\d+))\)").unwrap();
        let mut out = Vec::new();
        for line in text.lines() {
            if let Some(caps) = re.captures(line) {
                let seq: u64 = caps.name("seq").unwrap().as_str().parse().unwrap_or(0);
                let ordered = &caps["kind"] == "O";
                let (req, out_idx, item_seq) = if ordered {
                    let req = caps.name("req").unwrap().as_str().parse().unwrap_or(0);
                    let out_idx = caps.name("out").unwrap().as_str().parse().unwrap_or(0);
                    let iseq = caps.name("iseq").unwrap().as_str().parse().unwrap_or(0);
                    (req, out_idx, iseq)
                } else {
                    (0, 0, caps.name("uval").unwrap().as_str().parse().unwrap_or(0))
                };
                out.push(InsertLog { ordered, req, out: out_idx, item_seq, raw: seq });
            }
        }
        Ok(out)
    }

    let expected = parse_response_expected(&args.response_json)?;
    let actual = parse_tui_inserts(&args.tui_log)?;

    println!("Expected (first 20 sorted by out,seq):");
    for (i, (out, seq)) in expected.iter().take(20).enumerate() {
        println!("  {:>3}: out={} seq={}", i, out, seq);
    }

    println!("\nActual inserts (first 40):");
    for (i, log) in actual.iter().take(40).enumerate() {
        if log.ordered {
            println!("  {:>3}: O:req={} out={} seq={} (raw={})", i, log.req, log.out, log.item_seq, log.raw);
        } else {
            println!("  {:>3}: U:{}", i, log.item_seq);
        }
    }

    // Simple check: assistant (out=1) should appear before tool (out=2) within same req
    let pos_out1 = actual.iter().position(|l| l.ordered && l.req == 1 && l.out == 1);
    let pos_out2 = actual.iter().position(|l| l.ordered && l.req == 1 && l.out == 2);
    println!("\nCheck (req=1): first out=1 at {:?}, first out=2 at {:?}", pos_out1, pos_out2);
    if let (Some(p1), Some(p2)) = (pos_out1, pos_out2) {
        if p1 < p2 { println!("Result: OK (assistant precedes tool)"); } else { println!("Result: WRONG (tool precedes assistant)"); }
    }

    Ok(())
}

async fn preview_main(args: PreviewArgs) -> anyhow::Result<()> {
    use anyhow::{bail, Context};
    use flate2::read::GzDecoder;
    use std::env;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;
    use zip::ZipArchive;

    let repo = args
        .repo
        .or_else(|| env::var("GITHUB_REPOSITORY").ok())
        .unwrap_or_else(|| "just-every/code".to_string());
    let (owner, name) = repo
        .split_once('/')
        .map(|(o, n)| (o.to_string(), n.to_string()))
        .ok_or_else(|| anyhow::anyhow!(format!("Invalid repo format: {}", repo)))?;

    let os = env::consts::OS;
    let arch = env::consts::ARCH;
    let target = match (os, arch) {
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        ("linux", "aarch64") => "aarch64-unknown-linux-musl",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("windows", _) => "x86_64-pc-windows-msvc",
        _ => bail!(format!("Unsupported platform: {}/{}", os, arch)),
    };

    let client = reqwest::Client::builder().user_agent("codex-preview/1").build()?;

    // Resolve slug/tag from id
    let id = args.slug.trim().to_string();
    async fn fetch_json(client: &reqwest::Client, url: &str) -> anyhow::Result<serde_json::Value> {
        let r = client.get(url).send().await?;
        let s = r.status();
        let t = r.text().await?;
        if !s.is_success() { anyhow::bail!(format!("GET {} -> {} {}", url, s.as_u16(), t)); }
        Ok(serde_json::from_str(&t).unwrap_or(serde_json::Value::Null))
    }
    async fn latest_tag_for_slug(client: &reqwest::Client, owner: &str, name: &str, slug: &str) -> anyhow::Result<String> {
        let base = format!("preview-{}", slug);
        let url = format!("https://api.github.com/repos/{owner}/{name}/releases?per_page=100");
        let v = fetch_json(client, &url).await?;
        let mut latest = base.clone();
        let mut max_n: u64 = 0;
        if let Some(arr) = v.as_array() {
            let re = regex::Regex::new(&format!(r"^{}-(\\d+)$", regex::escape(&base))).unwrap();
            for it in arr {
                if let Some(tag) = it.get("tag_name").and_then(|x| x.as_str()) {
                    if tag == base { if max_n < 1 { max_n = 1; latest = base.clone(); } }
                    else if let Some(c) = re.captures(tag) {
                        let n: u64 = c.get(1).unwrap().as_str().parse().unwrap_or(0);
                        if n > max_n { max_n = n; latest = tag.to_string(); }
                    }
                }
            }
        }
        Ok(latest)
    }
    let slug = id.to_lowercase();
    let tag = latest_tag_for_slug(&client, &owner, &name, &slug).await?;
    let (slug, tag) = (slug, tag);
    let base = format!("https://github.com/{owner}/{name}/releases/download/{tag}");

    // Try to download the best asset for this platform; prefer .tar.gz on Unix and .zip on Windows; fallback to .zst.
    let mut urls: Vec<String> = vec![];
    if cfg!(windows) {
        urls.push(format!("{base}/code-x86_64-pc-windows-msvc.exe.zip"));
    } else {
        // tar.gz first, then zst
        urls.push(format!("{base}/code-{target}.tar.gz"));
        urls.push(format!("{base}/code-{target}.zst"));
    }

    let tmp = tempdir()?;
    let mut downloaded: Option<(std::path::PathBuf, String)> = None;
    for u in urls.iter() {
        let resp = client.get(u).send().await?;
        if resp.status().is_success() {
            let data = resp.bytes().await?;
            let filename = u.split('/').last().unwrap_or("download.bin");
            let p = tmp.path().join(filename);
            fs::write(&p, &data)?;
            downloaded = Some((p, u.clone()));
            break;
        }
    }
    let (path, url_used) = downloaded.context("No matching preview asset found on the prerelease. It may still be uploading; try again shortly.")?;

    // Find the easiest payload
    fn first_match(dir: &Path, pat: &str) -> Option<std::path::PathBuf> {
        for entry in fs::read_dir(dir).ok()? {
            let p = entry.ok()?.path();
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                if name.starts_with(pat) { return Some(p); }
            }
        }
        None
    }

    // Determine output directory
    // Default: ~/.code/bin
    let out_dir = if let Some(dir) = args.out_dir {
        dir
    } else {
        let home = if cfg!(windows) {
            env::var_os("USERPROFILE")
        } else {
            env::var_os("HOME")
        };
        let base = home
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join(".code").join("bin")
    };
    let _ = fs::create_dir_all(&out_dir);

    #[cfg(target_family = "unix")]
    fn make_exec(p: &Path) { use std::os::unix::fs::PermissionsExt; let _ = fs::set_permissions(p, fs::Permissions::from_mode(0o755)); }
    #[cfg(target_family = "windows")]
    fn make_exec(_p: &Path) { }

    if os != "windows" {
        // If we downloaded a tar.gz, extract
        if path.extension().and_then(|e| e.to_str()) == Some("gz") {
            let tgz = path.clone();
            let file = fs::File::open(&tgz)?;
            let gz = GzDecoder::new(file);
            let mut ar = tar::Archive::new(gz);
            ar.unpack(&out_dir)?;
            // Find extracted binary
            let bin = first_match(&out_dir, "code-").unwrap_or(out_dir.join("code"));
            let dest_name = format!("{}-{}", bin.file_name().and_then(|s| s.to_str()).unwrap_or("code"), slug);
            let dest = out_dir.join(dest_name);
            // Rename/move to include PR number suffix
            let _ = fs::rename(&bin, &dest).or_else(|_| { fs::copy(&bin, &dest).map(|_| () ) });
            make_exec(&dest);
            println!("Downloaded preview to {}", dest.display());
            if !args.extra.is_empty() { let _ = std::process::Command::new(&dest).args(&args.extra).status(); } else { let _ = std::process::Command::new(&dest).status(); }
            return Ok(());
        }
    } else {
        // Windows: expand zip
        if path.extension().and_then(|e| e.to_str()) == Some("zip") {
            let f = fs::File::open(&path)?;
            let mut z = ZipArchive::new(f)?;
            z.extract(&out_dir)?;
            let exe = first_match(&out_dir, "code-").unwrap_or(out_dir.join("code.exe"));
            // Append slug before extension if present
            let dest = match exe.extension().and_then(|e| e.to_str()) {
                Some(ext) => {
                    let stem = exe.file_stem().and_then(|s| s.to_str()).unwrap_or("code");
                    out_dir.join(format!("{}-{}.{}", stem, slug, ext))
                }
                None => out_dir.join(format!("{}-{}", exe.file_name().and_then(|s| s.to_str()).unwrap_or("code"), slug)),
            };
            let _ = fs::rename(&exe, &dest).or_else(|_| { fs::copy(&exe, &dest).map(|_| () ) });
            println!("Downloaded preview to {}", dest.display());
            if !args.extra.is_empty() { let _ = std::process::Command::new(&dest).args(&args.extra).spawn(); } else { let _ = std::process::Command::new(&dest).spawn(); }
            return Ok(());
        }
    }

    // Fallback: raw 'code' file (after .zst) if present
    if path.file_name().and_then(|s| s.to_str()).map(|n| n.ends_with(".zst")).unwrap_or(false) {
        // Try to decompress .zst to 'code'
        if which::which("zstd").is_ok() {
            // Derive base name from archive (e.g., code-aarch64-apple-darwin.zst -> code-aarch64-apple-darwin-<slug>.{exe?})
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("code");
            let dest = if cfg!(windows) { out_dir.join(format!("{}-{}.exe", stem, slug)) } else { out_dir.join(format!("{}-{}", stem, slug)) };
            let status = std::process::Command::new("zstd").arg("-d").arg(&path).arg("-o").arg(&dest).status()?;
            if status.success() {
                make_exec(&dest);
                println!("Downloaded preview from {} to {}", url_used, dest.display());
                if !args.extra.is_empty() { let _ = std::process::Command::new(&dest).args(&args.extra).status(); } else { let _ = std::process::Command::new(&dest).status(); }
                return Ok(());
            }
        }
        // If zstd missing, tell the user
        bail!("Downloaded .zst but 'zstd' is not installed. Install zstd or download the .tar.gz/.zip asset instead.");
    } else if let Some(bin) = first_match(tmp.path(), "code") {
        let dest = out_dir.join(bin.file_name().unwrap_or_default());
        fs::copy(&bin, &dest)?;
        make_exec(&dest);
        println!("Downloaded preview to {}", dest.display());
        if !args.extra.is_empty() { let _ = std::process::Command::new(&dest).args(&args.extra).status(); } else { let _ = std::process::Command::new(&dest).status(); }
        return Ok(());
    }

    bail!("No recognized artifact content found.")
}

async fn doctor_main() -> anyhow::Result<()> {
    use std::env;
    use std::process::Stdio;
    use tokio::process::Command;

    // Print current executable and version
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "<unknown>".to_string());
    println!("code version: {}", codex_version::version());
    println!("current_exe: {}", exe);

    // PATH
    let path = env::var("PATH").unwrap_or_default();
    println!("PATH: {}", path);

    // Helper to run a shell command and capture stdout (best-effort)
    async fn run_cmd(cmd: &str, args: &[&str]) -> String {
        let mut c = Command::new(cmd);
        c.args(args).stdin(Stdio::null()).stderr(Stdio::null());
        match c.output().await {
            Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_string(),
            Err(_) => String::new(),
        }
    }

    #[cfg(target_family = "unix")]
    let which_all = |name: &str| {
        let name = name.to_string();
        async move {
            let out = run_cmd("/bin/bash", &["-lc", &format!("which -a {} 2>/dev/null || true", name)]).await;
            out.split('\n').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect::<Vec<_>>()
        }
    };
    #[cfg(target_family = "windows")]
    let which_all = |name: &str| {
        let name = name.to_string();
        async move {
            let out = run_cmd("where", &[&name]).await;
            out.split('\n').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect::<Vec<_>>()
        }
    };

    // Gather candidates for code/coder
    let code_paths = which_all("code").await;
    let coder_paths = which_all("coder").await;

    println!("\nFound 'code' on PATH (in order):");
    if code_paths.is_empty() {
        println!("  <none>");
    } else {
        for p in &code_paths { println!("  {}", p); }
    }
    println!("\nFound 'coder' on PATH (in order):");
    if coder_paths.is_empty() {
        println!("  <none>");
    } else {
        for p in &coder_paths { println!("  {}", p); }
    }

    // Try to run --version for each resolved binary to show where mismatches come from
    async fn show_versions(caption: &str, paths: &[String]) {
        println!("\n{}:", caption);
        for p in paths {
            let out = run_cmd(p, &["--version"]).await;
            if out.is_empty() {
                println!("  {} -> (no output)", p);
            } else {
                println!("  {} -> {}", p, out);
            }
        }
    }
    show_versions("code --version by path", &code_paths).await;
    show_versions("coder --version by path", &coder_paths).await;

    // Detect Bun shims
    let bun_home = env::var("BUN_INSTALL").ok().or_else(|| {
        env::var("HOME").ok().map(|h| format!("{}/.bun", h))
    });
    if let Some(bun) = bun_home {
        let bun_bin = format!("{}/bin", bun);
        let bun_coder = format!("{}/coder", bun_bin);
        if coder_paths.iter().any(|p| p == &bun_coder) {
            println!("\nBun shim detected for 'coder': {}", bun_coder);
            println!("Suggestion: remove old Bun global with: bun remove -g @just-every/code");
        }
        let bun_code = format!("{}/code", bun_bin);
        if code_paths.iter().any(|p| p == &bun_code) {
            println!("Bun shim detected for 'code': {}", bun_code);
            println!("Suggestion: prefer 'coder' or remove Bun shim if it conflicts.");
        }
    }

    // Detect Homebrew overshadow of VS Code
    #[cfg(target_os = "macos")]
    {
        let brew_code = code_paths.iter().find(|p| p.contains("/homebrew/bin/code") || p.contains("/Cellar/code/"));
        let vscode_code = code_paths.iter().find(|p| p.contains("/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code"));
        if brew_code.is_some() && vscode_code.is_some() {
            println!("\nHomebrew 'code' precedes VS Code CLI in PATH.");
            println!("Suggestion: uninstall Homebrew formula 'code' (brew uninstall code) or reorder PATH so /usr/local/bin comes before /usr/local/homebrew/bin.");
        }
    }

    // npm global hints
    let npm_root = run_cmd("npm", &["root", "-g"]).await;
    let npm_prefix = run_cmd("npm", &["prefix", "-g"]).await;
    if !npm_root.is_empty() {
        println!("\nnpm root -g: {}", npm_root);
    }
    if !npm_prefix.is_empty() {
        println!("npm prefix -g: {}", npm_prefix);
    }

    println!("\nIf versions differ, remove older installs and keep one package manager:");
    println!("  - Bun: bun remove -g @just-every/code");
    println!("  - npm/pnpm: npm uninstall -g @just-every/code");
    println!("  - Homebrew: brew uninstall code");
    println!("  - Prefer using 'coder' to avoid conflicts with VS Code's 'code'.");

    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bash_completion_uses_code_command_name() {
        let mut buf = Vec::new();
        write_completion(Shell::Bash, &mut buf);
        let script = String::from_utf8(buf).expect("completion output should be valid UTF-8");
        assert!(script.contains("_code()"), "expected bash completion function to be named _code");
        assert!(!script.contains("_codex()"), "bash completion output should not use legacy codex prefix");
    }

    fn finalize_from_args(args: &[&str]) -> TuiCli {
        let cli = MultitoolCli::try_parse_from(args).expect("parse");
        let MultitoolCli {
            interactive,
            config_overrides: root_overrides,
            subcommand,
        } = cli;

        let Subcommand::Resume(ResumeCommand {
            session_id,
            last,
            config_overrides: resume_cli,
        }) = subcommand.expect("resume present")
        else {
            unreachable!()
        };

        finalize_resume_interactive(interactive, root_overrides, session_id, last, resume_cli)
    }

    #[test]
    fn resume_model_flag_applies_when_no_root_flags() {
        let interactive = finalize_from_args(["codex", "resume", "-m", "gpt-5-test"].as_ref());

        assert_eq!(interactive.model.as_deref(), Some("gpt-5-test"));
        assert!(interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
    }

    #[test]
    fn resume_picker_logic_none_and_not_last() {
        let interactive = finalize_from_args(["codex", "resume"].as_ref());
        assert!(interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
    }

    #[test]
    fn resume_picker_logic_last() {
        let interactive = finalize_from_args(["codex", "resume", "--last"].as_ref());
        assert!(!interactive.resume_picker);
        assert!(interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
    }

    #[test]
    fn resume_picker_logic_with_session_id() {
        let interactive = finalize_from_args(["codex", "resume", "1234"].as_ref());
        assert!(!interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id.as_deref(), Some("1234"));
    }

    #[test]
    fn resume_merges_option_flags_and_full_auto() {
        let interactive = finalize_from_args(
            [
                "codex",
                "resume",
                "sid",
                "--oss",
                "--full-auto",
                "--search",
                "--sandbox",
                "workspace-write",
                "--ask-for-approval",
                "on-request",
                "-m",
                "gpt-5-test",
                "-p",
                "my-profile",
                "-C",
                "/tmp",
                "-i",
                "/tmp/a.png,/tmp/b.png",
            ]
            .as_ref(),
        );

        assert_eq!(interactive.model.as_deref(), Some("gpt-5-test"));
        assert!(interactive.oss);
        assert_eq!(interactive.config_profile.as_deref(), Some("my-profile"));
        assert!(matches!(
            interactive.sandbox_mode,
            Some(codex_common::SandboxModeCliArg::WorkspaceWrite)
        ));
        assert!(matches!(
            interactive.approval_policy,
            Some(codex_common::ApprovalModeCliArg::OnRequest)
        ));
        assert!(interactive.full_auto);
        assert_eq!(
            interactive.cwd.as_deref(),
            Some(std::path::Path::new("/tmp"))
        );
        assert!(interactive.web_search);
        let has_a = interactive
            .images
            .iter()
            .any(|p| p == std::path::Path::new("/tmp/a.png"));
        let has_b = interactive
            .images
            .iter()
            .any(|p| p == std::path::Path::new("/tmp/b.png"));
        assert!(has_a && has_b);
        assert!(!interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id.as_deref(), Some("sid"));
    }

    #[test]
    fn resume_merges_dangerously_bypass_flag() {
        let interactive = finalize_from_args(
            [
                "codex",
                "resume",
                "--dangerously-bypass-approvals-and-sandbox",
            ]
            .as_ref(),
        );
        assert!(interactive.dangerously_bypass_approvals_and_sandbox);
        assert!(interactive.resume_picker);
        assert!(!interactive.resume_last);
        assert_eq!(interactive.resume_session_id, None);
    }
}
