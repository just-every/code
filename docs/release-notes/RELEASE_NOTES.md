## @just-every/code v0.6.45

Smooths TUI redraw stability for terminals experiencing backpressure.

### Changes
- TUI/Render: clear after WouldBlock redraws to resync the terminal and remove stale tail lines.
- TUI/Render: improve redraw stability under terminal backpressure so frames recover cleanly.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.44...v0.6.45
