## @just-every/code v0.6.30

Bugfix release improving TUI rendering stability and log noise handling.

### Changes
- TUI/Auto Drive: avoid full render rebuilds to cut redraw overhead during runs.
- TUI/History: cache patch summary layout to reduce churn and flicker.
- TUI/Logs: throttle thread spawn errors to prevent repeated warnings.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.29...v0.6.30
