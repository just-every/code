## @just-every/code v0.6.163

This release restores request_user_input blocking parity across the app-server protocol and core streaming paths.

### Changes

- Protocol: restore request_user_input blocking schema parity across app-server events and generated schemas.
- Core: carry blocking request_user_input behavior through streaming tool requests.

### Install

```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.162...v0.6.163
