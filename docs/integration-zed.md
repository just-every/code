# Zed Integration

To point Zed at Code's ACP server, add this block to `settings.json`:

```jsonc
{
  "agent_servers": {
    "Code": {
      "command": "npx",
      "args": ["-y", "@just-every/code", "acp"]
    }
  }
}
```

Adjust the `command` or `args` only if you pin a different version or use a globally installed binary.

## Zed prerequisites

- Zed Stable `0.201.5` (released August 27, 2025) or newer adds ACP support with the Agent Panel. Update via `Zed → Check for Updates` before wiring Code in. Zed’s docs call out ACP as the mechanism powering Gemini CLI and other external agents.
- External agents live inside the Agent Panel (`cmd-?`). Use the `+` button to start a new thread and pick `Code` from the external agent list. Zed runs our CLI as a subprocess over JSON‑RPC, so all prompts and diff previews stay local.
- Zed installs dependencies per entry automatically. If you keep `command = "npx"`, Zed will download the published `@just-every/code` package the first time you trigger the integration.

## How Code implements ACP

- The Rust MCP server exposes ACP tools: `session/new`, `session/prompt`, and fast interrupts via `session/cancel`. These are backed by the same conversation manager that powers the TUI, so approvals, confirm guards, and sandbox policies remain intact.
- Streaming `session/update` notifications bridge Codex events into Zed. You get Answer/Reasoning updates, shell command progress, approvals, and apply_patch diffs in the Zed UI without losing terminal parity.
- MCP configuration stays centralized in `CODEX_HOME/config.toml`. Use `[experimental_client_tools]` to delegate file read/write and permission requests back to Zed when you want its UI to handle approvals.
- The CLI entry point (`npx @just-every/code acp`) is a thin wrapper over the Rust binary (`cargo run -p code-mcp-server -- --stdio`) that ships alongside the rest of Code. Build-from-source workflows plug in by swapping `command` for an absolute path to that binary.

## Tips and troubleshooting

- Need to inspect the handshake? Run Zed’s `dev: open acp logs` command from the Command Palette; the log shows JSON‑RPC requests and Codex replies.
- If prompts hang, make sure no other process is bound to the same MCP port and that your `CODEX_HOME` points to the intended config directory. The ACP server inherits all of Code’s sandbox settings, so restrictive policies (e.g., `approval_policy = "never"`) still apply.
- Zed currently skips history restores and checkpoint UI for third-party agents. Stick to the TUI if you rely on those features; ACP support is still evolving upstream.
