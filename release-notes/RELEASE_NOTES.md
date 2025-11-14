## @just-every/code v0.4.15

Codex v0.4.15 refreshes model defaults and tightens the login flow for a smoother release-week upgrade.

### Changes
- Core: migrate default CLI, TUI, and Auto Drive models to gpt-5.1 so new sessions use the upgraded stack.
- Prompts: align the gpt-5.1 system instructions with Codex guidance to keep responses consistent.
- TUI Login: add device-code fallback and ensure ChatGPT auth links wrap cleanly on narrow terminals.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.4.14...v0.4.15
