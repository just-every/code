## @just-every/code v0.6.133

This release improves upstream compatibility, tool call handling, configuration compatibility, and dependency security.

### Changes

- Core: backport upstream model metadata parity for improved model selection.
- Core: accept legacy default service tier values in configuration.
- Tools: preserve custom tool namespaces across streamed responses.
- Dependencies: update OpenTelemetry SDK to resolve alert 120.

### Install

```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.132...v0.6.133
