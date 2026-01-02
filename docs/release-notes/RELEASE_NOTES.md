## @just-every/code v0.6.29

Markdown rendering now handles wide code blocks more gracefully in the TUI.

### Changes
- TUI/Markdown: wrap wide code graphemes to avoid overflow in rendered blocks.
- TUI/Markdown: flush wrapped code rows so virtualized views stay aligned.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.28...v0.6.29
