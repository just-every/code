## @just-every/code v0.6.43

TUI image handling is sturdier and respects terminal protocol limits to avoid rendering glitches.

### Changes
- TUI/Images: guard dropped images and clipped views so broken files fall back to placeholders.
- TUI/Images: avoid partial rendering on graphic protocols to prevent cursor corruption while scrolling.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.42...v0.6.43
