## @just-every/code v0.6.49
A release focused on smoother session forking, better headless auth, and more predictable tool behavior.

### Changes
- CLI/Fork: /fork now clones the current session and surfaces the source session id in /status.
- Auth: add device code login for headless environments to simplify setup.
- TUI/Auto-review: persist review baselines across sessions to avoid repeated prompts.
- Core: align tool output caps with model policy to prevent unexpected truncation.
- API: allow listing threads ordered by created_at or updated_at for predictable pagination.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.48...v0.6.49
