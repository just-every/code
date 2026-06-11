## @just-every/code v0.6.112

This release improves apply_patch dispatch reliability, especially on Windows.

### Changes

- Core: dispatch function apply_patch calls through the dedicated tool path.
- Windows: route apply_patch through the dedicated tool path by default.

### Install

```sh
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.111...v0.6.112
