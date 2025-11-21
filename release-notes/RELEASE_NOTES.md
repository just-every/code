## @just-every/code v0.5.0
This major release renames the project to **Every Code** and rolls up all 0.4.x improvements: sturdier Auto Drive, better defaults, and platform polish.

### Highlights
- Rebrand: Every Code (still invoked as `code`) with refreshed docs and messaging.
- Auto Drive: end-to-end resiliency upgrades (compaction, diagnostics, retries, observer stream, resume safety, clearer cards and status).
- Models: default CLI/TUI/Auto Drive presets moved to gpt-5.1 plus new lightweight codex-mini variants.
- UX: unified settings overlay refinements, /review uncommitted preset, strict streamed-order history, backtrack, and improved slash navigation.
- Notifications & status: desktop notifications on by default, clearer browser/exec logging, and richer session/resume catalogs.
- Platform/hardening: Nix offline builds, Windows AltGr + PATHEXT fixes, BSD keyring gating, responses proxy rename/hardening, and sandbox/process tightening.
- MCP/Integrations: sturdier MCP tooling (streamable HTTP client, timeouts, Zed/ACP guidance) and a responses API proxy for shared hosts.

### Install
```
npm install -g @just-every/code@latest
code
```

### Thanks
Thanks to everyone who contributed across the 0.4.x cycle!

Compare: https://github.com/just-every/code/compare/v0.4.20...v0.5.0

## @just-every/code v0.4.20
This release focuses on model stability across platforms plus tooling fixes for Windows and BSD users.

### Changes
- Core: serialize `shell_command` tool invocations so concurrent steps no longer trample each other during runs.
- Models: ignore empty Claude `finish_reason` fields so streamed answers no longer truncate mid-response.
- Windows: treat AltGr chords as literal text and resolve MCP script-based tools via PATHEXT so international keyboards and script servers work again.
- Core: overhaul compaction/truncation paths to remove double-truncation panics and keep summaries concise on long sessions.
- Platform: gate keyring backends per target and add BSD hardening so FreeBSD/OpenBSD builds succeed out of the box.

### Install
```
npm install -g @just-every/code@latest
code
```

### Thanks
Thanks to @dulikaifazr, @Akrelion45, @JoonsooLee, @Xiao-YongJin, and @AbkariMohammedSayeem for contributions!

Compare: https://github.com/just-every/code/compare/v0.4.19...v0.4.20
