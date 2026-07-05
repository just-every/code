## @just-every/code v0.6.140

This release improves TLS startup reliability across Code binaries.

### Changes

- Core: install the rustls crypto provider across Code binaries for reliable TLS startup.
- CLI: initialize TLS consistently from a shared rustls provider utility.

### Install

```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.139...v0.6.140
