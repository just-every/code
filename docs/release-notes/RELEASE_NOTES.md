## @just-every/code v0.6.148

This release improves protocol token usage reporting and fixes a TUI composer animation lifecycle issue.

### Changes

- TUI: stop the composer animation ticker when the app event channel closes.
- Protocol: track prompt cache write token usage across protocol events and SDK types.
- Dependencies: update serde_with to 3.21.0.

### Install

```bash
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.147...v0.6.148
