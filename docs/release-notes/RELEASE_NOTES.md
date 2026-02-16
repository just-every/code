## @just-every/code v0.6.69
Release 0.6.69 focuses on clearer approvals, smarter model routing, and tighter attachment handling.

### Changes
- TUI/Approvals: show structured network approval prompts with host/protocol context.
- Models: gate gpt-5.3-codex-spark behind pro-only auth capabilities.
- Core/Auto Drive: add fallback routing when spark hits overflow or usage limits.
- TUI: preserve remote image attachments across resume and backtrack flows.

### Install
```bash
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.68...v0.6.69
