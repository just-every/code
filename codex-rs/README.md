# Codex CLI (Rust Implementation)

## Fork Lineage

**This Repository**: https://github.com/theturtlecsz/code
**Upstream**: https://github.com/just-every/code
**Origin**: OpenAI Codex (community-maintained fork)

**NOT RELATED TO**: Anthropic's Claude Code (different product entirely)

**Fork-Specific Features**:
- **Spec-Kit Automation**: Multi-agent PRD workflows (Plan→Tasks→Implement→Validate→Audit→Unlock)
- **Consensus Synthesis**: Multi-model result aggregation via local-memory MCP
- **Quality Gates**: Automated requirement validation framework
- **Native MCP Integration**: 5.3x faster consensus checks vs subprocess baseline

---

We provide Codex CLI as a standalone, native executable to ensure a zero-dependency install.

## Installing This Fork

**Build from source**:
```shell
cd codex-rs
cargo build --release -p codex-cli
./target/release/code
```

**Note**: This fork is not published to npm. The original `@openai/codex@native` package is unrelated.

## Upstream Installation

For the upstream `just-every/code` project, see their repository for installation instructions.

## What's New in This Fork

This fork extends the Rust CLI with a **spec-kit automation framework** for multi-agent product requirements workflows. See `CLAUDE.md` and `REVIEW.md` for architecture details.

### Config

Codex supports a rich set of configuration options. Note that the Rust CLI uses `config.toml` instead of `config.json`. See [`docs/config.md`](../docs/config.md) for details.

### Model Context Protocol Support

Codex CLI functions as an MCP client that can connect to MCP servers on startup. See the [`mcp_servers`](../docs/config.md#mcp_servers) section in the configuration documentation for details.

It is still experimental, but you can also launch Codex as an MCP _server_ by running `codex mcp`. Use the [`@modelcontextprotocol/inspector`](https://github.com/modelcontextprotocol/inspector) to try it out:

```shell
npx @modelcontextprotocol/inspector codex mcp
```

### Notifications

You can enable notifications by configuring a script that is run whenever the agent finishes a turn. The [notify documentation](../docs/config.md#notify) includes a detailed example that explains how to get desktop notifications via [terminal-notifier](https://github.com/julienXX/terminal-notifier) on macOS.

### `codex exec` to run Codex programmatically/non-interactively

To run Codex non-interactively, run `codex exec PROMPT` (you can also pass the prompt via `stdin`) and Codex will work on your task until it decides that it is done and exits. Output is printed to the terminal directly. You can set the `RUST_LOG` environment variable to see more about what's going on.

### Use `@` for file search

Typing `@` triggers a fuzzy-filename search over the workspace root. Use up/down to select among the results and Tab or Enter to replace the `@` with the selected path. You can use Esc to cancel the search.

### Esc–Esc to edit a previous message

When the chat composer is empty, press Esc to prime “backtrack” mode. Press Esc again to open a transcript preview highlighting the last user message; press Esc repeatedly to step to older user messages. Press Enter to confirm and Codex will fork the conversation from that point, trim the visible transcript accordingly, and pre‑fill the composer with the selected user message so you can edit and resubmit it.

In the transcript preview, the footer shows an `Esc edit prev` hint while editing is active.

### `--cd`/`-C` flag

Sometimes it is not convenient to `cd` to the directory you want Codex to use as the "working root" before running Codex. Fortunately, `codex` supports a `--cd` option so you can specify whatever folder you want. You can confirm that Codex is honoring `--cd` by double-checking the **workdir** it reports in the TUI at the start of a new session.

### Shell completions

Generate shell completion scripts via:

```shell
code completion bash
code completion zsh
code completion fish
```

### Experimenting with the Codex Sandbox

To test to see what happens when a command is run under the sandbox provided by Codex, we provide the following subcommands in Codex CLI:

```
# macOS
codex debug seatbelt [--full-auto] [COMMAND]...

# Linux
codex debug landlock [--full-auto] [COMMAND]...
```

### Selecting a sandbox policy via `--sandbox`

The Rust CLI exposes a dedicated `--sandbox` (`-s`) flag that lets you pick the sandbox policy **without** having to reach for the generic `-c/--config` option:

```shell
# Run Codex with the default, read-only sandbox
codex --sandbox read-only

# Allow the agent to write within the current workspace while still blocking network access
codex --sandbox workspace-write

# Danger! Disable sandboxing entirely (only do this if you are already running in a container or other isolated env)
codex --sandbox danger-full-access
```

The same setting can be persisted in `~/.code/config.toml` via the top-level `sandbox_mode = "MODE"` key (Code will also read legacy `~/.codex/config.toml`), e.g. `sandbox_mode = "workspace-write"`.

If you want to prevent the agent from updating Git metadata (e.g., local safety), you can opt‑out with a workspace‑write tweak:

```toml
sandbox_mode = "workspace-write"

[sandbox_workspace_write]
allow_git_writes = false   # default is true; set false to protect .git
```

### Debugging Virtual Cursor

Use these console helpers to diagnose motion/cancellation behavior when testing in a real browser:

- Disable clickPulse transforms and force long CSS duration:

  `window.__vc && (window.__vc.clickPulse = () => (console.debug('[VC] clickPulse disabled'), 0), window.__vc.setMotion({ engine: 'css', cssDurationMs: 10000 }))`

- Wrap `moveTo` to log duplicates with sequence and inter-call delta:

  `(() => { const vc = window.__vc; if (!vc || vc.__wrapped) return; const orig = vc.moveTo; let seq=0, last=0; vc.moveTo = function(x,y,o){ const now=Date.now(); console.debug('[VC] moveTo call',{seq:++seq,x,y,o,sincePrevMs:last?now-last:null}); last=now; return orig.call(this,x,y,o); }; vc.__wrapped = true; console.debug('[VC] moveTo wrapper installed'); })();`

- Trigger a test move (adjust coordinates as needed):

  `window.__vc && window.__vc.moveTo(200, 200)`

## Code Organization

This folder is the root of a Cargo workspace. It contains quite a bit of experimental code, but here are the key crates:

- [`core/`](./core) contains the business logic for Codex. Ultimately, we hope this to be a library crate that is generally useful for building other Rust/native applications that use Codex.
- [`exec/`](./exec) "headless" CLI for use in automation.
- [`tui/`](./tui) CLI that launches a fullscreen TUI built with [Ratatui](https://ratatui.rs/).
- [`cli/`](./cli) CLI multitool that provides the aforementioned CLIs via subcommands.
