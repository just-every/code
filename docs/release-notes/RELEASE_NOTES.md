## @just-every/code v0.6.70

This release improves search persistence, Auto Drive routing control, and sandbox GPU access.

### Changes
- Core/Search: persist and restore tool selection after search.
- Core/Search: warn when falling back to default metadata and keep selection.
- Auto Drive: add configurable CLI routing entries.
- Linux Sandbox: allow GPU device paths in landlock.

### Install
```bash
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.69...v0.6.70
