## @just-every/code v0.6.52

This release tightens browser navigation waits to keep runs responsive.

### Changes
- Browser: bound DOMContentLoaded waits to avoid hangs during navigation.
- Browser: bound load waits with readyState polling fallback.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.51...v0.6.52
