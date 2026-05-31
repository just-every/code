## @just-every/code v0.6.100

This release improves session reliability, TUI usability, and remote execution security across Code.

### Changes

- TUI: render markdown tables and web links more cleanly, add vim text objects, and make turn interruption keybinds configurable.
- Auth: refresh ChatGPT tokens before expiry and improve Google preset login handling so long-running sessions stay signed in.
- Remote/App Server: migrate remote control to server tokens, resume threads with their turns page, and add `codex app-server --stdio`.
- Sandbox/Security: add named permission profiles in the TUI, block repository-configured code execution in `/diff`, and tighten Unix socket and websocket checks.
- CLI: add standalone websearch, allow API-key auth for remote exec-server registration, and make standalone installs and updates work noninteractively.

### Install

```bash
npm install -g @just-every/code@latest
code
```

### Thanks

Thanks to @stevendcoffey and @owenlin0 for contributions!

Compare: https://github.com/just-every/code/compare/v0.6.99...v0.6.100
