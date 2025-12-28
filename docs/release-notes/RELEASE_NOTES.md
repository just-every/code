## @just-every/code v0.6.17

This release refines transcript selection, hardens automation runs, and speeds up TUI rendering.

### Changes
- TUI2: improve transcript selection with multi-click, drag start, copy shortcut, and corruption fixes when copying offscreen text.
- Auto Drive: keep agent runs alive and clamp overlays to avoid misaligned prompts.
- Config: honor /etc/codex/config.toml, in-repo config sources, and project_root_markers for workspace detection.
- Exec/CLI: limit unified exec output size and improve ripgrep download diagnostics for clearer failures.
- Performance: cache history render requests and cap redraw scheduling to 60fps to reduce TUI CPU usage.

### Install
```
npm install -g @just-every/code@latest
code
```

### Thanks
Thanks to @RosarioYui for contributions!

Compare: https://github.com/just-every/code/compare/v0.6.16...v0.6.17
