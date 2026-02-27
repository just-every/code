## @just-every/code v0.6.73

This release improves Auto Review output handling in the TUI for clearer, less noisy summaries.

### Changes

- TUI/Auto Review: parse embedded JSON review results from mixed runner output so summaries stay focused on findings.
- TUI/Auto Review: truncate plain-text fallback summaries to prevent raw log dumps in chat history.

### Install

```bash
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.72...v0.6.73
