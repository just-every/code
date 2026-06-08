## @just-every/code v0.6.109

This release improves Responses metadata, release diagnostics, and CI runner selection.

### Changes

- Models: include the window id in Responses metadata so requests stay tied to the active window.
- Release: restore symbol artifacts with line tables for more useful release diagnostics.
- CI: template custom runner names by repository for steadier workflow execution.

### Install

```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.108...v0.6.109
