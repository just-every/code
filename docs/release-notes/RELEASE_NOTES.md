## @just-every/code v0.6.1
This release focuses on hardening Auto Review, upgrading the models stack, and tightening shell/exec capture while polishing navigation.

### Changes
- Auto Review: Harden locks, fallback worktrees, zero-count status, and pending-fix isolation so automated reviews stay reliable.
- Models: Introduce ModelsManager across app server and TUI, add a remote models flag, and cache disk-loaded presets with TTL/ETag for faster selection.
- Shell & Exec: Detect mutating commands, snapshot shell state, and clear lingering execs so automation captures side effects cleanly.
- TUI UX: Add vim-style pager keys, Ctrl+N/P list shortcuts, tighter shell output limits, and aligned auto-review footers for smoother navigation.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.0...v0.6.1
