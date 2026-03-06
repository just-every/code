## @just-every/code v0.6.75

This release adds faster model defaults, deeper artifact workflows, and stronger reliability across memories and JS REPL flows.

### Changes

- Models: add GPT-5.4 fast mode support and enable fast mode by default for quicker turns.
- Memories: add a settings pane with safer compaction fallback and improved workspace-write support.
- Artifacts: expand artifact workflows with package manager bindings plus spreadsheet and presentation generation.
- JS REPL: support local ESM imports, persist bindings after failed cells, and only allow `data:` image URLs.
- TUI/Core: show session speed in the header and surface diagnostics earlier in the workflow.

### Install

```bash
npm install -g @just-every/code@latest
code
```

### Thanks

Thanks to @owenlin0 and @felipecoury for contributions!

Compare: https://github.com/just-every/code/compare/v0.6.74...v0.6.75
