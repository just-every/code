## @just-every/code v0.6.41

Bugfix release tightening TUI execution rendering and exec request handling.

### Changes
- TUI/History: show exec and MCP cards immediately and drop spacer after collapsed reasoning before exec.
- Exec: send prompt and images in one turn to keep runs aligned.
- TUI/Queue: dispatch queued input immediately so interactions start without delay.
- TUI/Render: preserve WouldBlock kind in draw errors for accurate diagnostics.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.40...v0.6.41
