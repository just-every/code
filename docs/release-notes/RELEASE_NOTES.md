## @just-every/code v0.6.35

This release improves agent robustness when binaries go missing.

### Changes
- Core/Agent: keep packaged code executable available for read-only agents to avoid missing-binary failures.
- Core/Agent: fall back to local dev build when the running binary disappears to keep agent commands working.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.34...v0.6.35
