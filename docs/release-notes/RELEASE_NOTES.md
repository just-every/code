## @just-every/code v0.6.46

Stability-focused patch release improving TUI streaming order and redraw recovery.

### Changes

- TUI/Stream: preserve commit ticks while debouncing to keep command ordering intact.
- TUI/Render: resync buffers after WouldBlock errors so redraws recover cleanly.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.45...v0.6.46
