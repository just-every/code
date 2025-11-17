## @just-every/code v0.4.18

Shipping Nix packaging fixes so code-rs builds stay reproducible out of the box.

### Changes

- Nix: add the ratatui cargoLock output hash so code-rs builds succeed without manual prefetching.
- Nix: point the code-rs derivation at the workspace root and explicit sourceRoot to keep multi-crate packaging aligned.

### Install

```
npm install -g @just-every/code@latest
code
```

### Thanks

Thanks to @KaminariOS for contributions!

Compare: https://github.com/just-every/code/compare/v0.4.17...v0.4.18
