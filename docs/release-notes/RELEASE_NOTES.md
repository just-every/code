## @just-every/code v0.6.115

This release improves compact session parity, Windows release packaging, and compatibility test coverage.

### Changes

- Core: backport compact turn-state parity for more consistent session behavior.
- Release: package Windows ARM64 artifacts on x64 release runners.
- Release: stage npm packages and Windows archives, symbols, and compression in parallel.
- Testing: add hermetic Wine support and exec-server coverage for Windows compatibility.

### Install

```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.114...v0.6.115
