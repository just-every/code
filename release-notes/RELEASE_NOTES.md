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
