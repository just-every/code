## @just-every/code v0.6.72

This release improves agent config migration safety and keeps TUI auto-review feedback loops responsive.

### Changes

- Agents/App Server: add external agent config migration API with import depth guards to safely bring configs forward.
- TUI/Auto Review: dispatch idle review findings back to the model so automated review cycles continue reliably.

### Install

```bash
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.71...v0.6.72
