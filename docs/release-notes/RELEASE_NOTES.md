## @just-every/code v0.6.40

Small fixes to keep TUI image workflows and Linux builds stable.

### Changes
- TUI/Image: initialize picker state for image cards so selection works reliably.
- Core: gate cgroup helpers on Linux to avoid non-Linux builds invoking them.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.39...v0.6.40
