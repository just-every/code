## @just-every/code v0.6.150

This release brings upstream parity updates, release pipeline improvements, installer source support, and audio history fixes.

### Changes

- Core: backport upstream Responses and review prompt parity.
- Release: publish release metadata, stable installer aliases, and Rust artifacts to Cloudflare R2 channels.
- Install: add an optional releases.openai.com installer source.
- Core: preserve audio across history and tool outputs.

### Install

```sh
npm install -g @just-every/code@latest
code
```

Compare: https://github.com/just-every/code/compare/v0.6.149...v0.6.150
