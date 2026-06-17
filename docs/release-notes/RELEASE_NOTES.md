## @just-every/code v0.6.119

This release adds exec-server relay transport support, exposes app-server rate-limit reset details, and refreshes testing and release metadata.

### Changes

- Core: add Noise relay transport support for exec-server remote connections.
- App Server: expose rate-limit reset credits to clients.
- Testing: expand Bazel code-mode coverage and Wine-backed Windows executor support.
- Docs: record Rust path migration invariants and clarify app path-type guidance.
- Release: refresh upstream history and the codex-rs mirror for v0.6.119.

### Install

```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.118...v0.6.119
