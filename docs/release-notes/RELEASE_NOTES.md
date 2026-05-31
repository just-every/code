## @just-every/code v0.6.101

This release improves model routing, session workflows, plugin discovery, and execution reliability.

### Changes

- Models: update default agent routing, accept upstream tool-mode metadata, and fix Bedrock GPT service tiers.
- CLI/Threads: add archive commands and keep resumed prompt history scoped to the session.
- Plugins/Skills: improve connector and install suggestions, and add runtime extra skill roots.
- Sandbox/Exec: preserve deny-read protections, tighten Windows requirements, and clean up filesystem helpers.
- TUI/Tools: fix Vim editing, render multiline hook output, show web search activity, and finalize image generation natively.

### Install

```bash
npm install -g @just-every/code@latest
code
```

### Thanks

Thanks to @owenlin0 for contributions!

Compare: https://github.com/just-every/code/compare/v0.6.100...v0.6.101
