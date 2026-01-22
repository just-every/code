## @just-every/code v0.6.50

This release upgrades model defaults, improves request-user-input UX, and tightens config and sandbox handling.

### Changes
- Core/Models: default to gpt-5.2-codex with personality templating and request-user-input support.
- Core/Config: add layered config.toml support for app-server reads and merges.
- TUI: add request-user-input overlay with interactive picker and reliable pending/answer routing.
- Core/Runtime: preserve interrupted turns to prevent repeats and avoid touching thread mtime on resume.
- Sandbox/Paths: harden tilde expansion and Windows sandbox audit paths for safer writable roots.

### Install
```
npm install -g @just-every/code@latest
code
```

### Thanks
Thanks to @zerone0x and @sgraika127 for contributions!

Compare: https://github.com/just-every/code/compare/v0.6.49...v0.6.50
