## @just-every/code v0.6.18

This release adds a skills slash command plus stability fixes for exec and Auto Drive flows.

### Changes
- TUI: add `/skills` slash command to list available skills inline.
- Exec: handle missing wait output to keep execution results consistent.
- Auto Drive: stop runs after fatal errors to avoid hanging sessions.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.17...v0.6.18
