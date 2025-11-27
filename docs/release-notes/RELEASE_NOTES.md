## @just-every/code v0.5.7
Packed release tightening shell decoding, automation safety, and telemetry APIs ahead of the tag.

### Changes
- Core/Exec: decode shell output with detected encodings so Unicode logs stay readable across platforms.
- Auto Drive: force read-only agents when no git repo exists to avoid accidental writes during automation.
- App Server: emit token usage, compaction, and turn diff events plus thread metadata to improve monitoring.
- Shell MCP: declare capabilities, add login support, and publish the npm package to keep tool integrations healthy.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.5.6...v0.5.7
