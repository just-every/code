## @just-every/code v0.6.22

This release aligns the engine with upstream, fixes agent wake-ups, and refreshes dependencies.

### Changes
- Agents: wake on batch completion to avoid stalled automation runs.
- Core: refresh codex-rs mirror to upstream main to stay aligned with engine updates.
- Deps: bump tokio, tracing-subscriber, toml_edit, regex-lite in codex-rs for stability.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.21...v0.6.22
