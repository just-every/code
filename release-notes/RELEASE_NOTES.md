## @just-every/code v0.4.11
A focused update that improves model selection, long-run compaction, and client resilience.

### Changes
- Model: add gpt-5-codex-mini presets for quick access to lighter variants.
- Compaction: add per-message summaries, checkpoint warnings, and prompt overrides to keep long transcripts clear.
- Client: normalize retry-after handling, show resume times, and stop retrying fatal quota errors so recoveries are predictable.
- CLI: enable CTRL-n and CTRL-p to navigate slash commands, files, and history without leaving the keyboard.
- SDK: add network_access and web_search toggles to the TypeScript client for richer tool control.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.4.10...v0.4.11
