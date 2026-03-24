## @just-every/code v0.6.84

This release improves model access, multi-agent workflows, and day-to-day TUI reliability.

### Changes

- Models: add `gpt-5.4-mini` support and simplify auth refresh handling for smoother model access.
- TUI: refresh curated model choices and suppress clean auto-review notices to reduce chat noise.
- Plugins: add install/uninstall flows in the TUI and better plugin labeling/filtering in listings.
- Multi-agent: ship structured agent communication/output and custom watcher support for v2 runs.
- Core: improve command/runtime stability with safer PATH construction, vendored bubblewrap fallback, and unified realtime stop handling.

### Install

```bash
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.83...v0.6.84
