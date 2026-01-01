## @just-every/code v0.6.24

Small stability fixes for history virtualization and GitHub release monitoring.

### Changes
- TUI: keep virtualization frozen for tail-only views to avoid redraw churn.
- TUI: defer virtualization sync until the view is ready to prevent flicker.
- Core/GH: allow gh_run_wait to target specific repos for release monitoring.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.23...v0.6.24
