## @just-every/code v0.6.37
Focused polish across TUI visuals and config overrides for the latest release.

### Changes
- TUI/Image: render view image cards so attached visuals show inline.
- TUI/Browser: scope console logs to each browser card to avoid spillover.
- TUI/Resume: prevent footer underflow in resume layouts.
- TUI/Composer: guard composer height to keep the input stable.
- Core/Config: allow tool output size override to honor config limits.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.36...v0.6.37
