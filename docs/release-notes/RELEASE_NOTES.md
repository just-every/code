## @just-every/code v0.6.156

This release refreshes upstream parity and improves SDK, MCP, SQLite, and release publishing behavior.

### Changes

- Core: refresh upstream parity for v0.6.156.
- SDK: update generated app-server protocol artifacts and client RPC coverage.
- MCP: use configured HTTP clients for all OAuth requests.
- Core: honor the configured SQLite home in log storage and centralize connection creation.
- Release: advance the latest alpha CLI channel only after publishing completes.

### Install

```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.155...v0.6.156
