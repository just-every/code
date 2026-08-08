## @just-every/code v0.6.170

This release improves upstream protocol parity, configuration persistence, and dependency freshness.

### Changes

- Core: align protocol, routing, and executor environment behavior with upstream parity.
- App Server: expose code-mode host gRPC protocol schemas and generated v2 response metadata.
- Models: include image generation and model specialty metadata in protocol responses.
- Core: persist theme tables safely when writing configuration.
- Dependencies: update js-yaml to 4.3.1.

### Install

```sh
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.169...v0.6.170
