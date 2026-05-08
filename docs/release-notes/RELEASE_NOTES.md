## @just-every/code v0.6.98

This release improves TUI workflows, thread handling, plugin sharing, and Linux portability across Code.

### Changes

- TUI: add upstream-compatible slash commands, a redesigned session picker, raw scrollback mode, and broader key/input polish.
- Threads: return session IDs from thread and fork flows, paginate thread history, and keep live thread snapshots in sync.
- Plugins: expand plugin sharing with access controls, discoverability settings, marketplace source filters, and richer plugin details.
- Auth/Environments: enable AWS login credentials for Bedrock and route tools through selected environments more consistently.
- Linux sandbox: bundle standalone `bwrap` builds and harden fallback/startup handling to improve reliability on Linux.

### Install

```bash
npm install -g @just-every/code@latest
code
```

### Thanks

Thanks to @owenlin0, @alfozan111, and @vincentkoc for contributions!

Compare: https://github.com/just-every/code/compare/v0.6.97...v0.6.98
