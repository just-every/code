## @just-every/code v0.6.129

This release improves OpenAI request privacy defaults and release publishing reliability.

### Changes

- Core: disable storage for Chat Completions and default Responses requests so ZDR accounts do not receive `Store must be set to false` errors.
- Core: strip server item IDs from non-stored Responses requests to keep multi-turn local runs compatible with `store: false`.
- Release: allow more time for release notes generation during publish.

### Install

```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.128...v0.6.129
