## @just-every/code v0.6.2

Latest release with model upgrades, smoother TUI history, and more resilient exec handling.

### Changes
- Models: add a guided gpt-5.2 upgrade flow so users can move to the latest model smoothly.
- TUI history: keep mid-turn answers ordered, hide stray gutters, and collapse duplicate reasoning streams for cleaner transcripts.
- Exec: guard process spawns, pair early exec ends with begins, and keep live output flowing while capping previews to avoid hangs.
- TUI: allow user input to interrupt wait-only execs and force redraws after backpressure stalls for more responsive UI.
- Snapshots: warn when snapshots run long and add a shell command snapshot path.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.1...v0.6.2
