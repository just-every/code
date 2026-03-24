## @just-every/code v0.6.85

This release improves TUI/app-server reliability, streamlines plugin UX, and hardens auth and execution behavior.

### Changes

- TUI/App Server: open ChatGPT login in the local browser, cancel active login on Ctrl+C, and always restore terminal state on early exit.
- Plugins: improve `/plugins` UX with clearer labels/wording, better ordering, cleaner disabled rows, and less OAuth URL console noise during install.
- Plugin Listing: surface marketplace loading errors, stop filtering plugin/list results, and refresh mentions after install/uninstall.
- Auth: use access-token expiration consistently for proactive refresh and prevent repeated refresh storms after permanent token failures.
- Core/App Server: add back-pressure and batching to `command/exec` and complete codex exec migration to the app server for more stable execution under load.

### Install

```bash
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.84...v0.6.85
