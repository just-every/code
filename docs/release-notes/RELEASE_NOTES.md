## @just-every/code v0.6.21

Add gh_run_wait to poll GitHub Actions runs and tighten background job cleanup for clearer failures.

### Changes
- Core: add gh_run_wait tool to poll GitHub Actions runs and return completion summaries.
- Core: clear orphaned background jobs when tasks end without results to surface failures.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.20...v0.6.21
