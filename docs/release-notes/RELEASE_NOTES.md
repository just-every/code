## @just-every/code v0.6.36

Small stability release focused on Esc handling and reducing CI flakes.

### Changes
- TUI: prioritize task cancellation on Esc before agent input to make stopping runs reliable.
- Tests: reduce linux sandbox and TUI timeout flakes for steadier CI runs.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.35...v0.6.36
