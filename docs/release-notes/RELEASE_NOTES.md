## @just-every/code v0.6.194

This release adds app-level tool exposure controls, preserves upstream model parity, and improves release publishing stability.

### Changes

- Core: expose per-app tool exposure configuration in app-server config schemas.
- Core: align flex-tier model metadata with upstream parity.
- Core: keep credential debug output redacted across auth token data.
- Release: reduce R2 upload concurrency and rely on standard retries for publishing stability.

### Install

```sh
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.193...v0.6.194
