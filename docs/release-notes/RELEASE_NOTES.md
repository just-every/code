## @just-every/code v0.6.99

This release improves packaging, profiles, goals, plugins, and remote-control reliability across the CLI and TUI.

### Changes

- Install: unify npm and standalone releases around packaged archives, and bundle the zsh runtime for macOS x64 builds.
- CLI/Permissions: standardize on `--profile`, add profile-aware `codex sandbox`, and surface managed permission profiles more consistently.
- TUI/Goals: enable goals by default, improve `/goal` flows, and fix startup issues around working directories and Windows terminal setup.
- Plugins: tabulate `plugin list`, add marketplace workflows, and fix plugin bundle installs plus shared icon assets.
- Remote Control: improve daemon UX, reconnect dropped sessions more reliably, and cap reconnect backoff after failures.

### Install

```
npm install -g @just-every/code@latest
code
```

### Thanks

Thanks to @owenliang, @caseychow, @abhinav, @antonp, @michaelbolin, @channingconger, @celia, @wonpark and @tom for contributions!

Compare: https://github.com/just-every/code/compare/v0.6.98...v0.6.99
