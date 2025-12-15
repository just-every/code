## @just-every/code v0.6.6
New release with TUI polish, safer config handling, and sturdier platform support.

### Changes
- TUI: show Every Code title and stabilize header rendering so status bar and snapshots stay consistent.
- Skills: reimplement loading via SkillsManager and add skills/list op for more reliable discovery.
- Config: clean config loading/API, expand safe commands, and refresh disk status using latest values for MCP servers.
- Windows: locate pwsh.exe/powershell.exe reliably and parse PowerShell output with PowerShell for sturdier scripts.
- MCP/TUI: restore startup progress messages and show xhigh reasoning warnings for gpt-5.2 to keep users informed.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.5...v0.6.6
