## Advanced

## Non-interactive / CI mode

Run Every Code headless in pipelines. Example GitHub Action step:

```yaml
- name: Update changelog via Code
  run: |
    npm install -g @just-every/code
    export OPENAI_API_KEY="${{ secrets.OPENAI_KEY }}"
    code exec --full-auto "update CHANGELOG for next release"
```

### Resuming non-interactive sessions

You can resume a previous headless run to continue the same conversation context and append to the same rollout file.

Interactive TUI equivalent:

```shell
code resume             # picker
code resume --last      # most recent
code resume <SESSION_ID>
```

Compatibility:

- Latest source builds include `code exec resume` (examples below).
- If `code exec --help` shows no `resume`, update to the latest release; the flag ships in v0.5.0 and newer.

```shell
# Resume the most recent recorded session and run with a new prompt
code exec "ship a release draft changelog" resume --last

# Alternatively, pass the prompt via stdin
# Note: omit the trailing '-' to avoid it being parsed as a SESSION_ID
echo "ship a release draft changelog" | code exec resume --last

# Or resume a specific session by id (UUID)
code exec resume 7f9f9a2e-1b3c-4c7a-9b0e-123456789abc "continue the task"
```

Notes:

- When using `--last`, Code picks the newest recorded session; if none exist, it behaves like starting fresh.
- Resuming appends new events to the existing session file and maintains the same conversation id.

## Tracing / verbose logging

Because Code is written in Rust, it honors the `RUST_LOG` environment variable to configure its logging behavior.

When you run the TUI with `--debug`, log messages are written to `~/.code/debug_logs/codex-tui.log`, so you can leave the following running in a separate terminal to monitor log messages as they are written:

```
tail -F ~/.code/debug_logs/codex-tui.log
```

When you enable the CLI `--debug` flag, request/response JSON is partitioned
into helper-specific folders under `~/.code/debug_logs/`. Expect
subdirectories such as:

- `auto/coordinator`
- `auto/observer/bootstrap`
- `auto/observer/cadence`
- `auto/observer/cross_check`
- `guided_terminal/agent_install_flow`
- `guided_terminal/upgrade_terminal_flow`
- `tui/rate_limit_refresh`
- `ui/theme_spinner`
- `ui/theme_builder`
- `cli/manual_prompt`

Tags become nested path components, so custom helpers appear alongside the
existing timestamped filenames.

Without `--debug`, Code only writes critical crash/error logs to
`~/.code/debug_logs/critical.log.*`; routine log output is suppressed.

By comparison, the non-interactive mode (`code exec`) defaults to `RUST_LOG=error`, but messages are printed inline, so there is no need to monitor a separate file.

See the Rust documentation on [`RUST_LOG`](https://docs.rs/env_logger/latest/env_logger/#enabling-logging) for more information on the configuration options.

## Model Context Protocol (MCP)

The Code CLI can be configured to leverage MCP servers by defining an [`mcp_servers`](./config.md#mcp_servers) section in `~/.code/config.toml` (Code will also read a legacy `~/.codex/config.toml`). It is intended to mirror how tools such as Claude and Cursor define `mcpServers` in their respective JSON config files, though the Code format is slightly different since it uses TOML rather than JSON, e.g.:

```toml
# IMPORTANT: the top-level key is `mcp_servers` rather than `mcpServers`.
[mcp_servers.server-name]
command = "npx"
args = ["-y", "mcp-server"]
env = { "API_KEY" = "value" }
```

## Using Code as an MCP Server
> [!TIP]
> It is somewhat experimental, but the Code CLI can also be run as an MCP _server_ via `code mcp`. If you launch it with an MCP client such as `npx @modelcontextprotocol/inspector code mcp` and send it a `tools/list` request, you will see that there is only one tool, `code`, that accepts a grab-bag of inputs, including a catch-all `config` map for anything you might want to override. Feel free to play around with it and provide feedback via GitHub issues. 
