## @just-every/code v0.4.19

Improves the Nix packaging so local builds stay reproducible without extra manual setup.

### Changes
- Nix: vendor all git-sourced crates so offline builds no longer depend on network access.
- Build: point the Nix derivation at the repo root to keep codex-rs workspace dependencies available.

### Install
```
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.4.18...v0.4.19
