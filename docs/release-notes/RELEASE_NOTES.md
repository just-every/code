## @just-every/code v0.6.7
A focused update that improves remote model support, exec safety, and session resilience.

### Changes
- Core/TUI: add remote model support and harden exec memory handling for safer runs.
- Auto Drive: summarize the last session on completion so users get a quick recap.
- Exec: add a max-seconds budget with countdown nudges and clean up log paths for killed children.
- Reliability: auto-retry turns after usage limits and avoid cloning large histories during retention cleanup.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.6...v0.6.7
