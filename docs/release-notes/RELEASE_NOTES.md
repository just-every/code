## @just-every/code v0.6.12

This release smooths TUI rendering, speeds execs, and tightens sandbox governance.

### Changes
- TUI: coalesce transcript redraws, keep spinners live, and shorten status directory labels so streams stay smooth.
- Exec: reduce long-session stalls and collapse waiting time in unified exec so commands finish faster.
- CLI: add `/ps` and apply terminal-aware scroll scaling for clearer process visibility.
- Config/Skills: backport requirements updates, add ExternalSandbox policy, and support `/etc/codex/requirements.toml` for tighter governance.

### Install
```
npm install -g @just-every/code@latest
code
```

### Thanks
Thanks to @GalaxyDetective and @jdijk-deventit for contributions!

Compare: https://github.com/just-every/code/compare/v0.6.11...v0.6.12
