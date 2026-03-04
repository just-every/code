## @just-every/code v0.6.71

This release improves realtime reliability, tightens approvals, and upgrades core TUI and JS REPL workflows.

### Changes

- Core/Realtime: prefer websocket v2, add fallback behavior, and improve timeout handling for more resilient sessions.
- TUI: add `/copy`, improve clear controls (`/clear` and Ctrl-L), and expand multi-agent progress and picker UX.
- Security/Approvals: persist network approval policy and tighten zsh-fork approval and sandbox enforcement paths.
- JS REPL: lower Node minimum requirement, gate incompatible runtimes at startup, and improve error recovery in nested tool calls.

### Install

```bash
npm install -g @just-every/code@latest
code
```

### Thanks

Thanks to @rupurt, @dchimento, @JaviSoto, @owenlin0, and @felipecoury for contributions!

Compare: https://github.com/just-every/code/compare/v0.6.70...v0.6.71
