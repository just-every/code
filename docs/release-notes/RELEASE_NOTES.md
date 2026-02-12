## @just-every/code v0.6.66
A focused release that expands websocket support and smooths model/REPL behavior.

### Changes
- App Server: add websocket transport and protocol updates.
- Core/Websocket: bound ingress buffering and unblock spark exec/close readers.
- TUI/Model: surface gpt-5.3-codex-spark in /model.
- Core/JS REPL: add host helpers and exec end events.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.65...v0.6.66
