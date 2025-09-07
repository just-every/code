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
use codex_common::CliConfigOverrides;
use codex_exec::Cli as ExecCli;
use codex_tui::Cli as TuiCli;
use std::path::PathBuf;

use crate::proto::ProtoCli;

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
    // the generic `codex` command name that users run.
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

    /// Experimental: run Codex as an MCP server.
    #[cfg(feature = "mcp")]
    Mcp,

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

    /// Internal: generate TypeScript protocol bindings.
    #[clap(hide = true)]
    GenerateTs(GenerateTsCommand),

    /// Diagnose PATH, binary collisions, and versions.
    Doctor,
}

#[derive(Debug, Parser)]
struct CompletionCommand {
    /// Shell to generate completions for
    #[clap(value_enum, default_value_t = Shell::Bash)]
    shell: Shell,
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
    /// Path to a response.json captured under ~/.codex/debug_logs/*_response.json
    response_json: std::path::PathBuf,
    /// Path to codex-tui.log (typically ~/.codex/log/codex-tui.log)
    tui_log: std::path::PathBuf,
}

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|codex_linux_sandbox_exe| async move {
        cli_main(codex_linux_sandbox_exe).await?;
        Ok(())
    })
}

async fn cli_main(codex_linux_sandbox_exe: Option<PathBuf>) -> anyhow::Result<()> {
    let cli = MultitoolCli::parse();

    match cli.subcommand {
        None => {
            let mut tui_cli = cli.interactive;
            prepend_config_flags(&mut tui_cli.config_overrides, cli.config_overrides);
            let usage = codex_tui::run_main(tui_cli, codex_linux_sandbox_exe).await?;
            if !usage.is_zero() {
                println!("{}", codex_core::protocol::FinalOutput::from(usage));
            }
        }
        Some(Subcommand::Exec(mut exec_cli)) => {
            prepend_config_flags(&mut exec_cli.config_overrides, cli.config_overrides);
            codex_exec::run_main(exec_cli, codex_linux_sandbox_exe).await?;
        }
        #[cfg(feature = "mcp")]
        Some(Subcommand::Mcp) => {
            codex_mcp_server::run_main(codex_linux_sandbox_exe, cli.config_overrides).await?;
        }
        Some(Subcommand::Login(mut login_cli)) => {
            prepend_config_flags(&mut login_cli.config_overrides, cli.config_overrides);
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
            prepend_config_flags(&mut logout_cli.config_overrides, cli.config_overrides);
            run_logout(logout_cli.config_overrides).await;
        }
        Some(Subcommand::Proto(mut proto_cli)) => {
            prepend_config_flags(&mut proto_cli.config_overrides, cli.config_overrides);
            proto::run_main(proto_cli).await?;
        }
        Some(Subcommand::Completion(completion_cli)) => {
            print_completion(completion_cli);
        }
        Some(Subcommand::Debug(debug_args)) => match debug_args.cmd {
            DebugCommand::Seatbelt(mut seatbelt_cli) => {
                prepend_config_flags(&mut seatbelt_cli.config_overrides, cli.config_overrides);
                codex_cli::debug_sandbox::run_command_under_seatbelt(
                    seatbelt_cli,
                    codex_linux_sandbox_exe,
                )
                .await?;
            }
            DebugCommand::Landlock(mut landlock_cli) => {
                prepend_config_flags(&mut landlock_cli.config_overrides, cli.config_overrides);
                codex_cli::debug_sandbox::run_command_under_landlock(
                    landlock_cli,
                    codex_linux_sandbox_exe,
                )
                .await?;
            }
        },
        Some(Subcommand::Apply(mut apply_cli)) => {
            prepend_config_flags(&mut apply_cli.config_overrides, cli.config_overrides);
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

fn print_completion(cmd: CompletionCommand) {
    let mut app = MultitoolCli::command();
    let name = "codex";
    generate(cmd.shell, &mut app, name, &mut std::io::stdout());
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
