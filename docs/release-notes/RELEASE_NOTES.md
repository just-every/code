## @just-every/code v0.6.74

This release sharpens Auto Review messaging so results stay clear without extra transcript noise.

### Changes

- TUI/Auto Review: stop duplicating background review notes as `[developer]` history messages to keep transcript noise down.
- TUI/Auto Review: keep review findings routed through the dedicated Auto Review notice while still forwarding hidden context to the coordinator.

### Install

```bash
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.73...v0.6.74
