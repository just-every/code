## @just-every/code v0.4.16

Fresh polish for unified exec, onboarding, and automation integrations.

### Changes
- TUI: enable desktop notifications by default so background job updates surface immediately.
- TUI: refine unified exec with clearer UI and explicit workdir overrides for commands launched from history.
- Onboarding: handle "Don't Trust" directory selections gracefully so setup cannot get stuck in untrusted folders.
- SDK: add CLI environment override and AbortSignal support for better automation integrations.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.4.15...v0.4.16
