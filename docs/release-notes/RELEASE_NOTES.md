## @just-every/code v0.6.181

This release improves model metadata handling, executor identity reporting, release pinning, and build stability.

### Changes

- Core: accept upstream reasoning metadata in model responses.
- Core: expose stable executor build identity in environment metadata.
- Release: preserve standalone install pins during daemon updates and prevent published release replacement.
- Release: improve Bazel and Linux musl build stability with opt-in stamping and jemalloc flag fixes.

### Install

```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.180...v0.6.181
