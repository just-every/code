## @just-every/code v0.4.9

This release keeps automation stable and ensures CLI installs stay aligned with the latest build.

### Changes
- CLI: rerun the bootstrap step when postinstall scripts are skipped so upgrades stay healthy.
- Auto Drive: salvage user-turn JSON to keep transcripts recoverable after crashes.
- Homebrew: track the latest release so tap installs follow new versions immediately.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.4.8...v0.4.9
