## @just-every/code v0.6.4

Polish demo and TUI flows with quieter hidden-preface handling, clearer visuals, and smoother shutdowns when auto-review is active.

### Changes
- TUI: default hidden-preface injections to silent and allow silent submissions to reduce demo noise.
- CLI demo: add a --demo developer message injection flag for scripted demos.
- TUI: dim mid-turn assistant output and improve plan/cursor contrast in dark mode for clearer streams.
- Exec: add a grace delay before shutdown when auto-review is enabled to avoid abrupt stops.
- TUI: hide the directory label in demo mode for cleaner status displays.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.3...v0.6.4
