## @just-every/code v0.6.10

Stability-focused release to keep TUI command processing responsive under heavy redraws.

### Changes
- TUI: keep bulk command processing responsive during heavy redraw bursts.
- Performance: prevent redraw loops from starving queued work so outputs stay timely.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.9...v0.6.10
