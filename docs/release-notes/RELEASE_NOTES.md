## @just-every/code v0.6.11

Fresh model defaults and smoother cloud runs in this release.

### Changes
- Models: default to gpt-5.2-codex and migrate 5.1 presets with the new key.
- TUI: make the GPT-5.2 upgrade link clickable for smoother migrations.
- Core: adopt constraint-based loading to speed startup and reduce redundancy.
- Cloud: default cloud exec to the current branch to avoid mismatched runs.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.10...v0.6.11
