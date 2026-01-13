## @just-every/code v0.6.47

Stability and UX improvements across websockets, MCP, CLI, TUI scrollback, and auto exec.

### Changes
- Core/Websocket: reuse connections and add append support to cut reconnect churn.
- MCP: hot reload servers with static callbacks for smoother development.
- CLI: add --url with OAuth-friendly defaults and prompt Windows users on unsafe commands.
- TUI: keep scrollback tails visible and show in-flight coalesced tool calls.
- Exec/Auto: honor reasoning effort and dedupe reasoning output during auto runs.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.46...v0.6.47
