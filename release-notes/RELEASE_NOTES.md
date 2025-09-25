## @just-every/code v0.2.166

This release tightens TUI ergonomics, trims command latency, and refreshes packaging defaults.

### Changes

- TUI/History: refresh the popular commands lineup so quick actions match current workflows.
- TUI/Auto-upgrade: silence installer chatter and log completion once updates finish.
- Core/Client: skip the web_search tool when reasoning is minimal to reduce latency.
- TUI/Input: normalize legacy key press/release cases so hotkeys stay consistent on older terminals.
- Nix: make codex-rs the default package and drop the broken codex-cli derivation.

### Install

```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.2.165...v0.2.166
