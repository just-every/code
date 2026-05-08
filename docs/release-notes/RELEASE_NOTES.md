## @just-every/code v0.6.97

This release improves keyboard-driven workflows, hook management, plugin sharing, and sandbox controls across Code.

### Changes

- CLI/TUI: add configurable keymaps, a Vim composer mode, and a dedicated `codex update` command for faster keyboard-driven workflows.
- Hooks: add a `/hooks` browser, persist hook enablement state, and fix migrated hook path rewriting so hook management is easier and more reliable.
- Plugins: track local paths for shared plugins, add remote plugin skill reads, sync cached installed bundles, and surface admin-disabled remote plugin status.
- Sandbox: add explicit sandbox permission profiles and CLI config controls, and ignore dangerous project-level config keys by default.
- TUI: color the status line from the active theme, format multi-day goal durations clearly, and trim extended history persistence to keep large sessions responsive.

### Install

```bash
npm install -g @just-every/code@latest
code
```

### Thanks

Thanks to @owenlin for contributions!

Compare: https://github.com/just-every/code/compare/v0.6.96...v0.6.97
