## @just-every/code v0.6.143

This release brings upstream auth and web search parity updates, hosted mode defaults, and pnpm install detection improvements.

### Changes

- Core: backport upstream auth and web search parity across config, protocol, and streaming flows.
- CLI: default code-mode to hosted mode for new sessions.
- CLI: detect installs managed by pnpm when checking managed upgrade paths.
- TUI: update onboarding and account labels for refreshed auth modes.

### Install

```sh
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.142...v0.6.143
