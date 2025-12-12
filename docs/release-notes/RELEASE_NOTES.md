## @just-every/code v0.6.3

Stability update with focused fixes for build reliability and TUI polish.

### Changes

- Build: prevent concurrent tmp-bin races in build-fast to keep artifacts isolated.
- TUI history: handle background wait call_id to avoid orphaned exec entries.
- Onboarding: align trust directory prompt styling with the rest of the flow.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.2...v0.6.3
