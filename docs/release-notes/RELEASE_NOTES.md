## @just-every/code v0.6.83

This release improves release pipeline reliability when BuildBuddy credentials are unavailable.

### Changes

- CI: fall back to local Bazel execution when the BuildBuddy API key is unavailable, keeping release jobs running in restricted environments.
- Release Workflows: apply the BuildBuddy fallback path to both `rusty-v8-release` and `v8-canary` for consistent publish reliability.

### Install

```bash
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.82...v0.6.83
