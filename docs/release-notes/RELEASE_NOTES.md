## @just-every/code v0.6.116

This release refreshes upstream parity while tightening exec-server behavior, storage stability, and Windows coverage.

### Changes

- Core: honor remote environment cwd and shell settings in exec-server.
- Core: pin bundled SQLite to the fixed WAL-reset build for steadier local storage.
- Testing: add PowerShell coverage to the Wine harness for stronger Windows compatibility checks.
- Release: keep fork workflow and packaging choices aligned through the upstream refresh.

### Install

```sh
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.115...v0.6.116
