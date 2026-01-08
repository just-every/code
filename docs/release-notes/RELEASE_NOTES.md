## @just-every/code v0.6.44
Small rendering fix to keep TUI backgrounds drawing cleanly.

### Changes
- TUI/Render: reset skip flags when filling backgrounds so reused buffer cells redraw correctly.
- TUI/Render: ensure background fill without characters also clears skip to prevent lingering artifacts.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.43...v0.6.44
