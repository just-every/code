## @just-every/code v0.6.144

This release improves protocol compatibility, stored response handling, and installer reliability.

### Changes

- Core: accept upstream reasoning summary metadata for improved protocol compatibility.
- Core: strip invalid stored response item IDs before reusing stored responses.
- CLI: fix code-mode installation on Darwin systems.
- CLI: parse compact release metadata in the installer.

### Install

```sh
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.143...v0.6.144
