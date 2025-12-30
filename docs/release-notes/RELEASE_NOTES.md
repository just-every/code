## @just-every/code v0.6.20

Auto Drive reliability improvements for smoother hands-free runs.

### Changes
- Auto Drive: keep retrying after errors so runs recover instead of stopping early.
- Auto Drive: schedule restarts without depending on Tokio to avoid stalled recoveries.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.19...v0.6.20
