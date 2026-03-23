## @just-every/code v0.6.80

This release improves day-to-day reliability and workflows across the TUI, plugins, approvals, and realtime sessions.

### Changes
- TUI/App Server: complete the app-server-backed TUI migration with restored composer history and remote resume/fork history.
- Plugins: add the first `/plugins` TUI menu and expand featured/product-scoped plugin install and sync flows.
- Approvals/Sandbox: introduce `request_permissions`, persist its decisions across turns, and improve Linux sandbox defaults and split-filesystem handling.
- Multi-agent: switch agent identifiers to path-like IDs and add graph-style network visibility for agent runs.
- Core/Realtime: reduce startup hangs and stabilize realtime/websocket session shutdown and error delivery.

### Install
```bash
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.79...v0.6.80
