## @just-every/code v0.6.14

This release polishes TUI history behavior, stabilizes browser retries, and aligns auth with session accounts.

### Changes

- TUI: clear stale mid-turn output when starting a new task so history stays accurate.
- TUI: clear exec spinners when a TaskComplete event is missing to avoid stuck indicators.
- Core/Auth: switch the active account based on session context to honor workspace permissions.
- Browser: restart the navigation handler after repeated errors to restore browsing.
- Auto-review: defer baseline capture to keep automated review diffs stable.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.13...v0.6.14
