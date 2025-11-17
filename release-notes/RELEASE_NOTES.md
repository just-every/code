## @just-every/code v0.4.17
Code v0.4.17 focuses on faster resume workflows and more flexible reviews.

### Changes
- TUI: add an uncommitted preset to /review so you can diff local edits without staging.
- Resume: make the session picker async and add /push for fast handoff into publish.
- Resume: ignore system status snippets so regenerated plans stay focused on user messages.
- Resume: count user input coming from rollouts to keep token and action history accurate.
- Resume: unify the session catalog across views so saved sessions appear consistently.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.4.16...v0.4.17
