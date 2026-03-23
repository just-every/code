## @just-every/code v0.6.82

This release improves release pipeline resilience when remote BuildBuddy credentials are not available.

### Changes

- CI: fall back to local Bazel execution when the BuildBuddy API key is unavailable, preventing release pipeline failures in restricted environments.
- Release Workflows: apply the BuildBuddy fallback path to both `rusty-v8-release` and `v8-canary` jobs for consistent publish reliability.

### Install

```bash
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.81...v0.6.82
