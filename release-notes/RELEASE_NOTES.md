## @just-every/code v0.5.3

This release tightens automation reliability, improves paste handling, and smooths over flaky connectivity.

### Changes
- Auto Drive: add CLI aliases for automation runs and force headless sessions into full-auto so release flows stay hands-free.
- TUI: keep tab characters intact during paste bursts and block stray Enter submits from per-key pastes for reliable composer input.
- Connectivity: harden CLI/TUI retry paths so transient network drops automatically reconnect active sessions.
- Config: honor CODE_HOME and CODEX_HOME entries from .env and retry without reasoning summaries when providers reject them.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.5.2...v0.5.3
