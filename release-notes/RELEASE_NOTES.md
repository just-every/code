## @just-every/code v0.4.14

This release sharpens review ergonomics and smooths TUI flows so everyday runs stay reliable.

### Changes
- Settings: let reviewers choose the model used for /review from the settings overlay.
- TUI: keep the final scrollback line visible after a command completes so transcripts stay readable.
- TUI: simplify the /merge handoff so follow-up flows resume without manual cleanup.
- TUI: keep multiline slash commands intact when dispatching plan or solve sequences.
- Stability: recover gracefully when the working directory vanishes mid-run instead of crashing.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.4.13...v0.4.14
