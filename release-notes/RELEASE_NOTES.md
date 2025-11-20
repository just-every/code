## @just-every/code v0.4.21
A focused update that tightens model selection, exec policy rollout, and transcript stability.

### Changes
- Auto Drive: let runs choose a model and clamp verbosity so diagnostics stay predictable.
- Models: add gpt-5.1-codex-max default with one-time migration prompt so upgrades stay smooth.
- Core: wire execpolicy2 through core/exec-server and add shell fallbacks so commands keep running under the new policy.
- TUI: add branch-aware filtering to `codex resume` so large workspaces find the right session faster.
- Platform: enable remote compaction by default and schedule auto jobs to keep transcripts lean.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.4.20...v0.4.21
