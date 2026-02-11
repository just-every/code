## @just-every/code v0.6.63
This release improves websocket stability, rate-limit handling, and TUI behavior.

### Changes
- Core: support multiple rate limits.
- Core/Protocol: add websocket preference and rate-limit metadata; serialize rate-limit ids as nullable.
- Core/Websocket: avoid resending output items and tighten incrementality checks.
- TUI: queue rollback trims in app-event order and keep history recall cursor at line end.
- Exec Policy: reject empty command lists and honor never-prompt approval policy.

### Install
```bash
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.62...v0.6.63
