## @just-every/code v0.6.28

This release focuses on fresher model data, smoother limits UI, and clearer assistant code output.

### Changes
- TUI: wrap assistant code blocks to keep long outputs readable.
- TUI: auto-refresh secondary limit windows so status stays current.
- Core/Limits: retry after reset even when the prior attempt failed.
- Core/Models: refresh when models etag changes or mismatches to stay current.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.27...v0.6.28
