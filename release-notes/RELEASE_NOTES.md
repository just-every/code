## @just-every/code v0.5.4
This release keeps automation reliable by honoring reasoning budgets and letting long chats recover automatically.

### Changes
- Core/Agent: pass reasoning effort overrides through config so automation consistently honors requested budgets.
- Compact: trim chat history when context overflows and automatically retry to keep long sessions running.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.5.3...v0.5.4
