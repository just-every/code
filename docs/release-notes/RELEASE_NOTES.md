## @just-every/code v0.6.122

This release tightens core tool-turn handling for more stable sessions.

### Changes

- Core: bound repeated tool cycles so sessions recover instead of looping indefinitely.
- Core: stop replaying cancelled tool turns after interruption.
- Core: reject malformed tool batches to keep streaming state consistent.

### Install

```sh
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.121...v0.6.122
