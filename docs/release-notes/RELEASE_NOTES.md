## @just-every/code v0.6.104

This release improves model reasoning controls, plugin visibility, TUI polish, and runtime reliability.

### Changes

- Models: accept custom and model-advertised reasoning efforts with ordered TUI shortcuts.
- Plugins/App Server: expose remote MCP servers, marketplace source JSON, and `-c` config overrides.
- TUI: support F13-F24 keymaps, clarify shortcut overlays, and restore output-free cancelled prompts.
- Core: preserve logical AGENTS.md paths and environment-backed instruction loading.
- Runtime: improve standalone image path hints, rollout compression, and release build performance.

### Install

```
npm install -g @just-every/code@latest
code
```

### Thanks

Thanks to @enieuwy and @owenlin0 for contributions!

Compare: https://github.com/just-every/code/compare/v0.6.103...v0.6.104
