## @just-every/code v0.5.14
Automation stability, TUI polish, and new bridge telemetry land in this release.

### Changes
- Core/Bridge: surface code-bridge events directly in sessions so runs show live bridge activity.
- TUI: keep composer popups aligned after history navigation and wrap the agent list inside the command editor for better readability.
- Auto Drive: stabilize the intro placeholder and ensure exec completions render in order so automation transcripts stay coherent.
- Core/Compact: prune orphan tool outputs before compaction to shrink bloated histories and speed up resumes.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.5.13...v0.5.14
