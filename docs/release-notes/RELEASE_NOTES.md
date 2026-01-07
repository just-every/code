## @just-every/code v0.6.39

Latest release with transcript UX improvements, better auto-drive telemetry, and clearer TUI paths.

### Changes

- TUI/Auto-drive: add navigation telemetry and forward aligned compacted history for new browser runs.
- TUI2/Markdown: stream logical lines so transcripts reflow correctly on resize and copy/paste.
- TUI: render view-image paths relative to the working directory for non-git projects.
- TUI2/Transcript: add an auto-hiding scrollbar, anchor the copy pill at the viewport bottom, and cache rendering to cut redraws.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.38...v0.6.39
